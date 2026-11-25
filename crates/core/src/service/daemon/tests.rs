//! The service on fake Spotify installs, with a fake clock and a fake
//! process list. Nothing here touches the real Spotify.

use std::cell::RefCell;
use std::fs;

use super::*;
use crate::mods::ModKind;
use crate::patcher::{self, Payload};
use crate::service::events::Event;
use crate::testing::{ORIGINAL_INDEX, write_extension, write_spa};

#[derive(Default)]
struct FakeHost {
    busy: RefCell<Option<String>>,
    shown: RefCell<Vec<(String, String)>>,
    buttons: RefCell<Vec<String>>,
    closed: RefCell<usize>,
    launched: RefCell<usize>,
}

impl Host for &FakeHost {
    fn busy(&self) -> Result<Option<String>> {
        Ok(self.busy.borrow().clone())
    }

    fn notify(&self, title: &str, body: &str) {
        self.shown.borrow_mut().push((title.into(), body.into()));
    }

    fn offer_restart(&self, title: &str, body: &str, button: &str) {
        self.notify(title, body);
        self.buttons.borrow_mut().push(button.into());
    }

    fn close_spotify(&self) -> Result<()> {
        *self.closed.borrow_mut() += 1;
        self.set_busy(None);
        Ok(())
    }

    fn launch_spotify(&self, _install: &SpotifyInstall) -> Result<()> {
        *self.launched.borrow_mut() += 1;
        self.set_busy(Some("Spotify.exe"));
        Ok(())
    }
}

impl FakeHost {
    fn set_busy(&self, process: Option<&str>) {
        *self.busy.borrow_mut() = process.map(str::to_owned);
    }

    fn shown(&self) -> usize {
        self.shown.borrow().len()
    }

    fn last_body(&self) -> String {
        self.shown.borrow().last().unwrap().1.clone()
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    root: PathBuf,
    dirs: AppDirs,
    sources: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Spotify");
        fs::create_dir_all(root.join("Apps")).unwrap();
        write_spa(&root.join("Apps/xpui.spa"), ORIGINAL_INDEX);
        let dirs = AppDirs::in_folder(dir.path().join("home"));
        let config = Config {
            language: Some("en".into()),
            ..Config::default()
        };
        config.save(&dirs.config_file()).unwrap();
        Self {
            sources: dir.path().join("sources"),
            root,
            dirs,
            _dir: dir,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            dirs: self.dirs.clone(),
            spotify_path: Some(self.root.clone()),
        }
    }

    fn install(&self) -> SpotifyInstall {
        SpotifyInstall::from_dir(&self.root).unwrap()
    }

    fn store(&self) -> BackupStore {
        BackupStore::new(self.dirs.backups_dir())
    }

    fn spa(&self) -> PathBuf {
        self.root.join("Apps/xpui.spa")
    }

    fn sha(&self) -> String {
        backup::sha256_file(&self.spa()).unwrap()
    }

    fn config(&self) -> Config {
        Config::load(&self.dirs.config_file()).unwrap()
    }

    /// Installs and turns on an extension that relies on `selectors`.
    fn add_extension(&self, id: &str, selectors: &[&str]) {
        let dir = self.sources.join(id);
        write_extension(&dir, id, selectors);
        let mods = self.dirs.mods();
        mods.install(&dir).unwrap();
        let mut config = self.config();
        mods.enable(&mut config, id).unwrap();
        config.save(&self.dirs.config_file()).unwrap();
    }

    /// What `spacetify apply` does.
    fn apply(&self) {
        let active = self.dirs.mods().active(&self.config()).unwrap();
        patcher::apply_unchecked(&self.install(), &self.store(), &Payload::new(&active)).unwrap();
    }

    /// What a Spotify update does: a new original interface.
    fn update_spotify(&self, name: &str) {
        write_spa(&self.spa(), &ORIGINAL_INDEX.replace("Spotify", name));
    }

    /// Same, with a given modification time, so each write is told apart
    /// even when the clock does not move between them.
    fn update_spotify_at(&self, name: &str, second: u64) {
        self.update_spotify(name);
        fs::File::options()
            .write(true)
            .open(self.spa())
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + second))
            .unwrap();
    }

    fn state(&self) -> UiState {
        detect::inspect(&self.install()).unwrap().state
    }

    fn events(&self) -> Vec<Event> {
        EventLog::new(ServicePaths::new(&self.dirs).events_file())
            .recent(100)
            .unwrap()
    }

    fn has_event(&self, matches: impl Fn(&EventKind) -> bool) -> bool {
        self.events().iter().any(|event| matches(&event.kind))
    }
}

