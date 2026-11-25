//! Checks, without changing anything, that what the active mods rely on
//! still exists in the installed version of Spotify.
//!
//! The interface files inside `xpui.spa` are searched for every CSS
//! variable, `data-testid`, class and id the mods use:
//! - for the theme, they are read from its CSS and `colors.toml`;
//! - for extensions, from the `selectors` list of `extension.toml` (code
//!   cannot be read reliably).
//!
//! The search is textual. A match means the name is in Spotify's code, which
//! is a strong sign, not a proof that it is on screen. Some things cannot be
//! checked this way (translated labels, attributes set at runtime): they are
//! reported as such rather than guessed.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::config::Config;
use crate::mods::{ActiveMods, Extension, ModKind, ModStore, Theme, css};
use crate::patcher::ASSET_DIR;
use crate::spotify::SpotifyInstall;
use crate::{Error, Result};

/// The text of Spotify's interface, ready to be searched.
#[derive(Debug, Default)]
pub struct UiIndex {
    css: String,
    js: String,
    html: String,
}

impl UiIndex {
    /// Reads every stylesheet, script and page of `xpui.spa`, leaving out
    /// what spacetify itself added.
    pub fn from_spa(path: &Path) -> Result<Self> {
        let zip_err = |err: zip::result::ZipError| Error::InvalidArchive {
            path: path.to_path_buf(),
            reason: err.to_string(),
        };
        let file = File::open(path).map_err(Error::io(path))?;
        let mut archive = ZipArchive::new(BufReader::new(file)).map_err(zip_err)?;
        let mut index = Self::default();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(zip_err)?;
            let name = entry.name().map_err(zip_err)?.into_owned();
            if name.starts_with(ASSET_DIR) {
                continue;
            }
            let target = if name.ends_with(".css") {
                &mut index.css
            } else if name.ends_with(".js") {
                &mut index.js
            } else if name.ends_with(".html") {
                &mut index.html
            } else {
                continue;
            };
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(Error::io(path))?;
            target.push_str(&String::from_utf8_lossy(&bytes));
            target.push('\n');
        }
        Ok(index)
    }

    pub fn from_parts(css: &str, js: &str, html: &str) -> Self {
        Self {
            css: css.into(),
            js: js.into(),
            html: html.into(),
        }
    }

    fn status(&self, target: &Target) -> Status {
        let found = match target {
            Target::Variable(name) => {
                has_token(&self.css, name, not_ident, not_ident)
                    || has_token(&self.js, name, not_ident, not_ident)
            }
            Target::TestId(value) => {
                has_token(&self.js, value, is_quote, is_quote)
                    || self.css.contains(&format!("data-testid=\"{value}\""))
                    || self.css.contains(&format!("data-testid={value}]"))
            }
            Target::Class(name) => {
                let found = has_token(&self.css, name, |c| c == Some('.'), not_ident)
                    || has_token(&self.js, name, is_class_edge, is_class_edge)
                    || has_token(&self.html, name, is_class_edge, is_class_edge);
                if found && is_fragile_class(name) {
                    return Status::Fragile;
                }
                found
            }
            Target::Id(name) => {
                has_token(&self.css, name, |c| c == Some('#'), not_ident)
                    || has_token(&self.js, name, is_quote, is_quote)
                    || has_token(&self.html, name, is_quote, is_quote)
            }
            Target::EncoreId(value) => {
                let found = has_token(&self.js, value, is_quote, is_quote)
                    || self.css.contains(&format!("data-encore-id={value}]"))
                    || self.css.contains(&format!("data-encore-id=\"{value}\""));
                // Spotify's code mostly refers to these through minified constants.
                if !found {
                    return Status::Unverifiable(Unverifiable::SetAtRuntime);
                }
                true
            }
            Target::Partial { value, .. } => {
                self.js.contains(value.as_str()) || self.css.contains(value.as_str())
            }
            Target::Attribute { name, .. } => {
                return Status::Unverifiable(if TRANSLATED.contains(&name.as_str()) {
                    Unverifiable::Translated
                } else {
                    Unverifiable::SetAtRuntime
                });
            }
        };
        if found {
            Status::Found
        } else {
            Status::Missing
        }
    }
}

