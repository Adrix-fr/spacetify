//! Logging setup.
//!
//! Logs go to stderr so they never mix with command output. `SPACETIFY_LOG`
//! accepts the usual `tracing` filter syntax and overrides the verbosity.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;

/// The log file is set aside as `.old` once it is larger than this.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// `verbosity` is the number of `-v` flags: 0 = warnings, 1 = info, 2 = debug, 3+ = trace.
pub fn init(verbosity: u8) {
    let filter = filter(verbosity);

    // Fails only if a subscriber is already set, which is fine.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}

/// Same, into a file, with the time on each line. Used by the service,
/// which has no terminal. Keeps at most two files of about 1 MB.
pub fn init_file(path: &Path, verbosity: u8) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        fs::rename(path, path.with_extension("log.old"))?;
    }
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter(verbosity))
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_target(false)
        .try_init();
    Ok(())
}

fn filter(verbosity: u8) -> EnvFilter {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    EnvFilter::try_from_env("SPACETIFY_LOG").unwrap_or_else(|_| EnvFilter::new(level))
}
