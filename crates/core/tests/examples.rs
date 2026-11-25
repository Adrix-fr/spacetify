//! The example theme and extension shipped in `examples/` must always be valid
//! and only rely on stable hooks.

use std::path::PathBuf;

use spacetify_core::doctor::{self, Status, UiIndex, Verdict};
use spacetify_core::mods::{self, Loaded};

fn example(path: &str) -> Loaded {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(path);
    mods::load_dir(&dir).unwrap_or_else(|issue| panic!("{path}: {issue}"))
}

/// A tiny stand-in for Spotify's interface, with the hooks the examples use.
fn spotify() -> UiIndex {
    let css = mods::theme::COLOR_TOKENS
        .iter()
        .map(|token| format!("--{token}:#000;"))
        .collect::<String>();
    UiIndex::from_parts(
        &format!(".encore-dark-theme{{{css}}}"),
        r#"(0,i.jsx)("footer",{"data-testid":"now-playing-bar"})"#,
        "",
    )
}

#[test]
fn midnight_theme_is_valid_and_stable() {
    let Loaded::Theme(theme) = example("themes/midnight") else {
        panic!("not a theme");
    };
    assert_eq!(theme.manifest.meta.id, "midnight");
    let report = doctor::theme_report(&spotify(), &theme);
    assert_eq!(report.verdict(), Verdict::Compatible, "{report:?}");
    assert!(report.checks.iter().all(|c| c.status == Status::Found));
}

#[test]
fn hello_badge_extension_is_valid_and_stable() {
    let Loaded::Extension(extension) = example("extensions/hello-badge") else {
        panic!("not an extension");
    };
    assert_eq!(extension.manifest.meta.id, "hello-badge");
    assert_eq!(extension.manifest.settings.len(), 2);
    let report = doctor::extension_report(&spotify(), &extension);
    assert_eq!(report.verdict(), Verdict::Compatible, "{report:?}");
    assert_eq!(report.checks.len(), 1);
}
