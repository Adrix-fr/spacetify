//! Leaving Spicetify in one go: what it left on this computer, and moving
//! to spacetify.
//!
//! [`find`] reads Spicetify's own config, [`plan`] says what leaving would
//! do, and [`leave`] does it: put Spotify's original interface back (from
//! Spicetify's backup, only if it is the right version), take away the
//! folders Spicetify unpacked, turn each colour scheme of its themes into a
//! spacetify theme, and install the gallery's equivalents of its extensions.
//! Spicetify itself is left installed: nothing of it is deleted but the
//! folders it put inside Spotify.
//!
//! Themes installed from Spicetify's Marketplace are not in files: the
//! Marketplace keeps them in Spotify's page. The gallery page picks the
//! active one up the next time Spotify starts, if [`Marker`] says so (see
//! [`marketplace_theme`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::write_atomic;
use crate::config::{AppDirs, Config};
use crate::detect::{self, UiState};
use crate::gallery::{self, Source};
use crate::mods::manifest::{self, MAX_ID_LEN};
use crate::service::Text;
use crate::spotify::{self, SpotifyInstall};
use crate::{Error, Result};

const CONFIG_FILE: &str = "config-xpui.ini";
const COLOR_FILE: &str = "color.ini";
/// Not a theme: the name Spicetify's config uses when the Marketplace
/// chose the theme.
const MARKETPLACE: &str = "marketplace";
/// Ids of imported themes start with it, so they never clash with the
/// gallery's.
const ID_PREFIX: &str = "spicetify-";

/// Spicetify's extensions that have an equivalent in the gallery, by file
/// name (lowercase, without `.js`).
const EQUIVALENTS: &[(&str, &str)] = &[
    ("fullappdisplay", "fullscreen"),
    ("trashbin", "trash-bin"),
    ("bookmark", "bookmarks"),
];

/// Spicetify, as installed for this user.
#[derive(Debug, Clone)]
pub struct Spicetify {
    /// Where `config-xpui.ini`, the user's themes and the backup live.
    pub user_dir: PathBuf,
    /// The program, with the themes and extensions it comes with.
    pub app_dir: Option<PathBuf>,
    pub config: SpiceConfig,
}

/// What matters in `config-xpui.ini`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpiceConfig {
    pub current_theme: String,
    pub color_scheme: String,
    pub extensions: Vec<String>,
    pub custom_apps: Vec<String>,
    /// Version of Spotify its backup was made from.
    pub backup_version: Option<String>,
}

/// Finds Spicetify's config for this user, if there is one.
pub fn find() -> Option<Spicetify> {
    let home = dirs::home_dir();
    let mut users = Vec::new();
    users.extend(dirs::config_dir().map(|dir| dir.join("spicetify")));
    users.extend(
        home.as_ref()
            .map(|dir| dir.join(".config").join("spicetify")),
    );
    users.extend(home.as_ref().map(|dir| dir.join(".spicetify")));
    let mut apps = Vec::new();
    apps.extend(dirs::data_local_dir().map(|dir| dir.join("spicetify")));
    apps.extend(home.as_ref().map(|dir| dir.join(".spicetify")));
    find_in(&users, &apps)
}

fn find_in(users: &[PathBuf], apps: &[PathBuf]) -> Option<Spicetify> {
    let user_dir = users.iter().find(|dir| dir.join(CONFIG_FILE).is_file())?;
    let text = fs::read_to_string(user_dir.join(CONFIG_FILE)).ok()?;
    let app_dir = apps
        .iter()
        .find(|dir| *dir != user_dir && dir.join("Themes").is_dir())
        .cloned();
    Some(Spicetify {
        user_dir: user_dir.clone(),
        app_dir,
        config: SpiceConfig::parse(&text),
    })
}

type Section = (String, BTreeMap<String, String>);

/// Sections of an `.ini` file, in the file's order, each with its keys.
/// Comments (`;`, `#`) and lines without `=` are skipped. Keys before the
/// first section go to a section with an empty name.
fn parse_ini(text: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = vec![(String::new(), BTreeMap::new())];
    let mut current = 0;
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim();
            current = match sections.iter().position(|(other, _)| other == name) {
                Some(index) => index,
                None => {
                    sections.push((name.to_owned(), BTreeMap::new()));
                    sections.len() - 1
                }
            };
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.split(" ;").next().unwrap_or_default();
            sections[current]
                .1
                .insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    sections
}

