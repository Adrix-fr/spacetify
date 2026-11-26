//! The gallery: the official list of themes and extensions, kept in this
//! repository, that can be installed from inside Spotify.
//!
//! - The list is one file, `gallery/index.json`. It names each mod, where
//!   its files are in the repository and the SHA-256 of every file.
//! - It is downloaded when someone opens the gallery (one request for the
//!   whole list), then kept for a few minutes, and on disk as a fallback.
//! - A mod's files are downloaded only when someone installs it. Each file
//!   must have the hash written in the list, then the mod is validated like
//!   any other before it is installed.
//! - Files come from `raw.githubusercontent.com`, not GitHub's API (which
//!   allows few requests without an account). If GitHub refuses or cannot
//!   be reached, the same files are taken from the jsDelivr mirror: the
//!   hashes make a mirror as safe as the original.
//!
//! Only text files that run inside Spotify are downloaded, never programs.
//! The page in Spotify talks to the background service, which does all of
//! this: see [`server`].

mod link;
pub mod review;
pub mod server;
pub mod submission;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use link::Link;

use crate::config::{AppDirs, Config};
use crate::mods::{self, Installed, Loaded, ModKind, ModStore, manifest};
use crate::{Error, Result};

/// Path of the list inside the repository.
pub const INDEX_PATH: &str = "gallery/index.json";
/// Where the repository's files are read from, in order of preference.
pub const OFFICIAL_BASES: [&str; 2] = [
    "https://raw.githubusercontent.com/Adrix-fr/spacetify/main/",
    "https://cdn.jsdelivr.net/gh/Adrix-fr/spacetify@main/",
];
/// Where a mod's files can be read by people.
pub const CODE_BASE: &str = "https://github.com/Adrix-fr/spacetify/tree/main/";
/// Set to a folder (or a base URL ending in `/`) to use another copy of the
/// repository, for example to try a new list before publishing it.
pub const SOURCE_ENV: &str = "SPACETIFY_GALLERY";

const FORMAT: u32 = 1;
const MAX_INDEX: u64 = 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

/// Contents of `gallery/index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    pub mods: Vec<Entry>,
}

/// One mod of the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: ModKind,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_fr: Option<String>,
    /// Folder of the mod in the repository, such as `examples/themes/midnight`.
    pub path: String,
    /// Every file of the mod, relative to `path`.
    pub files: Vec<FileEntry>,
    /// Colours of the preview, for themes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Preview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
}

/// The colours used to draw a small picture of a theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub frame: String,
    pub background: String,
    pub highlight: String,
    pub text: String,
    pub accent: String,
}

impl Preview {
    /// Computed from a theme's `colors.toml`, with Spotify's own colours for
    /// what it leaves out.
    pub fn of_colors(colors: &BTreeMap<String, BTreeMap<String, String>>) -> Self {
        let pick = |choices: &[(&str, &str)], fallback: &str| {
            choices
                .iter()
                .find_map(|(set, token)| colors.get(*set).and_then(|tokens| tokens.get(*token)))
                .cloned()
                .unwrap_or_else(|| fallback.to_owned())
        };
        Self {
            frame: pick(&[("app-frame", "background-base")], "#000000"),
            background: pick(&[("base", "background-base")], "#121212"),
            highlight: pick(&[("base", "background-highlight")], "#1f1f1f"),
            text: pick(
                &[("base", "text-base"), ("base", "text-subdued")],
                "#b3b3b3",
            ),
            accent: pick(
                &[
                    ("bright-accent", "background-base"),
                    ("base", "essential-bright-accent"),
                    ("base", "text-bright-accent"),
                ],
                "#1ed760",
            ),
        }
    }
}

impl Catalog {
    /// Reads and checks the list. Anything odd refuses the whole list.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let invalid = |reason: String| Error::GalleryInvalid(reason);
        let catalog: Self =
            serde_json::from_slice(bytes).map_err(|err| invalid(err.to_string()))?;
        if catalog.format != FORMAT {
            return Err(invalid(format!("unknown format {}", catalog.format)));
        }
        let mut ids = std::collections::BTreeSet::new();
        for entry in &catalog.mods {
            let id = &entry.id;
            if !manifest::is_valid_id(id) || !ids.insert(id.clone()) {
                return Err(invalid(format!("bad or repeated id `{id}`")));
            }
            if !is_safe_path(&entry.path) {
                return Err(invalid(format!("bad folder for `{id}`")));
            }
            if entry.files.is_empty() || entry.files.len() > mods::MAX_FILES {
                return Err(invalid(format!("bad number of files for `{id}`")));
            }
            let mut names = std::collections::BTreeSet::new();
            for file in &entry.files {
                if !is_safe_path(&file.path) || !names.insert(file.path.as_str()) {
                    return Err(invalid(format!("bad file `{}` in `{id}`", file.path)));
                }
                if file.sha256.len() != 64
                    || !file
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(invalid(format!("bad hash for `{}` in `{id}`", file.path)));
                }
            }
            if !names.contains(entry.kind.manifest()) {
                return Err(invalid(format!("no {} in `{id}`", entry.kind.manifest())));
            }
            if let Some(p) = &entry.preview
                && ![&p.frame, &p.background, &p.highlight, &p.text, &p.accent]
                    .iter()
                    .all(|color| mods::theme::is_hex_color(color))
            {
                return Err(invalid(format!("bad preview colour in `{id}`")));
            }
        }
        Ok(catalog)
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.mods.iter().find(|entry| entry.id == id)
    }
}

