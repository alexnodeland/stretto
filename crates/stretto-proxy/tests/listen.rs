//! Hosts over Streamable HTTP (`--listen`): a scripted host drives the demo
//! through the proxy's HTTP endpoint, in this process with
//! [`stretto_proxy::listen_on`] and as the `stretto-proxy` binary, and gets
//! what a stdio host gets.

use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use stretto_proxy::{listen_on, Active, Config, Listen, Upstream};
use stretto_trace::mcp::{read_log, Peer};
use tokio::sync::oneshot;

const PROXY: &str = env!("CARGO_BIN_EXE_stretto-proxy");
const DEMO: &str = env!("CARGO_BIN_EXE_stretto-mcp-demo");
const BOTH: &str = "application/json, text/event-stream";
const VERSION: &str = "2025-11-25";

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-listen-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The proxy, listening in this process until stopped.
struct Proxy {
    url: String,
    stop: oneshot::Sender<()>,
    done: JoinHandle<anyhow::Result<i32>>,
}

impl Proxy {
    fn start(config: Config, active: Option<&'static Active>, options: Listen) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = stretto_proxy::listen::url(listener.local_addr().unwrap());
        let (stop, stopped) = oneshot::channel::<()>();
        let done = thread::spawn(move || {
            listen_on(listener, &config, active, &options, async move {
                let _ = stopped.await;
            })
        });
        Proxy { url, stop, done }
    }

    fn stop(self) -> i32 {
        let _ = self.stop.send(());
        self.done.join().unwrap().unwrap()
    }
}

/// The demo's retail shop, recorded to `logs`.
fn shop(logs: &Path) -> Config {
    Config {
        record: Some(logs.to_path_buf()),
        domain: Some("retail".into()),
        ..Config::new([DEMO, "--world", "retail"])
    }
}

/// An MCP host over HTTP.
struct Host {
    http: Client,
    url: String,
    session: Option<String>,
    token: Option<String>,
}

impl Host {
    fn new(url: &str) -> Host {
        Host {
            http: Client::builder().no_proxy().build().unwrap(),
            url: url.to_string(),
            session: None,
            token: None,
        }
    }

    fn with(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        let mut request = request.header("mcp-protocol-version", VERSION);
        if let Some(session) = &self.session {
            request = request.header("mcp-session-id", session);
        }
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
    }

    /// POST `body`, accepting `accept`.
    fn post(&self, body: &Value, accept: &str) -> Response {
        self.with(self.http.post(&self.url))
            .header("content-type", "application/json")
            .header("accept", accept)
            .body(body.to_string())
            .send()
            .unwrap()
    }

    /// A request's event stream, whole: what came before the answer, then
    /// the answer.
    fn ask(&self, id: u64, method: &str, params: Value) -> Vec<Value> {
        let response = self.post(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            BOTH,
        );
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"].to_str().unwrap(),
            "text/event-stream"
        );
        events(&response.text().unwrap())
    }

    fn call(&self, id: u64, name: &str, arguments: Value) -> Value {
        let mut events = self.ask(
            id,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
        let answer = events.pop().unwrap();
        assert_eq!(answer["id"], id, "{answer}");
        answer["result"].clone()
    }

    /// Start a session, sending `headers` with `initialize`.
    fn initialize(&mut self, headers: &[(&str, &str)]) -> Value {
        let mut request = self.with(self.http.post(&self.url));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request
            .header("content-type", "application/json")
            .header("accept", BOTH)
            .body(
                json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                    "protocolVersion": VERSION, "capabilities": {},
                    "clientInfo": {"name": "stretto-test-host", "version": "0"}}})
                .to_string(),
            )
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        self.session = Some(
            response.headers()["mcp-session-id"]
                .to_str()
                .unwrap()
                .to_string(),
        );
        let answer = events(&response.text().unwrap()).pop().unwrap();
        let initialized = self.post(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            BOTH,
        );
        assert_eq!(initialized.status(), StatusCode::ACCEPTED);
        assert_eq!(initialized.text().unwrap(), "");
        answer
    }

    /// The GET stream, as its messages come.
    fn stream(&self) -> BufReader<Response> {
        let response = self
            .with(self.http.get(&self.url))
            .header("accept", "text/event-stream")
            .send()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        BufReader::new(response)
    }

    fn delete(&self) -> StatusCode {
        self.with(self.http.delete(&self.url))
            .send()
            .unwrap()
            .status()
    }
}