impl SpiceConfig {
    pub fn parse(text: &str) -> Self {
        let ini = parse_ini(text);
        let get = |section: &str, key: &str| {
            ini.iter()
                .find(|(name, _)| name == section)
                .and_then(|(_, keys)| keys.get(key))
                .cloned()
                .unwrap_or_default()
        };
        let list = |section: &str, key: &str| {
            get(section, key)
                .split('|')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let backup = get("Backup", "version");
        Self {
            current_theme: get("Setting", "current_theme"),
            color_scheme: get("Setting", "color_scheme"),
            extensions: list("AdditionalOptions", "extensions"),
            custom_apps: list("AdditionalOptions", "custom_apps"),
            backup_version: (!backup.is_empty()).then_some(backup),
        }
    }
}

// ---------------------------------------------------------------- colours

/// Where each of Spicetify's colours goes in Spotify's colour sets. Later
/// lines win, so `main-elevated` wins over `card`.
const COLOR_MAP: &[(&str, &[(&str, &str)])] = &[
    ("text", &[("base", "text-base"), ("base", "essential-base")]),
    (
        "subtext",
        &[("base", "text-subdued"), ("base", "essential-subdued")],
    ),
    ("main", &[("base", "background-base")]),
    ("card", &[("base", "background-elevated-base")]),
    ("main-elevated", &[("base", "background-elevated-base")]),
    (
        "highlight",
        &[
            ("base", "background-highlight"),
            ("base", "background-press"),
        ],
    ),
    (
        "highlight-elevated",
        &[("base", "background-elevated-highlight")],
    ),
    ("sidebar", &[("app-frame", "background-base")]),
    (
        "button",
        &[
            ("base", "text-bright-accent"),
            ("base", "essential-bright-accent"),
            ("bright-accent", "background-base"),
        ],
    ),
    (
        "button-active",
        &[
            ("bright-accent", "background-highlight"),
            ("bright-accent", "background-press"),
        ],
    ),
    (
        "notification-error",
        &[("base", "text-negative"), ("base", "essential-negative")],
    ),
];

/// `FFFFFF`, `#fff`… as `#ffffff`. Anything else (such as Spicetify's
/// `${xrdb:…}` references) is skipped.
fn hex_color(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_owned(),
        _ => return None,
    };
    Some(format!("#{}", full.to_ascii_lowercase()))
}

/// Spotify's colour sets from one of Spicetify's colour schemes, or `None`
/// if not a single colour could be used.
pub fn colour_sets(
    scheme: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, BTreeMap<String, String>>> {
    let mut sets: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (key, targets) in COLOR_MAP {
        let Some(color) = scheme.get(*key).and_then(|value| hex_color(value)) else {
            continue;
        };
        for (set, token) in *targets {
            sets.entry((*set).to_owned())
                .or_default()
                .insert((*token).to_owned(), color.clone());
        }
    }
    (!sets.is_empty()).then_some(sets)
}

fn colors_toml(sets: &BTreeMap<String, BTreeMap<String, String>>, origin: &str) -> String {
    let mut text = format!(
        "# Colours imported from Spicetify ({origin}).\n\
         # Spicetify's user.css is not carried over: it relies on changes\n\
         # Spicetify makes to Spotify's code. See docs/themes.md.\n"
    );
    for (set, tokens) in sets {
        text.push_str(&format!("\n[{set}]\n"));
        for (token, color) in tokens {
            text.push_str(&format!("{token} = \"{color}\"\n"));
        }
    }
    text
}

/// "Sleek" + "Coral" → "spicetify-sleek-coral", within the id rules.
fn imported_id(theme: &str, scheme: Option<&str>) -> String {
    let mut raw = theme.to_owned();
    if let Some(scheme) = scheme {
        raw.push('-');
        raw.push_str(scheme);
    }
    let mut slug = String::new();
    for c in raw.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut id = format!("{ID_PREFIX}{}", slug.trim_end_matches('-'));
    if id.len() > MAX_ID_LEN {
        id.truncate(MAX_ID_LEN);
    }
    let id = id.trim_end_matches('-').to_owned();
    if id.len() == ID_PREFIX.len() - 1 || !manifest::is_valid_id(&id) {
        format!("{ID_PREFIX}theme")
    } else {
        id
    }
}

/// A colour scheme of a Spicetify theme, ready to become a spacetify theme.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeImport {
    pub id: String,
    pub name: String,
    /// The one Spicetify showed, which becomes the active theme.
    pub current: bool,
    #[serde(skip)]
    sets: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(skip)]
    origin: String,
}

