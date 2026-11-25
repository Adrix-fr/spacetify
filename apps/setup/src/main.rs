//! Installer and uninstaller of spacetify for Windows, with the same look as
//! the app. It installs for the current user only, in
//! `%LOCALAPPDATA%\spacetify` (the folder that also holds the backups: only
//! the files listed in [`files::ENTRIES`] are ever replaced or removed).
//!
//! One program, two builds (see `build.rs`): with the files to install
//! inside, it is the installer; without, it is the `uninstall.exe` that
//! Windows runs from Settings > Apps.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod environment;
#[cfg(windows)]
mod failure;
#[cfg(windows)]
mod files;
#[cfg(windows)]
mod ops;
#[cfg(windows)]
mod payload;
#[cfg(windows)]
mod processes;
#[cfg(windows)]
mod registry;
#[cfg(windows)]
mod shortcuts;

fn main() {
    #[cfg(windows)]
    app::run();
    #[cfg(not(windows))]
    eprintln!("The spacetify installer only runs on Windows.");
}
