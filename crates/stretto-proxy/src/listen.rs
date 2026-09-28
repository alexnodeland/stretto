//! Hosts over Streamable HTTP: MCP's HTTP transport (revision 2025-11-25,
//! and the 2025-06-18 and 2025-03-26 revisions it extends) on the host's
//! side.
//!
//! [`listen`] serves one MCP endpoint, [`PATH`], in place of stdio. A host
//! POSTs `initialize` to start a session, and the answer names the session
//! (`Mcp-Session-Id`) for every later request. Each session gets a server of
//! its own (the command, or a session of `--upstream`'s), a recording of its
//! own and its own run of [`Active`], exactly as one stdio proxy does: the
//! host's messages reach the session as the lines a stdio host would have
//! written, and each line the proxy would have written back goes to the
//! host's HTTP request it belongs to.
//!
//! - **A POST** carries one message, or a batch of them (2025-03-26). A
//!   notification or a response is accepted with 202. A request is answered
//!   on an event stream (`text/event-stream`), which carries what the server
//!   sends meanwhile and ends with the answer; a host that does not accept
//!   one gets the answer as one JSON body.
//! - **A GET** opens the session's stream for the server's own requests and
//!   notifications, and a new one replaces the one before. With none open,
//!   they go on the event stream of a request still waiting (the one whose
//!   progress token a progress notification names, else the newest), or wait
//!   for a stream to open, [`MAX_QUEUED`] at most.
//! - **A DELETE** ends the session: the server's input closes, as when a
//!   stdio host exits.
//!
//! A session also ends when its server exits, when the proxy stops, and
//! after [`Listen::idle`] with no request and no open stream. A request
//! still waiting then gets a JSON-RPC error, and a request that names the
//! session gets 404, which tells the host to start another.
//!
//! **Who may connect.** With [`Listen::token`], every request needs
//! `Authorization: Bearer <token>`. Without one, the proxy listens only on
//! a loopback address and answers only requests whose Host header names its
//! own loopback address and port, and whose Origin, if they have one, is a
//! page on this machine's loopback. Pages on other sites, including one
//! whose name resolves to 127.0.0.1 (DNS rebinding), get 403.
//!
//! **The host session.** The `Stretto-Session` header on `initialize` names
//! it, as `STRETTO_SESSION` does for a stdio proxy: the log header's
//! `host_session`, and `{session}` in `--context`, `--flow-log` and
//! `--confirm-log` ([`crate::session_path`]).

use crate::{serve, Active, Config};
use anyhow::{bail, Context, Result};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use serde_json::value::RawValue;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::future::Future;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// The MCP endpoint's path.
pub const PATH: &str = "/mcp";
/// The header that names the host session on `initialize`.
pub const HOST_SESSION: &str = "stretto-session";
/// The server's own messages kept for a stream to open, at most.
pub const MAX_QUEUED: usize = 1000;
/// Sessions open at once, at most.
pub const MAX_SESSIONS: usize = 64;
/// A POST's body, at most.
pub const MAX_BODY: usize = 4 << 20;

const SESSION: &str = "mcp-session-id";
const PROTOCOL: &str = "mcp-protocol-version";
/// How often an event stream says it is still open, so that a stream
/// whose host has gone is noticed.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// How [`listen`] serves hosts.
#[derive(Clone, Debug)]
pub struct Listen {
    /// The token each request must carry, as `Authorization: Bearer
    /// <token>`. Without one, only loopback (see [the module](self)).
    pub token: Option<String>,
    /// End a session after this long with no request and no open stream.
    pub idle: Duration,
    /// Prune these directories as each session starts, keeping this many
    /// days ([`crate::retain`]); the proxy runs for longer than one session.
    pub retain: Option<(Vec<PathBuf>, u64)>,
    /// Once stopping, how long to wait for the sessions' servers to exit
    /// and their recordings to close.
    pub grace: Duration,
}

