//! The service's main loop.
//!
//! It sleeps until something happens: a change in Spotify's `Apps` folder
//! (reported by the system, no polling), a request in the inbox, or the
//! safety check every few minutes in case a change was missed. Then:
//!
//! 1. If the interface is still patched, or no mods were ever applied,
//!    there is nothing to do.
//! 2. If an update replaced it, wait while Spotify or its updater runs, and
//!    until the file has stopped changing for a few seconds.
//! 3. Back up the new interface (checked by hash), turn off the mods that no
//!    longer fit it, and apply the rest.
//!
//! Each piece of news is reported once: in `events.jsonl`, and with a
//! desktop notification when the user may want to act.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind as FsEventKind, RecursiveMode, Watcher};

use super::events::{EventKind, EventLog};
use super::repair::{self, Outcome, Repaired};
use super::{
    InstanceLock, Phase, Request, ServicePaths, ServiceStatus, Text, read_status, take_requests,
    write_status,
};
use crate::backup::{self, BackupStore};
use crate::config::{AppDirs, Config};
use crate::detect::{self, UiState};
use crate::spotify::{self, SpotifyInstall};
use crate::{Error, Lang, Result};

/// How long the interface must stay unchanged before it is touched.
const SETTLE: Duration = Duration::from_secs(10);
/// How often to look again while Spotify or its updater runs. Short, so a
/// Spotify closed only for a moment is not missed.
const BUSY_POLL: Duration = Duration::from_secs(2);
/// When the interface is missing or half written.
const SOON: Duration = Duration::from_secs(30);
/// Safety check, in case the system missed a change.
const FALLBACK: Duration = Duration::from_secs(10 * 60);
/// News kept in `status.json` to avoid repeating it.
const MAX_SEEN: usize = 50;
/// After "Restart Spotify", Spotify is opened again within this time,
/// even if the mods could not be put back.
const RELAUNCH_WITHIN: Duration = Duration::from_secs(60);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

/// What the service needs from the outside world. Tests replace it.
pub trait Host {
    /// Name of a running Spotify or Spotify updater, if any.
    fn busy(&self) -> Result<Option<String>>;
    fn notify(&self, title: &str, body: &str);
    /// A notification whose button sends [`Request::RestartSpotify`].
    fn offer_restart(&self, title: &str, body: &str, button: &str);
    fn close_spotify(&self) -> Result<()>;
    fn launch_spotify(&self, install: &SpotifyInstall) -> Result<()>;
}

/// The real computer.
pub struct SystemHost {
    paths: ServicePaths,
}

impl Host for SystemHost {
    fn busy(&self) -> Result<Option<String>> {
        spotify::busy()
    }

    fn notify(&self, title: &str, body: &str) {
        super::alert::show(title, body);
    }

    fn offer_restart(&self, title: &str, body: &str, button: &str) {
        let paths = self.paths.clone();
        // Goes through the inbox like any other request, so the main loop
        // handles it in order.
        super::alert::show_with_button(title, body, button, move || {
            if let Err(err) = super::send(&paths, Request::RestartSpotify) {
                tracing::warn!("{err}");
            }
        });
    }

    fn close_spotify(&self) -> Result<()> {
        spotify::close(CLOSE_TIMEOUT)
    }

    fn launch_spotify(&self, install: &SpotifyInstall) -> Result<()> {
        spotify::launch(install, &[]).map(drop)
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub dirs: AppDirs,
    /// Spotify's folder, when it is not found automatically.
    pub spotify_path: Option<PathBuf>,
}

/// Runs until asked to stop. Returns at once if another service runs.
pub fn run(settings: Settings) -> Result<()> {
    let paths = ServicePaths::new(&settings.dirs);
    let Some(_instance) = InstanceLock::try_acquire(&paths)? else {
        tracing::info!("another spacetify service is already running");
        return Ok(());
    };
    // Requests sent while no service ran are out of date.
    let _ = take_requests(&paths);

    let inbox = paths.inbox_dir();
    std::fs::create_dir_all(&inbox).map_err(Error::io(&inbox))?;
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).map_err(watch_error)?;
    watcher
        .watch(&inbox, RecursiveMode::NonRecursive)
        .map_err(watch_error)?;
    let mut watched: Vec<PathBuf> = Vec::new();

