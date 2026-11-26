//! Core of spacetify.
//!
//! Everything that touches Spotify lives here: finding the install, backing up
//! its interface, detecting changes made by other tools and injecting mods.
//! The CLI and the background service are thin layers on top, and so will
//! be the desktop app.

pub mod backup;
pub mod config;
pub mod detect;
pub mod doctor;
pub mod errors;
pub mod gallery;
pub mod i18n;
pub mod logs;
pub mod mods;
pub mod patcher;
pub mod service;
pub mod setup;
pub mod spicetify;
pub mod spotify;

#[cfg(test)]
mod testing;

pub use errors::{Error, Result};
pub use i18n::Lang;

/// Used for folder names, archive paths and injected element ids.
pub const APP_NAME: &str = "spacetify";
