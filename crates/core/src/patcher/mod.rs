//! Applying and removing mods.
//!
//! The default method patches `xpui.spa` as little as possible: two tags in
//! `index.html` and a few files under `spacetify/`. Spotify's own code is never
//! rewritten, which is what usually breaks after an update. The patched file
//! is always rebuilt from the verified backup, so applying twice produces the
//! exact same bytes.
//!
//! [`live`] is the alternative that changes nothing on disk.

pub mod live;
mod payload;

pub use payload::Payload;

use std::fs::File;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::backup::{self, AppliedState, BackupRecord, BackupStore, OperationLock};
use crate::detect::{self, UiState};
use crate::spotify::{self, SpotifyInstall};
use crate::{Error, Result};

/// Folder inside the archive that holds everything we add.
pub const ASSET_DIR: &str = "spacetify/";
/// Presence of this entry means the archive was patched by us.
pub const MARKER_PATH: &str = "spacetify/marker.json";

const FORMAT: u32 = 1;
const BLOCK_START: &str = "<!-- spacetify:start -->";
const BLOCK_END: &str = "<!-- spacetify:end -->";

/// Stored in the patched archive so we can always find the original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub format: u32,
    pub tool_version: String,
    pub original_sha256: String,
}

impl Marker {
    fn new(original_sha256: &str) -> Self {
        Self {
            format: FORMAT,
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            original_sha256: original_sha256.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOutcome {
    Created(BackupRecord),
    AlreadyBackedUp(BackupRecord),
    /// Spotify is patched; this is the backup of its original interface.
    OriginalOfPatched(BackupRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied { new_backup: bool, sha256: String },
    AlreadyApplied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    Restored { sha256: String },
    AlreadyOriginal,
}

/// Backs up the original interface without changing anything.
pub fn backup(install: &SpotifyInstall, store: &BackupStore) -> Result<BackupOutcome> {
    install.ensure_modifiable()?;
    match detect::inspect(install)?.state {
        UiState::Original => {
            let (record, created) = store.create(&install.xpui_spa(), install.version.clone())?;
            Ok(if created {
                BackupOutcome::Created(record)
            } else {
                BackupOutcome::AlreadyBackedUp(record)
            })
        }
        UiState::Patched(marker) => Ok(BackupOutcome::OriginalOfPatched(original_backup(
            store, &marker,
        )?)),
        other => Err(refusal(install, other)),
    }
}

/// Backs up, then patches Spotify with `payload`. Refuses while Spotify is running.
pub fn apply(
    install: &SpotifyInstall,
    store: &BackupStore,
    payload: &Payload,
) -> Result<ApplyOutcome> {
    let _lock = OperationLock::acquire(store.root())?;
    ensure_closed()?;
    apply_unchecked(install, store, payload)
}

/// Puts back the original interface. Refuses while Spotify is running.
///
/// Also forgets that mods were applied, so the background service does not
/// put them back.
pub fn restore(install: &SpotifyInstall, store: &BackupStore) -> Result<RestoreOutcome> {
    let _lock = OperationLock::acquire(store.root())?;
    ensure_closed()?;
    restore_unchecked(install, store)
}

/// Does to the interface what a Spotify update does: puts the original file
/// back, but keeps the record that mods were applied. Used to test the
/// automatic repair on a real install. Returns the hash of the original.
pub fn simulate_update(install: &SpotifyInstall, store: &BackupStore) -> Result<String> {
    let _lock = OperationLock::acquire(store.root())?;
    ensure_closed()?;
    simulate_update_unchecked(install, store)
}

pub(crate) fn simulate_update_unchecked(
    install: &SpotifyInstall,
    store: &BackupStore,
) -> Result<String> {
    install.ensure_modifiable()?;
    match detect::inspect(install)?.state {
        UiState::Patched(marker) => {
            let original = original_backup(store, &marker)?;
            backup::copy_verified(
                &store.file_for(&original),
                &install.xpui_spa(),
                &original.sha256,
            )?;
            tracing::info!("simulated an update of {}", install.xpui_spa().display());
            Ok(original.sha256)
        }
        UiState::Original => Err(Error::NotPatched),
        other => Err(refusal(install, other)),
    }
}

pub(crate) fn apply_unchecked(
    install: &SpotifyInstall,
    store: &BackupStore,
    payload: &Payload,
) -> Result<ApplyOutcome> {
    install.ensure_modifiable()?;
    let spa = install.xpui_spa();
    let inspection = detect::inspect(install)?;

    let (original, new_backup) = match inspection.state {
        UiState::Original => store.create(&spa, install.version.clone())?,
        UiState::Patched(marker) => (original_backup(store, &marker)?, false),
        other => return Err(refusal(install, other)),
    };

    let patched = build_patched(
        &store.file_for(&original),
        &Marker::new(&original.sha256),
        payload,
    )?;
    let sha256 = backup::sha256_bytes(&patched);
    if inspection.sha256.as_deref() == Some(sha256.as_str()) {
        return Ok(ApplyOutcome::AlreadyApplied);
    }

    backup::write_atomic(&spa, &patched)?;
    backup::ensure_hash(&spa, &sha256)?;
    store.save_applied(&AppliedState {
        original_sha256: original.sha256,
        patched_sha256: sha256.clone(),
        spotify_version: install.version.clone(),
        applied_at: backup::now(),
    })?;
    tracing::info!("patched {}", spa.display());
    Ok(ApplyOutcome::Applied { new_backup, sha256 })
}

pub(crate) fn restore_unchecked(
    install: &SpotifyInstall,
    store: &BackupStore,
) -> Result<RestoreOutcome> {
    install.ensure_modifiable()?;
    match detect::inspect(install)?.state {
        UiState::Patched(marker) => {
            let original = original_backup(store, &marker)?;
            backup::copy_verified(
                &store.file_for(&original),
                &install.xpui_spa(),
                &original.sha256,
            )?;
            store.clear_applied()?;
            tracing::info!("restored {}", install.xpui_spa().display());
            Ok(RestoreOutcome::Restored {
                sha256: original.sha256,
            })
        }
        // Never "restore" over an original file: after an update it would downgrade Spotify.
        UiState::Original => {
            store.clear_applied()?;
            Ok(RestoreOutcome::AlreadyOriginal)
        }
        other => Err(refusal(install, other)),
    }
}

/// Builds a patched copy of the archive at `original`, in memory.
///
/// Entries are copied raw, in their original order, so only `index.html` and
/// our own files differ from the source.
pub fn build_patched(original: &Path, marker: &Marker, payload: &Payload) -> Result<Vec<u8>> {
    let zip_err = |err: zip::result::ZipError| Error::InvalidArchive {
        path: original.to_path_buf(),
        reason: err.to_string(),
    };
    let file = File::open(original).map_err(Error::io(original))?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let mut found_index = false;

    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(zip_err)?;
        let name = entry.name().map_err(zip_err)?.into_owned();
        if name.starts_with(ASSET_DIR) {
            continue;
        }
        if name != "index.html" {
            out.raw_copy_file(entry).map_err(zip_err)?;
            continue;
        }
        drop(entry);

        let mut html = String::new();
        archive
            .by_index(i)
            .map_err(zip_err)?
            .read_to_string(&mut html)
            .map_err(Error::io(original))?;
        out.start_file(name, file_options()).map_err(zip_err)?;
        out.write_all(inject_html(&html).as_bytes())
            .map_err(Error::io(original))?;
        found_index = true;
    }

    if !found_index {
        return Err(Error::InvalidArchive {
            path: original.to_path_buf(),
            reason: "index.html not found".into(),
        });
    }

    for (name, content) in payload.files(marker) {
        out.start_file(name, file_options()).map_err(zip_err)?;
        out.write_all(&content).map_err(Error::io(original))?;
    }
    Ok(out.finish().map_err(zip_err)?.into_inner())
}

/// Adds our loader block right before `</body>`, replacing any previous one.
pub fn inject_html(html: &str) -> String {
    let mut html = strip_block(html);
    let block = format!(
        "{BLOCK_START}<link rel=\"stylesheet\" href=\"/{ASSET_DIR}inject.css\">\
         <script defer src=\"/{ASSET_DIR}inject.js\"></script>{BLOCK_END}"
    );
    match html.rfind("</body>") {
        Some(at) => html.insert_str(at, &block),
        None => html.push_str(&block),
    }
    html
}

fn strip_block(html: &str) -> String {
    if let Some(start) = html.find(BLOCK_START)
        && let Some(len) = html[start..].find(BLOCK_END)
    {
        let end = start + len + BLOCK_END.len();
        return format!("{}{}", &html[..start], &html[end..]);
    }
    html.to_owned()
}

/// Fixed timestamps and permissions keep the output byte-for-byte reproducible.
fn file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644)
}

fn original_backup(store: &BackupStore, marker: &Marker) -> Result<BackupRecord> {
    let record = store
        .find(&marker.original_sha256)?
        .ok_or_else(|| Error::NoBackup {
            sha256: marker.original_sha256.clone(),
        })?;
    store.verify(&record)?;
    Ok(record)
}

fn ensure_closed() -> Result<()> {
    if spotify::is_running()? {
        Err(Error::SpotifyRunning)
    } else {
        Ok(())
    }
}

pub(crate) fn refusal(install: &SpotifyInstall, state: UiState) -> Error {
    match state {
        UiState::Spicetify => Error::SpicetifyActive,
        UiState::Missing => Error::XpuiMissing(install.xpui_spa()),
        UiState::Unknown(what) => Error::UnknownModification(what),
        UiState::Original | UiState::Patched(_) => unreachable!("not a refusal"),
    }
}

#[cfg(test)]
mod tests;
