//! Everything the window can ask for. Each command calls `spacetify_core`,
//! as the CLI does; long ones run on a worker thread and report their
//! progress, so the window never freezes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value as Json;
use spacetify_core::detect;
use spacetify_core::doctor;
use spacetify_core::gallery::{self, Link};
use spacetify_core::mods::{ModKind, ModStore};
use spacetify_core::patcher::{self, ApplyOutcome, Payload, RestoreOutcome};
use spacetify_core::service::autostart;
use spacetify_core::service::events::EventLog;
use spacetify_core::service::{self, Request, StartOutcome};
use spacetify_core::spotify::{self, InstallKind};
use spacetify_core::{Error, Lang, tr};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::model::{self, DoctorView, EventView, Palette, ServiceView, SettingView, SpotifyView};
use crate::state::{Failure, Shared, lang_code, resolve_lang};
use crate::tray;

const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) type Answer<T> = Result<T, Failure>;

/// Runs `job` on a worker thread.
pub(crate) async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Answer<T> + Send + 'static,
) -> Answer<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|err| Failure::new(None, err.to_string()))?
}

pub(crate) fn fail(lang: Lang) -> impl Fn(Error) -> Failure {
    move |err| Failure::of(&err, lang)
}

/// Tells the window what a long operation is doing.
pub(crate) fn progress(app: &AppHandle, text: String) {
    let _ = app.emit("progress", text);
}

// ------------------------------------------------------------------ home

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub spotify: SpotifyView,
    pub service: ServiceView,
    pub theme: Option<ThemeBrief>,
    pub extensions: Vec<String>,
    /// Problems with the mods turned on (deleted by hand, invalid…).
    pub mods_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThemeBrief {
    pub id: String,
    pub name: String,
    pub palette: Palette,
}

#[tauri::command]
pub async fn overview(state: State<'_, Shared>) -> Answer<Overview> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let (theme, extensions, mods_error) = match state
            .config()
            .and_then(|config| state.mods().active(&config))
        {
            Ok(active) => (
                active.theme.as_ref().map(|theme| ThemeBrief {
                    id: theme.manifest.meta.id.clone(),
                    name: theme.manifest.meta.name.clone(),
                    palette: model::theme_palette(lang, theme),
                }),
                active
                    .extensions
                    .iter()
                    .map(|e| e.manifest.meta.name.clone())
                    .collect(),
                None,
            ),
            Err(err) => (None, Vec::new(), Some(err.localized(lang))),
        };
        Ok(Overview {
            spotify: spotify_view(&state, lang),
            service: service_view(&state, lang),
            theme,
            extensions,
            mods_error,
        })
    })
    .await
}

fn spotify_view(state: &Shared, lang: Lang) -> SpotifyView {
    let install = match state.install() {
        Ok(install) => install,
        Err(err) => {
            return SpotifyView {
                error: Some(Failure::of(&err, lang).message),
                ..SpotifyView::default()
            };
        }
    };
    let mut view = SpotifyView {
        found: true,
        path: Some(install.root.display().to_string()),
        version: install.version.clone(),
        kind: Some(install.kind.label().to_owned()),
        read_only: !install.kind.is_modifiable(),
        running: spotify::is_running().unwrap_or(false),
        store_also_installed: install.kind == InstallKind::Standard
            && spotify::microsoft_store_installed(),
        backups: state.store().list().map(|list| list.len()).unwrap_or(0),
        ..SpotifyView::default()
    };
    if view.read_only {
        view.error = Some(
            Error::ReadOnlyInstall {
                kind: install.kind.label(),
            }
            .localized(lang),
        );
        return view;
    }
    match detect::inspect(&install) {
        Ok(inspection) => {
            view.state = Some(model::state_code(&inspection.state));
            view.state_label = Some(model::state_label(lang, &inspection.state));
            view.spicetify_leftovers = inspection
                .spicetify_leftovers
                .map(|dir| dir.display().to_string());
            view.update_replaced = inspection.state == detect::UiState::Original
                && state.store().load_applied().ok().flatten().is_some();
        }
        Err(err) => view.error = Some(err.localized(lang)),
    }
    view
}

