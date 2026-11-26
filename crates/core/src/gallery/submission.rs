//! Sending a mod to the gallery.
//!
//! - On the author's computer, the desktop app checks the mod, puts it in a
//!   `.zip` in their Downloads folder and opens GitHub's request form in
//!   their browser ([`pack`], [`form_url`]). spacetify itself sends nothing
//!   over the internet.
//! - On GitHub, the robot reads the request ([`Form::parse`]), unpacks the
//!   `.zip` safely ([`unpack`]), reviews it ([`super::review`]) and, once a
//!   maintainer accepts, the mod is in the official list ([`add`]).

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::review::is_note;
use super::{Catalog, Entry, FileEntry, INDEX_PATH, Preview, is_safe_path, sha256_hex};
use crate::mods::{self, Loaded, ModKind, manifest};
use crate::service::Text;
use crate::{Error, Result};

/// The repository that holds the gallery.
pub const REPOSITORY: &str = "Adrix-fr/spacetify";
/// The request form, in `.github/ISSUE_TEMPLATE/`.
pub const FORM: &str = "submit-mod.yml";
/// Who owns each mod of the gallery: only they can send its updates.
pub const OWNERS_PATH: &str = "gallery/owners.json";
/// Owner of the team's own mods. It cannot be a GitHub account (it has
/// spaces), so the robot never takes an update of them: the team changes
/// them by hand.
pub const TEAM_OWNER: &str = "the spacetify team";
/// Where accepted mods are kept in the repository.
pub const MODS_DIR: &str = "gallery/mods";

/// Biggest `.zip` the robot opens: a whole repository can be in it.
pub const MAX_ARCHIVE: u64 = 25 * 1024 * 1024;
const MAX_ENTRIES: usize = 5000;
/// Biggest single file, as when installing from the gallery.
const MAX_FILE: u64 = 2 * 1024 * 1024;

fn refused(en: impl Into<String>, fr: impl Into<String>) -> Error {
    Error::Submission(Text::new(en, fr))
}

// ------------------------------------------------------------------ author

