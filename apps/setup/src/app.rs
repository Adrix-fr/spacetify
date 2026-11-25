//! The window: what it asks the Rust side, and what it is told back.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use spacetify_core::Lang;
use spacetify_core::config::{AppDirs, Config};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use crate::ops::{self, FailedStep, Outcome, Report, Step, StepState};
use crate::payload::{self, VERSION};
use crate::processes::{self, Program};
use crate::registry::{self, Installed};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Mode {
    Install,
    Uninstall,
}

/// How this installer's version compares with the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Relation {
    /// Nothing installed.
    Fresh,
    Older,
    Same,
    Newer,
    /// Installed, version unknown.
    Unknown,
}

struct Setup {
    mode: Mode,
    dir: PathBuf,
    installed: Option<Installed>,
    lang: Lang,
    log_file: PathBuf,
    log: Mutex<Option<File>>,
    busy: AtomicBool,
    cancellable: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    mode: Mode,
    lang: &'static str,
    version: &'static str,
    installed_version: Option<String>,
    relation: Relation,
    dir: String,
}

impl Setup {
    fn relation(&self) -> Relation {
        if self.mode == Mode::Uninstall {
            return Relation::Same;
        }
        let Some(installed) = &self.installed else {
            return if self.dir.join("spacetify-app.exe").exists() {
                Relation::Unknown
            } else {
                Relation::Fresh
            };
        };
        let ours = semver::Version::parse(VERSION).ok();
        let theirs = installed
            .version
            .as_deref()
            .and_then(|version| semver::Version::parse(version.trim()).ok());
        match (ours, theirs) {
            (Some(ours), Some(theirs)) if theirs < ours => Relation::Older,
            (Some(ours), Some(theirs)) if theirs > ours => Relation::Newer,
            (Some(_), Some(_)) => Relation::Same,
            _ => Relation::Unknown,
        }
    }

    fn write_log(&self, line: &str) {
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        if log.is_none() {
            *log = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_file)
                .ok();
        }
        if let Some(file) = log.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Tells the window how the work goes.
struct Window<'a> {
    app: &'a AppHandle,
    setup: &'a Setup,
}

#[derive(Clone, Serialize)]
struct StepEvent {
    step: Step,
    state: StepState,
}

impl Report for Window<'_> {
    fn step(&self, step: Step, state: StepState) {
        let _ = self.app.emit("setup:step", StepEvent { step, state });
    }

    fn progress(&self, share: f64) {
        let _ = self.app.emit("setup:progress", share.clamp(0.0, 1.0));
    }

    fn log(&self, line: &str) {
        self.setup.write_log(line);
    }

    fn committed(&self) {
        self.setup.cancellable.store(false, Ordering::SeqCst);
        let _ = self.app.emit("setup:committed", ());
    }
}

#[tauri::command]
fn context(setup: State<'_, Setup>) -> Context {
    Context {
        mode: setup.mode,
        lang: match setup.lang {
            Lang::Fr => "fr",
            Lang::En => "en",
        },
        version: VERSION,
        installed_version: setup
            .installed
            .as_ref()
            .and_then(|installed| installed.version.clone()),
        relation: setup.relation(),
        dir: setup.dir.display().to_string(),
    }
}

#[tauri::command(async)]
fn running(setup: State<'_, Setup>) -> Result<Vec<Program>, String> {
    Ok(processes::running(&setup.dir))
}

#[tauri::command(async)]
fn close_running(setup: State<'_, Setup>) -> Result<bool, String> {
    Ok(processes::close(&setup.dir))
}

/// Starts the install (`uninstall` false) or the uninstall in the
/// background; the window hears about it through events.
#[tauri::command]
fn start(app: AppHandle, setup: State<'_, Setup>, uninstall: bool, restore: bool) -> bool {
    if setup.busy.swap(true, Ordering::SeqCst) {
        return false;
    }
    setup.cancel.store(false, Ordering::SeqCst);
    setup.cancellable.store(!uninstall, Ordering::SeqCst);
    std::thread::spawn(move || {
        let setup = app.state::<Setup>();
        let window = Window {
            app: &app,
            setup: &setup,
        };
        let outcome = if uninstall {
            ops::uninstall(&setup.dir, restore, &window)
        } else {
            match payload::archive() {
                Some(archive) => ops::install(&setup.dir, archive, &window, &setup.cancel),
                None => Outcome::Failed {
                    failure: crate::failure::Failure::other("this program has nothing to install"),
                    step: Step::Files,
                },
            }
        };
        setup.cancellable.store(false, Ordering::SeqCst);
        setup.busy.store(false, Ordering::SeqCst);
        let _ = app.emit("setup:done", outcome);
    });
    true
}

