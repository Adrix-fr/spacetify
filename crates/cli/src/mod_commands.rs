//! Commands about themes and extensions, and the compatibility report.

use std::path::Path;

use crossterm::style::Stylize;
use spacetify_core::doctor::{self, Report, Status, Unverifiable, Verdict};
use spacetify_core::mods::{ActiveMods, Listed, ModKind};
use spacetify_core::{Error, Lang, Result, tr};

use crate::commands::Context;
use crate::ui::{self, paint};

pub fn list(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let config = ctx.config()?;
    let listed = ctx.mods().list(&config)?;

    for kind in [ModKind::Theme, ModKind::Extension] {
        let heading = match kind {
            ModKind::Theme => tr!(lang, "Themes", "Thèmes"),
            ModKind::Extension => "Extensions".to_owned(),
        };
        println!(
            "{}",
            paint(heading.to_uppercase(), |s| s.dark_grey().bold())
        );
        let of_kind: Vec<&Listed> = listed.iter().filter(|l| l.kind == kind).collect();
        if of_kind.is_empty() {
            ui::hint(match kind {
                ModKind::Theme => tr!(lang, "  No theme installed.", "  Aucun thème installé."),
                ModKind::Extension => tr!(
                    lang,
                    "  No extension installed.",
                    "  Aucune extension installée."
                ),
            });
        }
        let width = of_kind.iter().map(|l| l.id.len()).max().unwrap_or(0);
        for item in of_kind {
            print_listed(lang, item, width);
        }
        println!();
    }
    let missing: Vec<&String> = config
        .theme
        .iter()
        .chain(&config.extensions)
        .filter(|id| !listed.iter().any(|l| &&l.id == id))
        .collect();
    for id in missing {
        ui::warn(tr!(
            lang,
            "`{id}` is turned on but no longer installed. Run `spacetify disable {id}`.",
            "`{id}` est activé mais n'est plus installé. Lance `spacetify disable {id}`."
        ));
    }
    ui::hint(tr!(
        lang,
        "Folder: {}",
        "Dossier : {}",
        ctx.config_dir.display()
    ));
    Ok(())
}

fn print_listed(lang: Lang, item: &Listed, width: usize) {
    let id = format!("{:<width$}", item.id);
    let mark = if item.active {
        paint("●", |s| s.green().bold())
    } else {
        paint("○", |s| s.dark_grey())
    };
    match &item.meta {
        Ok(meta) => {
            let state = match (item.active, item.kind) {
                (false, _) => String::new(),
                (true, ModKind::Theme) => tr!(lang, "  (active)", "  (actif)"),
                (true, ModKind::Extension) => tr!(lang, "  (on)", "  (activée)"),
            };
            let by = tr!(lang, "by", "par");
            println!(
                "  {mark} {id}  {} {} · {by} {}{}",
                meta.name,
                meta.version,
                meta.author,
                paint(state, |s| s.green())
            );
        }
        Err(issue) => {
            let refused = tr!(lang, "refused", "refusé");
            println!(
                "  {} {id}  {}",
                paint("✗", |s| s.red().bold()),
                paint(format!("{refused} : {}", issue.localized(lang)), |s| s
                    .red())
            );
        }
    }
}

pub fn install(ctx: &Context, path: &Path) -> Result<()> {
    let lang = ctx.lang;
    let store = ctx.mods();
    let installed = store.install(path)?;
    let mut config = ctx.config()?;
    store.enable(&mut config, &installed.meta.id)?;
    ctx.save(&config)?;

    let name = &installed.meta.name;
    let version = &installed.meta.version;
    ui::success(match installed.kind {
        ModKind::Theme => tr!(
            lang,
            "Theme “{name}” {version} installed. It is now the active theme.",
            "Thème « {name} » {version} installé. C'est maintenant le thème actif."
        ),
        ModKind::Extension => tr!(
            lang,
            "Extension “{name}” {version} installed and turned on.",
            "Extension « {name} » {version} installée et activée."
        ),
    });
    if installed.replaced {
        ui::hint(tr!(
            lang,
            "It replaces the copy that was already installed.",
            "Elle remplace la copie qui était déjà installée."
        ));
    }
    apply_hint(lang);
    Ok(())
}

pub fn uninstall(ctx: &Context, id: &str) -> Result<()> {
    let lang = ctx.lang;
    let mut config = ctx.config()?;
    let kind = ctx.mods().uninstall(&mut config, id)?;
    ctx.save(&config)?;
    ui::success(match kind {
        ModKind::Theme => tr!(lang, "Theme `{id}` deleted.", "Thème `{id}` supprimé."),
        ModKind::Extension => tr!(
            lang,
            "Extension `{id}` deleted.",
            "Extension `{id}` supprimée."
        ),
    });
    apply_hint(lang);
    Ok(())
}