impl ThemeImport {
    pub fn new(
        theme: &str,
        scheme: Option<&str>,
        colors: &BTreeMap<String, String>,
        current: bool,
    ) -> Option<Self> {
        let sets = colour_sets(colors)?;
        let name = match scheme {
            Some(scheme) => format!("{theme} · {scheme}"),
            None => theme.to_owned(),
        };
        let origin = match scheme {
            Some(scheme) => format!("theme {theme}, scheme {scheme}"),
            None => format!("theme {theme}"),
        };
        Some(Self {
            id: imported_id(theme, scheme),
            name,
            current,
            sets,
            origin,
        })
    }

    /// Writes the theme's folder into `dir`, which must exist.
    fn write(&self, dir: &Path) -> Result<()> {
        let name = self.name.replace('\\', "").replace('"', "'");
        let manifest_text = format!(
            "# Imported from Spicetify by spacetify.\n\n\
             id = \"{id}\"\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             author = \"Spicetify (imported)\"\n\
             description = \"The colours of the Spicetify {origin}.\"\n\
             spacetify = \">={major}.{minor}.0\"\n",
            id = self.id,
            origin = self.origin.replace('"', "'"),
            major = manifest::current_version().major,
            minor = manifest::current_version().minor,
        );
        let manifest_path = dir.join("theme.toml");
        fs::write(&manifest_path, manifest_text).map_err(Error::io(&manifest_path))?;
        let colors_path = dir.join("colors.toml");
        fs::write(&colors_path, colors_toml(&self.sets, &self.origin))
            .map_err(Error::io(&colors_path))?;
        Ok(())
    }
}

/// Every colour scheme of the themes in Spicetify's user folder, and of the
/// theme it shows if that one came with Spicetify.
fn themes(spicetify: &Spicetify) -> Vec<ThemeImport> {
    let config = &spicetify.config;
    let mut folders: Vec<PathBuf> = list_dirs(&spicetify.user_dir.join("Themes"));
    if let Some(app) = &spicetify.app_dir {
        let names: Vec<String> = folders.iter().filter_map(|f| file_name(f)).collect();
        folders.extend(list_dirs(&app.join("Themes")).into_iter().filter(|dir| {
            file_name(dir).is_some_and(|name| {
                name.eq_ignore_ascii_case(&config.current_theme) && !names.contains(&name)
            })
        }));
    }
    let mut imports: Vec<ThemeImport> = Vec::new();
    for folder in folders {
        let Some(theme) = file_name(&folder) else {
            continue;
        };
        if theme.eq_ignore_ascii_case(MARKETPLACE) {
            continue;
        }
        let Ok(text) = fs::read_to_string(folder.join(COLOR_FILE)) else {
            continue;
        };
        let schemes: Vec<(String, BTreeMap<String, String>)> = parse_ini(&text)
            .into_iter()
            .filter(|(name, colors)| !name.is_empty() && !colors.is_empty())
            .collect();
        let is_current = theme.eq_ignore_ascii_case(&config.current_theme);
        let current_scheme = schemes
            .iter()
            .position(|(name, _)| name.eq_ignore_ascii_case(&config.color_scheme))
            .unwrap_or(0);
        let several = schemes.len() > 1;
        for (index, (scheme, colors)) in schemes.iter().enumerate() {
            let scheme = several.then_some(scheme.as_str());
            let current = is_current && index == current_scheme;
            if let Some(import) = ThemeImport::new(&theme, scheme, colors, current)
                && !imports.iter().any(|other| other.id == import.id)
            {
                imports.push(import);
            }
        }
    }
    imports
}

fn list_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

// ------------------------------------------------------------------ plan