#[tauri::command(async)]
fn retry(
    app: AppHandle,
    setup: State<'_, Setup>,
    steps: Vec<Step>,
) -> Result<Vec<FailedStep>, String> {
    let window = Window {
        app: &app,
        setup: &setup,
    };
    Ok(ops::retry(&setup.dir, &steps, &window))
}

#[tauri::command]
fn cancel(setup: State<'_, Setup>) {
    if setup.cancellable.load(Ordering::SeqCst) {
        setup.cancel.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
fn launch_app(setup: State<'_, Setup>) -> Result<(), String> {
    Command::new(setup.dir.join("spacetify-app.exe"))
        .current_dir(&setup.dir)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn open_log(app: AppHandle, setup: State<'_, Setup>) -> Result<(), String> {
    setup.write_log("");
    app.opener()
        .open_path(setup.log_file.to_string_lossy(), None::<&str>)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn quit(app: AppHandle, setup: State<'_, Setup>) {
    if !setup.busy.load(Ordering::SeqCst) {
        app.exit(0);
    }
}

/// `%TEMP%\spacetify-setup\<pid>`: the window's cache and the log, so
/// nothing is left in the user's folders. Older ones are removed.
fn work_dir() -> PathBuf {
    let base = std::env::temp_dir().join("spacetify-setup");
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.filter_map(Result::ok) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    let dir = base.join(std::process::id().to_string());
    let _ = fs::create_dir_all(&dir);
    dir
}

/// The language chosen in spacetify, else the system's.
fn language() -> Lang {
    AppDirs::detect()
        .ok()
        .and_then(|dirs| Config::load(&dirs.config_file()).ok())
        .and_then(|config| config.language)
        .and_then(|tag| Lang::parse(&tag))
        .unwrap_or_else(Lang::detect)
}

fn default_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(spacetify_core::APP_NAME)
}

/// The uninstaller cannot delete itself while it runs: it starts a copy of
/// itself from the temporary folder, which does the work. Returns whether
/// the copy started (then this one ends).
fn hand_over_to_copy(work: &Path, dir: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let copy = work.join("uninstall.exe");
    fs::copy(&exe, &copy).is_ok() && Command::new(&copy).arg("--dir").arg(dir).spawn().is_ok()
}

pub fn run() {
    let mode = if payload::archive().is_some() {
        Mode::Install
    } else {
        Mode::Uninstall
    };
    let args: Vec<String> = std::env::args().collect();
    let dir_arg = args
        .windows(2)
        .find(|pair| pair[0] == "--dir")
        .map(|pair| PathBuf::from(&pair[1]));
    let installed = registry::read();
    let work = work_dir();

    let dir = match mode {
        Mode::Install => installed
            .as_ref()
            .map(|installed| installed.dir.clone())
            .unwrap_or_else(default_dir),
        Mode::Uninstall => match dir_arg {
            Some(dir) => dir,
            None => {
                let own_dir = std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(Path::to_path_buf));
                match own_dir {
                    Some(dir) => {
                        if hand_over_to_copy(&work, &dir) {
                            return;
                        }
                        dir
                    }
                    None => installed
                        .as_ref()
                        .map(|installed| installed.dir.clone())
                        .unwrap_or_else(default_dir),
                }
            }
        },
    };

    let lang = language();
    let title = match (mode, lang) {
        (Mode::Install, Lang::Fr) => "Installation de spacetify",
        (Mode::Install, Lang::En) => "spacetify setup",
        (Mode::Uninstall, Lang::Fr) => "Désinstallation de spacetify",
        (Mode::Uninstall, Lang::En) => "Uninstall spacetify",
    };
    let setup = Setup {
        mode,
        dir,
        installed,
        lang,
        log_file: work.join("setup.log"),
        log: Mutex::new(None),
        busy: AtomicBool::new(false),
        cancellable: AtomicBool::new(false),
        cancel: AtomicBool::new(false),
    };

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(setup)
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title(title)
                .inner_size(600.0, 400.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .shadow(true)
                .center()
                .background_color(tauri::webview::Color(14, 14, 15, 255))
                .data_directory(work.join("webview"))
                .build()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing while working: the window asks first, or waits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let setup = window.state::<Setup>();
                if setup.busy.load(Ordering::SeqCst) {
                    api.prevent_close();
                    if setup.cancellable.load(Ordering::SeqCst) {
                        let _ = window.emit("setup:close-requested", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            context,
            running,
            close_running,
            start,
            retry,
            cancel,
            launch_app,
            open_log,
            quit,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        tracing::error!("could not open the installer window: {err}");
        std::process::exit(1);
    }
}
