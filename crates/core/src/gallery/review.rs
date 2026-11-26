//! The security review of a mod sent to the gallery.
//!
//! The robot on GitHub and the desktop app run it on the files of a mod
//! before anyone accepts it. It only reads the files, it never runs them.
//! It flags anything out of the ordinary, with the file and the line, so
//! the people who accept mods can see for themselves:
//!
//! - **blocking**: never acceptable (a program disguised as an image,
//!   characters that hide code, a script inside an SVG…). The robot does
//!   not offer the mod until it is fixed;
//! - **to look at**: often fine, but a person decides (a web address, the
//!   use of Spotify's storage…).
//!
//! No automatic check proves that code is safe: this one points at what a
//! person must read.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;

use super::{sha256_hex, showcase};
use crate::Lang;
use crate::mods::{self, ModKind, extension, theme};
use crate::service::Text;

/// Biggest mod a review reads, as for installing one.
const MAX_FILE: u64 = 2 * 1024 * 1024;
/// Shown text of a line, at most.
const MAX_EXCERPT: usize = 160;
/// A JavaScript line longer than this is probably packed or minified.
const LONG_LINE: usize = 1000;
/// A run of base64 characters this long is probably an encoded file or code.
const LONG_BLOB: usize = 400;
/// Pictures larger than this, in pixels, are flagged.
const MAX_PIXELS: u32 = 8192;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Look,
    Block,
}

/// The families of checks, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// The mod loads like any other: manifest, CSS, colours.
    Valid,
    /// Each file is what its name says, and only allowed files are there.
    Files,
    /// Known viruses, from ClamAV. Only on GitHub.
    Antivirus,
    /// Invisible characters, or characters that reverse the text.
    Hidden,
    /// Code made hard to read on purpose, or code that builds code.
    Obfuscation,
    /// Every web address the mod contacts.
    Network,
    /// Spotify's account, storage, the clipboard, the keyboard…
    Sensitive,
    /// Scripts hidden in pictures, oversized pictures.
    Images,
    /// What changed since the version already in the gallery.
    Changes,
}

impl Check {
    pub const ALL: [Self; 9] = [
        Self::Valid,
        Self::Files,
        Self::Antivirus,
        Self::Hidden,
        Self::Obfuscation,
        Self::Network,
        Self::Sensitive,
        Self::Images,
        Self::Changes,
    ];

    pub fn title(self) -> Text {
        let (en, fr) = match self {
            Self::Valid => ("Valid mod", "Mod valide"),
            Self::Files => ("Disguised programs", "Programmes déguisés"),
            Self::Antivirus => ("Antivirus", "Antivirus"),
            Self::Hidden => ("Invisible characters", "Caractères invisibles"),
            Self::Obfuscation => ("Scrambled code", "Code brouillé"),
            Self::Network => ("Web addresses", "Adresses internet"),
            Self::Sensitive => ("Sensitive access", "Accès sensibles"),
            Self::Images => ("Trapped pictures", "Images piégées"),
            Self::Changes => (
                "Changes since the accepted version",
                "Changements depuis la version acceptée",
            ),
        };
        Text::new(en, fr)
    }

    /// What a check found nothing means.
    fn clean(self) -> Text {
        let (en, fr) = match self {
            Self::Valid => (
                "The manifest, the CSS and the colours are valid.",
                "Le manifeste, le CSS et les couleurs sont corrects.",
            ),
            Self::Files => (
                "Every file is what its name says. No program, no archive, no hidden file, no shortcut.",
                "Chaque fichier est bien ce que dit son nom. Aucun programme, aucune archive, aucun fichier caché, aucun raccourci.",
            ),
            Self::Antivirus => (
                "ClamAV, with today's list of known viruses, found nothing.",
                "L'antivirus ClamAV, avec sa liste de virus connus du jour, n'a rien trouvé.",
            ),
            Self::Hidden => (
                "No invisible character and none that reverses the text.",
                "Aucun caractère invisible ni qui inverse le sens du texte.",
            ),
            Self::Obfuscation => (
                "No code made unreadable on purpose, no long encoded block, no code that builds and runs code.",
                "Pas de code illisible exprès, pas de long bloc codé, pas de code qui fabrique et lance du code.",
            ),
            Self::Network => (
                "The mod contacts no web address.",
                "Le mod ne contacte aucune adresse internet.",
            ),
            Self::Sensitive => (
                "Nothing touches the Spotify account, the cookies, the clipboard or the keyboard, and nothing opens pages by itself.",
                "Rien ne touche au compte Spotify, aux cookies, au presse-papiers ou au clavier, et rien n'ouvre de page tout seul.",
            ),
            Self::Images => (
                "No picture hides code and none is oversized.",
                "Aucune image ne cache de code et aucune n'est démesurée.",
            ),
            Self::Changes => (
                "A new mod: nothing to compare.",
                "Nouveau mod : rien à comparer.",
            ),
        };
        Text::new(en, fr)
    }
}

