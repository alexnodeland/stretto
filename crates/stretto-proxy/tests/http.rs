//! A Streamable HTTP server behind the proxy: the demo in `--http` mode,
//! which insists on the session id and protocol version, answers tool calls
//! on an event stream and everything else as JSON, and sends a message of
//! its own on a GET stream.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use stretto_oracle::MockOracle;
use stretto_report::phase0::{compile_flow_from_episodes, Config};
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_trace::mcp::{read_log, Peer};
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

const PROXY: &str = env!("CARGO_BIN_EXE_stretto-proxy");
const DEMO: &str = env!("CARGO_BIN_EXE_stretto-mcp-demo");
const PATIENCE: Duration = Duration::from_secs(30);

/// The demo, serving Streamable HTTP; killed when dropped.
struct Server {
    child: Child,
    url: String,
}

impl Server {
    fn start(world: &str) -> Server {
        Server::start_with(&["--world", world])
    }

    fn start_with(args: &[&str]) -> Server {
        let mut child = Command::new(DEMO)
            .args(args)
            .args(["--http", "127.0.0.1:0"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut url = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut url)
            .unwrap();
        Server {
            child,
            url: url.trim().to_string(),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The host: runs the proxy, sends it lines and reads its answers.
struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
}

impl Host {
    fn start(args: &[&str]) -> Host {
        Host::start_env(args, &[])
    }

    fn start_env(args: &[&str], env: &[(&str, &str)]) -> Host {
        let mut child = Command::new(PROXY)
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in stdout.lines() {
                let Ok(line) = line else { return };
                if tx.send(serde_json::from_str(&line).unwrap()).is_err() {
                    return;
                }
            }
        });
        let stdin = child.stdin.take();
        Host {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    /// The answer to request `id`, and the notifications that came first.
    fn answer(&self, id: u64) -> (Value, Vec<Value>) {
        let mut notes = Vec::new();
        loop {
            let message = self
                .lines
                .recv_timeout(PATIENCE)
                .expect("the proxy answers");
            if message["id"] == id {
                return (message, notes);
            }
            notes.push(message);
        }
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> (Value, Vec<Value>) {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.answer(id)
    }

    fn open(&mut self) {
        let (init, _) = self.request(
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "stretto-test-host", "version": "0"}}),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18", "{init}");
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    fn finish(mut self) -> i32 {
        drop(self.stdin.take());
        self.child.wait().unwrap().code().unwrap()
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

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-http-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn session_log(dir: &Path) -> PathBuf {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            let name = p.to_string_lossy();
            name.ends_with(".jsonl") && !name.ends_with(".flow.jsonl")
        })
        .expect("a session log")
}

#[test]
fn records_a_streamable_http_session() {
    let server = Server::start("echo");
    let dir = temp("record");
    let logs = dir.join("logs");
    let url = format!("{}?token=s3cret", server.url);
    let mut host = Host::start(&[
        "--record",
        logs.to_str().unwrap(),
        "--domain",
        "echo",
        "--upstream",
        &url,
    ]);
    host.open();

    // The server's own stream carries a message nobody asked for.
    let own = host
        .lines
        .recv_timeout(PATIENCE)
        .expect("the server's own stream");
    assert_eq!(own["method"], "notifications/message", "{own}");
    assert_eq!(own["params"]["data"], "hello from the server's own stream");

    // A JSON answer, and an event stream with a log message first.
    let (listed, _) = host.request(2, "tools/list", json!({}));
    assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 2);
    let (called, notes) = host.request(
        3,
        "tools/call",
        json!({"name": "lookup", "arguments": {"text": "hi"}}),
    );
    assert_eq!(texts(&called["result"]), ["{\"text\":\"hi\"}"]);
    assert_eq!(notes[0]["params"]["data"], "calling lookup", "{notes:?}");
    let (pong, _) = host.request(4, "ping", json!({}));
    assert_eq!(pong["result"], json!({}));
    assert_eq!(host.finish(), 0);

    // Every message is in the log, and the URL's token is not.
    let log = read_log(&session_log(&logs)).unwrap();
    assert_eq!(log.header.server_command.len(), 1);
    let command = &log.header.server_command[0];
    assert!(command.ends_with("/mcp?token=<redacted>"), "{command}");
    let from = |peer| log.entries.iter().filter(|e| e.from == peer).count();
    assert_eq!(from(Peer::Client), 5);
    assert_eq!(from(Peer::Server), 6);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_request_the_server_cannot_take_is_answered_with_an_error() {
    // Nothing listens on the port the dropped server had.
    let url = Server::start("echo").url.clone();
    let mut host = Host::start(&["--upstream", &url]);
    let (init, _) = host.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "stretto-test-host", "version": "0"}}),
    );
    let message = init["error"]["message"].as_str().unwrap();
    assert!(message.contains("could not be reached"), "{init}");
    assert_eq!(host.finish(), 0);
}

// A flow over HTTP: the proxy's own lookups go to the server as requests
// too, with the session's headers.