fn service_view(state: &Shared, lang: Lang) -> ServiceView {
    let running = service::is_running(&state.service);
    let status = service::read_status(&state.service).ok().flatten();
    model::service_view(
        lang,
        running,
        autostart::status().map_err(|err| err.localized(lang)),
        status.as_ref(),
        service::executable().is_ok(),
    )
}

/// What happened, in one or two sentences.
#[derive(Debug, Serialize)]
pub struct Done {
    pub message: String,
    pub detail: Option<String>,
}

#[tauri::command]
pub async fn apply(app: AppHandle, state: State<'_, Shared>, close_spotify: bool) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let _busy = state.try_busy().ok_or_else(|| Failure::busy(lang))?;
        let install = state.install().map_err(fail(lang))?;
        install.ensure_modifiable().map_err(fail(lang))?;
        let mods = state
            .config()
            .and_then(|config| state.mods().active(&config))
            .map_err(fail(lang))?;
        let closed = close_if_running(&app, lang, close_spotify)?;

        progress(
            &app,
            tr!(lang, "Applying the mods…", "Application des mods…"),
        );
        let link = Link::for_payload(&service::ServicePaths::new(&state.dirs));
        let payload = Payload::new(&mods).with_gallery(link.as_ref());
        let result = patcher::apply(&install, &state.store(), &payload);
        if closed {
            progress(
                &app,
                tr!(lang, "Restarting Spotify…", "Relance de Spotify…"),
            );
            let _ = spotify::launch(&install, &[]);
        }
        let done = match result.map_err(fail(lang))? {
            ApplyOutcome::Applied { new_backup, .. } => Done {
                message: tr!(lang, "Mods applied.", "Mods appliqués."),
                detail: new_backup.then(|| {
                    tr!(
                        lang,
                        "Spotify's original interface was backed up and checked first.",
                        "L'interface d'origine de Spotify a d'abord été sauvegardée et vérifiée."
                    )
                }),
            },
            ApplyOutcome::AlreadyApplied => Done {
                message: tr!(
                    lang,
                    "Already applied, nothing to do.",
                    "Déjà appliqué, rien à faire."
                ),
                detail: None,
            },
        };
        Ok(with_launch_hint(lang, done, closed))
    })
    .await
}

#[tauri::command]
pub async fn restore(
    app: AppHandle,
    state: State<'_, Shared>,
    close_spotify: bool,
) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let _busy = state.try_busy().ok_or_else(|| Failure::busy(lang))?;
        let install = state.install().map_err(fail(lang))?;
        install.ensure_modifiable().map_err(fail(lang))?;
        let closed = close_if_running(&app, lang, close_spotify)?;

        progress(
            &app,
            tr!(
                lang,
                "Putting the original back…",
                "Remise de l'interface d'origine…"
            ),
        );
        let result = patcher::restore(&install, &state.store());
        if closed {
            progress(
                &app,
                tr!(lang, "Restarting Spotify…", "Relance de Spotify…"),
            );
            let _ = spotify::launch(&install, &[]);
        }
        let message = match result.map_err(fail(lang))? {
            RestoreOutcome::Restored { .. } => tr!(
                lang,
                "Spotify is back to its original state (checked by hash).",
                "Spotify est revenu à son état d'origine (empreinte vérifiée)."
            ),
            RestoreOutcome::AlreadyOriginal => tr!(
                lang,
                "Spotify was already original, nothing to do.",
                "Spotify était déjà d'origine, rien à faire."
            ),
        };
        Ok(Done {
            message,
            detail: None,
        })
    })
    .await
}

fn with_launch_hint(lang: Lang, mut done: Done, restarted: bool) -> Done {
    if !restarted && done.detail.is_none() {
        done.detail = Some(tr!(
            lang,
            "Open Spotify to see the result.",
            "Ouvre Spotify pour voir le résultat."
        ));
    }
    done
}