/// One thing the review found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub check: Check,
    pub level: Level,
    pub what: Text,
    /// File, relative to the mod's folder, with `/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// From 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The line, shortened, with invisible characters shown as `<U+200B>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewedFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Everything the review found about one mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub id: Option<String>,
    pub kind: Option<ModKind>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub files: Vec<ReviewedFile>,
    /// Every web address found, once each.
    pub addresses: Vec<String>,
    pub findings: Vec<Finding>,
    /// Checks that were not run (the antivirus outside GitHub, the
    /// comparison for a new mod).
    pub skipped: Vec<Check>,
    pub blocking: usize,
    pub to_look_at: usize,
}

impl Report {
    /// The worst level found by `check`, or `None` if it was not run.
    pub fn level_of(&self, check: Check) -> Option<Level> {
        if self.skipped.contains(&check) {
            return None;
        }
        Some(
            self.findings
                .iter()
                .filter(|f| f.check == check)
                .map(|f| f.level)
                .max()
                .unwrap_or(Level::Ok),
        )
    }

    pub fn summary(&self) -> Text {
        let (block, look) = (self.blocking, self.to_look_at);
        let good = Check::ALL
            .iter()
            .filter(|&&c| self.level_of(c) == Some(Level::Ok))
            .count();
        Text::new(
            format!("{good} checks passed · {look} to look at · {block} blocking"),
            format!("{good} contrôles bons · {look} à regarder · {block} bloquant(s)"),
        )
    }

    /// Adds what ClamAV said (`clamscan` output). Paths in it are made
    /// relative to `root`.
    pub fn add_antivirus(&mut self, clamscan: &str, root: &Path) {
        self.skipped.retain(|&c| c != Check::Antivirus);
        let root = root.to_string_lossy().replace('\\', "/");
        let mut infected = false;
        for line in clamscan.lines() {
            let Some(rest) = line.strip_suffix(" FOUND") else {
                continue;
            };
            let Some((path, virus)) = rest.rsplit_once(": ") else {
                continue;
            };
            infected = true;
            let path = path.replace('\\', "/");
            let file = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .trim_start_matches('/')
                .to_owned();
            let virus = virus.trim();
            self.findings.push(Finding {
                check: Check::Antivirus,
                level: Level::Block,
                what: Text::new(
                    format!("ClamAV recognises a known virus: {virus}."),
                    format!("ClamAV reconnaît un virus connu : {virus}."),
                ),
                file: Some(file),
                line: None,
                excerpt: None,
            });
        }
        // Without its summary, the scan did not finish: never take that as
        // a clean result.
        if !infected && !clamscan.contains("Infected files: 0") {
            self.findings.push(Finding {
                check: Check::Antivirus,
                level: Level::Block,
                what: Text::new(
                    "The antivirus scan did not finish. Run the robot again.",
                    "L'analyse antivirus ne s'est pas terminée. Relance le robot.",
                ),
                file: None,
                line: None,
                excerpt: None,
            });
        }
        self.finish();
    }

    fn finish(&mut self) {
        self.findings.sort_by(|a, b| {
            (a.check, std::cmp::Reverse(a.level), &a.file, a.line).cmp(&(
                b.check,
                std::cmp::Reverse(b.level),
                &b.file,
                b.line,
            ))
        });
        self.findings.dedup();
        self.blocking = self
            .findings
            .iter()
            .filter(|f| f.level == Level::Block)
            .count();
        self.to_look_at = self
            .findings
            .iter()
            .filter(|f| f.level == Level::Look)
            .count();
    }

