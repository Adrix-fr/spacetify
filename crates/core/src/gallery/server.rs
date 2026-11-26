//! The small local server that the gallery page inside Spotify talks to.
//!
//! The page cannot write files, so it asks the background service. The
//! server only listens on 127.0.0.1, and only answers requests that come
//! from Spotify's interface (`Origin`), sent to its own address (`Host`, so
//! another website cannot reach it under another name) and carrying the
//! secret of [`Link`]. Anything else gets an error and nothing is done.
//!
//! ```text
//! GET  /v1/state       the gallery and the installed mods
//! GET  /v1/showcase    ?id=…&file=…: one picture (as a data: URL) or text
//!                      of a mod's showcase, checked against its hash
//! POST /v1/install     {"id"}: download, check, install and turn on
//! POST /v1/enable      {"id"}
//! POST /v1/disable     {"id"}
//! POST /v1/restart     close Spotify, apply the changes, open it again
//! POST /v1/open-submit open the desktop app on its "Submit" page
//! POST /v1/spicetify-theme {"title", "scheme", "colors"}: the theme the
//!                      Spicetify Marketplace showed, imported once after
//!                      leaving Spicetify with spacetify
//! ```
//!
//! Changes are written to `config.toml` at once. Spotify's own file can only
//! be changed while Spotify is closed, so the service applies them the next
//! time Spotify closes, or right away with `restart`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::{Catalog, Link, Source};
use crate::backup::OperationLock;
use crate::config::{AppDirs, Config};
use crate::mods::ModKind;
use crate::service::{self, Request, ServicePaths, Text};
use crate::{Error, Result};

/// Spotify's interface, the only page allowed to talk to the server.
pub const ORIGIN: &str = "https://xpui.app.spotify.com";
const TOKEN_HEADER: &str = "x-spacetify-token";
/// How long the list is kept before it is downloaded again.
const CATALOG_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 16 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);

struct State {
    dirs: AppDirs,
    link: Link,
    source: Source,
    catalog: Mutex<Option<(Instant, Catalog)>>,
    /// Only one change at a time.
    changing: Mutex<()>,
}

/// Starts listening on the link's port, in the background. Fails if the
/// port is taken.
pub fn spawn(dirs: AppDirs, link: Link, source: Source) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, link.port))
        .map_err(Error::io(Link::file(&ServicePaths::new(&dirs))))?;
    let state = Arc::new(State {
        dirs,
        link,
        source,
        catalog: Mutex::new(None),
        changing: Mutex::new(()),
    });
    thread::Builder::new()
        .name("gallery".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = Arc::clone(&state);
                let _ = thread::Builder::new()
                    .name("gallery-request".into())
                    .spawn(move || {
                        if let Err(err) = handle(stream, &state) {
                            tracing::debug!("gallery request: {err}");
                        }
                    });
            }
        })
        .map_err(|err| Error::ServiceStart(err.to_string()))?;
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

struct Reply {
    status: u16,
    cors: bool,
    body: Json,
}

impl Reply {
    fn json(body: Json) -> Self {
        Self {
            status: 200,
            cors: true,
            body,
        }
    }

    fn error(status: u16, text: Text) -> Self {
        Self {
            status,
            cors: true,
            body: json!({ "error": text }),
        }
    }

    /// For requests that do not come from Spotify: no CORS, so the page that
    /// sent it cannot even read the answer.
    fn refused(status: u16) -> Self {
        Self {
            status,
            cors: false,
            body: json!({ "error": "refused" }),
        }
    }
}

fn handle(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let reply = match read_request(&mut stream)? {
        Some(request) => route(&request, state),
        None => Reply::refused(400),
    };
    write_reply(&mut stream, &reply)
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpRequest>> {
    let mut reader = BufReader::new(stream.take((MAX_HEAD + MAX_BODY) as u64));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut request = HttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        headers: BTreeMap::new(),
        body: Vec::new(),
    };
    let mut head = line.len();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        head += line.len();
        if head > MAX_HEAD {
            return Ok(None);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            request
                .headers
                .insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length: usize = request
        .headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return Ok(None);
    }
    request.body.resize(length, 0);
    reader.read_exact(&mut request.body)?;
    Ok(Some(request))
}

fn write_reply(stream: &mut TcpStream, reply: &Reply) -> std::io::Result<()> {
    let body = reply.body.to_string();
    let reason = match reply.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        reply.status,
        if reply.status == 204 { 0 } else { body.len() }
    );
    if reply.cors {
        head.push_str(&format!(
            "Access-Control-Allow-Origin: {ORIGIN}\r\nVary: Origin\r\n\
             Access-Control-Allow-Methods: GET, POST\r\n\
             Access-Control-Allow-Headers: content-type, {TOKEN_HEADER}\r\n\
             Access-Control-Allow-Private-Network: true\r\nAccess-Control-Max-Age: 600\r\n"
        ));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if reply.status != 204 {
        stream.write_all(body.as_bytes())?;
    }
    stream.flush()
}