/// What happens to Spotify's interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SpotifyStep {
    /// Spicetify left nothing in Spotify.
    Clean,
    /// Spotify already uses its own interface; only the folders Spicetify
    /// unpacked are left, and Spotify ignores them.
    Leftovers { folders: Vec<PathBuf> },
    /// Spicetify modifies Spotify: its backup of the original goes back.
    Restore {
        backup: PathBuf,
        folders: Vec<PathBuf>,
    },
    /// Spicetify modifies Spotify and its backup cannot be used.
    Reinstall { reason: Text },
}

/// One of Spicetify's extensions or apps, and what replaces it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Replacement {
    pub name: String,
    /// Id of the gallery's equivalent, if there is one.
    pub equivalent: Option<String>,
}

/// What leaving Spicetify would do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub spotify: SpotifyStep,
    pub themes: Vec<ThemeImport>,
    /// The theme Spicetify shows came from its Marketplace.
    pub marketplace_theme: bool,
    pub extensions: Vec<Replacement>,
    /// Spicetify's custom apps. Its Marketplace is replaced by the gallery.
    pub apps: Vec<Replacement>,
}

impl Plan {
    /// Whether there is anything to do at all.
    pub fn is_empty(&self) -> bool {
        self.spotify == SpotifyStep::Clean
            && self.themes.is_empty()
            && !self.marketplace_theme
            && self.extensions.is_empty()
            && self.apps.is_empty()
    }
}

pub fn plan(install: &SpotifyInstall, spicetify: Option<&Spicetify>) -> Result<Plan> {
    let spotify = spotify_step(install, spicetify)?;
    let Some(spicetify) = spicetify else {
        return Ok(Plan {
            spotify,
            themes: Vec::new(),
            marketplace_theme: false,
            extensions: Vec::new(),
            apps: Vec::new(),
        });
    };
    let config = &spicetify.config;
    let extensions = config
        .extensions
        .iter()
        .map(|name| {
            let stem = name.trim_end_matches(".js").to_ascii_lowercase();
            Replacement {
                name: name.clone(),
                equivalent: EQUIVALENTS
                    .iter()
                    .find(|(spice, _)| *spice == stem)
                    .map(|(_, ours)| (*ours).to_owned()),
            }
        })
        .collect();
    let apps = config
        .custom_apps
        .iter()
        .map(|name| Replacement {
            name: name.clone(),
            equivalent: name
                .eq_ignore_ascii_case(MARKETPLACE)
                .then(|| "gallery".to_owned()),
        })
        .collect();
    Ok(Plan {
        spotify,
        themes: themes(spicetify),
        marketplace_theme: config.current_theme.eq_ignore_ascii_case(MARKETPLACE),
        extensions,
        apps,
    })
}

/// Folders Spicetify unpacks inside Spotify: `Apps/xpui` and `Apps/login`.
fn unpacked(install: &SpotifyInstall) -> Vec<PathBuf> {
    ["xpui", "login"]
        .iter()
        .map(|name| install.apps_dir.join(name))
        .filter(|dir| dir.is_dir())
        .collect()
}

fn spotify_step(install: &SpotifyInstall, spicetify: Option<&Spicetify>) -> Result<SpotifyStep> {
    let inspection = detect::inspect(install)?;
    let folders = unpacked(install);
    if inspection.state != UiState::Spicetify {
        // Spotify only reads a folder when its .spa is missing.
        let ignored: Vec<PathBuf> = folders
            .into_iter()
            .filter(|dir| dir.with_extension("spa").is_file())
            .collect();
        return Ok(if ignored.is_empty() {
            SpotifyStep::Clean
        } else {
            SpotifyStep::Leftovers { folders: ignored }
        });
    }
    let Some(spicetify) = spicetify else {
        return Ok(SpotifyStep::Reinstall {
            reason: Text::new(
                "Spicetify's settings were not found, so neither was its backup of Spotify.",
                "Les réglages de Spicetify sont introuvables, donc sa sauvegarde de Spotify aussi.",
            ),
        });
    };
    let backup = spicetify.user_dir.join("Backup").join("xpui.spa");
    if !backup.is_file() || detect::inspect_spa(&backup).ok() != Some(UiState::Original) {
        return Ok(SpotifyStep::Reinstall {
            reason: Text::new(
                "Spicetify has no usable backup of Spotify's original interface.",
                "Spicetify n'a pas de sauvegarde utilisable de l'interface d'origine de Spotify.",
            ),
        });
    }
    let made_from = spicetify.config.backup_version.as_deref().unwrap_or("");
    if let Some(version) = &install.version
        && !same_version(made_from, version)
    {
        return Ok(SpotifyStep::Reinstall {
            reason: Text::new(
                format!(
                    "Spicetify's backup comes from Spotify {made_from}, but Spotify {version} is installed: putting it back would bring back an older interface."
                ),
                format!(
                    "La sauvegarde de Spicetify vient de Spotify {made_from}, mais c'est Spotify {version} qui est installé : la remettre ramènerait une ancienne interface."
                ),
            ),
        });
    }
    Ok(SpotifyStep::Restore { backup, folders })
}

