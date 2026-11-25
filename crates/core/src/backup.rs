//! Verified backups of `xpui.spa`.
//!
//! Each backup sits in its own folder named after its hash:
//!
//! ```text
//! backups/
//!   1cfae00b62fb3bdc/
//!     xpui.spa
//!     backup.toml
//!   applied.toml
//! ```
//!
//! A backup is trusted only while its hash still matches `backup.toml`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{APP_NAME, Error, Result};

const SPA_FILE: &str = "xpui.spa";
const RECORD_FILE: &str = "backup.toml";
const APPLIED_FILE: &str = "applied.toml";

/// Describes one backed up `xpui.spa`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRecord {
    pub sha256: String,
    pub size: u64,
    pub spotify_version: Option<String>,
    pub source: PathBuf,
    pub created_at: String,
}

/// What the last successful `apply` wrote.
///
/// If Spotify's interface is back to an original file while this exists,
/// an update replaced our patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedState {
    pub original_sha256: String,
    pub patched_sha256: String,
    pub spotify_version: Option<String>,
    pub applied_at: String,
}

#[derive(Debug, Clone)]
pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path of the backed up archive for `record`.
    pub fn file_for(&self, record: &BackupRecord) -> PathBuf {
        self.entry_dir(&record.sha256).join(SPA_FILE)
    }

    /// Backs up `spa` unless an intact backup with the same hash exists.
    ///
    /// Returns the record and whether a new copy was made.
    pub fn create(
        &self,
        spa: &Path,
        spotify_version: Option<String>,
    ) -> Result<(BackupRecord, bool)> {
        let sha256 = sha256_file(spa)?;
        if let Some(existing) = self.find(&sha256)? {
            if self.verify(&existing).is_ok() {
                return Ok((existing, false));
            }
            tracing::warn!("backup {sha256} is damaged, making a new copy");
        }

        let dir = self.entry_dir(&sha256);
        fs::create_dir_all(&dir).map_err(Error::io(&dir))?;
        let target = dir.join(SPA_FILE);
        copy_verified(spa, &target, &sha256)?;

        let record = BackupRecord {
            size: fs::metadata(&target).map_err(Error::io(&target))?.len(),
            sha256,
            spotify_version,
            source: spa.to_path_buf(),
            created_at: now(),
        };
        write_toml(&dir.join(RECORD_FILE), &record)?;
        tracing::info!("backed up {} to {}", spa.display(), target.display());
        Ok((record, true))
    }

    /// Looks up a backup by the hash of the original file.
    pub fn find(&self, sha256: &str) -> Result<Option<BackupRecord>> {
        let path = self.entry_dir(sha256).join(RECORD_FILE);
        let record: Option<BackupRecord> = read_toml(&path)?;
        Ok(record.filter(|record| record.sha256 == sha256))
    }

    /// Fails if the backed up file no longer matches its recorded hash.
    pub fn verify(&self, record: &BackupRecord) -> Result<()> {
        let path = self.file_for(record);
        match sha256_file(&path) {
            Ok(actual) if actual == record.sha256 => Ok(()),
            _ => Err(Error::BackupCorrupted(path)),
        }
    }

    /// All readable backups, oldest first.
    pub fn list(&self) -> Result<Vec<BackupRecord>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(Error::io(&self.root)(err)),
        };
        let mut records = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path().join(RECORD_FILE);
            match read_toml::<BackupRecord>(&path) {
                Ok(Some(record)) => records.push(record),
                Ok(None) => {}
                Err(err) => tracing::warn!("skipping unreadable backup: {err}"),
            }
        }
        records.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(records)
    }

    pub fn load_applied(&self) -> Result<Option<AppliedState>> {
        read_toml(&self.root.join(APPLIED_FILE))
    }

    pub fn save_applied(&self, state: &AppliedState) -> Result<()> {
        fs::create_dir_all(&self.root).map_err(Error::io(&self.root))?;
        write_toml(&self.root.join(APPLIED_FILE), state)
    }

    pub fn clear_applied(&self) -> Result<()> {
        let path = self.root.join(APPLIED_FILE);
        match fs::remove_file(&path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(Error::io(&path)(err)),
            _ => Ok(()),
        }
    }

    fn entry_dir(&self, sha256: &str) -> PathBuf {
        self.root.join(&sha256[..sha256.len().min(16)])
    }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(Error::io(path))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(Error::io(path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Replaces `target` with `bytes` through a temporary file in the same folder,
/// so a crash never leaves a half-written file behind.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = tmp_path(target);
    let result = File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, target));
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(target)(err));
    }
    Ok(())
}

/// Copies `src` over `dst` and checks the result against `expected_sha256`.
/// On mismatch `dst` is left untouched.
pub fn copy_verified(src: &Path, dst: &Path, expected_sha256: &str) -> Result<()> {
    let tmp = tmp_path(dst);
    fs::copy(src, &tmp).map_err(Error::io(src))?;

    let actual = sha256_file(&tmp)?;
    if actual != expected_sha256 {
        let _ = fs::remove_file(&tmp);
        return Err(Error::HashMismatch {
            path: dst.to_path_buf(),
            expected: expected_sha256.to_owned(),
            actual,
        });
    }
    fs::rename(&tmp, dst).map_err(Error::io(dst))
}

/// Errors with [`Error::HashMismatch`] unless `path` hashes to `expected`.
pub fn ensure_hash(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::HashMismatch {
            path: path.to_path_buf(),
            expected: expected.to_owned(),
            actual,
        })
    }
}

pub(crate) fn now() -> String {
    humantime::format_rfc3339_seconds(SystemTime::now()).to_string()
}

fn tmp_path(target: &Path) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    target.with_file_name(format!("{name}.{APP_NAME}-tmp"))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(Error::io(path)(err)),
    };
    toml::from_str(&text)
        .map(Some)
        .map_err(|err| Error::Config {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })
}

fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value).map_err(|err| Error::Config {
        path: path.to_path_buf(),
        reason: err.to_string(),
    })?;
    write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_value() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn creating_twice_reuses_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let spa = dir.path().join("xpui.spa");
        fs::write(&spa, b"original").unwrap();
        let store = BackupStore::new(dir.path().join("backups"));

        let (first, created) = store.create(&spa, Some("1.2.3".into())).unwrap();
        assert!(created);
        let (second, created) = store.create(&spa, None).unwrap();
        assert!(!created);
        assert_eq!(first, second);
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(fs::read(store.file_for(&first)).unwrap(), b"original");
    }

    #[test]
    fn detects_a_damaged_backup() {
        let dir = tempfile::tempdir().unwrap();
        let spa = dir.path().join("xpui.spa");
        fs::write(&spa, b"original").unwrap();
        let store = BackupStore::new(dir.path().join("backups"));
        let (record, _) = store.create(&spa, None).unwrap();

        fs::write(store.file_for(&record), b"tampered").unwrap();
        assert!(matches!(
            store.verify(&record),
            Err(Error::BackupCorrupted(_))
        ));

        // A new backup of the same file repairs it.
        let (_, created) = store.create(&spa, None).unwrap();
        assert!(created);
        store.verify(&record).unwrap();
    }

    #[test]
    fn copy_verified_refuses_wrong_hash() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::write(&src, b"new").unwrap();
        fs::write(&dst, b"old").unwrap();

        let err = copy_verified(&src, &dst, &sha256_bytes(b"something else")).unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }));
        assert_eq!(fs::read(&dst).unwrap(), b"old");
        assert!(!tmp_path(&dst).exists());
    }
}
