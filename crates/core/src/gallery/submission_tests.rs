use std::fs;
use std::path::Path;

use zip::write::SimpleFileOptions;

use super::*;

const THEME_TOML: &str = "id = \"aurora\"\nname = \"Aurora\"\nversion = \"1.0.0\"\n\
                          author = \"Lina\"\ndescription = \"Night green.\"\nspacetify = \">=0.4.0\"\n";

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn aurora(version: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            ("theme.toml", &THEME_TOML.replace("1.0.0", version)),
            ("colors.toml", "[base]\nbackground-base = \"#0b1a1f\"\n"),
            ("theme.css", "body { letter-spacing: 0; }\n"),
            ("README.md", "# Aurora\n"),
        ],
    );
    dir
}

fn zip_of(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        out.start_file(*name, SimpleFileOptions::default()).unwrap();
        out.write_all(bytes).unwrap();
    }
    let bytes = out.finish().unwrap().into_inner();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    file
}

/// A repository with the gallery's list: a copy of the real one when
/// `SPACETIFY_GALLERY_DIR` gives it, otherwise a list with the example
/// theme only, owned by the team.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("gallery")).unwrap();
    if let Some(root) = super::super::tests::gallery_root() {
        fs::copy(root.join(INDEX_PATH), dir.path().join(INDEX_PATH)).unwrap();
        fs::copy(root.join(OWNERS_PATH), dir.path().join(OWNERS_PATH)).unwrap();
        return dir;
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/themes/midnight");
    let dst = dir.path().join("gallery/mods/midnight");
    fs::create_dir_all(&dst).unwrap();
    let mut files = Vec::new();
    for entry in fs::read_dir(&src).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let bytes = fs::read(entry.path()).unwrap();
        fs::write(dst.join(&name), &bytes).unwrap();
        files.push(FileEntry {
            path: name,
            sha256: sha256_hex(&bytes),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let loaded = mods::load_dir(&dst).unwrap();
    let Loaded::Theme(theme) = &loaded else {
        panic!("the example is a theme");
    };
    let meta = loaded.meta();
    let catalog = Catalog {
        format: 1,
        mods: vec![Entry {
            id: meta.id.clone(),
            kind: loaded.kind(),
            name: meta.name.clone(),
            author: meta.author.clone(),
            version: meta.version.to_string(),
            description: meta.description.clone().unwrap_or_default(),
            description_fr: None,
            path: "gallery/mods/midnight".into(),
            files,
            preview: Some(Preview::of_colors(&theme.colors)),
            showcase: None,
            updated: None,
        }],
    };
    fs::write(
        dir.path().join(INDEX_PATH),
        serde_json::to_vec_pretty(&catalog).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.path().join(OWNERS_PATH),
        format!(
            "{{\"midnight\": \"{TEAM_OWNER}\"}}
"
        ),
    )
    .unwrap();
    dir
}

const BODY: &str = "### Mod name / Nom du mod\n\nAurora\n\n\
### Kind / Type\n\nTheme\n\n\
### Version\n\n1.0.0\n\n\
### Request / Demande\n\nNew mod\n\n\
### Description (English) / Description (anglais)\n\nNight green\nbackgrounds.\n\n\
### Description (French) / Description (français)\n\n_No response_\n\n\
### The mod (.zip) / Le mod (.zip)\n\n[aurora-1.0.0.zip](https://github.com/user-attachments/files/123456/aurora-1.0.0.zip)\n\n\
### Or a GitHub repository / Ou un dépôt GitHub\n\n_No response_\n\n\
### Tag or commit / Tag ou commit\n\n_No response_\n\n\
### Folder in the repository / Dossier dans le dépôt\n\n_No response_\n\n\
### Rights / Droits\n\n- [X] This mod is mine, or I may share it.\n";

#[test]
fn reads_a_request() {
    let form = Form::parse(BODY).unwrap();
    assert_eq!(form.name, "Aurora");
    assert_eq!(form.kind, ModKind::Theme);
    assert_eq!(form.version, "1.0.0");
    assert!(!form.update);
    assert_eq!(
        form.description.as_deref(),
        Some("Night green backgrounds.")
    );
    assert_eq!(form.description_fr, None);
    assert_eq!(form.folder, None);
    assert_eq!(
        form.origin,
        Origin::Attachment {
            url: "https://github.com/user-attachments/files/123456/aurora-1.0.0.zip".into()
        }
    );
}

#[test]
fn reads_a_request_from_a_repository() {
    let body = BODY
        .replace(
            "[aurora-1.0.0.zip](https://github.com/user-attachments/files/123456/aurora-1.0.0.zip)",
            "_No response_",
        )
        .replace(
            "### Or a GitHub repository / Ou un dépôt GitHub\n\n_No response_",
            "### Or a GitHub repository / Ou un dépôt GitHub\n\nhttps://github.com/lina-dev/themes.git",
        )
        .replace(
            "### Tag or commit / Tag ou commit\n\n_No response_",
            "### Tag or commit / Tag ou commit\n\nv1.0.0",
        )
        .replace(
            "### Folder in the repository / Dossier dans le dépôt\n\n_No response_",
            "### Folder in the repository / Dossier dans le dépôt\n\n/aurora/",
        );
    let form = Form::parse(&body).unwrap();
    assert_eq!(form.folder.as_deref(), Some("aurora"));
    assert_eq!(
        form.origin,
        Origin::Repository {
            owner: "lina-dev".into(),
            repo: "themes".into(),
            reference: "v1.0.0".into(),
            url: "https://codeload.github.com/lina-dev/themes/zip/v1.0.0".into(),
        }
    );
}

#[test]
fn refuses_requests_that_are_incomplete_or_point_elsewhere() {
    let without = |from: &str, to: &str| Form::parse(&BODY.replace(from, to)).unwrap_err();
    for (from, to) in [
        ("- [X]", "- [ ]"),
        ("\nTheme\n", "\nSomething\n"),
        (
            "https://github.com/user-attachments/files/123456/aurora-1.0.0.zip",
            "https://evil.example/files/123456/aurora-1.0.0.zip",
        ),
        (
            "https://github.com/user-attachments/files/123456/aurora-1.0.0.zip",
            "https://github.com/user-attachments/files/123456/aurora.exe",
        ),
        (
            "### Folder in the repository / Dossier dans le dépôt\n\n_No response_",
            "### Folder in the repository / Dossier dans le dépôt\n\n../secrets",
        ),
    ] {
        assert!(matches!(without(from, to), Error::Submission(_)), "{to}");
    }
    assert!(repository_of("https://github.com/a/b").is_some());
    assert!(repository_of("https://gitlab.com/a/b").is_none());
    assert!(repository_of("https://github.com/a/b/../c").is_none());
    assert!(is_reference("v1.0.0") && is_reference("0a1b2c3"));
    assert!(!is_reference("../main") && !is_reference("-x") && !is_reference("a b"));
}

#[test]
fn packs_only_the_files_of_the_mod() {
    let dir = aurora("1.0.0");
    write(dir.path(), &[(".git/HEAD", "x")]);
    let out = tempfile::tempdir().unwrap();
    let zip = pack(dir.path(), out.path()).unwrap();
    assert_eq!(zip.file_name().unwrap(), "aurora-1.0.0.zip");
    let archive = zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
    let mut names: Vec<String> = archive
        .file_names()
        .map(|name| name.unwrap().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["colors.toml", "theme.css", "theme.toml"]);

    let unpacked = tempfile::tempdir().unwrap();
    let into = unpacked.path().join("mod");
    unpack(&zip, &into, None).unwrap();
    assert!(mods::load_dir(&into).is_ok());
}

#[test]
fn the_form_link_is_filled_in() {
    let dir = aurora("1.0.0");
    let loaded = mods::load_dir(dir.path()).unwrap();
    let url = form_url(&loaded, false);
    assert!(url.starts_with(
        "https://github.com/spacetify-mod/gallery/issues/new?template=submit-mod.yml&"
    ));
    assert!(url.contains("&name=Aurora&kind=Theme&version=1.0.0&request=New%20mod"));
    assert!(url.contains("title=%5BMod%5D%20Aurora%201.0.0"));
}

#[test]
fn unpacks_a_folder_inside_the_archive() {
    let zip = zip_of(&[
        ("themes-1.0.0/README.md", b"x"),
        ("themes-1.0.0/aurora/theme.toml", THEME_TOML.as_bytes()),
        ("themes-1.0.0/aurora/theme.css", b"body{}"),
        ("themes-1.0.0/other/theme.toml", b"x"),
        ("__MACOSX/themes-1.0.0/._theme.toml", b"junk"),
    ]);
    let out = tempfile::tempdir().unwrap();
    let dir = unpack(zip.path(), &out.path().join("m"), Some("aurora")).unwrap();
    assert!(dir.join("theme.toml").is_file());
    assert!(!dir.join("other").exists());
    assert!(!out.path().join("__MACOSX").exists());

    let missing = unpack(zip.path(), &out.path().join("n"), Some("nothing"));
    assert!(missing.is_err());
}

#[test]
fn refuses_archives_that_leave_their_folder() {
    for name in ["../evil.js", "/etc/passwd", "a/../../b", "C:/x.js"] {
        let zip = zip_of(&[("theme.toml", b"x"), (name, b"x")]);
        let out = tempfile::tempdir().unwrap();
        assert!(
            unpack(zip.path(), &out.path().join("m"), None).is_err(),
            "{name}"
        );
        assert!(!out.path().join("evil.js").exists());
    }
    let big = vec![b'a'; 3 * 1024 * 1024];
    let zip = zip_of(&[("theme.css", &big)]);
    let out = tempfile::tempdir().unwrap();
    assert!(unpack(zip.path(), &out.path().join("m"), None).is_err());
}

#[test]
fn adds_a_new_mod_then_its_update() {
    let repo = repo();
    let form = Form::parse(BODY).unwrap();
    let added = add(aurora("1.0.0").path(), repo.path(), "lina-dev", &form).unwrap();
    assert_eq!(added.id, "aurora");
    assert_eq!(added.previous, None);
    assert_eq!(added.path, "gallery/mods/aurora");
    assert!(repo.path().join("gallery/mods/aurora/theme.css").is_file());
    assert!(!repo.path().join("gallery/mods/aurora/README.md").exists());

    let catalog = read_catalog(repo.path()).unwrap();
    let entry = catalog.get("aurora").unwrap();
    assert_eq!(entry.description, "Night green backgrounds.");
    assert_eq!(entry.files.len(), 3);
    assert!(entry.preview.is_some());
    for file in &entry.files {
        let bytes = fs::read(repo.path().join(&entry.path).join(&file.path)).unwrap();
        assert_eq!(sha256_hex(&bytes), file.sha256);
    }
    assert_eq!(read_owners(repo.path()).unwrap()["aurora"], "lina-dev");
    assert_eq!(
        accepted_folder(repo.path(), "aurora").unwrap(),
        Some(repo.path().join("gallery/mods/aurora"))
    );

    // The same version again, or someone else, is refused.
    assert!(add(aurora("1.0.0").path(), repo.path(), "lina-dev", &form).is_err());
    let mut update = form.clone();
    update.version = "1.1.0".into();
    assert!(add(aurora("1.1.0").path(), repo.path(), "someone", &update).is_err());
    let added = add(aurora("1.1.0").path(), repo.path(), "Lina-Dev", &update).unwrap();
    assert_eq!(added.previous.as_deref(), Some("1.0.0"));
    let catalog = read_catalog(repo.path()).unwrap();
    assert_eq!(catalog.mods.iter().filter(|e| e.id == "aurora").count(), 1);
    assert_eq!(catalog.get("aurora").unwrap().version, "1.1.0");
}

#[test]
fn the_showcase_is_listed_apart_and_the_day_is_written() {
    let repo = repo();
    let form = Form::parse(BODY).unwrap();
    let mod_dir = aurora("1.0.0");
    let png: &[u8] = b"\x89PNG\r\n\x1a\nthe picture";
    let folder = mod_dir.path().join("showcase");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("1-home.png"), png).unwrap();
    fs::write(folder.join("about.md"), "# Aurora").unwrap();
    fs::write(folder.join("showcase.toml"), "tags = [\"dark\", \"green\"]").unwrap();
    add(mod_dir.path(), repo.path(), "lina-dev", &form).unwrap();

    let catalog = read_catalog(repo.path()).unwrap();
    let entry = catalog.get("aurora").unwrap();
    assert_eq!(entry.files.len(), 3, "the showcase is not part of the mod");
    let showcase = entry.showcase.as_ref().unwrap();
    assert_eq!(showcase.images[0].path, "showcase/1-home.png");
    assert_eq!(showcase.images[0].sha256, sha256_hex(png));
    assert_eq!(showcase.about.as_ref().unwrap().path, "showcase/about.md");
    assert_eq!(showcase.tags, ["dark", "green"]);
    assert!(
        repo.path()
            .join("gallery/mods/aurora/showcase/1-home.png")
            .is_file()
    );
    assert!(super::super::is_date(entry.updated.as_deref().unwrap()));

    // A bad showcase refuses the whole mod, before anything is copied.
    let bad = aurora("1.1.0");
    fs::create_dir_all(bad.path().join("showcase")).unwrap();
    fs::write(bad.path().join("showcase/1.png"), b"not a picture").unwrap();
    let mut update = form;
    update.version = "1.1.0".into();
    assert!(add(bad.path(), repo.path(), "lina-dev", &update).is_err());
    assert_eq!(
        read_catalog(repo.path())
            .unwrap()
            .get("aurora")
            .unwrap()
            .version,
        "1.0.0"
    );
}

#[test]
fn the_form_must_match_the_files() {
    let repo = repo();
    let mut form = Form::parse(BODY).unwrap();
    form.version = "2.0.0".into();
    assert!(add(aurora("1.0.0").path(), repo.path(), "lina-dev", &form).is_err());
    let mut form = Form::parse(BODY).unwrap();
    form.kind = ModKind::Extension;
    assert!(add(aurora("1.0.0").path(), repo.path(), "lina-dev", &form).is_err());
}

#[test]
fn the_ids_of_the_examples_belong_to_the_owner() {
    let repo = repo();
    let owners = read_owners(repo.path()).unwrap();
    let catalog = read_catalog(repo.path()).unwrap();
    for entry in &catalog.mods {
        assert!(owners.contains_key(&entry.id), "{} has no owner", entry.id);
    }
}

#[test]
fn nobody_can_send_an_update_of_the_team_s_mods() {
    let repo = repo();
    let owners = read_owners(repo.path()).unwrap();
    assert_eq!(owners["midnight"], TEAM_OWNER);
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            (
                "theme.toml",
                "id = \"midnight\"\nname = \"Midnight\"\nversion = \"9.0.0\"\n\
                 author = \"Someone\"\nspacetify = \">=0.4.0\"\n",
            ),
            ("theme.css", "body {}\n"),
        ],
    );
    let mut form = Form::parse(BODY).unwrap();
    form.version = "9.0.0".into();
    for login in ["someone", "spacetify", "the-spacetify-team"] {
        assert!(
            add(dir.path(), repo.path(), login, &form).is_err(),
            "{login}"
        );
    }
}
