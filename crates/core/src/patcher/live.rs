//! Live injection through Chromium's remote debugging protocol.
//!
//! Nothing on disk changes, so there is nothing to repair after an update.
//! The trade-offs: Spotify must be started by us with a debugging port, and
//! this process has to stay attached so the script comes back after a reload.
//! The port only listens on 127.0.0.1, but any local program can use it while
//! Spotify runs, which is why this is opt-in.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use tungstenite::Message;
use tungstenite::stream::MaybeTlsStream;

use super::Payload;
use crate::spotify::{self, SpotifyInstall};
use crate::{Error, Result};

const UI_URL_PREFIX: &str = "https://xpui.app.spotify.com/";
/// Generous, so there is time to log in on a fresh install.
const UI_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    #[serde(rename = "type")]
    kind: String,
    url: String,
    web_socket_debugger_url: Option<String>,
}

/// Starts Spotify, injects the payload and stays attached until Spotify exits.
///
/// `on_ready` is called once the payload is in place.
pub fn run(install: &SpotifyInstall, payload: &Payload, on_ready: impl FnOnce()) -> Result<()> {
    let mut session = Session::start(install)?;
    session.inject(payload, true)?;
    tracing::info!("payload injected");
    on_ready();

    // Keep the session open: the injected script survives reloads only while we are attached.
    while session.socket.read().is_ok() {}
    Ok(())
}

/// Like [`run`], for mod authors: each payload received on `updates`
/// replaces the previous one and reloads Spotify's interface, without
/// restarting Spotify.
///
/// The interface is reloaded once at the start too, so mods applied to the
/// file give way to the live ones. `on_reload` is called after each reload
/// asked for, with the number of reloads so far (0 for the first one).
pub fn run_dev(
    install: &SpotifyInstall,
    payload: &Payload,
    updates: &Receiver<Payload>,
    mut on_reload: impl FnMut(usize),
) -> Result<()> {
    let mut session = Session::start(install)?;
    session.inject(payload, false)?;
    session.reload()?;
    on_reload(0);
    session.set_read_timeout(Some(POLL))?;

    let mut reloads = 0;
    loop {
        match session.socket.read() {
            Ok(_) => continue,
            Err(err) if is_timeout(&err) => {}
            // Spotify was closed.
            Err(_) => return Ok(()),
        }
        // Only the last of several quick saves matters.
        let Some(next) = updates.try_iter().last() else {
            continue;
        };
        session.inject(&next, false)?;
        session.reload()?;
        reloads += 1;
        on_reload(reloads);
    }
}

/// How often a dev session looks for new mods while Spotify is quiet.
const POLL: Duration = Duration::from_millis(250);
/// How long Spotify has to answer one command.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

type Socket = tungstenite::WebSocket<MaybeTlsStream<TcpStream>>;

/// A connection to Spotify's interface through its debugging port.
struct Session {
    socket: Socket,
    next_id: u64,
    /// Scripts registered for future page loads, to take them back.
    registered: Vec<String>,
}

impl Session {
    fn start(install: &SpotifyInstall) -> Result<Self> {
        if spotify::is_running()? {
            return Err(Error::SpotifyRunning);
        }
        let port = free_port()?;
        spotify::launch(install, &[format!("--remote-debugging-port={port}")])?;
        let ws_url = wait_for_ui(port)?;
        let (socket, _) = tungstenite::connect(ws_url.as_str()).map_err(live_err)?;
        tracing::info!("attached to Spotify through port {port}");
        let mut session = Self {
            socket,
            next_id: 0,
            registered: Vec::new(),
        };
        session.call("Page.enable", json!({}))?;
        Ok(session)
    }

    /// Sends a command and waits for its answer, skipping Spotify's events.
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "id": id, "method": method, "params": params });
        self.socket
            .send(Message::text(message.to_string()))
            .map_err(live_err)?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let message = match self.socket.read() {
                Ok(message) => message,
                Err(err) if is_timeout(&err) && Instant::now() < deadline => continue,
                Err(err) => return Err(live_err(err)),
            };
            let Ok(reply) = serde_json::from_str::<Value>(message.to_text().unwrap_or_default())
            else {
                continue;
            };
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                return Err(Error::Live(error.to_string()));
            }
            return Ok(reply["result"].clone());
        }
    }

    /// Registers each script for future page loads (Spotify reloads its
    /// interface sometimes), replacing the previous ones, and with `run_now`
    /// runs it once in the page that is already open.
    fn inject(&mut self, payload: &Payload, run_now: bool) -> Result<()> {
        for identifier in std::mem::take(&mut self.registered) {
            self.call(
                "Page.removeScriptToEvaluateOnNewDocument",
                json!({ "identifier": identifier }),
            )?;
        }
        for (method, params) in commands(&payload.live_scripts(), run_now) {
            let result = self.call(method, params)?;
            if let Some(identifier) = result["identifier"].as_str() {
                self.registered.push(identifier.to_owned());
            }
            // An extension that throws is the runtime's business: it is logged
            // in Spotify's console and must not stop the other mods.
            if let Some(exception) = result.get("exceptionDetails") {
                tracing::warn!("a mod script threw while starting: {exception}");
            }
        }
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        self.call("Page.reload", json!({})).map(drop)
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> Result<()> {
        if let MaybeTlsStream::Plain(stream) = self.socket.get_mut() {
            stream.set_read_timeout(timeout).map_err(live_err)?;
        }
        Ok(())
    }
}

