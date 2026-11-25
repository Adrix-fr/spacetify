//! Starting the service when the user logs in, without admin rights.
//!
//! - Windows: a `spacetify` value in `HKCU\…\CurrentVersion\Run`, the same
//!   place the Task Manager's "Startup apps" page reads.
//! - macOS: a LaunchAgent in `~/Library/LaunchAgents`.
//! - Linux: an XDG autostart entry in `~/.config/autostart`.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Name of the entry on every system.
pub const ENTRY_NAME: &str = "spacetify";
const MAC_LABEL: &str = "io.github.adrix-fr.spacetify";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Autostart {
    Off,
    /// `command` is what the system runs at login.
    On {
        command: String,
    },
    /// Registered, but turned off by the user in the system settings
    /// (the Task Manager on Windows).
    DisabledBySystem {
        command: String,
    },
}

/// What is registered for the current user.
pub fn status() -> Result<Autostart> {
    backend().status()
}

/// Starts `exe args` at every login. Replaces an older entry.
pub fn enable(exe: &Path, args: &[String]) -> Result<()> {
    backend().enable(exe, args)
}

pub fn disable() -> Result<()> {
    backend().disable()
}

/// Points an existing entry to `exe args`, without turning it back on if
/// the user turned it off in the system settings. Does nothing without an
/// entry.
pub fn retarget(exe: &Path, args: &[String]) -> Result<()> {
    match status()? {
        Autostart::Off => Ok(()),
        Autostart::On { .. } => enable(exe, args),
        #[cfg(windows)]
        Autostart::DisabledBySystem { .. } => backend().set_command(exe, args),
        #[cfg(not(windows))]
        Autostart::DisabledBySystem { .. } => Ok(()),
    }
}

fn failed(err: impl std::fmt::Display) -> Error {
    Error::Autostart(err.to_string())
}

#[cfg(windows)]
fn backend() -> windows::RunKey {
    windows::RunKey::user()
}

