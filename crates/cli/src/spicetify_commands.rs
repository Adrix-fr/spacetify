//! `spacetify leave-spicetify`: shows what leaving Spicetify would do,
//! does it once the user agrees, then applies the mods.

use spacetify_core::spicetify::{self, Plan, SpotifyStep};
use spacetify_core::{Lang, Result, tr};

use crate::commands::{self, Context};
use crate::ui;

pub fn leave(ctx: &Context, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    let found = spicetify::find();
    let plan = spicetify::plan(&install, found.as_ref())?;
    if plan.is_empty() {
        ui::success(tr!(
            lang,
            "Spicetify left nothing behind: there is nothing to do.",
            "Spicetify n'a rien laissé : il n'y a rien à faire."
        ));
        return Ok(());
    }
    print_plan(lang, &plan);
    if let SpotifyStep::Reinstall { reason } = &plan.spotify {
        println!();
        ui::error(reason.get(lang));
        ui::hint(tr!(
            lang,
            "Run `spicetify restore`, or reinstall Spotify from spotify.com/download (your account and downloads stay), then run this again.",
            "Lance `spicetify restore`, ou réinstalle Spotify depuis spotify.com/download (ton compte et tes téléchargements restent), puis relance cette commande."
        ));
        return Ok(());
    }
    println!();
    let question = tr!(
        lang,
        "Leave Spicetify now? (y/N)",
        "Quitter Spicetify maintenant ? (o/N)"
    );
    if !yes && !ui::confirm(&question) {
        ui::hint(tr!(lang, "Nothing was changed.", "Rien n'a été changé."));
        return Ok(());
    }
    // Agreeing to leave is agreeing to close Spotify for it.
    let closed = commands::close_if_running(lang, true)?;
    let outcome = spicetify::leave(&install, &plan, &ctx.dirs())?;
    if outcome.restored {
        ui::success(tr!(
            lang,
            "Spotify's original interface is back.",
            "L'interface d'origine de Spotify est revenue."
        ));
    }
    for folder in &outcome.removed {
        ui::success(tr!(
            lang,
            "Removed Spicetify's {}.",
            "Retiré : {} de Spicetify.",
            folder.display()
        ));
    }
    if !outcome.themes.is_empty() {
        let count = outcome.themes.len();
        ui::success(tr!(
            lang,
            "{count} theme(s) imported.",
            "{count} thème(s) importé(s)."
        ));
    }
    for id in &outcome.installed {
        ui::success(tr!(
            lang,
            "`{id}` installed from the gallery and turned on.",
            "`{id}` installé depuis la galerie et activé."
        ));
    }
    for (id, why) in &outcome.failed {
        ui::warn(tr!(
            lang,
            "`{id}` could not be installed: {}",
            "`{id}` n'a pas pu être installé : {}",
            why.get(lang)
        ));
    }
    println!();
    // The mods go in now, and Spotify opens again if it was open.
    commands::apply(ctx, true)?;
    if closed {
        commands::restart(lang, &install);
    } else if outcome.marketplace_pending {
        ui::hint(tr!(
            lang,
            "Open Spotify: your Marketplace theme is imported the first time it starts.",
            "Ouvre Spotify : ton thème du Marketplace est importé dès son premier démarrage."
        ));
    }
    ui::hint(tr!(
        lang,
        "Spicetify itself is still installed, unused. Its files were left as they were.",
        "Spicetify lui-même est encore installé, mais plus utilisé. Ses fichiers sont restés tels quels."
    ));
    Ok(())
}

fn print_plan(lang: Lang, plan: &Plan) {
    ui::title(&tr!(lang, "Leaving Spicetify", "Quitter Spicetify"));
    let spotify = match &plan.spotify {
        SpotifyStep::Clean => tr!(lang, "nothing to repair", "rien à réparer"),
        SpotifyStep::Leftovers { folders } => tr!(
            lang,
            "remove {} folder(s) Spicetify left inside Spotify",
            "retirer {} dossier(s) laissé(s) par Spicetify dans Spotify",
            folders.len()
        ),
        SpotifyStep::Restore { .. } => tr!(
            lang,
            "put back Spotify's original interface (Spicetify's backup)",
            "remettre l'interface d'origine de Spotify (sauvegarde de Spicetify)"
        ),
        SpotifyStep::Reinstall { .. } => {
            tr!(lang, "cannot be repaired here", "réparation impossible ici")
        }
    };
    ui::row("Spotify", spotify);
    let themes = if plan.themes.is_empty() && !plan.marketplace_theme {
        tr!(lang, "none", "aucun")
    } else {
        let mut names: Vec<String> = plan
            .themes
            .iter()
            .map(|t| {
                if t.current {
                    tr!(lang, "{} (active)", "{} (actif)", t.name)
                } else {
                    t.name.clone()
                }
            })
            .collect();
        if plan.marketplace_theme {
            names.push(tr!(
                lang,
                "the Marketplace theme, if there is one (at Spotify's next start)",
                "le thème du Marketplace, s'il y en a un (au prochain démarrage de Spotify)"
            ));
        }
        names.join(", ")
    };
    ui::row(tr!(lang, "Themes", "Thèmes"), themes);
    for item in &plan.extensions {
        let what = match &item.equivalent {
            Some(id) => tr!(
                lang,
                "→ `{id}` from the gallery",
                "→ `{id}` depuis la galerie"
            ),
            None => tr!(lang, "no equivalent yet", "pas encore d'équivalent"),
        };
        ui::row(&item.name, what);
    }
    for app in &plan.apps {
        let what = match app.equivalent.as_deref() {
            Some("gallery") => tr!(
                lang,
                "→ the spacetify gallery, built in",
                "→ la galerie spacetify, intégrée"
            ),
            _ => tr!(lang, "no equivalent yet", "pas encore d'équivalent"),
        };
        ui::row(&app.name, what);
    }
}
