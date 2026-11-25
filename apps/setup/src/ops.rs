//! The install and the uninstall, step by step, as the window shows them.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use spacetify_core::config::AppDirs;
use spacetify_core::service::{self, ServicePaths};
use spacetify_core::setup;

use crate::failure::Failure;
use crate::files::{self, Stop};
use crate::payload::VERSION;
use crate::{environment, processes, registry, shortcuts};

/// A line of the step list in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    // Install.
    Files,
    Path,
    Notifications,
    Autostart,
    // Uninstall.
    Restore,
    Undo,
    Remove,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StepState {
    Active,
    Done,
    Failed,
}

/// Where the steps are told (the window, and the log file).
pub trait Report: Sync {
    fn step(&self, step: Step, state: StepState);
    /// Share of the work done, from 0 to 1.
    fn progress(&self, share: f64);
    fn log(&self, line: &str);
    /// From now on, stopping half way is no longer possible.
    fn committed(&self);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedStep {
    pub step: Step,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Outcome {
    Done,
    /// Installed, but some steps after the files failed: they can be run again.
    Partial {
        failed: Vec<FailedStep>,
    },
    Cancelled,
    /// Stopped, and everything is as it was before.
    Failed {
        failure: Failure,
        step: Step,
    },
}

fn service_paths() -> Result<ServicePaths, Failure> {
    AppDirs::detect()
        .map(|dirs| ServicePaths::new(&dirs))
        .map_err(Failure::other)
}

fn failed(report: &dyn Report, step: Step, failure: Failure) -> Outcome {
    report.log(&format!("{step:?} failed: {failure}"));
    report.step(step, StepState::Failed);
    Outcome::Failed { failure, step }
}

/// Installs (or repairs, or updates) the files of `archive` into `dir`.
pub fn install(dir: &Path, archive: &[u8], report: &dyn Report, cancel: &AtomicBool) -> Outcome {
    let first_install = !dir.join("spacetify-app.exe").exists();
    report.log(&format!(
        "Installing spacetify {VERSION} in {}",
        dir.display()
    ));
    report.step(Step::Files, StepState::Active);
    let staged = match files::extract(archive, dir, |share| report.progress(share * 0.7), cancel) {
        Ok(staged) => staged,
        Err(Stop::Cancelled) => {
            report.log("Cancelled: nothing was changed.");
            return Outcome::Cancelled;
        }
        Err(Stop::Failed(failure)) => return failed(report, Step::Files, failure),
    };
    report.committed();

    let paths = match service_paths() {
        Ok(paths) => paths,
        Err(failure) => {
            staged.discard();
            return failed(report, Step::Files, failure);
        }
    };
    // The service runs from the files about to be replaced.
    let was_running = service::is_running(&paths);
    if let Err(err) = service::stop(&paths) {
        staged.discard();
        return failed(report, Step::Files, Failure::other(err));
    }
    if let Err(failure) = staged.swap() {
        staged.discard();
        if was_running {
            let _ = service::start(&paths, &dir.join(service::EXECUTABLE), &[]);
        }
        return failed(report, Step::Files, failure);
    }
    staged.finish();
    report.log("Files in place.");
    report.progress(0.8);

    // The files are in place: from here, a step that fails is only reported,
    // and can be run again from the window.
    let mut failed_steps = Vec::new();
    let steps = [
        (Step::Files, 0.84),
        (Step::Path, 0.88),
        (Step::Notifications, 0.94),
        (Step::Autostart, 1.0),
    ];
    for (step, share) in steps {
        if step != Step::Files {
            report.step(step, StepState::Active);
        }
        let result = if step == Step::Files {
            finish_files(dir, first_install, staged.bytes)
        } else {
            run_step(step, dir, &paths, report)
        };
        match result {
            Ok(()) => report.step(step, StepState::Done),
            Err(message) => {
                report.log(&format!("{step:?} failed: {message}"));
                report.step(step, StepState::Failed);
                failed_steps.push(FailedStep { step, message });
            }
        }
        report.progress(share);
    }
    // Running before, but its startup entry is off: start it again all the same.
    if was_running && !service::is_running(&paths) {
        let _ = service::start(&paths, &dir.join(service::EXECUTABLE), &[]);
    }
    if failed_steps.is_empty() {
        report.log("Done.");
        Outcome::Done
    } else {
        Outcome::Partial {
            failed: failed_steps,
        }
    }
}

/// Shortcuts and the entry in Settings > Apps.
fn finish_files(dir: &Path, first_install: bool, size: u64) -> Result<(), String> {
    registry::remove_old_installer_key();
    shortcuts::create(dir, first_install).map_err(|err| format!("shortcuts: {err}"))?;
    registry::write(dir, VERSION, size).map_err(|err| format!("Settings > Apps: {err}"))
}

/// Runs one of the steps that come after the files.
pub fn run_step(
    step: Step,
    dir: &Path,
    paths: &ServicePaths,
    report: &dyn Report,
) -> Result<(), String> {
    match step {
        Step::Files => finish_files(dir, false, files::installed_size(dir)),
        Step::Path => {
            setup::add_to_path(dir).map_err(|err| err.to_string())?;
            environment::announce_change();
            Ok(())
        }
        Step::Notifications => setup::register_notifications(dir)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        Step::Autostart => {
            let lines = setup::setup_autostart(dir, paths).map_err(|err| err.to_string())?;
            for line in lines {
                report.log(&line);
            }
            Ok(())
        }
        Step::Restore | Step::Undo | Step::Remove => Ok(()),
    }
}

/// Runs the steps that failed again. Returns those that still fail.
pub fn retry(dir: &Path, steps: &[Step], report: &dyn Report) -> Vec<FailedStep> {
    let paths = match service_paths() {
        Ok(paths) => paths,
        Err(failure) => {
            return steps
                .iter()
                .map(|&step| FailedStep {
                    step,
                    message: failure.to_string(),
                })
                .collect();
        }
    };
    steps
        .iter()
        .filter_map(|&step| {
            report.step(step, StepState::Active);
            match run_step(step, dir, &paths, report) {
                Ok(()) => {
                    report.step(step, StepState::Done);
                    None
                }
                Err(message) => {
                    report.step(step, StepState::Failed);
                    report.log(&format!("{step:?} failed again: {message}"));
                    Some(FailedStep { step, message })
                }
            }
        })
        .collect()
}

/// Removes spacetify from `dir`. Mods, settings and backups stay.
pub fn uninstall(dir: &Path, restore: bool, report: &dyn Report) -> Outcome {
    report.log(&format!("Uninstalling spacetify from {}", dir.display()));
    report.committed();
    let cli = dir.join("spacetify.exe");
    if restore && cli.is_file() {
        report.step(Step::Restore, StepState::Active);
        match run_hidden(&cli, &["restore", "--yes"]) {
            Ok(output) => report.log(&output),
            Err(message) => return failed(report, Step::Restore, Failure::other(message)),
        }
        report.step(Step::Restore, StepState::Done);
        report.progress(0.35);
    }

    report.step(Step::Undo, StepState::Active);
    let paths = match service_paths() {
        Ok(paths) => paths,
        Err(failure) => return failed(report, Step::Undo, failure),
    };
    match setup::before_uninstall(dir, &paths) {
        Ok(lines) => lines.iter().for_each(|line| report.log(line)),
        Err(err) => return failed(report, Step::Undo, Failure::other(err)),
    }
    environment::announce_change();
    report.step(Step::Undo, StepState::Done);
    report.progress(0.65);

    report.step(Step::Remove, StepState::Active);
    // In case one was opened again in the meantime.
    processes::close(dir);
    if let Err(failure) = files::remove_installed(dir) {
        return failed(report, Step::Remove, failure);
    }
    shortcuts::remove();
    if let Err(err) = registry::remove() {
        report.log(&format!("Settings > Apps entry: {err}"));
    }
    // The app window's cache (WebView2), not settings: those are in config.toml.
    if let Some(local) = dirs::data_local_dir() {
        let _ = std::fs::remove_dir_all(local.join(setup::APP_ID));
    }
    files::remove_dir_if_empty(dir);
    report.step(Step::Remove, StepState::Done);
    report.progress(1.0);
    report.log("Done.");
    Outcome::Done
}

/// Runs a spacetify command without a console window. Returns its output,
/// or the end of it as the error.
fn run_hidden(program: &Path, args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new(program)
        .args(args)
        .env("NO_COLOR", "1")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("{}: {err}", program.display()))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .trim()
    .to_owned();
    if output.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}
