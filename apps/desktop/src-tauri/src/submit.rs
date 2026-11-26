//! The "Submit" page: checks one of the user's mods like the gallery's
//! robot, puts it in a `.zip` in their Downloads folder and gives the link
//! to GitHub's request form. Nothing is sent over the internet from here:
//! the window opens the form in the user's browser.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use spacetify_core::gallery::review::{self, Check, Level};
use spacetify_core::gallery::submission;
use spacetify_core::gallery::{Catalog, load_saved_catalog};
use spacetify_core::mods::{self, Loaded, ModKind};
use spacetify_core::service::Text;
use spacetify_core::{Error, Lang};
use tauri::State;

use crate::model::{self, Palette};
use crate::state::{Failure, Shared};

type Answer<T> = Result<T, Failure>;

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Answer<T> + Send + 'static,
) -> Answer<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|err| Failure::new(None, err.to_string()))?
}

/// The page the window opens on, when the app is started for one (the
/// gallery in Spotify starts it with `--page submit`).
#[derive(Default)]
pub struct StartPage(pub Mutex<Option<String>>);

impl StartPage {
    /// `--page <name>` in `args`, for the pages that may be asked for.
    pub fn from_args(args: &[String]) -> Option<String> {
        let at = args.iter().position(|a| a == "--page")?;
        let page = args.get(at + 1)?;
        (page == "submit").then(|| page.clone())
    }
}

#[tauri::command]
pub fn take_start_page(start: State<'_, StartPage>) -> Option<String> {
    start
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Where a mod stands in the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum InGallery {
    /// Not in the gallery: a new mod.
    No,
    /// In it at the same version: nothing to send.
    Same,
    /// In it at an older version: an update.
    Older,
    /// In it at a newer version than this one.
    Newer,
}

fn in_gallery(catalog: Option<&Catalog>, loaded: &Loaded) -> (InGallery, Option<String>) {
    let meta = loaded.meta();
    let Some(entry) = catalog.and_then(|c| c.get(&meta.id)) else {
        return (InGallery::No, None);
    };
    let there = semver::Version::parse(&entry.version).ok();
    let status = match there {
        Some(v) if v == meta.version => InGallery::Same,
        Some(v) if v < meta.version => InGallery::Older,
        Some(_) => InGallery::Newer,
        None => InGallery::Older,
    };
    (status, Some(entry.version.clone()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub folder: String,
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub version: String,
    pub palette: Option<Palette>,
    in_gallery: InGallery,
}

fn kind_name(kind: ModKind) -> &'static str {
    match kind {
        ModKind::Theme => "theme",
        ModKind::Extension => "extension",
    }
}

/// The installed mods that are valid, with where they stand in the
/// gallery (as of the last time it was opened).
#[tauri::command]
pub async fn submit_candidates(state: State<'_, Shared>) -> Answer<Vec<Candidate>> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let config = state.config().map_err(|err| Failure::of(&err, lang))?;
        let store = state.mods();
        let catalog = load_saved_catalog(&state.dirs);
        let listed = store.list(&config).map_err(|err| Failure::of(&err, lang))?;
        let mut out = Vec::new();
        for item in listed {
            if item.meta.is_err() {
                continue;
            }
            let folder = store.path_of(item.kind, &item.id);
            let Ok(loaded) = mods::load_dir(&folder) else {
                continue;
            };
            out.push(candidate(lang, &folder, &loaded, catalog.as_ref()));
        }
        Ok(out)
    })
    .await
}

