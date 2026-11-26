//! Themes and extensions: where they are stored, which ones are active, and
//! how they are checked before being injected.
//!
//! Mods live in the config folder:
//!
//! ```text
//! <config>/themes/<id>/       theme.toml, theme.css, colors.toml, assets/
//! <config>/extensions/<id>/   extension.toml, main.js
//! ```
//!
//! `config.toml` remembers the active theme and the enabled extensions.
//! Everything is validated again before each injection, so a mod edited in
//! place cannot slip through broken.

pub mod css;
pub mod extension;
mod files;
mod issue;
pub mod manifest;
pub mod theme;

use std::fs;
use std::path::{Path, PathBuf};

pub use extension::Extension;
pub use files::{MAX_FILES, MAX_TOTAL};
pub use issue::{AssetProblem, ModIssue};
pub use manifest::{Meta, Setting, SettingKind};
pub use theme::Theme;

use serde_json::Value as Json;

use crate::config::Config;
use crate::{Error, Lang, Result};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ModKind {
    Theme,
    Extension,
}

impl ModKind {
    pub fn label(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Self::Theme, Lang::En) => "theme",
            (Self::Theme, Lang::Fr) => "thème",
            (Self::Extension, _) => "extension",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Self::Theme => "themes",
            Self::Extension => "extensions",
        }
    }

    pub(crate) fn manifest(self) -> &'static str {
        match self {
            Self::Theme => theme::MANIFEST,
            Self::Extension => extension::MANIFEST,
        }
    }
}

/// A mod read from a folder.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    Theme(Theme),
    Extension(Extension),
}

impl Loaded {
    pub fn kind(&self) -> ModKind {
        match self {
            Self::Theme(_) => ModKind::Theme,
            Self::Extension(_) => ModKind::Extension,
        }
    }

    pub fn meta(&self) -> &Meta {
        match self {
            Self::Theme(theme) => &theme.manifest.meta,
            Self::Extension(extension) => &extension.manifest.meta,
        }
    }
}

/// Reads and validates the mod in `dir`, whatever its kind.
pub fn load_dir(dir: &Path) -> Result<Loaded, ModIssue> {
    let is_theme = dir.join(theme::MANIFEST).is_file();
    let is_extension = dir.join(extension::MANIFEST).is_file();
    match (is_theme, is_extension) {
        (true, true) => Err(ModIssue::TwoManifests),
        (true, false) => theme::load(dir).map(Loaded::Theme),
        (false, true) => extension::load(dir).map(Loaded::Extension),
        (false, false) => Err(ModIssue::NoManifest),
    }
}

/// One installed mod, valid or not.
#[derive(Debug, Clone)]
pub struct Listed {
    pub kind: ModKind,
    /// Folder name, which is the id for mods installed by spacetify.
    pub id: String,
    pub meta: Result<Meta, ModIssue>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct Installed {
    pub kind: ModKind,
    pub meta: Meta,
    /// An older copy with the same id was replaced.
    pub replaced: bool,
}

/// What is injected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActiveMods {
    pub theme: Option<Theme>,
    pub extensions: Vec<Extension>,
}

impl ActiveMods {
    pub fn is_empty(&self) -> bool {
        self.theme.is_none() && self.extensions.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ModStore {
    root: PathBuf,
}

impl ModStore {
    /// `root` is the config folder.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn dir_of(&self, kind: ModKind) -> PathBuf {
        self.root.join(kind.folder())
    }

    pub fn path_of(&self, kind: ModKind, id: &str) -> PathBuf {
        self.dir_of(kind).join(id)
    }

    /// Where mods from the gallery are downloaded and checked before they
    /// are installed.
    pub(crate) fn downloads_dir(&self) -> PathBuf {
        self.root.join(".downloads")
    }

    /// Kind of the installed mod `id`, if any.
    pub fn kind_of(&self, id: &str) -> Option<ModKind> {
        if !manifest::is_valid_id(id) {
            return None;
        }
        [ModKind::Theme, ModKind::Extension]
            .into_iter()
            .find(|&kind| self.path_of(kind, id).join(kind.manifest()).is_file())
    }

    /// Every installed mod, themes first, sorted by id.
    pub fn list(&self, config: &Config) -> Result<Vec<Listed>> {
        let mut listed = Vec::new();
        for kind in [ModKind::Theme, ModKind::Extension] {
            let dir = self.dir_of(kind);
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(Error::io(&dir)(err)),
            };
            let mut ids = Vec::new();
            for entry in entries {
                let entry = entry.map_err(Error::io(&dir))?;
                let id = entry.file_name().to_string_lossy().into_owned();
                if !id.starts_with('.') && entry.path().is_dir() {
                    ids.push(id);
                }
            }
            ids.sort();
            for id in ids {
                let meta = self
                    .load_checked(kind, &id)
                    .map(|loaded| loaded.meta().clone());
                let active = match kind {
                    ModKind::Theme => config.theme.as_deref() == Some(id.as_str()),
                    ModKind::Extension => config.extensions.contains(&id),
                };
                listed.push(Listed {
                    kind,
                    id,
                    meta,
                    active,
                });
            }
        }
        Ok(listed)
    }