fn route(request: &HttpRequest, state: &State) -> Reply {
    if let Some(reply) = check(request, &state.link) {
        return reply;
    }
    let body = || serde_json::from_slice::<IdBody>(&request.body).ok();
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/state") => {
            return Reply::json(state_json(state, request.query.contains("refresh=1")));
        }
        ("GET", "/v1/showcase") => {
            let (Some(id), Some(file)) =
                (param(&request.query, "id"), param(&request.query, "file"))
            else {
                return Reply::refused(400);
            };
            return match showcase(state, id, file) {
                Ok(body) => Reply::json(body),
                Err(err) => Reply::error(500, Text::of_error(&err)),
            };
        }
        ("POST", "/v1/install") => body().map(|b| install(state, &b.id)),
        ("POST", "/v1/enable") => body().map(|b| set_on(state, &b.id, true)),
        ("POST", "/v1/disable") => body().map(|b| set_on(state, &b.id, false)),
        ("POST", "/v1/restart") => Some(
            service::send(&ServicePaths::new(&state.dirs), Request::ApplyNow).map(|()| json!({})),
        ),
        ("POST", "/v1/open-submit") => Some(open_submit()),
        ("POST", "/v1/spicetify-theme") => {
            serde_json::from_slice::<MarketplaceTheme>(&request.body)
                .ok()
                .map(|theme| import_marketplace_theme(state, &theme))
        }
        _ => return Reply::refused(404),
    };
    match result {
        None => Reply::refused(400),
        Some(Ok(mut body)) => {
            body["state"] = state_json(state, false);
            Reply::json(body)
        }
        Some(Err(err)) => Reply::error(500, Text::of_error(&err)),
    }
}

/// Opens the desktop app, installed next to the service, on its "Submit"
/// page. If it is open already, it goes to that page.
fn open_submit() -> Result<Json> {
    let missing = || {
        Error::Submission(Text::new(
            "The spacetify app was not found. Open it by hand, then go to Submit.",
            "L'appli spacetify est introuvable. Ouvre-la à la main, puis va sur Proposer.",
        ))
    };
    let exe = std::env::current_exe().map_err(|_| missing())?;
    let app = exe
        .parent()
        .ok_or_else(missing)?
        .join(format!("spacetify-app{}", std::env::consts::EXE_SUFFIX));
    if !app.is_file() {
        return Err(missing());
    }
    std::process::Command::new(&app)
        .args(["--page", "submit"])
        .spawn()
        .map_err(|_| missing())?;
    Ok(json!({}))
}

/// `None` if the request may go on.
fn check(request: &HttpRequest, link: &Link) -> Option<Reply> {
    if request.headers.get("origin").map(String::as_str) != Some(ORIGIN) {
        return Some(Reply::refused(403));
    }
    let host = request.headers.get("host").map(String::as_str);
    let port = link.port;
    if host != Some(&format!("127.0.0.1:{port}")) && host != Some(&format!("localhost:{port}")) {
        return Some(Reply::refused(403));
    }
    if request.method == "OPTIONS" {
        return Some(Reply {
            status: 204,
            cors: true,
            body: Json::Null,
        });
    }
    let token = request.headers.get(TOKEN_HEADER).map(String::as_str);
    if !token.is_some_and(|token| same(token.as_bytes(), link.token.as_bytes())) {
        return Some(Reply::refused(401));
    }
    None
}

/// Compares without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Deserialize)]
struct IdBody {
    id: String,
}

#[derive(Deserialize)]
struct MarketplaceTheme {
    title: String,
    scheme: Option<String>,
    colors: BTreeMap<String, String>,
}

/// Imports the Marketplace theme if leaving Spicetify asked for it, and
/// turns it on, with its CSS to show at once.
fn import_marketplace_theme(state: &State, theme: &MarketplaceTheme) -> Result<Json> {
    if theme.colors.len() > 64 || theme.title.len() > 200 {
        return Err(Error::GalleryInvalid("theme too large".into()));
    }
    let _one = state.changing.lock().unwrap_or_else(|e| e.into_inner());
    let imported = {
        let _lock = OperationLock::acquire(&state.dirs.backups_dir())?;
        crate::spicetify::marketplace_theme(
            &state.dirs,
            &theme.title,
            theme.scheme.as_deref(),
            &theme.colors,
        )?
    };
    let Some(id) = imported else {
        return Ok(json!({ "imported": null }));
    };
    tracing::info!("imported the Spicetify Marketplace theme as {id}");
    let mut answer = change(state, &id, true)?;
    answer["imported"] = json!(id);
    Ok(answer)
}

