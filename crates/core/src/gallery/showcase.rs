//! The showcase of a mod: pictures, a longer description and a few tags,
//! shown in the gallery but never put into Spotify.
//!
//! It is the optional `showcase/` folder of a mod:
//!
//! ```text
//! my-theme/
//!   theme.toml …
//!   showcase/
//!     1-home.png      pictures (png, jpg, webp, gif), in name order;
//!     2-playlist.png  the first one is the cover of the gallery card
//!     about.md        the longer description, in English
//!     about.fr.md     the same in French (optional)
//!     showcase.toml   tags = ["dark", "pastel"]
//! ```
//!
//! Its files have their SHA-256 in the gallery list, like the mod's own
//! files. Installing a mod from the gallery never downloads them: the page
//! asks for each one when it shows it, and the service checks its hash.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{FileEntry, is_safe_path, sha256_hex};
use crate::service::Text;
use crate::{Error, Result};

/// Folder of the showcase inside a mod.
pub const DIR: &str = "showcase";
pub const ABOUT: &str = "about.md";
pub const ABOUT_FR: &str = "about.fr.md";
pub const MANIFEST: &str = "showcase.toml";

/// With these limits a showcase stays well under a mod's 10 MiB.
pub const MAX_IMAGES: usize = 6;
pub const MAX_IMAGE: u64 = 1024 * 1024;
pub const MAX_ABOUT: u64 = 16 * 1024;
pub const MAX_TAGS: usize = 6;
pub const MAX_TAG_LEN: usize = 20;
const MAX_MANIFEST: u64 = 4 * 1024;

/// The showcase as written in the gallery list. Paths are relative to the
/// mod's folder, so they all start with `showcase/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Showcase {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about_fr: Option<FileEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Showcase {
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
            && self.about.is_none()
            && self.about_fr.is_none()
            && self.tags.is_empty()
    }

    /// Every file, pictures first.
    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.images
            .iter()
            .chain(self.about.iter())
            .chain(self.about_fr.iter())
    }

    /// The file called `name` inside `showcase/`.
    pub fn file(&self, name: &str) -> Option<&FileEntry> {
        self.files()
            .find(|file| file.path.strip_prefix("showcase/") == Some(name))
    }

    /// Checks a showcase read from the gallery list. Anything odd refuses it.
    pub fn check(&self) -> std::result::Result<(), String> {
        if self.images.len() > MAX_IMAGES {
            return Err("too many pictures".into());
        }
        for file in &self.images {
            check_listed(file, |name| image_type(name).is_some())?;
        }
        if let Some(file) = &self.about {
            check_listed(file, |name| name == ABOUT)?;
        }
        if let Some(file) = &self.about_fr {
            check_listed(file, |name| name == ABOUT_FR)?;
        }
        if self.tags.len() > MAX_TAGS || !self.tags.iter().all(|tag| is_valid_tag(tag)) {
            return Err("bad tags".into());
        }
        Ok(())
    }
}

fn check_listed(
    file: &FileEntry,
    allowed: impl Fn(&str) -> bool,
) -> std::result::Result<(), String> {
    let name = file.path.strip_prefix("showcase/").unwrap_or_default();
    let hash_ok = file.sha256.len() == 64
        && file
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !is_safe_path(&file.path) || name.contains('/') || !allowed(name) || !hash_ok {
        return Err(format!("bad showcase file `{}`", file.path));
    }
    Ok(())
}

/// True for files of the showcase folder, which are not part of the mod.
pub fn is_showcase_path(path: &str) -> bool {
    path.strip_prefix(DIR)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// A tag: lowercase letters, digits and dashes, such as `dark` or `lo-fi`.
pub fn is_valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_LEN
        && !tag.starts_with('-')
        && !tag.ends_with('-')
        && tag
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The media type of a picture, from its name.
pub fn image_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    })
}

