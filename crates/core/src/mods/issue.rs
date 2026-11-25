//! Why a theme or an extension was refused, in English and French.

use std::fmt;

use crate::Lang;

/// A problem found while validating a mod. Shown to the user as is, so every
/// variant explains what to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModIssue {
    NoManifest,
    TwoManifests,
    ManifestSyntax(String),
    InvalidId(String),
    FolderMismatch {
        folder: String,
        id: String,
    },
    EmptyField(&'static str),
    FieldTooLong {
        field: &'static str,
        max: usize,
    },
    InvalidVersion(String),
    InvalidRequirement(String),
    NeedsNewerSpacetify {
        required: String,
        current: String,
    },
    MissingFile(String),
    FileTooLarge {
        file: String,
        max_kib: u64,
    },
    TooManyFiles {
        max: usize,
    },
    NotUtf8(String),
    Symlink(String),
    EmptyTheme,
    CssUnclosedComment {
        file: String,
        line: usize,
    },
    CssUnclosedString {
        file: String,
        line: usize,
    },
    CssUnbalanced {
        file: String,
        line: usize,
    },
    CssImport {
        file: String,
        line: usize,
    },
    BadAsset {
        url: String,
        reason: AssetProblem,
    },
    ColorsSyntax(String),
    UnknownColorSet(String),
    UnknownColorToken {
        set: String,
        token: String,
    },
    BadColor {
        set: String,
        token: String,
        value: String,
    },
    InvalidSettingName(String),
    BadSettingDefault {
        setting: String,
    },
    ChoiceWithoutChoices {
        setting: String,
    },
    InvalidSelector(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetProblem {
    OutsideAssets,
    NotFound,
    UnsupportedType,
}

impl ModIssue {
    pub fn localized(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.english(),
            Lang::Fr => self.french(),
        }
    }

    fn english(&self) -> String {
        match self {
            Self::NoManifest => "no theme.toml or extension.toml in this folder".into(),
            Self::TwoManifests => {
                "the folder has both theme.toml and extension.toml; keep only one".into()
            }
            Self::ManifestSyntax(detail) => format!("the manifest cannot be read: {detail}"),
            Self::InvalidId(id) => format!(
                "invalid id `{id}`: use 1 to 48 lowercase letters, digits or dashes, starting with a letter or digit"
            ),
            Self::FolderMismatch { folder, id } => format!(
                "the folder is named `{folder}` but the manifest says id = \"{id}\"; install mods with `spacetify install`"
            ),
            Self::EmptyField(field) => format!("the `{field}` field is empty"),
            Self::FieldTooLong { field, max } => {
                format!("the `{field}` field is longer than {max} characters")
            }
            Self::InvalidVersion(value) => format!(
                "invalid version `{value}`: use the MAJOR.MINOR.PATCH form, for example 1.0.0"
            ),
            Self::InvalidRequirement(value) => {
                format!("invalid `spacetify` requirement `{value}`: use a form such as \">=0.2.0\"")
            }
            Self::NeedsNewerSpacetify { required, current } => format!(
                "needs spacetify {required}, but this is spacetify {current}; update spacetify first"
            ),
            Self::MissingFile(file) => format!("{file} is missing"),
            Self::FileTooLarge { file, max_kib } => {
                format!("{file} is too large (the limit is {max_kib} KiB)")
            }
            Self::TooManyFiles { max } => format!("the folder has more than {max} files"),
            Self::NotUtf8(file) => format!("{file} is not valid UTF-8 text"),
            Self::Symlink(file) => format!("{file} is a symbolic link, which is not allowed"),
            Self::EmptyTheme => "a theme needs a theme.css, a colors.toml, or both".into(),
            Self::CssUnclosedComment { file, line } => {
                format!("{file}, line {line}: a /* comment */ is never closed")
            }
            Self::CssUnclosedString { file, line } => {
                format!("{file}, line {line}: a quoted string is never closed")
            }
            Self::CssUnbalanced { file, line } => {
                format!("{file}, line {line}: the {{ and }} braces do not match")
            }
            Self::CssImport { file, line } => format!(
                "{file}, line {line}: @import is not supported; put everything in theme.css"
            ),
            Self::BadAsset { url, reason } => match reason {
                AssetProblem::OutsideAssets => format!(
                    "url({url}): local files must be inside the theme's assets folder, for example url(\"assets/logo.png\")"
                ),
                AssetProblem::NotFound => format!("url({url}): file not found"),
                AssetProblem::UnsupportedType => format!(
                    "url({url}): unsupported file type (use png, jpg, gif, webp, svg, woff, woff2, ttf or otf)"
                ),
            },
            Self::ColorsSyntax(detail) => format!("colors.toml cannot be read: {detail}"),
            Self::UnknownColorSet(set) => format!(
                "colors.toml: unknown section [{set}]; see the list of sections in the theme guide"
            ),
            Self::UnknownColorToken { set, token } => format!(
                "colors.toml: unknown color `{token}` in [{set}]; see the list of colors in the theme guide"
            ),
            Self::BadColor { set, token, value } => format!(
                "colors.toml: `{token}` in [{set}] is `{value}`, which is not a color like \"#1ed760\""
            ),
            Self::InvalidSettingName(name) => format!(
                "invalid setting name `{name}`: use lowercase letters, digits, - or _, starting with a letter"
            ),
            Self::BadSettingDefault { setting } => {
                format!("the default value of setting `{setting}` does not match its type")
            }
            Self::ChoiceWithoutChoices { setting } => {
                format!("setting `{setting}` is a choice but has no `choices` list")
            }
            Self::InvalidSelector(selector) => format!("invalid selector `{selector}`"),
        }
    }

    fn french(&self) -> String {
        match self {
            Self::NoManifest => "pas de theme.toml ni de extension.toml dans ce dossier".into(),
            Self::TwoManifests => {
                "le dossier contient à la fois theme.toml et extension.toml ; garde-en un seul"
                    .into()
            }
            Self::ManifestSyntax(detail) => format!("le manifeste est illisible : {detail}"),
            Self::InvalidId(id) => format!(
                "identifiant `{id}` invalide : utilise 1 à 48 lettres minuscules, chiffres ou tirets, en commençant par une lettre ou un chiffre"
            ),
            Self::FolderMismatch { folder, id } => format!(
                "le dossier s'appelle `{folder}` mais le manifeste indique id = \"{id}\" ; installe les mods avec `spacetify install`"
            ),
            Self::EmptyField(field) => format!("le champ `{field}` est vide"),
            Self::FieldTooLong { field, max } => {
                format!("le champ `{field}` dépasse {max} caractères")
            }
            Self::InvalidVersion(value) => format!(
                "version `{value}` invalide : utilise la forme MAJEUR.MINEUR.CORRECTIF, par exemple 1.0.0"
            ),
            Self::InvalidRequirement(value) => format!(
                "exigence `spacetify` invalide `{value}` : utilise une forme comme \">=0.2.0\""
            ),
            Self::NeedsNewerSpacetify { required, current } => format!(
                "demande spacetify {required}, mais ceci est spacetify {current} ; mets d'abord spacetify à jour"
            ),
            Self::MissingFile(file) => format!("{file} est absent"),
            Self::FileTooLarge { file, max_kib } => {
                format!("{file} est trop gros (la limite est de {max_kib} Kio)")
            }
            Self::TooManyFiles { max } => format!("le dossier contient plus de {max} fichiers"),
            Self::NotUtf8(file) => format!("{file} n'est pas du texte UTF-8 valide"),
            Self::Symlink(file) => format!("{file} est un lien symbolique, ce qui est interdit"),
            Self::EmptyTheme => {
                "un thème a besoin d'un theme.css, d'un colors.toml, ou des deux".into()
            }
            Self::CssUnclosedComment { file, line } => {
                format!("{file}, ligne {line} : un /* commentaire */ n'est jamais fermé")
            }
            Self::CssUnclosedString { file, line } => {
                format!("{file}, ligne {line} : un texte entre guillemets n'est jamais fermé")
            }
            Self::CssUnbalanced { file, line } => {
                format!("{file}, ligne {line} : les accolades {{ et }} ne correspondent pas")
            }
            Self::CssImport { file, line } => format!(
                "{file}, ligne {line} : @import n'est pas pris en charge ; mets tout dans theme.css"
            ),
            Self::BadAsset { url, reason } => match reason {
                AssetProblem::OutsideAssets => format!(
                    "url({url}) : les fichiers locaux doivent être dans le dossier assets du thème, par exemple url(\"assets/logo.png\")"
                ),
                AssetProblem::NotFound => format!("url({url}) : fichier introuvable"),
                AssetProblem::UnsupportedType => format!(
                    "url({url}) : type de fichier non pris en charge (utilise png, jpg, gif, webp, svg, woff, woff2, ttf ou otf)"
                ),
            },
            Self::ColorsSyntax(detail) => format!("colors.toml est illisible : {detail}"),
            Self::UnknownColorSet(set) => format!(
                "colors.toml : section [{set}] inconnue ; la liste des sections est dans le guide des thèmes"
            ),
            Self::UnknownColorToken { set, token } => format!(
                "colors.toml : couleur `{token}` inconnue dans [{set}] ; la liste des couleurs est dans le guide des thèmes"
            ),
            Self::BadColor { set, token, value } => format!(
                "colors.toml : `{token}` dans [{set}] vaut `{value}`, ce qui n'est pas une couleur comme \"#1ed760\""
            ),
            Self::InvalidSettingName(name) => format!(
                "nom de réglage `{name}` invalide : utilise des minuscules, chiffres, - ou _, en commençant par une lettre"
            ),
            Self::BadSettingDefault { setting } => {
                format!("la valeur par défaut du réglage `{setting}` ne correspond pas à son type")
            }
            Self::ChoiceWithoutChoices { setting } => {
                format!("le réglage `{setting}` est un choix mais n'a pas de liste `choices`")
            }
            Self::InvalidSelector(selector) => format!("sélecteur `{selector}` invalide"),
        }
    }
}

impl fmt::Display for ModIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.english())
    }
}