    let host = SystemHost {
        paths: paths.clone(),
    };
    let mut daemon = Daemon::new(settings, host, Instant::now());
    tracing::info!("service started");
    loop {
        // After the check, which is what finds Spotify's folder.
        daemon.tick(Instant::now());
        sync_watches(&mut watcher, &mut watched, daemon.install.as_ref());
        let timeout = daemon.due().saturating_duration_since(Instant::now());
        let event = match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                tracing::warn!("file watcher: {err}");
                continue;
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if event.paths.iter().any(|path| path.starts_with(&inbox)) {
            for request in take_requests(&paths) {
                match request {
                    Request::Stop => {
                        daemon.stop();
                        return Ok(());
                    }
                    Request::Check => daemon.check_now(Instant::now()),
                    Request::RestartSpotify => daemon.restart_spotify(Instant::now()),
                }
            }
        }
        if let Some(install) = &daemon.install
            && concerns_interface(&event.paths, &install.apps_dir)
        {
            tracing::debug!("change in Spotify's interface: {:?}", event.paths);
            if matches!(event.kind, FsEventKind::Remove(_) | FsEventKind::Create(_))
                && event.paths.contains(&install.apps_dir)
            {
                // The folder itself was replaced: watch the new one.
                watched.retain(|path| path != &install.apps_dir);
            }
            daemon.changed(Instant::now());
        }
    }
    daemon.stop();
    Ok(())
}

fn watch_error(err: notify::Error) -> Error {
    Error::ServiceStart(format!("cannot watch files: {err}"))
}

/// Watches Spotify's folder (to see `Apps` being replaced) and `Apps`.
fn sync_watches(
    watcher: &mut impl Watcher,
    watched: &mut Vec<PathBuf>,
    install: Option<&SpotifyInstall>,
) {
    let wanted: Vec<PathBuf> = install
        .map(|i| vec![i.root.clone(), i.apps_dir.clone()])
        .unwrap_or_default();
    watched.retain(|path| {
        let keep = wanted.contains(path);
        if !keep {
            let _ = watcher.unwatch(path);
        }
        keep
    });
    for path in wanted {
        if !watched.contains(&path) && path.is_dir() {
            match watcher.watch(&path, RecursiveMode::NonRecursive) {
                Ok(()) => watched.push(path),
                Err(err) => tracing::warn!("cannot watch {}: {err}", path.display()),
            }
        }
    }
}

/// `xpui.spa` (and its temporary copies), or the `Apps` folder itself.
fn concerns_interface(paths: &[PathBuf], apps_dir: &Path) -> bool {
    paths.iter().any(|path| {
        path == apps_dir
            || (path.parent() == Some(apps_dir)
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("xpui")))
    })
}

/// Size and modification time: when they stay the same, so does the file.
type Stamp = (u64, SystemTime);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

pub(crate) struct Daemon<H: Host> {
    settings: Settings,
    host: H,
    paths: ServicePaths,
    events: EventLog,
    status: ServiceStatus,
    install: Option<SpotifyInstall>,
    next_check: Instant,
    /// While an update settles: the interface's stamp, and since when it has not changed.
    settling: Option<(Stamp, Instant)>,
    /// Stamp of the interface when it was last seen patched.
    healthy: Option<Stamp>,
    /// Spotify was closed by "Restart Spotify": open it again by then.
    relaunch_by: Option<Instant>,
}

impl<H: Host> Daemon<H> {
    pub(crate) fn new(settings: Settings, host: H, now: Instant) -> Self {
        let paths = ServicePaths::new(&settings.dirs);
        let previous = read_status(&paths).ok().flatten();
        let status = ServiceStatus {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            started_at: backup::now(),
            updated_at: backup::now(),
            phase: Phase::Starting,
            spotify_version: None,
            last_repair: previous.as_ref().and_then(|p| p.last_repair.clone()),
            seen: previous.map(|p| p.seen).unwrap_or_default(),
        };
        let daemon = Self {
            events: EventLog::new(paths.events_file()),
            settings,
            host,
            paths,
            status,
            install: None,
            next_check: now,
            settling: None,
            healthy: None,
            relaunch_by: None,
        };
        daemon.save();
        daemon.log(EventKind::Started {
            version: daemon.status.version.clone(),
        });
        daemon
    }

