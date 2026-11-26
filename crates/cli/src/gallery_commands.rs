//! `spacetify gallery …`: checking a mod before sending it to the gallery,
//! and the steps the robot on GitHub runs on a request.
//!
//! The robot's steps write their result as JSON (`--json`), in both
//! languages, so it can answer the author whatever happens.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use spacetify_core::gallery::review::{self, Level, Report};
use spacetify_core::gallery::submission::{self, Form};
use spacetify_core::mods;
use spacetify_core::service::Text;
use spacetify_core::{Error, Lang, Result, tr};

use crate::args::GalleryAction;
use crate::commands::Context;
use crate::ui;

pub fn run(ctx: &Context, action: GalleryAction) -> Result<()> {
    match action {
        GalleryAction::Review {
            path,
            repo,
            antivirus,
            json,
            markdown,
            link_base,
        } => review_cmd(
            ctx.lang,
            &path,
            repo.as_deref(),
            antivirus.as_deref(),
            json.as_deref(),
            markdown.as_deref(),
            link_base.as_deref(),
        ),
        GalleryAction::Pack { path, into } => pack(ctx.lang, &path, into),
        GalleryAction::Form { body, json } => with_json(json.as_deref(), || {
            let text = read(&body)?;
            let form = Form::parse(&text)?;
            Ok(serde_json::to_value(&form).expect("form is serializable"))
        }),
        GalleryAction::Unpack {
            archive,
            into,
            folder,
            json,
        } => with_json(json.as_deref(), || {
            let dir = submission::unpack(&archive, &into, folder.as_deref())?;
            Ok(json!({ "dir": dir }))
        }),
        GalleryAction::Add {
            path,
            repo,
            owner,
            form,
            json,
        } => with_json(json.as_deref(), || {
            let form = Form::parse(&read(&form)?)?;
            let added = submission::add(&path, &repo, &owner, &form)?;
            Ok(serde_json::to_value(&added).expect("result is serializable"))
        }),
    }
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| Error::Io {
        path: path.to_path_buf(),
        source: err,
    })
}

fn write(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|err| Error::Io {
        path: path.to_path_buf(),
        source: err,
    })
}

/// Runs `job`. With `--json`, writes `{"ok": true, …}` or
/// `{"ok": false, "error": {"en", "fr"}}` and succeeds either way, so the
/// robot can tell the author what went wrong. Without it, prints.
fn with_json(path: Option<&Path>, job: impl FnOnce() -> Result<Json>) -> Result<()> {
    let result = job();
    let Some(path) = path else {
        let value = result?;
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("json is printable")
        );
        return Ok(());
    };
    let value = match result {
        Ok(Json::Object(mut map)) => {
            map.insert("ok".into(), Json::Bool(true));
            Json::Object(map)
        }
        Ok(other) => json!({ "ok": true, "value": other }),
        Err(err) => json!({ "ok": false, "error": Text::of_error(&err) }),
    };
    write(
        path,
        &serde_json::to_string_pretty(&value).expect("json is printable"),
    )
}

fn review_cmd(
    lang: Lang,
    dir: &Path,
    repo: Option<&Path>,
    antivirus: Option<&Path>,
    json: Option<&Path>,
    markdown: Option<&Path>,
    link_base: Option<&str>,
) -> Result<()> {
    // The version already in the gallery, to show what changed.
    let previous = match (repo, mods::load_dir(dir)) {
        (Some(repo), Ok(loaded)) => submission::accepted_folder(repo, &loaded.meta().id)?,
        _ => None,
    };
    let mut report = review::review(dir, previous.as_deref());
    if let Some(antivirus) = antivirus {
        report.add_antivirus(&read(antivirus)?, dir);
    }
    if let Some(path) = json {
        let text = serde_json::to_string_pretty(&report).expect("report is serializable");
        write(path, &text)?;
    }
    if let Some(path) = markdown {
        let id = report.id.clone().unwrap_or_default();
        let link = |file: &str, line: Option<usize>| match link_base {
            Some(base) => {
                let mut url = base.replace("{id}", &id);
                if !url.ends_with('/') {
                    url.push('/');
                }
                url.push_str(&encode_path(file));
                if let Some(line) = line {
                    url.push_str(&format!("#L{line}"));
                }
                url
            }
            None => "#".into(),
        };
        let mut text = report.markdown(Lang::En, &link);
        text.push_str("\n<details><summary>Français</summary>\n\n");
        text.push_str(&report.markdown(Lang::Fr, &link));
        text.push_str("\n</details>\n");
        write(path, &text)?;
    }
    if json.is_none() && markdown.is_none() {
        print_report(lang, &report);
    }
    Ok(())
}

/// A path for a URL: each part percent-encoded, `/` kept.
fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn print_report(lang: Lang, report: &Report) {
    ui::title(&tr!(lang, "Security review", "Contrôle de sécurité"));
    if let (Some(name), Some(version)) = (&report.name, &report.version) {
        ui::row(tr!(lang, "Mod", "Mod"), format!("{name} {version}"));
    }
    ui::row(
        tr!(lang, "Files", "Fichiers"),
        report.files.len().to_string(),
    );
    println!();
    for check in review::Check::ALL {
        let Some(level) = report.level_of(check) else {
            continue;
        };
        let title = check.title().get(lang).to_owned();
        match level {
            Level::Ok => ui::success(&title),
            Level::Look => ui::warn(&title),
            Level::Block => ui::error(&title),
        }
        for finding in report.findings.iter().filter(|f| f.check == check) {
            let place = match (&finding.file, finding.line) {
                (Some(file), Some(line)) => format!(" ({file}:{line})"),
                (Some(file), None) => format!(" ({file})"),
                _ => String::new(),
            };
            println!("    {}{place}", finding.what.get(lang));
            if let Some(excerpt) = &finding.excerpt {
                println!("      {excerpt}");
            }
        }
    }
    println!();
    let summary = report.summary();
    if report.blocking > 0 {
        ui::error(summary.get(lang));
    } else {
        ui::success(summary.get(lang));
    }
    ui::hint(tr!(
        lang,
        "The antivirus runs on GitHub, when the mod is sent.",
        "L'antivirus passe sur GitHub, à l'envoi du mod."
    ));
}

fn pack(lang: Lang, dir: &Path, into: Option<PathBuf>) -> Result<()> {
    let report = review::review(dir, None);
    if report.blocking > 0 {
        print_report(lang, &report);
        return Err(Error::Submission(Text::new(
            "Fix what is blocking first: the gallery would refuse the mod.",
            "Corrige d'abord ce qui bloque : la galerie refuserait le mod.",
        )));
    }
    let into = match into {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|err| Error::Io {
            path: PathBuf::from("."),
            source: err,
        })?,
    };
    let zip = submission::pack(dir, &into)?;
    let loaded = mods::load_dir(dir).map_err(|issue| Error::InvalidMod {
        what: dir.display().to_string(),
        issue,
    })?;
    ui::success(tr!(
        lang,
        "Ready to send: {}",
        "Prêt à envoyer : {}",
        zip.display()
    ));
    ui::hint(tr!(
        lang,
        "Open this form, drop the .zip into it and send it:",
        "Ouvre ce formulaire, glisses-y le .zip et envoie-le :"
    ));
    println!("{}", submission::form_url(&loaded, false));
    Ok(())
}
