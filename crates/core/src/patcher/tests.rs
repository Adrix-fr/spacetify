use std::fs;
use std::io::Read;

use zip::ZipArchive;

use super::*;
use crate::spotify::InstallKind;
use crate::testing::{Fixture, ORIGINAL_INDEX, write_spa};

fn empty() -> Payload {
    Payload::new(&crate::mods::ActiveMods::default())
}

fn read_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            (entry.name().unwrap().into_owned(), content)
        })
        .collect()
}

#[test]
fn injects_once_before_body_end() {
    let html = inject_html(ORIGINAL_INDEX);
    assert_eq!(html.matches(BLOCK_START).count(), 1);
    assert!(html.ends_with(&format!("{BLOCK_END}</body></html>")));
    assert_eq!(inject_html(&html), html);
}

#[test]
fn appends_when_there_is_no_body_tag() {
    let html = inject_html("<div>no body</div>");
    assert!(html.starts_with("<div>no body</div>"));
    assert!(html.ends_with(BLOCK_END));
}

#[test]
fn patched_archive_keeps_every_original_entry() {
    let fx = Fixture::new();
    let spa = fx.install.xpui_spa();
    let original = read_entries(&fs::read(&spa).unwrap());
    let patched = read_entries(&build_patched(&spa, &Marker::new("abc"), &empty()).unwrap());

    for (name, content) in &original {
        let (_, new_content) = patched.iter().find(|(n, _)| n == name).unwrap();
        if name == "index.html" {
            assert!(String::from_utf8_lossy(new_content).contains(BLOCK_START));
        } else {
            assert_eq!(new_content, content, "{name} changed");
        }
    }
    let names: Vec<_> = patched.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&MARKER_PATH));
    assert!(names.contains(&"spacetify/inject.js"));
    assert!(names.contains(&"spacetify/inject.css"));
}

#[test]
fn patching_is_reproducible() {
    let fx = Fixture::new();
    let marker = Marker::new("abc");
    let first = build_patched(&fx.install.xpui_spa(), &marker, &empty()).unwrap();
    let second = build_patched(&fx.install.xpui_spa(), &marker, &empty()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn apply_backs_up_then_patches() {
    let fx = Fixture::new();
    let original_sha = fx.spa_sha();

    let outcome = apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    let ApplyOutcome::Applied { new_backup, sha256 } = outcome else {
        panic!("expected Applied, got {outcome:?}");
    };
    assert!(new_backup);
    assert_eq!(fx.spa_sha(), sha256);

    let state = detect::inspect(&fx.install).unwrap().state;
    assert!(matches!(state, UiState::Patched(ref m) if m.original_sha256 == original_sha));
    assert!(fx.store.find(&original_sha).unwrap().is_some());
    assert_eq!(
        fx.store.load_applied().unwrap().unwrap().patched_sha256,
        sha256
    );
}

#[test]
fn apply_is_idempotent() {
    let fx = Fixture::new();
    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    let patched_sha = fx.spa_sha();

    assert_eq!(
        apply_unchecked(&fx.install, &fx.store, &empty()).unwrap(),
        ApplyOutcome::AlreadyApplied
    );
    assert_eq!(fx.spa_sha(), patched_sha);
    assert_eq!(fx.store.list().unwrap().len(), 1);
}

#[test]
fn restore_gives_back_the_exact_original() {
    let fx = Fixture::new();
    let original_bytes = fs::read(fx.install.xpui_spa()).unwrap();
    let original_sha = fx.spa_sha();

    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    let outcome = restore_unchecked(&fx.install, &fx.store).unwrap();

    assert_eq!(
        outcome,
        RestoreOutcome::Restored {
            sha256: original_sha.clone()
        }
    );
    assert_eq!(fx.spa_sha(), original_sha);
    assert_eq!(fs::read(fx.install.xpui_spa()).unwrap(), original_bytes);
    assert!(fx.store.load_applied().unwrap().is_none());
    assert_eq!(
        restore_unchecked(&fx.install, &fx.store).unwrap(),
        RestoreOutcome::AlreadyOriginal
    );
}

#[test]
fn restore_never_downgrades_after_a_spotify_update() {
    let fx = Fixture::new();
    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();

    // Spotify updates and ships a new original interface.
    write_spa(
        &fx.install.xpui_spa(),
        &ORIGINAL_INDEX.replace("Spotify", "Spotify 2"),
    );
    let updated_sha = fx.spa_sha();

    assert_eq!(
        restore_unchecked(&fx.install, &fx.store).unwrap(),
        RestoreOutcome::AlreadyOriginal
    );
    assert_eq!(fx.spa_sha(), updated_sha);

    // Applying again backs up the new version and patches it.
    let outcome = apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    assert!(matches!(
        outcome,
        ApplyOutcome::Applied {
            new_backup: true,
            ..
        }
    ));
    assert!(fx.store.find(&updated_sha).unwrap().is_some());
}

#[test]
fn refuses_to_touch_spicetify() {
    let fx = Fixture::new();
    let html = ORIGINAL_INDEX.replace(
        "</body>",
        "<script src='helper/spicetifyWrapper.js'></script></body>",
    );
    write_spa(&fx.install.xpui_spa(), &html);
    let before = fx.spa_sha();

    assert!(matches!(
        apply_unchecked(&fx.install, &fx.store, &empty()),
        Err(Error::SpicetifyActive)
    ));
    assert!(matches!(
        backup(&fx.install, &fx.store),
        Err(Error::SpicetifyActive)
    ));
    assert_eq!(fx.spa_sha(), before);
    assert!(fx.store.list().unwrap().is_empty());
}

#[test]
fn works_next_to_spicetify_leftovers_without_touching_them() {
    let fx = Fixture::new();
    fx.add_spicetify_folder();
    let leftover = fx.install.xpui_dir().join("index.html");
    let leftover_before = fs::read(&leftover).unwrap();

    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    restore_unchecked(&fx.install, &fx.store).unwrap();
    assert_eq!(fs::read(&leftover).unwrap(), leftover_before);
}

#[test]
fn refuses_read_only_installs() {
    let mut fx = Fixture::new();
    fx.install.kind = InstallKind::MicrosoftStore;
    assert!(matches!(
        apply_unchecked(&fx.install, &fx.store, &empty()),
        Err(Error::ReadOnlyInstall { .. })
    ));
}

#[test]
fn restore_refuses_a_damaged_backup() {
    let fx = Fixture::new();
    let original_sha = fx.spa_sha();
    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    let patched_sha = fx.spa_sha();

    let record = fx.store.find(&original_sha).unwrap().unwrap();
    fs::write(fx.store.file_for(&record), b"damaged").unwrap();

    assert!(matches!(
        restore_unchecked(&fx.install, &fx.store),
        Err(Error::BackupCorrupted(_))
    ));
    assert_eq!(fx.spa_sha(), patched_sha);
}

#[test]
fn backup_command_does_not_modify_spotify() {
    let fx = Fixture::new();
    let before = fx.spa_sha();
    assert!(matches!(
        backup(&fx.install, &fx.store).unwrap(),
        BackupOutcome::Created(_)
    ));
    assert!(matches!(
        backup(&fx.install, &fx.store).unwrap(),
        BackupOutcome::AlreadyBackedUp(_)
    ));
    assert_eq!(fx.spa_sha(), before);

    apply_unchecked(&fx.install, &fx.store, &empty()).unwrap();
    let outcome = backup(&fx.install, &fx.store).unwrap();
    assert!(matches!(outcome, BackupOutcome::OriginalOfPatched(ref r) if r.sha256 == before));
}
