use std::path::Path;

use super::*;
use crate::testing::{Fixture, write_spa};

const CONFIG: &str = "\
[AdditionalOptions]
extensions            = fullAppDisplay.js|adblock.js|trashbin.js
custom_apps           = marketplace|lyrics-plus

[Setting]
current_theme          = Sleek
color_scheme           = Coral ; the one shown

; DO NOT CHANGE!
[Backup]
version = 1.3.6.1921.g5d40d5480152
with    = 2.45.3
";

const SLEEK: &str = "\
; COLORS KEYS DESCRIPTION
[Nord]
text    = D8DEE9
main    = 2E3440
button  = #88c0d0

[Coral]
text               = fff
subtext            = F0F0F0
main               = 1B1B1B
card               = 2a2a2a
main-elevated      = 303030
sidebar            = 111111
button             = FF7F50
button-active      = ff9a76
misc               = ${xrdb:color1:#fff}
";

/// A fake Spicetify: its user folder (config, themes, backup) and its program folder.
struct Spice {
    _dir: tempfile::TempDir,
    users: Vec<PathBuf>,
    apps: Vec<PathBuf>,
}

impl Spice {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("Roaming/spicetify");
        let app = dir.path().join("Local/spicetify");
        fs::create_dir_all(user.join("Themes/Sleek")).unwrap();
        fs::create_dir_all(user.join("Themes/marketplace")).unwrap();
        fs::create_dir_all(app.join("Themes/SpicetifyDefault")).unwrap();
        fs::write(user.join(CONFIG_FILE), CONFIG).unwrap();
        fs::write(user.join("Themes/Sleek/color.ini"), SLEEK).unwrap();
        fs::write(user.join("Themes/Sleek/user.css"), ".main-view {}").unwrap();
        fs::write(user.join("Themes/marketplace/color.ini"), "[Marketplace]\n").unwrap();
        fs::write(
            app.join("Themes/SpicetifyDefault/color.ini"),
            "[green-dark]\nmain = 2E2837\n",
        )
        .unwrap();
        Self {
            users: vec![dir.path().join("nowhere"), user],
            apps: vec![app],
            _dir: dir,
        }
    }

    fn find(&self) -> Spicetify {
        find_in(&self.users, &self.apps).unwrap()
    }

    fn backup(&self) -> PathBuf {
        self.users[1].join("Backup")
    }
}

/// Makes the fixture's Spotify look like Spicetify left it: no xpui.spa,
/// an unpacked folder, and Spicetify's backup of the original.
fn modified_by_spicetify(fx: &Fixture, spice: &Spice) {
    fs::create_dir_all(spice.backup()).unwrap();
    fs::copy(fx.install.xpui_spa(), spice.backup().join("xpui.spa")).unwrap();
    fx.add_spicetify_folder();
    fs::remove_file(fx.install.xpui_spa()).unwrap();
}