    pub(crate) fn due(&self) -> Instant {
        self.next_check
    }

    /// Something changed in Spotify's folder. Looks again shortly, without
    /// putting it off if changes keep coming.
    pub(crate) fn changed(&mut self, now: Instant) {
        self.healthy = None;
        self.next_check = self.next_check.min(now + SETTLE);
    }

    pub(crate) fn check_now(&mut self, now: Instant) {
        self.healthy = None;
        self.next_check = now;
    }

    pub(crate) fn tick(&mut self, now: Instant) {
        if now >= self.next_check {
            let wait = self.evaluate(now);
            self.next_check = now + wait;
            self.relaunch_if_done(now);
        }
    }

    /// "Restart Spotify" was pressed: close it, put the mods back right
    /// away, then open it again.
    pub(crate) fn restart_spotify(&mut self, now: Instant) {
        if !matches!(self.status.phase, Phase::WaitingForSpotify { .. }) {
            tracing::info!("restart asked, but nothing is waiting for Spotify");
            return;
        }
        if let Err(err) = self.host.close_spotify() {
            tracing::warn!("could not close Spotify: {err}");
            return;
        }
        self.relaunch_by = Some(now + RELAUNCH_WITHIN);
        self.next_check = now;
    }

    /// Opens Spotify again after "Restart Spotify", once the repair is over
    /// one way or another.
    fn relaunch_if_done(&mut self, now: Instant) {
        let Some(deadline) = self.relaunch_by else {
            return;
        };
        let pending = matches!(self.status.phase, Phase::Settling | Phase::Repairing);
        if pending && now < deadline {
            return;
        }
        self.relaunch_by = None;
        // Opened again by hand in the meantime: nothing to do.
        if matches!(self.status.phase, Phase::WaitingForSpotify { .. }) {
            return;
        }
        let Some(install) = self.install.clone() else {
            return;
        };
        match self.host.launch_spotify(&install) {
            Ok(()) => self.log(EventKind::SpotifyRestarted),
            Err(err) => tracing::warn!("could not start Spotify: {err}"),
        }
    }

    pub(crate) fn stop(&mut self) {
        self.set_phase(Phase::Stopped);
        self.log(EventKind::Stopped);
        tracing::info!("service stopped");
    }

    /// Looks at Spotify and does what is needed. Returns when to look again.
    fn evaluate(&mut self, now: Instant) -> Duration {
        let Some(install) = self.locate() else {
            self.set_phase(Phase::SpotifyNotFound);
            return FALLBACK;
        };
        if let Err(err) = install.ensure_modifiable() {
            self.set_phase(Phase::Blocked {
                reason: Text::of_error(&err),
            });
            return FALLBACK;
        }
        let store = BackupStore::new(self.settings.dirs.backups_dir());
        match store.load_applied() {
            Ok(Some(_)) => {}
            Ok(None) => {
                self.settling = None;
                self.new_episode();
                self.set_phase(Phase::NothingApplied);
                return FALLBACK;
            }
            Err(err) => {
                tracing::warn!("{err}");
                return FALLBACK;
            }
        }

        let spa = install.xpui_spa();
        let current = stamp(&spa);
        if self.status.phase == Phase::Watching && current.is_some() && current == self.healthy {
            return FALLBACK;
        }
        let inspection = match detect::inspect(&install) {
            Ok(inspection) => inspection,
            Err(err) => {
                // Usually an update writing the file right now.
                tracing::info!("interface not readable yet: {err}");
                self.settling = None;
                self.set_phase(Phase::Settling);
                return SOON;
            }
        };
        match inspection.state {
            UiState::Patched(_) => {
                self.healthy = current;
                self.settling = None;
                self.new_episode();
                self.set_phase(Phase::Watching);
                FALLBACK
            }
            UiState::Original => self.repair_when_ready(now, install, &store),
            UiState::Missing => {
                self.set_phase(Phase::Settling);
                SOON
            }
            UiState::Spicetify => self.hold(Text::of_error(&Error::SpicetifyActive)),
            UiState::Unknown(what) => self.hold(Text::of_error(&Error::UnknownModification(what))),
        }
    }

