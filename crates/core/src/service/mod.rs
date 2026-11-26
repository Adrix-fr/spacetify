//! Background service that puts the mods back after a Spotify update.
//!
//! The `spacetify-service` program runs [`daemon::run`]. It watches Spotify's
//! `Apps` folder. When an update has replaced the patched interface, it waits
//! until Spotify and its updater are closed and the files stop changing,
//! backs up the new interface, turns off the mods that no longer fit, and
//! applies the others.
//!
//! Front ends (the CLI today, the desktop app later) talk to it through files
//! in its folder, never through the network. They live in the user's own
//! profile, so only the user and their programs can read or write them.
//!
//! ```text
//! <data>/service/
//!   service.lock   locked while the service runs: only one runs at a time
//!   status.json    what it is doing now, rewritten when that changes
//!   events.jsonl   what happened, one JSON object per line
//!   service.log    technical log
//!   inbox/         requests (`check`, `restart-spotify`, `apply-later`,
//!                  `apply-now`, `stop`): one file each, deleted once read
//!   gallery.json   port and secret of the gallery's local server
//! ```
//!
//! The one exception to "files only" is the gallery page inside Spotify,
//! which cannot write files: it talks to a small server of the service on
//! 127.0.0.1 (see [`crate::gallery::server`]).

pub mod alert;
pub mod autostart;
pub mod daemon;
pub mod events;
mod repair;

use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::backup::{try_lock_file, write_atomic};
use crate::config::AppDirs;
use crate::{Error, Lang, Result};

/// File name of the service program, next to the `spacetify` one.
pub const EXECUTABLE: &str = if cfg!(windows) {
    "spacetify-service.exe"
} else {
    "spacetify-service"
};

const REQUEST_EXT: &str = "req";

/// Where the service keeps its files.
#[derive(Debug, Clone)]
pub struct ServicePaths {
    pub dir: PathBuf,
}

impl ServicePaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            dir: dirs.data_dir.join("service"),
        }
    }

    pub fn lock_file(&self) -> PathBuf {
        self.dir.join("service.lock")
    }

    pub fn status_file(&self) -> PathBuf {
        self.dir.join("status.json")
    }

    pub fn events_file(&self) -> PathBuf {
        self.dir.join("events.jsonl")
    }

    pub fn log_file(&self) -> PathBuf {
        self.dir.join("service.log")
    }

    pub fn inbox_dir(&self) -> PathBuf {
        self.dir.join("inbox")
    }
}

/// A message in both languages, so files written by the service read well
/// whatever language the reader uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    pub en: String,
    pub fr: String,
}

impl Text {
    pub fn new(en: impl Into<String>, fr: impl Into<String>) -> Self {
        Self {
            en: en.into(),
            fr: fr.into(),
        }
    }

    pub fn of_error(err: &Error) -> Self {
        Self::new(err.localized(Lang::En), err.localized(Lang::Fr))
    }

    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::En => &self.en,
            Lang::Fr => &self.fr,
        }
    }
}

/// What the service is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Phase {
    Starting,
    /// The mods are in place. Waiting for something to change.
    Watching,
    /// No mods were applied, so there is nothing to keep in place.
    NothingApplied,
    SpotifyNotFound,
    /// An update replaced the interface. Waiting for its files to settle.
    Settling,
    /// An update replaced the interface. Waiting for this program to close.
    WaitingForSpotify {
        process: String,
    },
    Repairing,
    /// Writing changes made in the gallery into Spotify.
    Applying,
    /// Nothing is done until the situation changes.
    Blocked {
        reason: Text,
    },
    Stopped,
}

impl Phase {
    pub fn describe(&self, lang: Lang) -> String {
        match self {
            Self::Starting => crate::tr!(lang, "starting", "démarrage"),
            Self::Watching => crate::tr!(
                lang,
                "watching Spotify, mods in place",
                "surveille Spotify, mods en place"
            ),
            Self::NothingApplied => crate::tr!(
                lang,
                "waiting: no mods applied yet (`spacetify apply`)",
                "en attente : aucun mod appliqué pour l'instant (`spacetify apply`)"
            ),
            Self::SpotifyNotFound => crate::tr!(lang, "Spotify not found", "Spotify introuvable"),
            Self::Settling => crate::tr!(
                lang,
                "Spotify was updated, waiting for the update to finish",
                "Spotify a été mis à jour, attend la fin de la mise à jour"
            ),
            Self::WaitingForSpotify { process } => crate::tr!(
                lang,
                "Spotify was updated, waiting for {process} to close",
                "Spotify a été mis à jour, attend que {process} soit fermé"
            ),
            Self::Repairing => crate::tr!(lang, "putting the mods back", "remet les mods"),
            Self::Applying => crate::tr!(
                lang,
                "applying the changes made in the gallery",
                "applique les changements de la galerie"
            ),
            Self::Blocked { reason } => crate::tr!(
                lang,
                "on hold: {}",
                "en pause : {}",
                reason.get(lang).lines().next().unwrap_or_default()
            ),
            Self::Stopped => crate::tr!(lang, "stopped", "arrêté"),
        }
    }
}

