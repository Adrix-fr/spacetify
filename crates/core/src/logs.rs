//! Logging setup.
//!
//! Logs go to stderr so they never mix with command output. `SPACETIFY_LOG`
//! accepts the usual `tracing` filter syntax and overrides the verbosity.

use tracing_subscriber::EnvFilter;

/// `verbosity` is the number of `-v` flags: 0 = warnings, 1 = info, 2 = debug, 3+ = trace.
pub fn init(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_env("SPACETIFY_LOG").unwrap_or_else(|_| EnvFilter::new(level));

    // Fails only if a subscriber is already set, which is fine.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}
