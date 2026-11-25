//! spacetify programs running from the install folder, which must close
//! before their files can be replaced or removed. The background service is
//! not listed: it is stopped and started again on its own.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use spacetify_core::setup::same_folder;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// A program that has to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Program {
    /// The app window (or its icon in the notification area).
    App,
    /// `spacetify` in a terminal.
    Terminal,
}

fn scan(dir: &Path) -> Vec<(Pid, Program)> {
    let Ok(mut system) = System::new() else {
        return Vec::new();
    };
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let own = std::process::id();
    system
        .processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != own)
        .filter_map(|(pid, process)| {
            let exe = process.exe()?;
            if !exe.parent().is_some_and(|parent| same_folder(parent, dir)) {
                return None;
            }
            let name = exe.file_name()?.to_string_lossy().to_ascii_lowercase();
            let program = match name.as_str() {
                "spacetify-app.exe" => Program::App,
                "spacetify.exe" => Program::Terminal,
                // The service and anything else.
                _ => return None,
            };
            Some((*pid, program))
        })
        .collect()
}

/// What runs from `dir`, each kind once.
pub fn running(dir: &Path) -> Vec<Program> {
    let mut programs: Vec<Program> = scan(dir).into_iter().map(|(_, program)| program).collect();
    programs.sort_by_key(|program| *program as u8);
    programs.dedup();
    programs
}

/// Closes everything that runs from `dir`. Returns whether all are gone.
/// The app saves each change as it is made, so nothing is lost.
pub fn close(dir: &Path) -> bool {
    let Ok(mut system) = System::new() else {
        return false;
    };
    for (pid, _) in scan(dir) {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if scan(dir).is_empty() {
            // Windows releases the files a moment after the process ends.
            thread::sleep(Duration::from_millis(300));
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}
