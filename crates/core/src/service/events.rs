//! What the service did, kept in `events.jsonl` so the CLI and the desktop
//! app can show it. One JSON object per line; old lines are dropped when
//! the file grows.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Text;
use crate::backup::write_atomic;
use crate::mods::ModKind;
use crate::{Error, Lang, Result, tr};

/// The file is trimmed to [`KEEP`] lines once it is larger than this.
const MAX_BYTES: u64 = 256 * 1024;
const KEEP: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// RFC 3339, UTC.
    pub at: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventKind {
    Started {
        version: String,
    },
    Stopped,
    /// The patched interface was replaced by an original one.
    UpdateDetected {
        spotify_version: Option<String>,
    },
    WaitingForSpotify {
        process: String,
    },
    /// A new original interface was backed up and checked.
    BackedUp {
        sha256: String,
        spotify_version: Option<String>,
    },
    /// A mod no longer fits this version of Spotify and was turned off.
    ModTurnedOff {
        kind: ModKind,
        id: String,
        /// Selectors or variables that are gone. 0 when the mod could not be loaded.
        missing: usize,
    },
    Repaired {
        spotify_version: Option<String>,
        theme: Option<String>,
        extensions: usize,
    },
    RepairFailed {
        reason: Text,
    },
    /// Spotify was closed and opened again at the user's request.
    SpotifyRestarted,
    /// Spotify is in a state the service must not touch (Spicetify, …).
    OnHold {
        reason: Text,
    },
    /// A mod was installed from the gallery inside Spotify.
    GalleryInstalled {
        kind: ModKind,
        id: String,
        version: String,
    },
    /// Changes made in the gallery were applied to Spotify.
    ChangesApplied {
        theme: Option<String>,
        extensions: usize,
    },
}