    fn repair_when_ready(
        &mut self,
        now: Instant,
        mut install: SpotifyInstall,
        store: &BackupStore,
    ) -> Duration {
        if self.first_time("update") {
            install = self.refreshed(install);
            self.log(EventKind::UpdateDetected {
                spotify_version: install.version.clone(),
            });
        }

        match self.host.busy() {
            Ok(None) => {}
            Ok(Some(process)) => {
                // Spotify runs with this file, so it is not being written. If it is
                // still the same once Spotify closes, it can be repaired at once;
                // if an updater changes it after Spotify closes, the clock restarts.
                let current = stamp(&install.xpui_spa());
                if self.settling.map(|(seen, _)| seen) != current {
                    self.settling = current.map(|current| (current, now));
                }
                if self.first_time("wait") {
                    self.log(EventKind::WaitingForSpotify {
                        process: process.clone(),
                    });
                    let lang = self.lang();
                    let title = Text::new("Spotify was updated", "Spotify a été mis à jour");
                    let body = Text::new(
                        "The update removed your mods. \"Restart Spotify\" puts them back in a few \
                         seconds (the music stops for a moment). Otherwise they come back once \
                         you close Spotify.",
                        "La mise à jour a retiré tes mods. « Relancer Spotify » les remet en quelques \
                         secondes (la musique s'arrête un instant). Sinon, ils reviendront quand tu \
                         fermeras Spotify.",
                    );
                    let button = Text::new("Restart Spotify", "Relancer Spotify");
                    self.host
                        .offer_restart(title.get(lang), body.get(lang), button.get(lang));
                }
                self.set_phase(Phase::WaitingForSpotify { process });
                return BUSY_POLL;
            }
            Err(err) => {
                tracing::warn!("{err}");
                return BUSY_POLL;
            }
        }

        let current = stamp(&install.xpui_spa());
        match (self.settling, current) {
            (Some((seen, since)), Some(current)) if seen == current => {
                let quiet = now.saturating_duration_since(since);
                if quiet < SETTLE {
                    self.set_phase(Phase::Settling);
                    return SETTLE - quiet;
                }
            }
            _ => {
                self.settling = current.map(|current| (current, now));
                self.set_phase(Phase::Settling);
                return SETTLE;
            }
        }
        self.settling = None;

        self.set_phase(Phase::Repairing);
        let install = self.refreshed(install);
        let outcome = repair::run(
            &install,
            store,
            &self.settings.dirs.mods(),
            &self.settings.dirs.config_file(),
            || repair::idle_unless(self.host.busy()),
        );
        match outcome {
            Ok(Outcome::Repaired(repaired)) => {
                self.repaired(&install, repaired);
                FALLBACK
            }
            Ok(Outcome::NotNeeded) => Duration::from_secs(1),
            // Started in the meantime: wait for it again.
            Err(Error::SpotifyRunning) => BUSY_POLL,
            Err(Error::Busy) => SOON,
            Err(err) => {
                tracing::warn!("repair failed: {err}");
                let reason = Text::of_error(&err);
                if self.first_time(&format!("failed:{}", reason.en)) {
                    self.log(EventKind::RepairFailed {
                        reason: reason.clone(),
                    });
                    self.notify(
                        Text::new(
                            "spacetify could not put your mods back",
                            "spacetify n'a pas pu remettre tes mods",
                        ),
                        Text::new(
                            format!("{} Details: `spacetify service events`.", reason.en),
                            format!("{} Détails : `spacetify service events`.", reason.fr),
                        ),
                    );
                }
                self.set_phase(Phase::Blocked { reason });
                FALLBACK
            }
        }
    }