impl Default for Listen {
    fn default() -> Self {
        Self {
            token: None,
            idle: Duration::from_secs(4 * 3600),
            retain: None,
            grace: Duration::from_secs(10),
        }
    }
}

/// Serve hosts at `addr` until the process is interrupted (Ctrl-C, or
/// SIGTERM on Unix), and return the status to exit with. The endpoint's
/// URL ([`url`]) goes to stdout, on a line of its own.
///
/// `active` is shared by every session, each with its own run of it.
pub fn listen(
    addr: SocketAddr,
    config: &Config,
    active: Option<&'static Active>,
    options: &Listen,
) -> Result<i32> {
    admissible(addr, options)?;
    let listener = TcpListener::bind(addr).with_context(|| format!("listening on {addr}"))?;
    let url = url(listener.local_addr().context("reading the address")?);
    println!("{url}");
    eprintln!("stretto-proxy: serving hosts over Streamable HTTP at {url}");
    listen_on(listener, config, active, options, stopped())
}

/// [`listen`] on `listener`, until `stop` completes.
pub fn listen_on<F>(
    listener: TcpListener,
    config: &Config,
    active: Option<&'static Active>,
    options: &Listen,
    stop: F,
) -> Result<i32>
where
    F: Future<Output = ()> + Send + 'static,
{
    let local = listener.local_addr().context("reading the address")?;
    admissible(local, options)?;
    listener
        .set_nonblocking(true)
        .context("readying the listener")?;
    let app = Arc::new(App {
        config: config.clone(),
        active,
        options: options.clone(),
        port: local.port(),
        sessions: Mutex::new(HashMap::new()),
        opened: AtomicU64::new(0),
        threads: Mutex::new(Vec::new()),
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the HTTP server")?;
    let served = runtime.block_on(serve_hosts(app.clone(), listener, stop));
    app.finish();
    served.map(|()| 0)
}

/// The endpoint's URL at `addr`, with loopback named for an unspecified
/// address (`0.0.0.0` or `::`).
pub fn url(addr: SocketAddr) -> String {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}{PATH}", SocketAddr::new(ip, addr.port()))
}

/// Refuse to serve anything but loopback without a token.
fn admissible(addr: SocketAddr, options: &Listen) -> Result<()> {
    if options.token.is_none() && !addr.ip().is_loopback() {
        bail!(
            "listening on {addr} needs a token (--listen-token-file): without one, \
             stretto-proxy listens only on a loopback address, such as 127.0.0.1:8931"
        );
    }
    Ok(())
}

/// Until Ctrl-C, or SIGTERM on Unix.
async fn stopped() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("listening for SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

async fn serve_hosts<F>(app: Arc<App>, listener: TcpListener, stop: F) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let listener = tokio::net::TcpListener::from_std(listener).context("serving")?;
    let sweeper = tokio::spawn(sweep(app.clone()));
    let router = Router::new()
        .route(PATH, post(message).get(stream).delete(end))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(app.clone());
    let served = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            stop.await;
            // Their event streams end with them, which lets the server stop.
            app.close_all("stretto-proxy is stopping");
        })
        .await;
    sweeper.abort();
    served.context("serving hosts")
}

/// End the sessions that have been idle too long, now and then.
async fn sweep(app: Arc<App>) {
    let every = (app.options.idle / 4).clamp(Duration::from_millis(10), Duration::from_secs(60));
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        app.reap();
    }
}

