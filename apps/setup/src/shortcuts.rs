//! Shortcuts in the Start menu and on the desktop, as the first installer
//! made them (same names, so an update replaces them).

use std::io;
use std::path::{Path, PathBuf};

const NAME: &str = "spacetify.lnk";

fn start_menu() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join(NAME),
    )
}

fn desktop() -> Option<PathBuf> {
    Some(dirs::desktop_dir()?.join(NAME))
}

/// Makes the shortcuts to the app in `dir`. The one on the desktop is made
/// on a first install only, or replaced if it is still there: someone who
/// deleted it does not get it back with each update.
pub fn create(dir: &Path, first_install: bool) -> io::Result<()> {
    let app = dir.join("spacetify-app.exe");
    if let Some(path) = start_menu() {
        make(&app, dir, &path)?;
    }
    if let Some(path) = desktop()
        && (first_install || path.exists())
    {
        make(&app, dir, &path)?;
    }
    Ok(())
}

/// Removes both shortcuts.
pub fn remove() {
    for path in [start_menu(), desktop()].into_iter().flatten() {
        let _ = std::fs::remove_file(path);
    }
}

fn make(app: &Path, dir: &Path, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut link = mslnk::ShellLink::new(app).map_err(io::Error::other)?;
    link.set_working_dir(Some(dir.to_string_lossy().into_owned()));
    link.set_icon_location(Some(app.to_string_lossy().into_owned()));
    link.set_name(Some(
        "A reliable mod manager for the Spotify desktop client.".to_owned(),
    ));
    link.create_lnk(path).map_err(io::Error::other)
}
