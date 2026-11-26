//! "Leave Spicetify" on the Home page: what it would do, and doing it in
//! one click (then the mods are applied, as with Apply).

use serde::Serialize;
use spacetify_core::gallery::Link;
use spacetify_core::patcher::{self, Payload};
use spacetify_core::service::ServicePaths;
use spacetify_core::spicetify::{self, Plan, SpotifyStep};
use spacetify_core::spotify;
use spacetify_core::{Lang, tr};
use tauri::{AppHandle, State};

use crate::commands::{Answer, Done, blocking, close_if_running, fail, progress};
use crate::state::{Failure, Shared};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveView {
    /// Show the offer on Home.
    pub offered: bool,
    /// One line per thing that will happen.
    pub steps: Vec<Step>,
    /// Why it cannot be done here, if so.
    pub blocked: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Step {
    pub label: String,
    pub detail: String,
    /// "good": done for you; "none": nothing replaces it yet.
    pub tone: &'static str,
}

#[tauri::command]
pub async fn spicetify_plan(state: State<'_, Shared>) -> Answer<LeaveView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let nothing = || LeaveView {
            offered: false,
            steps: Vec::new(),
            blocked: None,
        };
        let Ok(install) = state.install() else {
            return Ok(nothing());
        };
        let found = spicetify::find();
        let plan = spicetify::plan(&install, found.as_ref()).map_err(fail(lang))?;
        let left = spicetify::already_left(&state.dirs);
        if plan.is_empty() || (left && plan.spotify == SpotifyStep::Clean) {
            return Ok(nothing());
        }
        let blocked = match &plan.spotify {
            SpotifyStep::Reinstall { reason } => Some(format!(
                "{} {}",
                reason.get(lang),
                tr!(
                    lang,
                    "Run `spicetify restore`, or reinstall Spotify from spotify.com/download (your account and downloads stay), then come back here.",
                    "Lance `spicetify restore`, ou réinstalle Spotify depuis spotify.com/download (ton compte et tes téléchargements restent), puis reviens ici."
                )
            )),
            _ => None,
        };
        Ok(LeaveView {
            offered: true,
            steps: steps(lang, &plan),
            blocked,
        })
    })
    .await
}

fn steps(lang: Lang, plan: &Plan) -> Vec<Step> {
    let mut steps = Vec::new();
    match &plan.spotify {
        SpotifyStep::Clean | SpotifyStep::Reinstall { .. } => {}
        SpotifyStep::Leftovers { folders } => steps.push(Step {
            label: "Spotify".into(),
            detail: tr!(
                lang,
                "Remove the {} folder(s) Spicetify left inside Spotify",
                "Retirer les {} dossier(s) laissé(s) par Spicetify dans Spotify",
                folders.len()
            ),
            tone: "good",
        }),
        SpotifyStep::Restore { .. } => steps.push(Step {
            label: "Spotify".into(),
            detail: tr!(
                lang,
                "Put back Spotify's original interface, from Spicetify's backup",
                "Remettre l'interface d'origine de Spotify, depuis la sauvegarde de Spicetify"
            ),
            tone: "good",
        }),
    }
    for theme in &plan.themes {
        steps.push(Step {
            label: theme.name.clone(),
            detail: if theme.current {
                tr!(
                    lang,
                    "Colours imported, and made the active theme",
                    "Couleurs importées, et choisi comme thème actif"
                )
            } else {
                tr!(lang, "Colours imported", "Couleurs importées")
            },
            tone: "good",
        });
    }
    if plan.marketplace_theme {
        steps.push(Step {
            label: tr!(lang, "Marketplace theme", "Thème du Marketplace"),
            detail: tr!(
                lang,
                "Its colours are imported the next time Spotify starts, if there is one",
                "Ses couleurs sont importées au prochain démarrage de Spotify, s'il y en a un"
            ),
            tone: "good",
        });
    }
    for item in plan.extensions.iter().chain(&plan.apps) {
        let (detail, tone) = match item.equivalent.as_deref() {
            Some("gallery") => (
                tr!(
                    lang,
                    "Replaced by the spacetify gallery, built into Spotify",
                    "Remplacé par la galerie spacetify, intégrée à Spotify"
                ),
                "good",
            ),
            Some(id) => (
                tr!(
                    lang,
                    "Replaced by “{id}” from the gallery, installed for you",
                    "Remplacée par « {id} » de la galerie, installée pour toi"
                ),
                "good",
            ),
            None => (
                tr!(lang, "No equivalent yet", "Pas encore d'équivalent"),
                "none",
            ),
        };
        steps.push(Step {
            label: item.name.clone(),
            detail,
            tone,
        });
    }
    steps
}

#[tauri::command]
pub async fn leave_spicetify(app: AppHandle, state: State<'_, Shared>) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let _busy = state.try_busy().ok_or_else(|| Failure::busy(lang))?;
        let install = state.install().map_err(fail(lang))?;
        install.ensure_modifiable().map_err(fail(lang))?;
        let found = spicetify::find();
        let plan = spicetify::plan(&install, found.as_ref()).map_err(fail(lang))?;
        // Agreeing to leave Spicetify is agreeing to close Spotify for it.
        let closed = close_if_running(&app, lang, true)?;

        progress(
            &app,
            tr!(lang, "Leaving Spicetify…", "Sortie de Spicetify…"),
        );
        let left = spicetify::leave(&install, &plan, &state.dirs);
        let applied = left.as_ref().ok().map(|_| {
            progress(
                &app,
                tr!(lang, "Applying the mods…", "Application des mods…"),
            );
            let mods = state
                .config()
                .and_then(|config| state.mods().active(&config))?;
            let link = Link::for_payload(&ServicePaths::new(&state.dirs));
            patcher::apply(
                &install,
                &state.store(),
                &Payload::new(&mods).with_gallery(link.as_ref()),
            )
        });
        if closed || left.is_ok() {
            progress(&app, tr!(lang, "Opening Spotify…", "Ouverture de Spotify…"));
            let _ = spotify::launch(&install, &[]);
        }
        let outcome = left.map_err(fail(lang))?;
        if let Some(Err(err)) = applied {
            return Err(Failure::of(&err, lang));
        }

        let mut done = Vec::new();
        if !outcome.themes.is_empty() {
            done.push(tr!(
                lang,
                "{} theme(s) imported",
                "{} thème(s) importé(s)",
                outcome.themes.len()
            ));
        }
        if !outcome.installed.is_empty() {
            done.push(tr!(
                lang,
                "{} extension(s) installed",
                "{} extension(s) installée(s)",
                outcome.installed.len()
            ));
        }
        for (id, why) in &outcome.failed {
            done.push(tr!(
                lang,
                "“{id}” could not be installed: {}",
                "« {id} » n'a pas pu être installée : {}",
                why.get(lang)
            ));
        }
        Ok(Done {
            message: tr!(
                lang,
                "You have left Spicetify. Your mods no longer disappear after Spotify updates.",
                "Tu as quitté Spicetify. Tes mods ne disparaissent plus après les mises à jour de Spotify."
            ),
            detail: (!done.is_empty()).then(|| format!("{}.", done.join(" · "))),
        })
    })
    .await
}
