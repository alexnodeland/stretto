//! A Streamable HTTP upstream: MCP's HTTP transport (revision 2025-06-18)
//! on the server's side, stdio on the host's.
//!
//! [`connect`] gives the proxy the two ends a stdio server has: a writer for
//! lines to the server, and a reader for lines from it. Behind them, each
//! JSON-RPC message the proxy writes is POSTed to the server's endpoint, and
//! what the server answers comes back as lines: one JSON body, or each event
//! of a Server-Sent Events stream. The session id the server assigns when it
//! answers `initialize` (`Mcp-Session-Id`), and the protocol version it
//! agreed (`MCP-Protocol-Version`), go with every later request. Once the
//! client has said `notifications/initialized`, a GET stream carries the
//! messages the server sends on its own, and is opened again if it ends.
//! When the proxy's side closes, requests in flight finish, and the session
//! is ended with a DELETE.
//!
//! A request the server refuses, or cannot be reached for, is answered with
//! a JSON-RPC error, so the host is never left waiting. The values of
//! [`Upstream::headers`] are sent and never logged.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, PipeReader, PipeWriter, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const SESSION: &str = "mcp-session-id";
const PROTOCOL: &str = "mcp-protocol-version";
/// Consecutive failures after which the GET stream is given up.
const STREAM_RETRIES: u32 = 5;

/// A Streamable HTTP server to proxy for.
#[derive(Clone, Debug, Default)]
pub struct Upstream {
    /// The MCP endpoint, such as `https://example.com/mcp`.
    pub url: String,
    /// Headers sent with every request, such as `Authorization`. Their
    /// values are never logged.
    pub headers: Vec<(String, String)>,
}

/// The upstream, connected: the ends the proxy writes to and reads from,
/// and the thread to wait for once the proxy's end is closed.
pub struct Connection {
    /// Lines to the server.
    pub to_server: PipeWriter,
    /// Lines from the server.
    pub from_server: PipeReader,
    /// Ends after the session does.
    pub done: JoinHandle<()>,
}

/// A line for the host, or the end of them.
enum Out {
    Line(Vec<u8>),
    Close,
}

#[derive(Default)]
struct Session {
    id: Option<String>,
    protocol: Option<String>,
    /// The last event id the GET stream carried, to resume it.
    last_event: Option<String>,
}

struct Shared {
    upstream: Upstream,
    client: reqwest::blocking::Client,
    session: Mutex<Session>,
    closing: AtomicBool,
}

impl Shared {
    /// `request` with the session's headers and the configured ones.
    fn with_headers(
        &self,
        mut request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        let session = self.session.lock().expect("session lock");
        if let Some(id) = &session.id {
            request = request.header(SESSION, id);
        }
        if let Some(version) = &session.protocol {
            request = request.header(PROTOCOL, version);
        }
        for (name, value) in &self.upstream.headers {
            request = request.header(name, value);
        }
        request
    }
}

/// Connect to `upstream`. Nothing is sent until the proxy writes its first
/// line.
pub fn connect(upstream: Upstream) -> Result<Connection> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(None)
        .build()
        .context("building the HTTP client")?;
    let (from_proxy, to_server) = std::io::pipe().context("creating a pipe")?;
    let (from_server, to_proxy) = std::io::pipe().context("creating a pipe")?;
    let (tx, rx) = mpsc::channel::<Out>();
    thread::Builder::new()
        .name("stretto-proxy http writer".into())
        .spawn(move || write_lines(to_proxy, rx))
        .context("starting the HTTP writer thread")?;
    let shared = Arc::new(Shared {
        upstream,
        client,
        session: Mutex::new(Session::default()),
        closing: AtomicBool::new(false),
    });
    let done = thread::Builder::new()
        .name("stretto-proxy http".into())
        .spawn(move || outbound(from_proxy, &shared, &tx))
        .context("starting the HTTP thread")?;
    Ok(Connection {
        to_server,
        from_server,
        done,
    })
}

/// Hand each line to the proxy, until told to close.
fn write_lines(mut to_proxy: PipeWriter, rx: mpsc::Receiver<Out>) {
    for out in rx {
        match out {
            Out::Line(mut line) => {
                if line.last() != Some(&b'\n') {
                    line.push(b'\n');
                }
                if to_proxy
                    .write_all(&line)
                    .and_then(|()| to_proxy.flush())
                    .is_err()
                {
                    return;
                }
            }
            Out::Close => return,
        }
    }
}

