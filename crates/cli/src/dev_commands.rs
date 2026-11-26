//! Tools for mod authors: `spacetify new` makes a ready-to-use folder,
//! `spacetify dev` runs a mod in Spotify and reloads it at each save.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use spacetify_core::gallery::Link;
use spacetify_core::mods::{self, ModKind};
use spacetify_core::patcher::{Payload, live};
use spacetify_core::{Error, Result, tr};

use crate::args::NewKind;
use crate::commands::{self, Context};
use crate::{mod_commands, ui};

/// Saves often come in bursts (editors write several files): the mod is
/// installed again once nothing changed for this long.
const SETTLE: Duration = Duration::from_millis(300);

const EXTENSION_TEMPLATE: &[(&str, &str)] = &[
    (
        "extension.toml",
        include_str!("../templates/extension/extension.toml"),
    ),
    ("main.js", include_str!("../templates/extension/main.js")),
    (
        "spacetify.d.ts",
        include_str!("../templates/extension/spacetify.d.ts"),
    ),
    (
        "jsconfig.json",
        include_str!("../templates/extension/jsconfig.json"),
    ),
    (
        "README.md",
        include_str!("../templates/extension/README.md"),
    ),
];

const THEME_TEMPLATE: &[(&str, &str)] = &[
    ("theme.toml", include_str!("../templates/theme/theme.toml")),
    (
        "colors.toml",
        include_str!("../templates/theme/colors.toml"),
    ),
    ("theme.css", include_str!("../templates/theme/theme.css")),
    ("README.md", include_str!("../templates/theme/README.md")),
];

pub fn new(ctx: &Context, kind: NewKind, id: &str, into: Option<&Path>) -> Result<()> {
    let lang = ctx.lang;
    let dir = create(kind, id, into.unwrap_or(Path::new(".")))?;
    let shown = dir.display();
    ui::success(match kind {
        NewKind::Extension => tr!(
            lang,
            "Extension `{id}` created in {shown}.",
            "Extension `{id}` créée dans {shown}."
        ),
        NewKind::Theme => tr!(
            lang,
            "Theme `{id}` created in {shown}.",
            "Thème `{id}` créé dans {shown}."
        ),
    });
    ui::hint(tr!(
        lang,
        "Try it: `spacetify dev {shown}`. Spotify starts with it and reloads it each time you save a file.",
        "Pour l'essayer : `spacetify dev {shown}`. Spotify démarre avec, et le recharge à chaque fois que tu enregistres un fichier."
    ));
    Ok(())
}

/// Writes the template into `<parent>/<id>`, which must not exist yet.
fn create(kind: NewKind, id: &str, parent: &Path) -> Result<PathBuf> {
    if !mods::manifest::is_valid_id(id) {
        return Err(Error::InvalidId(id.to_owned()));
    }
    let dir = parent.join(id);
    if dir.exists() {
        return Err(Error::AlreadyExists(dir));
    }
    let template = match kind {
        NewKind::Extension => EXTENSION_TEMPLATE,
        NewKind::Theme => THEME_TEMPLATE,
    };
    fs::create_dir_all(&dir).map_err(io_error(&dir))?;
    let name = display_name(id);
    let version = mods::manifest::current_version();
    let spacetify = format!("{}.{}.0", version.major, version.minor);
    for (file, text) in template {
        let path = dir.join(file);
        let text = text
            .replace("{{id}}", id)
            .replace("{{name}}", &name)
            .replace("{{spacetify}}", &spacetify);
        fs::write(&path, text).map_err(io_error(&path))?;
    }
    Ok(dir)
}

