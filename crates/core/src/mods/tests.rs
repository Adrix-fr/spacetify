use std::fs;
use std::path::Path;

use super::*;

fn write_theme(dir: &Path, id: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(theme::MANIFEST),
        format!(
            "id = \"{id}\"\nname = \"Theme {id}\"\nversion = \"1.0.0\"\nauthor = \"Me\"\nspacetify = \">=0.2.0\"\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join(theme::COLORS_FILE),
        "[base]\nessential-bright-accent = \"#8b5cf6\"\n",
    )
    .unwrap();
}

fn write_extension(dir: &Path, id: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(extension::MANIFEST),
        format!(
            "id = \"{id}\"\nname = \"Ext {id}\"\nversion = \"0.1.0\"\nauthor = \"Me\"\nspacetify = \">=0.2.0\"\n"
        ),
    )
    .unwrap();
    fs::write(dir.join(extension::SCRIPT), "spacetify.log.info('hi');").unwrap();
}

struct Setup {
    _dir: tempfile::TempDir,
    sources: std::path::PathBuf,
    store: ModStore,
    config: Config,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let sources = dir.path().join("sources");
    write_theme(&sources.join("midnight"), "midnight");
    write_theme(&sources.join("dawn"), "dawn");
    write_extension(&sources.join("hello"), "hello");
    Setup {
        store: ModStore::new(dir.path().join("config")),
        sources,
        config: Config::default(),
        _dir: dir,
    }
}

#[test]
fn installs_lists_and_activates() {
    let mut s = setup();
    let installed = s.store.install(&s.sources.join("midnight")).unwrap();
    assert_eq!(installed.kind, ModKind::Theme);
    assert!(!installed.replaced);
    s.store.install(&s.sources.join("hello")).unwrap();

    assert_eq!(
        s.store.enable(&mut s.config, "midnight").unwrap(),
        (ModKind::Theme, true)
    );
    assert_eq!(
        s.store.enable(&mut s.config, "hello").unwrap(),
        (ModKind::Extension, true)
    );
    assert_eq!(
        s.store.enable(&mut s.config, "hello").unwrap(),
        (ModKind::Extension, false)
    );
    assert_eq!(s.config.theme.as_deref(), Some("midnight"));
    assert_eq!(s.config.extensions, ["hello"]);

    let listed = s.store.list(&s.config).unwrap();
    let summary: Vec<_> = listed
        .iter()
        .map(|l| (l.kind, l.id.as_str(), l.active, l.meta.is_ok()))
        .collect();
    assert_eq!(
        summary,
        [
            (ModKind::Theme, "midnight", true, true),
            (ModKind::Extension, "hello", true, true)
        ]
    );

    let active = s.store.active(&s.config).unwrap();
    assert_eq!(active.theme.unwrap().manifest.meta.id, "midnight");
    assert_eq!(active.extensions.len(), 1);
}

#[test]
fn reinstalling_replaces_the_old_copy() {
    let s = setup();
    let src = s.sources.join("hello");
    s.store.install(&src).unwrap();
    fs::write(src.join(extension::SCRIPT), "spacetify.log.info('v2');").unwrap();
    assert!(s.store.install(&src).unwrap().replaced);
    let code = s.store.load_extension("hello").unwrap().code;
    assert!(code.contains("v2"));
    let leftovers: Vec<_> = fs::read_dir(s.store.dir_of(ModKind::Extension))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["hello"]);
}

#[test]
fn refuses_invalid_mods_and_keeps_the_previous_copy() {
    let s = setup();
    let src = s.sources.join("midnight");
    s.store.install(&src).unwrap();
    fs::write(
        src.join(theme::COLORS_FILE),
        "[base]\ntext-base = \"white\"\n",
    )
    .unwrap();

    let err = s.store.install(&src).unwrap_err();
    assert!(matches!(
        err,
        Error::InvalidMod {
            issue: ModIssue::BadColor { .. },
            ..
        }
    ));
    assert!(s.store.load_theme("midnight").is_ok());

    let empty = s.sources.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        s.store.install(&empty).unwrap_err(),
        Error::InvalidMod {
            issue: ModIssue::NoManifest,
            ..
        }
    ));
}

#[test]
fn ids_are_unique_across_kinds() {
    let s = setup();
    s.store.install(&s.sources.join("hello")).unwrap();
    let clash = s.sources.join("clash");
    write_theme(&clash, "hello");
    assert!(matches!(
        s.store.install(&clash).unwrap_err(),
        Error::IdTaken {
            kind: ModKind::Extension,
            ..
        }
    ));
}

#[test]
fn refuses_to_install_from_the_store_itself() {
    let s = setup();
    s.store.install(&s.sources.join("hello")).unwrap();
    let installed = s.store.path_of(ModKind::Extension, "hello");
    assert!(matches!(
        s.store.install(&installed).unwrap_err(),
        Error::InstallFromStore(_)
    ));
    assert!(s.store.load_extension("hello").is_ok());
}

#[test]
fn only_one_theme_is_active() {
    let mut s = setup();
    s.store.install(&s.sources.join("midnight")).unwrap();
    s.store.install(&s.sources.join("dawn")).unwrap();
    s.store.install(&s.sources.join("hello")).unwrap();

    assert!(s.store.set_theme(&mut s.config, Some("midnight")).unwrap());
    assert!(s.store.set_theme(&mut s.config, Some("dawn")).unwrap());
    assert!(!s.store.set_theme(&mut s.config, Some("dawn")).unwrap());
    assert_eq!(s.config.theme.as_deref(), Some("dawn"));
    assert!(matches!(
        s.store.set_theme(&mut s.config, Some("hello")).unwrap_err(),
        Error::NotATheme(_)
    ));
    assert!(matches!(
        s.store.set_theme(&mut s.config, Some("nope")).unwrap_err(),
        Error::ModNotFound(_)
    ));
    assert!(s.store.set_theme(&mut s.config, None).unwrap());
    assert_eq!(s.config.theme, None);
}

