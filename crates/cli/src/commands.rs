//! One function per subcommand. The logic lives in `spacetify_core`; this
//! file only talks to the user.

use std::path::PathBuf;
use std::time::Duration;

use spacetify_core::backup::BackupStore;
use spacetify_core::config::Config;
use spacetify_core::detect::{self, UiState};
use spacetify_core::gallery::Link;
use spacetify_core::mods::ModStore;
use spacetify_core::patcher::{self, ApplyOutcome, BackupOutcome, Payload, RestoreOutcome, live};
use spacetify_core::service::ServicePaths;
use spacetify_core::spotify::{self, InstallKind, SpotifyInstall};
use spacetify_core::{Error, Lang, Result, tr};

use crate::args::{Command, InstallerStep};
use crate::{mod_commands, service_commands, ui};

const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Context {
    pub lang: Lang,
    pub spotify_path: Option<PathBuf>,
    pub backups_dir: PathBuf,
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub service: ServicePaths,
    /// `--spotify-path`, given on the command line (passed on to the service).
    pub spotify_path_flag: Option<PathBuf>,
}

impl Context {
    pub fn install(&self) -> Result<SpotifyInstall> {
        spotify::locate(self.spotify_path.as_deref())
    }

    pub fn store(&self) -> BackupStore {
        BackupStore::new(&self.backups_dir)
    }

    pub fn mods(&self) -> ModStore {
        ModStore::new(&self.config_dir)
    }

    pub fn config(&self) -> Result<Config> {
        Config::load(&self.config_file)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        config.save(&self.config_file)
    }
}

pub fn run(ctx: &Context, command: Command) -> Result<()> {
    match command {
        Command::Status => status(ctx),
        Command::Backup => backup(ctx),
        Command::Apply(args) => apply(ctx, args.yes),
        Command::Restore(args) => restore(ctx, args.yes),
        Command::Launch(args) => launch(ctx, args.live, args.close.yes),
        Command::List => mod_commands::list(ctx),
        Command::Install { path } => mod_commands::install(ctx, &path),
        Command::Uninstall { id } => mod_commands::uninstall(ctx, &id),
        Command::Enable { id } => mod_commands::enable(ctx, &id),
        Command::Disable { id } => mod_commands::disable(ctx, &id),
        Command::Theme { id } => mod_commands::theme(ctx, &id),
        Command::Doctor => mod_commands::doctor(ctx),
        Command::Service { action } => service_commands::run(ctx, action),
        Command::Installer { step } => installer(ctx, step),
    }
}

/// Steps run by the Windows installer, in the folder of this program.
fn installer(ctx: &Context, step: InstallerStep) -> Result<()> {
    let exe = std::env::current_exe().map_err(|err| Error::Setup(err.to_string()))?;
    let dir = exe
        .parent()
        .ok_or_else(|| Error::Setup(exe.display().to_string()))?;
    let steps = match step {
        InstallerStep::PostInstall => spacetify_core::setup::after_install(dir, &ctx.service)?,
        InstallerStep::PreUninstall => spacetify_core::setup::before_uninstall(dir, &ctx.service)?,
    };
    for line in steps {
        ui::success(line);
    }
    Ok(())
}