/// The messages of an event stream.
fn events(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| serde_json::from_str(data.trim()).unwrap())
        .collect()
}

/// The next message on a stream.
fn next(stream: &mut BufReader<Response>) -> Value {
    loop {
        let mut line = String::new();
        assert!(stream.read_line(&mut line).unwrap() > 0, "the stream ended");
        if let Some(data) = line.strip_prefix("data:") {
            return serde_json::from_str(data.trim()).unwrap();
        }
    }
}

fn texts(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["text"].as_str().unwrap().to_string())
        .collect()
}

/// The session logs in `dir`.
fn logs_in(dir: &Path) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.to_string_lossy();
            name.ends_with(".jsonl") && !name.ends_with(".flow.jsonl")
        })
        .collect();
    logs.sort();
    logs
}

#[test]
fn a_host_over_http_is_answered_and_recorded_as_over_stdio() {
    let dir = temp("session");
    let proxy = Proxy::start(shop(&dir), None, Listen::default());
    let mut host = Host::new(&proxy.url);
    let init = host.initialize(&[("stretto-session", "task-7")]);
    assert_eq!(init["result"]["serverInfo"]["name"], "stretto-mcp-demo");
    assert_eq!(init["result"]["protocolVersion"], VERSION);

    // A request's answer comes on its event stream, or as one body.
    let found = host.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c7@example.com"}),
    );
    assert_eq!(texts(&found), ["user_7"]);
    let listed = host.post(
        &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}),
        "application/json",
    );
    assert_eq!(listed.headers()["content-type"], "application/json");
    let listed: Value = listed.json().unwrap();
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 4);
    // A batch as one body: an array of the answers.
    let batch: Value = host
        .post(
            &json!([{"jsonrpc": "2.0", "id": 5, "method": "ping"},
                    {"jsonrpc": "2.0", "method": "notifications/cancelled",
                     "params": {"requestId": 99}},
                    {"jsonrpc": "2.0", "id": 6, "method": "ping"}]),
            "application/json",
        )
        .json()
        .unwrap();
    let ids: Vec<&Value> = batch.as_array().unwrap().iter().map(|a| &a["id"]).collect();
    assert_eq!(ids, [&json!(5), &json!(6)]);

    // What the proxy refuses.
    let refused = |response: Response| {
        let status = response.status();
        let error: Value = response.json().unwrap();
        assert!(error["error"]["message"].is_string(), "{error}");
        status
    };
    let text = host
        .with(host.http.post(&host.url))
        .header("content-type", "text/plain")
        .body("{}")
        .send()
        .unwrap();
    assert_eq!(refused(text), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let not_json = host
        .with(host.http.post(&host.url))
        .header("content-type", "application/json")
        .body("{")
        .send()
        .unwrap();
    assert_eq!(refused(not_json), StatusCode::BAD_REQUEST);
    let again = host.post(
        &json!({"jsonrpc": "2.0", "id": 7, "method": "initialize", "params": {}}),
        BOTH,
    );
    assert_eq!(refused(again), StatusCode::BAD_REQUEST);
    let twice = host.post(
        &json!([{"jsonrpc": "2.0", "id": 8, "method": "ping"},
                {"jsonrpc": "2.0", "id": 8, "method": "ping"}]),
        BOTH,
    );
    assert_eq!(refused(twice), StatusCode::BAD_REQUEST);
    let get_json = host
        .with(host.http.get(&host.url))
        .header("accept", "application/json")
        .send()
        .unwrap();
    assert_eq!(refused(get_json), StatusCode::NOT_ACCEPTABLE);
    let head = host.with(host.http.head(&host.url)).send().unwrap();
    assert_eq!(head.status(), StatusCode::METHOD_NOT_ALLOWED);
    let put = host.with(host.http.put(&host.url)).send().unwrap();
    assert_eq!(put.status(), StatusCode::METHOD_NOT_ALLOWED);
    let evil = host
        .with(host.http.get(&host.url))
        .header("accept", "text/event-stream")
        .header("origin", "https://evil.example")
        .send()
        .unwrap();
    assert_eq!(refused(evil), StatusCode::FORBIDDEN);
    let evil = host
        .with(host.http.delete(&host.url))
        .header("origin", "https://evil.example")
        .send()
        .unwrap();
    assert_eq!(refused(evil), StatusCode::FORBIDDEN);
    let stranger = Host::new(&proxy.url);
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    assert_eq!(refused(stranger.post(&ping, BOTH)), StatusCode::BAD_REQUEST);
    let lost = Host {
        session: Some("0123".into()),
        ..Host::new(&proxy.url)
    };
    assert_eq!(refused(lost.post(&ping, BOTH)), StatusCode::NOT_FOUND);
    let get = lost
        .with(lost.http.get(&lost.url))
        .header("accept", "text/event-stream")
        .send()
        .unwrap();
    assert_eq!(refused(get), StatusCode::NOT_FOUND);

    // The GET stream opens, and a DELETE ends the session and its stream.
    let mut stream = host.stream();
    assert_eq!(host.delete(), StatusCode::OK);
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut stream, &mut rest).unwrap();
    assert_eq!(events(&rest), Vec::<Value>::new());
    assert_eq!(refused(host.post(&ping, BOTH)), StatusCode::NOT_FOUND);
    assert_eq!(host.delete(), StatusCode::NOT_FOUND);
    assert_eq!(proxy.stop(), 0);

    // Recorded as a stdio session is.
    let logs = logs_in(&dir);
    assert_eq!(logs.len(), 1, "{logs:?}");
    let log = read_log(&logs[0]).unwrap();
    assert!(log.header.session.ends_with("-1"));
    assert_eq!(log.header.host_session.as_deref(), Some("task-7"));
    assert_eq!(log.header.domain.as_deref(), Some("retail"));
    let from_client = log
        .messages()
        .filter(|(from, _)| *from == Peer::Client)
        .count();
    let from_server = log
        .messages()
        .filter(|(from, _)| *from == Peer::Server)
        .count();
    // initialize, initialized, the call, tools/list, two pings and a
    // cancellation; five answers.
    assert_eq!((from_client, from_server), (7, 5));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sessions_are_apart_and_end_when_idle() {
    let dir = temp("sessions");
    let options = Listen {
        idle: Duration::from_millis(300),
        ..Listen::default()
    };
    let proxy = Proxy::start(shop(&dir), None, options);
    let mut first = Host::new(&proxy.url);
    let mut second = Host::new(&proxy.url);
    first.initialize(&[]);
    second.initialize(&[]);
    assert_ne!(first.session, second.session);
    let found = second.call(2, "get_user_details", json!({"user_id": "user_3"}));
    assert!(texts(&found)[0].contains("#W3a"));
    // Idle, the sessions end, and their hosts are told to start again.
    thread::sleep(Duration::from_millis(1200));
    let ping = json!({"jsonrpc": "2.0", "id": 9, "method": "ping"});
    assert_eq!(first.post(&ping, BOTH).status(), StatusCode::NOT_FOUND);
    assert_eq!(proxy.stop(), 0);
    let logs = logs_in(&dir);
    assert_eq!(logs.len(), 2, "{logs:?}");
    let names: Vec<String> = logs
        .iter()
        .map(|p| read_log(p).unwrap().header.session)
        .collect();
    assert!(names.iter().any(|n| n.ends_with("-1")) && names.iter().any(|n| n.ends_with("-2")));
    fs::remove_dir_all(&dir).unwrap();
}

