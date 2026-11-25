//! What the window shows, built from the core's types. Texts are in the
//! user's language here, next to the CLI's own wording, so both front ends
//! say the same thing.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value as Json;
use spacetify_core::detect::UiState;
use spacetify_core::doctor::{Report, Status, Unverifiable, Verdict};
use spacetify_core::mods::{SettingKind, SettingValue, Theme};
use spacetify_core::service::events::{Event, EventKind};
use spacetify_core::service::{self, ServiceStatus};
use spacetify_core::{Lang, tr};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyView {
    pub found: bool,
    /// Why it was not found, or could not be read.
    pub error: Option<String>,
    pub path: Option<String>,
    pub version: Option<String>,
    pub kind: Option<String>,
    pub read_only: bool,
    /// `original`, `patched`, `spicetify`, `missing` or `unknown`.
    pub state: Option<&'static str>,
    pub state_label: Option<String>,
    pub running: bool,
    pub spicetify_leftovers: Option<String>,
    /// An update replaced the patched interface (and no service fixed it yet).
    pub update_replaced: bool,
    pub store_also_installed: bool,
    pub backups: usize,
}

pub fn state_code(state: &UiState) -> &'static str {
    match state {
        UiState::Original => "original",
        UiState::Patched(_) => "patched",
        UiState::Spicetify => "spicetify",
        UiState::Missing => "missing",
        UiState::Unknown(_) => "unknown",
    }
}