/// A relative path with plain names only: no `..`, no hidden files, no
/// backslashes, nothing absolute.
fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Where the gallery's files are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Base URLs of the repository, tried in order.
    Web(Vec<String>),
    /// A copy of the repository on this computer.
    Folder(PathBuf),
}

impl Source {
    pub fn official() -> Self {
        Self::Web(
            OFFICIAL_BASES
                .iter()
                .map(|base| (*base).to_owned())
                .collect(),
        )
    }

    /// The official one, unless [`SOURCE_ENV`] says otherwise.
    pub fn from_env() -> Self {
        match std::env::var(SOURCE_ENV) {
            Ok(value) if value.starts_with("https://") || value.starts_with("http://") => {
                let base = if value.ends_with('/') {
                    value
                } else {
                    format!("{value}/")
                };
                Self::Web(vec![base])
            }
            Ok(value) if !value.trim().is_empty() => Self::Folder(PathBuf::from(value)),
            _ => Self::official(),
        }
    }

    /// Link to read a mod's files on GitHub, for the official list only.
    pub fn code_url(&self, entry: &Entry) -> Option<String> {
        (*self == Self::official()).then(|| format!("{CODE_BASE}{}", entry.path))
    }

    /// Downloads one file of the repository. `path` must be safe.
    fn get(&self, path: &str, max: u64) -> Result<Vec<u8>> {
        match self {
            Self::Folder(root) => {
                let file = root.join(path);
                let bytes = fs::read(&file).map_err(Error::io(&file))?;
                if bytes.len() as u64 > max {
                    return Err(Error::GalleryInvalid(format!("{path} is too large")));
                }
                Ok(bytes)
            }
            Self::Web(bases) => {
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(TIMEOUT))
                    .user_agent(concat!("spacetify/", env!("CARGO_PKG_VERSION")))
                    .build()
                    .into();
                let mut last = String::new();
                for base in bases {
                    let url = format!("{base}{path}");
                    match agent.get(&url).call() {
                        Ok(mut response) => {
                            return response
                                .body_mut()
                                .with_config()
                                .limit(max)
                                .read_to_vec()
                                .map_err(|err| Error::GalleryUnreachable(err.to_string()));
                        }
                        // Not there: a mirror will not have it either.
                        Err(ureq::Error::StatusCode(404)) => {
                            return Err(Error::GalleryUnreachable(format!("{url}: not found")));
                        }
                        Err(err) => {
                            tracing::info!("{url}: {err}");
                            last = err.to_string();
                        }
                    }
                }
                Err(Error::GalleryUnreachable(last))
            }
        }
    }

    /// Downloads and checks the list.
    pub fn catalog(&self) -> Result<Catalog> {
        Catalog::parse(&self.get(INDEX_PATH, MAX_INDEX)?)
    }
}

/// The last list downloaded, kept to show something when GitHub cannot be
/// reached.
pub fn saved_catalog_file(dirs: &AppDirs) -> PathBuf {
    dirs.data_dir.join("gallery").join("index.json")
}

pub fn save_catalog(dirs: &AppDirs, catalog: &Catalog) {
    let path = saved_catalog_file(dirs);
    let json = serde_json::to_vec_pretty(catalog).expect("catalog is serializable");
    let saved = path
        .parent()
        .map_or(Ok(()), |dir| {
            fs::create_dir_all(dir).map_err(Error::io(dir))
        })
        .and_then(|()| crate::backup::write_atomic(&path, &json));
    if let Err(err) = saved {
        tracing::warn!("could not keep the gallery list: {err}");
    }
}

pub fn load_saved_catalog(dirs: &AppDirs) -> Option<Catalog> {
    fs::read(saved_catalog_file(dirs))
        .ok()
        .and_then(|bytes| Catalog::parse(&bytes).ok())
}