    /// The report for a GitHub comment. `link` turns a file and a line into
    /// a link to that line. Everything that comes from the mod is escaped,
    /// so it cannot format the comment or mention anyone.
    pub fn markdown(&self, lang: Lang, link: &dyn Fn(&str, Option<usize>) -> String) -> String {
        let mut out = String::new();
        let title = match lang {
            Lang::En => "Security report",
            Lang::Fr => "Rapport de sécurité",
        };
        out.push_str(&format!(
            "#### {title}\n\n**{}**\n\n",
            self.summary().get(lang)
        ));
        for check in Check::ALL {
            let Some(level) = self.level_of(check) else {
                continue;
            };
            let mark = match level {
                Level::Ok => "🟢",
                Level::Look => "🟠",
                Level::Block => "🔴",
            };
            let found: Vec<&Finding> = self.findings.iter().filter(|f| f.check == check).collect();
            out.push_str(&format!("- {mark} **{}**", check.title().get(lang)));
            if found.is_empty() {
                let colon = match lang {
                    Lang::En => ":",
                    Lang::Fr => " :",
                };
                out.push_str(&format!("{colon} {}\n", check.clean().get(lang)));
                continue;
            }
            out.push('\n');
            for finding in found {
                out.push_str(&format!("  - {}", defang(&escape(finding.what.get(lang)))));
                if let Some(file) = &finding.file {
                    let place = match finding.line {
                        Some(line) => match lang {
                            Lang::En => format!("{file}, line {line}"),
                            Lang::Fr => format!("{file}, ligne {line}"),
                        },
                        None => file.clone(),
                    };
                    // No link when the files cannot be read anywhere yet.
                    let url = link(file, finding.line);
                    if url.is_empty() {
                        out.push_str(&format!(" ({})", escape(&place)));
                    } else {
                        out.push_str(&format!(" ([{}]({url}))", escape(&place)));
                    }
                }
                if let Some(excerpt) = &finding.excerpt {
                    out.push_str(&format!("<br><code>{}</code>", defang(&html(excerpt))));
                }
                out.push('\n');
            }
        }
        let note = match lang {
            Lang::En => {
                "\n🔴 blocking: the robot offers the mod only once it is fixed. 🟠 to look at: a maintainer decides. No automatic check can prove code is safe: it points at what a person must read.\n"
            }
            Lang::Fr => {
                "\n🔴 bloquant : le robot ne propose le mod qu'une fois corrigé. 🟠 à regarder : un mainteneur décide. Aucun contrôle automatique ne prouve qu'un code est sain : il montre ce qu'une personne doit lire.\n"
            }
        };
        out.push_str(note);
        out
    }
}

/// Web addresses from a mod, made unclickable (`https[:]//…`): GitHub turns
/// addresses into links, and a maintainer must not open one by mistake.
fn defang(text: &str) -> String {
    text.replace("://", "[:]//").replace("www.", "www[.]")
}

/// Text from the mod, safe in Markdown: no formatting, no mention, no HTML.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '!' | '|' | '~' | '>' | '<'
            | '&' => out.push_str(&format!("&#{};", c as u32)),
            '@' => out.push_str("&#64;"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '@' => out.push_str("&#64;"),
            '|' => out.push_str("&#124;"),
            '`' => out.push_str("&#96;"),
            c => out.push(c),
        }
    }
    out
}

/// Reviews the mod in `dir`. With `previous` (the accepted version's
/// folder), also shows what changed. The antivirus is left out: see
/// [`Report::add_antivirus`].
pub fn review(dir: &Path, previous: Option<&Path>) -> Report {
    let mut report = Report {
        id: None,
        kind: None,
        name: None,
        version: None,
        files: Vec::new(),
        addresses: Vec::new(),
        findings: Vec::new(),
        skipped: vec![Check::Antivirus],
        blocking: 0,
        to_look_at: 0,
    };
    match mods::load_dir(dir) {
        Ok(loaded) => {
            let meta = loaded.meta();
            report.id = Some(meta.id.clone());
            report.kind = Some(loaded.kind());
            report.name = Some(meta.name.clone());
            report.version = Some(meta.version.to_string());
        }
        Err(issue) => report.findings.push(Finding {
            check: Check::Valid,
            level: Level::Block,
            what: Text::new(issue.localized(Lang::En), issue.localized(Lang::Fr)),
            file: None,
            line: None,
            excerpt: None,
        }),
    }

    if let Err(err) = showcase::read(dir) {
        let text = Text::of_error(&err);
        report.findings.push(Finding {
            check: Check::Files,
            level: Level::Block,
            what: text,
            file: Some(showcase::DIR.to_owned()),
            line: None,
            excerpt: None,
        });
    }

    let scanned = scan_dir(dir, report.kind);
    report.files = scanned.files;
    report.findings.extend(scanned.findings);
    report.addresses = scanned.addresses.into_iter().collect();

    match previous {
        Some(old) => compare(&mut report, old),
        None => report.skipped.push(Check::Changes),
    }
    report.finish();
    report
}

struct Scanned {
    files: Vec<ReviewedFile>,
    findings: Vec<Finding>,
    addresses: BTreeSet<String>,
}

fn scan_dir(dir: &Path, kind: Option<ModKind>) -> Scanned {
    let mut scanned = Scanned {
        files: Vec::new(),
        findings: Vec::new(),
        addresses: BTreeSet::new(),
    };
    let mut paths = Vec::new();
    walk(dir, "", &mut paths, &mut scanned.findings);
    paths.sort();
    for path in paths {
        let bytes = match read_limited(&dir.join(&path)) {
            Some(bytes) => bytes,
            None => {
                scanned.findings.push(block_file(
                    Check::Files,
                    &path,
                    "This file is too large to be part of a mod.",
                    "Ce fichier est trop gros pour faire partie d'un mod.",
                ));
                continue;
            }
        };
        scanned.files.push(ReviewedFile {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        });
        scan_file(&path, &bytes, kind, &mut scanned);
    }
    scanned
}