/// POST each line the proxy writes, until its end closes; then end the
/// session.
fn outbound(from_proxy: PipeReader, shared: &Arc<Shared>, tx: &Sender<Out>) {
    let mut reader = BufReader::new(from_proxy);
    let mut in_flight: Vec<JoinHandle<()>> = Vec::new();
    let mut listening = false;
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let body: Vec<u8> = line.trim_ascii_end().to_vec();
        if body.is_empty() {
            continue;
        }
        let method = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|m| m.get("method").and_then(Value::as_str).map(str::to_string));
        match method.as_deref() {
            // The session's id and version come from the answer to
            // `initialize`, and the stream opens once the client has said it
            // is ready, so these two go one at a time.
            Some("initialize") => post(shared, tx, body),
            Some("notifications/initialized") => {
                post(shared, tx, body);
                if !listening {
                    listening = true;
                    let (shared, tx) = (shared.clone(), tx.clone());
                    let _ = thread::Builder::new()
                        .name("stretto-proxy http stream".into())
                        .spawn(move || listen(&shared, &tx));
                }
            }
            _ => {
                in_flight.retain(|h| !h.is_finished());
                let (shared, tx) = (shared.clone(), tx.clone());
                if let Ok(h) = thread::Builder::new()
                    .name("stretto-proxy http request".into())
                    .spawn(move || post(&shared, &tx, body))
                {
                    in_flight.push(h);
                }
            }
        }
    }
    for h in in_flight {
        let _ = h.join();
    }
    shared.closing.store(true, Ordering::SeqCst);
    let id = shared.session.lock().expect("session lock").id.clone();
    if id.is_some() {
        let request = shared
            .with_headers(shared.client.delete(&shared.upstream.url))
            .timeout(Duration::from_secs(5));
        if let Err(e) = request.send() {
            eprintln!("stretto-proxy: ending the HTTP session: {e}");
        }
    }
    let _ = tx.send(Out::Close);
}

/// A JSON-RPC error for request `id`, if the message was a request.
fn refuse(tx: &Sender<Out>, id: Option<&Value>, why: &str) {
    if let Some(id) = id {
        let error = json!({"jsonrpc": "2.0", "id": id,
            "error": {"code": -32000, "message": format!("stretto-proxy: {why}")}});
        let _ = tx.send(Out::Line(error.to_string().into_bytes()));
    }
}

/// POST one message, and hand the host what the server answers.
fn post(shared: &Shared, tx: &Sender<Out>, body: Vec<u8>) {
    let message: Option<Value> = serde_json::from_slice(&body).ok();
    let request_id = message
        .as_ref()
        .filter(|m| m.get("method").is_some())
        .and_then(|m| m.get("id"))
        .filter(|id| !id.is_null())
        .cloned();
    let initialize = message
        .as_ref()
        .and_then(|m| m.get("method"))
        .and_then(Value::as_str)
        == Some("initialize");
    let request = shared.with_headers(
        shared
            .client
            .post(&shared.upstream.url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(body),
    );
    let response = match request.send() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("stretto-proxy: the HTTP server could not be reached: {e}");
            return refuse(tx, request_id.as_ref(), "the server could not be reached");
        }
    };
    if initialize {
        if let Some(id) = response
            .headers()
            .get(SESSION)
            .and_then(|v| v.to_str().ok())
        {
            shared.session.lock().expect("session lock").id = Some(id.to_string());
        }
    }
    let status = response.status();
    if status.as_u16() == 202 {
        return;
    }
    if !status.is_success() {
        let gone =
            status.as_u16() == 404 && shared.session.lock().expect("session lock").id.is_some();
        let why = if gone {
            "the server has ended the session".to_string()
        } else {
            format!("the server answered HTTP {status}")
        };
        eprintln!("stretto-proxy: {why}");
        return refuse(tx, request_id.as_ref(), &why);
    }
    let events = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.to_ascii_lowercase().starts_with("text/event-stream"));
    let hand = |data: &str| {
        let line = match serde_json::from_str::<Value>(data) {
            Ok(value) => {
                if initialize {
                    agree(shared, &value);
                }
                value.to_string()
            }
            Err(_) => data.replace('\n', " "),
        };
        let _ = tx.send(Out::Line(line.into_bytes()));
    };
    if events {
        read_events(response, |_, data| hand(data));
    } else {
        let mut text = String::new();
        if let Err(e) = BufReader::new(response).read_to_string(&mut text) {
            eprintln!("stretto-proxy: reading the HTTP server's answer: {e}");
            return refuse(tx, request_id.as_ref(), "the server's answer was cut off");
        }
        if !text.trim().is_empty() {
            hand(text.trim());
        }
    }
}