/// Advances the fake clock one second at a time.
fn run_for<H: Host>(daemon: &mut Daemon<H>, clock: &mut Instant, seconds: u64) {
    for _ in 0..seconds {
        *clock += Duration::from_secs(1);
        daemon.tick(*clock);
    }
}

fn phase<H: Host>(daemon: &Daemon<H>) -> &Phase {
    &daemon.status.phase
}

#[test]
fn repairs_after_an_update_once_spotify_is_closed() {
    let s = Setup::new();
    let host = FakeHost::default();
    let old_original = s.sha();
    s.apply();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    daemon.tick(clock);
    assert_eq!(phase(&daemon), &Phase::Watching);

    // Spotify updates itself and restarts with the new interface.
    s.update_spotify("Spotify 2");
    let new_original = s.sha();
    host.set_busy(Some("Spotify.exe"));
    daemon.changed(clock);
    run_for(&mut daemon, &mut clock, 60);
    assert!(matches!(phase(&daemon), Phase::WaitingForSpotify { .. }));
    assert_eq!(
        s.sha(),
        new_original,
        "nothing is written while Spotify runs"
    );
    assert_eq!(host.shown(), 1, "one notification, not one per check");
    assert!(host.last_body().contains("close Spotify"));
    assert_eq!(*host.buttons.borrow(), ["Restart Spotify"]);

    // Closed for only two seconds, then opened again: that is enough.
    host.set_busy(None);
    run_for(&mut daemon, &mut clock, 2);
    host.set_busy(Some("Spotify.exe"));

    let UiState::Patched(marker) = s.state() else {
        panic!("not repaired: {:?}", phase(&daemon));
    };
    assert_eq!(marker.original_sha256, new_original);
    assert_eq!(phase(&daemon), &Phase::Watching);
    assert_eq!(host.shown(), 2);
    assert!(host.last_body().contains("your mods are back"));

    // Both originals are kept, and `restore` gives back the new one.
    let store = s.store();
    assert!(store.find(&old_original).unwrap().is_some());
    assert!(store.find(&new_original).unwrap().is_some());
    assert!(s.has_event(|e| matches!(e, EventKind::UpdateDetected { .. })));
    assert!(
        s.has_event(|e| matches!(e, EventKind::BackedUp { sha256, .. } if *sha256 == new_original))
    );
    assert!(s.has_event(|e| matches!(e, EventKind::Repaired { .. })));
    patcher::restore_unchecked(&s.install(), &store).unwrap();
    assert_eq!(s.sha(), new_original);

    // Nothing more happens afterwards.
    run_for(&mut daemon, &mut clock, 120);
    assert_eq!(host.shown(), 2);
}

#[test]
fn turns_off_mods_that_no_longer_fit() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.add_extension("sturdy", &[]);
    s.add_extension("fragile", &[r#"[data-testid="now-playing-bar"]"#]);
    s.apply();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    s.update_spotify("Spotify 2");
    run_for(&mut daemon, &mut clock, 30);

    assert!(matches!(s.state(), UiState::Patched(_)));
    assert_eq!(s.config().extensions, ["sturdy"]);
    assert!(s.has_event(|e| matches!(
        e,
        EventKind::ModTurnedOff { kind: ModKind::Extension, id, missing: 1 } if id == "fragile"
    )));
    assert!(s.has_event(|e| matches!(e, EventKind::Repaired { extensions: 1, .. })));
    assert!(host.last_body().contains("fragile"));
}

#[test]
fn turns_off_mods_that_were_deleted_by_hand() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.add_extension("gone", &[]);
    s.apply();
    fs::remove_dir_all(s.dirs.mods().path_of(ModKind::Extension, "gone")).unwrap();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    s.update_spotify("Spotify 2");
    run_for(&mut daemon, &mut clock, 30);

    assert!(matches!(s.state(), UiState::Patched(_)));
    assert!(s.config().extensions.is_empty());
}

#[test]
fn leaves_spotify_alone_after_restore() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    patcher::restore_unchecked(&s.install(), &s.store()).unwrap();
    let original = s.sha();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 60);
    assert_eq!(phase(&daemon), &Phase::NothingApplied);
    assert_eq!(s.sha(), original);
    assert_eq!(host.shown(), 0);
}

#[test]
fn never_touches_spicetify() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    let html = ORIGINAL_INDEX.replace(
        "</body>",
        "<script src='helper/spicetifyWrapper.js'></script></body>",
    );
    write_spa(&s.spa(), &html);
    let before = s.sha();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 30);
    daemon.check_now(clock);
    run_for(&mut daemon, &mut clock, 30);

    assert!(matches!(phase(&daemon), Phase::Blocked { .. }));
    assert_eq!(s.sha(), before);
    assert_eq!(host.shown(), 1);
    assert!(host.last_body().contains("Spicetify"));
}