fn repository() -> Source {
    Source::Folder(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// A copy of the gallery's repository (spacetify-mod/gallery), given by
/// `SPACETIFY_GALLERY_DIR`: its CI sets it. Without it, the test that
/// installs the gallery's equivalents is skipped.
fn gallery_repository() -> Option<Source> {
    let dir = std::path::PathBuf::from(std::env::var_os("SPACETIFY_GALLERY_DIR")?);
    dir.join(crate::gallery::INDEX_PATH)
        .is_file()
        .then_some(Source::Folder(dir))
}

#[test]
fn reads_spicetify_config() {
    let spice = Spice::new();
    let found = spice.find();
    assert_eq!(found.user_dir, spice.users[1]);
    assert_eq!(found.app_dir.as_deref(), Some(spice.apps[0].as_path()));
    assert_eq!(
        found.config,
        SpiceConfig {
            current_theme: "Sleek".into(),
            color_scheme: "Coral".into(),
            extensions: vec![
                "fullAppDisplay.js".into(),
                "adblock.js".into(),
                "trashbin.js".into()
            ],
            custom_apps: vec!["marketplace".into(), "lyrics-plus".into()],
            backup_version: Some("1.3.6.1921.g5d40d5480152".into()),
        }
    );
    assert!(find_in(&[spice.users[0].clone()], &spice.apps).is_none());
}

#[test]
fn turns_colour_schemes_into_spotify_colours() {
    let ini = parse_ini(SLEEK);
    let names: Vec<&str> = ini.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["", "Nord", "Coral"]);
    let sets = colour_sets(&ini[2].1).unwrap();
    assert_eq!(sets["base"]["text-base"], "#ffffff");
    assert_eq!(sets["base"]["background-base"], "#1b1b1b");
    // main-elevated wins over card.
    assert_eq!(sets["base"]["background-elevated-base"], "#303030");
    assert_eq!(sets["app-frame"]["background-base"], "#111111");
    assert_eq!(sets["bright-accent"]["background-base"], "#ff7f50");
    assert_eq!(sets["bright-accent"]["background-press"], "#ff9a76");
    assert!(sets.values().flatten().all(|(_, c)| c.len() == 7));
    assert_eq!(
        colour_sets(&BTreeMap::from([("misc".into(), "fff".into())])),
        None
    );
    assert_eq!(hex_color("${xrdb:color1}"), None);
    assert_eq!(hex_color("#ABC"), Some("#aabbcc".into()));
}

#[test]
fn makes_ids_that_spacetify_accepts() {
    assert_eq!(imported_id("Sleek", Some("Coral")), "spicetify-sleek-coral");
    assert_eq!(
        imported_id("Catppuccin  Mocha!", None),
        "spicetify-catppuccin-mocha"
    );
    assert_eq!(imported_id("★★", None), "spicetify-theme");
    let long = imported_id(&"x".repeat(80), Some("y"));
    assert!(long.len() <= MAX_ID_LEN && manifest::is_valid_id(&long));
}

#[test]
fn plans_every_scheme_and_the_equivalents() {
    let fx = Fixture::new();
    let spice = Spice::new();
    let found = spice.find();
    let plan = plan(&fx.install, Some(&found)).unwrap();
    assert_eq!(plan.spotify, SpotifyStep::Clean);
    let ids: Vec<(&str, bool)> = plan
        .themes
        .iter()
        .map(|t| (t.id.as_str(), t.current))
        .collect();
    // The bundled default theme is only taken when it is the current one.
    assert_eq!(
        ids,
        [
            ("spicetify-sleek-nord", false),
            ("spicetify-sleek-coral", true)
        ]
    );
    assert_eq!(plan.themes[1].name, "Sleek · Coral");
    assert!(!plan.marketplace_theme);
    let equivalents: Vec<Option<&str>> = plan
        .extensions
        .iter()
        .map(|e| e.equivalent.as_deref())
        .collect();
    assert_eq!(equivalents, [Some("fullscreen"), None, Some("trash-bin")]);
    assert_eq!(plan.apps[0].equivalent.as_deref(), Some("gallery"));
    assert_eq!(plan.apps[1].equivalent, None);
}

#[test]
fn spotify_is_only_restored_from_a_backup_of_the_same_version() {
    let fx = Fixture::new();
    let spice = Spice::new();
    modified_by_spicetify(&fx, &spice);
    let found = spice.find();
    assert!(matches!(
        plan(&fx.install, Some(&found)).unwrap().spotify,
        SpotifyStep::Restore { .. }
    ));
    let mut newer = fx.install.clone();
    newer.version = Some("1.3.10.2586".into());
    assert!(matches!(
        plan(&newer, Some(&found)).unwrap().spotify,
        SpotifyStep::Reinstall { .. }
    ));
    assert!(matches!(
        plan(&fx.install, None).unwrap().spotify,
        SpotifyStep::Reinstall { .. }
    ));
    write_spa(
        &spice.backup().join("xpui.spa"),
        "<html>spicetifyWrapper</html>",
    );
    assert!(matches!(
        plan(&fx.install, Some(&found)).unwrap().spotify,
        SpotifyStep::Reinstall { .. }
    ));
    assert!(same_version("1.3.6.1921.g5d40d5480152", "1.3.6.1921"));
    assert!(!same_version("1.3.6.1921", "1.3.10.2586"));
    assert!(!same_version("", ""));
}

#[test]
fn leaving_restores_spotify_imports_themes_and_installs_equivalents() {
    let Some(gallery) = gallery_repository() else {
        eprintln!("SPACETIFY_GALLERY_DIR is not set: this test runs in the gallery's CI");
        return;
    };
    let fx = Fixture::new();
    let spice = Spice::new();
    modified_by_spicetify(&fx, &spice);
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::in_folder(home.path());
    let found = spice.find();
    let plan = plan(&fx.install, Some(&found)).unwrap();
    assert!(!already_left(&dirs));

    let outcome = leave_with(&fx.install, &plan, &dirs, &gallery).unwrap();
    assert!(outcome.restored);
    assert_eq!(outcome.removed, [fx.install.xpui_dir()]);
    assert_eq!(
        detect::inspect(&fx.install).unwrap().state,
        UiState::Original
    );
    assert_eq!(
        outcome.themes,
        ["spicetify-sleek-nord", "spicetify-sleek-coral"]
    );
    assert_eq!(outcome.installed, ["fullscreen", "trash-bin"]);
    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);

    let config = Config::load(&dirs.config_file()).unwrap();
    assert_eq!(config.theme.as_deref(), Some("spicetify-sleek-coral"));
    assert_eq!(config.extensions, ["fullscreen", "trash-bin"]);
    let active = dirs.mods().active(&config).unwrap();
    assert!(active.theme.unwrap().css.contains("#ff7f50"));
    assert!(already_left(&dirs));
    assert!(!dirs.data_dir.join("spicetify-import").exists());
    // Spicetify's own files are left alone.
    assert!(spice.backup().join("xpui.spa").is_file());
}

