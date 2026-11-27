use std::path::{Path, PathBuf};

use super::*;
use crate::mods::ModStore;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A copy of the gallery's own repository (spacetify-mod/gallery), given by
/// `SPACETIFY_GALLERY_DIR`. Its CI sets it; elsewhere the tests that need it
/// are skipped.
pub(super) fn gallery_root() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SPACETIFY_GALLERY_DIR")?);
    dir.join(INDEX_PATH).is_file().then_some(dir)
}

/// Every file of a mod folder, relative to it, sorted.
fn files_of(dir: &Path) -> Vec<FileEntry> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<FileEntry>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                out.push(FileEntry {
                    path: relative.to_string_lossy().replace('\\', "/"),
                    sha256: sha256_hex(&fs::read(&path).unwrap()),
                });
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The files of the mod itself, without its showcase.
fn mod_part(dir: &Path) -> Vec<FileEntry> {
    files_of(dir)
        .into_iter()
        .filter(|file| !showcase::is_showcase_path(&file.path))
        .collect()
}

/// A copy of part of the repository, with a list for `folders`.
fn fake_repo(folders: &[&str]) -> (tempfile::TempDir, Catalog) {
    let dir = tempfile::tempdir().unwrap();
    let mut mods = Vec::new();
    for folder in folders {
        let src = repo_root().join(folder);
        let dst = dir.path().join(folder);
        for file in files_of(&src) {
            let target = dst.join(&file.path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(src.join(&file.path), target).unwrap();
        }
        let loaded = mods::load_dir(&src).unwrap();
        let meta = loaded.meta();
        mods.push(Entry {
            id: meta.id.clone(),
            kind: loaded.kind(),
            name: meta.name.clone(),
            author: meta.author.clone(),
            version: meta.version.to_string(),
            description: meta.description.clone().unwrap_or_default(),
            description_fr: None,
            path: (*folder).to_owned(),
            files: mod_part(&src),
            preview: match &loaded {
                Loaded::Theme(theme) => Some(Preview::of_colors(&theme.colors)),
                Loaded::Extension(_) => None,
            },
            showcase: showcase::read(&src).unwrap(),
            updated: None,
        });
    }
    let catalog = Catalog { format: 1, mods };
    let index = dir.path().join(INDEX_PATH);
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(&index, serde_json::to_vec_pretty(&catalog).unwrap()).unwrap();
    (dir, catalog)
}

#[test]
fn the_official_list_matches_the_files_in_the_repository() {
    let Some(root) = gallery_root() else {
        eprintln!(
            "SPACETIFY_GALLERY_DIR is not set: the gallery's list is checked in its own repository"
        );
        return;
    };
    let catalog = Catalog::parse(&fs::read(root.join(INDEX_PATH)).unwrap()).unwrap();
    assert!(!catalog.mods.is_empty());
    for entry in &catalog.mods {
        let dir = root.join(&entry.path);
        let files = mod_part(&dir);
        assert_eq!(
            entry.files, files,
            "the files of `{}` changed: update gallery/index.json",
            entry.id
        );
        assert_eq!(
            entry.showcase,
            showcase::read(&dir).unwrap(),
            "the showcase of `{}` changed: update gallery/index.json",
            entry.id
        );
        let loaded = mods::load_dir(&dir).unwrap();
        assert_eq!(loaded.kind(), entry.kind, "{}", entry.id);
        assert_eq!(loaded.meta().id, entry.id);
        assert_eq!(
            loaded.meta().version.to_string(),
            entry.version,
            "{}",
            entry.id
        );
        if let Loaded::Theme(theme) = &loaded {
            assert_eq!(
                entry.preview,
                Some(Preview::of_colors(&theme.colors)),
                "the preview of `{}` does not match its colors.toml",
                entry.id
            );
        }
    }
}

#[test]
fn installs_from_the_gallery_after_checking_every_file() {
    let (repo, catalog) = fake_repo(&[
        "examples/themes/midnight",
        "examples/extensions/hello-badge",
    ]);
    let source = Source::Folder(repo.path().to_path_buf());
    assert_eq!(source.catalog().unwrap(), catalog);

    let home = tempfile::tempdir().unwrap();
    let store = ModStore::new(home.path());
    let installed = install(&source, catalog.get("midnight").unwrap(), &store).unwrap();
    assert_eq!(installed.kind, ModKind::Theme);
    assert!(store.load_theme("midnight").is_ok());
    install(&source, catalog.get("hello-badge").unwrap(), &store).unwrap();
    assert!(store.load_extension("hello-badge").is_ok());
    // Nothing is left behind.
    assert!(!store.downloads_dir().join("midnight").exists());

    let config = Config::default();
    let views = views(Some(&catalog), &source, &store, &config).unwrap();
    assert_eq!(views.len(), 2);
    assert!(
        views
            .iter()
            .all(|v| v.in_gallery && v.installed.is_some() && !v.update)
    );
    assert!(views[0].preview.is_some());
    assert_eq!(views[0].code_url, None);

    // Verified while it is exactly what the gallery accepted.
    assert!(is_verified(&catalog, &store, ModKind::Theme, "midnight"));
    assert!(!is_verified(
        &catalog,
        &store,
        ModKind::Extension,
        "midnight"
    ));
    assert!(!is_verified(&catalog, &store, ModKind::Theme, "nord"));
    let main = store
        .path_of(ModKind::Extension, "hello-badge")
        .join("main.js");
    assert!(is_verified(
        &catalog,
        &store,
        ModKind::Extension,
        "hello-badge"
    ));
    fs::write(&main, "alert('changed by hand');").unwrap();
    assert!(!is_verified(
        &catalog,
        &store,
        ModKind::Extension,
        "hello-badge"
    ));
}

#[test]
fn a_changed_file_is_refused_and_nothing_is_installed() {
    let (repo, catalog) = fake_repo(&["examples/extensions/hello-badge"]);
    let main = repo.path().join("examples/extensions/hello-badge/main.js");
    fs::write(&main, "alert('not the reviewed code');").unwrap();

    let home = tempfile::tempdir().unwrap();
    let store = ModStore::new(home.path());
    let source = Source::Folder(repo.path().to_path_buf());
    let err = install(&source, &catalog.mods[0], &store).unwrap_err();
    assert!(
        matches!(&err, Error::GalleryFileMismatch { file, .. } if file == "main.js"),
        "{err}"
    );
    assert!(store.kind_of("hello-badge").is_none());
    assert!(!store.downloads_dir().join("hello-badge").exists());
}

#[test]
fn the_showcase_is_shown_but_never_installed() {
    let (repo, mut catalog) = fake_repo(&["examples/themes/midnight"]);
    let folder = repo.path().join("examples/themes/midnight/showcase");
    fs::create_dir_all(&folder).unwrap();
    let png = b"\x89PNG\r\n\x1a\nrest of the picture";
    fs::write(folder.join("1.png"), png).unwrap();
    catalog.mods[0].showcase = Some(Showcase {
        images: vec![FileEntry {
            path: "showcase/1.png".into(),
            sha256: sha256_hex(png),
        }],
        ..Showcase::default()
    });
    let source = Source::Folder(repo.path().to_path_buf());
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::in_folder(home.path());
    let entry = &catalog.mods[0];

    // Fetched and kept by its hash, so it is read from disk the next time.
    assert_eq!(showcase_file(&source, &dirs, entry, "1.png").unwrap(), png);
    fs::write(folder.join("1.png"), b"changed").unwrap();
    assert_eq!(showcase_file(&source, &dirs, entry, "1.png").unwrap(), png);
    // A changed file that is not kept yet is refused.
    fs::remove_dir_all(showcase_cache_dir(&dirs)).unwrap();
    assert!(matches!(
        showcase_file(&source, &dirs, entry, "1.png"),
        Err(Error::GalleryFileMismatch { .. })
    ));
    // Only files the list names can be asked for.
    assert!(showcase_file(&source, &dirs, entry, "theme.css").is_err());

    // Installing takes the mod's files only.
    let store = ModStore::new(home.path());
    install(&source, entry, &store).unwrap();
    assert!(
        !store
            .dir_of(ModKind::Theme)
            .join("midnight/showcase")
            .exists()
    );

    let views = views(Some(&catalog), &source, &store, &Config::default()).unwrap();
    let shown = views[0].showcase.as_ref().unwrap();
    assert_eq!(shown.images, ["1.png"]);
    assert!(!shown.about);
}

#[test]
fn a_mod_that_is_not_what_the_list_says_is_refused() {
    let (repo, mut catalog) = fake_repo(&["examples/themes/midnight"]);
    catalog.mods[0].version = "9.9.9".into();
    let home = tempfile::tempdir().unwrap();
    let store = ModStore::new(home.path());
    let source = Source::Folder(repo.path().to_path_buf());
    let err = install(&source, &catalog.mods[0], &store).unwrap_err();
    assert!(matches!(err, Error::GalleryModMismatch(_)), "{err}");
    assert!(store.kind_of("midnight").is_none());
}

#[test]
fn installed_mods_outside_the_gallery_are_listed_too() {
    let (repo, catalog) = fake_repo(&["examples/themes/midnight"]);
    let home = tempfile::tempdir().unwrap();
    let store = ModStore::new(home.path());
    store
        .install(&repo_root().join("examples/extensions/hello-badge"))
        .unwrap();
    let mut config = Config::default();
    store.enable(&mut config, "hello-badge").unwrap();
    let source = Source::Folder(repo.path().to_path_buf());
    let views = views(Some(&catalog), &source, &store, &config).unwrap();
    assert_eq!(views.len(), 2);
    assert!(views[0].in_gallery && views[0].installed.is_none());
    assert!(!views[1].in_gallery && views[1].active);
    // Without the list (no internet), installed mods still show.
    let offline = super::views(None, &source, &store, &config).unwrap();
    assert_eq!(offline.len(), 1);
}

#[test]
fn odd_lists_are_refused() {
    let file = |path: &str, hash: &str| format!(r#"{{"path":"{path}","sha256":"{hash}"}}"#);
    let hash = "a".repeat(64);
    let list = |id: &str, folder: &str, files: &str| {
        format!(
            r#"{{"format":1,"mods":[{{"id":"{id}","kind":"extension","name":"N","author":"A",
            "version":"1.0.0","description":"D","path":"{folder}","files":[{files}]}}]}}"#
        )
    };
    let good = list("ok", "mods/ok", &file("extension.toml", &hash));
    assert!(Catalog::parse(good.as_bytes()).is_ok());

    let bad = [
        list("Bad Id", "mods/ok", &file("extension.toml", &hash)),
        list("ok", "../outside", &file("extension.toml", &hash)),
        list("ok", "/abs", &file("extension.toml", &hash)),
        list("ok", "mods/ok", &file("..\\\\x", &hash)),
        list("ok", "mods/ok", &file(".hidden", &hash)),
        list("ok", "mods/ok", &file("extension.toml", "123")),
        list("ok", "mods/ok", &file("main.js", &hash)),
        good.replace("\"format\":1", "\"format\":2"),
        good.replace(
            "\"files\"",
            r##""preview":{"frame":"red;x:y","background":"#000","highlight":"#000","text":"#000","accent":"#000"},"files""##,
        ),
        good.replace("\"author\":\"A\"", "\"author\":\"A\",\"extra\":1"),
        // A showcase may only point inside showcase/, at the right kind of file.
        good.replace("\"files\"", &format!(r#""showcase":{{"images":[{}]}},"files""#, file("main.js", &hash))),
        good.replace("\"files\"", &format!(r#""showcase":{{"images":[{}]}},"files""#, file("showcase/x.exe", &hash))),
        good.replace("\"files\"", &format!(r#""showcase":{{"about":{}}},"files""#, file("showcase/1.png", &hash))),
        good.replace("\"files\"", r#""showcase":{"tags":["Not A Tag"]},"files""#),
        good.replace("\"files\"", r#""updated":"yesterday","files""#),
    ];
    let with_showcase = good.replace(
        "\"files\"",
        &format!(
            r#""showcase":{{"images":[{}],"about":{},"tags":["dark"]}},"updated":"2026-11-26","files""#,
            file("showcase/1.png", &hash),
            file("showcase/about.md", &hash)
        ),
    );
    assert!(Catalog::parse(with_showcase.as_bytes()).is_ok());
    for text in bad {
        assert!(Catalog::parse(text.as_bytes()).is_err(), "{text}");
    }
}

#[test]
fn the_link_is_made_once_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let paths = crate::service::ServicePaths::new(&AppDirs::in_folder(dir.path()));
    assert_eq!(Link::load(&paths), None);
    let link = Link::load_or_create(&paths).unwrap();
    assert_eq!(link.token.len(), 32);
    assert_eq!(Link::load_or_create(&paths).unwrap(), link);
    assert_ne!(Link::create(&paths).unwrap().token, link.token);
}