pub fn state_label(lang: Lang, state: &UiState) -> String {
    match state {
        UiState::Original => tr!(lang, "Original", "D'origine"),
        UiState::Patched(_) => tr!(lang, "Modified by spacetify", "Modifié par spacetify"),
        UiState::Spicetify => tr!(lang, "Modified by Spicetify", "Modifié par Spicetify"),
        UiState::Missing => tr!(lang, "Interface missing", "Interface absente"),
        UiState::Unknown(_) => tr!(
            lang,
            "Changed by another tool",
            "Modifié par un autre outil"
        ),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub running: bool,
    /// `on`, `off`, `disabled` (turned off in the Task Manager) or `error`.
    pub autostart: &'static str,
    pub autostart_detail: Option<String>,
    pub phase: Option<String>,
    pub started: Option<String>,
    pub last_repair: Option<String>,
    pub installed: bool,
}

pub fn service_view(
    lang: Lang,
    running: bool,
    autostart: Result<spacetify_core::service::autostart::Autostart, String>,
    status: Option<&ServiceStatus>,
    installed: bool,
) -> ServiceView {
    use spacetify_core::service::autostart::Autostart;
    let (autostart, autostart_detail) = match autostart {
        Ok(Autostart::On { .. }) => ("on", None),
        Ok(Autostart::Off) => ("off", None),
        Ok(Autostart::DisabledBySystem { .. }) => (
            "disabled",
            Some(tr!(
                lang,
                "Turned off in Task Manager > Startup apps.",
                "Désactivé dans Gestionnaire des tâches > Applications de démarrage."
            )),
        ),
        Err(err) => ("error", Some(err)),
    };
    ServiceView {
        running,
        autostart,
        autostart_detail,
        phase: status
            .filter(|_| running)
            .map(|status| capitalize(&status.phase.describe(lang))),
        started: status
            .filter(|_| running)
            .map(|status| service::time_ago(&status.started_at, lang)),
        last_repair: status
            .and_then(|status| status.last_repair.as_deref())
            .map(|at| service::time_ago(at, lang)),
        installed,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EventView {
    pub at: String,
    pub when: String,
    pub text: String,
    /// `good`, `bad`, `warn` or `info`, for the colour of the dot.
    pub tone: &'static str,
}

pub fn event_view(lang: Lang, event: &Event) -> EventView {
    let tone = match &event.kind {
        EventKind::Repaired { .. } | EventKind::BackedUp { .. } => "good",
        EventKind::RepairFailed { .. } => "bad",
        EventKind::UpdateDetected { .. }
        | EventKind::ModTurnedOff { .. }
        | EventKind::OnHold { .. }
        | EventKind::WaitingForSpotify { .. } => "warn",
        _ => "info",
    };
    EventView {
        at: event.at.clone(),
        when: service::time_ago(&event.at, lang),
        text: event.kind.describe(lang),
        tone,
    }
}

/// Colours used to draw a small preview of a theme.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    pub frame: String,
    pub background: String,
    pub highlight: String,
    pub elevated: String,
    pub text: String,
    pub accent: String,
    pub swatches: Vec<Swatch>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Swatch {
    pub label: String,
    pub color: String,
}

/// Spotify's own look, for themes that leave a colour out and for "no theme".
pub fn spotify_palette(lang: Lang) -> Palette {
    palette(lang, &BTreeMap::new())
}

pub fn theme_palette(lang: Lang, theme: &Theme) -> Palette {
    palette(lang, &theme.colors)
}

fn palette(lang: Lang, colors: &BTreeMap<String, BTreeMap<String, String>>) -> Palette {
    let get = |set: &str, token: &str| colors.get(set).and_then(|tokens| tokens.get(token));
    let pick = |choices: &[(&str, &str)], fallback: &str| {
        choices
            .iter()
            .find_map(|(set, token)| get(set, token))
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    };
    let mut swatches = Vec::new();
    for (set, tokens) in colors {
        for (token, color) in tokens {
            if swatches.iter().any(|s: &Swatch| &s.color == color) {
                continue;
            }
            swatches.push(Swatch {
                label: token_label(lang, set, token),
                color: color.clone(),
            });
        }
    }
    Palette {
        frame: pick(&[("app-frame", "background-base")], "#000000"),
        background: pick(&[("base", "background-base")], "#121212"),
        highlight: pick(&[("base", "background-highlight")], "#1f1f1f"),
        elevated: pick(
            &[
                ("base", "background-elevated-base"),
                ("base", "background-highlight"),
            ],
            "#2a2a2a",
        ),
        text: pick(&[("base", "text-subdued")], "#b3b3b3"),
        accent: pick(
            &[
                ("bright-accent", "background-base"),
                ("base", "essential-bright-accent"),
                ("base", "text-bright-accent"),
            ],
            "#1ed760",
        ),
        swatches,
    }
}

fn token_label(lang: Lang, set: &str, token: &str) -> String {
    let known = match (set, token) {
        ("base", "background-base") => Some(tr!(lang, "Background", "Fond")),
        ("base", "background-highlight") => Some(tr!(lang, "Hover", "Survol")),
        ("base", "background-press") => Some(tr!(lang, "Pressed", "Appui")),
        ("base", "background-elevated-base") => Some(tr!(lang, "Panels", "Panneaux")),
        ("base", "text-base") => Some(tr!(lang, "Text", "Texte")),
        ("base", "text-subdued") => Some(tr!(lang, "Secondary text", "Texte secondaire")),
        ("base", "text-bright-accent") => Some(tr!(lang, "Accent text", "Texte d'accent")),
        ("base", "essential-bright-accent") => Some(tr!(lang, "Accent", "Accent")),
        ("base", "decorative-subdued") => Some(tr!(lang, "Separators", "Séparateurs")),
        ("app-frame", "background-base") => Some(tr!(lang, "Frame", "Cadre")),
        ("bright-accent", "background-base") => Some(tr!(lang, "Buttons", "Boutons")),
        ("bright-accent", "background-highlight") => {
            Some(tr!(lang, "Button hover", "Bouton survolé"))
        }
        ("bright-accent", "background-press") => Some(tr!(lang, "Button pressed", "Bouton appuyé")),
        _ => None,
    };
    known.unwrap_or_else(|| format!("{set} · {token}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingView {
    pub name: String,
    pub label: String,
    /// `boolean`, `number`, `string` or `choice`.
    pub kind: &'static str,
    pub value: Json,
    pub default: Json,
    pub choices: Vec<String>,
    pub custom: bool,
}

pub fn setting_view(value: SettingValue) -> SettingView {
    SettingView {
        label: value
            .setting
            .label
            .clone()
            .unwrap_or_else(|| value.name.clone()),
        kind: match value.setting.kind {
            SettingKind::Boolean => "boolean",
            SettingKind::Number => "number",
            SettingKind::String => "string",
            SettingKind::Choice => "choice",
        },
        default: value.setting.default.clone(),
        choices: value.setting.choices.clone(),
        custom: value.custom,
        value: value.value,
        name: value.name,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorView {
    pub spotify_version: Option<String>,
    /// `nothing`, `compatible`, `warnings`, `broken` or `invalid`.
    pub verdict: &'static str,
    pub summary: String,
    pub mods: Vec<ModCheckView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModCheckView {
    pub kind: String,
    pub title: String,
    pub verdict: &'static str,
    pub headline: String,
    pub error: Option<String>,
    pub problems: Vec<ProblemView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProblemView {
    /// `missing`, `fragile` or `unknown`.
    pub level: &'static str,
    pub text: String,
}

fn verdict_code(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Compatible => "compatible",
        Verdict::Warnings => "warnings",
        Verdict::Broken => "broken",
        Verdict::Invalid => "invalid",
    }
}

pub fn doctor_view(lang: Lang, report: &Report) -> DoctorView {
    if report.mods.is_empty() {
        return DoctorView {
            spotify_version: report.spotify_version.clone(),
            verdict: "nothing",
            summary: tr!(
                lang,
                "No theme or extension is turned on, so there is nothing to check.",
                "Aucun thème ni extension n'est activé, il n'y a donc rien à vérifier."
            ),
            mods: Vec::new(),
        };
    }
    let mods = report
        .mods
        .iter()
        .map(|item| {
            let found = item.count(|s| s == Status::Found);
            let missing = item.count(|s| s == Status::Missing);
            let headline = match item.verdict() {
                Verdict::Compatible if item.checks.is_empty() => tr!(
                    lang,
                    "Relies on nothing specific in Spotify",
                    "Ne dépend de rien de particulier dans Spotify"
                ),
                Verdict::Compatible if found == 1 => {
                    tr!(lang, "1 element checked, present", "1 élément vérifié, présent")
                }
                Verdict::Compatible => tr!(
                    lang,
                    "{found} elements checked, all present",
                    "{found} éléments vérifiés, tous présents"
                ),
                Verdict::Warnings => tr!(
                    lang,
                    "Works today, could break with a future Spotify update",
                    "Fonctionne, mais pourrait casser à une prochaine mise à jour de Spotify"
                ),
                Verdict::Broken if missing == 1 => tr!(
                    lang,
                    "1 element it uses is gone from this Spotify, part of it will not show",
                    "1 élément utilisé n'existe plus dans ce Spotify, une partie ne s'affichera pas"
                ),
                Verdict::Broken => tr!(
                    lang,
                    "{missing} elements it uses are gone from this Spotify, part of it will not show",
                    "{missing} éléments utilisés n'existent plus dans ce Spotify, une partie ne s'affichera pas"
                ),
                Verdict::Invalid => tr!(
                    lang,
                    "Cannot be loaded, left out",
                    "Impossible à charger, laissé de côté"
                ),
            };
            let problems = item
                .checks
                .iter()
                .filter_map(|check| {
                    let target = check.target.css();
                    let (level, text) = match check.status {
                        Status::Found => return None,
                        Status::Missing => (
                            "missing",
                            tr!(lang, "{target} not found", "{target} introuvable"),
                        ),
                        Status::Fragile => (
                            "fragile",
                            tr!(
                                lang,
                                "{target} exists, but its name is generated and may change with the next update",
                                "{target} existe, mais son nom est généré et peut changer à la prochaine mise à jour"
                            ),
                        ),
                        Status::Unverifiable(Unverifiable::Translated) => (
                            "unknown",
                            tr!(
                                lang,
                                "{target} cannot be checked: the text depends on Spotify's language",
                                "{target} impossible à vérifier : le texte dépend de la langue de Spotify"
                            ),
                        ),
                        Status::Unverifiable(Unverifiable::SetAtRuntime) => (
                            "unknown",
                            tr!(
                                lang,
                                "{target} cannot be checked without running Spotify",
                                "{target} impossible à vérifier sans lancer Spotify"
                            ),
                        ),
                    };
                    Some(ProblemView { level, text })
                })
                .collect();
            ModCheckView {
                kind: capitalize(item.kind.label(lang)),
                title: item.title.clone().unwrap_or_else(|| item.id.clone()),
                verdict: verdict_code(item.verdict()),
                headline,
                error: item.error.as_ref().map(|err| err.localized(lang)),
                problems,
            }
        })
        .collect();
    let summary = match report.verdict() {
        Verdict::Compatible => tr!(
            lang,
            "Everything works with this version of Spotify.",
            "Tout fonctionne avec cette version de Spotify."
        ),
        Verdict::Warnings => tr!(
            lang,
            "Nothing is broken. The warnings point to what could break with a future update.",
            "Rien n'est cassé. Les avertissements montrent ce qui pourrait casser avec une prochaine mise à jour."
        ),
        Verdict::Broken | Verdict::Invalid => tr!(
            lang,
            "Some mods no longer fit this version of Spotify. They still apply, but part of them will not show.",
            "Certains mods ne sont plus adaptés à cette version de Spotify. Ils s'appliquent quand même, mais une partie ne s'affichera pas."
        ),
    };
    DoctorView {
        spotify_version: report.spotify_version.clone(),
        verdict: verdict_code(report.verdict()),
        summary,
        mods,
    }
}

pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Splits what was typed in the Terminal tab into arguments. Quotes group
/// words; a leading `spacetify` is dropped, since that is the program run.
pub fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    if args.first().is_some_and(|first| {
        first.eq_ignore_ascii_case("spacetify") || first.eq_ignore_ascii_case("spacetify.exe")
    }) {
        args.remove(0);
    }
    args
}

#[cfg(test)]
mod tests {
    use spacetify_core::doctor::{Check, ModReport, Target};
    use spacetify_core::service::Text;

    use spacetify_core::mods::ModKind;

    use super::*;

    #[test]
    fn splits_typed_commands() {
        assert_eq!(split_args("spacetify status"), ["status"]);
        assert_eq!(
            split_args(r#"  install "C:\My Mods\neon"  "#),
            ["install", r"C:\My Mods\neon"]
        );
        assert_eq!(
            split_args("service events -n 5"),
            ["service", "events", "-n", "5"]
        );
        assert_eq!(split_args(r#"theme """#), ["theme", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn palette_falls_back_to_spotify_colours() {
        let spotify = spotify_palette(Lang::En);
        assert_eq!(spotify.background, "#121212");
        assert!(spotify.swatches.is_empty());

        let colors = BTreeMap::from([
            (
                "base".to_owned(),
                BTreeMap::from([
                    ("background-base".to_owned(), "#0f1020".to_owned()),
                    ("essential-bright-accent".to_owned(), "#a78bfa".to_owned()),
                    ("text-positive".to_owned(), "#a78bfa".to_owned()),
                ]),
            ),
            (
                "app-frame".to_owned(),
                BTreeMap::from([("background-base".to_owned(), "#07070f".to_owned())]),
            ),
        ]);
        let theme = palette(Lang::Fr, &colors);
        assert_eq!(theme.background, "#0f1020");
        assert_eq!(theme.frame, "#07070f");
        assert_eq!(theme.accent, "#a78bfa");
        assert_eq!(theme.highlight, "#1f1f1f");
        // Same colour twice is shown once.
        assert_eq!(theme.swatches.len(), 3);
        assert!(theme.swatches.iter().any(|s| s.label == "Cadre"));
    }

    #[test]
    fn doctor_explains_each_mod() {
        let report = Report {
            spotify_version: Some("1.3.10".into()),
            mods: vec![
                ModReport {
                    kind: ModKind::Theme,
                    id: "midnight".into(),
                    title: Some("Midnight 1.0.0".into()),
                    checks: vec![Check {
                        target: Target::Variable("--background-base".into()),
                        status: Status::Found,
                    }],
                    error: None,
                },
                ModReport {
                    kind: ModKind::Extension,
                    id: "badge".into(),
                    title: None,
                    checks: vec![
                        Check {
                            target: Target::TestId("now-playing-bar".into()),
                            status: Status::Missing,
                        },
                        Check {
                            target: Target::Class("Xk3a".into()),
                            status: Status::Fragile,
                        },
                    ],
                    error: None,
                },
            ],
        };
        let view = doctor_view(Lang::Fr, &report);
        assert_eq!(view.verdict, "broken");
        assert_eq!(view.mods[0].verdict, "compatible");
        assert_eq!(view.mods[0].kind, "Thème");
        assert_eq!(view.mods[1].title, "badge");
        assert_eq!(view.mods[1].problems.len(), 2);
        assert_eq!(view.mods[1].problems[0].level, "missing");
        assert!(view.mods[1].problems[0].text.contains("now-playing-bar"));

        let empty = Report {
            spotify_version: None,
            mods: vec![],
        };
        assert_eq!(doctor_view(Lang::En, &empty).verdict, "nothing");
    }

    #[test]
    fn events_get_a_tone() {
        let event = |kind| Event {
            at: "2026-11-25T10:00:00Z".into(),
            kind,
        };
        let failed = event_view(
            Lang::En,
            &event(EventKind::RepairFailed {
                reason: Text::new("disk full", "disque plein"),
            }),
        );
        assert_eq!(failed.tone, "bad");
        assert!(failed.text.contains("disk full"));
        assert_eq!(
            event_view(Lang::En, &event(EventKind::Stopped)).tone,
            "info"
        );
    }
}