/// The demo over HTTP behind the proxy: its log message on a tool call's
/// event stream, and its own message on the host's GET stream.
#[test]
fn the_servers_own_messages_reach_the_host_on_its_streams() {
    let mut demo = Command::new(DEMO)
        .args(["--world", "retail", "--http", "127.0.0.1:0"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut upstream = String::new();
    BufReader::new(demo.stdout.take().unwrap())
        .read_line(&mut upstream)
        .unwrap();
    let config = Config {
        upstream: Some(Upstream {
            url: upstream.trim().to_string(),
            headers: Vec::new(),
        }),
        ..Config::default()
    };
    let proxy = Proxy::start(config, None, Listen::default());
    let mut host = Host::new(&proxy.url);
    host.initialize(&[]);
    // The demo's own message, which its GET stream sends once the host
    // has initialized, goes on the host's.
    let mut stream = host.stream();
    let own = next(&mut stream);
    assert_eq!(own["method"], "notifications/message", "{own}");
    // So does the log message the demo sends before a tool call's answer,
    // while the host keeps its GET stream open; the call's stream carries
    // the answer alone.
    let events = host.ask(
        2,
        "tools/call",
        json!({"name": "get_order_details", "arguments": {"order_id": "#W1a"}}),
    );
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["id"], 2);
    let log = next(&mut stream);
    assert_eq!(log["method"], "notifications/message", "{log}");
    assert_ne!(log, own);
    assert_eq!(host.delete(), StatusCode::OK);
    assert_eq!(proxy.stop(), 0);
    let _ = demo.kill();
    let _ = demo.wait();
}

/// Each session runs what the proxy does besides forwarding: here, the
/// commit tool, with a token.
#[test]
fn each_session_is_active_and_a_token_keeps_others_out() {
    let active: &'static Active = Box::leak(Box::new(Active {
        commit: true,
        ..Active::default()
    }));
    let options = Listen {
        token: Some("0123456789abcdef-token".into()),
        ..Listen::default()
    };
    let proxy = Proxy::start(
        Config::new([DEMO, "--world", "retail"]),
        Some(active),
        options,
    );
    let mut stranger = Host::new(&proxy.url);
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    let refused = stranger.post(&ping, BOTH);
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(refused.headers()["www-authenticate"], "Bearer");
    stranger.token = Some("0123456789abcdef-token".into());
    stranger.initialize(&[]);
    let tools = stranger.ask(2, "tools/list", json!({}));
    let names: Vec<&str> = tools[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&stretto_proxy::COMMIT_TOOL), "{names:?}");
    assert_eq!(proxy.stop(), 0);
}

