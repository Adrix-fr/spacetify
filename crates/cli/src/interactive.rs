//! The menu shown when `spacetify` runs without a subcommand.

use std::io;
use std::path::PathBuf;

use crossterm::style::Stylize;
use spacetify_core::config::Config;
use spacetify_core::mods::ModKind;
use spacetify_core::{Lang, Result, detect, spotify, tr};

use crate::commands::{self, Context};
use crate::menu::{self, Entry};
use crate::ui::{self, ACCENT, paint};
use crate::{mod_commands, service_commands};

#[derive(Clone, Copy)]
enum Action {
    Apply,
    Restore,
    Backup,
    Status,
    Launch,
    LaunchLive,
    ListMods,
    InstallMod,
    ChooseTheme,
    ToggleExtensions,
    UninstallMod,
    Doctor,
    AutoRepair,
    ServiceStatus,
    ServiceEvents,
    SimulateUpdate,
    Language,
    Quit,
}

pub fn run(ctx: &mut Context) -> io::Result<()> {
    loop {
        let items = items(ctx);
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
            Action::ListMods => screen(ctx, title, mod_commands::list)?,
            Action::InstallMod => screen(ctx, title, install_mod)?,
            Action::ChooseTheme => choose_theme(ctx, title)?,
            Action::ToggleExtensions => toggle_extensions(ctx, title)?,
            Action::UninstallMod => uninstall_mod(ctx, title)?,
            Action::Doctor => screen(ctx, title, mod_commands::doctor)?,
            Action::AutoRepair => screen(ctx, title, |ctx| {
                if service_commands::is_on(ctx) {
                    service_commands::disable(ctx)
                } else {
                    service_commands::enable(ctx)
                }
            })?,
            Action::ServiceStatus => screen(ctx, title, service_commands::status)?,
            Action::ServiceEvents => screen(ctx, title, |ctx| service_commands::events(ctx, 15))?,
            Action::SimulateUpdate => screen(ctx, title, |ctx| {
                service_commands::simulate_update(ctx, false)
            })?,
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

fn items(ctx: &Context) -> Items {
    let lang = ctx.lang;
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

    items.section(tr!(lang, "Themes and extensions", "Thèmes et extensions"));
    items.choice(
        Action::ListMods,
        tr!(lang, "Installed mods", "Mods installés"),
        tr!(
            lang,
            "what is installed and on",
            "ce qui est installé et activé"
        ),
    );
    items.choice(
        Action::InstallMod,
        tr!(lang, "Install from a folder", "Installer depuis un dossier"),
        tr!(lang, "a theme or an extension", "un thème ou une extension"),
    );
    items.choice(
        Action::ChooseTheme,
        tr!(lang, "Choose the theme", "Choisir le thème"),
        "",
    );
    items.choice(
        Action::ToggleExtensions,
        tr!(
            lang,
            "Turn extensions on/off",
            "Activer/désactiver des extensions"
        ),
        "",
    );
    items.choice(
        Action::UninstallMod,
        tr!(lang, "Uninstall a mod", "Désinstaller un mod"),
        "",
    );
    items.choice(
        Action::Doctor,
        tr!(lang, "Check compatibility", "Vérifier la compatibilité"),
        tr!(
            lang,
            "with this version of Spotify",
            "avec cette version de Spotify"
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

    items.section(tr!(lang, "Automatic repair", "Réparation automatique"));
    let (state, hint) = if service_commands::is_on(ctx) {
        (
            tr!(lang, "on", "activée"),
            tr!(lang, "enter to turn off", "entrée pour désactiver"),
        )
    } else {
        (
            tr!(lang, "off", "désactivée"),
            tr!(
                lang,
                "puts mods back after Spotify updates",
                "remet les mods après les mises à jour"
            ),
        )
    };
    items.choice(
        Action::AutoRepair,
        tr!(
            lang,
            "Automatic repair: {state}",
            "Réparation auto : {state}"
        ),
        hint,
    );
    items.choice(
        Action::ServiceStatus,
        tr!(lang, "Service status", "État du service"),
        "",
    );
    items.choice(
        Action::ServiceEvents,
        tr!(lang, "Latest events", "Derniers événements"),
        "",
    );
    items.choice(
        Action::SimulateUpdate,
        tr!(lang, "Simulate an update", "Simuler une mise à jour"),
        tr!(lang, "to test the repair", "pour tester la réparation"),
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

fn install_mod(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let question = tr!(
        lang,
        "Folder of the theme or extension (you can drag it here):",
        "Dossier du thème ou de l'extension (tu peux le glisser ici) :"
    );
    match ui::ask_line(&question) {
        Some(path) => mod_commands::install(ctx, &PathBuf::from(path)),
        None => Ok(()),
    }
}

/// Installed mods of one kind, as `(id, label, on)`.
fn installed(ctx: &Context, kind: ModKind) -> Result<Vec<(String, String, bool)>> {
    let config = ctx.config()?;
    Ok(ctx
        .mods()
        .list(&config)?
        .into_iter()
        .filter(|item| item.kind == kind)
        .map(|item| {
            let label = match &item.meta {
                Ok(meta) => format!("{} {}", meta.name, meta.version),
                Err(_) => tr!(ctx.lang, "{} (refused)", "{} (refusé)", item.id),
            };
            (item.id, label, item.active)
        })
        .collect())
}

/// A sub-menu. Returns the picked index, or `None` to go back.
fn pick(ctx: &Context, title: &str, entries: &[Entry]) -> io::Result<Option<usize>> {
    let footer = tr!(
        ctx.lang,
        "↑↓ move · enter select · esc back",
        "↑↓ naviguer · entrée valider · échap retour"
    );
    let header = vec![format!("  {}", paint(title, |s| s.with(ACCENT).bold()))];
    menu::select(&header, entries, &footer)
}

fn choose_theme(ctx: &Context, title: &str) -> io::Result<()> {
    let lang = ctx.lang;
    let themes = match installed(ctx, ModKind::Theme) {
        Ok(themes) => themes,
        Err(err) => return screen(ctx, title, |_| Err(err)),
    };
    let none_active = !themes.iter().any(|(_, _, active)| *active);
    let mut entries = vec![Entry::section("")];
    entries.push(Entry::choice(
        tr!(lang, "None (Spotify's look)", "Aucun (look d'origine)"),
        active_hint(lang, none_active, ModKind::Theme),
    ));
    for (_, label, active) in &themes {
        entries.push(Entry::choice(
            label.clone(),
            active_hint(lang, *active, ModKind::Theme),
        ));
    }
    let Some(picked) = pick(ctx, title, &entries)? else {
        return Ok(());
    };
    let id = match picked {
        0 => "none".to_owned(),
        n => themes[n - 1].0.clone(),
    };
    screen(ctx, title, |ctx| mod_commands::theme(ctx, &id))
}

fn toggle_extensions(ctx: &Context, title: &str) -> io::Result<()> {
    let lang = ctx.lang;
    loop {
        let extensions = match installed(ctx, ModKind::Extension) {
            Ok(extensions) => extensions,
            Err(err) => return screen(ctx, title, |_| Err(err)),
        };
        if extensions.is_empty() {
            return screen(ctx, title, |_| {
                ui::hint(tr!(
                    lang,
                    "No extension installed yet.",
                    "Aucune extension installée pour l'instant."
                ));
                Ok(())
            });
        }
        let mut entries = vec![Entry::section("")];
        for (_, label, active) in &extensions {
            let mark = if *active { "●" } else { "○" };
            entries.push(Entry::choice(
                format!("{mark} {label}"),
                active_hint(lang, *active, ModKind::Extension),
            ));
        }
        let Some(picked) = pick(ctx, title, &entries)? else {
            return Ok(());
        };
        let (id, _, active) = &extensions[picked];
        let result = if *active {
            ctx.config().and_then(|mut config| {
                ctx.mods().disable(&mut config, id)?;
                ctx.save(&config)
            })
        } else {
            ctx.config().and_then(|mut config| {
                ctx.mods().enable(&mut config, id)?;
                ctx.save(&config)
            })
        };
        if let Err(err) = result {
            screen(ctx, title, |_| Err(err))?;
        }
    }
}

fn uninstall_mod(ctx: &Context, title: &str) -> io::Result<()> {
    let lang = ctx.lang;
    let mut all = Vec::new();
    for kind in [ModKind::Theme, ModKind::Extension] {
        match installed(ctx, kind) {
            Ok(list) => all.extend(list.into_iter().map(|(id, label, _)| (kind, id, label))),
            Err(err) => return screen(ctx, title, |_| Err(err)),
        }
    }
    if all.is_empty() {
        return screen(ctx, title, |_| {
            ui::hint(tr!(lang, "Nothing is installed.", "Rien n'est installé."));
            Ok(())
        });
    }
    let mut entries = vec![Entry::section("")];
    for (kind, _, label) in &all {
        entries.push(Entry::choice(label.clone(), kind.label(lang)));
    }
    let Some(picked) = pick(ctx, title, &entries)? else {
        return Ok(());
    };
    let (_, id, label) = &all[picked];
    screen(ctx, title, |ctx| {
        let question = tr!(lang, "Delete {label}? (y/N)", "Supprimer {label} ? (o/N)");
        if ui::confirm(&question) {
            mod_commands::uninstall(ctx, id)
        } else {
            Ok(())
        }
    })
}

fn active_hint(lang: Lang, active: bool, kind: ModKind) -> String {
    match (active, kind) {
        (false, _) => String::new(),
        (true, ModKind::Theme) => tr!(lang, "active", "actif"),
        (true, ModKind::Extension) => tr!(lang, "on", "activée"),
    }
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
