//! `theme.toml` and `extension.toml`.
//!
//! The `spacetify` requirement is checked before anything else, so a mod
//! written for a newer spacetify gets "update spacetify" instead of a
//! confusing error about a field this version does not know yet.

use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::ModIssue;

pub const MAX_ID_LEN: usize = 48;
const MAX_NAME_LEN: usize = 60;
const MAX_TEXT_LEN: usize = 300;
const MAX_SELECTORS: usize = 100;
const MAX_SELECTOR_LEN: usize = 300;

/// The fields every mod has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub author: String,
    pub description: Option<String>,
    pub spacetify: VersionReq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeManifest {
    pub meta: Meta,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionManifest {
    pub meta: Meta,
    /// Selectors the extension relies on, checked by `doctor`.
    pub selectors: Vec<String>,
    pub settings: BTreeMap<String, Setting>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub kind: SettingKind,
    pub default: Json,
    pub label: Option<String>,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingKind {
    Boolean,
    Number,
    String,
    Choice,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    id: String,
    name: String,
    version: String,
    author: String,
    description: Option<String>,
    spacetify: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExtension {
    id: String,
    name: String,
    version: String,
    author: String,
    description: Option<String>,
    spacetify: String,
    #[serde(default)]
    selectors: Vec<String>,
    #[serde(default)]
    settings: BTreeMap<String, RawSetting>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSetting {
    #[serde(rename = "type")]
    kind: SettingKind,
    default: toml::Value,
    label: Option<String>,
    choices: Option<Vec<String>>,
}

pub fn parse_theme(text: &str) -> Result<ThemeManifest, ModIssue> {
    check_requirement_first(text)?;
    let raw: RawTheme = toml::from_str(text).map_err(syntax)?;
    let meta = meta(
        raw.id,
        raw.name,
        &raw.version,
        raw.author,
        raw.description,
        &raw.spacetify,
    )?;
    Ok(ThemeManifest { meta })
}

pub fn parse_extension(text: &str) -> Result<ExtensionManifest, ModIssue> {
    check_requirement_first(text)?;
    let raw: RawExtension = toml::from_str(text).map_err(syntax)?;
    let meta = meta(
        raw.id,
        raw.name,
        &raw.version,
        raw.author,
        raw.description,
        &raw.spacetify,
    )?;

    if raw.selectors.len() > MAX_SELECTORS {
        return Err(ModIssue::FieldTooLong {
            field: "selectors",
            max: MAX_SELECTORS,
        });
    }
    for selector in &raw.selectors {
        let trimmed = selector.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_SELECTOR_LEN
            || trimmed.contains(['{', '}', ';'])
        {
            return Err(ModIssue::InvalidSelector(selector.clone()));
        }
    }

    let mut settings = BTreeMap::new();
    for (name, raw) in raw.settings {
        let setting = setting(&name, raw)?;
        settings.insert(name, setting);
    }
    Ok(ExtensionManifest {
        meta,
        selectors: raw.selectors.iter().map(|s| s.trim().to_owned()).collect(),
        settings,
    })
}

/// The current version of spacetify.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is valid semver")
}

/// `none` is reserved: `spacetify theme none` turns the theme off.
pub fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    id != "none"
        && !bytes.is_empty()
        && bytes.len() <= MAX_ID_LEN
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

impl Setting {
    /// Shape given to the injected runtime.
    pub fn to_json(&self) -> Json {
        let mut value = json!({ "type": self.kind, "default": self.default });
        if let Some(label) = &self.label {
            value["label"] = json!(label);
        }
        if self.kind == SettingKind::Choice {
            value["choices"] = json!(self.choices);
        }
        value
    }
}

impl serde::Serialize for SettingKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Choice => "choice",
        })
    }
}

fn check_requirement_first(text: &str) -> Result<(), ModIssue> {
    let table: toml::Table = toml::from_str(text).map_err(syntax)?;
    if let Some(value) = table.get("spacetify").and_then(toml::Value::as_str) {
        requirement(value)?;
    }
    Ok(())
}

fn requirement(value: &str) -> Result<VersionReq, ModIssue> {
    let req = VersionReq::parse(value.trim())
        .map_err(|_| ModIssue::InvalidRequirement(value.to_owned()))?;
    let current = current_version();
    if !req.matches(&current) {
        return Err(ModIssue::NeedsNewerSpacetify {
            required: value.trim().to_owned(),
            current: current.to_string(),
        });
    }
    Ok(req)
}

fn meta(
    id: String,
    name: String,
    version: &str,
    author: String,
    description: Option<String>,
    spacetify: &str,
) -> Result<Meta, ModIssue> {
    if !is_valid_id(&id) {
        return Err(ModIssue::InvalidId(id));
    }
    let name = text_field("name", name, MAX_NAME_LEN)?;
    let author = text_field("author", author, MAX_NAME_LEN)?;
    let description = match description {
        Some(text) if text.trim().is_empty() => None,
        Some(text) => Some(text_field("description", text, MAX_TEXT_LEN)?),
        None => None,
    };
    let version =
        Version::parse(version.trim()).map_err(|_| ModIssue::InvalidVersion(version.to_owned()))?;
    Ok(Meta {
        id,
        name,
        version,
        author,
        description,
        spacetify: requirement(spacetify)?,
    })
}

fn text_field(field: &'static str, value: String, max: usize) -> Result<String, ModIssue> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ModIssue::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ModIssue::FieldTooLong { field, max });
    }
    // Control characters would mess up the terminal when listing mods.
    Ok(value.chars().filter(|c| !c.is_control()).collect())
}

