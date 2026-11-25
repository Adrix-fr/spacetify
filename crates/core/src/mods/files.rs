//! Reading mod files with limits, and copying a mod folder.

use std::fs;
use std::io::Read;
use std::path::Path;

use super::ModIssue;
use crate::{Error, Result};

/// Limits for a whole mod folder when installing it.
pub const MAX_FILES: usize = 500;
pub const MAX_TOTAL: u64 = 10 * 1024 * 1024;

pub fn read_bytes(dir: &Path, name: &str, max: u64) -> Result<Vec<u8>, ModIssue> {
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path).map_err(|_| ModIssue::MissingFile(name.into()))?;
    if meta.file_type().is_symlink() {
        return Err(ModIssue::Symlink(name.into()));
    }
    if !meta.is_file() {
        return Err(ModIssue::MissingFile(name.into()));
    }
    let too_large = || ModIssue::FileTooLarge {
        file: name.into(),
        max_kib: max / 1024,
    };
    if meta.len() > max {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| file.take(max + 1).read_to_end(&mut bytes))
        .map_err(|_| ModIssue::MissingFile(name.into()))?;
    if bytes.len() as u64 > max {
        return Err(too_large());
    }
    Ok(bytes)
}

pub fn read_text(dir: &Path, name: &str, max: u64) -> Result<String, ModIssue> {
    let bytes = read_bytes(dir, name, max)?;
    let text = String::from_utf8(bytes).map_err(|_| ModIssue::NotUtf8(name.into()))?;
    Ok(text
        .strip_prefix('\u{feff}')
        .map(str::to_owned)
        .unwrap_or(text))
}

/// Copies `src` into `dst` (which must not exist), skipping hidden entries
/// such as `.git`. Refuses symbolic links and oversized folders.
pub fn copy_mod_dir(src: &Path, dst: &Path) -> Result<std::result::Result<(), ModIssue>> {
    let mut files = 0;
    let mut total = 0;
    if let Err(issue) = check_dir(src, Path::new(""), &mut files, &mut total)? {
        return Ok(Err(issue));
    }
    copy_dir(src, dst)?;
    Ok(Ok(()))
}

fn check_dir(
    dir: &Path,
    relative: &Path,
    files: &mut usize,
    total: &mut u64,
) -> Result<std::result::Result<(), ModIssue>> {
    for entry in fs::read_dir(dir).map_err(Error::io(dir))? {
        let entry = entry.map_err(Error::io(dir))?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let shown = relative.join(&name);
        let meta = fs::symlink_metadata(&path).map_err(Error::io(&path))?;
        if meta.file_type().is_symlink() {
            return Ok(Err(ModIssue::Symlink(display(&shown))));
        }
        if meta.is_dir() {
            if let Err(issue) = check_dir(&path, &shown, files, total)? {
                return Ok(Err(issue));
            }
            continue;
        }
        *files += 1;
        *total += meta.len();
        if *files > MAX_FILES {
            return Ok(Err(ModIssue::TooManyFiles { max: MAX_FILES }));
        }
        if *total > MAX_TOTAL {
            return Ok(Err(ModIssue::FileTooLarge {
                file: display(Path::new(".")),
                max_kib: MAX_TOTAL / 1024,
            }));
        }
    }
    Ok(Ok(()))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(Error::io(dst))?;
    for entry in fs::read_dir(src).map_err(Error::io(src))? {
        let entry = entry.map_err(Error::io(src))?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().map_err(Error::io(&from))?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(Error::io(&from))?;
        }
    }
    Ok(())
}

fn display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_without_bom_and_enforces_limits() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.css"), "\u{feff}body{}").unwrap();
        assert_eq!(read_text(dir.path(), "a.css", 100).unwrap(), "body{}");
        assert_eq!(
            read_text(dir.path(), "a.css", 4).unwrap_err(),
            ModIssue::FileTooLarge {
                file: "a.css".into(),
                max_kib: 0
            }
        );
        assert_eq!(
            read_text(dir.path(), "missing.css", 100).unwrap_err(),
            ModIssue::MissingFile("missing.css".into())
        );
        fs::write(dir.path().join("bin.css"), [0xff, 0xfe, 0xfd]).unwrap();
        assert_eq!(
            read_text(dir.path(), "bin.css", 100).unwrap_err(),
            ModIssue::NotUtf8("bin.css".into())
        );
    }

    #[test]
    fn copies_without_hidden_entries() {
        let src = tempfile::tempdir().unwrap();
        fs::create_dir_all(src.path().join("assets")).unwrap();
        fs::create_dir_all(src.path().join(".git")).unwrap();
        fs::write(src.path().join("assets/a.png"), [1]).unwrap();
        fs::write(src.path().join(".git/HEAD"), "x").unwrap();
        fs::write(src.path().join("theme.toml"), "x").unwrap();

        let dst = tempfile::tempdir().unwrap();
        let target = dst.path().join("copy");
        copy_mod_dir(src.path(), &target).unwrap().unwrap();
        assert!(target.join("assets/a.png").is_file());
        assert!(target.join("theme.toml").is_file());
        assert!(!target.join(".git").exists());
    }
}