/// Lists the files under `dir`, flagging hidden entries and links.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let shown = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            findings.push(block_file(
                Check::Files,
                &shown,
                "A shortcut (link) to another place: not allowed in a mod.",
                "Un raccourci (lien) vers un autre endroit : interdit dans un mod.",
            ));
            continue;
        }
        if name.starts_with('.') {
            findings.push(block_file(
                Check::Files,
                &shown,
                "A hidden file or folder: not allowed in a mod.",
                "Un fichier ou dossier caché : interdit dans un mod.",
            ));
            continue;
        }
        if meta.is_dir() {
            walk(&entry.path(), &shown, out, findings);
        } else {
            out.push(shown);
        }
    }
}

fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE {
        return None;
    }
    fs::read(path).ok()
}

/// What a file must be, from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expected {
    Text(TextKind),
    Png,
    Jpeg,
    Gif,
    Webp,
    Woff,
    Woff2,
    Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Toml,
    Css,
    Js,
    Svg,
    /// README and licence, which are allowed but not part of the gallery.
    Notes,
}

/// Files that may sit next to a mod without being part of it: notes, and
/// the editor helpers `spacetify new` writes (never sent to the gallery).
pub fn is_note(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    !path.contains('/')
        && (lower == "readme.md"
            || lower == "jsconfig.json"
            || lower == "spacetify.d.ts"
            || lower == "readme.txt"
            || lower == "license"
            || lower == "license.md"
            || lower == "license.txt"
            || lower == "licence"
            || lower == "licence.md"
            || lower == "licence.txt")
}

fn expected(path: &str, kind: Option<ModKind>) -> Option<Expected> {
    if is_note(path) {
        return Some(Expected::Text(TextKind::Notes));
    }
    // The showcase: pictures and texts for the gallery page only.
    if let Some(name) = path.strip_prefix("showcase/") {
        if name.contains('/') {
            return None;
        }
        return match name {
            showcase::ABOUT | showcase::ABOUT_FR => Some(Expected::Text(TextKind::Notes)),
            showcase::MANIFEST => Some(Expected::Text(TextKind::Toml)),
            _ => match showcase::image_type(name)? {
                "image/png" => Some(Expected::Png),
                "image/jpeg" => Some(Expected::Jpeg),
                "image/gif" => Some(Expected::Gif),
                _ => Some(Expected::Webp),
            },
        };
    }
    let theme_files = [theme::MANIFEST, theme::CSS_FILE, theme::COLORS_FILE];
    let extension_files = [extension::MANIFEST, extension::SCRIPT];
    let top = match kind {
        Some(ModKind::Theme) => theme_files.contains(&path),
        Some(ModKind::Extension) => extension_files.contains(&path),
        None => theme_files.contains(&path) || extension_files.contains(&path),
    };
    if top {
        return Some(match path.rsplit('.').next() {
            Some("css") => Expected::Text(TextKind::Css),
            Some("js") => Expected::Text(TextKind::Js),
            _ => Expected::Text(TextKind::Toml),
        });
    }
    let in_assets = kind != Some(ModKind::Extension)
        && path
            .strip_prefix(theme::ASSETS_DIR)
            .is_some_and(|rest| rest.starts_with('/'));
    if !in_assets {
        return None;
    }
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => Expected::Png,
        "jpg" | "jpeg" => Expected::Jpeg,
        "gif" => Expected::Gif,
        "webp" => Expected::Webp,
        "svg" => Expected::Text(TextKind::Svg),
        "woff" => Expected::Woff,
        "woff2" => Expected::Woff2,
        "ttf" | "otf" => Expected::Font,
        _ => return None,
    })
}

/// A program or an archive, whatever the file is called.
fn dangerous_content(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"MZ") {
        return Some(("a Windows program", "un programme Windows"));
    }
    if starts(b"\x7fELF") {
        return Some(("a Linux program", "un programme Linux"));
    }
    if [
        &b"\xfe\xed\xfa\xce"[..],
        b"\xfe\xed\xfa\xcf",
        b"\xce\xfa\xed\xfe",
        b"\xcf\xfa\xed\xfe",
        b"\xca\xfe\xba\xbe",
    ]
    .iter()
    .any(|magic| starts(magic))
    {
        return Some(("a Mac program", "un programme Mac"));
    }
    if starts(b"#!") {
        return Some(("a script for the computer", "un script pour l'ordinateur"));
    }
    if [
        &b"PK\x03\x04"[..],
        b"PK\x05\x06",
        b"Rar!",
        b"7z\xbc\xaf\x27\x1c",
        b"\x1f\x8b",
        b"MSCF",
        b"BZh",
        b"\xfd7zXZ",
    ]
    .iter()
    .any(|magic| starts(magic))
    {
        return Some(("an archive", "une archive"));
    }
    if starts(b"%PDF") {
        return Some(("a PDF document", "un document PDF"));
    }
    if starts(b"\xd0\xcf\x11\xe0") {
        return Some(("an Office document", "un document Office"));
    }
    None
}