/// The proxy's state across sessions.
struct App {
    config: Config,
    active: Option<&'static Active>,
    options: Listen,
    /// The port it listens on, which a Host header must name.
    port: u16,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Sessions opened so far.
    opened: AtomicU64,
    /// Each session's thread, until it ends.
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl App {
    /// Whether a request may be served: its token, else its Host and
    /// Origin; and a protocol version it names must look like one.
    fn check(&self, headers: &HeaderMap) -> Result<(), Response> {
        let text = |name: HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
        match &self.options.token {
            Some(token) => {
                let given = text(header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer "));
                if !given.is_some_and(|g| same(g.trim().as_bytes(), token.as_bytes())) {
                    let mut refused = refuse(
                        StatusCode::UNAUTHORIZED,
                        "send Authorization: Bearer <token>, with the token stretto-proxy was \
                         given (--listen-token-file)",
                    );
                    refused
                        .headers_mut()
                        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                    return Err(refused);
                }
            }
            None => {
                if !text(header::HOST).is_some_and(|h| loopback_host(h, self.port)) {
                    return Err(refuse(
                        StatusCode::FORBIDDEN,
                        "the Host header must be localhost, 127.0.0.1 or [::1] with the proxy's \
                         port: without a token, stretto-proxy answers only its own address",
                    ));
                }
                if headers.contains_key(header::ORIGIN)
                    && !text(header::ORIGIN).is_some_and(loopback_origin)
                {
                    return Err(refuse(
                        StatusCode::FORBIDDEN,
                        "a page on another site may not use stretto-proxy: without a token, \
                         only pages on this machine's loopback may",
                    ));
                }
            }
        }
        if !headers
            .get(PROTOCOL)
            .is_none_or(|v| v.to_str().is_ok_and(revision))
        {
            return Err(refuse(
                StatusCode::BAD_REQUEST,
                "MCP-Protocol-Version must name a revision, such as 2025-11-25",
            ));
        }
        Ok(())
    }

    /// The session a request names.
    fn session(&self, headers: &HeaderMap) -> Result<Arc<Session>, Response> {
        let Some(id) = headers.get(SESSION).and_then(|v| v.to_str().ok()) else {
            return Err(refuse(
                StatusCode::BAD_REQUEST,
                "no Mcp-Session-Id: start a session with initialize",
            ));
        };
        lock(&self.sessions)
            .get(id.trim())
            .cloned()
            .ok_or_else(gone)
    }

    /// Open a session for an `initialize` request, with a stream for its
    /// answer (`json`: as one body) before the session's proxy starts, so a
    /// server that cannot start answers it too.
    fn open(
        self: &Arc<Self>,
        headers: &HeaderMap,
        requests: &[Request],
        json: bool,
    ) -> Result<(Arc<Session>, UnboundedReceiver<String>), Response> {
        let named = match headers.get(HOST_SESSION).map(HeaderValue::to_str) {
            None => None,
            Some(Ok(name)) if host_session(name) => Some(name.to_string()),
            Some(_) => {
                return Err(refuse(
                    StatusCode::BAD_REQUEST,
                    "Stretto-Session names the host session: at most 128 letters, digits, \
                     and . _ : -",
                ))
            }
        };
        let mut sessions = lock(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            return Err(refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "stretto-proxy has as many sessions as it keeps: end one (DELETE) first",
            ));
        }
        let n = self.opened.fetch_add(1, Ordering::Relaxed) + 1;
        let (to_core, from_host) = mpsc::channel();
        let session = Arc::new(Session::new(new_id(), n, to_core));
        let answers = lock(&session.routes).expect(requests, json);
        sessions.insert(session.id.clone(), session.clone());
        drop(sessions);
        if let Some((dirs, days)) = &self.options.retain {
            crate::retain(dirs, *days);
        }
        let mut config = self.config.clone();
        eprintln!(
            "stretto-proxy: session {n} opened{}",
            named
                .as_deref()
                .map(|h| format!(", for host session {h}"))
                .unwrap_or_default()
        );
        config.host_session = named;
        let (app, active, own) = (self.clone(), self.active, session.clone());
        let out = Out {
            session: session.clone(),
            line: Vec::new(),
        };
        let thread = thread::Builder::new()
            .name(format!("stretto-proxy session {n}"))
            .spawn(move || {
                let why = match serve(&config, active, Lines::new(from_host), out, Some(n)) {
                    Ok(status) => format!("its server exited with status {status}"),
                    Err(e) => format!("{e:#}"),
                };
                app.close(&own, &why);
            })
            .expect("starting a session's thread");
        let mut threads = lock(&self.threads);
        threads.retain(|t| !t.is_finished());
        threads.push(thread);
        Ok((session, answers))
    }

    /// End `session`, if it has not ended.
    fn close(&self, session: &Session, why: &str) {
        lock(&self.sessions).remove(&session.id);
        if session.end(why) {
            eprintln!("stretto-proxy: session {} ended: {why}", session.n);
        }
    }

    fn close_all(&self, why: &str) {
        let all: Vec<Arc<Session>> = lock(&self.sessions).values().cloned().collect();
        for session in all {
            self.close(&session, why);
        }
    }

    /// End the sessions idle for [`Listen::idle`].
    fn reap(&self) {
        let idle: Vec<Arc<Session>> = lock(&self.sessions)
            .values()
            .filter(|s| s.idle_for() >= self.options.idle)
            .cloned()
            .collect();
        for session in idle {
            self.close(
                &session,
                &format!("no request and no open stream for {:?}", self.options.idle),
            );
        }
    }

    /// End every session, and wait up to [`Listen::grace`] for their
    /// threads, whose recordings close as they end.
    fn finish(&self) {
        self.close_all("stretto-proxy is stopping");
        let threads = std::mem::take(&mut *lock(&self.threads));
        let deadline = Instant::now() + self.options.grace;
        while threads.iter().any(|t| !t.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let left = threads.iter().filter(|t| !t.is_finished()).count();
        if left > 0 {
            eprintln!(
                "stretto-proxy: {left} sessions' servers were still running after {:?}; \
                 leaving them",
                self.options.grace
            );
        }
    }
}

/// POST: a message, or a batch, from the host.
async fn message(State(app): State<Arc<App>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(refused) = app.check(&headers) {
        return refused;
    }
    if !json_body(&headers) {
        return refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "a message needs Content-Type: application/json",
        );
    }
    let (messages, batch) = match messages(&body) {
        Ok(parsed) => parsed,
        Err(refused) => return refused,
    };
    let opening = messages.iter().any(|m| m.initialize);
    let requests: Vec<Request> = messages.iter().filter_map(|m| m.request.clone()).collect();
    let lines = messages.into_iter().map(|m| m.line);
    let events = accepts(&headers, "text/event-stream");
    let expected = if opening {
        if batch || headers.contains_key(SESSION) {
            return refuse(
                StatusCode::BAD_REQUEST,
                "initialize comes alone, and without Mcp-Session-Id: it starts a session",
            );
        }
        app.open(&headers, &requests, !events)
    } else {
        let session = match app.session(&headers) {
            Ok(session) => session,
            Err(refused) => return refused,
        };
        if requests.is_empty() {
            session.touch();
            return accepted(&session, lines);
        }
        session
            .expect(&requests, !events)
            .map(|answers| (session, answers))
    };
    let (session, answers) = match expected {
        Ok(expected) => expected,
        Err(refused) => return refused,
    };
    session.touch();
    // Should the session end first, it answers what it expected itself.
    let _ = session.send(lines);
    let mut response = if events {
        event_stream(answers)
    } else {
        one_body(answers, batch).await
    };
    if opening {
        response.headers_mut().insert(
            HeaderName::from_static(SESSION),
            HeaderValue::from_str(&session.id).expect("a session id is visible ASCII"),
        );
    }
    response
}

/// GET: the stream for the server's own messages.
async fn stream(State(app): State<Arc<App>>, method: Method, headers: HeaderMap) -> Response {
    // A HEAD would take the host's stream away, and read nothing from it.
    if method == Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if let Err(refused) = app.check(&headers) {
        return refused;
    }
    if !accepts(&headers, "text/event-stream") {
        return refuse(
            StatusCode::NOT_ACCEPTABLE,
            "a GET opens an event stream: send Accept: text/event-stream",
        );
    }
    let session = match app.session(&headers) {
        Ok(session) => session,
        Err(refused) => return refused,
    };
    session.touch();
    events_for(&session)
}

/// DELETE: the host is done with the session.
async fn end(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if let Err(refused) = app.check(&headers) {
        return refused;
    }
    match app.session(&headers) {
        Ok(session) => {
            app.close(&session, "its host ended it");
            StatusCode::OK.into_response()
        }
        Err(refused) => refused,
    }
}

/// 202 for messages that want no answer, once passed on to the session.
fn accepted(session: &Session, lines: impl IntoIterator<Item = Vec<u8>>) -> Response {
    if session.send(lines) {
        StatusCode::ACCEPTED.into_response()
    } else {
        gone()
    }
}

/// The session's stream for the server's own messages.
fn events_for(session: &Session) -> Response {
    match session.listen() {
        Some(messages) => event_stream(messages),
        None => gone(),
    }
}

/// An event stream of `messages`, one event each, which ends when they do.
fn event_stream(messages: UnboundedReceiver<String>) -> Response {
    let events = futures_util::stream::unfold(messages, |mut messages| async move {
        let message = messages.recv().await?;
        Some((
            Ok::<_, Infallible>(Event::default().data(message)),
            messages,
        ))
    });
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response()
}

/// The answers as one JSON body: the one answer, or an array for a batch.
async fn one_body(mut answers: UnboundedReceiver<String>, batch: bool) -> Response {
    let mut all = Vec::new();
    while let Some(answer) = answers.recv().await {
        all.push(answer);
    }
    let body = if batch {
        format!("[{}]", all.join(","))
    } else {
        all.concat()
    };
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// An HTTP error whose body is a JSON-RPC error with no id: an invalid
/// request for 400, a server error otherwise.
fn refuse(status: StatusCode, message: &str) -> Response {
    let code = if status == StatusCode::BAD_REQUEST {
        -32600
    } else {
        -32000
    };
    refuse_with(status, code, message)
}

fn refuse_with(status: StatusCode, code: i64, message: &str) -> Response {
    let body = json!({"jsonrpc": "2.0", "id": null,
                      "error": {"code": code, "message": message}});
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// 404: the session ended, or never was.
fn gone() -> Response {
    refuse(
        StatusCode::NOT_FOUND,
        "no such session: it ended, or never was; start another with initialize",
    )
}

/// A request's id, as JSON text, and its progress token, if it has one.
type Request = (String, Option<String>);

/// A message from the host.
struct Message {
    /// Its text as the host sent it, on one line, ending in a newline.
    line: Vec<u8>,
    /// For a request, its id and progress token.
    request: Option<Request>,
    initialize: bool,
}

/// A POST's messages, and whether they came as a batch.
fn messages(body: &[u8]) -> Result<(Vec<Message>, bool), Response> {
    // A parse error.
    let bad = |message: String| refuse_with(StatusCode::BAD_REQUEST, -32700, &message);
    let text = std::str::from_utf8(body).map_err(|_| bad("the body is not UTF-8".into()))?;
    let batch = text.trim_start().starts_with('[');
    let raw: Vec<Box<RawValue>> = if batch {
        serde_json::from_str(text)
    } else {
        serde_json::from_str(text).map(|one| vec![one])
    }
    .map_err(|e| bad(format!("the body is not JSON: {e}")))?;
    if raw.is_empty() {
        return Err(refuse(StatusCode::BAD_REQUEST, "an empty batch"));
    }
    let messages = raw
        .iter()
        .map(|m| message_of(m))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((messages, batch))
}

fn message_of(raw: &RawValue) -> Result<Message, Response> {
    let not_one = || {
        refuse(
            StatusCode::BAD_REQUEST,
            "each message is a JSON-RPC request, notification or response",
        )
    };
    let fields: Map<String, Value> = serde_json::from_str(raw.get()).map_err(|_| not_one())?;
    let method = fields.get("method").and_then(Value::as_str);
    let id = fields.get("id").map(Value::to_string);
    let answer = fields.contains_key("result") || fields.contains_key("error");
    let request = match (method, id) {
        (Some(_), Some(id)) => {
            let progress = fields
                .get("params")
                .and_then(|p| p.pointer("/_meta/progressToken"))
                .map(Value::to_string);
            Some((id, progress))
        }
        (Some(_), None) => None,
        (None, Some(_)) if answer => None,
        _ => return Err(not_one()),
    };
    // JSON has no raw line breaks but between tokens, where a space will do.
    let mut line: Vec<u8> = raw
        .get()
        .bytes()
        .map(|b| if b == b'\n' || b == b'\r' { b' ' } else { b })
        .collect();
    line.push(b'\n');
    Ok(Message {
        line,
        request,
        initialize: method == Some("initialize"),
    })
}

/// One session: its way in, and the streams its answers go out on.
struct Session {
    /// Its `Mcp-Session-Id`.
    id: String,
    /// Its number in the process, for the proxy's messages and its log.
    n: u64,
    /// Lines to the session's proxy, until the session ends.
    to_core: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    routes: Mutex<Routes>,
    /// When the host last asked for anything.
    seen: Mutex<Instant>,
}

impl Session {
    fn new(id: String, n: u64, to_core: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            id,
            n,
            to_core: Mutex::new(Some(to_core)),
            routes: Mutex::new(Routes::default()),
            seen: Mutex::new(Instant::now()),
        }
    }

    fn touch(&self) {
        *lock(&self.seen) = Instant::now();
    }

    /// How long it has had no request and no open stream.
    fn idle_for(&self) -> Duration {
        if lock(&self.routes)
            .streams
            .values()
            .any(|s| !s.tx.is_closed())
        {
            return Duration::ZERO;
        }
        lock(&self.seen).elapsed()
    }

    /// Pass the host's lines on; false once the session has ended.
    fn send(&self, lines: impl IntoIterator<Item = Vec<u8>>) -> bool {
        match lock(&self.to_core).as_ref() {
            Some(to_core) => lines.into_iter().all(|line| to_core.send(line).is_ok()),
            None => false,
        }
    }

    /// A stream for the answers to `requests`, which ends with the last of
    /// them. `json`: it takes nothing else.
    fn expect(
        &self,
        requests: &[Request],
        json: bool,
    ) -> Result<UnboundedReceiver<String>, Response> {
        let mut routes = lock(&self.routes);
        if routes.ended {
            return Err(gone());
        }
        let mut ids = HashSet::new();
        if let Some((id, _)) = requests
            .iter()
            .find(|(id, _)| routes.pending.contains_key(id) || !ids.insert(id))
        {
            return Err(refuse(
                StatusCode::BAD_REQUEST,
                &format!("request {id} is already waiting for its answer"),
            ));
        }
        Ok(routes.expect(requests, json))
    }

    /// Open the stream for the server's own messages, in place of the one
    /// before; none once the session has ended.
    fn listen(&self) -> Option<UnboundedReceiver<String>> {
        let mut routes = lock(&self.routes);
        if routes.ended {
            return None;
        }
        if let Some(old) = routes.get.take() {
            routes.forget(old);
        }
        let (tx, rx) = unbounded_channel();
        let n = routes.open(Stream {
            tx,
            owed: 0,
            progress: Vec::new(),
            json: false,
        });
        routes.get = Some(n);
        Some(rx)
    }

    /// Route a line the session's proxy wrote for the host.
    fn deliver(&self, line: &[u8]) {
        // An event's data may hold no line break; JSON needs none.
        let text = String::from_utf8_lossy(line).trim().replace('\r', " ");
        if text.is_empty() {
            return;
        }
        let Ok(message) = serde_json::from_str::<Value>(&text) else {
            eprintln!(
                "stretto-proxy: session {}: the server wrote a line that is not JSON, which a \
                 host over HTTP cannot take; dropped",
                self.n
            );
            return;
        };
        let mut routes = lock(&self.routes);
        match message {
            Value::Array(batch) => {
                for message in batch {
                    let text = message.to_string();
                    routes.route(&message, text, self.n);
                }
            }
            message => routes.route(&message, text, self.n),
        }
    }

    /// End the session: answer each request still waiting with an error,
    /// close every stream, and close the server's input. True the first
    /// time.
    fn end(&self, why: &str) -> bool {
        let mut routes = lock(&self.routes);
        if routes.ended {
            return false;
        }
        routes.ended = true;
        let pending = std::mem::take(&mut routes.pending);
        for (id, n) in pending {
            if let Some(stream) = routes.streams.get(&n) {
                let _ = stream.tx.send(ended(&id, why));
            }
        }
        routes.streams.clear();
        routes.get = None;
        routes.queue.clear();
        drop(routes);
        lock(&self.to_core).take();
        true
    }
}

/// Where a session's lines for the host go.
#[derive(Default)]
struct Routes {
    /// Requests waiting for their answers, by id: the stream each goes on.
    pending: HashMap<String, u64>,
    /// The open streams, by number, oldest first.
    streams: BTreeMap<u64, Stream>,
    /// The GET stream's number, while one is open.
    get: Option<u64>,
    /// The server's own messages, while no stream is open to take them.
    queue: VecDeque<String>,
    next: u64,
    ended: bool,
}

/// An open stream to the host.
struct Stream {
    tx: UnboundedSender<String>,
    /// Answers still to go on it; a GET stream is owed none, and stays.
    owed: usize,
    /// Its requests' progress tokens.
    progress: Vec<String>,
    /// Whether it answers as one JSON body, which takes answers alone.
    json: bool,
}

impl Routes {
    /// A stream for the answers to `requests`.
    fn expect(&mut self, requests: &[Request], json: bool) -> UnboundedReceiver<String> {
        let (tx, rx) = unbounded_channel();
        let n = self.open(Stream {
            tx,
            owed: requests.len(),
            progress: requests.iter().filter_map(|(_, p)| p.clone()).collect(),
            json,
        });
        for (id, _) in requests {
            self.pending.insert(id.clone(), n);
        }
        rx
    }

    /// Open `stream`, giving it the queued messages it can take.
    fn open(&mut self, stream: Stream) -> u64 {
        let n = self.next;
        self.next += 1;
        if !stream.json {
            for message in self.queue.drain(..) {
                let _ = stream.tx.send(message);
            }
        }
        self.streams.insert(n, stream);
        n
    }

    /// Close stream `n`, whose host has gone or was replaced.
    fn forget(&mut self, n: u64) {
        self.streams.remove(&n);
        if self.get == Some(n) {
            self.get = None;
        }
        self.pending.retain(|_, m| *m != n);
    }

    fn route(&mut self, message: &Value, text: String, session: u64) {
        if self.ended {
            return;
        }
        let answer = message.get("method").is_none()
            && (message.get("result").is_some() || message.get("error").is_some());
        if answer {
            let id = message
                .get("id")
                .map_or("null".to_string(), Value::to_string);
            let Some(n) = self.pending.remove(&id) else {
                eprintln!(
                    "stretto-proxy: session {session}: an answer to request {id}, which no \
                     host request is waiting for; dropped"
                );
                return;
            };
            if let Some(stream) = self.streams.get_mut(&n) {
                let _ = stream.tx.send(text);
                stream.owed -= 1;
                if stream.owed == 0 {
                    self.streams.remove(&n);
                }
            }
            return;
        }
        // The server's own request or notification: to the stream of the
        // request whose progress it reports, else the GET stream, else the
        // newest request's stream, else the queue.
        let token = message
            .get("params")
            .and_then(|p| p.get("progressToken"))
            .map(Value::to_string);
        loop {
            let open = |s: &&Stream| !s.json;
            let target = token
                .as_ref()
                .and_then(|t| {
                    self.streams
                        .iter()
                        .find(|(_, s)| open(s) && s.progress.contains(t))
                })
                .map(|(n, _)| *n)
                .or(self.get)
                .or_else(|| {
                    self.streams
                        .iter()
                        .rev()
                        .find(|(_, s)| open(s))
                        .map(|(n, _)| *n)
                });
            let Some(n) = target else {
                if self.queue.len() == MAX_QUEUED {
                    self.queue.pop_front();
                    eprintln!(
                        "stretto-proxy: session {session}: {MAX_QUEUED} of the server's messages \
                         wait for a stream; dropped the oldest"
                    );
                }
                self.queue.push_back(text);
                return;
            };
            if self.streams[&n].tx.send(text.clone()).is_ok() {
                return;
            }
            self.forget(n);
        }
    }
}

/// The error that answers request `id` when its session ends first.
fn ended(id: &str, why: &str) -> String {
    let id: Value = serde_json::from_str(id).unwrap_or(Value::Null);
    json!({"jsonrpc": "2.0", "id": id,
           "error": {"code": -32000, "message": format!("the session ended: {why}")}})
    .to_string()
}

/// The session's input: the lines POSTed to it, until it ends.
struct Lines {
    rx: mpsc::Receiver<Vec<u8>>,
    line: Vec<u8>,
    at: usize,
}

impl Lines {
    fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            line: Vec::new(),
            at: 0,
        }
    }
}

