//! Tells who, if anyone, has modified Spotify's interface.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::backup::sha256_file;
use crate::patcher::{MARKER_PATH, Marker};
use crate::spotify::SpotifyInstall;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiState {
    /// `xpui.spa` as shipped by Spotify.
    Original,
    /// Patched by us. The marker points to the original backup.
    Patched(Marker),
    /// Modified by Spicetify. We never touch it.
    Spicetify,
    /// No interface file at all.
    Missing,
    /// Changed in a way we do not recognise.
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct Inspection {
    pub state: UiState,
    /// Hash of the current `xpui.spa`, if it exists.
    pub sha256: Option<String>,
    /// An `Apps/xpui` folder that Spotify ignores because `xpui.spa` exists.
    /// Spicetify leaves one behind after Spotify updates.
    pub spicetify_leftovers: Option<PathBuf>,
}

pub fn inspect(install: &SpotifyInstall) -> Result<Inspection> {
    let spa = install.xpui_spa();
    let unpacked = install.xpui_dir();

    if spa.is_file() {
        return Ok(Inspection {
            state: inspect_spa(&spa)?,
            sha256: Some(sha256_file(&spa)?),
            spicetify_leftovers: unpacked.is_dir().then_some(unpacked),
        });
    }

    // Without xpui.spa, Spotify loads the unpacked folder instead. That is how Spicetify works.
    let index = unpacked.join("index.html");
    let state = match fs::read_to_string(&index) {
        Ok(html) if mentions_spicetify(&html) => UiState::Spicetify,
        Ok(_) => UiState::Unknown("unpacked Apps/xpui folder without xpui.spa".into()),
        Err(_) => UiState::Missing,
    };
    Ok(Inspection {
        state,
        sha256: None,
        spicetify_leftovers: None,
    })
}

/// Looks inside an `xpui.spa` archive.
pub fn inspect_spa(path: &Path) -> Result<UiState> {
    let invalid = |reason: String| Error::InvalidArchive {
        path: path.to_path_buf(),
        reason,
    };
    let file = File::open(path).map_err(Error::io(path))?;
    let mut archive =
        ZipArchive::new(BufReader::new(file)).map_err(|err| invalid(err.to_string()))?;

    if let Ok(mut entry) = archive.by_name(MARKER_PATH) {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(Error::io(path))?;
        return Ok(match serde_json::from_str(&json) {
            Ok(marker) => UiState::Patched(marker),
            Err(_) => UiState::Unknown(format!("unreadable {MARKER_PATH}")),
        });
    }

    if archive
        .file_names()
        .any(|name| name.is_ok_and(|name| mentions_spicetify(&name)))
    {
        return Ok(UiState::Spicetify);
    }
    let mut html = String::new();
    archive
        .by_name("index.html")
        .map_err(|_| invalid("index.html not found".into()))?
        .read_to_string(&mut html)
        .map_err(Error::io(path))?;

    Ok(if mentions_spicetify(&html) {
        UiState::Spicetify
    } else {
        UiState::Original
    })
}

/// Whether Spicetify itself is installed for this user.
pub fn spicetify_installed() -> bool {
    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(dirs::config_dir().map(|d| d.join("spicetify")));
    dirs.extend(dirs::data_local_dir().map(|d| d.join("spicetify")));
    dirs.extend(dirs::home_dir().map(|d| d.join(".spicetify")));
    dirs.iter().any(|dir| dir.is_dir())
}

fn mentions_spicetify(text: &str) -> bool {
    text.to_ascii_lowercase().contains("spicetify")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fixture, ORIGINAL_INDEX, write_spa};

    #[test]
    fn recognises_an_original_file() {
        let fx = Fixture::new();
        let found = inspect(&fx.install).unwrap();
        assert_eq!(found.state, UiState::Original);
        assert_eq!(found.sha256, Some(fx.spa_sha()));
        assert!(found.spicetify_leftovers.is_none());
    }

    #[test]
    fn recognises_spicetify_inside_the_archive() {
        let fx = Fixture::new();
        let html = ORIGINAL_INDEX.replace(
            "</body>",
            "<script src='helper/spicetifyWrapper.js'></script></body>",
        );
        write_spa(&fx.install.xpui_spa(), &html);
        assert_eq!(inspect(&fx.install).unwrap().state, UiState::Spicetify);
    }

    #[test]
    fn recognises_spicetify_unpacked_folder() {
        let fx = Fixture::new();
        fs::remove_file(fx.install.xpui_spa()).unwrap();
        fx.add_spicetify_folder();
        assert_eq!(inspect(&fx.install).unwrap().state, UiState::Spicetify);
    }

    #[test]
    fn reports_spicetify_leftovers_next_to_an_original_file() {
        let fx = Fixture::new();
        fx.add_spicetify_folder();
        let found = inspect(&fx.install).unwrap();
        assert_eq!(found.state, UiState::Original);
        assert_eq!(found.spicetify_leftovers, Some(fx.install.xpui_dir()));
    }

    #[test]
    fn reports_a_missing_interface() {
        let fx = Fixture::new();
        fs::remove_file(fx.install.xpui_spa()).unwrap();
        assert_eq!(inspect(&fx.install).unwrap().state, UiState::Missing);
    }

    #[test]
    fn rejects_a_file_that_is_not_an_archive() {
        let fx = Fixture::new();
        fs::write(fx.install.xpui_spa(), b"not a zip").unwrap();
        assert!(matches!(
            inspect(&fx.install),
            Err(Error::InvalidArchive { .. })
        ));
    }
}
