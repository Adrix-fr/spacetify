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
    /// List installed themes and extensions
    #[command(visible_alias = "ls")]
    List,
    /// Install a theme or an extension from a folder, and turn it on
    Install {
        /// Folder that contains theme.toml or extension.toml
        #[arg(value_name = "DIR")]
        path: PathBuf,
    },
    /// Delete an installed theme or extension
    Uninstall {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Turn on an extension, or make a theme the active one
    Enable {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Turn off an extension or the active theme
    Disable {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Choose the active theme (`none` for Spotify's own look)
    Theme {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Check that the active mods still match this version of Spotify
    Doctor,
    /// Automatic repair after Spotify updates (background service)
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Check a mod before sending it to the gallery, and the robot's steps
    Gallery {
        #[command(subcommand)]
        action: GalleryAction,
    },
    /// Run by the Windows installer of the desktop app
    #[command(hide = true)]
    Installer {
        #[command(subcommand)]
        step: InstallerStep,
    },
}

#[derive(Debug, Clone, Copy, Subcommand)]
pub enum InstallerStep {
    /// Add spacetify to the Path, name its notifications, move the startup entry here
    PostInstall,
    /// Stop the service and take back what post-install did
    PreUninstall,
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// Turn on automatic repair: start the service now and at every login
    Enable,
    /// Turn off automatic repair: stop the service and remove it from startup
    Disable,
    /// Start the service now, without changing what happens at login
    Start,
    /// Stop the service now, without changing what happens at login
    Stop,
    /// Show whether the service runs and what it is doing
    Status,
    /// Show what the service did lately
    Events {
        /// How many events to show
        #[arg(short = 'n', long, default_value_t = 15, value_name = "N")]
        count: usize,
    },
    /// Put Spotify's original interface back, as an update does, to test the repair
    SimulateUpdate(CloseArgs),
}

#[derive(Debug, Subcommand)]
pub enum GalleryAction {
    /// Look for anything hidden or risky in a mod, as the gallery's robot does
    Review {
        /// Folder of the mod
        #[arg(value_name = "DIR")]
        path: PathBuf,
        /// Copy of the repository, to compare with the version in the gallery
        #[arg(long, value_name = "DIR")]
        repo: Option<PathBuf>,
        /// Output of `clamscan` on the mod's folder
        #[arg(long, value_name = "FILE")]
        antivirus: Option<PathBuf>,
        /// Write the report as JSON
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
        /// Write the report for a GitHub comment
        #[arg(long, value_name = "FILE")]
        markdown: Option<PathBuf>,
        /// Where the files can be read, for links to each line ({id} is replaced)
        #[arg(long, value_name = "URL")]
        link_base: Option<String>,
    },
    /// Check a mod and put it in a .zip, ready for the request form
    Pack {
        /// Folder of the mod
        #[arg(value_name = "DIR")]
        path: PathBuf,
        /// Where to put the .zip (the current folder by default)
        #[arg(long, value_name = "DIR")]
        into: Option<PathBuf>,
    },
    /// Robot: read a request made with the form
    #[command(hide = true)]
    Form {
        /// The request's text
        #[arg(value_name = "FILE")]
        body: PathBuf,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
    /// Robot: open a .zip safely
    #[command(hide = true)]
    Unpack {
        #[arg(value_name = "ZIP")]
        archive: PathBuf,
        #[arg(value_name = "DIR")]
        into: PathBuf,
        /// Folder of the mod inside the .zip
        #[arg(long, value_name = "PATH")]
        folder: Option<String>,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
    /// Robot: put an accepted mod into the gallery's list
    #[command(hide = true)]
    Add {
        #[arg(value_name = "DIR")]
        path: PathBuf,
        /// Copy of the repository
        #[arg(long, value_name = "DIR")]
        repo: PathBuf,
        /// GitHub account of the author
        #[arg(long, value_name = "LOGIN")]
        owner: String,
        /// The request's text
        #[arg(long, value_name = "FILE")]
        form: PathBuf,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
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
        .mut_subcommand("list", |c| {
            c.about("Lister les thèmes et extensions installés")
        })
        .mut_subcommand("install", |c| {
            c.about("Installer un thème ou une extension depuis un dossier, et l'activer")
                .mut_arg("path", |a| {
                    a.help("Dossier qui contient theme.toml ou extension.toml")
                })
        })
        .mut_subcommand("uninstall", |c| {
            c.about("Supprimer un thème ou une extension installé")
        })
        .mut_subcommand("enable", |c| {
            c.about("Activer une extension, ou choisir un thème comme thème actif")
        })
        .mut_subcommand("disable", |c| {
            c.about("Désactiver une extension ou le thème actif")
        })
        .mut_subcommand("theme", |c| {
            c.about("Choisir le thème actif (`none` pour le look d'origine de Spotify)")
        })
        .mut_subcommand("doctor", |c| {
            c.about("Vérifier que les mods actifs correspondent encore à cette version de Spotify")
        })
        .mut_subcommand("service", |c| {
            c.about("Réparation automatique après les mises à jour de Spotify (service en arrière-plan)")
                .mut_subcommand("enable", |c| {
                    c.about("Activer la réparation automatique : lancer le service maintenant et à chaque démarrage")
                })
                .mut_subcommand("disable", |c| {
                    c.about("Désactiver la réparation automatique : arrêter le service et le retirer du démarrage")
                })
                .mut_subcommand("start", |c| {
                    c.about("Lancer le service maintenant, sans changer le démarrage")
                })
                .mut_subcommand("stop", |c| {
                    c.about("Arrêter le service maintenant, sans changer le démarrage")
                })
                .mut_subcommand("status", |c| {
                    c.about("Afficher si le service tourne et ce qu'il fait")
                })
                .mut_subcommand("events", |c| {
                    c.about("Afficher ce que le service a fait récemment")
                        .mut_arg("count", |a| a.help("Nombre d'événements à afficher"))
                })
                .mut_subcommand("simulate-update", |c| {
                    c.about("Remettre l'interface d'origine de Spotify, comme une mise à jour, pour tester la réparation")
                        .mut_arg("yes", |a| a.help(YES))
                })
        })
        .mut_subcommand("gallery", |c| {
            c.about("Vérifier un mod avant de l'envoyer à la galerie, et les étapes du robot")
                .mut_subcommand("review", |c| {
                    c.about("Chercher tout ce qui est caché ou risqué dans un mod, comme le robot de la galerie")
                        .mut_arg("path", |a| a.help("Dossier du mod"))
                        .mut_arg("repo", |a| {
                            a.help("Copie du dépôt, pour comparer avec la version de la galerie")
                        })
                        .mut_arg("antivirus", |a| {
                            a.help("Résultat de `clamscan` sur le dossier du mod")
                        })
                        .mut_arg("json", |a| a.help("Écrire le rapport en JSON"))
                        .mut_arg("markdown", |a| {
                            a.help("Écrire le rapport pour un commentaire GitHub")
                        })
                        .mut_arg("link_base", |a| {
                            a.help("Où lire les fichiers, pour un lien vers chaque ligne ({id} est remplacé)")
                        })
                })
                .mut_subcommand("pack", |c| {
                    c.about("Vérifier un mod et le mettre dans un .zip, prêt pour le formulaire de demande")
                        .mut_arg("path", |a| a.help("Dossier du mod"))
                        .mut_arg("into", |a| {
                            a.help("Où mettre le .zip (le dossier courant par défaut)")
                        })
                })
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
        let cli = Cli::try_parse_from(["spacetify", "install", "my theme"]).unwrap();
        assert!(
            matches!(cli.command, Some(Command::Install { path }) if path.as_os_str() == "my theme")
        );
        let cli = Cli::try_parse_from(["spacetify", "service", "events", "-n", "3"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Service {
                action: ServiceAction::Events { count: 3 }
            })
        ));
        assert!(Cli::try_parse_from(["spacetify", "service", "simulate-update", "--yes"]).is_ok());
        let cli = Cli::try_parse_from(["spacetify", "gallery", "review", "m", "--json", "r.json"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Gallery {
                action: GalleryAction::Review { json: Some(_), .. }
            })
        ));
        let cli = Cli::try_parse_from(["spacetify", "ls"]).unwrap();
        assert!(matches!(cli.command, Some(Command::List)));
        assert!(Cli::try_parse_from(["spacetify", "theme"]).is_err());
        assert!(
            Cli::try_parse_from(["spacetify"])
                .unwrap()
                .command
                .is_none()
        );
    }
}