fn manifest() -> ToolManifest {
    ToolManifest {
        domain: "retail".to_string(),
        tools: [
            ("find_user_id_by_email", ToolKind::Read),
            ("get_user_details", ToolKind::Read),
            ("get_order_details", ToolKind::Read),
            ("cancel_pending_order", ToolKind::Write),
        ]
        .into_iter()
        .map(|(t, k)| (t.to_string(), k))
        .collect(),
        docs: Default::default(),
    }
}

fn call(id: &str, name: &str, arguments: Value) -> Event {
    Event::Assistant {
        text: None,
        calls: vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }],
        usage: None,
    }
}

fn result(id: &str, name: &str, content: Value) -> Event {
    Event::ToolResult {
        call_id: id.to_string(),
        name: name.to_string(),
        error: false,
        content: match content {
            Value::String(s) => s,
            v => v.to_string(),
        },
    }
}

/// Customer `n` gives an email; the agent finds them, reads their details
/// and one order.
fn session(n: usize) -> Episode {
    Episode {
        id: format!("s{n}"),
        task_id: String::new(),
        trial: 0,
        domain: "retail".to_string(),
        agent_model: "agent".to_string(),
        reward: 1.0,
        events: vec![
            Event::User {
                text: format!("I'm c{n}@example.com."),
            },
            call(
                "1",
                "find_user_id_by_email",
                json!({"email": format!("c{n}@example.com")}),
            ),
            result("1", "find_user_id_by_email", json!(format!("user_{n}"))),
            call(
                "2",
                "get_user_details",
                json!({"user_id": format!("user_{n}")}),
            ),
            result(
                "2",
                "get_user_details",
                json!({"user_id": format!("user_{n}"),
                       "orders": [format!("#W{n}a"), format!("#W{n}b")]}),
            ),
            call(
                "3",
                "get_order_details",
                json!({"order_id": format!("#W{n}a")}),
            ),
            result("3", "get_order_details", json!({"status": "pending"})),
            Event::Assistant {
                text: Some("Your order is pending.".to_string()),
                calls: Vec::new(),
                usage: None,
            },
        ],
    }
}

#[test]
fn a_flow_looks_things_up_over_http() {
    let dir = temp("flow");
    let episodes: Vec<Episode> = (0..40).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let flow = compile_flow_from_episodes(&config, &episodes, &manifest(), &oracle).unwrap();
    let flow_path = dir.join("retail.flow.json");
    flow.save(&flow_path).unwrap();

    let server = Server::start("retail");
    let logs = dir.join("logs");
    let mut host = Host::start(&[
        "--record",
        logs.to_str().unwrap(),
        "--domain",
        "retail",
        "--flow",
        flow_path.to_str().unwrap(),
        "--oracle",
        "mock",
        "--flow-decider",
        "habit",
        "--upstream",
        &server.url,
    ]);
    host.open();
    let (_, _) = host.request(2, "tools/list", json!({}));
    let (found, _) = host.request(
        3,
        "tools/call",
        json!({"name": "find_user_id_by_email", "arguments": {"email": "c7@example.com"}}),
    );
    let parts = texts(&found["result"]);
    assert_eq!(parts[0], "user_7");
    assert!(
        parts[1].contains("get_user_details {\"user_id\":\"user_7\"}:"),
        "{parts:?}"
    );
    assert_eq!(host.finish(), 0);
    // The flow's lookup is in the log as the proxy's own request.
    let log = read_log(&session_log(&logs)).unwrap();
    assert!(log.entries.iter().any(|e| e.from == Peer::Proxy));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn headers_come_from_the_environment_and_stay_out_of_the_log() {
    let server = Server::start_with(&["--require-auth", "Bearer t0ken-value"]);
    let dir = temp("auth");
    let logs = dir.join("logs");
    let args = [
        "--record",
        logs.to_str().unwrap(),
        "--upstream",
        &server.url,
        "--upstream-header",
        "Authorization=STRETTO_TEST_AUTH",
    ];
    let mut host = Host::start_env(&args, &[("STRETTO_TEST_AUTH", "Bearer t0ken-value")]);
    host.open();
    let (listed, _) = host.request(2, "tools/list", json!({}));
    assert!(listed["result"]["tools"].is_array(), "{listed}");
    assert_eq!(host.finish(), 0);
    let log = fs::read_to_string(session_log(&logs)).unwrap();
    assert!(!log.contains("t0ken"), "{log}");

    // Without the header the server refuses, and the host hears why.
    let mut host = Host::start(&["--upstream", &server.url]);
    let (init, _) = host.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "stretto-test-host", "version": "0"}}),
    );
    assert!(
        init["error"]["message"].as_str().unwrap().contains("401"),
        "{init}"
    );
    assert_eq!(host.finish(), 0);

    // A variable that is not set is an error before anything is sent.
    let unset = Command::new(PROXY)
        .args([
            "--upstream",
            &server.url,
            "--upstream-header",
            "Authorization=STRETTO_TEST_UNSET",
        ])
        .env_remove("STRETTO_TEST_UNSET")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(unset.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&unset.stderr).contains("STRETTO_TEST_UNSET"));
    let _ = fs::remove_dir_all(&dir);
}
