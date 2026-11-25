//! The Terminal tab: runs the `spacetify` command-line program that sits
//! next to the app, and streams what it prints to the window. Only that
//! program can be run from here, never a shell.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::PoisonError;
use std::thread;

use serde::Serialize;
use spacetify_core::tr;
use tauri::{AppHandle, Emitter, State};

use crate::model::split_args;
use crate::state::{Failure, Shared, lang_code};

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    /// `out` or `err`.
    pub stream: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Exit {
    pub code: Option<i32>,
}

fn cli_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "spacetify.exe"
    } else {
        "spacetify"
    };
    let exe = std::env::current_exe().ok()?.with_file_name(name);
    exe.is_file().then_some(exe)
}

/// Starts `spacetify <line>`. Its output arrives as `terminal-line` events,
/// then a `terminal-exit` event.
#[tauri::command]
pub fn run_cli(app: AppHandle, state: State<'_, Shared>, line: String) -> Result<(), Failure> {
    let lang = state.lang();
    let mut guard = state
        .terminal
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(child) = guard.as_mut()
        && matches!(child.try_wait(), Ok(None))
    {
        return Err(Failure::busy(lang));
    }
    let program = cli_path().ok_or_else(|| {
        Failure::new(
            None,
            tr!(
                lang,
                "The spacetify command-line program was not found next to the app.",
                "Le programme en ligne de commande spacetify est introuvable à côté de l'appli."
            ),
        )
    })?;
    let mut args = split_args(&line);
    if args.is_empty() {
        // Without arguments it would open its menu, which needs a real terminal.
        args.push("--help".into());
    }
    let mut command = Command::new(program);
    command
        .args(&args)
        .args(["--lang", lang_code(lang)])
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|err| Failure::new(None, err.to_string()))?;
    let readers = [
        child.stdout.take().map(|out| stream(&app, "out", out)),
        child.stderr.take().map(|err| stream(&app, "err", err)),
    ];
    *guard = Some(child);
    drop(guard);

    let state = state.inner().clone();
    thread::spawn(move || {
        for reader in readers.into_iter().flatten() {
            let _ = reader.join();
        }
        let code = state
            .terminal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .and_then(|mut child| child.wait().ok())
            .and_then(|status| status.code());
        let _ = app.emit("terminal-exit", Exit { code });
    });
    Ok(())
}

fn stream(
    app: &AppHandle,
    name: &'static str,
    pipe: impl Read + Send + 'static,
) -> thread::JoinHandle<()> {
    let app = app.clone();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(text) = line else { break };
            let _ = app.emit("terminal-line", Line { stream: name, text });
        }
    })
}

/// Stops the program started from the Terminal tab (such as `launch --live`).
#[tauri::command]
pub fn stop_cli(state: State<'_, Shared>) {
    if let Some(child) = state
        .terminal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        let _ = child.kill();
    }
}
