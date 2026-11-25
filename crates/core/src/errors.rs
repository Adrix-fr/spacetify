//! Error type shared by the whole crate.
//!
//! `Display` gives the English message. Use [`Error::localized`] for the
//! user's language.

use std::io;
use std::path::{Path, PathBuf};

use crate::Lang;
use crate::mods::{ModIssue, ModKind};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Spotify was not found on this computer. If it is installed somewhere else, pass its folder with --spotify-path."
    )]
    SpotifyNotFound,

    #[error("{} does not look like a Spotify install (no Apps folder).", .0.display())]
    NotASpotifyFolder(PathBuf),

    #[error(
        "This Spotify install ({kind}) is read-only and cannot be modded. Install the regular version from spotify.com/download instead."
    )]
    ReadOnlyInstall { kind: &'static str },

    #[error(
        "Spotify is running. Close it first, or run the command again with --yes to close it automatically."
    )]
    SpotifyRunning,

    #[error(
        "Spotify has been modified by Spicetify, so nothing was changed.\n\
         - To keep Spicetify: there is nothing to do.\n\
         - To switch to spacetify: run `spicetify restore`, then `spacetify apply`."
    )]
    SpicetifyActive,

    #[error(
        "Spotify's interface was changed by an unknown tool ({0}). Nothing was changed to be safe."
    )]
    UnknownModification(String),

    #[error("Spotify's interface file is missing: {}. Reinstalling Spotify should fix it.", .0.display())]
    XpuiMissing(PathBuf),

    #[error("{} is not a valid archive: {reason}", .path.display())]
    InvalidArchive { path: PathBuf, reason: String },

    #[error(
        "No backup of the original interface was found (sha256 {sha256}). Restoring is not possible without one."
    )]
    NoBackup { sha256: String },

    #[error("The backup {} is damaged (its hash changed). Nothing was restored.", .0.display())]
    BackupCorrupted(PathBuf),

    #[error("Check failed after writing {}: expected sha256 {expected}, got {actual}.", .path.display())]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("Invalid config file {}: {reason}", .path.display())]
    Config { path: PathBuf, reason: String },

    #[error("Could not find the user folders of this system.")]
    NoUserDirs,

    #[error("Could not list running programs: {0}")]
    Processes(String),

    #[error("Could not close Spotify. Close it by hand and try again.")]
    CloseFailed,

    #[error("Could not start Spotify: {0}")]
    Launch(String),

    #[error("Live injection failed: {0}")]
    Live(String),

    #[error("{what} was refused: {issue}")]
    InvalidMod { what: String, issue: ModIssue },

    #[error(
        "No theme or extension named `{0}` is installed. `spacetify list` shows what is installed."
    )]
    ModNotFound(String),

    #[error("`{0}` is an extension, not a theme.")]
    NotATheme(String),

    #[error("The id `{id}` is already used by an installed {}. Ids must be unique.", .kind.label(Lang::En))]
    IdTaken { id: String, kind: ModKind },

    #[error("{} is already in spacetify's mods folder. Install from your own copy of the mod instead.", .0.display())]
    InstallFromStore(PathBuf),

    #[error("{0} problem(s) found with your mods for this version of Spotify. Details are above.")]
    Incompatible(usize),

    #[error("File error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>) -> impl FnOnce(io::Error) -> Self {
        let path = path.as_ref().to_path_buf();
        move |source| Self::Io { path, source }
    }

    /// Message in the requested language.
    pub fn localized(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.to_string(),
            Lang::Fr => self.french(),
        }
    }

    fn french(&self) -> String {
        match self {
            Self::SpotifyNotFound => "Spotify est introuvable sur cet ordinateur. S'il est installé ailleurs, indique son dossier avec --spotify-path.".into(),
            Self::NotASpotifyFolder(path) => format!("{} ne ressemble pas à une installation de Spotify (pas de dossier Apps).", path.display()),
            Self::ReadOnlyInstall { kind } => format!("Cette installation de Spotify ({kind}) est en lecture seule et ne peut pas être modifiée. Installe plutôt la version classique depuis spotify.com/download."),
            Self::SpotifyRunning => "Spotify est ouvert. Ferme-le d'abord, ou relance la commande avec --yes pour le fermer automatiquement.".into(),
            Self::SpicetifyActive => "Spotify a été modifié par Spicetify, donc rien n'a été changé.\n\
                 - Pour garder Spicetify : il n'y a rien à faire.\n\
                 - Pour passer à spacetify : lance `spicetify restore`, puis `spacetify apply`."
                .into(),
            Self::UnknownModification(what) => format!("L'interface de Spotify a été modifiée par un outil inconnu ({what}). Rien n'a été changé par sécurité."),
            Self::XpuiMissing(path) => format!("Le fichier de l'interface de Spotify est absent : {}. Réinstaller Spotify devrait régler le problème.", path.display()),
            Self::InvalidArchive { path, reason } => format!("{} n'est pas une archive valide : {reason}", path.display()),
            Self::NoBackup { sha256 } => format!("Aucune sauvegarde de l'interface d'origine n'a été trouvée (sha256 {sha256}). Impossible de restaurer sans elle."),
            Self::BackupCorrupted(path) => format!("La sauvegarde {} est abîmée (son empreinte a changé). Rien n'a été restauré.", path.display()),
            Self::HashMismatch { path, expected, actual } => format!("Vérification échouée après l'écriture de {} : sha256 attendu {expected}, obtenu {actual}.", path.display()),
            Self::Config { path, reason } => format!("Fichier de configuration invalide {} : {reason}", path.display()),
            Self::NoUserDirs => "Impossible de trouver les dossiers utilisateur de ce système.".into(),
            Self::Processes(reason) => format!("Impossible de lister les programmes ouverts : {reason}"),
            Self::CloseFailed => "Impossible de fermer Spotify. Ferme-le à la main puis réessaie.".into(),
            Self::Launch(reason) => format!("Impossible de lancer Spotify : {reason}"),
            Self::Live(reason) => format!("L'injection en direct a échoué : {reason}"),
            Self::InvalidMod { what, issue } => format!("{what} est refusé : {}", issue.localized(Lang::Fr)),
            Self::ModNotFound(id) => format!("Aucun thème ni extension nommé `{id}` n'est installé. `spacetify list` montre ce qui est installé."),
            Self::NotATheme(id) => format!("`{id}` est une extension, pas un thème."),
            Self::IdTaken { id, kind } => format!("L'identifiant `{id}` est déjà pris par un(e) {} installé(e). Chaque identifiant doit être unique.", kind.label(Lang::Fr)),
            Self::InstallFromStore(path) => format!("{} est déjà dans le dossier des mods de spacetify. Installe plutôt depuis ta propre copie du mod.", path.display()),
            Self::Incompatible(count) => format!("{count} problème(s) trouvé(s) avec tes mods pour cette version de Spotify. Le détail est au-dessus."),
            Self::Io { path, source } => format!("Erreur de fichier sur {} : {source}", path.display()),
        }
    }
}