#[test]
fn leftovers_are_removed_and_an_unusable_backup_stops_everything() {
    let fx = Fixture::new();
    fx.add_spicetify_folder();
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::in_folder(home.path());
    let plan = plan(&fx.install, None).unwrap();
    assert_eq!(
        plan.spotify,
        SpotifyStep::Leftovers {
            folders: vec![fx.install.xpui_dir()]
        }
    );
    let outcome = leave_with(&fx.install, &plan, &dirs, &repository()).unwrap();
    assert!(!fx.install.xpui_dir().exists());
    assert!(!outcome.restored);

    let fx = Fixture::new();
    fx.add_spicetify_folder();
    fs::remove_file(fx.install.xpui_spa()).unwrap();
    let plan = super::plan(&fx.install, None).unwrap();
    assert!(matches!(
        leave_with(&fx.install, &plan, &dirs, &repository()),
        Err(Error::Spicetify(_))
    ));
    assert!(fx.install.xpui_dir().is_dir());
}

#[test]
fn the_marketplace_theme_is_imported_once_when_asked() {
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::in_folder(home.path());
    let colors = BTreeMap::from([
        ("main".to_owned(), "101010".to_owned()),
        ("button".to_owned(), "1db954".to_owned()),
    ]);
    // Not asked: nothing happens.
    assert_eq!(
        marketplace_theme(&dirs, "Comfy", Some("Mono"), &colors).unwrap(),
        None
    );
    Marker {
        marketplace_theme_pending: true,
    }
    .save(&dirs)
    .unwrap();
    let id = marketplace_theme(&dirs, "Comfy", Some("Mono"), &colors).unwrap();
    assert_eq!(id.as_deref(), Some("spicetify-comfy-mono"));
    let config = Config::load(&dirs.config_file()).unwrap();
    assert_eq!(config.theme.as_deref(), Some("spicetify-comfy-mono"));
    assert_eq!(
        marketplace_theme(&dirs, "Comfy", Some("Mono"), &colors).unwrap(),
        None
    );
}