pub fn enable(ctx: &Context, id: &str) -> Result<()> {
    let lang = ctx.lang;
    let mut config = ctx.config()?;
    let (kind, changed) = ctx.mods().enable(&mut config, id)?;
    ctx.save(&config)?;
    ui::success(match (kind, changed) {
        (ModKind::Theme, true) => tr!(
            lang,
            "`{id}` is now the active theme.",
            "`{id}` est maintenant le thème actif."
        ),
        (ModKind::Theme, false) => tr!(
            lang,
            "`{id}` is already the active theme.",
            "`{id}` est déjà le thème actif."
        ),
        (ModKind::Extension, true) => tr!(
            lang,
            "Extension `{id}` turned on.",
            "Extension `{id}` activée."
        ),
        (ModKind::Extension, false) => tr!(
            lang,
            "Extension `{id}` is already on.",
            "L'extension `{id}` est déjà activée."
        ),
    });
    if changed {
        apply_hint(lang);
    }
    Ok(())
}

pub fn disable(ctx: &Context, id: &str) -> Result<()> {
    let lang = ctx.lang;
    let mut config = ctx.config()?;
    let changed = ctx.mods().disable(&mut config, id)?;
    ctx.save(&config)?;
    if changed {
        ui::success(tr!(lang, "`{id}` turned off.", "`{id}` désactivé."));
        apply_hint(lang);
    } else {
        ui::success(tr!(
            lang,
            "`{id}` was already off.",
            "`{id}` était déjà désactivé."
        ));
    }
    Ok(())
}

pub fn theme(ctx: &Context, id: &str) -> Result<()> {
    let lang = ctx.lang;
    let mut config = ctx.config()?;
    let id = (id != "none").then_some(id);
    let changed = ctx.mods().set_theme(&mut config, id)?;
    ctx.save(&config)?;
    ui::success(match (id, changed) {
        (Some(id), true) => tr!(
            lang,
            "`{id}` is now the active theme.",
            "`{id}` est maintenant le thème actif."
        ),
        (Some(id), false) => tr!(
            lang,
            "`{id}` is already the active theme.",
            "`{id}` est déjà le thème actif."
        ),
        (None, true) => tr!(
            lang,
            "Theme turned off: Spotify will use its own look.",
            "Thème désactivé : Spotify reprendra son look d'origine."
        ),
        (None, false) => tr!(lang, "No theme was active.", "Aucun thème n'était actif."),
    });
    if changed {
        apply_hint(lang);
    }
    Ok(())
}

pub fn doctor(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    let config = ctx.config()?;
    let report = doctor::run(&install, &ctx.mods(), &config)?;
    print_report(lang, &report);
    match report.verdict() {
        Verdict::Broken | Verdict::Invalid => Err(Error::Incompatible(problems(&report))),
        _ => Ok(()),
    }
}

/// Loads what is turned on. Used by `apply` and `launch --live`.
pub fn active_mods(ctx: &Context) -> Result<ActiveMods> {
    ctx.mods().active(&ctx.config()?)
}

/// One line per mod, then a short verdict. Shown before applying.
pub fn print_active(lang: Lang, mods: &ActiveMods) {
    let theme = mods.theme.as_ref().map_or_else(
        || tr!(lang, "none (Spotify's look)", "aucun (look d'origine)"),
        |t| format!("{} {}", t.manifest.meta.name, t.manifest.meta.version),
    );
    ui::row(tr!(lang, "Theme", "Thème"), theme);
    let extensions = if mods.extensions.is_empty() {
        tr!(lang, "none", "aucune")
    } else {
        mods.extensions
            .iter()
            .map(|e| format!("{} {}", e.manifest.meta.name, e.manifest.meta.version))
            .collect::<Vec<_>>()
            .join(", ")
    };
    ui::row("Extensions", extensions);
}

/// Non-blocking check before `apply`: warns, never refuses.
pub fn quick_check(ctx: &Context, mods: &ActiveMods) {
    let lang = ctx.lang;
    if mods.is_empty() {
        return;
    }
    let Ok(install) = ctx.install() else { return };
    let report = match doctor::check(&install, mods) {
        Ok(report) => report,
        Err(err) => {
            tracing::warn!("compatibility check skipped: {err}");
            return;
        }
    };
    let label = tr!(lang, "Check", "Vérification");
    match report.verdict() {
        Verdict::Compatible => ui::row(label, tr!(lang, "all good", "tout est compatible")),
        Verdict::Warnings => ui::row(
            label,
            tr!(
                lang,
                "works, with warnings (see `spacetify doctor`)",
                "ça marche, avec des avertissements (voir `spacetify doctor`)"
            ),
        ),
        Verdict::Broken | Verdict::Invalid => {
            let count = problems(&report);
            ui::warn(tr!(
                lang,
                "{count} selector(s) used by your mods were not found in this Spotify. \
                 Some changes may not show. Run `spacetify doctor` for details.",
                "{count} sélecteur(s) utilisé(s) par tes mods sont introuvables dans ce Spotify. \
                 Certains changements risquent de ne pas s'afficher. Lance `spacetify doctor` pour le détail."
            ));
        }
    }
}