/// Keep the protocol version the server agreed to in its answer to
/// `initialize`.
fn agree(shared: &Shared, answer: &Value) {
    if let Some(version) = answer
        .get("result")
        .and_then(|r| r.get("protocolVersion"))
        .and_then(Value::as_str)
    {
        shared.session.lock().expect("session lock").protocol = Some(version.to_string());
    }
}

/// Read the server's own stream (GET), and open it again when it ends,
/// until the session closes or the server says it offers none.
fn listen(shared: &Shared, tx: &Sender<Out>) {
    let mut failures = 0;
    while !shared.closing.load(Ordering::SeqCst) && failures < STREAM_RETRIES {
        let mut request = shared.with_headers(
            shared
                .client
                .get(&shared.upstream.url)
                .header("accept", "text/event-stream"),
        );
        let last = shared
            .session
            .lock()
            .expect("session lock")
            .last_event
            .clone();
        if let Some(last) = last {
            request = request.header("last-event-id", last);
        }
        match request.send() {
            // The server offers no stream of its own.
            Ok(r) if r.status().as_u16() == 405 => return,
            Ok(r) if r.status().is_success() => {
                failures = 0;
                read_events(r, |id, data| {
                    if let Some(id) = id {
                        shared.session.lock().expect("session lock").last_event =
                            Some(id.to_string());
                    }
                    let line = serde_json::from_str::<Value>(data)
                        .map_or_else(|_| data.replace('\n', " "), |v| v.to_string());
                    let _ = tx.send(Out::Line(line.into_bytes()));
                });
            }
            Ok(r) if r.status().as_u16() == 404 => return,
            Ok(_) | Err(_) => failures += 1,
        }
        if !shared.closing.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(500 * u64::from(failures.max(1))));
        }
    }
}

/// Read a Server-Sent Events stream, calling `each` with every event's id
/// (if it has one) and data. An event is dispatched at the blank line that
/// ends it; one cut off by the end of the stream is dispatched too.
pub(crate) fn read_events(stream: impl Read, mut each: impl FnMut(Option<&str>, &str)) {
    let mut data = String::new();
    let mut id: Option<String> = None;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            if !data.is_empty() {
                each(id.as_deref(), data.trim_end_matches('\n'));
            }
            data.clear();
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_str(), ""),
        };
        match field {
            "data" => {
                data.push_str(value);
                data.push('\n');
            }
            "id" => id = Some(value.to_string()),
            _ => {}
        }
    }
    if !data.is_empty() {
        each(id.as_deref(), data.trim_end_matches('\n'));
    }
}

/// `url` for the log: without a user and password, and with the values of
/// query parameters that look like credentials hidden.
pub(crate) fn redact_url(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let (query, fragment) = query.split_once('#').unwrap_or((query, ""));
    let mut out = if scheme.is_empty() {
        format!("{host}{path}")
    } else {
        format!("{scheme}://{host}{path}")
    };
    if !query.is_empty() {
        let params: Vec<String> = query
            .split('&')
            .map(|p| match p.split_once('=') {
                Some((name, _)) if crate::record::names_secret(name) => {
                    format!("{name}={}", crate::record::REDACTED)
                }
                _ => p.to_string(),
            })
            .collect();
        out = format!("{out}?{}", params.join("&"));
    }
    if !fragment.is_empty() {
        out = format!("{out}#{fragment}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_server_sent_events() {
        let stream = b": a comment\n\
            event: message\nid: 7\ndata: {\"a\":1}\n\n\
            data: {\"b\":\ndata: 2}\n\n\
            data: cut off";
        let mut got: Vec<(Option<String>, String)> = Vec::new();
        read_events(&stream[..], |id, data| {
            got.push((id.map(str::to_string), data.to_string()))
        });
        assert_eq!(
            got,
            [
                (Some("7".to_string()), "{\"a\":1}".to_string()),
                (Some("7".to_string()), "{\"b\":\n2}".to_string()),
                (Some("7".to_string()), "cut off".to_string()),
            ]
        );
    }

    #[test]
    fn hides_credentials_in_the_url() {
        assert_eq!(
            redact_url("https://me:pa55@example.com:8443/mcp?token=abc&region=eu#x"),
            "https://example.com:8443/mcp?token=<redacted>&region=eu#x"
        );
        assert_eq!(
            redact_url("http://127.0.0.1:9/mcp"),
            "http://127.0.0.1:9/mcp"
        );
        assert_eq!(redact_url("https://example.com"), "https://example.com");
    }
}