fn matches_expected(bytes: &[u8], expected: Expected) -> bool {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    match expected {
        Expected::Png => starts(b"\x89PNG\r\n\x1a\n"),
        Expected::Jpeg => starts(b"\xff\xd8\xff"),
        Expected::Gif => starts(b"GIF87a") || starts(b"GIF89a"),
        Expected::Webp => bytes.len() > 12 && starts(b"RIFF") && &bytes[8..12] == b"WEBP",
        Expected::Woff => starts(b"wOFF"),
        Expected::Woff2 => starts(b"wOF2"),
        Expected::Font => starts(b"\0\x01\0\0") || starts(b"OTTO") || starts(b"true"),
        Expected::Text(_) => std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0),
    }
}

fn scan_file(path: &str, bytes: &[u8], kind: Option<ModKind>, scanned: &mut Scanned) {
    if let Some((en, fr)) = dangerous_content(bytes) {
        scanned.findings.push(block_file(
            Check::Files,
            path,
            &format!("This file is really {en}, whatever its name says."),
            &format!("Ce fichier est en réalité {fr}, quel que soit son nom."),
        ));
        return;
    }
    let Some(expected) = expected(path, kind) else {
        scanned.findings.push(block_file(
            Check::Files,
            path,
            "This file has no place in a mod of this kind. Only the files the mod uses are allowed (and a README or LICENSE).",
            "Ce fichier n'a pas sa place dans ce genre de mod. Seuls les fichiers dont le mod se sert sont permis (plus un README ou une LICENSE).",
        ));
        return;
    };
    if !matches_expected(bytes, expected) {
        scanned.findings.push(block_file(
            Check::Files,
            path,
            "This file is not what its name says.",
            "Ce fichier n'est pas ce que dit son nom.",
        ));
        return;
    }
    match expected {
        Expected::Text(text_kind) => {
            let text = String::from_utf8_lossy(bytes);
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
            scan_text(path, text, text_kind, scanned);
        }
        Expected::Png => check_size(path, png_size(bytes), scanned),
        Expected::Gif => check_size(path, gif_size(bytes), scanned),
        _ => {}
    }
}

fn check_size(path: &str, size: Option<(u32, u32)>, scanned: &mut Scanned) {
    if let Some((w, h)) = size
        && (w > MAX_PIXELS || h > MAX_PIXELS)
    {
        scanned.findings.push(Finding {
            check: Check::Images,
            level: Level::Look,
            what: Text::new(
                format!("A very large picture ({w} × {h} pixels), which can slow Spotify down."),
                format!("Une image très grande ({w} × {h} pixels), qui peut ralentir Spotify."),
            ),
            file: Some(path.to_owned()),
            line: None,
            excerpt: None,
        });
    }
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let w = bytes.get(16..20)?;
    let h = bytes.get(20..24)?;
    Some((
        u32::from_be_bytes(w.try_into().ok()?),
        u32::from_be_bytes(h.try_into().ok()?),
    ))
}

fn gif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let w = bytes.get(6..8)?;
    let h = bytes.get(8..10)?;
    Some((
        u16::from_le_bytes(w.try_into().ok()?).into(),
        u16::from_le_bytes(h.try_into().ok()?).into(),
    ))
}

/// A pattern looked for in a text file, case-insensitively.
struct Rule {
    check: Check,
    level: Level,
    needles: &'static [&'static str],
    en: &'static str,
    fr: &'static str,
}

