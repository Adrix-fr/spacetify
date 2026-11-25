//! Builds the installer, or the uninstaller.
//!
//! With `SPACETIFY_SETUP_PAYLOAD` set to a folder (the files to install, laid
//! out as in the install folder), that folder is zipped into the program,
//! which then installs it: `spacetify_<version>_x64-setup.exe`. Without it,
//! the program only knows how to uninstall: it is the `uninstall.exe` put in
//! the install folder. `apps/desktop/scripts/bundle.mjs` builds both.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const PAYLOAD_ENV: &str = "SPACETIFY_SETUP_PAYLOAD";

fn main() {
    println!("cargo::rustc-check-cfg=cfg(payload)");
    println!("cargo::rerun-if-env-changed={PAYLOAD_ENV}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if let Some(folder) = std::env::var_os(PAYLOAD_ENV).filter(|value| !value.is_empty()) {
        let folder = PathBuf::from(folder);
        let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
        let zip = out.join("payload.zip");
        zip_folder(&folder, &zip).unwrap_or_else(|err| {
            panic!("cannot pack {}: {err}", folder.display());
        });
        println!("cargo::rustc-cfg=payload");
        println!("cargo::rustc-env=SPACETIFY_PAYLOAD_ZIP={}", zip.display());
    }

    let manifest = include_str!("windows/app.manifest");
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri build step");
}

fn zip_folder(folder: &Path, zip: &Path) -> io::Result<()> {
    let mut files = Vec::new();
    collect(folder, folder, &mut files)?;
    if files.is_empty() {
        return Err(io::Error::other("the folder is empty"));
    }
    files.sort();

    let mut writer = zip::ZipWriter::new(File::create(zip)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9))
        .large_file(false);
    for (name, path) in files {
        println!("cargo::rerun-if-changed={}", path.display());
        writer.start_file(name, options).map_err(io::Error::other)?;
        writer.write_all(&fs::read(&path)?)?;
    }
    writer.finish().map_err(io::Error::other)?;
    Ok(())
}

/// Every file under `dir`, with its path inside `root` written with `/`.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    println!("cargo::rerun-if-changed={}", dir.display());
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let name = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((name, path));
        }
    }
    Ok(())
}