/// True if `bytes` really are a picture of the type its name says.
pub fn is_image_of_type(bytes: &[u8], media_type: &str) -> bool {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    match media_type {
        "image/png" => starts(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => starts(b"\xff\xd8\xff"),
        "image/gif" => starts(b"GIF87a") || starts(b"GIF89a"),
        "image/webp" => bytes.len() > 12 && starts(b"RIFF") && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

/// True if `bytes` are a valid about text: UTF-8, no NUL, not too long.
pub fn is_about_text(bytes: &[u8]) -> bool {
    bytes.len() as u64 <= MAX_ABOUT && std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    tags: Vec<String>,
}

fn refused(en: impl Into<String>, fr: impl Into<String>) -> Error {
    Error::Submission(Text::new(en, fr))
}

/// Reads and checks the showcase of the mod in `dir`. `None` when the mod
/// has no showcase folder, or an empty one.
pub fn read(dir: &Path) -> Result<Option<Showcase>> {
    let folder = dir.join(DIR);
    if !folder.exists() {
        return Ok(None);
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&folder).map_err(Error::io(&folder))? {
        let entry = entry.map_err(Error::io(&folder))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(Error::io(entry.path()))?;
        if !meta.is_file() {
            return Err(refused(
                format!(
                    "`{DIR}/{name}`: the showcase folder holds files only, no folders or links."
                ),
                format!(
                    "`{DIR}/{name}` : le dossier showcase ne contient que des fichiers, pas de dossiers ni de liens."
                ),
            ));
        }
        names.push(name);
    }
    names.sort();

    let mut showcase = Showcase::default();
    for name in names {
        let path = format!("{DIR}/{name}");
        if !is_safe_path(&path) {
            return Err(refused(
                format!(
                    "The file name `{path}` is not allowed: use letters, digits, `-`, `_` and `.` only."
                ),
                format!(
                    "Le nom de fichier `{path}` n'est pas permis : utilise seulement des lettres, des chiffres, `-`, `_` et `.`."
                ),
            ));
        }
        let file = folder.join(&name);
        let size = fs::metadata(&file).map_err(Error::io(&file))?.len();
        if name == MANIFEST {
            if size > MAX_MANIFEST {
                return Err(refused(
                    format!("`{path}` is too large."),
                    format!("`{path}` est trop gros."),
                ));
            }
            let text = fs::read_to_string(&file).map_err(Error::io(&file))?;
            let manifest: Manifest = toml::from_str(&text).map_err(|err| {
                refused(
                    format!("`{path}` is not valid: {}", err.message()),
                    format!("`{path}` n'est pas valide : {}", err.message()),
                )
            })?;
            for tag in manifest.tags {
                let tag = tag.trim().to_owned();
                if !is_valid_tag(&tag) {
                    return Err(refused(
                        format!(
                            "The tag `{tag}` is not allowed: lowercase letters, digits and `-`, {MAX_TAG_LEN} characters at most."
                        ),
                        format!(
                            "L'étiquette `{tag}` n'est pas permise : lettres minuscules, chiffres et `-`, {MAX_TAG_LEN} caractères au plus."
                        ),
                    ));
                }
                if !showcase.tags.contains(&tag) {
                    showcase.tags.push(tag);
                }
            }
            if showcase.tags.len() > MAX_TAGS {
                return Err(refused(
                    format!("At most {MAX_TAGS} tags."),
                    format!("{MAX_TAGS} étiquettes au plus."),
                ));
            }
            continue;
        }
        let about = name == ABOUT || name == ABOUT_FR;
        let media_type = image_type(&name);
        if !about && media_type.is_none() {
            return Err(refused(
                format!(
                    "`{path}` has no place in the showcase: only pictures (png, jpg, webp, gif), {ABOUT}, {ABOUT_FR} and {MANIFEST}."
                ),
                format!(
                    "`{path}` n'a pas sa place dans showcase : seulement des images (png, jpg, webp, gif), {ABOUT}, {ABOUT_FR} et {MANIFEST}."
                ),
            ));
        }
        let max = if about { MAX_ABOUT } else { MAX_IMAGE };
        if size > max {
            return Err(refused(
                format!("`{path}` is too large ({} KiB at most).", max / 1024),
                format!("`{path}` est trop gros ({} Kio au plus).", max / 1024),
            ));
        }
        let bytes = fs::read(&file).map_err(Error::io(&file))?;
        let ok = match media_type {
            Some(media_type) => is_image_of_type(&bytes, media_type),
            None => is_about_text(&bytes),
        };
        if !ok {
            return Err(refused(
                format!("`{path}` is not what its name says."),
                format!("`{path}` n'est pas ce que dit son nom."),
            ));
        }
        let entry = FileEntry {
            path,
            sha256: sha256_hex(&bytes),
        };
        match name.as_str() {
            ABOUT => showcase.about = Some(entry),
            ABOUT_FR => showcase.about_fr = Some(entry),
            _ => showcase.images.push(entry),
        }
    }
    if showcase.images.len() > MAX_IMAGES {
        return Err(refused(
            format!("At most {MAX_IMAGES} pictures in the showcase."),
            format!("{MAX_IMAGES} images au plus dans showcase."),
        ));
    }
    Ok((!showcase.is_empty()).then_some(showcase))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in files {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        dir
    }

    #[test]
    fn reads_pictures_texts_and_tags_in_order() {
        let dir = with(&[
            ("theme.toml", b"x"),
            ("showcase/2-player.png", PNG),
            ("showcase/1-home.png", PNG),
            ("showcase/about.md", b"# Hello"),
            ("showcase/about.fr.md", "# Bonjour".as_bytes()),
            (
                "showcase/showcase.toml",
                b"tags = [\"dark\", \"lo-fi\", \"dark\"]",
            ),
        ]);
        let showcase = read(dir.path()).unwrap().unwrap();
        let images: Vec<_> = showcase.images.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(images, ["showcase/1-home.png", "showcase/2-player.png"]);
        assert_eq!(showcase.about.unwrap().path, "showcase/about.md");
        assert_eq!(showcase.about_fr.unwrap().path, "showcase/about.fr.md");
        assert_eq!(showcase.tags, ["dark", "lo-fi"]);
    }

    #[test]
    fn no_folder_or_an_empty_one_is_no_showcase() {
        assert_eq!(read(with(&[("theme.toml", b"x")]).path()).unwrap(), None);
        let dir = with(&[("theme.toml", b"x")]);
        fs::create_dir(dir.path().join(DIR)).unwrap();
        assert_eq!(read(dir.path()).unwrap(), None);
    }

    #[test]
    fn refuses_odd_files() {
        let refused = |files: &[(&str, &[u8])]| read(with(files).path()).is_err();
        assert!(refused(&[("showcase/run.exe", b"MZ")]));
        assert!(refused(&[("showcase/fake.png", b"not a picture")]));
        assert!(refused(&[("showcase/about.md", b"a\0b")]));
        assert!(refused(&[("showcase/more/1.png", PNG)]));
        assert!(refused(&[(
            "showcase/showcase.toml",
            b"tags = [\"Bad Tag\"]"
        )]));
        assert!(refused(&[("showcase/showcase.toml", b"colour = \"red\"")]));
        let many: Vec<(String, &[u8])> = (0..=MAX_IMAGES)
            .map(|i| (format!("showcase/{i}.png"), PNG))
            .collect();
        let many: Vec<(&str, &[u8])> = many.iter().map(|(n, b)| (n.as_str(), *b)).collect();
        assert!(refused(&many));
    }

    #[test]
    fn checks_a_listed_showcase() {
        let file = |path: &str| FileEntry {
            path: path.into(),
            sha256: "0".repeat(64),
        };
        let good = Showcase {
            images: vec![file("showcase/1.png")],
            about: Some(file("showcase/about.md")),
            about_fr: None,
            tags: vec!["dark".into()],
        };
        assert!(good.check().is_ok());
        assert_eq!(good.file("1.png").unwrap().path, "showcase/1.png");
        assert!(good.file("theme.css").is_none());

        let mut bad = good.clone();
        bad.images = vec![file("theme.css")];
        assert!(bad.check().is_err());
        let mut bad = good.clone();
        bad.about = Some(file("showcase/../theme.css"));
        assert!(bad.check().is_err());
        let mut bad = good;
        bad.tags = vec!["<b>".into()];
        assert!(bad.check().is_err());
    }

    #[test]
    fn knows_its_files() {
        assert!(is_showcase_path("showcase/1.png"));
        assert!(!is_showcase_path("showcase.png"));
        assert!(!is_showcase_path("assets/showcase/1.png"));
        assert!(is_image_of_type(PNG, "image/png"));
        assert!(!is_image_of_type(PNG, "image/jpeg"));
    }
}