/// The value of `key` in a query such as `id=nord&file=1.png`. Only plain
/// names are accepted, so nothing needs decoding.
fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, value)| value)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 100
                && !value.starts_with('.')
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
}

/// One file of a mod's showcase: a picture as a `data:` URL, which the page
/// can show without loading anything from the web, or a text.
fn showcase(state: &State, id: &str, file: &str) -> Result<Json> {
    let (catalog, error) = catalog(state, false);
    let Some(catalog) = catalog else {
        return Err(Error::GalleryUnreachable(
            error.map(|text| text.en).unwrap_or_default(),
        ));
    };
    let entry = catalog
        .get(id)
        .ok_or_else(|| Error::NotInGallery(id.to_owned()))?;
    let bytes = super::showcase_file(&state.source, &state.dirs, entry, file)?;
    let invalid = || Error::GalleryFileMismatch {
        id: id.to_owned(),
        file: format!("showcase/{file}"),
    };
    match super::showcase::image_type(file) {
        Some(media_type) => {
            if !super::showcase::is_image_of_type(&bytes, media_type) {
                return Err(invalid());
            }
            let data = format!(
                "data:{media_type};base64,{}",
                crate::mods::theme::base64(&bytes)
            );
            Ok(json!({ "data": data }))
        }
        None => {
            if !super::showcase::is_about_text(&bytes) {
                return Err(invalid());
            }
            Ok(json!({ "text": String::from_utf8_lossy(&bytes) }))
        }
    }
}

/// The list, from memory if recent, else downloaded, else the copy on disk.
fn catalog(state: &State, refresh: bool) -> (Option<Catalog>, Option<Text>) {
    let mut cached = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, catalog)) = cached.as_ref()
        && !refresh
        && at.elapsed() < CATALOG_TTL
    {
        return (Some(catalog.clone()), None);
    }
    match state.source.catalog() {
        Ok(catalog) => {
            super::save_catalog(&state.dirs, &catalog);
            *cached = Some((Instant::now(), catalog.clone()));
            (Some(catalog), None)
        }
        Err(err) => {
            tracing::warn!("gallery list: {err}");
            let fallback = cached
                .as_ref()
                .map(|(_, catalog)| catalog.clone())
                .or_else(|| super::load_saved_catalog(&state.dirs));
            (fallback, Some(Text::of_error(&err)))
        }
    }
}

fn state_json(state: &State, refresh: bool) -> Json {
    let (catalog, error) = catalog(state, refresh);
    let config = Config::load(&state.dirs.config_file());
    let pending = service::read_status(&ServicePaths::new(&state.dirs))
        .ok()
        .flatten()
        .is_some_and(|status| status.apply_pending);
    let mods = config.as_ref().map_err(Text::of_error).and_then(|config| {
        super::views(catalog.as_ref(), &state.source, &state.dirs.mods(), config)
            .map_err(|err| Text::of_error(&err))
    });
    match mods {
        Ok(mods) => json!({
            "version": env!("CARGO_PKG_VERSION"),
            "mods": mods,
            "gallery_error": error,
            "offline_list": error.is_some() && catalog.is_some(),
            "pending": pending,
        }),
        Err(text) => json!({ "version": env!("CARGO_PKG_VERSION"), "mods": [], "error": text }),
    }
}

/// Downloads and installs `id` from the gallery, then turns it on.
fn install(state: &State, id: &str) -> Result<Json> {
    let (catalog, error) = catalog(state, false);
    let Some(catalog) = catalog else {
        return Err(Error::GalleryUnreachable(
            error.map(|text| text.en).unwrap_or_default(),
        ));
    };
    let entry = catalog
        .get(id)
        .ok_or_else(|| Error::NotInGallery(id.to_owned()))?;
    let _one = state.changing.lock().unwrap_or_else(|e| e.into_inner());
    let store = state.dirs.mods();
    let installed = super::install(&state.source, entry, &store)?;
    tracing::info!("gallery: installed {} {}", entry.id, entry.version);
    let log = service::events::EventLog::new(ServicePaths::new(&state.dirs).events_file());
    if let Err(err) = log.append(service::events::EventKind::GalleryInstalled {
        kind: installed.kind,
        id: entry.id.clone(),
        version: entry.version.clone(),
    }) {
        tracing::warn!("{err}");
    }
    change(state, id, true)
}