#[test]
fn disable_and_uninstall_clean_the_config() {
    let mut s = setup();
    s.store.install(&s.sources.join("midnight")).unwrap();
    s.store.install(&s.sources.join("hello")).unwrap();
    s.store.enable(&mut s.config, "midnight").unwrap();
    s.store.enable(&mut s.config, "hello").unwrap();

    assert!(s.store.disable(&mut s.config, "hello").unwrap());
    assert!(!s.store.disable(&mut s.config, "hello").unwrap());
    assert!(s.config.extensions.is_empty());

    assert_eq!(
        s.store.uninstall(&mut s.config, "midnight").unwrap(),
        ModKind::Theme
    );
    assert_eq!(s.config.theme, None);
    assert!(!s.store.path_of(ModKind::Theme, "midnight").exists());
    assert!(matches!(
        s.store.uninstall(&mut s.config, "midnight").unwrap_err(),
        Error::ModNotFound(_)
    ));

    // A mod deleted by hand can still be turned off.
    s.config.extensions.push("gone".into());
    assert!(s.store.disable(&mut s.config, "gone").unwrap());
    assert!(matches!(
        s.store.disable(&mut s.config, "never").unwrap_err(),
        Error::ModNotFound(_)
    ));
}

#[test]
fn active_mods_are_validated_again() {
    let mut s = setup();
    s.store.install(&s.sources.join("midnight")).unwrap();
    s.store.enable(&mut s.config, "midnight").unwrap();

    // Edited in place after installing.
    let installed = s.store.path_of(ModKind::Theme, "midnight");
    fs::write(installed.join(theme::CSS_FILE), "a { color: red").unwrap();
    assert!(matches!(
        s.store.active(&s.config).unwrap_err(),
        Error::InvalidMod {
            issue: ModIssue::CssUnbalanced { .. },
            ..
        }
    ));
    let listed = s.store.list(&s.config).unwrap();
    assert!(listed[0].meta.is_err());

    s.config.extensions.push("missing".into());
    fs::remove_file(installed.join(theme::CSS_FILE)).unwrap();
    assert!(matches!(
        s.store.active(&s.config).unwrap_err(),
        Error::ModNotFound(_)
    ));
}

#[test]
fn refuses_a_folder_whose_name_does_not_match_the_id() {
    let mut s = setup();
    write_theme(&s.store.path_of(ModKind::Theme, "copied"), "original");
    assert!(matches!(
        s.store.enable(&mut s.config, "copied").unwrap_err(),
        Error::InvalidMod {
            issue: ModIssue::FolderMismatch { .. },
            ..
        }
    ));
}

#[test]
fn settings_chosen_in_spacetify_are_checked_and_injected() {
    let mut s = setup();
    let dir = s.sources.join("badge");
    write_extension(&dir, "badge");
    let mut manifest = fs::read_to_string(dir.join(extension::MANIFEST)).unwrap();
    manifest.push_str(
        "[settings.text]\ntype = \"string\"\ndefault = \"hi\"\n\
         [settings.side]\ntype = \"choice\"\nchoices = [\"left\", \"right\"]\ndefault = \"right\"\n\
         [settings.size]\ntype = \"number\"\ndefault = 12\n",
    );
    fs::write(dir.join(extension::MANIFEST), manifest).unwrap();
    s.store.install(&dir).unwrap();
    s.store.enable(&mut s.config, "badge").unwrap();

    let defaults = s.store.settings(&s.config, "badge").unwrap();
    assert_eq!(defaults.len(), 3);
    assert!(defaults.iter().all(|v| !v.custom));

    use serde_json::json;
    s.store
        .set_setting(&mut s.config, "badge", "side", Some(&json!("left")))
        .unwrap();
    s.store
        .set_setting(&mut s.config, "badge", "size", Some(&json!(14.5)))
        .unwrap();
    for (name, value) in [
        ("side", json!("top")),
        ("text", json!(3)),
        ("missing", json!("x")),
    ] {
        assert!(matches!(
            s.store
                .set_setting(&mut s.config, "badge", name, Some(&value)),
            Err(Error::InvalidSetting { .. })
        ));
    }

    let side = s
        .store
        .settings(&s.config, "badge")
        .unwrap()
        .into_iter()
        .find(|v| v.name == "side")
        .unwrap();
    assert_eq!((side.value, side.custom), (json!("left"), true));

    // A value that no longer fits (edited by hand, or the mod changed) is ignored.
    s.config
        .settings
        .get_mut("badge")
        .unwrap()
        .insert("text".into(), toml::Value::Integer(1));
    let active = s.store.active(&s.config).unwrap();
    let configured = &active.extensions[0].configured;
    assert_eq!(configured.len(), 2);
    assert_eq!(configured["side"], json!("left"));
    assert_eq!(configured["size"], json!(14.5));

    s.store
        .set_setting(&mut s.config, "badge", "side", None)
        .unwrap();
    s.store
        .set_setting(&mut s.config, "badge", "size", None)
        .unwrap();
    s.store
        .set_setting(&mut s.config, "badge", "text", None)
        .unwrap();
    assert!(s.config.settings.is_empty());

    s.store
        .set_setting(&mut s.config, "badge", "text", Some(&json!("yo")))
        .unwrap();
    s.store.uninstall(&mut s.config, "badge").unwrap();
    assert!(s.config.settings.is_empty());
}
