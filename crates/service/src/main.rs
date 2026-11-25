//! `spacetify-service`: puts the mods back after a Spotify update.
//!
//! Started by `spacetify service enable` (now and at every login), it runs
//! in the background without a window and writes its log to
//! `<data>/service/service.log`. All the logic is in `spacetify_core::service`.
//!
//! It only ever uses the file method. It never starts Spotify with a
//! debugging port, never downloads anything, and never needs admin rights.
//!
//! Options: `--home DIR` (same as `SPACETIFY_HOME`), `--spotify-path DIR`,
//! `-v`/`-vv` for more details in the log.

// No console window: the service runs in the background. It still shows in
// the Task Manager as `spacetify-service.exe`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use spacetify_core::config::{AppDirs, Config};
use spacetify_core::logs;
use spacetify_core::service::ServicePaths;
use spacetify_core::service::daemon::{self, Settings};

#[derive(Debug, Default, PartialEq)]
struct Args {
    home: Option<PathBuf>,
    spotify_path: Option<PathBuf>,
    verbose: u8,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} needs a folder"))
        };
        match arg.to_string_lossy().as_ref() {
            "--home" => parsed.home = Some(value("--home")?),
            "--spotify-path" => parsed.spotify_path = Some(value("--spotify-path")?),
            "-v" | "--verbose" => parsed.verbose += 1,
            "-vv" => parsed.verbose += 2,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(parsed)
}

fn main() -> ExitCode {
    let args = match parse(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("spacetify-service: {err}");
            return ExitCode::FAILURE;
        }
    };
    let dirs = match &args.home {
        Some(home) => AppDirs::in_folder(home),
        None => match AppDirs::detect() {
            Ok(dirs) => dirs,
            Err(err) => {
                eprintln!("spacetify-service: {err}");
                return ExitCode::FAILURE;
            }
        },
    };
    let paths = ServicePaths::new(&dirs);
    if let Err(err) = logs::init_file(&paths.log_file(), args.verbose.max(1)) {
        eprintln!("spacetify-service: cannot write the log: {err}");
    }
    std::panic::set_hook(Box::new(|info| tracing::error!("crashed: {info}")));

    let spotify_path = args.spotify_path.or_else(|| {
        Config::load(&dirs.config_file())
            .ok()
            .and_then(|config| config.spotify_path)
    });
    match daemon::run(Settings { dirs, spotify_path }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse(list.iter().map(OsString::from))
    }

    #[test]
    fn parses_options() {
        assert_eq!(args(&[]).unwrap(), Args::default());
        let parsed = args(&[
            "--home",
            "C:\\my mods",
            "--spotify-path",
            "/opt/spotify",
            "-v",
        ])
        .unwrap();
        assert_eq!(parsed.home, Some(PathBuf::from("C:\\my mods")));
        assert_eq!(parsed.spotify_path, Some(PathBuf::from("/opt/spotify")));
        assert_eq!(parsed.verbose, 1);
        assert!(args(&["--home"]).is_err());
        assert!(args(&["--live"]).is_err());
    }
}