fn setting(name: &str, raw: RawSetting) -> Result<Setting, ModIssue> {
    let valid_name = name.len() <= 40
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !valid_name {
        return Err(ModIssue::InvalidSettingName(name.to_owned()));
    }
    let bad_default = || ModIssue::BadSettingDefault {
        setting: name.to_owned(),
    };
    let choices = raw.choices.unwrap_or_default();
    let default = match (raw.kind, &raw.default) {
        (SettingKind::Boolean, toml::Value::Boolean(b)) => json!(b),
        (SettingKind::Number, toml::Value::Integer(n)) => json!(n),
        (SettingKind::Number, toml::Value::Float(n)) if n.is_finite() => json!(n),
        (SettingKind::String, toml::Value::String(s)) => json!(s),
        (SettingKind::Choice, toml::Value::String(s)) => {
            if choices.is_empty() {
                return Err(ModIssue::ChoiceWithoutChoices {
                    setting: name.to_owned(),
                });
            }
            if !choices.contains(s) {
                return Err(bad_default());
            }
            json!(s)
        }
        _ => return Err(bad_default()),
    };
    let label = match raw.label {
        Some(label) => Some(text_field("label", label, MAX_NAME_LEN)?),
        None => None,
    };
    Ok(Setting {
        kind: raw.kind,
        default,
        label,
        choices,
    })
}

fn syntax(err: toml::de::Error) -> ModIssue {
    ModIssue::ManifestSyntax(err.message().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: &str = r#"
        id = "midnight"
        name = "Midnight"
        version = "1.0.0"
        author = "Someone"
        spacetify = ">=0.2.0"
    "#;

    #[test]
    fn parses_a_theme() {
        let manifest = parse_theme(THEME).unwrap();
        assert_eq!(manifest.meta.id, "midnight");
        assert_eq!(manifest.meta.version, Version::new(1, 0, 0));
        assert_eq!(manifest.meta.description, None);
    }

    #[test]
    fn rejects_bad_fields() {
        let with = |from: &str, to: &str| parse_theme(&THEME.replace(from, to)).unwrap_err();
        assert_eq!(
            with("\"midnight\"", "\"Mid Night\""),
            ModIssue::InvalidId("Mid Night".into())
        );
        assert_eq!(
            with("\"1.0.0\"", "\"1.0\""),
            ModIssue::InvalidVersion("1.0".into())
        );
        assert_eq!(
            with("\"Someone\"", "\"  \""),
            ModIssue::EmptyField("author")
        );
        assert!(matches!(
            with("author", "autor"),
            ModIssue::ManifestSyntax(_)
        ));
        assert!(matches!(
            with("id = \"midnight\"", ""),
            ModIssue::ManifestSyntax(_)
        ));
    }

    #[test]
    fn checks_the_spacetify_requirement_before_unknown_fields() {
        let future = THEME.replace(">=0.2.0", ">=99.0.0") + "\nnew_field = true\n";
        assert!(matches!(
            parse_theme(&future).unwrap_err(),
            ModIssue::NeedsNewerSpacetify { .. }
        ));
        assert_eq!(
            parse_theme(&THEME.replace(">=0.2.0", "soon")).unwrap_err(),
            ModIssue::InvalidRequirement("soon".into())
        );
    }

    #[test]
    fn validates_ids() {
        assert!(is_valid_id("hello-badge"));
        assert!(is_valid_id("2048"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-x"));
        assert!(!is_valid_id("Hello"));
        assert!(!is_valid_id("a/b"));
        assert!(!is_valid_id("none"));
        assert!(!is_valid_id(&"a".repeat(MAX_ID_LEN + 1)));
    }

    #[test]
    fn parses_extension_settings() {
        let text = THEME.to_owned()
            + r#"
            selectors = ['[data-testid="now-playing-bar"]']
            [settings.text]
            type = "string"
            default = "hi"
            label = "Text"
            [settings.side]
            type = "choice"
            choices = ["left", "right"]
            default = "right"
            [settings.size]
            type = "number"
            default = 12
        "#;
        let manifest = parse_extension(&text).unwrap();
        assert_eq!(manifest.selectors, ["[data-testid=\"now-playing-bar\"]"]);
        assert_eq!(manifest.settings["text"].default, json!("hi"));
        assert_eq!(
            manifest.settings["side"].to_json(),
            json!({"type": "choice", "default": "right", "choices": ["left", "right"]})
        );
        assert_eq!(manifest.settings["size"].kind, SettingKind::Number);
    }

    #[test]
    fn rejects_bad_settings() {
        let with = |setting: &str| parse_extension(&format!("{THEME}\n{setting}")).unwrap_err();
        assert_eq!(
            with("[settings.on]\ntype = \"boolean\"\ndefault = \"yes\""),
            ModIssue::BadSettingDefault {
                setting: "on".into()
            }
        );
        assert_eq!(
            with("[settings.side]\ntype = \"choice\"\ndefault = \"up\""),
            ModIssue::ChoiceWithoutChoices {
                setting: "side".into()
            }
        );
        assert_eq!(
            with("[settings.side]\ntype = \"choice\"\nchoices = [\"a\"]\ndefault = \"b\""),
            ModIssue::BadSettingDefault {
                setting: "side".into()
            }
        );
        assert_eq!(
            with("[settings.Bad]\ntype = \"string\"\ndefault = \"\""),
            ModIssue::InvalidSettingName("Bad".into())
        );
        assert_eq!(
            with("selectors = [\"a { color: red }\"]"),
            ModIssue::InvalidSelector("a { color: red }".into())
        );
    }
}
