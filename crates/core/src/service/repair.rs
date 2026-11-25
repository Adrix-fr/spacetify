//! Putting the mods back on a new version of Spotify.

use std::path::Path;

use crate::backup::{BackupStore, OperationLock};
use crate::config::Config;
use crate::detect::{self, UiState};
use crate::doctor::{self, Verdict};
use crate::mods::{ModKind, ModStore};
use crate::patcher::{self, Payload};
use crate::spotify::SpotifyInstall;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnedOff {
    pub kind: ModKind,
    pub id: String,
    /// What the mod relies on and is gone. 0 when it could not be loaded.
    pub missing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repaired {
    /// Hash of the new original, if this is the first time it is backed up.
    pub new_backup: Option<String>,
    pub turned_off: Vec<TurnedOff>,
    pub theme: Option<String>,
    pub extensions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Repaired(Repaired),
    /// Nothing to do any more: someone applied or restored in the meantime.
    NotNeeded,
}

/// Backs up the current (new) interface, turns off the mods that no longer
/// fit it, then applies the others.
///
/// `idle` must fail with [`Error::SpotifyRunning`] while Spotify or its
/// updater runs. It is asked again right before writing.
pub(crate) fn run(
    install: &SpotifyInstall,
    store: &BackupStore,
    mods: &ModStore,
    config_file: &Path,
    idle: impl Fn() -> Result<()>,
) -> Result<Outcome> {
    let _lock = OperationLock::acquire(store.root())?;
    install.ensure_modifiable()?;
    // Checked again under the lock: `restore` may have run in the meantime.
    if store.load_applied()?.is_none() {
        return Ok(Outcome::NotNeeded);
    }
    match detect::inspect(install)?.state {
        UiState::Original => {}
        UiState::Patched(_) => return Ok(Outcome::NotNeeded),
        other => return Err(patcher::refusal(install, other)),
    }
    idle()?;

    // A verified copy of the new interface, before anything is written.
    // It is what `restore` will put back from now on.
    let (backup, created) = store.create(&install.xpui_spa(), install.version.clone())?;

    let mut config = Config::load(config_file)?;
    let report = doctor::run(install, mods, &config)?;
    let mut turned_off = Vec::new();
    for found in &report.mods {
        if matches!(found.verdict(), Verdict::Broken | Verdict::Invalid) {
            mods.disable(&mut config, &found.id)?;
            turned_off.push(TurnedOff {
                kind: found.kind,
                id: found.id.clone(),
                missing: found.count(|s| s == doctor::Status::Missing),
            });
        }
    }
    if !turned_off.is_empty() {
        config.save(config_file)?;
    }

    let active = mods.active(&config)?;
    idle()?;
    patcher::apply_unchecked(install, store, &Payload::new(&active))?;
    Ok(Outcome::Repaired(Repaired {
        new_backup: created.then_some(backup.sha256),
        turned_off,
        theme: active
            .theme
            .as_ref()
            .map(|theme| theme.manifest.meta.name.clone()),
        extensions: active.extensions.len(),
    }))
}

/// For [`run`]: fails while `busy` names a running program.
pub(crate) fn idle_unless(busy: Result<Option<String>>) -> Result<()> {
    match busy? {
        Some(_) => Err(Error::SpotifyRunning),
        None => Ok(()),
    }
}
