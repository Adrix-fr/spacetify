//! Themes: `theme.toml`, plus `theme.css` and/or `colors.toml`, plus an
//! optional `assets/` folder.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Component, Path};

use super::files::{read_bytes, read_text};
use super::manifest::{self, ThemeManifest};
use super::{AssetProblem, ModIssue, css};

pub const MANIFEST: &str = "theme.toml";
pub const CSS_FILE: &str = "theme.css";
pub const COLORS_FILE: &str = "colors.toml";
pub const ASSETS_DIR: &str = "assets";

const MAX_CSS: u64 = 1024 * 1024;
const MAX_ASSET: u64 = 2 * 1024 * 1024;

/// Spotify's colour tokens, as CSS custom properties without the `--`.
///
/// Every colour of the interface comes from these. They are the most
/// stable part of Spotify's styles (see the theme guide).
pub const COLOR_TOKENS: [&str; 25] = [
    "background-base",
    "background-highlight",
    "background-press",
    "background-elevated-base",
    "background-elevated-highlight",
    "background-elevated-press",
    "background-tinted-base",
    "background-tinted-highlight",
    "background-tinted-press",
    "text-base",
    "text-subdued",
    "text-bright-accent",
    "text-negative",
    "text-warning",
    "text-positive",
    "text-announcement",
    "essential-base",
    "essential-subdued",
    "essential-bright-accent",
    "essential-negative",
    "essential-warning",
    "essential-positive",
    "essential-announcement",
    "decorative-base",
    "decorative-subdued",
];

/// Groups of colours ("sets"). `base` is the general look; the others are
/// used by specific parts, such as `bright-accent` for the big play button.
pub const COLOR_SETS: [&str; 16] = [
    "base",
    "bright-accent",
    "muted-accent",
    "positive",
    "positive-subdued",
    "negative",
    "negative-subdued",
    "warning",
    "warning-subdued",
    "announcement",
    "announcement-subdued",
    "inverted",
    "inverted-dark",
    "inverted-light",
    "over-media",
    "app-frame",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub manifest: ThemeManifest,
    /// Colours from `colors.toml`, by set then token.
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
    /// `theme.css` as written by the author.
    pub source_css: Option<String>,
    /// What gets injected: colours, then `theme.css` with assets inlined.
    pub css: String,
}

pub fn load(dir: &Path) -> Result<Theme, ModIssue> {
    let manifest = manifest::parse_theme(&read_text(dir, MANIFEST, MAX_CSS)?)?;

    let colors = if dir.join(COLORS_FILE).exists() {
        parse_colors(&read_text(dir, COLORS_FILE, MAX_CSS)?)?
    } else {
        BTreeMap::new()
    };
    let source_css = if dir.join(CSS_FILE).exists() {
        let text = read_text(dir, CSS_FILE, MAX_CSS)?;
        css::validate(CSS_FILE, &text)?;
        Some(text)
    } else {
        None
    };
    if colors.is_empty() && source_css.is_none() {
        return Err(ModIssue::EmptyTheme);
    }

    let meta = &manifest.meta;
    let mut out = format!("/* spacetify theme: {} {} */\n", meta.id, meta.version);
    out.push_str(&colors_css(&colors));
    if let Some(text) = &source_css {
        out.push_str(&inline_assets(dir, text)?);
        out.push('\n');
    }
    Ok(Theme {
        manifest,
        colors,
        source_css,
        css: out,
    })
}