/// The files of the mod in `dir` that go to the gallery: everything but
/// hidden files and a README or licence. Relative, with `/`, sorted.
pub fn mod_files(dir: &Path) -> Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(Error::io(dir))? {
            let entry = entry.map_err(Error::io(dir))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(Error::io(&path))?;
            if meta.file_type().is_symlink() {
                return Err(refused(
                    format!(
                        "`{name}` is a shortcut (link): copy the real file into the mod instead."
                    ),
                    format!(
                        "`{name}` est un raccourci (lien) : mets plutôt le vrai fichier dans le mod."
                    ),
                ));
            }
            if meta.is_dir() {
                walk(root, &path, out)?;
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if !is_note(&relative) {
                    out.push(relative);
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

/// Puts the mod in `dir` in `<into>/<id>-<version>.zip` and returns that
/// path. The mod must be valid.
pub fn pack(dir: &Path, into: &Path) -> Result<PathBuf> {
    let loaded = mods::load_dir(dir).map_err(|issue| Error::InvalidMod {
        what: dir.display().to_string(),
        issue,
    })?;
    let meta = loaded.meta();
    let files = mod_files(dir)?;
    let zip_err = |err: zip::result::ZipError| refused(err.to_string(), err.to_string());
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for file in &files {
        if !is_safe_path(file) {
            return Err(refused(
                format!(
                    "The file name `{file}` cannot be sent: use letters, digits, `-`, `_` and `.` only."
                ),
                format!(
                    "Le nom de fichier `{file}` ne peut pas être envoyé : utilise seulement des lettres, des chiffres, `-`, `_` et `.`."
                ),
            ));
        }
        let bytes = fs::read(dir.join(file)).map_err(Error::io(dir.join(file)))?;
        out.start_file(file.as_str(), options).map_err(zip_err)?;
        out.write_all(&bytes).map_err(Error::io(dir.join(file)))?;
    }
    let bytes = out.finish().map_err(zip_err)?.into_inner();
    fs::create_dir_all(into).map_err(Error::io(into))?;
    let target = into.join(format!("{}-{}.zip", meta.id, meta.version));
    fs::write(&target, bytes).map_err(Error::io(&target))?;
    Ok(target)
}

/// The user's Downloads folder, where the `.zip` goes.
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

/// GitHub's request form, already filled in for the mod.
pub fn form_url(loaded: &Loaded, update: bool) -> String {
    let meta = loaded.meta();
    let kind = match loaded.kind() {
        ModKind::Theme => "Theme",
        ModKind::Extension => "Extension",
    };
    let mut fields = vec![
        ("template", FORM.to_owned()),
        ("title", format!("[Mod] {} {}", meta.name, meta.version)),
        ("name", meta.name.clone()),
        ("kind", kind.to_owned()),
        ("version", meta.version.to_string()),
        (
            "request",
            if update { "Update" } else { "New mod" }.to_owned(),
        ),
    ];
    if let Some(description) = &meta.description {
        fields.push(("description", description.clone()));
    }
    let query: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect();
    format!(
        "https://github.com/{REPOSITORY}/issues/new?{}",
        query.join("&")
    )
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ------------------------------------------------------------------ robot

/// Where the robot gets the mod from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Origin {
    /// A `.zip` attached to the request.
    Attachment { url: String },
    /// A tag, branch or commit of a public GitHub repository.
    Repository {
        owner: String,
        repo: String,
        reference: String,
        url: String,
    },
}

/// A request, as written in the form on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Form {
    pub name: String,
    pub kind: ModKind,
    pub version: String,
    pub update: bool,
    pub description: Option<String>,
    pub description_fr: Option<String>,
    pub origin: Origin,
    /// Folder of the mod inside the `.zip` or the repository.
    pub folder: Option<String>,
}

impl Form {
    /// Reads the body of a request made with the form. Each answer is under
    /// a `### Label` heading; GitHub writes `_No response_` for an empty one.
    pub fn parse(body: &str) -> Result<Self> {
        let mut answers: BTreeMap<String, String> = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in body.lines() {
            if let Some(label) = line.strip_prefix("### ") {
                let key = label
                    .split(" / ")
                    .next()
                    .unwrap_or(label)
                    .trim()
                    .to_ascii_lowercase();
                answers.entry(key.clone()).or_default();
                current = Some(key);
            } else if let Some(key) = &current {
                let value = answers.entry(key.clone()).or_default();
                value.push_str(line);
                value.push('\n');
            }
        }
        let get = |key: &str| {
            answers
                .get(key)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty() && *v != "_No response_" && *v != "None")
                .map(str::to_owned)
        };
        let missing = |what: &str, quoi: &str| {
            refused(
                format!("The request has no {what}. Edit it and fill that in."),
                format!("La demande n'a pas de {quoi}. Modifie-la pour le remplir."),
            )
        };

        let name = get("mod name").ok_or_else(|| missing("mod name", "nom de mod"))?;
        let kind = match get("kind").map(|k| k.to_ascii_lowercase()).as_deref() {
            Some(k) if k.starts_with("theme") => ModKind::Theme,
            Some(k) if k.starts_with("extension") => ModKind::Extension,
            _ => {
                return Err(missing(
                    "kind (theme or extension)",
                    "type (thème ou extension)",
                ));
            }
        };
        let version = get("version").ok_or_else(|| missing("version", "version"))?;
        let update = get("request").is_some_and(|r| r.to_ascii_lowercase().starts_with("update"));
        let description = get("description (english)").map(|d| one_line(&d, 300));
        let description_fr = get("description (french)").map(|d| one_line(&d, 300));
        let folder = get("folder in the repository")
            .or_else(|| get("folder"))
            .map(|f| f.trim_matches('/').to_owned());
        if let Some(folder) = &folder
            && !is_safe_path(folder)
        {
            return Err(refused(
                format!("The folder `{folder}` is not a plain path such as `themes/aurora`."),
                format!("Le dossier `{folder}` n'est pas un chemin simple comme `themes/aurora`."),
            ));
        }
        if !get("rights").is_some_and(|r| r.contains("[X]") || r.contains("[x]")) {
            return Err(refused(
                "The request must confirm that you may share this mod.",
                "La demande doit confirmer que tu as le droit de partager ce mod.",
            ));
        }

        let attachment = get("the mod (.zip)")
            .or_else(|| get("the mod"))
            .and_then(|text| attachment_url(&text));
        let repository = get("or a github repository").or_else(|| get("github repository"));
        let reference = get("tag or commit");
        let origin = match (attachment, repository) {
            (Some(url), _) => Origin::Attachment { url },
            (None, Some(repository)) => {
                let (owner, repo) = repository_of(&repository).ok_or_else(|| {
                    refused(
                        "The repository must be written like https://github.com/name/repository.",
                        "Le dépôt doit être écrit comme https://github.com/nom/depot.",
                    )
                })?;
                let reference = reference
                    .filter(|r| is_reference(r))
                    .ok_or_else(|| {
                        refused(
                            "With a repository, give the tag or commit of this version (such as v1.0.0).",
                            "Avec un dépôt, indique le tag ou le commit de cette version (comme v1.0.0).",
                        )
                    })?;
                let url = format!("https://codeload.github.com/{owner}/{repo}/zip/{reference}");
                Origin::Repository {
                    owner,
                    repo,
                    reference,
                    url,
                }
            }
            (None, None) => {
                return Err(refused(
                    "The request has no .zip and no repository. Drop the .zip spacetify made into the form.",
                    "La demande n'a ni .zip ni dépôt. Glisse dans le formulaire le .zip préparé par spacetify.",
                ));
            }
        };
        Ok(Self {
            name: one_line(&name, 60),
            kind,
            version,
            update,
            description,
            description_fr,
            origin,
            folder,
        })
    }
}

fn one_line(text: &str, max: usize) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(max).collect()
}