/// Closes Spotify if it runs and the user agreed. Returns whether it did.
pub(crate) fn close_if_running(app: &AppHandle, lang: Lang, allowed: bool) -> Answer<bool> {
    if !spotify::is_running().map_err(fail(lang))? {
        return Ok(false);
    }
    if !allowed {
        return Err(Failure::of(&Error::SpotifyRunning, lang));
    }
    progress(app, tr!(lang, "Closing Spotify…", "Fermeture de Spotify…"));
    spotify::close(CLOSE_TIMEOUT).map_err(fail(lang))?;
    Ok(true)
}

#[tauri::command]
pub async fn launch_spotify(state: State<'_, Shared>) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        if spotify::is_running().map_err(fail(lang))? {
            return Ok(());
        }
        let install = state.install().map_err(fail(lang))?;
        spotify::launch(&install, &[]).map_err(fail(lang))?;
        Ok(())
    })
    .await
}

// ------------------------------------------------------------------ mods

#[derive(Debug, Serialize)]
pub struct ModsView {
    pub themes: Vec<ModView>,
    pub extensions: Vec<ModView>,
    pub examples: Vec<ExampleView>,
    pub folder: String,
    /// Spotify's own colours, for the "no theme" card.
    pub original: Palette,
}

#[derive(Debug, Serialize)]
pub struct ModView {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub active: bool,
    /// Why it was refused, when it cannot be loaded.
    pub error: Option<String>,
    pub palette: Option<Palette>,
    pub settings: Vec<SettingView>,
    /// Exactly as the gallery accepted it (antivirus, robot, team review).
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct ExampleView {
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub installed: bool,
}

#[tauri::command]
pub async fn list_mods(app: AppHandle, state: State<'_, Shared>) -> Answer<ModsView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let config = state.config().map_err(fail(lang))?;
        let store = state.mods();
        let listed = store.list(&config).map_err(fail(lang))?;
        // The list last seen by the gallery: no download here.
        let catalog = gallery::load_saved_catalog(&state.dirs);
        let mut themes = Vec::new();
        let mut extensions = Vec::new();
        for item in listed {
            let mut view = ModView {
                id: item.id.clone(),
                name: item.id.clone(),
                version: None,
                author: None,
                description: None,
                active: item.active,
                error: None,
                palette: None,
                settings: Vec::new(),
                verified: catalog.as_ref().is_some_and(|catalog| {
                    gallery::is_verified(catalog, &store, item.kind, &item.id)
                }),
            };
            match &item.meta {
                Ok(meta) => {
                    view.name = meta.name.clone();
                    view.version = Some(meta.version.to_string());
                    view.author = Some(meta.author.clone());
                    view.description = meta.description.clone();
                }
                Err(issue) => view.error = Some(issue.localized(lang)),
            }
            match item.kind {
                ModKind::Theme => {
                    if view.error.is_none() {
                        view.palette = store
                            .load_theme(&item.id)
                            .ok()
                            .map(|theme| model::theme_palette(lang, &theme));
                    }
                    themes.push(view);
                }
                ModKind::Extension => {
                    if view.error.is_none() {
                        view.settings = store
                            .settings(&config, &item.id)
                            .map(|values| values.into_iter().map(model::setting_view).collect())
                            .unwrap_or_default();
                    }
                    extensions.push(view);
                }
            }
        }
        let examples = examples(&app, &store);
        Ok(ModsView {
            themes,
            extensions,
            examples,
            folder: state.config_dir().display().to_string(),
            original: model::spotify_palette(lang),
        })
    })
    .await
}

/// The example mods shipped with the app (from the repository in development).
fn examples_dir(app: &AppHandle) -> Option<PathBuf> {
    let shipped = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("examples"));
    if let Some(dir) = shipped.filter(|dir| dir.is_dir()) {
        return Some(dir);
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    (cfg!(debug_assertions) && repo.is_dir()).then_some(repo)
}

fn examples(app: &AppHandle, store: &ModStore) -> Vec<ExampleView> {
    let Some(root) = examples_dir(app) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (folder, kind) in [("themes", "theme"), ("extensions", "extension")] {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let Ok(loaded) = spacetify_core::mods::load_dir(&dir) else {
                continue;
            };
            let meta = loaded.meta();
            found.push(ExampleView {
                kind,
                id: meta.id.clone(),
                name: meta.name.clone(),
                description: meta.description.clone(),
                installed: store.kind_of(&meta.id).is_some(),
            });
        }
    }
    found
}

