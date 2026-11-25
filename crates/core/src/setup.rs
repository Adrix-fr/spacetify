//! What the Windows installer of the desktop app sets up for the current
//! user, and takes back on uninstall. Nothing here needs admin rights:
//!
//! - the install folder in the user's `Path` (`HKCU\Environment`), so
//!   `spacetify` works in any new terminal;
//! - a name for notifications (`HKCU\Software\Classes\AppUserModelId\…`), so
//!   they show as "spacetify" and not as "Windows PowerShell";
//! - the startup entry of the service, moved to the installed copy when it
//!   pointed somewhere else (an older build, `target\release`…).
//!
//! The installer (`apps/setup`) runs each step on its own, so it can show
//! which one failed and run it again: [`add_to_path`],
//! [`register_notifications`] and [`setup_autostart`]. [`after_install`]
//! runs all three, for `spacetify installer post-install`; on uninstall,
//! [`before_uninstall`] takes everything back. On other systems they do
//! nothing.

use std::path::Path;

use crate::service::{self, ServicePaths};
use crate::{Lang, Result, tr};

/// Identifier of the desktop app, also used as the notification name on
/// Windows and the LaunchAgent label on macOS.
pub const APP_ID: &str = "io.github.adrix-fr.spacetify";

/// Name shown on notifications.
pub const DISPLAY_NAME: &str = "spacetify";

/// One line per thing done, for the installer's details view.
pub type Steps = Vec<String>;

/// Sets everything up for the copy installed in `dir`.
pub fn after_install(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
    let lang = Lang::detect();
    let mut steps = Steps::new();
    if add_to_path(dir)? {
        steps.push(tr!(
            lang,
            "Added {} to the user Path.",
            "{} ajouté au Path de l'utilisateur.",
            dir.display()
        ));
    }
    if register_notifications(dir)? {
        steps.push(tr!(
            lang,
            "Notifications will show as \"spacetify\".",
            "Les notifications s'afficheront au nom de « spacetify »."
        ));
    }
    steps.extend(setup_autostart(dir, paths)?);
    Ok(steps)
}

/// Adds `dir` to the user `Path`, so `spacetify` works in any new terminal.
/// Returns whether the `Path` changed. Terminals opened from the Start menu
/// only see it once Windows is told (the installer does that).
pub fn add_to_path(dir: &Path) -> Result<bool> {
    platform::add_to_path(dir)
}

/// Gives notifications spacetify's own name and icon (`icon.png` in `dir`).
/// Returns whether that is done (never on other systems).
pub fn register_notifications(dir: &Path) -> Result<bool> {
    platform::register_notifications(dir)
}

/// Points the startup entry of the service to the copy in `dir`, turns it
/// back on after a quick reinstall, and starts the new service when the
/// automatic repair is on.
pub fn setup_autostart(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
    platform::setup_autostart(dir, paths)
}

/// Takes back what [`after_install`] did, and stops the service, so the
/// files can be deleted. Mods, settings and backups are left alone.
pub fn before_uninstall(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
    platform::before_uninstall(dir, paths)
}

/// Whether notifications can use spacetify's own name.
pub fn app_id_registered() -> bool {
    platform::app_id_registered()
}

/// Whether `command` (a startup command line) runs a program from `dir`.
pub fn command_in_dir(command: &str, dir: &Path) -> bool {
    let program = command
        .trim_start()
        .strip_prefix('"')
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| command.split_whitespace().next().unwrap_or_default())
        .replace('/', "\\");
    // Windows paths, whatever system the tests run on.
    match program.rsplit_once('\\') {
        Some((folder, _)) => same_folder(Path::new(folder), dir),
        None => false,
    }
}

/// Compares folders the way Windows does: case and trailing separators do
/// not matter.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    let clean = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    clean(a) == clean(b)
}

/// The user `Path` with `dir` added at the end, or `None` if it is already there.
pub fn path_with(current: &str, dir: &Path) -> Option<String> {
    if path_entries(current).any(|entry| same_folder(Path::new(entry), dir)) {
        return None;
    }
    let dir = dir.to_string_lossy();
    let current = current.trim_end_matches(';');
    Some(if current.is_empty() {
        dir.into_owned()
    } else {
        format!("{current};{dir}")
    })
}