fn set_on(state: &State, id: &str, on: bool) -> Result<Json> {
    let _one = state.changing.lock().unwrap_or_else(|e| e.into_inner());
    change(state, id, on)
}

/// Turns `id` on or off in `config.toml` and asks the service to apply it
/// when Spotify closes. For a theme, the answer carries the CSS to show
/// right away.
fn change(state: &State, id: &str, on: bool) -> Result<Json> {
    let store = state.dirs.mods();
    let config_file = state.dirs.config_file();
    let kind = {
        let _lock = OperationLock::acquire(&state.dirs.backups_dir())?;
        let mut config = Config::load(&config_file)?;
        let kind = store
            .kind_of(id)
            .ok_or_else(|| Error::ModNotFound(id.to_owned()))?;
        let changed = if on {
            store.enable(&mut config, id)?.1
        } else {
            store.disable(&mut config, id)?
        };
        if changed {
            config.save(&config_file)?;
        }
        kind
    };
    service::send(&ServicePaths::new(&state.dirs), Request::ApplyLater)?;
    let mut answer = json!({ "kind": kind });
    if kind == ModKind::Theme {
        let css = if on {
            store.load_theme(id)?.css
        } else {
            String::new()
        };
        answer["theme_css"] = json!(css);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: "GET".into(),
            path: "/v1/state".into(),
            query: String::new(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body: Vec::new(),
        }
    }

    fn link() -> Link {
        Link {
            port: 4242,
            token: "0123456789abcdef0123456789abcdef".into(),
        }
    }

    #[test]
    fn only_spotify_with_the_secret_gets_in() {
        let ok = [
            ("origin", ORIGIN),
            ("host", "127.0.0.1:4242"),
            (TOKEN_HEADER, "0123456789abcdef0123456789abcdef"),
        ];
        assert!(check(&request(&ok), &link()).is_none());

        let mut other_site = ok;
        other_site[0].1 = "https://example.com";
        let reply = check(&request(&other_site), &link()).unwrap();
        assert_eq!((reply.status, reply.cors), (403, false));

        let mut rebinding = ok;
        rebinding[1].1 = "evil.example:4242";
        assert_eq!(check(&request(&rebinding), &link()).unwrap().status, 403);

        let mut wrong = ok;
        wrong[2].1 = "0123456789abcdef0123456789abcdee";
        assert_eq!(check(&request(&wrong), &link()).unwrap().status, 401);
        assert_eq!(check(&request(&ok[..2]), &link()).unwrap().status, 401);
    }

    #[test]
    fn showcase_parameters_are_plain_names() {
        assert_eq!(param("id=nord&file=1-home.png", "file"), Some("1-home.png"));
        assert_eq!(param("id=nord&file=1-home.png", "id"), Some("nord"));
        assert_eq!(param("id=nord", "file"), None);
        assert_eq!(param("file=..%2Ftheme.css", "file"), None);
        assert_eq!(param("file=../theme.css", "file"), None);
        assert_eq!(param("file=.hidden", "file"), None);
        assert_eq!(param("file=", "file"), None);
    }

    #[test]
    fn the_marketplace_theme_is_imported_only_after_leaving_spicetify() {
        let home = tempfile::tempdir().unwrap();
        let dirs = AppDirs::in_folder(home.path());
        let state = State {
            dirs: dirs.clone(),
            link: link(),
            source: Source::Folder(home.path().join("no-gallery")),
            catalog: Mutex::new(None),
            changing: Mutex::new(()),
        };
        let mut post = request(&[
            ("origin", ORIGIN),
            ("host", "127.0.0.1:4242"),
            (TOKEN_HEADER, "0123456789abcdef0123456789abcdef"),
        ]);
        post.method = "POST".into();
        post.path = "/v1/spicetify-theme".into();
        post.body =
            br##"{"title":"Comfy","scheme":"Mono","colors":{"main":"101010","button":"#1db954"}}"##
                .to_vec();

        let reply = route(&post, &state);
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["imported"], Json::Null);

        crate::spicetify::Marker {
            marketplace_theme_pending: true,
        }
        .save(&dirs)
        .unwrap();
        let reply = route(&post, &state);
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["imported"], "spicetify-comfy-mono");
        assert!(
            reply.body["theme_css"]
                .as_str()
                .unwrap()
                .contains("#1db954")
        );

        post.body = b"not json".to_vec();
        assert_eq!(route(&post, &state).status, 400);
    }

    #[test]
    fn preflight_is_answered_without_the_secret() {
        let mut preflight = request(&[("origin", ORIGIN), ("host", "localhost:4242")]);
        preflight.method = "OPTIONS".into();
        let reply = check(&preflight, &link()).unwrap();
        assert_eq!((reply.status, reply.cors), (204, true));
    }
}
