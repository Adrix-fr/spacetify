//! Putting spacetify's files in place, all or nothing.
//!
//! Everything is first unpacked next to the old files (`<name>.spacetify-new`),
//! then each one is swapped in by renaming, the old one kept aside
//! (`<name>.spacetify-old`) until all are in place. If anything goes wrong,
//! the old files are put back as they were.

use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::failure::Failure;

/// Everything the installer puts at the top of the install folder. Nothing
/// else there is ever touched: the same folder holds the backups.
pub const ENTRIES: &[&str] = &[
    "spacetify-app.exe",
    "spacetify.exe",
    "spacetify-service.exe",
    "uninstall.exe",
    "icon.png",
    "examples",
];

const NEW: &str = ".spacetify-new";
const OLD: &str = ".spacetify-old";

/// Why unpacking stopped.
#[derive(Debug)]
pub enum Stop {
    Cancelled,
    Failed(Failure),
}

/// Files unpacked next to the installed ones, not swapped in yet.
#[derive(Debug)]
pub struct Staged {
    dir: PathBuf,
    entries: Vec<String>,
    /// Size of everything unpacked.
    pub bytes: u64,
}

/// Unpacks `archive` into `dir`, next to the files already there.
/// `progress` gets the share done, from 0 to 1.
pub fn extract(
    archive: &[u8],
    dir: &Path,
    mut progress: impl FnMut(f64),
    cancel: &AtomicBool,
) -> Result<Staged, Stop> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|err| Stop::Failed(Failure::other(format!("installer data: {err}"))))?;
    let total = (0..zip.len())
        .filter_map(|index| zip.by_index(index).ok().map(|file| file.size()))
        .sum::<u64>()
        .max(1);
    fs::create_dir_all(dir).map_err(|err| Stop::Failed(Failure::from_io(&err, dir)))?;
    clean_leftovers(dir);

    let mut staged = Staged {
        dir: dir.to_path_buf(),
        entries: Vec::new(),
        bytes: 0,
    };
    let mut buffer = vec![0; 256 * 1024];
    for index in 0..zip.len() {
        let result = (|| {
            let mut file = zip
                .by_index(index)
                .map_err(|err| Stop::Failed(Failure::other(format!("installer data: {err}"))))?;
            let name = file.enclosed_name().ok_or_else(|| {
                Stop::Failed(Failure::other(format!(
                    "unsafe file name: {}",
                    String::from_utf8_lossy(file.name_raw())
                )))
            })?;
            let mut parts = name.components();
            let top = parts
                .next()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_default();
            if !ENTRIES.contains(&top.as_str()) {
                return Err(Stop::Failed(Failure::other(format!(
                    "unexpected file in the installer: {top}"
                ))));
            }
            if !staged.entries.contains(&top) {
                staged.entries.push(top.clone());
            }
            let rest = parts.as_path();
            let mut target = dir.join(format!("{top}{NEW}"));
            if !rest.as_os_str().is_empty() {
                target = target.join(rest);
            }
            if file.is_dir() {
                return fs::create_dir_all(&target)
                    .map_err(|err| Stop::Failed(Failure::from_io(&err, &target)));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| Stop::Failed(Failure::from_io(&err, parent)))?;
            }
            let mut out = File::create(&target)
                .map_err(|err| Stop::Failed(Failure::from_io(&err, &target)))?;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Stop::Cancelled);
                }
                let read = file.read(&mut buffer).map_err(|err| {
                    Stop::Failed(Failure::other(format!("installer data: {err}")))
                })?;
                if read == 0 {
                    break;
                }
                out.write_all(&buffer[..read])
                    .map_err(|err| Stop::Failed(Failure::from_io(&err, &target)))?;
                staged.bytes += read as u64;
                progress(staged.bytes as f64 / total as f64);
            }
            Ok(())
        })();
        if let Err(stop) = result {
            staged.discard();
            return Err(stop);
        }
    }
    Ok(staged)
}

impl Staged {
    /// Swaps the unpacked files in. On failure, the old files are back.
    pub fn swap(&self) -> Result<(), Failure> {
        let mut done: Vec<(&str, bool)> = Vec::new();
        for top in &self.entries {
            let target = self.dir.join(top);
            let old = self.dir.join(format!("{top}{OLD}"));
            let new = self.dir.join(format!("{top}{NEW}"));
            let had = target.symlink_metadata().is_ok();
            if had && let Err(err) = fs::rename(&target, &old) {
                self.rollback(&done);
                return Err(Failure::from_io(&err, &target));
            }
            if let Err(err) = fs::rename(&new, &target) {
                if had {
                    let _ = fs::rename(&old, &target);
                }
                self.rollback(&done);
                return Err(Failure::from_io(&err, &target));
            }
            done.push((top, had));
        }
        Ok(())
    }

    fn rollback(&self, done: &[(&str, bool)]) {
        for &(top, had) in done.iter().rev() {
            let target = self.dir.join(top);
            let _ = remove(&target);
            if had {
                let _ = fs::rename(self.dir.join(format!("{top}{OLD}")), &target);
            }
        }
    }