/// "1.3.5.3058.g5d40d5480152" and "1.3.5.3058" are the same version.
fn same_version(a: &str, b: &str) -> bool {
    let numbers = |v: &str| -> Vec<String> {
        v.split('.')
            .take_while(|part| part.bytes().all(|b| b.is_ascii_digit()) && !part.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (a, b) = (numbers(a), numbers(b));
    !a.is_empty() && a == b
}

// ----------------------------------------------------------------- leave

/// What [`leave`] did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Outcome {
    pub restored: bool,
    pub removed: Vec<PathBuf>,
    /// Ids of the imported themes.
    pub themes: Vec<String>,
    /// The imported theme made active, if any.
    pub active_theme: Option<String>,
    /// The Marketplace theme will be imported when Spotify starts.
    pub marketplace_pending: bool,
    /// Ids of the gallery's equivalents installed and turned on.
    pub installed: Vec<String>,
    /// Equivalents that could not be installed (no internet…), with why.
    pub failed: Vec<(String, Text)>,
}

/// Leaves Spicetify as `plan` says. Spotify must be closed. The mods are
/// not applied: the caller does it, as for any change of mods.
pub fn leave(install: &SpotifyInstall, plan: &Plan, dirs: &AppDirs) -> Result<Outcome> {
    if spotify::is_running()? {
        return Err(Error::SpotifyRunning);
    }
    leave_with(install, plan, dirs, &Source::from_env())
}

fn leave_with(
    install: &SpotifyInstall,
    plan: &Plan,
    dirs: &AppDirs,
    source: &Source,
) -> Result<Outcome> {
    install.ensure_modifiable()?;
    let mut outcome = Outcome::default();

    match &plan.spotify {
        SpotifyStep::Clean => {}
        SpotifyStep::Reinstall { reason } => {
            return Err(Error::Spicetify(reason.clone()));
        }
        SpotifyStep::Leftovers { folders } => remove_folders(folders, &mut outcome)?,
        SpotifyStep::Restore { backup, folders } => {
            restore_spa(backup, &install.xpui_spa())?;
            let login = backup.with_file_name("login.spa");
            let login_target = install.apps_dir.join("login.spa");
            if !login_target.is_file() && login.is_file() {
                restore_spa(&login, &login_target)?;
            }
            outcome.restored = true;
            let ignored: Vec<PathBuf> = folders
                .iter()
                .filter(|dir| dir.with_extension("spa").is_file())
                .cloned()
                .collect();
            remove_folders(&ignored, &mut outcome)?;
            if detect::inspect(install)?.state != UiState::Original {
                return Err(Error::Spicetify(Text::new(
                    "Spotify still does not look original after putting the backup back.",
                    "Spotify ne semble toujours pas d'origine après la remise de la sauvegarde.",
                )));
            }
        }
    }

    let store = dirs.mods();
    let mut config = Config::load(&dirs.config_file())?;
    if !plan.themes.is_empty() {
        let staging = dirs.data_dir.join("spicetify-import");
        let _ = fs::remove_dir_all(&staging);
        for theme in &plan.themes {
            let dir = staging.join(&theme.id);
            fs::create_dir_all(&dir).map_err(Error::io(&dir))?;
            theme.write(&dir)?;
            store.install(&dir)?;
            outcome.themes.push(theme.id.clone());
            if theme.current {
                store.enable(&mut config, &theme.id)?;
                outcome.active_theme = Some(theme.id.clone());
            }
        }
        let _ = fs::remove_dir_all(&staging);
    }

    let wanted: Vec<&str> = plan
        .extensions
        .iter()
        .filter_map(|e| e.equivalent.as_deref())
        .collect();
    if !wanted.is_empty() {
        match source.catalog() {
            Ok(catalog) => {
                for id in wanted {
                    let installed = catalog
                        .get(id)
                        .ok_or_else(|| Error::NotInGallery(id.to_owned()))
                        .and_then(|entry| gallery::install(source, entry, &store))
                        .and_then(|_| store.enable(&mut config, id));
                    match installed {
                        Ok(_) => outcome.installed.push(id.to_owned()),
                        Err(err) => outcome.failed.push((id.to_owned(), Text::of_error(&err))),
                    }
                }
            }
            Err(err) => {
                let why = Text::of_error(&err);
                outcome.failed = wanted
                    .iter()
                    .map(|id| ((*id).to_owned(), why.clone()))
                    .collect();
            }
        }
    }
    config.save(&dirs.config_file())?;

    outcome.marketplace_pending = plan.marketplace_theme;
    Marker {
        marketplace_theme_pending: plan.marketplace_theme,
    }
    .save(dirs)?;
    Ok(outcome)
}

fn restore_spa(from: &Path, to: &Path) -> Result<()> {
    let bytes = fs::read(from).map_err(Error::io(from))?;
    write_atomic(to, &bytes)
}

fn remove_folders(folders: &[PathBuf], outcome: &mut Outcome) -> Result<()> {
    for folder in folders {
        fs::remove_dir_all(folder).map_err(Error::io(folder))?;
        tracing::info!("removed Spicetify's {}", folder.display());
        outcome.removed.push(folder.clone());
    }
    Ok(())
}

// ---------------------------------------------------------------- marker

/// Written once Spicetify was left, so the app stops offering it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// The Marketplace theme is still to be imported.
    pub marketplace_theme_pending: bool,
}