#[cfg(target_os = "macos")]
fn backend() -> EntryFile {
    let dir = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents");
    EntryFile::new(dir.join(format!("{MAC_LABEL}.plist")), Format::LaunchAgent)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn backend() -> EntryFile {
    let dir = dirs::config_dir().unwrap_or_default().join("autostart");
    EntryFile::new(
        dir.join(format!("{ENTRY_NAME}-service.desktop")),
        Format::Desktop,
    )
}

#[cfg(not(any(windows, unix)))]
fn backend() -> EntryFile {
    EntryFile::new(PathBuf::new(), Format::Desktop)
}

/// Quotes an argument for a Windows command line or an `Exec=` line.
fn quote(arg: &str) -> String {
    format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
}

fn command_line(exe: &Path, args: &[String], quote: fn(&str) -> String) -> String {
    std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|arg| quote(&arg))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
mod windows {
    use std::io::ErrorKind;
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    use super::{Autostart, ENTRY_NAME, command_line, failed};
    use crate::Result;

    pub struct RunKey {
        run: String,
        approved: String,
    }

    impl RunKey {
        pub fn user() -> Self {
            Self::at(r"Software\Microsoft\Windows\CurrentVersion")
        }

        /// `base` replaces `Software\Microsoft\Windows\CurrentVersion`, for tests.
        pub fn at(base: &str) -> Self {
            Self {
                run: format!(r"{base}\Run"),
                approved: format!(r"{base}\Explorer\StartupApproved\Run"),
            }
        }

        pub fn status(&self) -> Result<Autostart> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let command: String = match hkcu
                .open_subkey(&self.run)
                .and_then(|key| key.get_value(ENTRY_NAME))
            {
                Ok(command) => command,
                Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Autostart::Off),
                Err(err) => return Err(failed(err)),
            };
            // The Task Manager writes 12 bytes; an odd first byte means "disabled".
            let disabled = hkcu
                .open_subkey(&self.approved)
                .and_then(|key| key.get_raw_value(ENTRY_NAME))
                .is_ok_and(|value| value.bytes.first().is_some_and(|b| b % 2 == 1));
            Ok(if disabled {
                Autostart::DisabledBySystem { command }
            } else {
                Autostart::On { command }
            })
        }

        pub fn enable(&self, exe: &Path, args: &[String]) -> Result<()> {
            self.set_command(exe, args)?;
            // Turning it on by hand overrides an earlier "disabled" in the Task Manager.
            self.forget_approval()
        }

        /// Writes the command, leaving the Task Manager's on/off flag alone.
        pub fn set_command(&self, exe: &Path, args: &[String]) -> Result<()> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (run, _) = hkcu.create_subkey(&self.run).map_err(failed)?;
            run.set_value(ENTRY_NAME, &command_line(exe, args, windows_quote))
                .map_err(failed)
        }

        pub fn disable(&self) -> Result<()> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            match hkcu
                .open_subkey_with_flags(&self.run, winreg::enums::KEY_SET_VALUE)
                .and_then(|key| key.delete_value(ENTRY_NAME))
            {
                Err(err) if err.kind() != ErrorKind::NotFound => return Err(failed(err)),
                _ => {}
            }
            self.forget_approval()
        }

        fn forget_approval(&self) -> Result<()> {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            match hkcu
                .open_subkey_with_flags(&self.approved, winreg::enums::KEY_SET_VALUE)
                .and_then(|key| key.delete_value(ENTRY_NAME))
            {
                Err(err) if err.kind() != ErrorKind::NotFound => Err(failed(err)),
                _ => Ok(()),
            }
        }
    }

    /// Windows paths keep their backslashes; only the quotes matter.
    fn windows_quote(arg: &str) -> String {
        format!("\"{arg}\"")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A throwaway key under `HKCU\Software`, deleted afterwards.
        struct TestKey(String);

        impl Drop for TestKey {
            fn drop(&mut self) {
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
            }
        }

        #[test]
        fn registers_and_unregisters_in_a_test_key() {
            let key = TestKey(format!(r"Software\spacetify-tests\{}", std::process::id()));
            let run = RunKey::at(&key.0);
            assert_eq!(run.status().unwrap(), Autostart::Off);
            run.disable().unwrap();

            let exe = Path::new(r"C:\Program Files\spacetify\spacetify-service.exe");
            run.enable(exe, &["--home".into(), r"D:\my mods".into()])
                .unwrap();
            let expected =
                r#""C:\Program Files\spacetify\spacetify-service.exe" "--home" "D:\my mods""#;
            assert_eq!(
                run.status().unwrap(),
                Autostart::On {
                    command: expected.into()
                }
            );

            // Turned off in the Task Manager.
            let (approved, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(&run.approved)
                .unwrap();
            let value = winreg::RegValue {
                bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into(),
                vtype: winreg::enums::RegType::REG_BINARY,
            };
            approved.set_raw_value(ENTRY_NAME, &value).unwrap();
            assert!(matches!(
                run.status().unwrap(),
                Autostart::DisabledBySystem { .. }
            ));

            run.enable(exe, &[]).unwrap();
            assert!(matches!(run.status().unwrap(), Autostart::On { .. }));
            run.disable().unwrap();
            assert_eq!(run.status().unwrap(), Autostart::Off);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Each system uses one of them; tests use both.
#[allow(dead_code)]
enum Format {
    LaunchAgent,
    Desktop,
}

/// An autostart entry that is a file (macOS and Linux).
#[derive(Debug, Clone)]
#[cfg_attr(windows, allow(dead_code))]
struct EntryFile {
    path: PathBuf,
    format: Format,
}

#[cfg_attr(windows, allow(dead_code))]
impl EntryFile {
    fn new(path: PathBuf, format: Format) -> Self {
        Self { path, format }
    }

    fn status(&self) -> Result<Autostart> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                let command = self.path.display().to_string();
                let hidden = self.format == Format::Desktop
                    && text.contains("X-GNOME-Autostart-enabled=false");
                Ok(if hidden {
                    Autostart::DisabledBySystem { command }
                } else {
                    Autostart::On { command }
                })
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Autostart::Off),
            Err(err) => Err(failed(err)),
        }
    }

    fn enable(&self, exe: &Path, args: &[String]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let text = match self.format {
            Format::LaunchAgent => launch_agent(exe, args),
            Format::Desktop => desktop_entry(exe, args),
        };
        crate::backup::write_atomic(&self.path, text.as_bytes())
    }

    fn disable(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(failed(err)),
            _ => Ok(()),
        }
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn launch_agent(exe: &Path, args: &[String]) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let arguments: String = std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|arg| format!("    <string>{}</string>\n", escape(&arg)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key>\n  <string>{MAC_LABEL}</string>\n\
         \x20 <key>ProgramArguments</key>\n  <array>\n{arguments}  </array>\n\
         \x20 <key>RunAtLoad</key>\n  <true/>\n\
         \x20 <key>ProcessType</key>\n  <string>Background</string>\n\
         </dict>\n</plist>\n"
    )
}

#[cfg_attr(windows, allow(dead_code))]
fn desktop_entry(exe: &Path, args: &[String]) -> String {
    let exec = command_line(exe, args, |arg| {
        quote(arg).replace('$', "\\$").replace('`', "\\`")
    });
    format!(
        "[Desktop Entry]\nType=Application\nName=spacetify\n\
         Comment=Puts your Spotify mods back after an update\n\
         Exec={exec}\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_removes_entry_files() {
        let dir = tempfile::tempdir().unwrap();
        for (name, format) in [
            ("agent.plist", Format::LaunchAgent),
            ("entry.desktop", Format::Desktop),
        ] {
            let entry = EntryFile::new(dir.path().join("nested").join(name), format);
            assert_eq!(entry.status().unwrap(), Autostart::Off);
            entry.disable().unwrap();
            entry
                .enable(
                    Path::new("/opt/space tify/spacetify-service"),
                    &["-v".into()],
                )
                .unwrap();
            assert!(matches!(entry.status().unwrap(), Autostart::On { .. }));
            entry.disable().unwrap();
            assert_eq!(entry.status().unwrap(), Autostart::Off);
        }
    }

    #[test]
    fn renders_entries() {
        let exe = Path::new("/home/me/a&b/spacetify-service");
        let plist = launch_agent(exe, &["--home".into(), "/x".into()]);
        assert!(plist.contains("<string>/home/me/a&amp;b/spacetify-service</string>"));
        assert!(plist.contains("<string>--home</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));

        let desktop = desktop_entry(Path::new("/opt/my $dir/spacetify-service"), &[]);
        assert!(desktop.contains("Exec=\"/opt/my \\$dir/spacetify-service\"\n"));
    }
}
