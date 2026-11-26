//! `spacetify` command-line tool.
//!
//! Without arguments it opens an interactive menu. Subcommands
//! (`spacetify apply`, …) are there for scripts and quick use.

mod args;
mod commands;
mod gallery_commands;
mod interactive;
mod menu;
mod mod_commands;
mod service_commands;
mod ui;

use std::process::ExitCode;

use spacetify_core::config::{AppDirs, Config};
use spacetify_core::service::ServicePaths;
use spacetify_core::{Lang, Result, logs};

use args::Cli;
use commands::Context;

fn main() -> ExitCode {
    ui::init();
    let early_lang = args::lang_from_args().unwrap_or_else(Lang::detect);
    let cli = Cli::parse_in(early_lang);
    logs::init(cli.verbose);

    let mut ctx = match context(&cli) {
        Ok(ctx) => ctx,
        Err(err) => return fail(early_lang, &err),
    };

    match cli.command {
        Some(command) => match commands::run(&ctx, command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(ctx.lang, &err),
        },
        None if ui::is_interactive() => match interactive::run(&mut ctx) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                ui::error(err);
                ExitCode::FAILURE
            }
        },
        None => {
            let _ = Cli::command_in(ctx.lang).print_help();
            ExitCode::SUCCESS
        }
    }
}

fn context(cli: &Cli) -> Result<Context> {
    let dirs = AppDirs::detect()?;
    let config = Config::load(&dirs.config_file())?;
    Ok(Context {
        lang: resolve_lang(cli.lang, &config),
        spotify_path: cli.spotify_path.clone().or(config.spotify_path),
        backups_dir: dirs.backups_dir(),
        config_file: dirs.config_file(),
        config_dir: dirs.config_dir.clone(),
        service: ServicePaths::new(&dirs),
        spotify_path_flag: cli.spotify_path.clone(),
    })
}

/// `--lang`, then `SPACETIFY_LANG`, then the config file, then the system language.
fn resolve_lang(flag: Option<Lang>, config: &Config) -> Lang {
    flag.or_else(|| {
        std::env::var("SPACETIFY_LANG")
            .ok()
            .and_then(|tag| Lang::parse(&tag))
    })
    .or_else(|| config.language.as_deref().and_then(Lang::parse))
    .unwrap_or_else(Lang::detect)
}

fn fail(lang: Lang, err: &spacetify_core::Error) -> ExitCode {
    ui::error(err.localized(lang));
    ExitCode::FAILURE
}