fn candidate(lang: Lang, folder: &Path, loaded: &Loaded, catalog: Option<&Catalog>) -> Candidate {
    let meta = loaded.meta();
    Candidate {
        folder: folder.display().to_string(),
        id: meta.id.clone(),
        name: meta.name.clone(),
        kind: kind_name(loaded.kind()),
        version: meta.version.to_string(),
        palette: match loaded {
            Loaded::Theme(theme) => Some(model::theme_palette(lang, theme)),
            Loaded::Extension(_) => None,
        },
        in_gallery: in_gallery(catalog, loaded).0,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckView {
    title: String,
    /// `ok`, `look` or `block`.
    level: Level,
    /// What the check means when it found nothing.
    clean: Option<String>,
    findings: Vec<FindingView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    what: String,
    place: Option<String>,
    excerpt: Option<String>,
    level: Level,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewView {
    candidate: Option<Candidate>,
    in_gallery: InGallery,
    gallery_version: Option<String>,
    checks: Vec<CheckView>,
    summary: String,
    blocking: usize,
    to_look_at: usize,
    size: u64,
}

/// Runs the gallery robot's checks (all but the antivirus) on `folder`.
#[tauri::command]
pub async fn submit_review(state: State<'_, Shared>, folder: String) -> Answer<ReviewView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let folder = PathBuf::from(folder);
        let report = review::review(&folder, None);
        let catalog = load_saved_catalog(&state.dirs);
        let loaded = mods::load_dir(&folder).ok();
        let (status, gallery_version) = loaded
            .as_ref()
            .map(|l| in_gallery(catalog.as_ref(), l))
            .unwrap_or((InGallery::No, None));
        let mut checks = Vec::new();
        for check in Check::ALL {
            let Some(level) = report.level_of(check) else {
                continue;
            };
            let findings: Vec<FindingView> = report
                .findings
                .iter()
                .filter(|f| f.check == check)
                .map(|f| FindingView {
                    what: f.what.get(lang).to_owned(),
                    place: f.file.as_ref().map(|file| match f.line {
                        Some(line) => match lang {
                            Lang::En => format!("{file}, line {line}"),
                            Lang::Fr => format!("{file}, ligne {line}"),
                        },
                        None => file.clone(),
                    }),
                    excerpt: f.excerpt.clone(),
                    level: f.level,
                })
                .collect();
            checks.push(CheckView {
                title: check.title().get(lang).to_owned(),
                level,
                clean: findings.is_empty().then(|| clean_text(check, lang)),
                findings,
            });
        }
        Ok(ReviewView {
            candidate: loaded
                .as_ref()
                .map(|l| candidate(lang, &folder, l, catalog.as_ref())),
            in_gallery: status,
            gallery_version,
            summary: report.summary().get(lang).to_owned(),
            blocking: report.blocking,
            to_look_at: report.to_look_at,
            size: report.files.iter().map(|f| f.size).sum(),
            checks,
        })
    })
    .await
}

/// A short line for a check that found nothing, as on the page.
fn clean_text(check: Check, lang: Lang) -> String {
    let text = match check {
        Check::Valid => Text::new(
            "Manifest and files are valid",
            "Manifeste et fichiers corrects",
        ),
        Check::Files => Text::new("Only allowed files", "Seulement des fichiers permis"),
        Check::Hidden => Text::new("No invisible character", "Aucun caractère invisible"),
        Check::Obfuscation => Text::new("Readable code", "Code lisible"),
        Check::Network => Text::new("No web address", "Aucune adresse internet"),
        Check::Sensitive => Text::new("No sensitive access", "Aucun accès sensible"),
        Check::Images => Text::new("Clean pictures", "Images saines"),
        Check::Antivirus | Check::Changes => Text::new("", ""),
    };
    text.get(lang).to_owned()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    zip: String,
    form_url: String,
}

/// Checks the mod in `folder` again, puts it in a `.zip` in the Downloads
/// folder and returns the link to the request form.
#[tauri::command]
pub async fn submit_prepare(state: State<'_, Shared>, folder: String) -> Answer<Prepared> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let fail = |err: Error| Failure::of(&err, lang);
        let folder = PathBuf::from(folder);
        let report = review::review(&folder, None);
        if report.blocking > 0 {
            return Err(fail(Error::Submission(Text::new(
                "Fix what is blocking first: the gallery would refuse the mod.",
                "Corrige d'abord ce qui bloque : la galerie refuserait le mod.",
            ))));
        }
        let loaded = mods::load_dir(&folder).map_err(|issue| {
            fail(Error::InvalidMod {
                what: folder.display().to_string(),
                issue,
            })
        })?;
        let catalog = load_saved_catalog(&state.dirs);
        let (status, version) = in_gallery(catalog.as_ref(), &loaded);
        match status {
            InGallery::Same | InGallery::Newer => {
                let version = version.unwrap_or_default();
                return Err(fail(Error::Submission(Text::new(
                    format!("The gallery already has version {version}: raise the version in the manifest to send an update."),
                    format!("La galerie a déjà la version {version} : augmente la version dans le manifeste pour envoyer une mise à jour."),
                ))));
            }
            InGallery::No | InGallery::Older => {}
        }
        let into = submission::downloads_dir()
            .unwrap_or_else(|| state.dirs.data_dir.join("submissions"));
        let zip = submission::pack(&folder, &into).map_err(fail)?;
        Ok(Prepared {
            zip: zip.display().to_string(),
            form_url: submission::form_url(&loaded, status == InGallery::Older),
        })
    })
    .await
}