/// Contents of `status.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub pid: u32,
    pub version: String,
    pub started_at: String,
    pub updated_at: String,
    pub phase: Phase,
    pub spotify_version: Option<String>,
    pub last_repair: Option<String>,
    /// News already reported, so the same thing is never reported twice,
    /// even after a restart.
    #[serde(default)]
    pub seen: Vec<String>,
    /// Changes made in the gallery wait for Spotify to close.
    #[serde(default)]
    pub apply_pending: bool,
}

/// "5 min ago", "il y a 2 h"… for a time written by the service.
pub fn time_ago(at: &str, lang: Lang) -> String {
    let Ok(then) = humantime::parse_rfc3339(at) else {
        return at.to_owned();
    };
    let seconds = SystemTime::now()
        .duration_since(then)
        .unwrap_or_default()
        .as_secs();
    let (value, en, fr) = match seconds {
        0..60 => return crate::tr!(lang, "just now", "à l'instant"),
        60..3600 => (seconds / 60, "min", "min"),
        3600..86400 => (seconds / 3600, "h", "h"),
        _ => (seconds / 86400, "d", "j"),
    };
    crate::tr!(lang, "{value} {en} ago", "il y a {value} {fr}")
}

pub fn read_status(paths: &ServicePaths) -> Result<Option<ServiceStatus>> {
    let path = paths.status_file();
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| Error::Config {
                path,
                reason: err.to_string(),
            }),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::io(&path)(err)),
    }
}

pub(crate) fn write_status(paths: &ServicePaths, status: &ServiceStatus) -> Result<()> {
    fs::create_dir_all(&paths.dir).map_err(Error::io(&paths.dir))?;
    let json = serde_json::to_vec_pretty(status).expect("status is always serializable");
    write_atomic(&paths.status_file(), &json)
}

/// Proof that this process is the only service running. Released on drop,
/// or by the system if the process dies.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// `None` if another service already runs.
    pub fn try_acquire(paths: &ServicePaths) -> Result<Option<Self>> {
        Ok(try_lock_file(&paths.lock_file())?.map(|file| Self { _file: file }))
    }
}

/// Whether a service holds the lock right now.
pub fn is_running(paths: &ServicePaths) -> bool {
    if !paths.lock_file().is_file() {
        return false;
    }
    matches!(try_lock_file(&paths.lock_file()), Ok(None))
}

/// Something a front end asks the service to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Look at Spotify now instead of waiting.
    Check,
    /// Close Spotify, put the mods back and open it again. Sent by the
    /// button of the "Spotify was updated" notification.
    RestartSpotify,
    /// The gallery changed the mods: apply them once Spotify closes.
    ApplyLater,
    /// The gallery's "Restart Spotify": close it, apply, open it again.
    ApplyNow,
    Stop,
}

impl Request {
    fn word(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::RestartSpotify => "restart-spotify",
            Self::ApplyLater => "apply-later",
            Self::ApplyNow => "apply-now",
            Self::Stop => "stop",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "check" => Some(Self::Check),
            "restart-spotify" => Some(Self::RestartSpotify),
            "apply-later" => Some(Self::ApplyLater),
            "apply-now" => Some(Self::ApplyNow),
            "stop" => Some(Self::Stop),
            _ => None,
        }
    }
}

/// Drops a request in the service's inbox. The service reacts within a
/// moment if it runs; otherwise the request is discarded at its next start.
pub fn send(paths: &ServicePaths, request: Request) -> Result<()> {
    let inbox = paths.inbox_dir();
    fs::create_dir_all(&inbox).map_err(Error::io(&inbox))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("{nanos}-{}", std::process::id());
    // Written under another extension first, so the service never reads half a file.
    let tmp = inbox.join(format!("{name}.tmp"));
    fs::write(&tmp, request.word()).map_err(Error::io(&tmp))?;
    let target = inbox.join(format!("{name}.{REQUEST_EXT}"));
    fs::rename(&tmp, &target).map_err(Error::io(&target))
}

/// Reads and deletes every request in the inbox, oldest first.
pub(crate) fn take_requests(paths: &ServicePaths) -> Vec<Request> {
    let Ok(entries) = fs::read_dir(paths.inbox_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == REQUEST_EXT))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok();
            let _ = fs::remove_file(&path);
            text.as_deref().and_then(Request::parse)
        })
        .collect()
}

/// The service program that sits next to the running one.
pub fn executable() -> Result<PathBuf> {
    let current = std::env::current_exe().map_err(|err| Error::ServiceStart(err.to_string()))?;
    let exe = current.with_file_name(EXECUTABLE);
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(Error::ServiceMissing(exe))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    Started,
    AlreadyRunning,
}