/// Downloads the mod `entry`, checks every file against the list, then
/// installs it like `spacetify install` would. Does not turn it on.
pub fn install(source: &Source, entry: &Entry, store: &ModStore) -> Result<Installed> {
    let staging = store.downloads_dir().join(&entry.id);
    remove_dir(&staging)?;
    let result = download(source, entry, &staging).and_then(|()| {
        let mismatch = || Error::GalleryModMismatch(entry.id.clone());
        let loaded = mods::load_dir(&staging).map_err(|issue| Error::InvalidMod {
            what: entry.name.clone(),
            issue,
        })?;
        let meta = loaded.meta();
        if loaded.kind() != entry.kind
            || meta.id != entry.id
            || meta.version.to_string() != entry.version
        {
            return Err(mismatch());
        }
        if let Loaded::Theme(theme) = &loaded
            && entry.preview.is_some()
            && entry.preview != Some(Preview::of_colors(&theme.colors))
        {
            return Err(mismatch());
        }
        store.install(&staging)
    });
    let _ = fs::remove_dir_all(&staging);
    result
}

fn download(source: &Source, entry: &Entry, dir: &Path) -> Result<()> {
    let mut total = 0u64;
    for file in &entry.files {
        let bytes = source.get(&format!("{}/{}", entry.path, file.path), MAX_FILE)?;
        total += bytes.len() as u64;
        if total > mods::MAX_TOTAL {
            return Err(Error::GalleryModMismatch(entry.id.clone()));
        }
        if sha256_hex(&bytes) != file.sha256 {
            return Err(Error::GalleryFileMismatch {
                id: entry.id.clone(),
                file: file.path.clone(),
            });
        }
        let target = dir.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        fs::write(&target, &bytes).map_err(Error::io(&target))?;
    }
    Ok(())
}

fn remove_dir(dir: &Path) -> Result<()> {
    match fs::remove_dir_all(dir) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(Error::io(dir)(err)),
        _ => Ok(()),
    }
}

/// One mod as the page in Spotify shows it: from the gallery, installed, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModView {
    pub id: String,
    pub kind: ModKind,
    pub name: String,
    pub author: String,
    /// Version in the gallery, or the installed one.
    pub version: String,
    pub description: String,
    pub description_fr: Option<String>,
    pub preview: Option<Preview>,
    pub in_gallery: bool,
    /// Installed version, if installed and valid.
    pub installed: Option<String>,
    pub active: bool,
    /// The gallery has another version than the installed one.
    pub update: bool,
    pub code_url: Option<String>,
}

/// The gallery's mods, then installed mods that are not in it.
pub fn views(
    catalog: Option<&Catalog>,
    source: &Source,
    store: &ModStore,
    config: &Config,
) -> Result<Vec<ModView>> {
    let listed = store.list(config)?;
    let mut views = Vec::new();
    for entry in catalog.map(|c| c.mods.as_slice()).unwrap_or_default() {
        let local = listed
            .iter()
            .find(|l| l.id == entry.id && l.kind == entry.kind);
        let installed = local.and_then(|l| l.meta.as_ref().ok().map(|m| m.version.to_string()));
        views.push(ModView {
            id: entry.id.clone(),
            kind: entry.kind,
            name: entry.name.clone(),
            author: entry.author.clone(),
            version: entry.version.clone(),
            description: entry.description.clone(),
            description_fr: entry.description_fr.clone(),
            preview: entry.preview.clone(),
            in_gallery: true,
            update: installed.as_ref().is_some_and(|v| *v != entry.version),
            installed,
            active: local.is_some_and(|l| l.active),
            code_url: source.code_url(entry),
        });
    }
    for local in &listed {
        if views.iter().any(|v| v.id == local.id) {
            continue;
        }
        let Ok(meta) = &local.meta else { continue };
        let preview = match local.kind {
            ModKind::Theme => store
                .load_theme(&local.id)
                .ok()
                .map(|theme| Preview::of_colors(&theme.colors)),
            ModKind::Extension => None,
        };
        views.push(ModView {
            id: local.id.clone(),
            kind: local.kind,
            name: meta.name.clone(),
            author: meta.author.clone(),
            version: meta.version.to_string(),
            description: meta.description.clone().unwrap_or_default(),
            description_fr: None,
            preview,
            in_gallery: false,
            installed: Some(meta.version.to_string()),
            active: local.active,
            update: false,
            code_url: None,
        });
    }
    Ok(views)
}

#[cfg(test)]
mod tests;
