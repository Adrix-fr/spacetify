//! What the app keeps while it runs: folders, language, and a lock so only
//! one long operation runs at a time.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use serde::Serialize;
use spacetify_core::backup::BackupStore;
use spacetify_core::config::{AppDirs, Config};
use spacetify_core::mods::ModStore;
use spacetify_core::service::ServicePaths;
use spacetify_core::spotify::{self, SpotifyInstall};
use spacetify_core::{Error, Lang, Result, tr};

pub type Shared = Arc<AppState>;

pub struct AppState {
    pub dirs: AppDirs,
    pub service: ServicePaths,
    lang: Mutex<Lang>,
    busy: Mutex<()>,
    /// The program started from the Terminal tab, if one runs.
    pub terminal: Mutex<Option<std::process::Child>>,
}

impl AppState {
    pub fn load() -> Shared {
        let dirs = AppDirs::detect().unwrap_or_else(|err| {
            tracing::error!("{err}");
            AppDirs::in_folder(std::env::temp_dir().join(spacetify_core::APP_NAME))
        });
        let config = Config::load(&dirs.config_file()).unwrap_or_default();
        Arc::new(Self {
            service: ServicePaths::new(&dirs),
            lang: Mutex::new(resolve_lang(config.language.as_deref())),
            busy: Mutex::new(()),
            terminal: Mutex::new(None),
            dirs,
        })
    }

    pub fn lang(&self) -> Lang {
        *self.lang.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_lang(&self, lang: Lang) {
        *self.lang.lock().unwrap_or_else(PoisonError::into_inner) = lang;
    }

    pub fn config(&self) -> Result<Config> {
        Config::load(&self.dirs.config_file())
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        config.save(&self.dirs.config_file())
    }

    pub fn mods(&self) -> ModStore {
        self.dirs.mods()
    }

    pub fn store(&self) -> BackupStore {
        BackupStore::new(self.dirs.backups_dir())
    }

    pub fn install(&self) -> Result<SpotifyInstall> {
        spotify::locate(self.config()?.spotify_path.as_deref())
    }

    /// Options the service needs to see the same folders as the app.
    pub fn service_args(&self) -> Vec<String> {
        match std::env::var_os("SPACETIFY_HOME") {
            Some(home) => vec!["--home".into(), home.to_string_lossy().into_owned()],
            None => Vec::new(),
        }
    }

    /// Held during apply, restore and the like. `None` if one already runs.
    pub fn try_busy(&self) -> Option<MutexGuard<'_, ()>> {
        match self.busy.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    pub fn config_dir(&self) -> PathBuf {
        self.dirs.config_dir.clone()
    }
}

/// The language saved in `config.toml` (`en`, `fr`), else the system's.
pub fn resolve_lang(saved: Option<&str>) -> Lang {
    saved.and_then(Lang::parse).unwrap_or_else(Lang::detect)
}

pub fn lang_code(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "en",
        Lang::Fr => "fr",
    }
}

/// An error as the window shows it. `code` lets it react to some of them,
/// such as asking to close Spotify.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Failure {
    pub code: Option<&'static str>,
    pub message: String,
}

impl Failure {
    pub fn new(code: Option<&'static str>, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn of(err: &Error, lang: Lang) -> Self {
        let code = match err {
            Error::SpotifyRunning => Some("spotify_running"),
            Error::SpicetifyActive => Some("spicetify"),
            Error::Busy => Some("busy"),
            Error::SpotifyNotFound => Some("spotify_not_found"),
            _ => None,
        };
        let message = match err {
            // The CLI's wording mentions `--yes`; the app asks instead.
            Error::SpotifyRunning => tr!(
                lang,
                "Spotify is open. It has to be closed first.",
                "Spotify est ouvert. Il faut d'abord le fermer."
            ),
            // The CLI's wording mentions `--spotify-path`.
            Error::SpotifyNotFound => tr!(
                lang,
                "Spotify was not found on this computer. Install it from spotify.com/download (the Microsoft Store version cannot be modded).",
                "Spotify est introuvable sur cet ordinateur. Installe-le depuis spotify.com/download (la version Microsoft Store ne peut pas être modifiée)."
            ),
            _ => err.localized(lang),
        };
        Self::new(code, message)
    }

    pub fn busy(lang: Lang) -> Self {
        Self::of(&Error::Busy, lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_a_code_the_window_can_act_on() {
        let failure = Failure::of(&Error::SpotifyRunning, Lang::Fr);
        assert_eq!(failure.code, Some("spotify_running"));
        assert!(!failure.message.contains("--yes"), "{}", failure.message);
        assert_eq!(
            Failure::of(&Error::SpicetifyActive, Lang::En).code,
            Some("spicetify")
        );
        assert_eq!(Failure::of(&Error::NoUserDirs, Lang::En).code, None);
    }

    #[test]
    fn saved_language_wins_over_the_system() {
        assert_eq!(resolve_lang(Some("fr")), Lang::Fr);
        assert_eq!(resolve_lang(Some("en-US")), Lang::En);
        assert_eq!(lang_code(Lang::Fr), "fr");
    }
}
