//! The entry in Settings > Apps, for the current user.

use std::io;
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\spacetify";
/// Left by the first installer (made by Tauri's NSIS bundler).
const OLD_INSTALLER: &str = r"Software\Adrix-fr (spacetify project)";
const PUBLISHER: &str = "Adrix-fr (spacetify project)";
const HOMEPAGE: &str = "https://github.com/Adrix-fr/spacetify";

/// An installed copy, as Windows knows it.
#[derive(Debug, Clone)]
pub struct Installed {
    pub dir: PathBuf,
    pub version: Option<String>,
}

/// The installed copy, if Windows has one.
pub fn read() -> Option<Installed> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(UNINSTALL, KEY_READ)
        .ok()?;
    let location = key
        .get_value::<String, _>("InstallLocation")
        .ok()
        .map(|value| unquote(&value))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            // An entry without a location: the folder of its uninstaller.
            let command: String = key.get_value("UninstallString").ok()?;
            Path::new(&unquote(&command))
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
        })?;
    let version = key
        .get_value::<String, _>("DisplayVersion")
        .ok()
        .filter(|value| !value.trim().is_empty());
    Some(Installed {
        dir: PathBuf::from(location),
        version,
    })
}

/// Writes the entry for the copy in `dir`. `size` is in bytes.
pub fn write(dir: &Path, version: &str, size: u64) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // Starts clean: the first installer wrote values this one does not use.
    match hkcu.delete_subkey_all(UNINSTALL) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    let (key, _) = hkcu.create_subkey(UNINSTALL)?;
    let dir_text = dir.to_string_lossy().into_owned();
    let app = dir.join("spacetify-app.exe").to_string_lossy().into_owned();
    let uninstaller = dir.join("uninstall.exe").to_string_lossy().into_owned();
    key.set_value("DisplayName", &"spacetify")?;
    key.set_value("DisplayVersion", &version)?;
    key.set_value("DisplayIcon", &app)?;
    key.set_value("Publisher", &PUBLISHER)?;
    key.set_value("InstallLocation", &dir_text)?;
    key.set_value("UninstallString", &format!("\"{uninstaller}\""))?;
    key.set_value("URLInfoAbout", &HOMEPAGE)?;
    key.set_value("HelpLink", &HOMEPAGE)?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value(
        "EstimatedSize",
        &u32::try_from(size / 1024).unwrap_or(u32::MAX),
    )?;
    Ok(())
}

/// Removes the entry, and what the first installer left.
pub fn remove() -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for key in [UNINSTALL, OLD_INSTALLER] {
        match hkcu.delete_subkey_all(key) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
    }
    Ok(())
}

/// Removes only what the first installer left (on install).
pub fn remove_old_installer_key() {
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(OLD_INSTALLER);
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_owned()
}

#[cfg(test)]
mod tests {
    use super::unquote;

    #[test]
    fn reads_quoted_paths() {
        assert_eq!(
            unquote(r#""C:\Users\me\AppData\Local\spacetify""#),
            r"C:\Users\me\AppData\Local\spacetify"
        );
        assert_eq!(unquote(r"C:\plain"), r"C:\plain");
    }
}
