//! One agent, two servers. Each session, a host runs two proxies, a shop's
//! and a notes server's, which record into one directory under one host
//! session; `stretto_trace::mcp::merge` makes one session of their logs; a
//! flow learned from those spans both servers; and each proxy serving it
//! looks up only its own server's tools.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use stretto_report::phase0::{compile_habit_flow_from_episodes, Config};
use stretto_trace::mcp::{episode, manifest_of, merge, read_sessions};
use stretto_trace::{Episode, Event, ToolKind};

const PROXY: &str = env!("CARGO_BIN_EXE_stretto-proxy");
const DEMO: &str = env!("CARGO_BIN_EXE_stretto-mcp-demo");
const PATIENCE: Duration = Duration::from_secs(30);

/// A host's end of one proxy.
struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Host {
    /// Start the proxy with `args` in host session `session`, and open the
    /// MCP session.
    fn start(args: &[&str], session: &str) -> Host {
        let mut child = Command::new(PROXY)
            .args(args)
            .env("STRETTO_SESSION", session)
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
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut host = Host {
            child,
            stdin,
            lines,
        };
        host.send(
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "stretto-test-host", "version": "0"}}}),
        );
        assert_eq!(host.recv()["id"], 1);
        host.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        host.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        assert_eq!(host.recv()["id"], 2);
        host
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn recv(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(PATIENCE)
            .expect("the proxy answers");
        serde_json::from_str(&line).unwrap()
    }

    /// Call a tool, and return the text of each item of its result. Like
    /// a model, the agent takes a moment first: the logs time their lines
    /// to the millisecond, so calls to two servers closer than that could
    /// be merged in either order.
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> Vec<String> {
        thread::sleep(Duration::from_millis(5));
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                         "params": {"name": name, "arguments": arguments}}));
        let response = self.recv();
        assert_eq!(response["id"], id, "{response}");
        response["result"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["text"].as_str().unwrap().to_string())
            .collect()
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        assert_eq!(self.child.wait().unwrap().code(), Some(0));
    }
}

/// The shop's proxy and the notes server's, with `args` before their
/// commands.
fn shop(args: &[&str], session: &str) -> Host {
    let all = [
        args,
        &["--server-name", "shop", "--", DEMO, "--world", "retail"],
    ]
    .concat();
    Host::start(&all, session)
}

fn notes(args: &[&str], session: &str) -> Host {
    let all = [args, &["--server-name", "notes", "--", DEMO]].concat();
    Host::start(&all, session)
}

/// The tools of each call, in order.
fn calls(ep: &Episode) -> Vec<&str> {
    ep.events
        .iter()
        .flat_map(|e| match e {
            Event::Assistant { calls, .. } => calls.iter().map(|c| c.name.as_str()).collect(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn a_host_sessions_servers_are_learned_as_one_session_and_served_apart() {
    let dir = std::env::temp_dir().join(format!("stretto-servers-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let logs = dir.join("logs");
    let record = ["--record", logs.to_str().unwrap()];

    // Each customer: the agent finds them in the shop, reads their details,
    // looks them up in its notes, and reads their first order.
    for n in 0..20 {
        let session = format!("task-{n}");
        let mut shop = shop(&record, &session);
        let mut notes = notes(&record, &session);
        let user = shop.call(
            3,
            "find_user_id_by_email",
            json!({"email": format!("c{n}@example.com")}),
        );
        assert_eq!(user, [format!("user_{n}")]);
        shop.call(4, "get_user_details", json!({"user_id": user[0]}));
        notes.call(3, "lookup", json!({"text": user[0]}));
        shop.call(
            5,
            "get_order_details",
            json!({"order_id": format!("#W{n}a")}),
        );
        shop.finish();
        notes.finish();
    }

    // Forty logs, twenty sessions, each with both servers' calls in order.
    let logs = read_sessions(&logs).unwrap();
    assert_eq!(logs.len(), 40);
    assert!(logs.iter().all(|l| l
        .header
        .host_session
        .as_deref()
        .is_some_and(|s| s.starts_with("task-"))));
    let sessions = merge(logs);
    assert_eq!((sessions.merged_logs, sessions.merged_sessions), (40, 20));
    let episodes: Vec<Episode> = sessions
        .logs
        .iter()
        .map(|log| {
            let mut ep = episode(log);
            ep.reward = 1.0;
            ep
        })
        .collect();
    assert_eq!(
        calls(&episodes[0]),
        [
            "shop::find_user_id_by_email",
            "shop::get_user_details",
            "notes::lookup",
            "shop::get_order_details"
        ]
    );
    let manifest = manifest_of(&sessions.logs, "support");
    assert_eq!(manifest.tools["notes::lookup"], ToolKind::Read);
    assert_eq!(manifest.tools["notes::update"], ToolKind::Write);
    assert_eq!(manifest.tools["shop::get_user_details"], ToolKind::Read);

    // A flow across both servers.
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let flow = compile_habit_flow_from_episodes(&config, &episodes, &manifest).unwrap();
    let path = dir.join("support.flow.json");
    flow.save(&path).unwrap();
    let serve = ["--flow", path.to_str().unwrap(), "--flow-decider", "habit"];

    // The shop's proxy looks the customer's details up with the shop's own
    // tool, as the agent did next.
    let mut shop = shop(&serve, "task-new");
    let found = shop.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c123@example.com"}),
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[1].contains("\n\nget_user_details {\"user_id\":\"user_123\"}:\n"),
        "{}",
        found[1]
    );
    shop.finish();
    // What the agent did after its notes, the notes server's proxy leaves
    // to it: the order is the shop's.
    let mut notes = notes(&serve, "task-new");
    assert_eq!(
        notes.call(3, "lookup", json!({"text": "user_123"})).len(),
        1
    );
    notes.finish();
    fs::remove_dir_all(&dir).unwrap();
}