/// The first `.zip` uploaded to GitHub in `text`.
fn attachment_url(text: &str) -> Option<String> {
    const PREFIXES: [&str; 2] = [
        "https://github.com/user-attachments/files/",
        "https://github.com/Adrix-fr/spacetify/files/",
    ];
    for prefix in PREFIXES {
        let mut from = 0;
        while let Some(at) = text[from..].find(prefix) {
            let start = from + at;
            let rest = &text[start + prefix.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || "-_./".contains(c)))
                .unwrap_or(rest.len());
            let tail = &rest[..end];
            let mut parts = tail.splitn(2, '/');
            let number = parts.next().unwrap_or_default();
            let file = parts.next().unwrap_or_default();
            if !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && file.to_ascii_lowercase().ends_with(".zip")
                && !file.contains('/')
                && !file.starts_with('.')
            {
                return Some(format!("{prefix}{tail}"));
            }
            from = start + prefix.len();
        }
    }
    None
}

fn repository_of(text: &str) -> Option<(String, String)> {
    let rest = text.trim().strip_prefix("https://github.com/")?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, repo) = rest.split_once('/')?;
    let owner_ok = (1..=39).contains(&owner.len())
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !owner.starts_with('-');
    let repo_ok = (1..=100).contains(&repo.len())
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !repo.starts_with('.');
    (owner_ok && repo_ok).then(|| (owner.to_owned(), repo.to_owned()))
}

fn is_reference(text: &str) -> bool {
    (1..=100).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
        && !text.contains("..")
        && !text.starts_with(['-', '/', '.'])
        && !text.ends_with('/')
}

