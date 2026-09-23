//! The proxy end to end: a host drives the demo server through it.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use stretto_trace::mcp::{episode, manifest, read_log, Peer};
use stretto_trace::{Event, ToolKind};

const PROXY: &str = env!("CARGO_BIN_EXE_stretto-proxy");
const DEMO: &str = env!("CARGO_BIN_EXE_stretto-mcp-demo");

/// How long to wait for any one reply before failing.
const PATIENCE: Duration = Duration::from_secs(20);

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"stretto-test-host","version":"0.0.1"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
// Sent together; the first waits before answering, so both are in flight.
const CALL_3: &str = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"order #W1","delay_ms":300}}}"#;
const CALL_4: &str = r#"{"jsonrpc":"2.0","id":"call-4","method":"tools/call","params":{"name":"update","arguments":{"text":"café ☕"}}}"#;
const CALL_5: &str = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"x","fail":true}}}"#;
const CALL_6: &str = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"delete_everything","arguments":{}}}"#;
const JUNK: &str = "this line is not JSON";

/// A host talking to an MCP server over stdio, one step at a time.
struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Vec<u8>>,
    stderr: Option<JoinHandle<String>>,
    /// Every byte the server side wrote, in order.
    seen: Vec<u8>,
}

impl Host {
    fn start(program: &str, args: &[&str]) -> Host {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || loop {
            let mut line = Vec::new();
            match stdout.read_until(b'\n', &mut line) {
                Ok(n) if n > 0 && tx.send(line).is_ok() => {}
                _ => return,
            }
        });
        let mut stderr = child.stderr.take().unwrap();
        let stderr = thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        });
        Host {
            stdin: child.stdin.take(),
            child,
            lines,
            stderr: Some(stderr),
            seen: Vec::new(),
        }
    }

    /// Send `lines` in a single write, then wait for `replies` lines back.
    fn step(&mut self, lines: &[&str], replies: usize) {
        let batch: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(batch.as_bytes()).unwrap();
        stdin.flush().unwrap();
        for _ in 0..replies {
            let line = self.lines.recv_timeout(PATIENCE).expect("a reply in time");
            self.seen.extend(line);
        }
    }

    /// Optionally close the server's input, then collect whatever else it
    /// writes and wait for it to exit. Returns what the host saw, the exit
    /// status and the server side's stderr.
    fn finish(mut self, close_input: bool) -> (Vec<u8>, ExitStatus, String) {
        if close_input {
            drop(self.stdin.take());
        }
        loop {
            match self.lines.recv_timeout(PATIENCE) {
                Ok(line) => self.seen.extend(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => panic!("the server did not finish in time"),
            }
        }
        let status = self.child.wait().unwrap();
        let stderr = self.stderr.take().unwrap().join().unwrap();
        (std::mem::take(&mut self.seen), status, stderr)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A fresh directory under the system temp dir, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let path =
            std::env::temp_dir().join(format!("stretto-proxy-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        TempDir(path)
    }

    fn join(&self, path: &str) -> String {
        self.0.join(path).to_str().unwrap().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The session the host runs: initialize, list tools, two overlapping
/// calls, a failing call, an unknown tool, and a line that is not JSON.
fn session(host: &mut Host) {
    host.step(&[INITIALIZE], 1);
    host.step(&[INITIALIZED, LIST], 1);
    host.step(&[CALL_3, CALL_4], 2);
    host.step(&[CALL_5], 1);
    host.step(&[CALL_6], 1);
    host.step(&[JUNK], 1);
}

/// What the host sees from the demo server with no proxy in between.
fn direct_session() -> Vec<u8> {
    let mut host = Host::start(DEMO, &[]);
    session(&mut host);
    let (seen, status, _) = host.finish(true);
    assert!(status.success());
    assert_eq!(seen.iter().filter(|&&b| b == b'\n').count(), 7);
    seen
}

/// The only file in `dir`.
fn only_file(dir: &str) -> PathBuf {
    let files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    files.into_iter().next().unwrap()
}

#[test]
fn forwards_unchanged_and_records_an_episode() {
    let expected = direct_session();
    let tmp = TempDir::new("record");
    let logs = tmp.join("logs");

    let mut host = Host::start(PROXY, &["--record", &logs, "--domain", "demo", "--", DEMO]);
    session(&mut host);
    let (seen, status, stderr) = host.finish(true);
    assert_eq!(status.code(), Some(0));
    // As text first, for a readable failure; then byte for byte.
    assert_eq!(
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&expected)
    );
    assert_eq!(seen, expected);

    let path = only_file(&logs);
    assert!(stderr.contains(&format!("stretto-proxy: recording to {}", path.display())));
    let log = read_log(&path).unwrap();
    assert_eq!(
        path.file_stem().and_then(|s| s.to_str()),
        Some(log.header.session.as_str())
    );
    assert_eq!(log.header.server_command, [DEMO]);
    assert_eq!(log.header.domain.as_deref(), Some("demo"));
    assert_eq!(log.header.agent_model, None);

    // Eight lines from the host, seven from the server; the junk line is raw.
    let from = |peer| log.entries.iter().filter(|e| e.from == peer).count();
    assert_eq!((from(Peer::Client), from(Peer::Server)), (8, 7));
    let raw: Vec<&str> = log
        .entries
        .iter()
        .filter_map(|e| e.raw.as_deref())
        .collect();
    assert_eq!(raw, [JUNK]);
    // Messages are logged exactly as they crossed the wire.
    let text = fs::read_to_string(&path).unwrap();
    for line in [
        INITIALIZE,
        INITIALIZED,
        LIST,
        CALL_3,
        CALL_4,
        CALL_5,
        CALL_6,
    ] {
        assert!(
            text.contains(&format!(r#""from":"client","message":{line}}}"#)),
            "{line}"
        );
    }
    for line in String::from_utf8(seen).unwrap().lines() {
        assert!(
            text.contains(&format!(r#""from":"server","message":{line}}}"#)),
            "{line}"
        );
    }

    let ep = episode(&log);
    assert_eq!(ep.id, log.header.session);
    assert_eq!(ep.domain, "demo");
    assert_eq!(ep.agent_model, "stretto-test-host");
    assert!(!ep.succeeded());
    let outline: Vec<String> = ep
        .events
        .iter()
        .map(|e| match e {
            Event::Assistant { calls, .. } => {
                let calls: Vec<String> = calls
                    .iter()
                    .map(|c| format!("{}={}", c.id, c.name))
                    .collect();
                format!("turn({})", calls.join(","))
            }
            Event::ToolResult {
                call_id,
                name,
                error,
                ..
            } => {
                format!(
                    "{}({call_id}={name})",
                    if *error { "error" } else { "result" }
                )
            }
            Event::User { .. } => "user".to_string(),
        })
        .collect();
    assert_eq!(
        outline,
        [
            "turn(3=lookup,call-4=update)",
            "result(3=lookup)",
            "result(call-4=update)",
            "turn(5=lookup)",
            "error(5=lookup)",
            "turn(6=delete_everything)",
            "error(6=delete_everything)",
        ]
    );
    let contents: Vec<&str> = ep
        .events
        .iter()
        .filter_map(|e| match e {
            Event::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    let json = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
    assert_eq!(
        json(contents[0]),
        json(r#"{"text":"order #W1","delay_ms":300}"#)
    );
    assert_eq!(json(contents[1]), json(r#"{"text":"café ☕"}"#));
    assert!(contents[2].starts_with("lookup failed, as asked"));
    assert_eq!(contents[3], "Unknown tool: delete_everything");

    let tools = manifest(&log, &ep.domain);
    assert_eq!(tools.tools.len(), 2);
    assert_eq!(tools.kind("lookup"), Some(ToolKind::Read));
    assert_eq!(tools.kind("update"), Some(ToolKind::Write));
}

#[test]
fn without_record_it_only_forwards() {
    let expected = direct_session();
    let mut host = Host::start(PROXY, &["--", DEMO]);
    session(&mut host);
    let (seen, status, stderr) = host.finish(true);
    assert_eq!(status.code(), Some(0));
    assert_eq!(seen, expected);
    assert_eq!(stderr, "");
}

#[cfg(unix)]
#[test]
fn exits_with_the_server_when_it_exits_first() {
    let tmp = TempDir::new("exit-first");
    let logs = tmp.join("logs");
    let server = r#"echo '{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"bye"}}'; exit 3"#;
    let host = Host::start(PROXY, &["--record", &logs, "--", "sh", "-c", server]);
    // The host never closes its end: the server leaving must be enough.
    let (seen, status, _) = host.finish(false);
    assert_eq!(status.code(), Some(3));
    assert!(seen.starts_with(br#"{"jsonrpc":"2.0","method":"notifications/message""#));
    let log = read_log(&only_file(&logs)).unwrap();
    assert_eq!(log.entries.len(), 1);
    assert_eq!(log.entries[0].from, Peer::Server);
    assert_eq!(log.header.server_command[..2], ["sh", "-c"]);
}

#[test]
fn reports_a_server_that_cannot_start() {
    let tmp = TempDir::new("no-server");
    let logs = tmp.join("logs");
    let out = Command::new(PROXY)
        .args([
            "--record",
            &logs,
            "--",
            "/nonexistent/stretto-no-such-server",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(125));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("stretto-proxy: starting /nonexistent/stretto-no-such-server"),
        "{stderr}"
    );
    // No session happened, so no log is left behind.
    assert_eq!(fs::read_dir(Path::new(&logs)).unwrap().count(), 0);
}