/// Installs a mod and turns it on, as `spacetify install` does.
fn install_from(state: &Shared, lang: Lang, dir: &Path) -> Answer<Done> {
    let store = state.mods();
    let installed = store.install(dir).map_err(fail(lang))?;
    let mut config = state.config().map_err(fail(lang))?;
    store
        .enable(&mut config, &installed.meta.id)
        .map_err(fail(lang))?;
    state.save(&config).map_err(fail(lang))?;
    let name = &installed.meta.name;
    let message = match installed.kind {
        ModKind::Theme => tr!(
            lang,
            "Theme “{name}” installed. It is now the active theme.",
            "Thème « {name} » installé. C'est maintenant le thème actif."
        ),
        ModKind::Extension => tr!(
            lang,
            "Extension “{name}” installed and turned on.",
            "Extension « {name} » installée et activée."
        ),
    };
    Ok(Done {
        message,
        detail: Some(apply_hint(lang)),
    })
}

fn apply_hint(lang: Lang) -> String {
    tr!(
        lang,
        "Press Apply to see it in Spotify.",
        "Clique sur Appliquer pour le voir dans Spotify."
    )
}

#[tauri::command]
pub async fn install_mod(state: State<'_, Shared>, path: String) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || install_from(&state, state.lang(), Path::new(&path))).await
}

#[tauri::command]
pub async fn install_example(app: AppHandle, state: State<'_, Shared>, id: String) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let root =
            examples_dir(&app).ok_or_else(|| Failure::of(&Error::ModNotFound(id.clone()), lang))?;
        let dir = ["themes", "extensions"]
            .iter()
            .map(|folder| root.join(folder).join(&id))
            .find(|dir| dir.is_dir())
            .ok_or_else(|| Failure::of(&Error::ModNotFound(id.clone()), lang))?;
        install_from(&state, lang, &dir)
    })
    .await
}

#[tauri::command]
pub async fn uninstall_mod(state: State<'_, Shared>, id: String) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let mut config = state.config().map_err(fail(lang))?;
        state
            .mods()
            .uninstall(&mut config, &id)
            .map_err(fail(lang))?;
        state.save(&config).map_err(fail(lang))?;
        Ok(Done {
            message: tr!(lang, "`{id}` uninstalled.", "`{id}` désinstallé."),
            detail: Some(apply_hint(lang)),
        })
    })
    .await
}

/// Chooses the active theme, `None` for Spotify's own look.
#[tauri::command]
pub async fn set_theme(state: State<'_, Shared>, id: Option<String>) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let mut config = state.config().map_err(fail(lang))?;
        state
            .mods()
            .set_theme(&mut config, id.as_deref())
            .map_err(fail(lang))?;
        state.save(&config).map_err(fail(lang))
    })
    .await
}

#[tauri::command]
pub async fn set_extension(state: State<'_, Shared>, id: String, on: bool) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let mut config = state.config().map_err(fail(lang))?;
        let store = state.mods();
        if on {
            store.enable(&mut config, &id).map_err(fail(lang))?;
        } else {
            store.disable(&mut config, &id).map_err(fail(lang))?;
        }
        state.save(&config).map_err(fail(lang))
    })
    .await
}

/// Chooses a setting's value, or goes back to the default with `None`.
#[tauri::command]
pub async fn set_setting(
    state: State<'_, Shared>,
    id: String,
    name: String,
    value: Option<Json>,
) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let mut config = state.config().map_err(fail(lang))?;
        state
            .mods()
            .set_setting(&mut config, &id, &name, value.as_ref())
            .map_err(fail(lang))?;
        state.save(&config).map_err(fail(lang))
    })
    .await
}

// ------------------------------------------------------------ compatibility

#[tauri::command]
pub async fn doctor(state: State<'_, Shared>) -> Answer<DoctorView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let install = state.install().map_err(fail(lang))?;
        let config = state.config().map_err(fail(lang))?;
        let report = doctor::run(&install, &state.mods(), &config).map_err(fail(lang))?;
        Ok(model::doctor_view(lang, &report))
    })
    .await
}

