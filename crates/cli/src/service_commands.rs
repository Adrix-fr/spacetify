//! `spacetify service …`: the background service that puts the mods back
//! after a Spotify update.

use crossterm::style::Stylize;
use spacetify_core::patcher;
use spacetify_core::service::autostart::{self, Autostart};
use spacetify_core::service::events::EventLog;
use spacetify_core::service::{self, StartOutcome};
use spacetify_core::{Error, Result, tr};

use crate::args::ServiceAction;
use crate::commands::{self, Context};
use crate::ui;

pub fn run(ctx: &Context, action: ServiceAction) -> Result<()> {
    match action {
        ServiceAction::Enable => enable(ctx),
        ServiceAction::Disable => disable(ctx),
        ServiceAction::Start => start(ctx),
        ServiceAction::Stop => stop(ctx),
        ServiceAction::Status => status(ctx),
        ServiceAction::Events { count } => events(ctx, count),
        ServiceAction::SimulateUpdate(args) => simulate_update(ctx, args.yes),
    }
}

/// Whether automatic repair is on, for the menu.
pub fn is_on(ctx: &Context) -> bool {
    service::is_running(&ctx.service) || matches!(autostart::status(), Ok(Autostart::On { .. }))
}

/// Options the service needs to see the same folders as this command.
fn service_args(ctx: &Context) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(home) = std::env::var_os("SPACETIFY_HOME") {
        args.push("--home".to_owned());
        args.push(home.to_string_lossy().into_owned());
    }
    if let Some(path) = &ctx.spotify_path_flag {
        args.push("--spotify-path".to_owned());
        args.push(path.to_string_lossy().into_owned());
    }
    args
}

pub fn enable(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let exe = service::executable()?;
    autostart::enable(&exe, &service_args(ctx))?;
    service::start(&ctx.service, &exe, &service_args(ctx))?;
    ui::success(tr!(
        lang,
        "Automatic repair is on. The service runs now and will start with the computer.",
        "Réparation automatique activée. Le service tourne et démarrera avec l'ordinateur."
    ));
    if cfg!(windows) {
        ui::hint(tr!(
            lang,
            "It shows as \"spacetify\" in Task Manager > Startup apps. `spacetify service disable` removes it.",
            "Il apparaît comme « spacetify » dans Gestionnaire des tâches > Applications de démarrage. `spacetify service disable` le retire."
        ));
    }
    if ctx.store().load_applied()?.is_none() {
        ui::hint(tr!(
            lang,
            "Apply your mods once with `spacetify apply`: the service keeps them in place after that.",
            "Applique tes mods une fois avec `spacetify apply` : le service les garde en place ensuite."
        ));
    }
    Ok(())
}

pub fn disable(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    service::stop(&ctx.service)?;
    autostart::disable()?;
    ui::success(tr!(
        lang,
        "Automatic repair is off. The service is stopped and no longer starts with the computer.",
        "Réparation automatique désactivée. Le service est arrêté et ne démarre plus avec l'ordinateur."
    ));
    Ok(())
}

fn start(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let exe = service::executable()?;
    match service::start(&ctx.service, &exe, &service_args(ctx))? {
        StartOutcome::Started => ui::success(tr!(lang, "Service started.", "Service lancé.")),
        StartOutcome::AlreadyRunning => ui::success(tr!(
            lang,
            "The service is already running.",
            "Le service tourne déjà."
        )),
    }
    Ok(())
}

fn stop(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    if service::stop(&ctx.service)? {
        ui::success(tr!(lang, "Service stopped.", "Service arrêté."));
    } else {
        ui::success(tr!(
            lang,
            "The service was not running.",
            "Le service ne tournait pas."
        ));
    }
    Ok(())
}