const JS_RULES: &[Rule] = &[
    Rule {
        check: Check::Obfuscation,
        level: Level::Look,
        needles: &[
            "eval(",
            "new function",
            "function(\"",
            "function('",
            "settimeout(\"",
            "settimeout('",
            "settimeout(`",
            "setinterval(\"",
            "setinterval('",
            "setinterval(`",
            ".constructor.constructor",
            "[\"constructor\"]",
            "['constructor']",
        ],
        en: "Code that builds other code from text and runs it.",
        fr: "Du code qui fabrique un autre code à partir de texte et le lance.",
    },
    Rule {
        check: Check::Obfuscation,
        level: Level::Look,
        needles: &[
            "atob(",
            "fromcharcode",
            "unescape(",
            "decodeuricomponent(\"%",
        ],
        en: "Text decoded at run time: a common way to hide what code does.",
        fr: "Du texte décodé au moment de l'exécution : une façon courante de cacher ce que fait le code.",
    },
    Rule {
        check: Check::Obfuscation,
        level: Level::Look,
        needles: &["webassembly", "new worker(", "sharedworker("],
        en: "Code running apart from the page (WebAssembly or a worker): sometimes used to mine cryptocurrency.",
        fr: "Du code qui tourne à part de la page (WebAssembly ou worker) : parfois utilisé pour miner des cryptomonnaies.",
    },
    Rule {
        check: Check::Obfuscation,
        level: Level::Look,
        needles: &[
            "document.write(",
            "import(",
            "createelement(\"script",
            "createelement('script",
        ],
        en: "Code that loads more code into Spotify.",
        fr: "Du code qui charge encore du code dans Spotify.",
    },
    Rule {
        check: Check::Network,
        level: Level::Look,
        needles: &[
            "fetch(",
            "xmlhttprequest",
            "websocket(",
            "eventsource(",
            "sendbeacon(",
            "importscripts(",
            "rtcpeerconnection",
        ],
        en: "Sends or receives data over the internet.",
        fr: "Envoie ou reçoit des données par internet.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &[
            "accesstoken",
            "access_token",
            "authorization",
            "bearer ",
            "sp_dc",
            "sp_key",
            "/api/token",
            "spclient",
        ],
        en: "Touches the Spotify account's login (token, authorization). An extension almost never needs it.",
        fr: "Touche à la connexion du compte Spotify (jeton, autorisation). Une extension n'en a presque jamais besoin.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &["document.cookie"],
        en: "Reads or writes Spotify's cookies.",
        fr: "Lit ou écrit les cookies de Spotify.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &["localstorage", "sessionstorage", "indexeddb"],
        en: "Uses the data Spotify keeps in the page (storage). Fine for its own settings; check it does not read Spotify's.",
        fr: "Utilise les données gardées par Spotify dans la page (stockage). Normal pour ses propres réglages ; vérifier qu'il ne lit pas celles de Spotify.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &["navigator.clipboard", "execcommand(", "clipboarddata"],
        en: "Uses the clipboard.",
        fr: "Utilise le presse-papiers.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &[
            "\"keydown\"",
            "'keydown'",
            "\"keyup\"",
            "'keyup'",
            "\"keypress\"",
            "'keypress'",
            "onkeydown",
            "onkeyup",
            "onkeypress",
        ],
        en: "Listens to the keys typed. Fine for a shortcut; check it does not record what is typed.",
        fr: "Écoute les touches tapées. Normal pour un raccourci ; vérifier qu'il n'enregistre pas ce qui est tapé.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &[
            "window.open(",
            "location.href",
            "location.assign(",
            "location.replace(",
            "top.location",
            "location =",
        ],
        en: "Opens a page or leaves Spotify's page.",
        fr: "Ouvre une page ou quitte la page de Spotify.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Look,
        needles: &[
            "geolocation",
            "getusermedia",
            "mediadevices",
            "getdisplaymedia",
        ],
        en: "Asks for the position, the camera, the microphone or the screen.",
        fr: "Demande la position, la caméra, le micro ou l'écran.",
    },
    Rule {
        check: Check::Sensitive,
        level: Level::Block,
        needles: &[
            "child_process",
            "require(\"fs\")",
            "require('fs')",
            "process.binding",
        ],
        en: "Code meant to control the computer, not Spotify's page.",
        fr: "Du code fait pour piloter l'ordinateur, pas la page de Spotify.",
    },
];

const CSS_RULES: &[Rule] = &[Rule {
    check: Check::Obfuscation,
    level: Level::Block,
    needles: &[
        "expression(",
        "javascript:",
        "-moz-binding",
        "behavior:",
        "behaviour:",
    ],
    en: "CSS that tries to run code.",
    fr: "Du CSS qui essaie de lancer du code.",
}];

const SVG_RULES: &[Rule] = &[
    Rule {
        check: Check::Images,
        level: Level::Block,
        needles: &[
            "<script",
            "javascript:",
            "<foreignobject",
            "<iframe",
            "<embed",
            "<object",
            "<handler",
            "<listener",
        ],
        en: "A picture with code hidden inside.",
        fr: "Une image avec du code caché dedans.",
    },
    Rule {
        check: Check::Images,
        level: Level::Block,
        needles: &[
            " onload",
            " onerror",
            " onclick",
            " onmouseover",
            " onbegin",
            " onend",
            " onfocus",
            " onactivate",
        ],
        en: "A picture that runs code when it is shown or touched.",
        fr: "Une image qui lance du code quand elle s'affiche ou qu'on la touche.",
    },
];