pub fn status(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    let store = ctx.store();

    ui::row("Spotify", install.root.display());
    let version = install
        .version
        .clone()
        .unwrap_or_else(|| tr!(lang, "unknown", "inconnue"));
    ui::row("Version", version);

    let kind = install.kind.label();
    if !install.kind.is_modifiable() {
        ui::row(
            "Type",
            tr!(lang, "{kind} (read-only)", "{kind} (lecture seule)"),
        );
        println!();
        ui::warn(Error::ReadOnlyInstall { kind }.localized(lang));
        return Ok(());
    }
    ui::row("Type", kind);

    let inspection = detect::inspect(&install)?;
    ui::row("Interface", state_label(lang, &inspection.state));
    if let Some(sha256) = &inspection.sha256 {
        ui::row("SHA-256", sha256);
    }

    let running = match spotify::is_running() {
        Ok(true) => tr!(lang, "yes", "oui"),
        Ok(false) => tr!(lang, "no", "non"),
        Err(_) => "?".into(),
    };
    ui::row(tr!(lang, "Running", "Ouvert"), running);

    let spicetify = if let Some(dir) = &inspection.spicetify_leftovers {
        let dir = dir.display();
        tr!(
            lang,
            "inactive leftovers in {dir} (ignored by Spotify, left untouched)",
            "restes inactifs dans {dir} (ignorés par Spotify, laissés tels quels)"
        )
    } else if detect::spicetify_installed() {
        tr!(lang, "installed", "installé")
    } else {
        tr!(lang, "not found", "absent")
    };
    ui::row("Spicetify", spicetify);

    let count = store.list()?.len();
    let dir = store.root().display();
    ui::row(
        tr!(lang, "Backups", "Sauvegardes"),
        tr!(lang, "{count} in {dir}", "{count} dans {dir}"),
    );

    if install.kind == InstallKind::Standard && spotify::microsoft_store_installed() {
        println!();
        ui::warn(tr!(
            lang,
            "The Microsoft Store version is also installed. spacetify only uses the regular one.",
            "La version Microsoft Store est aussi installée. spacetify utilise seulement la version classique."
        ));
    }
    match inspection.state {
        UiState::Spicetify => {
            println!();
            ui::warn(Error::SpicetifyActive.localized(lang));
        }
        UiState::Original if store.load_applied()?.is_some() => {
            println!();
            ui::warn(tr!(
                lang,
                "A Spotify update replaced the patched interface. Run `spacetify apply` again.",
                "Une mise à jour de Spotify a remplacé l'interface modifiée. Relance `spacetify apply`."
            ));
        }
        _ => {}
    }
    Ok(())
}

pub fn backup(ctx: &Context) -> Result<()> {
    let lang = ctx.lang;
    let store = ctx.store();
    match patcher::backup(&ctx.install()?, &store)? {
        BackupOutcome::Created(record) => {
            ui::success(tr!(lang, "Backup created.", "Sauvegarde créée."));
            ui::row(
                tr!(lang, "File", "Fichier"),
                store.file_for(&record).display(),
            );
            ui::row("SHA-256", record.sha256);
        }
        BackupOutcome::AlreadyBackedUp(record) => {
            ui::success(tr!(lang, "Already backed up.", "Déjà sauvegardé."));
            ui::row(
                tr!(lang, "File", "Fichier"),
                store.file_for(&record).display(),
            );
        }
        BackupOutcome::OriginalOfPatched(record) => {
            ui::success(tr!(
                lang,
                "Spotify is already patched. Its original interface is backed up.",
                "Spotify est déjà modifié par spacetify. Son interface d'origine est sauvegardée."
            ));
            ui::row(
                tr!(lang, "File", "Fichier"),
                store.file_for(&record).display(),
            );
        }
    }
    Ok(())
}

pub fn apply(ctx: &Context, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    install.ensure_modifiable()?;
    let mods = mod_commands::active_mods(ctx)?;
    mod_commands::print_active(lang, &mods);
    mod_commands::quick_check(ctx, &mods);
    println!();
    let closed = close_if_running(lang, yes)?;

    let result = patcher::apply(
        &install,
        &ctx.store(),
        &Payload::new(&mods).with_gallery(Link::for_payload(&ctx.service).as_ref()),
    );
    match &result {
        Ok(ApplyOutcome::Applied { new_backup, sha256 }) => {
            if *new_backup {
                ui::success(tr!(
                    lang,
                    "Original interface backed up.",
                    "Interface d'origine sauvegardée."
                ));
            }
            ui::success(tr!(lang, "Mods applied.", "Mods appliqués."));
            ui::row("SHA-256", sha256);
        }
        Ok(ApplyOutcome::AlreadyApplied) => {
            ui::success(tr!(
                lang,
                "Already applied, nothing to do.",
                "Déjà appliqué, rien à faire."
            ));
        }
        Err(_) => {}
    }
    if closed {
        restart(lang, &install);
    } else if result.is_ok() {
        ui::hint(tr!(
            lang,
            "Start Spotify to see the result.",
            "Lance Spotify pour voir le résultat."
        ));
    }
    result.map(drop)
}