fn is_timeout(err: &tungstenite::Error) -> bool {
    matches!(err, tungstenite::Error::Io(io) if matches!(io.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

/// Each script is registered for future page loads and, with `run_now`, run
/// once in the page that is already open.
fn commands(scripts: &[String], run_now: bool) -> Vec<(&'static str, Value)> {
    let mut commands = Vec::new();
    for script in scripts {
        commands.push((
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        ));
        if run_now {
            commands.push(("Runtime.evaluate", json!({ "expression": script })));
        }
    }
    commands
}

/// Asks the OS for a free port, so two sessions never collide.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(live_err)?;
    Ok(listener.local_addr().map_err(live_err)?.port())
}

fn wait_for_ui(port: u16) -> Result<String> {
    let deadline = Instant::now() + UI_TIMEOUT;
    loop {
        let found = http_get(port, "/json/list")
            .ok()
            .and_then(|body| serde_json::from_str::<Vec<Target>>(&body).ok())
            .and_then(|targets| {
                targets
                    .into_iter()
                    .find(|t| t.kind == "page" && t.url.starts_with(UI_URL_PREFIX))
            })
            .and_then(|target| target.web_socket_debugger_url);
        if let Some(url) = found {
            return Ok(url);
        }
        if Instant::now() > deadline {
            return Err(Error::Live(
                "Spotify's interface did not show up in time".into(),
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Minimal HTTP GET.
///
/// The DevTools server keeps the connection open after responding, so the
/// body is read up to `Content-Length` instead of until the socket closes.
fn http_get(port: u16, path: &str) -> io::Result<String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;

    let mut reader = BufReader::new(stream);
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let length = content_length.ok_or(io::ErrorKind::InvalidData)?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    String::from_utf8(body).map_err(|_| io::ErrorKind::InvalidData.into())
}

fn live_err(err: impl std::fmt::Display) -> Error {
    Error::Live(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_response_from_a_server_that_keeps_the_connection_open() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 512];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK
Content-Length:2

[]",
                )
                .unwrap();
            // Like Chromium: answer, then keep the socket open for a while.
            thread::sleep(Duration::from_secs(5));
        });

        let started = Instant::now();
        assert_eq!(http_get(port, "/json/list").unwrap(), "[]");
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(server);
    }

    #[test]
    fn registers_every_script_and_runs_it_only_when_asked() {
        let scripts = ["a".to_owned(), "b".to_owned()];
        let now = commands(&scripts, true);
        let methods: Vec<&str> = now.iter().map(|(method, _)| *method).collect();
        assert_eq!(
            methods,
            [
                "Page.addScriptToEvaluateOnNewDocument",
                "Runtime.evaluate",
                "Page.addScriptToEvaluateOnNewDocument",
                "Runtime.evaluate"
            ]
        );
        assert_eq!(now[3].1["expression"], "b");
        let later = commands(&scripts, false);
        assert_eq!(later.len(), 2);
        assert_eq!(later[1].1["source"], "b");
    }

    #[test]
    fn tells_a_quiet_socket_from_a_closed_one() {
        let quiet = tungstenite::Error::Io(io::ErrorKind::WouldBlock.into());
        assert!(is_timeout(&quiet));
        let quiet = tungstenite::Error::Io(io::ErrorKind::TimedOut.into());
        assert!(is_timeout(&quiet));
        assert!(!is_timeout(&tungstenite::Error::ConnectionClosed));
    }

    #[test]
    fn picks_a_usable_port() {
        assert_ne!(free_port().unwrap(), 0);
    }

    #[test]
    fn parses_devtools_targets() {
        let body = r#"[{"type":"page","url":"https://xpui.app.spotify.com/index.html",
            "webSocketDebuggerUrl":"ws://127.0.0.1:9/devtools/page/A"},
            {"type":"browser_ui","url":"chrome://omnibox-popup.top-chrome/"}]"#;
        let targets: Vec<Target> = serde_json::from_str(body).unwrap();
        let ui = targets
            .iter()
            .find(|t| t.url.starts_with(UI_URL_PREFIX))
            .unwrap();
        assert_eq!(ui.kind, "page");
        assert_eq!(
            ui.web_socket_debugger_url.as_deref(),
            Some("ws://127.0.0.1:9/devtools/page/A")
        );
    }
}