/// Opens the `.zip` at `archive` into `into` (which must not exist) and
/// returns the mod's folder in it: the folder `folder`, or the only folder
/// at the top, or the top itself. Refuses paths that leave the folder,
/// links and anything too large. Hidden files are kept, so the review shows
/// them, except the leftovers macOS adds (`__MACOSX`, `.DS_Store`).
pub fn unpack(archive: &Path, into: &Path, folder: Option<&str>) -> Result<PathBuf> {
    let bad = |reason: String| {
        refused(
            format!("The .zip cannot be opened safely: {reason}."),
            format!("Le .zip ne peut pas être ouvert sans risque : {reason}."),
        )
    };
    let size = fs::metadata(archive).map_err(Error::io(archive))?.len();
    if size > MAX_ARCHIVE {
        return Err(bad(format!(
            "it is larger than {} MB",
            MAX_ARCHIVE / 1024 / 1024
        )));
    }
    let file = fs::File::open(archive).map_err(Error::io(archive))?;
    let mut zip = ZipArchive::new(file).map_err(|err| bad(err.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(bad("it holds too many files".into()));
    }

    let mut names = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|err| bad(err.to_string()))?;
        let raw = entry
            .name()
            .map_err(|err| bad(err.to_string()))?
            .into_owned();
        let name = raw.replace('\\', "/");
        let junk = name.starts_with("__MACOSX/") || name.rsplit('/').next() == Some(".DS_Store");
        if junk || entry.is_dir() {
            continue;
        }
        if entry.is_symlink() {
            return Err(bad(format!("`{name}` is a link")));
        }
        if entry.encrypted() {
            return Err(bad(format!("`{name}` has a password")));
        }
        let parts: Vec<&str> = name.split('/').collect();
        if name.starts_with('/')
            || name.contains(':')
            || parts
                .iter()
                .any(|p| p.is_empty() || *p == "." || *p == "..")
            || entry.enclosed_name().is_none()
        {
            return Err(bad(format!("`{name}` points outside the folder")));
        }
        names.push((i, name));
    }

    // The mod's folder inside the archive.
    let top: Option<String> = {
        let firsts: std::collections::BTreeSet<&str> = names
            .iter()
            .map(|(_, n)| n.split('/').next().unwrap_or_default())
            .collect();
        let all_nested = names.iter().all(|(_, n)| n.contains('/'));
        (firsts.len() == 1 && all_nested)
            .then(|| firsts.into_iter().next().unwrap_or_default().to_owned())
    };
    let mut base = String::new();
    if let Some(top) = &top {
        base.push_str(top);
        base.push('/');
    }
    if let Some(folder) = folder {
        base.push_str(folder.trim_matches('/'));
        base.push('/');
    }

    fs::create_dir_all(into).map_err(Error::io(into))?;
    let mut count = 0usize;
    let mut total = 0u64;
    for (i, name) in names {
        let Some(relative) = name.strip_prefix(&base) else {
            continue;
        };
        count += 1;
        if count > mods::MAX_FILES {
            return Err(bad("the mod holds too many files".into()));
        }
        let mut entry = zip.by_index(i).map_err(|err| bad(err.to_string()))?;
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| bad(err.to_string()))?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(bad(format!("`{relative}` is larger than 2 MB")));
        }
        total += bytes.len() as u64;
        if total > mods::MAX_TOTAL {
            return Err(bad("the mod is larger than 10 MB".into()));
        }
        let target = into.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        fs::write(&target, &bytes).map_err(Error::io(&target))?;
    }
    if count == 0 {
        return Err(bad(match folder {
            Some(folder) => format!("there is no folder `{folder}` in it"),
            None => "it is empty".into(),
        }));
    }
    Ok(into.to_path_buf())
}

/// Who owns each mod: GitHub account by mod id.
pub fn read_owners(repo: &Path) -> Result<BTreeMap<String, String>> {
    let path = repo.join(OWNERS_PATH);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| Error::GalleryInvalid(format!("{OWNERS_PATH}: {err}"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(Error::io(&path)(err)),
    }
}

/// The folder of the version of `id` already in the gallery of `repo`.
pub fn accepted_folder(repo: &Path, id: &str) -> Result<Option<PathBuf>> {
    let catalog = read_catalog(repo)?;
    Ok(catalog.get(id).map(|entry| repo.join(&entry.path)))
}

fn read_catalog(repo: &Path) -> Result<Catalog> {
    let path = repo.join(INDEX_PATH);
    let bytes = fs::read(&path).map_err(Error::io(&path))?;
    Catalog::parse(&bytes)
}

/// What [`add`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Added {
    pub id: String,
    pub kind: ModKind,
    pub name: String,
    pub version: String,
    /// The mod was in the gallery already, at this older version.
    pub previous: Option<String>,
    /// Folder of the mod in the repository.
    pub path: String,
}

