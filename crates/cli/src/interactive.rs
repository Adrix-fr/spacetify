//! The menu shown when `spacetify` runs without a subcommand.

use std::io;

use crossterm::style::Stylize;
use spacetify_core::config::Config;
use spacetify_core::{Lang, Result, detect, spotify, tr};

use crate::commands::{self, Context};
use crate::menu::{self, Entry};
use crate::ui::{self, ACCENT, paint};

#[derive(Clone, Copy)]
enum Action {
    Apply,
    Restore,
    Backup,
    Status,
    Launch,
    LaunchLive,
    Language,
    Quit,
}

pub fn run(ctx: &mut Context) -> io::Result<()> {
    loop {
        let items = items(ctx.lang);
        let footer = tr!(
            ctx.lang,
            "↑↓ move · enter select · q quit",
            "↑↓ naviguer · entrée valider · q quitter"
        );
        let Some(picked) = menu::select(&header(ctx), &items.entries, &footer)? else {
            return ui::clear_screen();
        };
        let (action, title) = &items.actions[picked];

        match action {
            Action::Quit => return ui::clear_screen(),
            Action::Language => switch_language(ctx),
            Action::Apply => screen(ctx, title, |ctx| commands::apply(ctx, false))?,
            Action::Restore => screen(ctx, title, |ctx| commands::restore(ctx, false))?,
            Action::Backup => screen(ctx, title, commands::backup)?,
            Action::Status => screen(ctx, title, commands::status)?,
            Action::Launch => screen(ctx, title, |ctx| commands::launch(ctx, false, false))?,
            Action::LaunchLive => screen(ctx, title, |ctx| commands::launch(ctx, true, false))?,
        }
    }
}

/// Runs one action on a clean screen, then waits before going back to the menu.
fn screen(
    ctx: &Context,
    title: &str,
    action: impl FnOnce(&Context) -> Result<()>,
) -> io::Result<()> {
    ui::clear_screen()?;
    println!();
    ui::title(title);
    if let Err(err) = action(ctx) {
        ui::error(err.localized(ctx.lang));
    }
    println!();
    ui::hint(tr!(
        ctx.lang,
        "Press any key to go back to the menu",
        "Appuie sur une touche pour revenir au menu"
    ));
    ui::wait_for_key()
}

#[derive(Default)]
struct Items {
    entries: Vec<Entry>,
    actions: Vec<(Action, String)>,
}

impl Items {
    fn section(&mut self, name: impl Into<String>) {
        self.entries.push(Entry::section(name));
    }

    fn choice(&mut self, action: Action, label: String, hint: impl Into<String>) {
        self.entries.push(Entry::choice(label.clone(), hint));
        self.actions.push((action, label));
    }
}

fn items(lang: Lang) -> Items {
    let mut items = Items::default();

    items.section("Mods");
    items.choice(
        Action::Apply,
        tr!(lang, "Apply mods", "Appliquer les mods"),
        tr!(lang, "back up, then inject", "sauvegarde, puis injecte"),
    );
    items.choice(
        Action::Restore,
        tr!(lang, "Restore Spotify", "Restaurer Spotify"),
        tr!(
            lang,
            "put it back exactly as it was",
            "le remettre exactement comme avant"
        ),
    );
    items.choice(
        Action::Backup,
        tr!(lang, "Back up only", "Sauvegarder seulement"),
        tr!(
            lang,
            "save the original, change nothing",
            "garder l'original, sans rien modifier"
        ),
    );

    items.section("Spotify");
    items.choice(
        Action::Status,
        tr!(lang, "Status", "État"),
        tr!(
            lang,
            "where it is and what changed",
            "où il est et ce qui a changé"
        ),
    );
    items.choice(Action::Launch, tr!(lang, "Launch", "Lancer"), "");
    items.choice(
        Action::LaunchLive,
        tr!(lang, "Live launch", "Lancer en direct"),
        tr!(
            lang,
            "inject without touching any file",
            "injecter sans toucher aux fichiers"
        ),
    );

    items.section(tr!(lang, "Settings", "Réglages"));
    let language = match lang {
        Lang::En => "English",
        Lang::Fr => "Français",
    };
    items.choice(Action::Language, tr!(lang, "Language", "Langue"), language);

    items.section("");
    items.choice(Action::Quit, tr!(lang, "Quit", "Quitter"), "");
    items
}

fn header(ctx: &Context) -> Vec<String> {
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    vec![
        format!(
            "  {}  {}",
            paint("spacetify", |s| s.with(ACCENT).bold()),
            paint(version, |s| s.dark_grey())
        ),
        format!("  {}", paint(spotify_summary(ctx), |s| s.dark_grey())),
    ]
}

/// One line such as `Spotify 1.3.6.1921 · standard · original`.
fn spotify_summary(ctx: &Context) -> String {
    let lang = ctx.lang;
    let Ok(install) = spotify::locate(ctx.spotify_path.as_deref()) else {
        return tr!(lang, "Spotify not found", "Spotify introuvable");
    };
    let version = install.version.as_deref().unwrap_or("?");
    let state = match detect::inspect(&install) {
        Ok(inspection) => commands::state_label(lang, &inspection.state),
        Err(_) => "?".into(),
    };
    format!("Spotify {version} · {} · {state}", install.kind.label())
}

/// Toggles between English and French and remembers the choice.
fn switch_language(ctx: &mut Context) {
    ctx.lang = match ctx.lang {
        Lang::En => Lang::Fr,
        Lang::Fr => Lang::En,
    };
    let code = match ctx.lang {
        Lang::En => "en",
        Lang::Fr => "fr",
    };
    let saved = Config::load(&ctx.config_file).and_then(|mut config| {
        config.language = Some(code.to_owned());
        config.save(&ctx.config_file)
    });
    if let Err(err) = saved {
        tracing::warn!("could not save the language: {err}");
    }
}
