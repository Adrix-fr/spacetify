//! Live injection through Chromium's remote debugging protocol.
//!
//! Nothing on disk changes, so there is nothing to repair after an update.
//! The trade-offs: Spotify must be started by us with a debugging port, and
//! this process has to stay attached so the script comes back after a reload.
//! The port only listens on 127.0.0.1, but any local program can use it while
//! Spotify runs, which is why this is opt-in.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use tungstenite::Message;

use super::payload;
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
pub fn run(install: &SpotifyInstall, on_ready: impl FnOnce()) -> Result<()> {
    if spotify::is_running()? {
        return Err(Error::SpotifyRunning);
    }
    let port = free_port()?;
    spotify::launch(install, &[format!("--remote-debugging-port={port}")])?;

    let ws_url = wait_for_ui(port)?;
    let (mut socket, _) = tungstenite::connect(ws_url.as_str()).map_err(live_err)?;
    let script = payload::live_script();

    let commands = [
        (1, "Page.enable", json!({})),
        (
            2,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        ),
        (3, "Runtime.evaluate", json!({ "expression": script })),
    ];
    for (id, method, params) in &commands {
        let message = json!({ "id": id, "method": method, "params": params });
        socket
            .send(Message::text(message.to_string()))
            .map_err(live_err)?;
    }

    let mut pending: Vec<u64> = vec![1, 2, 3];
    while !pending.is_empty() {
        let message = socket.read().map_err(live_err)?;
        let Ok(reply) = serde_json::from_str::<Value>(message.to_text().unwrap_or_default()) else {
            continue;
        };
        let Some(id) = reply["id"].as_u64() else {
            continue;
        };
        if let Some(error) = reply
            .get("error")
            .or_else(|| reply["result"].get("exceptionDetails"))
        {
            return Err(Error::Live(error.to_string()));
        }
        pending.retain(|&p| p != id);
    }
    tracing::info!("payload injected through port {port}");
    on_ready();

    // Keep the session open: the injected script survives reloads only while we are attached.
    while socket.read().is_ok() {}
    Ok(())
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