impl EventKind {
    pub fn describe(&self, lang: Lang) -> String {
        let version = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
        match self {
            Self::Started { version } => tr!(
                lang,
                "Service started (spacetify {version}).",
                "Service démarré (spacetify {version})."
            ),
            Self::Stopped => tr!(lang, "Service stopped.", "Service arrêté."),
            Self::UpdateDetected { spotify_version } => tr!(
                lang,
                "Spotify was updated (version {}) and the mods were removed.",
                "Spotify a été mis à jour (version {}) et les mods ont été retirés.",
                version(spotify_version)
            ),
            Self::WaitingForSpotify { process } => tr!(
                lang,
                "Waiting for {process} to close before putting the mods back.",
                "Attente de la fermeture de {process} avant de remettre les mods."
            ),
            Self::BackedUp {
                sha256,
                spotify_version,
            } => tr!(
                lang,
                "New interface backed up and checked (version {}, sha256 {}).",
                "Nouvelle interface sauvegardée et vérifiée (version {}, sha256 {}).",
                version(spotify_version),
                &sha256[..sha256.len().min(16)]
            ),
            Self::ModTurnedOff { kind, id, missing } if *missing == 0 => tr!(
                lang,
                "The {} `{id}` could not be loaded and was turned off.",
                "Le/la {} `{id}` n'a pas pu être chargé(e) et a été désactivé(e).",
                kind.label(lang)
            ),
            Self::ModTurnedOff { kind, id, missing } => tr!(
                lang,
                "The {} `{id}` no longer fits this Spotify ({missing} missing) and was turned off. \
                 `spacetify enable {id}` turns it back on.",
                "Le/la {} `{id}` n'est plus compatible avec ce Spotify ({missing} manquant(s)) et a été désactivé(e). \
                 `spacetify enable {id}` le/la réactive.",
                kind.label(lang)
            ),
            Self::Repaired {
                spotify_version,
                theme,
                extensions,
            } => {
                let theme = theme.clone().unwrap_or_else(|| tr!(lang, "none", "aucun"));
                tr!(
                    lang,
                    "Mods put back for Spotify {} (theme: {theme}, extensions: {extensions}).",
                    "Mods remis en place pour Spotify {} (thème : {theme}, extensions : {extensions}).",
                    version(spotify_version)
                )
            }
            Self::RepairFailed { reason } => tr!(
                lang,
                "Could not put the mods back: {}",
                "Impossible de remettre les mods : {}",
                reason.get(lang)
            ),
            Self::SpotifyRestarted => tr!(
                lang,
                "Spotify was restarted at your request.",
                "Spotify a été relancé à ta demande."
            ),
            Self::OnHold { reason } => reason.get(lang).to_owned(),
            Self::GalleryInstalled { kind, id, version } => tr!(
                lang,
                "The {} `{id}` {version} was installed from the gallery.",
                "Le/la {} `{id}` {version} a été installé(e) depuis la galerie.",
                kind.label(lang)
            ),
            Self::ChangesApplied { theme, extensions } => {
                let theme = theme.clone().unwrap_or_else(|| tr!(lang, "none", "aucun"));
                tr!(
                    lang,
                    "Changes from the gallery applied (theme: {theme}, extensions: {extensions}).",
                    "Changements de la galerie appliqués (thème : {theme}, extensions : {extensions})."
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventLog {
    path: PathBuf,
}

impl EventLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn append(&self, kind: EventKind) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        let event = Event {
            at: crate::backup::now(),
            kind,
        };
        let mut line = serde_json::to_string(&event).expect("events are always serializable");
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(Error::io(&self.path))?;
        file.write_all(line.as_bytes())
            .map_err(Error::io(&self.path))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        drop(file);
        if size > MAX_BYTES {
            self.trim()?;
        }
        Ok(())
    }

    /// The last `count` events, oldest first. Lines that cannot be read
    /// (written by a newer version, say) are skipped.
    pub fn recent(&self, count: usize) -> Result<Vec<Event>> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(Error::io(&self.path)(err)),
        };
        let events: Vec<Event> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let skip = events.len().saturating_sub(count);
        Ok(events.into_iter().skip(skip).collect())
    }

    fn trim(&self) -> Result<()> {
        let text = fs::read_to_string(&self.path).map_err(Error::io(&self.path))?;
        // The newest lines, at most KEEP of them and half the size limit.
        let mut size = 0;
        let mut kept: Vec<&str> = text
            .lines()
            .rev()
            .take(KEEP)
            .take_while(|line| {
                size += line.len() + 1;
                size as u64 <= MAX_BYTES / 2
            })
            .collect();
        kept.reverse();
        let kept = kept.join("\n") + "\n";
        write_atomic(&self.path, kept.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::new(dir.path().join("service/events.jsonl"));
        assert!(log.recent(10).unwrap().is_empty());

        log.append(EventKind::Started {
            version: "0.3.0".into(),
        })
        .unwrap();
        log.append(EventKind::ModTurnedOff {
            kind: ModKind::Theme,
            id: "midnight".into(),
            missing: 2,
        })
        .unwrap();
        log.append(EventKind::Stopped).unwrap();

        let events = log.recent(2).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].kind, EventKind::ModTurnedOff { .. }));
        assert_eq!(events[1].kind, EventKind::Stopped);

        let text = fs::read_to_string(dir.path().join("service/events.jsonl")).unwrap();
        assert!(
            text.contains(r#""event":"mod_turned_off","kind":"theme""#),
            "{text}"
        );
        assert!(events[0].kind.describe(Lang::Fr).contains("midnight"));
    }

    #[test]
    fn skips_unreadable_lines_and_trims() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        fs::write(
            &path,
            "not json\n{\"at\":\"x\",\"event\":\"from_the_future\"}\n",
        )
        .unwrap();
        let log = EventLog::new(&path);
        log.append(EventKind::Stopped).unwrap();
        assert_eq!(log.recent(10).unwrap().len(), 1);

        let reason = Text::new("x".repeat(1000), "y".repeat(1000));
        for _ in 0..200 {
            log.append(EventKind::OnHold {
                reason: reason.clone(),
            })
            .unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= MAX_BYTES);
        let events = log.recent(1000).unwrap();
        assert!(!events.is_empty() && events.len() <= KEEP);
    }
}