    pub fn load_theme(&self, id: &str) -> Result<Theme> {
        match self.load(ModKind::Theme, id)? {
            Loaded::Theme(theme) => Ok(theme),
            Loaded::Extension(_) => Err(Error::NotATheme(id.to_owned())),
        }
    }

    pub fn load_extension(&self, id: &str) -> Result<Extension> {
        match self.load(ModKind::Extension, id)? {
            Loaded::Extension(extension) => Ok(extension),
            Loaded::Theme(_) => Err(Error::ModNotFound(id.to_owned())),
        }
    }

    /// Validates the mod in `src`, then copies it into the store, replacing
    /// an older copy with the same id. The copy is checked again before it
    /// takes the place of the old one.
    pub fn install(&self, src: &Path) -> Result<Installed> {
        let refused = |issue| Error::InvalidMod {
            what: src.display().to_string(),
            issue,
        };
        if !src.is_dir() {
            return Err(Error::ModNotFound(src.display().to_string()));
        }
        let loaded = load_dir(src).map_err(refused)?;
        let kind = loaded.kind();
        let meta = loaded.meta().clone();

        let src_real = src.canonicalize().map_err(Error::io(src))?;
        for store_kind in [ModKind::Theme, ModKind::Extension] {
            if let Ok(store_dir) = self.dir_of(store_kind).canonicalize()
                && src_real.starts_with(&store_dir)
            {
                return Err(Error::InstallFromStore(src.to_path_buf()));
            }
        }
        if let Some(other) = self.kind_of(&meta.id)
            && other != kind
        {
            return Err(Error::IdTaken {
                id: meta.id,
                kind: other,
            });
        }

        let parent = self.dir_of(kind);
        fs::create_dir_all(&parent).map_err(Error::io(&parent))?;
        let staging = parent.join(format!(".{}.installing", meta.id));
        remove_dir_if_exists(&staging)?;
        let copied = files::copy_mod_dir(src, &staging).and_then(|copy| {
            copy.and_then(|()| load_dir(&staging).map(drop))
                .map_err(refused)
        });
        if let Err(err) = copied {
            let _ = fs::remove_dir_all(&staging);
            return Err(err);
        }

        let target = self.path_of(kind, &meta.id);
        let replaced = target.exists();
        remove_dir_if_exists(&target)?;
        fs::rename(&staging, &target).map_err(Error::io(&target))?;
        tracing::info!("installed {} into {}", meta.id, target.display());
        Ok(Installed {
            kind,
            meta,
            replaced,
        })
    }

    /// Deletes an installed mod and forgets it in `config`.
    pub fn uninstall(&self, config: &mut Config, id: &str) -> Result<ModKind> {
        let kind = self
            .kind_of(id)
            .ok_or_else(|| Error::ModNotFound(id.to_owned()))?;
        let dir = self.path_of(kind, id);
        fs::remove_dir_all(&dir).map_err(Error::io(&dir))?;
        if config.theme.as_deref() == Some(id) {
            config.theme = None;
        }
        config.extensions.retain(|e| e != id);
        config.settings.remove(id);
        Ok(kind)
    }

    /// Makes `id` the active theme, or enables the extension `id`.
    /// Returns its kind and whether anything changed.
    pub fn enable(&self, config: &mut Config, id: &str) -> Result<(ModKind, bool)> {
        let kind = self
            .kind_of(id)
            .ok_or_else(|| Error::ModNotFound(id.to_owned()))?;
        self.load(kind, id)?;
        let changed = match kind {
            ModKind::Theme => config.theme.replace(id.to_owned()).as_deref() != Some(id),
            ModKind::Extension if config.extensions.iter().any(|e| e == id) => false,
            ModKind::Extension => {
                config.extensions.push(id.to_owned());
                true
            }
        };
        Ok((kind, changed))
    }