pub fn parse_colors(text: &str) -> Result<BTreeMap<String, BTreeMap<String, String>>, ModIssue> {
    let table: BTreeMap<String, BTreeMap<String, String>> =
        toml::from_str(text).map_err(|err| ModIssue::ColorsSyntax(err.message().to_owned()))?;
    for (set, tokens) in &table {
        if !COLOR_SETS.contains(&set.as_str()) {
            return Err(ModIssue::UnknownColorSet(set.clone()));
        }
        for (token, value) in tokens {
            if !COLOR_TOKENS.contains(&token.as_str()) {
                return Err(ModIssue::UnknownColorToken {
                    set: set.clone(),
                    token: token.clone(),
                });
            }
            if !is_hex_color(value) {
                return Err(ModIssue::BadColor {
                    set: set.clone(),
                    token: token.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    Ok(table)
}

/// Accepts `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
pub fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Spotify's desktop app always uses its dark theme, so only that one is
/// recoloured. `base` comes first so the other sets win where they apply.
fn colors_css(colors: &BTreeMap<String, BTreeMap<String, String>>) -> String {
    let ordered = colors
        .get_key_value("base")
        .into_iter()
        .chain(colors.iter().filter(|(set, _)| set.as_str() != "base"));
    let mut out = String::new();
    for (set, tokens) in ordered {
        if tokens.is_empty() {
            continue;
        }
        let selector = if set == "base" {
            ".encore-dark-theme,\n.encore-dark-theme .encore-base-set".to_owned()
        } else {
            format!(".encore-dark-theme .encore-{set}-set")
        };
        let _ = writeln!(out, "{selector} {{");
        for (token, value) in tokens {
            let _ = writeln!(out, "  --{token}: {value} !important;");
        }
        out.push_str("}\n");
    }
    out
}

/// Replaces `url(assets/…)` with the file itself, as a `data:` URL, so the
/// theme works the same with the file method and with live injection.
fn inline_assets(dir: &Path, text: &str) -> Result<String, ModIssue> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = find_url(rest) {
        out.push_str(&rest[..at]);
        let inner_start = at + 4;
        let Some(close) = rest[inner_start..].find(')') else {
            out.push_str(&rest[at..]);
            return Ok(out);
        };
        let raw = &rest[inner_start..inner_start + close];
        let url = raw.trim().trim_matches(['"', '\'']).trim();
        out.push_str(&match local_asset(url) {
            Some(path) => format!("url(\"{}\")", data_url(dir, url, path)?),
            None => format!("url({raw})"),
        });
        rest = &rest[inner_start + close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn find_url(text: &str) -> Option<usize> {
    text.match_indices("url(").map(|(at, _)| at).find(|&at| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// `None` for URLs left as they are (remote, absolute, `data:`, fragments).
fn local_asset(url: &str) -> Option<&str> {
    let lower = url.to_ascii_lowercase();
    let external = url.is_empty()
        || url.starts_with(['/', '#'])
        || lower.starts_with("data:")
        || lower.contains("://");
    (!external).then_some(url.strip_prefix("./").unwrap_or(url))
}

fn data_url(dir: &Path, url: &str, path: &str) -> Result<String, ModIssue> {
    let bad = |reason| ModIssue::BadAsset {
        url: url.to_owned(),
        reason,
    };
    let relative = Path::new(path);
    let mut components = relative.components();
    let inside_assets = components.next() == Some(Component::Normal(ASSETS_DIR.as_ref()))
        && components.clone().next().is_some()
        && components.all(|c| matches!(c, Component::Normal(_)));
    if !inside_assets {
        return Err(bad(AssetProblem::OutsideAssets));
    }
    let extension = relative
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let mime = match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => return Err(bad(AssetProblem::UnsupportedType)),
    };
    if !dir.join(relative).is_file() {
        return Err(bad(AssetProblem::NotFound));
    }
    let bytes = read_bytes(dir, path, MAX_ASSET)?;
    Ok(format!("data:{mime};base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const MANIFEST_TEXT: &str = "id = \"midnight\"\nname = \"Midnight\"\nversion = \"1.0.0\"\n\
                                 author = \"Me\"\nspacetify = \">=0.2.0\"\n";

    fn theme_dir(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), MANIFEST_TEXT).unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        dir
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn checks_hex_colors() {
        assert!(is_hex_color("#1ed760"));
        assert!(is_hex_color("#fff"));
        assert!(is_hex_color("#ffffff1a"));
        assert!(!is_hex_color("1ed760"));
        assert!(!is_hex_color("#12345"));
        assert!(!is_hex_color("red"));
        assert!(!is_hex_color("#fff;}body{"));
    }

    #[test]
    fn builds_css_from_colors_and_theme_css() {
        let dir = theme_dir(&[
            (
                COLORS_FILE,
                b"[bright-accent]\nbackground-base = \"#8b5cf6\"\n\n[base]\nessential-bright-accent = \"#8b5cf6\"\n",
            ),
            (CSS_FILE, b"[data-testid=\"now-playing-bar\"] { border-radius: 8px }"),
        ]);
        let theme = load(dir.path()).unwrap();
        let base = theme.css.find(".encore-dark-theme,\n").unwrap();
        let accent = theme
            .css
            .find(".encore-dark-theme .encore-bright-accent-set {")
            .unwrap();
        let custom = theme.css.find("now-playing-bar").unwrap();
        assert!(base < accent && accent < custom);
        assert!(
            theme
                .css
                .contains("--essential-bright-accent: #8b5cf6 !important;")
        );
    }

    #[test]
    fn refuses_unknown_or_bad_colors() {
        let load_colors = |text: &str| load(theme_dir(&[(COLORS_FILE, text.as_bytes())]).path());
        assert_eq!(
            load_colors("[sidebar]\ntext-base = \"#fff\"").unwrap_err(),
            ModIssue::UnknownColorSet("sidebar".into())
        );
        assert_eq!(
            load_colors("[base]\nspice-main = \"#fff\"").unwrap_err(),
            ModIssue::UnknownColorToken {
                set: "base".into(),
                token: "spice-main".into()
            }
        );
        assert!(matches!(
            load_colors("[base]\ntext-base = \"white\"").unwrap_err(),
            ModIssue::BadColor { .. }
        ));
    }

    #[test]
    fn refuses_an_empty_theme_and_broken_css() {
        assert_eq!(
            load(theme_dir(&[]).path()).unwrap_err(),
            ModIssue::EmptyTheme
        );
        assert!(matches!(
            load(theme_dir(&[(CSS_FILE, b"a { color: red")]).path()).unwrap_err(),
            ModIssue::CssUnbalanced { .. }
        ));
    }

    #[test]
    fn inlines_assets_and_leaves_other_urls_alone() {
        let dir = theme_dir(&[
            ("assets/dot.png", &[1, 2, 3]),
            (
                CSS_FILE,
                b".a { background: url(\"assets/dot.png\") }\n\
                  .b { background: url( ./assets/dot.png ) }\n\
                  .c { background: url(https://example.com/x.png) }\n\
                  .d { mask: url(#m) }",
            ),
        ]);
        let css = load(dir.path()).unwrap().css;
        assert_eq!(
            css.matches("url(\"data:image/png;base64,AQID\")").count(),
            2
        );
        assert!(css.contains("url(https://example.com/x.png)"));
        assert!(css.contains("url(#m)"));
    }

    #[test]
    fn refuses_bad_assets() {
        let with_css = |css: &str| {
            let dir = theme_dir(&[("assets/a.txt", b"x"), (CSS_FILE, css.as_bytes())]);
            load(dir.path()).unwrap_err()
        };
        let reason = |issue| match issue {
            ModIssue::BadAsset { reason, .. } => reason,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            reason(with_css(".a { background: url(logo.png) }")),
            AssetProblem::OutsideAssets
        );
        assert_eq!(
            reason(with_css(".a { background: url(assets/../theme.toml) }")),
            AssetProblem::OutsideAssets
        );
        assert_eq!(
            reason(with_css(".a { background: url(assets/missing.png) }")),
            AssetProblem::NotFound
        );
        assert_eq!(
            reason(with_css(".a { background: url(assets/a.txt) }")),
            AssetProblem::UnsupportedType
        );
    }
}
