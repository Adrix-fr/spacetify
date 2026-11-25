//! User settings and the folders spacetify writes to.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mods::ModStore;
use crate::{APP_NAME, Error, Result};

/// Contents of `config.toml`. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Spotify install folder, when auto-detection is not enough.
    pub spotify_path: Option<PathBuf>,
    /// `en` or `fr`. Unset means the system language.
    pub language: Option<String>,
    /// Id of the active theme. Unset means Spotify's own look.
    pub theme: Option<String>,
    /// Ids of the enabled extensions, in loading order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Extension settings chosen in spacetify, by extension id then setting
    /// name. They replace the values saved in Spotify at its next start.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, BTreeMap<String, toml::Value>>,
}

impl Config {
    /// Loads the file, or returns the defaults if it does not exist yet.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|err| Error::Config {
                path: path.to_path_buf(),
                reason: err.to_string(),
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(Error::io(path)(err)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        let text = toml::to_string_pretty(self).map_err(|err| Error::Config {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        fs::write(path, text).map_err(Error::io(path))
    }
}

/// Folders used by spacetify.
///
/// Follows each OS's conventions. Setting `SPACETIFY_HOME` puts everything
/// in one folder instead, which is handy for portable setups.
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppDirs {
    pub fn detect() -> Result<Self> {
        if let Some(home) = std::env::var_os("SPACETIFY_HOME") {
            return Ok(Self::in_folder(home));
        }
        Ok(Self {
            config_dir: dirs::config_dir().ok_or(Error::NoUserDirs)?.join(APP_NAME),
            data_dir: dirs::data_local_dir()
                .ok_or(Error::NoUserDirs)?
                .join(APP_NAME),
        })
    }

    /// Everything in one folder, as with `SPACETIFY_HOME`.
    pub fn in_folder(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        Self {
            config_dir: home.clone(),
            data_dir: home,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Installed themes and extensions live next to the config file.
    pub fn mods(&self) -> ModStore {
        ModStore::new(&self.config_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let config = Config {
            spotify_path: Some(PathBuf::from("/opt/spotify")),
            language: Some("fr".into()),
            theme: Some("midnight".into()),
            extensions: vec!["hello-badge".into()],
            settings: BTreeMap::from([(
                "hello-badge".into(),
                BTreeMap::from([("text".into(), toml::Value::String("Hi".into()))]),
            )]),
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn rejects_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "spotfy_path = \"typo\"\n").unwrap();
        assert!(matches!(Config::load(&path), Err(Error::Config { .. })));
    }
}
