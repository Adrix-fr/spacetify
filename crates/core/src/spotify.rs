//! Finding Spotify, reading its version and managing its process.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{Error, Result};

/// How Spotify was installed. Some kinds are read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Standard,
    MicrosoftStore,
    Snap,
    Flatpak,
}

impl InstallKind {
    pub fn is_modifiable(self) -> bool {
        !matches!(self, Self::MicrosoftStore | Self::Snap)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::MicrosoftStore => "Microsoft Store",
            Self::Snap => "Snap",
            Self::Flatpak => "Flatpak",
        }
    }
}

/// A Spotify install found on disk.
#[derive(Debug, Clone)]
pub struct SpotifyInstall {
    pub root: PathBuf,
    /// Folder holding `xpui.spa`.
    pub apps_dir: PathBuf,
    pub executable: PathBuf,
    pub kind: InstallKind,
    pub version: Option<String>,
}

impl SpotifyInstall {
    /// Describes the install in `root`, or `None` if it has no `Apps` folder.
    pub fn from_dir(root: &Path) -> Option<Self> {
        let (apps_dir, executable) = layout(root);
        if !apps_dir.is_dir() {
            return None;
        }
        Some(Self {
            version: read_version(root, &executable),
            kind: kind_from_path(root),
            root: root.to_path_buf(),
            apps_dir,
            executable,
        })
    }

    pub fn xpui_spa(&self) -> PathBuf {
        self.apps_dir.join("xpui.spa")
    }

    /// Where Spicetify unpacks the interface.
    pub fn xpui_dir(&self) -> PathBuf {
        self.apps_dir.join("xpui")
    }

    pub fn ensure_modifiable(&self) -> Result<()> {
        if self.kind.is_modifiable() {
            Ok(())
        } else {
            Err(Error::ReadOnlyInstall {
                kind: self.kind.label(),
            })
        }
    }
}

/// Finds Spotify, in `custom` if given, otherwise in the usual places.
pub fn locate(custom: Option<&Path>) -> Result<SpotifyInstall> {
    if let Some(dir) = custom {
        return SpotifyInstall::from_dir(dir)
            .ok_or_else(|| Error::NotASpotifyFolder(dir.to_path_buf()));
    }
    if let Some(install) = candidates()
        .iter()
        .find_map(|dir| SpotifyInstall::from_dir(dir))
    {
        tracing::debug!("found Spotify in {}", install.root.display());
        return Ok(install);
    }
    microsoft_store_install().ok_or(Error::SpotifyNotFound)
}

/// Whether the Microsoft Store version is installed, even alongside the regular one.
pub fn microsoft_store_installed() -> bool {
    microsoft_store_install().is_some()
}

/// The Store version lives in a protected folder we cannot read, so the
/// returned install only points at its data folder.
fn microsoft_store_install() -> Option<SpotifyInstall> {
    if !cfg!(windows) {
        return None;
    }
    let packages = dirs::data_local_dir()?.join("Packages");
    let dir = fs::read_dir(packages)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("SpotifyAB.SpotifyMusic_")
        })?
        .path();
    Some(SpotifyInstall {
        apps_dir: dir.join("Apps"),
        executable: PathBuf::new(),
        kind: InstallKind::MicrosoftStore,
        version: None,
        root: dir,
    })
}

fn candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = dirs::home_dir();

    if cfg!(windows) {
        dirs.extend(dirs::config_dir().map(|appdata| appdata.join("Spotify")));
    } else if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/Applications/Spotify.app"));
        dirs.extend(home.as_ref().map(|h| h.join("Applications/Spotify.app")));
    } else {
        dirs.extend(
            [
                "/opt/spotify",
                "/usr/share/spotify",
                "/usr/lib/spotify",
                "/var/lib/flatpak/app/com.spotify.Client/current/active/files/extra/share/spotify",
                "/snap/spotify/current/usr/share/spotify",
            ]
            .map(PathBuf::from),
        );
        if let Some(home) = &home {
            dirs.push(home.join(".local/share/spotify-launcher/install/usr/share/spotify"));
            dirs.push(home.join(".local/share/flatpak/app/com.spotify.Client/current/active/files/extra/share/spotify"));
        }
    }
    dirs
}