    /// Turns off the theme or extension `id`. Works even if it was deleted
    /// by hand, so the config can always be cleaned up.
    pub fn disable(&self, config: &mut Config, id: &str) -> Result<bool> {
        let was_theme = config.theme.as_deref() == Some(id);
        if was_theme {
            config.theme = None;
        }
        let before = config.extensions.len();
        config.extensions.retain(|e| e != id);
        let changed = was_theme || config.extensions.len() != before;
        if !changed && self.kind_of(id).is_none() {
            return Err(Error::ModNotFound(id.to_owned()));
        }
        Ok(changed)
    }

    /// Sets the active theme (`None` for Spotify's own look).
    pub fn set_theme(&self, config: &mut Config, id: Option<&str>) -> Result<bool> {
        let Some(id) = id else {
            return Ok(config.theme.take().is_some());
        };
        match self.kind_of(id) {
            Some(ModKind::Theme) => self.enable(config, id).map(|(_, changed)| changed),
            Some(ModKind::Extension) => Err(Error::NotATheme(id.to_owned())),
            None => Err(Error::ModNotFound(id.to_owned())),
        }
    }

    /// Loads and validates everything that is turned on in `config`.
    pub fn active(&self, config: &Config) -> Result<ActiveMods> {
        let theme = config
            .theme
            .as_deref()
            .map(|id| self.load_theme(id))
            .transpose()?;
        let extensions = config
            .extensions
            .iter()
            .map(|id| {
                let mut extension = self.load_extension(id)?;
                extension.configured = configured(config, &extension);
                Ok(extension)
            })
            .collect::<Result<_>>()?;
        Ok(ActiveMods { theme, extensions })
    }

    /// The settings of the extension `id`, with the value chosen in
    /// spacetify, or the default. In the manifest's order.
    pub fn settings(&self, config: &Config, id: &str) -> Result<Vec<SettingValue>> {
        let extension = self.load_extension(id)?;
        let chosen = configured(config, &extension);
        Ok(extension
            .manifest
            .settings
            .iter()
            .map(|(name, setting)| SettingValue {
                name: name.clone(),
                value: chosen
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| setting.default.clone()),
                custom: chosen.contains_key(name),
                setting: setting.clone(),
            })
            .collect())
    }

    /// Chooses the value of a setting of the extension `id`, or goes back to
    /// the default with `None`. Spotify takes it at the next `apply`.
    pub fn set_setting(
        &self,
        config: &mut Config,
        id: &str,
        name: &str,
        value: Option<&Json>,
    ) -> Result<()> {
        let extension = self.load_extension(id)?;
        let invalid = || Error::InvalidSetting {
            id: id.to_owned(),
            setting: name.to_owned(),
        };
        let setting = extension.manifest.settings.get(name).ok_or_else(invalid)?;
        match value {
            Some(value) => {
                let value = setting.value_to_toml(value).ok_or_else(invalid)?;
                config
                    .settings
                    .entry(id.to_owned())
                    .or_default()
                    .insert(name.to_owned(), value);
            }
            None => {
                if let Some(values) = config.settings.get_mut(id) {
                    values.remove(name);
                    if values.is_empty() {
                        config.settings.remove(id);
                    }
                }
            }
        }
        Ok(())
    }

    fn load(&self, kind: ModKind, id: &str) -> Result<Loaded> {
        if self.kind_of(id) != Some(kind) {
            return Err(Error::ModNotFound(id.to_owned()));
        }
        self.load_checked(kind, id)
            .map_err(|issue| Error::InvalidMod {
                what: id.to_owned(),
                issue,
            })
    }

    fn load_checked(&self, kind: ModKind, id: &str) -> Result<Loaded, ModIssue> {
        let loaded = load_dir(&self.path_of(kind, id))?;
        if loaded.kind() != kind {
            return Err(ModIssue::NoManifest);
        }
        if loaded.meta().id != id {
            return Err(ModIssue::FolderMismatch {
                folder: id.to_owned(),
                id: loaded.meta().id.clone(),
            });
        }
        Ok(loaded)
    }
}

/// One setting of an extension and its current value.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingValue {
    pub name: String,
    pub setting: Setting,
    pub value: Json,
    /// Chosen in spacetify, rather than the default.
    pub custom: bool,
}

/// Values from `config.toml` that fit the extension's manifest. The others
/// (a setting that was removed or changed type in an update) are ignored.
fn configured(config: &Config, extension: &Extension) -> std::collections::BTreeMap<String, Json> {
    let Some(values) = config.settings.get(&extension.manifest.meta.id) else {
        return Default::default();
    };
    values
        .iter()
        .filter_map(|(name, value)| {
            let setting = extension.manifest.settings.get(name)?;
            Some((name.clone(), setting.value_from_toml(value)?))
        })
        .collect()
}

fn remove_dir_if_exists(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(Error::io(dir)(err)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