fn scan_text(path: &str, text: &str, kind: TextKind, scanned: &mut Scanned) {
    let rules: &[Rule] = match kind {
        TextKind::Js => JS_RULES,
        TextKind::Css => CSS_RULES,
        TextKind::Svg => SVG_RULES,
        TextKind::Toml | TextKind::Notes => &[],
    };
    let mut escapes = 0usize;
    let mut first_escape = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        hidden_characters(path, number, line, kind, scanned);
        if matches!(kind, TextKind::Notes) {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        for rule in rules {
            if rule.needles.iter().any(|n| lower.contains(n)) {
                scanned.findings.push(line_finding(
                    rule.check, rule.level, rule.en, rule.fr, path, number, line,
                ));
            }
        }
        if kind != TextKind::Toml {
            for address in addresses(line) {
                scanned.findings.push(line_finding(
                    Check::Network,
                    Level::Look,
                    &format!("Contacts {address}."),
                    &format!("Contacte {address}."),
                    path,
                    number,
                    line,
                ));
                scanned.addresses.insert(address);
            }
        }
        if kind == TextKind::Css && lower.contains("@import") {
            scanned.findings.push(line_finding(
                Check::Network,
                Level::Look,
                "Loads another style sheet.",
                "Charge une autre feuille de style.",
                path,
                number,
                line,
            ));
        }
        if kind == TextKind::Js && line.chars().count() > LONG_LINE {
            scanned.findings.push(line_finding(
                Check::Obfuscation,
                Level::Look,
                "A very long line: the code may be packed (minified), which makes it hard to read. Ask for the readable version.",
                "Une ligne très longue : le code est peut-être compacté, donc difficile à lire. Demander la version lisible.",
                path,
                number,
                line,
            ));
        }
        if longest_blob(line) >= LONG_BLOB {
            scanned.findings.push(line_finding(
                Check::Obfuscation,
                Level::Look,
                "A long encoded block (base64): it may hide a file or code.",
                "Un long bloc codé (base64) : il peut cacher un fichier ou du code.",
                path,
                number,
                line,
            ));
        }
        let count = line.matches("\\x").count() + line.matches("\\u").count();
        if count > 0 && first_escape.is_none() {
            first_escape = Some((number, line.to_owned()));
        }
        escapes += count;
    }
    if matches!(kind, TextKind::Js | TextKind::Css) && escapes > 40 {
        let (number, line) = first_escape.unwrap_or_default();
        scanned.findings.push(line_finding(
            Check::Obfuscation,
            Level::Look,
            &format!("Many escaped characters ({escapes}): text written so it cannot be read."),
            &format!(
                "Beaucoup de caractères codés ({escapes}) : du texte écrit pour ne pas être lu."
            ),
            path,
            number,
            &line,
        ));
    }
}

/// Characters that cannot be seen, or that change the order text is shown
/// in, so the code read is not the code run ("Trojan Source").
fn hidden_characters(path: &str, number: usize, line: &str, kind: TextKind, scanned: &mut Scanned) {
    let mut reversing = false;
    let mut invisible = false;
    for c in line.chars() {
        match c {
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => reversing = true,
            '\u{200b}'..='\u{200f}'
            | '\u{2060}'..='\u{2064}'
            | '\u{feff}'
            | '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180e}'
            | '\u{3164}'
            | '\u{ffa0}'
            | '\u{e0000}'..='\u{e007f}' => invisible = true,
            c if c.is_control() && c != '\t' => invisible = true,
            _ => {}
        }
    }
    if reversing {
        scanned.findings.push(line_finding(
            Check::Hidden,
            Level::Block,
            "Characters that reverse the order of the text, so what is read is not what runs.",
            "Des caractères qui inversent l'ordre du texte : ce qu'on lit n'est pas ce qui s'exécute.",
            path,
            number,
            line,
        ));
    }
    if invisible {
        let level = if matches!(kind, TextKind::Js) {
            Level::Block
        } else {
            Level::Look
        };
        scanned.findings.push(line_finding(
            Check::Hidden,
            level,
            "Invisible characters, shown here as <U+…>.",
            "Des caractères invisibles, montrés ici en <U+…>.",
            path,
            number,
            line,
        ));
    }
}