// ------------------------------------------------------------------ service

#[derive(Debug, Serialize)]
pub struct RepairView {
    pub service: ServiceView,
    pub events: Vec<EventView>,
    pub log: String,
}

#[tauri::command]
pub async fn repair_info(state: State<'_, Shared>, count: Option<usize>) -> Answer<RepairView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let events = EventLog::new(state.service.events_file())
            .recent(count.unwrap_or(30))
            .unwrap_or_default();
        Ok(RepairView {
            service: service_view(&state, lang),
            events: events
                .iter()
                .rev()
                .map(|event| model::event_view(lang, event))
                .collect(),
            log: state.service.log_file().display().to_string(),
        })
    })
    .await
}

/// Starts or stops the service now.
#[tauri::command]
pub async fn set_service_running(state: State<'_, Shared>, on: bool) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let message = if on {
            let exe = service::executable().map_err(fail(lang))?;
            match service::start(&state.service, &exe, &state.service_args()).map_err(fail(lang))? {
                StartOutcome::Started => tr!(lang, "Service started.", "Service lancé."),
                StartOutcome::AlreadyRunning => tr!(
                    lang,
                    "The service is already running.",
                    "Le service tourne déjà."
                ),
            }
        } else {
            service::stop(&state.service).map_err(fail(lang))?;
            tr!(lang, "Service stopped.", "Service arrêté.")
        };
        let detail = (on && state.store().load_applied().ok().flatten().is_none()).then(|| {
            tr!(
                lang,
                "Apply your mods once: the service keeps them in place after that.",
                "Applique tes mods une fois : le service les garde en place ensuite."
            )
        });
        Ok(Done { message, detail })
    })
    .await
}

/// Turns the start with Windows on or off.
#[tauri::command]
pub async fn set_autostart(state: State<'_, Shared>, on: bool) -> Answer<Done> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let message = if on {
            let exe = service::executable().map_err(fail(lang))?;
            autostart::enable(&exe, &state.service_args()).map_err(fail(lang))?;
            tr!(
                lang,
                "The service will start with Windows. It shows as “spacetify” in Task Manager > Startup apps.",
                "Le service démarrera avec Windows. Il apparaît comme « spacetify » dans Gestionnaire des tâches > Applications de démarrage."
            )
        } else {
            autostart::disable().map_err(fail(lang))?;
            tr!(
                lang,
                "The service no longer starts with Windows.",
                "Le service ne démarre plus avec Windows."
            )
        };
        Ok(Done {
            message,
            detail: None,
        })
    })
    .await
}

/// Asks the service to look at Spotify now.
#[tauri::command]
pub async fn service_check(state: State<'_, Shared>) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        service::send(&state.service, Request::Check).map_err(fail(lang))
    })
    .await
}

// ------------------------------------------------------------------ settings

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// `system`, `en` or `fr`.
    pub language: String,
    /// The language in use: `en` or `fr`.
    pub lang: &'static str,
    pub version: &'static str,
    pub config_dir: String,
    pub data_dir: String,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Shared>) -> Answer<SettingsView> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let config = state.config().map_err(fail(lang))?;
        Ok(SettingsView {
            language: config.language.unwrap_or_else(|| "system".into()),
            lang: lang_code(lang),
            version: env!("CARGO_PKG_VERSION"),
            config_dir: state.dirs.config_dir.display().to_string(),
            data_dir: state.dirs.data_dir.display().to_string(),
        })
    })
    .await
}

/// `system`, `en` or `fr`. Shared with the CLI through `config.toml`.
#[tauri::command]
pub async fn set_language(
    app: AppHandle,
    state: State<'_, Shared>,
    language: String,
) -> Answer<()> {
    let state = state.inner().clone();
    blocking(move || {
        let lang = state.lang();
        let mut config = state.config().map_err(fail(lang))?;
        config.language = match language.as_str() {
            "en" | "fr" => Some(language),
            _ => None,
        };
        state.save(&config).map_err(fail(lang))?;
        let lang = resolve_lang(config.language.as_deref());
        state.set_lang(lang);
        tray::refresh(&app, lang);
        Ok(())
    })
    .await
}