/// "my-cool-mod" → "My cool mod".
fn display_name(id: &str) -> String {
    let words = id.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

pub fn dev(ctx: &Context, path: &Path, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    let store = ctx.mods();
    let installed = store.install(path)?;
    let mut config = ctx.config()?;
    store.enable(&mut config, &installed.meta.id)?;
    ctx.save(&config)?;
    let name = installed.meta.name.clone();
    ui::success(match installed.kind {
        ModKind::Theme => tr!(
            lang,
            "Theme “{name}” installed and made the active theme.",
            "Thème « {name} » installé et choisi comme thème actif."
        ),
        ModKind::Extension => tr!(
            lang,
            "Extension “{name}” installed and turned on.",
            "Extension « {name} » installée et activée."
        ),
    });
    ui::warn(tr!(
        lang,
        "Like live mode, this gives Spotify a debugging port on 127.0.0.1 while it runs.",
        "Comme le mode direct, ceci donne à Spotify un port de débogage sur 127.0.0.1 tant qu'il tourne."
    ));
    commands::close_if_running(lang, yes)?;

    let first = payload(ctx)?;
    let (updates, received) = mpsc::channel();
    let (changes, changed) = mpsc::channel();
    // Absolute, without the `\\?\` prefix Windows adds when resolving links,
    // so the paths of the events start with it.
    let root = std::path::absolute(path).map_err(io_error(path))?;
    let watched = root.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event
                .paths
                .iter()
                .any(|path| !is_hidden(path.strip_prefix(&watched).unwrap_or(path)))
        {
            let _ = changes.send(());
        }
    })
    .map_err(|err| Error::Watch(err.to_string()))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|err| Error::Watch(err.to_string()))?;

    // Installs the mod again after each save and hands the new payload to
    // the live session. A broken save is reported and skipped: Spotify
    // keeps the last good version.
    let source = path.to_path_buf();
    let context = ctx.clone();
    thread::spawn(move || {
        while changed.recv().is_ok() {
            while changed.recv_timeout(SETTLE).is_ok() {}
            let reinstalled = context
                .mods()
                .install(&source)
                .and_then(|_| payload(&context));
            match reinstalled {
                Ok(next) => {
                    if updates.send(next).is_err() {
                        return;
                    }
                }
                Err(err) => ui::error(tr!(
                    context.lang,
                    "Not reloaded: {}",
                    "Pas rechargé : {}",
                    err.localized(context.lang)
                )),
            }
        }
    });

    ui::hint(tr!(lang, "Starting Spotify…", "Lancement de Spotify…"));
    live::run_dev(&install, &first, &received, |reloads| {
        if reloads == 0 {
            ui::success(tr!(
                lang,
                "Ready. Edit and save your files: Spotify reloads them. Close Spotify to stop.",
                "Prêt. Modifie et enregistre tes fichiers : Spotify les recharge. Ferme Spotify pour arrêter."
            ));
        } else {
            ui::success(tr!(lang, "Reloaded.", "Rechargé."));
        }
    })?;
    drop(watcher);
    ui::success(tr!(
        lang,
        "Spotify was closed, dev session ended.",
        "Spotify a été fermé, fin de la session de développement."
    ));
    mod_commands::apply_hint(lang);
    Ok(())
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.to_path_buf();
    move |source| Error::Io { path, source }
}

fn payload(ctx: &Context) -> Result<Payload> {
    let mods = mod_commands::active_mods(ctx)?;
    Ok(Payload::new(&mods).with_gallery(Link::for_payload(&ctx.service).as_ref()))
}

/// Editors and version control keep their own files there (.git, .swp…).
/// `path` is relative to the mod's folder.
fn is_hidden(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        || path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with('~'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_ids() {
        assert_eq!(display_name("my-cool-mod"), "My cool mod");
        assert_eq!(display_name("x"), "X");
    }

    #[test]
    fn new_mods_are_valid_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        for (kind, id) in [(NewKind::Extension, "my-ext"), (NewKind::Theme, "my-theme")] {
            let made = create(kind, id, dir.path()).unwrap();
            let loaded = mods::load_dir(&made).unwrap();
            assert_eq!(loaded.meta().id, id);
            assert!(loaded.meta().name.starts_with("My "));
            assert!(matches!(
                create(kind, id, dir.path()),
                Err(Error::AlreadyExists(_))
            ));
        }
        let main = fs::read_to_string(dir.path().join("my-ext/main.js")).unwrap();
        assert!(!main.contains("{{"));
        assert!(matches!(
            create(NewKind::Theme, "Bad Id", dir.path()),
            Err(Error::InvalidId(_))
        ));
    }

    #[test]
    fn editor_files_do_not_trigger_reloads() {
        assert!(is_hidden(Path::new(".git/index")));
        assert!(is_hidden(Path::new("main.js~")));
        assert!(!is_hidden(Path::new("main.js")));
        assert!(!is_hidden(Path::new("assets/logo.png")));
    }
}