/// Something a mod relies on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Variable(String),
    TestId(String),
    EncoreId(String),
    Id(String),
    Class(String),
    /// `[attr^=value]` and the like: only the value is searched.
    Partial {
        attribute: String,
        operator: String,
        value: String,
    },
    /// Attributes that cannot be checked, such as `aria-label`.
    Attribute {
        name: String,
        value: String,
    },
}

impl Target {
    /// As it would appear in CSS.
    pub fn css(&self) -> String {
        match self {
            Self::Variable(name) => name.clone(),
            Self::TestId(value) => format!("[data-testid=\"{value}\"]"),
            Self::EncoreId(value) => format!("[data-encore-id=\"{value}\"]"),
            Self::Id(name) => format!("#{name}"),
            Self::Class(name) => format!(".{name}"),
            Self::Partial {
                attribute,
                operator,
                value,
            } => format!("[{attribute}{operator}\"{value}\"]"),
            Self::Attribute { name, value } => format!("[{name}=\"{value}\"]"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Found,
    /// Found, but its name is generated and changes without notice.
    Fragile,
    Missing,
    Unverifiable(Unverifiable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unverifiable {
    /// Text that depends on Spotify's language, such as `aria-label`.
    Translated,
    /// Only known once Spotify runs.
    SetAtRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub target: Target,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Compatible,
    /// Works today, but relies on something fragile or uncheckable.
    Warnings,
    /// Something it relies on is gone.
    Broken,
    /// The mod itself could not be loaded.
    Invalid,
}

#[derive(Debug)]
pub struct ModReport {
    pub kind: ModKind,
    pub id: String,
    /// `name version`, when the mod could be loaded.
    pub title: Option<String>,
    pub checks: Vec<Check>,
    pub error: Option<Error>,
}

impl ModReport {
    pub fn verdict(&self) -> Verdict {
        if self.error.is_some() {
            Verdict::Invalid
        } else if self.count(|s| s == Status::Missing) > 0 {
            Verdict::Broken
        } else if self.count(|s| s != Status::Found) > 0 {
            Verdict::Warnings
        } else {
            Verdict::Compatible
        }
    }

    pub fn count(&self, filter: impl Fn(Status) -> bool) -> usize {
        self.checks.iter().filter(|c| filter(c.status)).count()
    }
}

#[derive(Debug)]
pub struct Report {
    pub spotify_version: Option<String>,
    pub mods: Vec<ModReport>,
}

impl Report {
    pub fn verdict(&self) -> Verdict {
        self.mods
            .iter()
            .map(ModReport::verdict)
            .max()
            .unwrap_or(Verdict::Compatible)
    }

    /// Checks that failed, over all mods.
    pub fn missing(&self) -> usize {
        self.mods
            .iter()
            .map(|m| m.count(|s| s == Status::Missing))
            .sum()
    }
}

/// Checks the mods turned on in `config` against the installed Spotify.
/// Mods that cannot be loaded are reported, not returned as errors.
pub fn run(install: &SpotifyInstall, store: &ModStore, config: &Config) -> Result<Report> {
    let index = index_of(install)?;
    let mut mods = Vec::new();
    if let Some(id) = &config.theme {
        mods.push(match store.load_theme(id) {
            Ok(theme) => theme_report(&index, &theme),
            Err(err) => failed(ModKind::Theme, id, err),
        });
    }
    for id in &config.extensions {
        mods.push(match store.load_extension(id) {
            Ok(extension) => extension_report(&index, &extension),
            Err(err) => failed(ModKind::Extension, id, err),
        });
    }
    Ok(Report {
        spotify_version: install.version.clone(),
        mods,
    })
}

/// Same as [`run`], for mods that are already loaded.
pub fn check(install: &SpotifyInstall, mods: &ActiveMods) -> Result<Report> {
    let index = index_of(install)?;
    let mut reports: Vec<ModReport> = mods
        .theme
        .iter()
        .map(|theme| theme_report(&index, theme))
        .collect();
    reports.extend(mods.extensions.iter().map(|e| extension_report(&index, e)));
    Ok(Report {
        spotify_version: install.version.clone(),
        mods: reports,
    })
}

fn index_of(install: &SpotifyInstall) -> Result<UiIndex> {
    let spa = install.xpui_spa();
    if !spa.is_file() {
        return Err(Error::XpuiMissing(spa));
    }
    UiIndex::from_spa(&spa)
}

pub fn theme_report(index: &UiIndex, theme: &Theme) -> ModReport {
    let mut targets = BTreeSet::new();
    for tokens in theme.colors.values() {
        for token in tokens.keys() {
            targets.insert(Target::Variable(format!("--{token}")));
        }
    }
    if let Some(source) = &theme.source_css {
        targets.extend(theme_variables(source));
        for selector in css::selectors(source) {
            targets.extend(targets_in_selector(&selector));
        }
    }
    report(ModKind::Theme, &theme.manifest.meta, index, targets)
}

pub fn extension_report(index: &UiIndex, extension: &Extension) -> ModReport {
    let targets = extension
        .manifest
        .selectors
        .iter()
        .flat_map(|selector| targets_in_selector(selector))
        .collect();
    report(ModKind::Extension, &extension.manifest.meta, index, targets)
}

fn report(
    kind: ModKind,
    meta: &crate::mods::Meta,
    index: &UiIndex,
    targets: BTreeSet<Target>,
) -> ModReport {
    ModReport {
        kind,
        id: meta.id.clone(),
        title: Some(format!("{} {}", meta.name, meta.version)),
        checks: targets
            .into_iter()
            .map(|target| Check {
                status: index.status(&target),
                target,
            })
            .collect(),
        error: None,
    }
}

fn failed(kind: ModKind, id: &str, error: Error) -> ModReport {
    ModReport {
        kind,
        id: id.to_owned(),
        title: None,
        checks: Vec::new(),
        error: Some(error),
    }
}

/// Variables the theme reads or overrides, without the ones it defines for
/// itself (declared *and* read by the theme).
fn theme_variables(source: &str) -> Vec<Target> {
    let declared: BTreeSet<String> = css::variables_declared(source).into_iter().collect();
    let used: BTreeSet<String> = css::variables_used(source).into_iter().collect();
    used.symmetric_difference(&declared)
        .map(|name| Target::Variable(name.clone()))
        .collect()
}

/// Attributes whose value is translated text.
const TRANSLATED: [&str; 4] = ["aria-label", "title", "alt", "placeholder"];

/// Classes, ids and attributes a selector depends on.
pub fn targets_in_selector(selector: &str) -> Vec<Target> {
    let chars: Vec<char> = selector.chars().collect();
    let mut targets = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '[' => {
                let end = closing_bracket(&chars, i).unwrap_or(chars.len());
                let inside: String = chars[i + 1..end.min(chars.len())].iter().collect();
                if let Some(target) = attribute_target(&inside) {
                    targets.push(target);
                }
                i = end + 1;
            }
            '.' | '#' => {
                let (name, next) = ident(&chars, i + 1);
                if !name.is_empty() {
                    targets.push(if chars[i] == '.' {
                        Target::Class(name)
                    } else {
                        Target::Id(name)
                    });
                }
                i = next.max(i + 1);
            }
            ':' => {
                // Pseudo-classes and pseudo-elements, such as :hover or ::after.
                let start = if chars.get(i + 1) == Some(&':') {
                    i + 2
                } else {
                    i + 1
                };
                let (_, next) = ident(&chars, start);
                i = next.max(i + 1);
            }
            '"' | '\'' => i = skip_string(&chars, i),
            _ => i += 1,
        }
    }
    targets
}

fn ident(chars: &[char], start: usize) -> (String, usize) {
    let starts_ok = match chars.get(start) {
        Some(&c) if c.is_alphabetic() || c == '_' || c == '\\' => true,
        Some('-') => chars
            .get(start + 1)
            .is_some_and(|&c| c.is_alphabetic() || c == '_' || c == '-'),
        _ => false,
    };
    if !starts_ok {
        return (String::new(), start);
    }
    let mut name = String::new();
    let mut i = start;
    while let Some(&c) = chars.get(i) {
        if c == '\\' {
            if let Some(&escaped) = chars.get(i + 1) {
                name.push(escaped);
            }
            i += 2;
        } else if c.is_alphanumeric() || c == '-' || c == '_' {
            name.push(c);
            i += 1;
        } else {
            break;
        }
    }
    (name, i)
}

fn skip_string(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut i = start + 1;
    while let Some(&c) = chars.get(i) {
        if c == '\\' {
            i += 2;
            continue;
        }
        i += 1;
        if c == quote {
            break;
        }
    }
    i
}

fn closing_bracket(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while let Some(&c) = chars.get(i) {
        match c {
            '"' | '\'' => {
                i = skip_string(chars, i);
                continue;
            }
            ']' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn attribute_target(inside: &str) -> Option<Target> {
    let inside = inside.trim();
    let name_end = inside
        .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(inside.len());
    let name = inside[..name_end].to_ascii_lowercase();
    let rest = inside[name_end..].trim_start();
    let op_len = if rest.starts_with('=') {
        1
    } else if rest.len() >= 2 && rest.as_bytes()[1] == b'=' && "~|^$*".contains(&rest[..1]) {
        2
    } else {
        // `[attr]` alone only needs the attribute to exist.
        return None;
    };
    let operator = rest[..op_len].to_owned();
    let mut value = rest[op_len..].trim();
    // Flags such as ` i` at the end.
    if let Some(stripped) = value
        .strip_suffix(" i")
        .or_else(|| value.strip_suffix(" s"))
    {
        value = stripped.trim_end();
    }
    let value = value.trim_matches(['"', '\'']).to_owned();
    if name.is_empty() || value.is_empty() {
        return None;
    }
    let exact = operator == "=";
    Some(match name.as_str() {
        "data-testid" if exact => Target::TestId(value),
        "data-encore-id" if exact => Target::EncoreId(value),
        "id" if exact => Target::Id(value),
        "class" if operator == "~=" => Target::Class(value),
        "data-testid" | "data-encore-id" | "id" | "class" => Target::Partial {
            attribute: name,
            operator,
            value,
        },
        _ => Target::Attribute { name, value },
    })
}

/// Hashed class names (`gyQ3lHdZrOLMpQhHd2rw`), versioned Encore classes
/// (`e-10860-button`) and CSS-module names (`_card_16wgz_1`) are generated
/// by Spotify's build and can change with any update.
pub fn is_fragile_class(name: &str) -> bool {
    let hashed = name.len() == 20
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && name.chars().any(|c| c.is_ascii_uppercase())
        && name.chars().any(|c| c.is_ascii_lowercase());
    let versioned = name.strip_prefix("e-").is_some_and(|rest| {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        digits >= 3 && rest[digits..].starts_with('-')
    });
    let css_module = name.starts_with('_') && {
        let parts: Vec<&str> = name[1..].rsplitn(3, '_').collect();
        parts.len() == 3
            && parts[0].chars().all(|c| c.is_ascii_digit())
            && !parts[0].is_empty()
            && parts[1].len() == 5
            && parts[1]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    };
    hashed || versioned || css_module
}

fn has_token(
    haystack: &str,
    needle: &str,
    before: impl Fn(Option<char>) -> bool,
    after: impl Fn(Option<char>) -> bool,
) -> bool {
    !needle.is_empty()
        && haystack.match_indices(needle).any(|(at, _)| {
            before(haystack[..at].chars().next_back())
                && after(haystack[at + needle.len()..].chars().next())
        })
}

fn not_ident(c: Option<char>) -> bool {
    !c.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn is_quote(c: Option<char>) -> bool {
    matches!(c, Some('"' | '\'' | '`'))
}

fn is_class_edge(c: Option<char>) -> bool {
    matches!(c, Some('"' | '\'' | '`' | ' '))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPOTIFY_CSS: &str = ".encore-dark-theme,.encore-dark-theme .encore-base-set{--background-base:#121212;--text-base:#fff}\
        .gyQ3lHdZrOLMpQhHd2rw{display:flex}.main-view-container{top:0}[data-encore-id=buttonTertiary]{x:y}";
    const SPOTIFY_JS: &str = r#"(0,i.jsx)("div",{"data-testid":"now-playing-bar",className:"gyQ3lHdZrOLMpQhHd2rw NowPlayingView",id:"main"});
        e.style.setProperty("--dynamic-height",h)"#;

    fn index() -> UiIndex {
        UiIndex::from_parts(SPOTIFY_CSS, SPOTIFY_JS, "<div id=\"video-player\"></div>")
    }

    fn status(selector: &str) -> Vec<(String, Status)> {
        targets_in_selector(selector)
            .into_iter()
            .map(|t| (t.css(), index().status(&t)))
            .collect()
    }

    #[test]
    fn extracts_targets_from_selectors() {
        assert_eq!(
            targets_in_selector(
                ".a > #b:not(.c)::after, [data-testid=\"d\"] [aria-label='Play' i] [data-testid^=control-] [hidden] [title=\"x.y\"]"
            ),
            [
                Target::Class("a".into()),
                Target::Id("b".into()),
                Target::Class("c".into()),
                Target::TestId("d".into()),
                Target::Attribute {
                    name: "aria-label".into(),
                    value: "Play".into()
                },
                Target::Partial {
                    attribute: "data-testid".into(),
                    operator: "^=".into(),
                    value: "control-".into()
                },
                Target::Attribute {
                    name: "title".into(),
                    value: "x.y".into()
                },
            ]
        );
        assert!(targets_in_selector("50%").is_empty());
        assert!(targets_in_selector("12.5%").is_empty());
        assert_eq!(targets_in_selector(".a\\:b"), [Target::Class("a:b".into())]);
    }

    #[test]
    fn finds_what_spotify_has() {
        assert_eq!(
            status("[data-testid=\"now-playing-bar\"] .main-view-container #main"),
            [
                (
                    "[data-testid=\"now-playing-bar\"]".to_owned(),
                    Status::Found
                ),
                (".main-view-container".to_owned(), Status::Found),
                ("#main".to_owned(), Status::Found),
            ]
        );
        assert_eq!(status(".NowPlayingView #video-player")[1].1, Status::Found);
        assert_eq!(
            status("[data-encore-id=buttonTertiary]")[0].1,
            Status::Found
        );
    }

    #[test]
    fn reports_missing_fragile_and_unverifiable() {
        assert_eq!(
            status(
                "[data-testid=\"left-sidebar\"] .main-nowPlayingBar-container .gyQ3lHdZrOLMpQhHd2rw [aria-label=\"Play\"] [data-encore-id=\"chip\"]"
            ),
            [
                ("[data-testid=\"left-sidebar\"]".to_owned(), Status::Missing),
                (".main-nowPlayingBar-container".to_owned(), Status::Missing),
                (".gyQ3lHdZrOLMpQhHd2rw".to_owned(), Status::Fragile),
                (
                    "[aria-label=\"Play\"]".to_owned(),
                    Status::Unverifiable(Unverifiable::Translated)
                ),
                (
                    "[data-encore-id=\"chip\"]".to_owned(),
                    Status::Unverifiable(Unverifiable::SetAtRuntime)
                ),
            ]
        );
    }

    #[test]
    fn checks_variables_with_exact_names() {
        let index = index();
        let check = |name: &str| index.status(&Target::Variable(name.into()));
        assert_eq!(check("--background-base"), Status::Found);
        assert_eq!(check("--dynamic-height"), Status::Found);
        assert_eq!(check("--background"), Status::Missing);
        assert_eq!(check("--spice-main"), Status::Missing);
    }

    #[test]
    fn tells_own_variables_from_spotify_ones() {
        let css = ".a { --mine: red; color: var(--mine); background: var(--background-base) }\n\
                   .encore-dark-theme { --text-base: #eee }";
        let mut names: Vec<String> = theme_variables(css).iter().map(Target::css).collect();
        names.sort();
        assert_eq!(names, ["--background-base", "--text-base"]);
    }

    #[test]
    fn recognizes_generated_class_names() {
        assert!(is_fragile_class("gyQ3lHdZrOLMpQhHd2rw"));
        assert!(is_fragile_class("e-10860-button"));
        assert!(is_fragile_class("_card_16wgz_1"));
        assert!(!is_fragile_class("main-view-container"));
        assert!(!is_fragile_class("encore-dark-theme"));
        assert!(!is_fragile_class("NowPlayingView"));
    }
}