    fn repaired(&mut self, install: &SpotifyInstall, repaired: Repaired) {
        let version = install.version.clone();
        if let Some(sha256) = &repaired.new_backup {
            self.log(EventKind::BackedUp {
                sha256: sha256.clone(),
                spotify_version: version.clone(),
            });
        }
        for off in &repaired.turned_off {
            self.log(EventKind::ModTurnedOff {
                kind: off.kind,
                id: off.id.clone(),
                missing: off.missing,
            });
        }
        self.log(EventKind::Repaired {
            spotify_version: version.clone(),
            theme: repaired.theme.clone(),
            extensions: repaired.extensions,
        });

        let shown = version.as_deref().unwrap_or("?");
        let mut body = if self.relaunch_by.is_some() {
            Text::new(
                format!("Spotify {shown}: your mods are back. Spotify is opening again."),
                format!("Spotify {shown} : tes mods sont de retour. Spotify se rouvre."),
            )
        } else {
            Text::new(
                format!("Spotify {shown}: your mods are back. Open Spotify to see them."),
                format!("Spotify {shown} : tes mods sont de retour. Ouvre Spotify pour les voir."),
            )
        };
        if !repaired.turned_off.is_empty() {
            let ids: Vec<&str> = repaired.turned_off.iter().map(|m| m.id.as_str()).collect();
            let ids = ids.join(", ");
            body.en += &format!(" Turned off because they no longer fit: {ids}.");
            body.fr += &format!(" Désactivé(s) car plus compatible(s) : {ids}.");
        }
        self.notify(Text::new("Mods put back", "Mods remis en place"), body);

        self.status.last_repair = Some(backup::now());
        self.healthy = stamp(&install.xpui_spa());
        self.new_episode();
        self.set_phase(Phase::Watching);
    }

    /// Spotify is in a state the service must not touch.
    fn hold(&mut self, reason: Text) -> Duration {
        self.settling = None;
        if self.first_time(&format!("hold:{}", reason.en)) {
            self.log(EventKind::OnHold {
                reason: reason.clone(),
            });
            self.notify(
                Text::new("spacetify is on hold", "spacetify est en pause"),
                reason.clone(),
            );
        }
        self.set_phase(Phase::Blocked { reason });
        FALLBACK
    }

    fn locate(&mut self) -> Option<SpotifyInstall> {
        if let Some(install) = &self.install
            && install.apps_dir.is_dir()
        {
            return Some(install.clone());
        }
        match spotify::locate(self.settings.spotify_path.as_deref()) {
            Ok(install) => {
                self.remember(&install);
                Some(install)
            }
            Err(err) => {
                tracing::debug!("{err}");
                self.install = None;
                None
            }
        }
    }

    /// Reads the version again: an update changes it.
    fn refreshed(&mut self, install: SpotifyInstall) -> SpotifyInstall {
        match SpotifyInstall::from_dir(&install.root) {
            Some(mut fresh) => {
                // Keep the kind found by `locate`, which knows the Store version.
                fresh.kind = install.kind;
                self.remember(&fresh);
                fresh
            }
            None => install,
        }
    }

    fn remember(&mut self, install: &SpotifyInstall) {
        self.install = Some(install.clone());
        if self.status.spotify_version != install.version {
            self.status.spotify_version = install.version.clone();
            self.save();
        }
    }

    /// The interface is in a settled state again: news about the next
    /// update must be reported again.
    fn new_episode(&mut self) {
        if !self.status.seen.is_empty() {
            self.status.seen.clear();
            self.save();
        }
    }

    /// True the first time `key` is seen in the current episode.
    fn first_time(&mut self, key: &str) -> bool {
        if self.status.seen.iter().any(|seen| seen == key) {
            return false;
        }
        self.status.seen.push(key.to_owned());
        let excess = self.status.seen.len().saturating_sub(MAX_SEEN);
        self.status.seen.drain(..excess);
        self.save();
        true
    }

    fn set_phase(&mut self, phase: Phase) {
        if self.status.phase != phase {
            tracing::info!("now: {}", phase.describe(Lang::En));
            self.status.phase = phase;
            self.save();
        }
    }

    fn save(&self) {
        let mut status = self.status.clone();
        status.updated_at = backup::now();
        if let Err(err) = write_status(&self.paths, &status) {
            tracing::warn!("{err}");
        }
    }

    fn log(&self, kind: EventKind) {
        tracing::info!("{}", kind.describe(Lang::En));
        if let Err(err) = self.events.append(kind) {
            tracing::warn!("{err}");
        }
    }

    fn notify(&self, title: Text, body: Text) {
        let lang = self.lang();
        self.host.notify(title.get(lang), body.get(lang));
    }

    /// The language chosen in spacetify, or the system's.
    fn lang(&self) -> Lang {
        Config::load(&self.settings.dirs.config_file())
            .ok()
            .and_then(|config| config.language)
            .and_then(|tag| Lang::parse(&tag))
            .unwrap_or_else(Lang::detect)
    }
}

#[cfg(test)]
mod tests;
