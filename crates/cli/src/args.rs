//! Command-line arguments, with help text in English or French.

use std::path::PathBuf;

use clap::{ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use spacetify_core::Lang;

#[derive(Debug, Parser)]
#[command(
    name = "spacetify",
    version,
    about = "A reliable mod manager for the Spotify desktop client."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Spotify install folder, if it is not found automatically
    #[arg(long, global = true, value_name = "DIR")]
    pub spotify_path: Option<PathBuf>,

    /// Language of messages: en or fr
    #[arg(long, global = true, value_name = "LANG", value_parser = parse_lang)]
    pub lang: Option<Lang>,

    /// Show more details (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show where Spotify is and whether it has been modified
    Status,
    /// Back up Spotify's interface without changing anything
    Backup,
    /// Back up, then inject the mods into Spotify
    Apply(CloseArgs),
    /// Put Spotify back exactly as it was
    Restore(CloseArgs),
    /// Start Spotify
    Launch(LaunchArgs),
}

#[derive(Debug, Args)]
pub struct CloseArgs {
    /// Close Spotify without asking if it is running
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct LaunchArgs {
    /// Inject at runtime without touching any file (keeps this terminal busy)
    #[arg(long)]
    pub live: bool,

    #[command(flatten)]
    pub close: CloseArgs,
}

impl Cli {
    /// Parses the process arguments, showing help in `lang`.
    pub fn parse_in(lang: Lang) -> Self {
        let matches = Self::command_in(lang).get_matches();
        Self::from_arg_matches(&matches).unwrap_or_else(|err| err.exit())
    }

    pub fn command_in(lang: Lang) -> clap::Command {
        match lang {
            Lang::En => Self::command(),
            Lang::Fr => in_french(Self::command()),
        }
    }
}

/// Finds `--lang` before clap runs, so even `--help` uses the right language.
pub fn lang_from_args() -> Option<Lang> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--lang=") {
            return Lang::parse(value);
        }
        if arg == "--lang" {
            return args.next().and_then(|value| Lang::parse(&value));
        }
    }
    None
}

fn parse_lang(value: &str) -> Result<Lang, String> {
    Lang::parse(value).ok_or_else(|| format!("unsupported language `{value}` (use en or fr)"))
}

fn in_french(command: clap::Command) -> clap::Command {
    const YES: &str = "Fermer Spotify sans demander s'il est ouvert";
    command
        .about("Un gestionnaire de mods fiable pour le client desktop de Spotify.")
        .mut_arg("spotify_path", |a| {
            a.help("Dossier de Spotify, s'il n'est pas trouvé automatiquement")
        })
        .mut_arg("lang", |a| a.help("Langue des messages : en ou fr"))
        .mut_arg("verbose", |a| {
            a.help("Afficher plus de détails (-v, -vv, -vvv)")
        })
        .mut_subcommand("status", |c| {
            c.about("Afficher où est Spotify et s'il a été modifié")
        })
        .mut_subcommand("backup", |c| {
            c.about("Sauvegarder l'interface de Spotify sans rien modifier")
        })
        .mut_subcommand("apply", |c| {
            c.about("Sauvegarder, puis injecter les mods dans Spotify")
                .mut_arg("yes", |a| a.help(YES))
        })
        .mut_subcommand("restore", |c| {
            c.about("Remettre Spotify exactement comme avant")
                .mut_arg("yes", |a| a.help(YES))
        })
        .mut_subcommand("launch", |c| {
            c.about("Lancer Spotify")
                .mut_arg("live", |a| {
                    a.help("Injecter en direct sans toucher aux fichiers (occupe ce terminal)")
                })
                .mut_arg("yes", |a| a.help(YES))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_is_valid_in_both_languages() {
        Cli::command().debug_assert();
        in_french(Cli::command()).debug_assert();
    }

    #[test]
    fn parses_commands() {
        let cli = Cli::try_parse_from(["spacetify", "apply", "--yes", "--lang", "fr"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Apply(CloseArgs { yes: true }))
        ));
        assert_eq!(cli.lang, Some(Lang::Fr));
        assert!(Cli::try_parse_from(["spacetify", "status", "--lang", "de"]).is_err());
        assert!(
            Cli::try_parse_from(["spacetify"])
                .unwrap()
                .command
                .is_none()
        );
    }
}
