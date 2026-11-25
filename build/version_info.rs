// Shared by the build scripts of the `spacetify` and `spacetify-service`
// programs (`include!`d, not a module). Adds Windows version information to
// the .exe: product name, description, publisher and version, as shown in
// the file's Properties and in the Task Manager. Nothing is compressed or
// packed. On other systems it does nothing.

fn version_info(file_name: &str, description: &str) {
    println!("cargo:rerun-if-changed=../../build/version_info.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
    let numbers: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    let comma = format!("{},{},{},0", numbers[0], numbers[1], numbers[2]);
    let rc = format!(
        r#"1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Adrix-fr (spacetify project)"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{file_name}"
      VALUE "LegalCopyright", "GPL-3.0-or-later"
      VALUE "OriginalFilename", "{file_name}.exe"
      VALUE "ProductName", "spacetify"
      VALUE "ProductVersion", "{version}"
      VALUE "Comments", "Open source: https://github.com/Adrix-fr/spacetify"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    let path = out.join("version.rc");
    std::fs::write(&path, rc).expect("cannot write version.rc");
    embed_resource::compile(&path, embed_resource::NONE)
        .manifest_optional()
        .expect("cannot compile the Windows version information");
}