fn problems(report: &Report) -> usize {
    report.missing() + report.mods.iter().filter(|m| m.error.is_some()).count()
}

fn print_report(lang: Lang, report: &Report) {
    let version = report
        .spotify_version
        .clone()
        .unwrap_or_else(|| tr!(lang, "unknown version", "version inconnue"));
    ui::row("Spotify", version);
    println!();

    if report.mods.is_empty() {
        ui::hint(tr!(
            lang,
            "No theme or extension is turned on, so there is nothing to check.",
            "Aucun thème ni extension n'est activé, il n'y a donc rien à vérifier."
        ));
        return;
    }

    for item in &report.mods {
        let kind = capitalize(item.kind.label(lang));
        let title = item.title.clone().unwrap_or_else(|| item.id.clone());
        let found = item.count(|s| s == Status::Found);
        let missing = item.count(|s| s == Status::Missing);
        let headline = format!("{kind} {title}");
        match item.verdict() {
            Verdict::Compatible if item.checks.is_empty() => ui::success(tr!(
                lang,
                "{headline}: nothing to check (no selector declared)",
                "{headline} : rien à vérifier (aucun sélecteur déclaré)"
            )),
            Verdict::Compatible => ui::success(tr!(
                lang,
                "{headline}: compatible ({found} element(s) checked, all present)",
                "{headline} : compatible ({found} élément(s) vérifié(s), tous présents)"
            )),
            Verdict::Warnings => ui::warn(tr!(
                lang,
                "{headline}: works today, with warnings",
                "{headline} : fonctionne aujourd'hui, avec des avertissements"
            )),
            Verdict::Broken => ui::error(tr!(
                lang,
                "{headline}: {missing} element(s) no longer exist",
                "{headline} : {missing} élément(s) n'existe(nt) plus"
            )),
            Verdict::Invalid => ui::error(tr!(
                lang,
                "{headline}: cannot be loaded",
                "{headline} : impossible à charger"
            )),
        }
        if let Some(err) = &item.error {
            println!("    {}", err.localized(lang));
        }
        for check in item.checks.iter().filter(|c| c.status != Status::Found) {
            let target = check.target.css();
            let (symbol, text) = match check.status {
                Status::Found => continue,
                Status::Missing => (
                    paint("✗", |s| s.red().bold()),
                    tr!(lang, "{target} not found", "{target} introuvable"),
                ),
                Status::Fragile => (
                    paint("!", |s| s.yellow().bold()),
                    tr!(
                        lang,
                        "{target} exists, but its name is generated and may change with the next update",
                        "{target} existe, mais son nom est généré et peut changer à la prochaine mise à jour"
                    ),
                ),
                Status::Unverifiable(Unverifiable::Translated) => (
                    paint("?", |s| s.dark_grey().bold()),
                    tr!(
                        lang,
                        "{target} cannot be checked: the text depends on Spotify's language",
                        "{target} impossible à vérifier : le texte dépend de la langue de Spotify"
                    ),
                ),
                Status::Unverifiable(Unverifiable::SetAtRuntime) => (
                    paint("?", |s| s.dark_grey().bold()),
                    tr!(
                        lang,
                        "{target} cannot be checked without running Spotify",
                        "{target} impossible à vérifier sans lancer Spotify"
                    ),
                ),
            };
            println!("    {symbol} {text}");
        }
    }
    println!();
    match report.verdict() {
        Verdict::Compatible => ui::success(tr!(
            lang,
            "Everything is compatible with this version of Spotify.",
            "Tout est compatible avec cette version de Spotify."
        )),
        Verdict::Warnings => ui::hint(tr!(
            lang,
            "Nothing is broken. The warnings point to what could break with a future update.",
            "Rien n'est cassé. Les avertissements montrent ce qui pourrait casser avec une prochaine mise à jour."
        )),
        Verdict::Broken | Verdict::Invalid => {}
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

pub fn apply_hint(lang: Lang) {
    ui::hint(tr!(
        lang,
        "Run `spacetify apply` (or `spacetify launch --live`) to see the change in Spotify.",
        "Lance `spacetify apply` (ou `spacetify launch --live`) pour voir le changement dans Spotify."
    ));
}