/// The user `Path` without `dir`, or `None` if it was not there.
pub fn path_without(current: &str, dir: &Path) -> Option<String> {
    let kept: Vec<&str> = path_entries(current)
        .filter(|entry| !same_folder(Path::new(entry), dir))
        .collect();
    (kept.len() != path_entries(current).count()).then(|| kept.join(";"))
}

fn path_entries(path: &str) -> impl Iterator<Item = &str> {
    path.split(';').filter(|entry| !entry.trim().is_empty())
}

/// A reinstall within this delay keeps the automatic repair on.
const REINSTALL_WINDOW: std::time::Duration = std::time::Duration::from_secs(30 * 60);

fn reinstall_note(paths: &ServicePaths) -> std::path::PathBuf {
    paths.dir.join("autostart-before-uninstall")
}

/// Remembers that the automatic repair was on when spacetify was removed.
#[cfg_attr(not(windows), allow(dead_code))]
fn leave_reinstall_note(paths: &ServicePaths) {
    let _ = std::fs::create_dir_all(&paths.dir);
    let _ = std::fs::write(reinstall_note(paths), b"on");
}

/// Whether spacetify was removed with the automatic repair on, moments ago
/// (the installer removes the older copy before installing a new one).
/// The note is deleted either way.
#[cfg_attr(not(windows), allow(dead_code))]
fn take_reinstall_note(paths: &ServicePaths) -> bool {
    let path = reinstall_note(paths);
    let recent = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .is_some_and(|age| age < REINSTALL_WINDOW);
    let _ = std::fs::remove_file(&path);
    recent
}

fn stop_service(paths: &ServicePaths, lang: crate::Lang, steps: &mut Steps) -> Result<()> {
    if service::stop(paths)? {
        steps.push(tr!(lang, "Service stopped.", "Service arrêté."));
    }
    Ok(())
}