pub fn status(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let login = match autostart::status() {
        Ok(Autostart::On { .. }) => tr!(lang, "on", "activé"),
        Ok(Autostart::DisabledBySystem { .. }) => tr!(
            lang,
            "turned off in the system settings (Task Manager)",
            "désactivé dans les réglages du système (Gestionnaire des tâches)"
        ),
        Ok(Autostart::Off) => tr!(lang, "off", "désactivé"),
        Err(err) => err.localized(lang),
    };
    ui::row(tr!(lang, "At login", "Au démarrage"), login);

    let running = service::is_running(&ctx.service);
    let status = service::read_status(&ctx.service).unwrap_or_default();
    match (&status, running) {
        (Some(status), true) => {
            let since = service::time_ago(&status.started_at, lang);
            ui::row(
                "Service",
                tr!(
                    lang,
                    "running (PID {}, started {since})",
                    "en marche (PID {}, lancé {since})",
                    status.pid
                ),
            );
            ui::row(tr!(lang, "Now", "Maintenant"), status.phase.describe(lang));
        }
        _ => ui::row("Service", tr!(lang, "stopped", "arrêté")),
    }
    if let Some(status) = &status {
        if let Some(version) = &status.spotify_version {
            ui::row("Spotify", version);
        }
        let last = status.last_repair.as_deref().map_or_else(
            || tr!(lang, "never", "jamais"),
            |at| service::time_ago(at, lang),
        );
        ui::row(tr!(lang, "Repaired", "Réparé"), last);
    }
    ui::row(
        tr!(lang, "Log", "Journal"),
        ctx.service.events_file().display(),
    );
    if !running {
        println!();
        ui::hint(tr!(
            lang,
            "`spacetify service enable` turns automatic repair on.",
            "`spacetify service enable` active la réparation automatique."
        ));
    }
    Ok(())
}

pub fn events(ctx: &Context, count: usize) -> Result<()> {
    let lang = ctx.lang;
    let events = EventLog::new(ctx.service.events_file()).recent(count)?;
    if events.is_empty() {
        ui::hint(tr!(
            lang,
            "No events yet.",
            "Aucun événement pour l'instant."
        ));
        return Ok(());
    }
    let whens: Vec<String> = events
        .iter()
        .map(|event| service::time_ago(&event.at, lang))
        .collect();
    let width = whens.iter().map(|w| w.chars().count()).max().unwrap_or(0);
    for (event, when) in events.iter().zip(whens) {
        println!(
            "  {}  {}",
            ui::paint(format!("{when:<width$}"), |s| s.dark_grey()),
            event.kind.describe(lang).replace('\n', "\n    ")
        );
    }
    Ok(())
}

pub fn simulate_update(ctx: &Context, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    if !service::is_running(&ctx.service) {
        return Err(Error::ServiceNotRunning);
    }
    let install = ctx.install()?;
    install.ensure_modifiable()?;
    ui::hint(tr!(
        lang,
        "This puts back Spotify's original interface, exactly as an update does.\n\
         The service should notice it and put your mods back by itself.",
        "Ceci remet l'interface d'origine de Spotify, exactement comme une mise à jour.\n\
         Le service doit le remarquer et remettre tes mods tout seul."
    ));
    println!();
    let closed = commands::close_if_running(lang, yes)?;
    patcher::simulate_update(&install, &ctx.store())?;
    ui::success(tr!(
        lang,
        "Original interface put back (hash verified), as after an update.",
        "Interface d'origine remise (empreinte vérifiée), comme après une mise à jour."
    ));
    if closed {
        commands::restart(lang, &install);
        ui::hint(tr!(
            lang,
            "Spotify was restarted, as after a real update. A notification offers to restart it:
             press \"Restart Spotify\" (or close Spotify) and the mods come back within seconds.",
            "Spotify a été relancé, comme après une vraie mise à jour. Une notification propose de le relancer :
             clique sur « Relancer Spotify » (ou ferme Spotify) et les mods reviennent en quelques secondes."
        ));
    } else {
        ui::hint(tr!(
            lang,
            "The mods should be back in about 20 seconds. Check with `spacetify service events`.",
            "Les mods devraient revenir d'ici 20 secondes environ. Vérifie avec `spacetify service events`."
        ));
    }
    Ok(())
}