pub fn restore(ctx: &Context, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;
    install.ensure_modifiable()?;
    let closed = close_if_running(lang, yes)?;

    let result = patcher::restore(&install, &ctx.store());
    match &result {
        Ok(RestoreOutcome::Restored { sha256 }) => {
            ui::success(tr!(
                lang,
                "Spotify is back to its original state (hash verified).",
                "Spotify est revenu à son état d'origine (empreinte vérifiée)."
            ));
            ui::row("SHA-256", sha256);
        }
        Ok(RestoreOutcome::AlreadyOriginal) => {
            ui::success(tr!(
                lang,
                "Spotify's interface is already original, nothing to do.",
                "L'interface de Spotify est déjà d'origine, rien à faire."
            ));
        }
        Err(_) => {}
    }
    if closed {
        restart(lang, &install);
    }
    result.map(drop)
}

pub fn launch(ctx: &Context, live_mode: bool, yes: bool) -> Result<()> {
    let lang = ctx.lang;
    let install = ctx.install()?;

    if !live_mode {
        if spotify::is_running()? {
            ui::success(tr!(
                lang,
                "Spotify is already running.",
                "Spotify est déjà ouvert."
            ));
        } else {
            spotify::launch(&install, &[])?;
            ui::success(tr!(lang, "Spotify started.", "Spotify lancé."));
        }
        return Ok(());
    }

    ui::warn(tr!(
        lang,
        "Live mode writes nothing to disk, but Spotify gets a debugging port on 127.0.0.1.\n\
         While it runs, other programs on this computer could control Spotify.",
        "Le mode direct n'écrit rien sur le disque, mais Spotify reçoit un port de débogage sur 127.0.0.1.\n\
         Tant qu'il tourne, d'autres programmes de cet ordinateur pourraient contrôler Spotify."
    ));
    println!();
    let mods = mod_commands::active_mods(ctx)?;
    mod_commands::print_active(lang, &mods);
    println!();
    close_if_running(lang, yes)?;
    ui::hint(tr!(lang, "Starting Spotify…", "Lancement de Spotify…"));
    let payload = Payload::new(&mods).with_gallery(Link::for_payload(&ctx.service).as_ref());
    live::run(&install, &payload, || {
        ui::success(tr!(
            lang,
            "Injected. Keep this window open while you use Spotify.",
            "Injecté. Garde cette fenêtre ouverte pendant que tu utilises Spotify."
        ));
    })?;
    ui::success(tr!(
        lang,
        "Spotify was closed, live session ended.",
        "Spotify a été fermé, fin de la session en direct."
    ));
    Ok(())
}

pub fn state_label(lang: Lang, state: &UiState) -> String {
    match state {
        UiState::Original => tr!(lang, "original", "d'origine"),
        UiState::Patched(_) => tr!(lang, "patched by spacetify", "modifiée par spacetify"),
        UiState::Spicetify => tr!(lang, "modified by Spicetify", "modifiée par Spicetify"),
        UiState::Missing => tr!(lang, "missing", "absente"),
        UiState::Unknown(what) => tr!(
            lang,
            "changed by an unknown tool ({what})",
            "modifiée par un outil inconnu ({what})"
        ),
    }
}

/// Returns whether Spotify was closed, so the caller can start it again.
pub fn close_if_running(lang: Lang, yes: bool) -> Result<bool> {
    if !spotify::is_running()? {
        return Ok(false);
    }
    let question = tr!(
        lang,
        "Spotify is running. Close it now? (y/N)",
        "Spotify est ouvert. Le fermer maintenant ? (o/N)"
    );
    if !yes && !ui::confirm(&question) {
        return Err(Error::SpotifyRunning);
    }
    ui::hint(tr!(lang, "Closing Spotify…", "Fermeture de Spotify…"));
    spotify::close(CLOSE_TIMEOUT)?;
    Ok(true)
}

pub fn restart(lang: Lang, install: &SpotifyInstall) {
    match spotify::launch(install, &[]) {
        Ok(_) => ui::success(tr!(lang, "Spotify restarted.", "Spotify relancé.")),
        Err(err) => ui::error(err.localized(lang)),
    }
}