impl Read for Lines {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.at == self.line.len() {
            match self.rx.recv() {
                Ok(line) => (self.line, self.at) = (line, 0),
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.line.len() - self.at);
        out[..n].copy_from_slice(&self.line[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// The session's output: each line the proxy writes for the host is routed
/// to the host's streams.
struct Out {
    session: Arc<Session>,
    line: Vec<u8>,
}

impl Write for Out {
    fn write(&mut self, mut data: &[u8]) -> io::Result<usize> {
        let written = data.len();
        while let Some(end) = data.iter().position(|&b| b == b'\n') {
            self.line.extend_from_slice(&data[..end]);
            self.session.deliver(&self.line);
            self.line.clear();
            data = &data[end + 1..];
        }
        self.line.extend_from_slice(data);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Out {
    /// A last line with no newline is a message too.
    fn drop(&mut self) {
        if !self.line.is_empty() {
            self.session.deliver(&self.line);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A new session id: 16 random bytes from the operating system, as hex.
fn new_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `a` is `b`, in time that depends on their lengths alone.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether a Host header names this machine's loopback with `port` (or
/// none, on port 80).
fn loopback_host(host: &str, port: u16) -> bool {
    let (name, given) = split_host(host);
    loopback_name(name) && given.map_or(port == 80, |p| p.parse() == Ok(port))
}

/// Whether an Origin is a page on this machine's loopback, on any port.
fn loopback_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|rest| loopback_name(split_host(rest).0))
}

/// A host and its port, as `name`, `name:port` or `[v6]:port` give them.
fn split_host(host: &str) -> (&str, Option<&str>) {
    let host = host.trim();
    match host.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((inside, after)) => (&host[..inside.len() + 2], after.strip_prefix(':')),
        None => match host.rsplit_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        },
    }
}

fn loopback_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("localhost") || name == "127.0.0.1" || name == "[::1]"
}

/// Whether `version` looks like a revision of MCP, `YYYY-MM-DD`.
fn revision(version: &str) -> bool {
    let b = version.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

/// Whether a `Stretto-Session` names a host session.
fn host_session(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
}

fn json_body(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
}

fn accepts(headers: &HeaderMap, kind: &str) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.to_ascii_lowercase().contains(kind))
}

#[cfg(test)]
#[path = "listen_tests.rs"]
mod tests;