impl Marker {
    fn file(dirs: &AppDirs) -> PathBuf {
        dirs.data_dir.join("spicetify.json")
    }

    pub fn load(dirs: &AppDirs) -> Option<Self> {
        serde_json::from_slice(&fs::read(Self::file(dirs)).ok()?).ok()
    }

    pub fn save(&self, dirs: &AppDirs) -> Result<()> {
        let path = Self::file(dirs);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        let json = serde_json::to_vec_pretty(self).expect("marker is serializable");
        write_atomic(&path, &json)
    }
}

/// Whether Spicetify was already left with spacetify.
pub fn already_left(dirs: &AppDirs) -> bool {
    Marker::load(dirs).is_some()
}

/// Imports the Marketplace theme sent by the gallery page, if leaving
/// Spicetify asked for it, and makes it the active theme. Returns its id,
/// or `None` when it was not asked for.
pub fn marketplace_theme(
    dirs: &AppDirs,
    title: &str,
    scheme: Option<&str>,
    colors: &BTreeMap<String, String>,
) -> Result<Option<String>> {
    if !Marker::load(dirs).is_some_and(|m| m.marketplace_theme_pending) {
        return Ok(None);
    }
    let title = title.trim();
    let title = if title.is_empty() {
        "Marketplace"
    } else {
        title
    };
    let theme = ThemeImport::new(title, scheme, colors, true).ok_or_else(|| {
        Error::Spicetify(Text::new(
            "The Marketplace theme has no colour spacetify can use.",
            "Le thème du Marketplace n'a aucune couleur utilisable par spacetify.",
        ))
    })?;
    let staging = dirs.data_dir.join("spicetify-import").join(&theme.id);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(Error::io(&staging))?;
    theme.write(&staging)?;
    let store = dirs.mods();
    let installed = store.install(&staging);
    let _ = fs::remove_dir_all(dirs.data_dir.join("spicetify-import"));
    installed?;
    let mut config = Config::load(&dirs.config_file())?;
    store.enable(&mut config, &theme.id)?;
    config.save(&dirs.config_file())?;
    Marker {
        marketplace_theme_pending: false,
    }
    .save(dirs)?;
    Ok(Some(theme.id))
}

#[cfg(test)]
#[path = "spicetify_tests.rs"]
mod tests;