/// Puts the mod in `dir` into the gallery of the repository `repo`:
/// copies its files to `gallery/mods/<id>/`, writes their SHA-256 in the
/// list and remembers that `owner` (a GitHub account) owns it.
///
/// A new mod needs a free id. An update must come from the same owner and
/// have a higher version. `form` is checked against the mod.
pub fn add(dir: &Path, repo: &Path, owner: &str, form: &Form) -> Result<Added> {
    let loaded = mods::load_dir(dir).map_err(|issue| Error::InvalidMod {
        what: form.name.clone(),
        issue,
    })?;
    let meta = loaded.meta().clone();
    let id = meta.id.clone();
    if loaded.kind() != form.kind {
        return Err(refused(
            format!(
                "The form says {}, but the files are a {}.",
                form.kind.label(crate::Lang::En),
                loaded.kind().label(crate::Lang::En)
            ),
            format!(
                "Le formulaire dit {}, mais les fichiers sont un(e) {}.",
                form.kind.label(crate::Lang::Fr),
                loaded.kind().label(crate::Lang::Fr)
            ),
        ));
    }
    if meta.version.to_string() != form.version.trim().trim_start_matches('v') {
        return Err(refused(
            format!(
                "The form says version {}, but the manifest says {}.",
                form.version, meta.version
            ),
            format!(
                "Le formulaire dit version {}, mais le manifeste dit {}.",
                form.version, meta.version
            ),
        ));
    }
    if !manifest::is_valid_id(&id) {
        return Err(refused(
            format!("`{id}` is not a valid id."),
            format!("`{id}` n'est pas un identifiant valide."),
        ));
    }

    // Pictures and texts for the gallery page, checked before anything is copied.
    let showcase = super::showcase::read(dir)?;

    let mut catalog = read_catalog(repo)?;
    let mut owners = read_owners(repo)?;
    let existing = catalog.mods.iter().position(|e| e.id == id);
    let previous = match existing {
        None => None,
        Some(at) => {
            let entry = &catalog.mods[at];
            let current_owner = owners.get(&id).map(String::as_str);
            if !current_owner.is_some_and(|o| o.eq_ignore_ascii_case(owner)) {
                return Err(refused(
                    format!(
                        "The id `{id}` is already used in the gallery by another author. Choose another id in the manifest."
                    ),
                    format!(
                        "L'identifiant `{id}` est déjà pris dans la galerie par un autre auteur. Choisis un autre identifiant dans le manifeste."
                    ),
                ));
            }
            if entry.kind != loaded.kind() {
                return Err(refused(
                    format!(
                        "`{id}` is a {} in the gallery.",
                        entry.kind.label(crate::Lang::En)
                    ),
                    format!(
                        "`{id}` est un(e) {} dans la galerie.",
                        entry.kind.label(crate::Lang::Fr)
                    ),
                ));
            }
            let older = semver::Version::parse(&entry.version).ok();
            if older.is_some_and(|old| meta.version <= old) {
                return Err(refused(
                    format!(
                        "Version {} is already in the gallery: raise the version in the manifest.",
                        entry.version
                    ),
                    format!(
                        "La version {} est déjà dans la galerie : augmente la version dans le manifeste.",
                        entry.version
                    ),
                ));
            }
            Some(entry.version.clone())
        }
    };

    // Copies the files that are part of the mod.
    let path = format!("{MODS_DIR}/{id}");
    let target = repo.join(&path);
    match fs::remove_dir_all(&target) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::io(&target)(err));
        }
        _ => {}
    }
    let mut files = Vec::new();
    for file in mod_files(dir)? {
        if !is_safe_path(&file) {
            return Err(refused(
                format!("The file name `{file}` is not allowed."),
                format!("Le nom de fichier `{file}` n'est pas permis."),
            ));
        }
        let bytes = fs::read(dir.join(&file)).map_err(Error::io(dir.join(&file)))?;
        let to = target.join(&file);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        fs::write(&to, &bytes).map_err(Error::io(&to))?;
        // The showcase is listed apart: it is never installed.
        if !super::showcase::is_showcase_path(&file) {
            files.push(FileEntry {
                path: file,
                sha256: sha256_hex(&bytes),
            });
        }
    }

    let entry = Entry {
        id: id.clone(),
        kind: loaded.kind(),
        name: meta.name.clone(),
        author: meta.author.clone(),
        version: meta.version.to_string(),
        description: form
            .description
            .clone()
            .or_else(|| meta.description.clone())
            .unwrap_or_default(),
        description_fr: form.description_fr.clone(),
        path: path.clone(),
        files,
        preview: match &loaded {
            Loaded::Theme(theme) => Some(Preview::of_colors(&theme.colors)),
            Loaded::Extension(_) => None,
        },
        showcase,
        updated: Some(super::today()),
    };
    match existing {
        Some(at) => catalog.mods[at] = entry,
        None => catalog.mods.push(entry),
    }
    owners.insert(id.clone(), owner.to_owned());

    let mut json = serde_json::to_string_pretty(&catalog).expect("catalog is serializable");
    json.push('\n');
    Catalog::parse(json.as_bytes())?;
    let index = repo.join(INDEX_PATH);
    fs::write(&index, json).map_err(Error::io(&index))?;
    let mut owners_json = serde_json::to_string_pretty(&owners).expect("owners are serializable");
    owners_json.push('\n');
    let owners_path = repo.join(OWNERS_PATH);
    fs::write(&owners_path, owners_json).map_err(Error::io(&owners_path))?;

    Ok(Added {
        id,
        kind: loaded.kind(),
        name: meta.name,
        version: meta.version.to_string(),
        previous,
        path,
    })
}

#[cfg(test)]
#[path = "submission_tests.rs"]
mod tests;