/// Starts the service in the background, detached from this process.
pub fn start(paths: &ServicePaths, exe: &Path, args: &[String]) -> Result<StartOutcome> {
    if is_running(paths) {
        return Ok(StartOutcome::AlreadyRunning);
    }
    let mut child =
        spawn_detached(exe, args).map_err(|err| Error::ServiceStart(err.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if is_running(paths) {
            return Ok(StartOutcome::Started);
        }
        if let Ok(Some(status)) = child.try_wait() {
            // It exits right away when another one took the lock in the meantime.
            if is_running(paths) {
                return Ok(StartOutcome::AlreadyRunning);
            }
            tracing::info!("the service exited with {status}");
            return Err(Error::ServiceFailed(paths.log_file()));
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(Error::ServiceFailed(paths.log_file()))
}

fn spawn_detached(exe: &Path, args: &[String]) -> io::Result<std::process::Child> {
    let command = || {
        let mut command = Command::new(exe);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Leaving the terminal's job keeps the service alive when the terminal
        // closes. Some jobs forbid it, then the plain way is used.
        command()
            .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .or_else(|_| command().creation_flags(CREATE_NEW_PROCESS_GROUP).spawn())
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command().process_group(0).spawn()
    }
    #[cfg(not(any(windows, unix)))]
    {
        command().spawn()
    }
}

/// Asks the service to stop and waits for it. Returns whether one was running.
pub fn stop(paths: &ServicePaths) -> Result<bool> {
    if !is_running(paths) {
        return Ok(false);
    }
    send(paths, Request::Stop)?;
    if wait_until_stopped(paths, Duration::from_secs(5)) {
        return Ok(true);
    }
    // Not answering: end the process, after checking it really is the service.
    if let Some(status) = read_status(paths)?
        && kill_service(status.pid)
        && wait_until_stopped(paths, Duration::from_secs(3))
    {
        return Ok(true);
    }
    Err(Error::ServiceStop)
}

fn wait_until_stopped(paths: &ServicePaths, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_running(paths) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

fn kill_service(pid: u32) -> bool {
    let Ok(mut system) = System::new() else {
        return false;
    };
    let pid = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system.process(pid).is_some_and(|process| {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        name.starts_with("spacetify-service") && process.kill()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> (tempfile::TempDir, ServicePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ServicePaths::new(&AppDirs::in_folder(dir.path()));
        (dir, paths)
    }

    #[test]
    fn only_one_instance_at_a_time() {
        let (_dir, paths) = paths();
        assert!(!is_running(&paths));
        let lock = InstanceLock::try_acquire(&paths).unwrap().unwrap();
        assert!(is_running(&paths));
        assert!(InstanceLock::try_acquire(&paths).unwrap().is_none());
        drop(lock);
        assert!(!is_running(&paths));
    }

    #[test]
    fn requests_go_through_the_inbox_once() {
        let (_dir, paths) = paths();
        assert!(take_requests(&paths).is_empty());
        send(&paths, Request::Check).unwrap();
        send(&paths, Request::Stop).unwrap();
        fs::write(paths.inbox_dir().join("junk.req"), "format c:").unwrap();
        fs::write(paths.inbox_dir().join("half.tmp"), "stop").unwrap();

        let requests = take_requests(&paths);
        assert_eq!(requests.len(), 2);
        assert!(requests.contains(&Request::Check));
        assert!(requests.contains(&Request::Stop));
        assert!(take_requests(&paths).is_empty());
    }

    #[test]
    fn says_how_long_ago() {
        let at = |secs: u64| {
            humantime::format_rfc3339_seconds(SystemTime::now() - Duration::from_secs(secs))
                .to_string()
        };
        assert_eq!(time_ago(&at(5), Lang::Fr), "à l'instant");
        assert_eq!(time_ago(&at(300), Lang::En), "5 min ago");
        assert_eq!(time_ago(&at(7300), Lang::Fr), "il y a 2 h");
        assert_eq!(time_ago(&at(3 * 86400), Lang::Fr), "il y a 3 j");
        assert_eq!(time_ago("garbage", Lang::En), "garbage");
    }

    #[test]
    fn status_round_trips() {
        let (_dir, paths) = paths();
        assert_eq!(read_status(&paths).unwrap(), None);
        let status = ServiceStatus {
            pid: 42,
            version: "0.3.0".into(),
            started_at: "2026-11-25T10:00:00Z".into(),
            updated_at: "2026-11-25T10:00:05Z".into(),
            phase: Phase::WaitingForSpotify {
                process: "Spotify.exe".into(),
            },
            spotify_version: Some("1.3.6.1921".into()),
            last_repair: None,
            seen: vec!["update:abc".into()],
            apply_pending: true,
        };
        write_status(&paths, &status).unwrap();
        assert_eq!(read_status(&paths).unwrap(), Some(status));
        let json = fs::read_to_string(paths.status_file()).unwrap();
        assert!(
            json.contains("\"state\": \"waiting_for_spotify\""),
            "{json}"
        );
    }
}