/// Returns the `Apps` folder and the executable for an install root.
fn layout(root: &Path) -> (PathBuf, PathBuf) {
    if root.extension().is_some_and(|ext| ext == "app") {
        let contents = root.join("Contents");
        (
            contents.join("Resources/Apps"),
            contents.join("MacOS/Spotify"),
        )
    } else {
        let exe = if cfg!(windows) {
            "Spotify.exe"
        } else {
            "spotify"
        };
        (root.join("Apps"), root.join(exe))
    }
}

fn kind_from_path(root: &Path) -> InstallKind {
    let path = root.to_string_lossy().replace('\\', "/");
    if path.contains("/WindowsApps/") {
        InstallKind::MicrosoftStore
    } else if path.starts_with("/snap/") {
        InstallKind::Snap
    } else if path.contains("/flatpak/") {
        InstallKind::Flatpak
    } else {
        InstallKind::Standard
    }
}

fn read_version(root: &Path, executable: &Path) -> Option<String> {
    if let Ok(plist) = fs::read_to_string(root.join("Contents/Info.plist")) {
        return parse_plist_version(&plist);
    }
    // Only Windows executables carry a version resource; others give `None`.
    parse_pe_version(&fs::read(executable).ok()?)
}

/// Reads the file version from a Windows executable's `VS_FIXEDFILEINFO`.
///
/// The structure sits right after the UTF-16 key `VS_VERSION_INFO` (plus up
/// to 3 padding bytes). Anchoring on the key avoids false matches in code.
pub fn parse_pe_version(bytes: &[u8]) -> Option<String> {
    // dwSignature 0xFEEF04BD followed by dwStrucVersion 0x00010000, little-endian.
    const SIGNATURE: [u8; 8] = [0xBD, 0x04, 0xEF, 0xFE, 0x00, 0x00, 0x01, 0x00];
    let key = utf16_le("VS_VERSION_INFO\0");

    let start = bytes
        .windows(key.len())
        .enumerate()
        .filter(|(_, window)| *window == key.as_slice())
        .find_map(|(at, _)| {
            let after = at + key.len();
            (0..4).map(|pad| after + pad).find(|&candidate| {
                bytes
                    .get(candidate..candidate + SIGNATURE.len())
                    .is_some_and(|found| found == SIGNATURE)
            })
        })?;
    let dword = |offset: usize| -> Option<u32> {
        let at = start + offset;
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let (ms, ls) = (dword(8)?, dword(12)?);
    Some(format!(
        "{}.{}.{}.{}",
        ms >> 16,
        ms & 0xFFFF,
        ls >> 16,
        ls & 0xFFFF
    ))
}

fn utf16_le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Reads `CFBundleShortVersionString` from a macOS `Info.plist`.
pub fn parse_plist_version(plist: &str) -> Option<String> {
    let after_key = plist.split_once("<key>CFBundleShortVersionString</key>")?.1;
    let value = after_key
        .split_once("<string>")?
        .1
        .split_once("</string>")?
        .0;
    Some(value.trim().to_owned())
}

pub fn is_running() -> Result<bool> {
    Ok(spotify_pids(&processes()?).next().is_some())
}

/// Name of a running Spotify, or of a Spotify updater or installer, if any.
/// While one runs, Spotify's files may change at any moment.
pub fn busy() -> Result<Option<String>> {
    Ok(processes()?
        .processes()
        .values()
        .map(|process| process.name().to_string_lossy().into_owned())
        .find(|name| is_spotify_or_updater(name)))
}

/// `Spotify.exe`, `spotify`, and Spotify's update and setup programs.
pub fn is_spotify_or_updater(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    stem == "spotify"
        || (stem.starts_with("spotify")
            && ["update", "setup", "install", "migrat"]
                .iter()
                .any(|word| stem.contains(word)))
}

/// Closes every Spotify process and waits for them to exit.
pub fn close(timeout: Duration) -> Result<()> {
    let system = processes()?;
    for pid in spotify_pids(&system) {
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }

    let deadline = Instant::now() + timeout;
    while is_running()? {
        if Instant::now() > deadline {
            return Err(Error::CloseFailed);
        }
        thread::sleep(Duration::from_millis(200));
    }
    tracing::info!("Spotify closed");
    Ok(())
}

/// Starts Spotify detached from this process.
pub fn launch(install: &SpotifyInstall, args: &[String]) -> Result<Child> {
    install.ensure_modifiable()?;
    tracing::info!(
        "starting {} {}",
        install.executable.display(),
        args.join(" ")
    );
    Command::new(&install.executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| Error::Launch(err.to_string()))
}

fn processes() -> Result<System> {
    let mut system = System::new().map_err(|err| Error::Processes(err.to_string()))?;
    // Names only: much cheaper than the default, which also reads CPU and memory use.
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    Ok(system)
}

fn spotify_pids(system: &System) -> impl Iterator<Item = sysinfo::Pid> + '_ {
    system.processes().iter().filter_map(|(pid, process)| {
        let name = process.name().to_string_lossy();
        let is_spotify =
            name.eq_ignore_ascii_case("spotify.exe") || name.eq_ignore_ascii_case("spotify");
        is_spotify.then_some(*pid)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNATURE: [u8; 8] = [0xBD, 0x04, 0xEF, 0xFE, 0x00, 0x00, 0x01, 0x00];

    fn fixed_file_info(major: u16, minor: u16, patch: u16, build: u16) -> Vec<u8> {
        let mut info = SIGNATURE.to_vec();
        info.extend(((u32::from(major) << 16) | u32::from(minor)).to_le_bytes());
        info.extend(((u32::from(patch) << 16) | u32::from(build)).to_le_bytes());
        info
    }

    #[test]
    fn reads_pe_file_version() {
        // A stray signature in code must be ignored, like in the real Spotify.exe.
        let mut bytes = vec![0x48; 16];
        bytes.extend(fixed_file_info(9340, 36168, 8075, 18720));
        bytes.extend(utf16_le("VS_VERSION_INFO\0"));
        bytes.extend([0, 0]);
        bytes.extend(fixed_file_info(1, 3, 6, 1921));
        bytes.extend([0u8; 32]);
        assert_eq!(parse_pe_version(&bytes).as_deref(), Some("1.3.6.1921"));
    }

    #[test]
    fn ignores_files_without_version_resource() {
        assert_eq!(parse_pe_version(b"\x7fELF not a windows binary"), None);
        assert_eq!(parse_pe_version(&fixed_file_info(1, 2, 3, 4)), None);
    }

    #[test]
    fn reads_plist_version() {
        let plist = "<dict>\n<key>CFBundleName</key>\n<string>Spotify</string>\n\
                     <key>CFBundleShortVersionString</key>\n<string>1.2.48.405</string>\n</dict>";
        assert_eq!(parse_plist_version(plist).as_deref(), Some("1.2.48.405"));
    }

    #[test]
    fn recognises_spotify_and_its_updater() {
        for name in [
            "Spotify.exe",
            "spotify",
            "SpotifyUpdate.exe",
            "SpotifySetup.exe",
            "SpotifyFullSetup.exe",
            "SpotifyMigrator.exe",
        ] {
            assert!(is_spotify_or_updater(name), "{name}");
        }
        for name in ["spacetify-service.exe", "SpotifyWidget.exe", "notepad.exe"] {
            assert!(!is_spotify_or_updater(name), "{name}");
        }
    }

    #[test]
    fn classifies_install_paths() {
        assert_eq!(
            kind_from_path(Path::new("/snap/spotify/current/usr/share/spotify")),
            InstallKind::Snap
        );
        assert_eq!(
            kind_from_path(Path::new(
                r"C:\Program Files\WindowsApps\SpotifyAB.SpotifyMusic_1\x"
            )),
            InstallKind::MicrosoftStore
        );
        assert_eq!(
            kind_from_path(Path::new("/opt/spotify")),
            InstallKind::Standard
        );
        assert!(!InstallKind::Snap.is_modifiable());
        assert!(InstallKind::Flatpak.is_modifiable());
    }

    #[test]
    fn rejects_folders_without_apps() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SpotifyInstall::from_dir(dir.path()).is_none());
        fs::create_dir(dir.path().join("Apps")).unwrap();
        let install = SpotifyInstall::from_dir(dir.path()).unwrap();
        assert_eq!(install.kind, InstallKind::Standard);
        assert_eq!(install.xpui_spa(), dir.path().join("Apps").join("xpui.spa"));
    }
}
