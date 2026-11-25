//! The files to install, zipped into the installer by `build.rs`.

/// The zip archive, or `None` in the uninstaller.
pub fn archive() -> Option<&'static [u8]> {
    #[cfg(payload)]
    {
        Some(include_bytes!(env!("SPACETIFY_PAYLOAD_ZIP")))
    }
    #[cfg(not(payload))]
    {
        None
    }
}

/// Version of spacetify this program installs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