#[cfg(windows)]
mod platform {
    use std::io::ErrorKind;
    use std::path::Path;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, RegType};
    use winreg::{RegKey, RegValue};

    use super::{APP_ID, DISPLAY_NAME, Steps, path_with, path_without, stop_service};
    use crate::service::autostart::{self, Autostart};
    use crate::service::{self, ServicePaths};
    use crate::{Error, Lang, Result, tr};

    const ENVIRONMENT: &str = "Environment";
    const APP_IDS: &str = r"Software\Classes\AppUserModelId";

    fn failed(err: impl std::fmt::Display) -> Error {
        Error::Setup(err.to_string())
    }

    pub fn add_to_path(dir: &Path) -> Result<bool> {
        Keys::user().add_to_path(dir)
    }

    pub fn register_notifications(dir: &Path) -> Result<bool> {
        Keys::user()
            .register_app_id(&dir.join("icon.png"))
            .map(|()| true)
    }

    pub fn setup_autostart(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
        let lang = Lang::detect();
        let mut steps = Steps::new();
        let exe = dir.join(service::EXECUTABLE);
        if super::take_reinstall_note(paths) && matches!(autostart::status()?, Autostart::Off) {
            // The installer removed the older copy first (which also removes
            // the startup entry): turn it back on as it was.
            autostart::enable(&exe, &[])?;
            steps.push(tr!(
                lang,
                "Automatic repair turned back on.",
                "Réparation automatique réactivée."
            ));
        }
        match autostart::status()? {
            Autostart::On { command } if !super::command_in_dir(&command, dir) => {
                stop_service(paths, lang, &mut steps)?;
                autostart::enable(&exe, &[])?;
                service::start(paths, &exe, &[])?;
                steps.push(tr!(
                    lang,
                    "Automatic repair now runs from {}.",
                    "La réparation automatique tourne maintenant depuis {}.",
                    dir.display()
                ));
            }
            Autostart::On { .. } => {
                // Same folder: an update of spacetify. The installer stopped the
                // service to replace its file; start the new one.
                service::start(paths, &exe, &[])?;
            }
            Autostart::DisabledBySystem { command } if !super::command_in_dir(&command, dir) => {
                // Turned off by the user in the Task Manager: point it to the
                // installed copy, but leave it off.
                autostart::retarget(&exe, &[])?;
            }
            Autostart::DisabledBySystem { .. } | Autostart::Off => {}
        }
        Ok(steps)
    }

    pub fn before_uninstall(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
        let lang = Lang::detect();
        let mut steps = Steps::new();
        let keys = Keys::user();

        stop_service(paths, lang, &mut steps)?;
        let (ours, on) = match autostart::status()? {
            Autostart::On { command } => (super::command_in_dir(&command, dir), true),
            Autostart::DisabledBySystem { command } => {
                (super::command_in_dir(&command, dir), false)
            }
            Autostart::Off => (false, false),
        };
        if ours && on {
            // In case this is a reinstall, so the next install turns it back on.
            super::leave_reinstall_note(paths);
        }
        if ours {
            autostart::disable()?;
            steps.push(tr!(
                lang,
                "Removed from the programs that start with Windows.",
                "Retiré des programmes qui démarrent avec Windows."
            ));
        }
        if keys.remove_from_path(dir)? {
            steps.push(tr!(
                lang,
                "Removed {} from the user Path.",
                "{} retiré du Path de l'utilisateur.",
                dir.display()
            ));
        }
        keys.unregister_app_id()?;
        Ok(steps)
    }

    pub fn app_id_registered() -> bool {
        Keys::user().app_id_registered()
    }

    /// Registry locations, movable for tests.
    pub struct Keys {
        environment: String,
        app_id: String,
    }

    impl Keys {
        pub fn user() -> Self {
            Self {
                environment: ENVIRONMENT.to_owned(),
                app_id: format!(r"{APP_IDS}\{APP_ID}"),
            }
        }

        /// Everything under `base`, for tests.
        #[cfg(test)]
        pub fn at(base: &str) -> Self {
            Self {
                environment: format!(r"{base}\Environment"),
                app_id: format!(r"{base}\AppUserModelId\{APP_ID}"),
            }
        }

        /// The user `Path`, and whether it is stored as an expandable string.
        fn read_path(&self) -> Result<(String, bool)> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let value = match hkcu
                .open_subkey_with_flags(&self.environment, KEY_READ)
                .and_then(|key| key.get_raw_value("Path"))
            {
                Ok(value) => value,
                Err(err) if err.kind() == ErrorKind::NotFound => return Ok((String::new(), true)),
                Err(err) => return Err(failed(err)),
            };
            let expand = value.vtype == RegType::REG_EXPAND_SZ;
            let units: Vec<u16> = value
                .bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .take_while(|&unit| unit != 0)
                .collect();
            Ok((String::from_utf16_lossy(&units), expand))
        }

        fn write_path(&self, path: &str, expand: bool) -> Result<()> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(&self.environment).map_err(failed)?;
            let bytes: Vec<u8> = path
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            let value = RegValue {
                bytes: bytes.into(),
                // Keeps `%USERPROFILE%`-style entries working.
                vtype: if expand {
                    RegType::REG_EXPAND_SZ
                } else {
                    RegType::REG_SZ
                },
            };
            key.set_raw_value("Path", &value).map_err(failed)
        }

        /// Returns whether the `Path` changed.
        pub fn add_to_path(&self, dir: &Path) -> Result<bool> {
            let (current, expand) = self.read_path()?;
            match path_with(&current, dir) {
                Some(path) => self.write_path(&path, expand).map(|()| true),
                None => Ok(false),
            }
        }

        /// Returns whether the `Path` changed.
        pub fn remove_from_path(&self, dir: &Path) -> Result<bool> {
            let (current, expand) = self.read_path()?;
            match path_without(&current, dir) {
                Some(path) => self.write_path(&path, expand).map(|()| true),
                None => Ok(false),
            }
        }

        pub fn register_app_id(&self, icon: &Path) -> Result<()> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(&self.app_id).map_err(failed)?;
            key.set_value("DisplayName", &DISPLAY_NAME)
                .map_err(failed)?;
            if icon.is_file() {
                key.set_value("IconUri", &icon.to_string_lossy().into_owned())
                    .map_err(failed)?;
            }
            Ok(())
        }

        pub fn unregister_app_id(&self) -> Result<()> {
            match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.app_id) {
                Err(err) if err.kind() != ErrorKind::NotFound => Err(failed(err)),
                _ => Ok(()),
            }
        }

        pub fn app_id_registered(&self) -> bool {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(&self.app_id, KEY_READ)
                .and_then(|key| key.get_value::<String, _>("DisplayName"))
                .is_ok()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct TestKey(String);

        impl Drop for TestKey {
            fn drop(&mut self) {
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
            }
        }

        #[test]
        fn path_and_app_id_in_a_test_key() {
            let base = TestKey(format!(
                r"Software\spacetify-tests\setup-{}",
                std::process::id()
            ));
            let keys = Keys::at(&base.0);
            let dir = Path::new(r"C:\Users\me\AppData\Local\spacetify");

            assert!(keys.add_to_path(dir).unwrap());
            assert!(!keys.add_to_path(dir).unwrap());
            assert_eq!(keys.read_path().unwrap().0, dir.to_string_lossy());

            keys.write_path(r"%USERPROFILE%\bin;C:\Tools", true)
                .unwrap();
            assert!(keys.add_to_path(dir).unwrap());
            let (path, expand) = keys.read_path().unwrap();
            assert!(expand);
            assert_eq!(
                path,
                r"%USERPROFILE%\bin;C:\Tools;C:\Users\me\AppData\Local\spacetify"
            );
            assert!(
                keys.remove_from_path(Path::new(r"c:\users\me\appdata\local\SPACETIFY\"))
                    .unwrap()
            );
            assert_eq!(keys.read_path().unwrap().0, r"%USERPROFILE%\bin;C:\Tools");
            assert!(!keys.remove_from_path(dir).unwrap());

            assert!(!keys.app_id_registered());
            keys.register_app_id(Path::new("missing.png")).unwrap();
            assert!(keys.app_id_registered());
            keys.unregister_app_id().unwrap();
            keys.unregister_app_id().unwrap();
            assert!(!keys.app_id_registered());
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    use super::Steps;
    use crate::Result;
    use crate::service::ServicePaths;

    pub fn add_to_path(_dir: &Path) -> Result<bool> {
        Ok(false)
    }

    pub fn register_notifications(_dir: &Path) -> Result<bool> {
        Ok(false)
    }

    pub fn setup_autostart(_dir: &Path, _paths: &ServicePaths) -> Result<Steps> {
        Ok(Steps::new())
    }

    pub fn before_uninstall(dir: &Path, paths: &ServicePaths) -> Result<Steps> {
        let mut steps = Steps::new();
        super::stop_service(paths, crate::Lang::detect(), &mut steps)?;
        let _ = dir;
        Ok(steps)
    }

    pub fn app_id_registered() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn edits_the_path_once() {
        let dir = Path::new(r"C:\Apps\spacetify");
        assert_eq!(path_with("", dir).as_deref(), Some(r"C:\Apps\spacetify"));
        assert_eq!(
            path_with(r"C:\A;C:\B;", dir).as_deref(),
            Some(r"C:\A;C:\B;C:\Apps\spacetify")
        );
        assert_eq!(path_with(r"C:\A;c:\apps\SPACETIFY\", dir), None);
        assert_eq!(
            path_without(r"C:\A;C:\Apps\spacetify;C:\B", dir).as_deref(),
            Some(r"C:\A;C:\B")
        );
        assert_eq!(path_without(r"C:\A;C:\B", dir), None);
    }

    #[test]
    fn a_quick_reinstall_remembers_the_automatic_repair() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ServicePaths::new(&crate::config::AppDirs::in_folder(dir.path()));
        assert!(!take_reinstall_note(&paths));
        leave_reinstall_note(&paths);
        assert!(take_reinstall_note(&paths));
        assert!(!take_reinstall_note(&paths), "used once");
    }

    #[test]
    fn tells_where_a_startup_command_runs_from() {
        let dir = Path::new(r"C:\Users\me\AppData\Local\spacetify");
        assert!(command_in_dir(
            r#""C:\Users\me\AppData\Local\spacetify\spacetify-service.exe""#,
            dir
        ));
        assert!(command_in_dir(
            r#""c:\users\me\appdata\local\spacetify\spacetify-service.exe" "--home" "D:\x""#,
            dir
        ));
        assert!(!command_in_dir(
            r#""C:\Users\me\Desktop\spacetify\target\release\spacetify-service.exe""#,
            dir
        ));
        assert!(!command_in_dir("", dir));
    }
}