#[test]
fn waits_while_the_update_is_still_writing() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    // The file changes every few seconds, as while an installer runs.
    for step in 0..6_u64 {
        s.update_spotify_at("Spotify 2", step);
        daemon.changed(clock);
        run_for(&mut daemon, &mut clock, 6);
        assert_eq!(s.state(), UiState::Original, "touched while changing");
    }
    run_for(&mut daemon, &mut clock, 25);
    assert!(matches!(s.state(), UiState::Patched(_)));
}

#[test]
fn does_not_repeat_news_after_a_restart() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    s.update_spotify("Spotify 2");
    host.set_busy(Some("Spotify.exe"));

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 20);
    assert_eq!(host.shown(), 1);
    daemon.stop();
    drop(daemon);

    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 20);
    assert_eq!(host.shown(), 1);

    // A second update later is news again.
    host.set_busy(None);
    run_for(&mut daemon, &mut clock, 30);
    assert!(matches!(s.state(), UiState::Patched(_)));
    assert_eq!(host.shown(), 2);
    patcher::simulate_update_unchecked(&s.install(), &s.store()).unwrap();
    host.set_busy(Some("Spotify.exe"));
    daemon.changed(clock);
    run_for(&mut daemon, &mut clock, 20);
    assert_eq!(host.shown(), 3);
}

#[test]
fn simulated_update_is_repaired_with_the_existing_backup() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    let patched = s.sha();
    patcher::simulate_update_unchecked(&s.install(), &s.store()).unwrap();

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 30);
    assert_eq!(s.sha(), patched, "same mods, same bytes");
    assert!(!s.has_event(|e| matches!(e, EventKind::BackedUp { .. })));
}

#[test]
fn reports_a_missing_spotify_without_failing() {
    let s = Setup::new();
    let host = FakeHost::default();
    fs::remove_dir_all(&s.root).unwrap();
    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 5);
    assert_eq!(phase(&daemon), &Phase::SpotifyNotFound);
    assert_eq!(daemon.due(), clock - Duration::from_secs(4) + FALLBACK);
}

#[test]
fn only_changes_to_the_interface_matter() {
    let apps = Path::new("/spotify/Apps");
    let paths = |p: &str| vec![PathBuf::from(p)];
    assert!(concerns_interface(&paths("/spotify/Apps/xpui.spa"), apps));
    assert!(concerns_interface(
        &paths("/spotify/Apps/xpui.spa.spacetify-tmp"),
        apps
    ));
    assert!(concerns_interface(&paths("/spotify/Apps"), apps));
    assert!(!concerns_interface(&paths("/spotify/Apps/login.spa"), apps));
    assert!(!concerns_interface(&paths("/spotify/prefs"), apps));
}

#[test]
fn waits_if_the_updater_changes_the_file_after_spotify_closes() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    s.update_spotify_at("Spotify 2", 1);
    host.set_busy(Some("Spotify.exe"));

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 20);
    host.set_busy(None);
    s.update_spotify_at("Spotify 3", 2);
    run_for(&mut daemon, &mut clock, 5);
    assert_eq!(s.state(), UiState::Original, "touched while changing");
    run_for(&mut daemon, &mut clock, 20);
    assert!(matches!(s.state(), UiState::Patched(_)));
}

#[test]
fn restart_button_repairs_and_reopens_spotify() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    s.update_spotify("Spotify 2");
    host.set_busy(Some("Spotify.exe"));

    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 20);
    assert!(matches!(phase(&daemon), Phase::WaitingForSpotify { .. }));

    daemon.restart_spotify(clock);
    run_for(&mut daemon, &mut clock, 1);
    assert!(matches!(s.state(), UiState::Patched(_)));
    assert_eq!(*host.closed.borrow(), 1);
    assert_eq!(*host.launched.borrow(), 1);
    assert!(host.last_body().contains("opening again"));
    assert!(s.has_event(|e| *e == EventKind::SpotifyRestarted));

    // Spotify runs again with the mods: nothing more to do.
    run_for(&mut daemon, &mut clock, 60);
    assert_eq!(phase(&daemon), &Phase::Watching);
    assert_eq!(*host.launched.borrow(), 1);
}

#[test]
fn restart_button_does_nothing_when_nothing_waits() {
    let s = Setup::new();
    let host = FakeHost::default();
    s.apply();
    let mut clock = Instant::now();
    let mut daemon = Daemon::new(s.settings(), &host, clock);
    run_for(&mut daemon, &mut clock, 2);
    daemon.restart_spotify(clock);
    run_for(&mut daemon, &mut clock, 5);
    assert_eq!(*host.closed.borrow(), 0);
    assert_eq!(*host.launched.borrow(), 0);
}