/// Web addresses in a line: `http(s)://…`, `ws(s)://…` and `url(//…)`.
fn addresses(line: &str) -> Vec<String> {
    let lower = line.to_ascii_lowercase();
    let mut found = Vec::new();
    for scheme in ["https://", "http://", "wss://", "ws://"] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(scheme) {
            let start = from + at;
            // Not the end of a longer word, such as `xhttps://`.
            let glued = lower[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());
            let rest = &line[start..];
            let end = rest
                .find(|c: char| c.is_whitespace() || "\"'`()<>\\,;{}[]".contains(c))
                .unwrap_or(rest.len());
            if !glued && end > scheme.len() {
                found.push(rest[..end].to_owned());
            }
            from = start + scheme.len();
        }
    }
    let mut from = 0;
    while let Some(at) = lower[from..].find("url(") {
        let start = from + at + 4;
        let inner = lower[start..].trim_start_matches([' ', '"', '\'']);
        if let Some(host) = inner.strip_prefix("//") {
            let end = host
                .find(|c: char| c.is_whitespace() || "\"')".contains(c))
                .unwrap_or(host.len());
            if end > 0 {
                found.push(format!("//{}", &host[..end]));
            }
        }
        from = start;
    }
    // Addresses written in a comment or a string are still shown: a person
    // decides. Only the beginning of very long ones is kept. The W3C's are
    // names of formats (`xmlns`), never contacted.
    found
        .into_iter()
        .filter(|a| {
            let lower = a.to_ascii_lowercase();
            !lower.starts_with("http://www.w3.org/") && !lower.starts_with("https://www.w3.org/")
        })
        .map(|a| shorten(&a, 120))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn longest_blob(line: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for b in line.bytes() {
        if b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// The line as shown in a report: invisible characters made visible.
fn excerpt(line: &str) -> String {
    let mut shown = String::new();
    for c in line.trim().chars() {
        let hidden = matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2069}'
            | '\u{feff}'
            | '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180e}'
            | '\u{3164}'
            | '\u{ffa0}'
            | '\u{e0000}'..='\u{e007f}')
            || (c.is_control() && c != '\t');
        if hidden {
            shown.push_str(&format!("<U+{:04X}>", c as u32));
        } else {
            shown.push(c);
        }
    }
    shorten(&shown, MAX_EXCERPT)
}

fn line_finding(
    check: Check,
    level: Level,
    en: &str,
    fr: &str,
    path: &str,
    line: usize,
    text: &str,
) -> Finding {
    Finding {
        check,
        level,
        what: Text::new(en, fr),
        file: Some(path.to_owned()),
        line: (line > 0).then_some(line),
        excerpt: (!text.trim().is_empty()).then(|| excerpt(text)),
    }
}

fn block_file(check: Check, path: &str, en: &str, fr: &str) -> Finding {
    Finding {
        check,
        level: Level::Block,
        what: Text::new(en, fr),
        file: Some(path.to_owned()),
        line: None,
        excerpt: None,
    }
}

/// Lists what changed since the accepted version in `old`, and flags new
/// web addresses and new sensitive uses.
fn compare(report: &mut Report, old: &Path) {
    let before = scan_dir(old, report.kind);
    let old_files: BTreeMap<&str, &str> = before
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let new_files: BTreeMap<&str, &str> = report
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let mut changes = Vec::new();
    for (path, hash) in &new_files {
        match old_files.get(path) {
            None => changes.push((*path, "Added file.", "Fichier ajouté.")),
            Some(old) if old != hash => changes.push((*path, "Changed file.", "Fichier modifié.")),
            _ => {}
        }
    }
    for path in old_files.keys() {
        if !new_files.contains_key(path) {
            changes.push((*path, "Removed file.", "Fichier supprimé."));
        }
    }
    for (path, en, fr) in changes {
        report.findings.push(Finding {
            check: Check::Changes,
            level: Level::Ok,
            what: Text::new(en, fr),
            file: Some(path.to_owned()),
            line: None,
            excerpt: None,
        });
    }
    for address in &report.addresses {
        if !before.addresses.contains(address) {
            report.findings.push(Finding {
                check: Check::Changes,
                level: Level::Look,
                what: Text::new(
                    format!("New web address since the accepted version: {address}."),
                    format!("Nouvelle adresse internet depuis la version acceptée : {address}."),
                ),
                file: None,
                line: None,
                excerpt: None,
            });
        }
    }
    let known: BTreeSet<(Check, String)> = before
        .findings
        .iter()
        .filter(|f| f.check == Check::Sensitive)
        .map(|f| (f.check, f.what.en.clone()))
        .collect();
    let new: BTreeSet<String> = report
        .findings
        .iter()
        .filter(|f| f.check == Check::Sensitive && !known.contains(&(f.check, f.what.en.clone())))
        .map(|f| f.what.en.clone())
        .collect();
    for what in new {
        let fr = report
            .findings
            .iter()
            .find(|f| f.what.en == what)
            .map(|f| f.what.fr.clone())
            .unwrap_or_default();
        report.findings.push(Finding {
            check: Check::Changes,
            level: Level::Look,
            what: Text::new(
                format!("New since the accepted version: {what}"),
                format!("Nouveau depuis la version acceptée : {fr}"),
            ),
            file: None,
            line: None,
            excerpt: None,
        });
    }
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