    /// Removes the old files once the new ones are in place. A file still
    /// open somewhere stays until the next install.
    pub fn finish(&self) {
        for top in &self.entries {
            let _ = remove(&self.dir.join(format!("{top}{OLD}")));
        }
    }

    /// Removes what was unpacked and not swapped in.
    pub fn discard(&self) {
        for top in &self.entries {
            let _ = remove(&self.dir.join(format!("{top}{NEW}")));
        }
    }
}

/// Removes spacetify's files from `dir`, and nothing else.
pub fn remove_installed(dir: &Path) -> Result<(), Failure> {
    let mut first = None;
    for entry in ENTRIES {
        let path = dir.join(entry);
        if let Err(err) = remove(&path) {
            first.get_or_insert(Failure::from_io(&err, &path));
        }
    }
    clean_leftovers(dir);
    first.map_or(Ok(()), Err)
}

/// Size of spacetify's files in `dir`, in bytes.
pub fn installed_size(dir: &Path) -> u64 {
    fn size(path: &Path) -> u64 {
        match path.symlink_metadata() {
            Ok(meta) if meta.is_dir() => fs::read_dir(path)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .map(|entry| size(&entry.path()))
                        .sum()
                })
                .unwrap_or(0),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        }
    }
    ENTRIES.iter().map(|entry| size(&dir.join(entry))).sum()
}

/// Removes `dir` if nothing is left in it.
pub fn remove_dir_if_empty(dir: &Path) {
    let _ = fs::remove_dir(dir);
}

/// Leftovers of an install that stopped half way.
fn clean_leftovers(dir: &Path) {
    for entry in ENTRIES {
        for suffix in [NEW, OLD] {
            let _ = remove(&dir.join(format!("{entry}{suffix}")));
        }
    }
}

/// Removes a file or a folder; one that is not there is fine.
fn remove(path: &Path) -> io::Result<()> {
    let result = match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) => Err(err),
    };
    match result {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn installs_then_updates_without_touching_other_files() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("spacetify");
        fs::create_dir_all(dir.join("backups")).unwrap();
        fs::write(dir.join("backups/old.zip"), "mine").unwrap();
        let no = AtomicBool::new(false);

        let first = archive(&[
            ("spacetify.exe", "v1"),
            ("examples/themes/a.toml", "a1"),
            ("examples/themes/gone.toml", "x"),
        ]);
        let mut seen = 0.0;
        let staged = extract(&first, &dir, |share| seen = share, &no).unwrap();
        assert!((seen - 1.0).abs() < f64::EPSILON);
        staged.swap().unwrap();
        staged.finish();
        assert_eq!(read(&dir.join("spacetify.exe")), "v1");
        assert_eq!(read(&dir.join("examples/themes/a.toml")), "a1");

        let second = archive(&[("spacetify.exe", "v2"), ("examples/themes/a.toml", "a2")]);
        let staged = extract(&second, &dir, |_| {}, &no).unwrap();
        staged.swap().unwrap();
        staged.finish();
        assert_eq!(read(&dir.join("spacetify.exe")), "v2");
        assert_eq!(read(&dir.join("examples/themes/a.toml")), "a2");
        assert!(!dir.join("examples/themes/gone.toml").exists());
        assert_eq!(read(&dir.join("backups/old.zip")), "mine");
        assert_eq!(names(&dir), ["backups", "examples", "spacetify.exe"]);

        remove_installed(&dir).unwrap();
        assert_eq!(names(&dir), ["backups"]);
    }

    #[test]
    fn a_failed_swap_puts_the_old_files_back() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().to_path_buf();
        fs::write(dir.join("spacetify.exe"), "old").unwrap();
        fs::write(dir.join("icon.png"), "old icon").unwrap();
        let no = AtomicBool::new(false);

        let staged = extract(
            &archive(&[("spacetify.exe", "new"), ("icon.png", "new icon")]),
            &dir,
            |_| {},
            &no,
        )
        .unwrap();
        // The second file cannot be swapped in: its new copy disappeared.
        fs::remove_file(dir.join("icon.png.spacetify-new")).unwrap();
        assert!(staged.swap().is_err());
        staged.discard();
        assert_eq!(read(&dir.join("spacetify.exe")), "old");
        assert_eq!(read(&dir.join("icon.png")), "old icon");
        assert_eq!(names(&dir), ["icon.png", "spacetify.exe"]);
    }

    #[test]
    fn cancelling_or_odd_files_leave_nothing_behind() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().to_path_buf();
        let yes = AtomicBool::new(true);
        let result = extract(&archive(&[("spacetify.exe", "x")]), &dir, |_| {}, &yes);
        assert!(matches!(result, Err(Stop::Cancelled)));
        assert!(names(&dir).is_empty());

        let no = AtomicBool::new(false);
        let result = extract(
            &archive(&[("spacetify.exe", "x"), ("other.exe", "y")]),
            &dir,
            |_| {},
            &no,
        );
        assert!(matches!(result, Err(Stop::Failed(Failure::Other { .. }))));
        assert!(names(&dir).is_empty());
    }
}
