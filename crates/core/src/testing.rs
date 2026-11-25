//! Fake Spotify installs for tests. The real Spotify is never touched.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::backup::{BackupStore, sha256_file};
use crate::spotify::{InstallKind, SpotifyInstall};

pub const ORIGINAL_INDEX: &str = "<!doctype html><html><head><title>Spotify</title>\
    <link href=\"/xpui-snapshot.css\" rel=\"stylesheet\"></head><body><div id=\"main\"></div>\
    <script defer=\"defer\" src=\"/xpui-snapshot.js\"></script></body></html>";

/// Writes a small `xpui.spa` with a mix of compressed and stored entries.
pub fn write_spa(path: &Path, index_html: &str) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let entries: [(&str, &[u8], SimpleFileOptions); 4] = [
        (
            "xpui-snapshot.js",
            b"console.log('spotify');".as_slice(),
            deflated,
        ),
        ("index.html", index_html.as_bytes(), deflated),
        (
            "xpui-snapshot.css",
            b".encore-dark-theme{--text-bright-accent:#1ed760}",
            deflated,
        ),
        (
            "images/logo.png",
            &[0x89, b'P', b'N', b'G', 0, 1, 2, 3],
            stored,
        ),
    ];
    for (name, content, options) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(content).unwrap();
    }
    zip.finish().unwrap();
}

/// Writes an extension that relies on `selectors`.
pub fn write_extension(dir: &Path, id: &str, selectors: &[&str]) {
    fs::create_dir_all(dir).unwrap();
    let selectors: Vec<String> = selectors.iter().map(|s| format!("'{s}'")).collect();
    fs::write(
        dir.join("extension.toml"),
        format!(
            "id = \"{id}\"
name = \"Ext {id}\"
version = \"0.1.0\"
author = \"Me\"
             spacetify = \">=0.2.0\"
selectors = [{}]
",
            selectors.join(", ")
        ),
    )
    .unwrap();
    fs::write(dir.join("main.js"), "spacetify.log.info('hi');").unwrap();
}

pub struct Fixture {
    _dir: TempDir,
    pub install: SpotifyInstall,
    pub store: BackupStore,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Spotify");
        let apps_dir = root.join("Apps");
        fs::create_dir_all(&apps_dir).unwrap();
        write_spa(&apps_dir.join("xpui.spa"), ORIGINAL_INDEX);

        Self {
            install: SpotifyInstall {
                executable: root.join("Spotify.exe"),
                kind: InstallKind::Standard,
                version: Some("1.3.6.1921".into()),
                apps_dir,
                root,
            },
            store: BackupStore::new(dir.path().join("backups")),
            _dir: dir,
        }
    }

    pub fn spa_sha(&self) -> String {
        sha256_file(&self.install.xpui_spa()).unwrap()
    }

    /// Mimics the folder Spicetify unpacks next to `xpui.spa`.
    pub fn add_spicetify_folder(&self) {
        let dir = self.install.xpui_dir();
        fs::create_dir_all(dir.join("helper")).unwrap();
        fs::write(
            dir.join("helper/spicetifyWrapper.js"),
            "window.Spicetify = {};",
        )
        .unwrap();
        let html = ORIGINAL_INDEX.replace(
            "</body>",
            "<script src='helper/spicetifyWrapper.js'></script></body>",
        );
        fs::write(dir.join("index.html"), html).unwrap();
    }
}