/// The binary: a flow served over HTTP gives the host what it gives over
/// stdio, and the proxy stops on SIGTERM, closing its sessions' logs.
#[cfg(unix)]
#[test]
fn the_binary_serves_a_flow_over_http_as_over_stdio() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Learned from the demo shop's sessions: after an email is looked up,
    // it looks up the user.
    let flow = root.join("crates/stretto-console/tests/fixtures/home/shop.flow.json");
    let dir = temp("binary");
    let args = |logs: &Path| -> Vec<String> {
        [
            "--record",
            logs.to_str().unwrap(),
            "--domain",
            "shop",
            "--flow",
            flow.to_str().unwrap(),
            "--flow-decider",
            "reach",
            "--oracle",
            "mock",
        ]
        .map(String::from)
        .to_vec()
    };
    let call = json!({"email": "c4@example.com"});

    // Over stdio.
    let mut stdio = Command::new(PROXY)
        .args(args(&dir.join("stdio")))
        .args(["--", DEMO, "--world", "retail"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut to = stdio.stdin.take().unwrap();
    let mut from = BufReader::new(stdio.stdout.take().unwrap());
    for message in [
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": VERSION, "capabilities": {},
            "clientInfo": {"name": "stretto-test-host", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "find_user_id_by_email", "arguments": call}}),
    ] {
        writeln!(to, "{message}").unwrap();
    }
    let mut answers = Vec::new();
    while answers.len() < 2 {
        let mut line = String::new();
        from.read_line(&mut line).unwrap();
        answers.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    drop(to);
    assert!(stdio.wait().unwrap().success());
    let over_stdio = texts(&answers[1]["result"]);
    assert!(over_stdio.len() > 1, "the flow looks up: {over_stdio:?}");

    // Over HTTP.
    let mut listening = Command::new(PROXY)
        .args(args(&dir.join("http")))
        .args(["--listen", "127.0.0.1:0", "--", DEMO, "--world", "retail"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(listening.stdout.take().unwrap())
        .read_line(&mut url)
        .unwrap();
    let mut host = Host::new(url.trim());
    host.initialize(&[]);
    let over_http = texts(&host.call(3, "find_user_id_by_email", call));
    assert_eq!(over_http, over_stdio);
    stop(&mut listening);
    let logs = logs_in(&dir.join("http"));
    assert_eq!(logs.len(), 1, "{logs:?}");
    // The flow's decisions sit next to the session's log.
    assert!(logs[0].with_extension("flow.jsonl").exists());
    let log = read_log(&logs[0]).unwrap();
    assert!(log.messages().count() >= 5);
    fs::remove_dir_all(&dir).unwrap();
}

/// SIGTERM, then the exit status.
#[cfg(unix)]
fn stop(child: &mut Child) {
    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert!(child.wait().unwrap().success());
}

#[test]
fn the_binary_refuses_to_listen_beyond_loopback_without_a_token() {
    let out = Command::new(PROXY)
        .args(["--listen", "0.0.0.0:0", "--", DEMO])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(125));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("needs a token (--listen-token-file)"),
        "{stderr}"
    );
}
