//! Active mode end to end: a host drives the demo's retail world through the
//! proxy, with a flow learned from synthetic sessions, the retail guards,
//! the commit tool and a conversation file.

use serde_json::{json, Value};
use std::collections::BTreeMap;
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
use stretto_trace::mcp::{episode, read_log};
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

const PROXY: &str = env!("CARGO_BIN_EXE_stretto-proxy");
const DEMO: &str = env!("CARGO_BIN_EXE_stretto-mcp-demo");
const PATIENCE: Duration = Duration::from_secs(30);

fn manifest() -> ToolManifest {
    ToolManifest {
        domain: "retail".to_string(),
        tools: BTreeMap::from([
            ("find_user_id_by_email".to_string(), ToolKind::Read),
            ("get_user_details".to_string(), ToolKind::Read),
            ("get_order_details".to_string(), ToolKind::Read),
            ("cancel_pending_order".to_string(), ToolKind::Write),
        ]),
        docs: BTreeMap::new(),
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
            other => other.to_string(),
        },
    }
}

fn say(text: &str) -> Event {
    Event::Assistant {
        text: Some(text.to_string()),
        calls: Vec::new(),
        usage: None,
    }
}

fn order(n: usize, which: char) -> Value {
    json!({"order_id": format!("#W{n}{which}"), "user_id": format!("user_{n}"), "status": "pending"})
}

/// Customer `n` is found by email; the agent reads their details and both
/// orders, then cancels the first once they agree.
fn session(n: usize) -> Episode {
    let user = format!("user_{n}");
    Episode {
        id: format!("session-{n}"),
        task_id: String::new(),
        trial: 0,
        domain: "retail".to_string(),
        agent_model: "agent".to_string(),
        reward: 1.0,
        events: vec![
            Event::User {
                text: format!("Hi, I'm c{n}@example.com and I want to cancel an order."),
            },
            call(
                "1",
                "find_user_id_by_email",
                json!({"email": format!("c{n}@example.com")}),
            ),
            result("1", "find_user_id_by_email", json!(user)),
            call("2", "get_user_details", json!({"user_id": user})),
            result(
                "2",
                "get_user_details",
                json!({"user_id": user, "orders": [format!("#W{n}a"), format!("#W{n}b")]}),
            ),
            call(
                "3",
                "get_order_details",
                json!({"order_id": format!("#W{n}a")}),
            ),
            result("3", "get_order_details", order(n, 'a')),
            call(
                "4",
                "get_order_details",
                json!({"order_id": format!("#W{n}b")}),
            ),
            result("4", "get_order_details", order(n, 'b')),
            say("Both orders are pending. Cancel the first, as no longer needed?"),
            Event::User {
                text: "Yes, please.".to_string(),
            },
            call(
                "5",
                "cancel_pending_order",
                json!({"order_id": format!("#W{n}a"), "reason": "no longer needed"}),
            ),
            result(
                "5",
                "cancel_pending_order",
                json!({"order_id": format!("#W{n}a"), "status": "cancelled"}),
            ),
            say("Done."),
        ],
    }
}

/// A flow learned from sixty such sessions, saved to `dir`.
fn learned_flow(dir: &Path) -> PathBuf {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
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
    let path = dir.join("retail.flow.json");
    flow.save(&path).unwrap();
    path
}

struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Host {
    fn start(args: &[&str]) -> Host {
        Host::start_without(args, &[])
    }

    /// Start the proxy with `unset` removed from its environment.
    fn start_without(args: &[&str], unset: &[&str]) -> Host {
        let mut command = Command::new(PROXY);
        for var in unset {
            command.env_remove(var);
        }
        let mut child = command
            .args(args)
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

    /// The next message from the proxy.
    fn recv(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(PATIENCE)
            .expect("the proxy answers");
        serde_json::from_str(&line).unwrap()
    }

    /// Call a tool and return its result.
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                         "params": {"name": name, "arguments": arguments}}));
        let response = self.recv();
        assert_eq!(response["id"], id, "{response}");
        response["result"].clone()
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

#[test]
fn flows_guards_and_commit_through_the_proxy() {
    let dir = std::env::temp_dir().join(format!("stretto-active-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let flow = learned_flow(&dir);
    let logs = dir.join("logs");
    let context = dir.join("context.jsonl");

    let mut host = Host::start(&[
        "--record",
        logs.to_str().unwrap(),
        "--domain",
        "retail",
        "--flow",
        flow.to_str().unwrap(),
        "--oracle",
        "mock",
        "--guards",
        "--commit",
        "--context",
        context.to_str().unwrap(),
        "--",
        DEMO,
        "--world",
        "retail",
    ]);
    host.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "stretto-test-host", "version": "0"}}}),
    );
    assert_eq!(host.recv()["id"], 1);
    host.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    // The commit tool is listed after the server's own.
    host.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let listed = host.recv();
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names.last(), Some(&"stretto_commit"), "{names:?}");
    assert_eq!(names.len(), 5);

    // The host hands over what the customer said.
    fs::write(
        &context,
        "{\"role\":\"user\",\"content\":\"Hi, I'm c7@example.com and I want to cancel an order.\"}\n",
    )
    .unwrap();

    // The agent finds the user; the flow reads their details in the same
    // response, with the id bound from the first result.
    let found = host.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c7@example.com"}),
    );
    let parts = texts(&found);
    assert_eq!(parts[0], "user_7");
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        parts[1].starts_with(stretto_proxy::APPENDIX),
        "{}",
        parts[1]
    );
    assert!(
        parts[1].contains("\n\nget_user_details {\"user_id\":\"user_7\"}:\n"),
        "{}",
        parts[1]
    );
    // A flow never writes.
    assert!(!parts[1].contains("cancel_pending_order"), "{}", parts[1]);

    // A cancellation for a reason the policy does not accept is refused,
    // and never reaches the server.
    let refused = host.call(
        4,
        "cancel_pending_order",
        json!({"order_id": "#W7a", "reason": "found it cheaper"}),
    );
    assert_eq!(refused["isError"], true);
    let why = &texts(&refused)[0];
    assert!(why.contains("retail.cancel_reason"), "{why}");

    // The customer confirms; one commit checks the status and cancels.
    fs::write(
        &context,
        "{\"role\":\"user\",\"content\":\"Hi, I'm c7@example.com and I want to cancel an order.\"}\n\
         {\"role\":\"user\",\"content\":\"Yes, cancel #W7a, I no longer need it.\"}\n",
    )
    .unwrap();
    let committed = host.call(
        5,
        "stretto_commit",
        json!({"calls": [
            {"name": "get_order_details", "arguments": {"order_id": "#W7a"}},
            {"name": "cancel_pending_order", "arguments": {"order_id": "#W7a", "reason": "no longer needed"}}
        ]}),
    );
    assert_eq!(committed["isError"], false, "{committed}");
    let report = &texts(&committed)[0];
    assert!(report.contains("\"status\":\"cancelled\""), "{report}");

    // A commit stops at the first refusal and says what did not run.
    let stopped = host.call(
        6,
        "stretto_commit",
        json!({"calls": [
            {"name": "cancel_pending_order", "arguments": {"order_id": "#W7b", "reason": "found it cheaper"}},
            {"name": "get_order_details", "arguments": {"order_id": "#W7b"}}
        ]}),
    );
    assert_eq!(stopped["isError"], true);
    let report = &texts(&stopped)[0];
    assert!(
        report.contains("was refused by the policy check"),
        "{report}"
    );
    assert!(
        report.contains("get_order_details {\"order_id\":\"#W7b\"} was not run"),
        "{report}"
    );

    assert_eq!(host.finish(), 0);

    // The log holds the whole session: the conversation, the flow's own
    // calls, the refusal as the refused call's result, and the commit.
    let mut sessions: Vec<PathBuf> = fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    sessions.sort();
    let (flow_logs, session_logs): (Vec<PathBuf>, Vec<PathBuf>) = sessions
        .into_iter()
        .partition(|p| p.to_string_lossy().ends_with(".flow.jsonl"));
    assert_eq!(session_logs.len(), 1);
    assert_eq!(flow_logs.len(), 1);
    assert!(!fs::read_to_string(&flow_logs[0]).unwrap().is_empty());
    let log = read_log(&session_logs[0]).unwrap();
    let ep = episode(&log);
    let said: Vec<&str> = ep
        .events
        .iter()
        .filter_map(|e| match e {
            Event::User { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    let calls: Vec<(&str, &str)> = ep
        .events
        .iter()
        .flat_map(|e| match e {
            Event::Assistant { calls, .. } => calls
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    assert!(
        calls
            .iter()
            .any(|(id, name)| id.starts_with("stretto-") && *name == "get_user_details"),
        "{calls:?}"
    );
    let refusal = ep.events.iter().find_map(|e| match e {
        Event::ToolResult {
            call_id,
            error,
            content,
            ..
        } if call_id == "4" => Some((*error, content.clone())),
        _ => None,
    });
    let (error, content) = refusal.expect("the refused call has a result");
    assert!(
        error && content.contains("retail.cancel_reason"),
        "{content}"
    );
    // The server saw exactly one cancellation: the committed one.
    let cancels = log
        .messages()
        .filter(|(from, m)| {
            *from != stretto_trace::mcp::Peer::Client
                && m.get("method").and_then(Value::as_str) == Some("tools/call")
                && m.pointer("/params/name").and_then(Value::as_str) == Some("cancel_pending_order")
        })
        .count();
    assert_eq!(cancels, 1);

    fs::remove_dir_all(&dir).unwrap();
}

/// Start a session: initialize, then list the tools.
fn open_session(host: &mut Host) {
    host.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "stretto-test-host", "version": "0"}}}),
    );
    assert_eq!(host.recv()["id"], 1);
    host.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    host.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert_eq!(host.recv()["id"], 2);
}

/// Say `text` as the customer, in the conversation file the host keeps.
fn say_to(context: &Path, text: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(context)
        .unwrap();
    writeln!(file, "{}", json!({"role": "user", "content": text})).unwrap();
}

/// Record, learn, serve: sessions recorded through the proxy teach a flow,
/// which the proxy then runs on a new customer.
#[test]
fn sessions_recorded_through_the_proxy_teach_the_flow_it_serves() {
    let dir = std::env::temp_dir().join(format!("stretto-loop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).unwrap();

    // Forty agents' sessions, as the agents ran them, with the conversation.
    for n in 0..40 {
        let context = dir.join(format!("context-{n}.jsonl"));
        let mut host = Host::start(&[
            "--record",
            logs.to_str().unwrap(),
            "--domain",
            "retail",
            "--context",
            context.to_str().unwrap(),
            "--",
            DEMO,
            "--world",
            "retail",
        ]);
        open_session(&mut host);
        say_to(
            &context,
            &format!("Hi, I'm c{n}@example.com and I want to cancel an order."),
        );
        let user = host.call(
            3,
            "find_user_id_by_email",
            json!({"email": format!("c{n}@example.com")}),
        );
        assert_eq!(texts(&user), [format!("user_{n}")]);
        host.call(
            4,
            "get_user_details",
            json!({"user_id": format!("user_{n}")}),
        );
        host.call(
            5,
            "get_order_details",
            json!({"order_id": format!("#W{n}a")}),
        );
        host.call(
            6,
            "get_order_details",
            json!({"order_id": format!("#W{n}b")}),
        );
        say_to(&context, "Yes, please cancel the first one.");
        let cancelled = host.call(
            7,
            "cancel_pending_order",
            json!({"order_id": format!("#W{n}a"), "reason": "no longer needed"}),
        );
        assert_eq!(cancelled["isError"], false);
        assert_eq!(host.finish(), 0);
    }

    // Learn as `stretto learn` does: the tools from the sessions' own
    // tools/list, every session counted as a success.
    let sessions = stretto_trace::mcp::read_sessions(&logs).unwrap();
    assert_eq!(sessions.len(), 40);
    let manifest = stretto_trace::mcp::manifest_of(&sessions, "retail");
    assert_eq!(manifest.tools["get_user_details"], ToolKind::Read);
    assert_eq!(manifest.tools["cancel_pending_order"], ToolKind::Write);
    let episodes: Vec<Episode> = sessions
        .iter()
        .map(|log| {
            let mut ep = episode(log);
            ep.reward = 1.0;
            ep
        })
        .collect();
    assert!(episodes
        .iter()
        .all(|ep| matches!(&ep.events[0], Event::User { text } if text.starts_with("Hi, I'm"))));
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let flow = compile_flow_from_episodes(&config, &episodes, &manifest, &oracle).unwrap();
    let path = dir.join("learned.flow.json");
    flow.save(&path).unwrap();

    // Serve it: a new customer's first lookup brings their details along.
    let context = dir.join("context-new.jsonl");
    say_to(
        &context,
        "Hi, I'm c123@example.com and I want to cancel an order.",
    );
    let mut host = Host::start(&[
        "--domain",
        "retail",
        "--flow",
        path.to_str().unwrap(),
        "--oracle",
        "mock",
        "--context",
        context.to_str().unwrap(),
        "--",
        DEMO,
        "--world",
        "retail",
    ]);
    open_session(&mut host);
    let found = host.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c123@example.com"}),
    );
    let parts = texts(&found);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        parts[1].contains("get_user_details {\"user_id\":\"user_123\"}"),
        "{}",
        parts[1]
    );
    assert_eq!(host.finish(), 0);

    // The habit alone serves the same lookup, and asks no one: it needs no
    // key, even with the Jev oracle named.
    let mut host = Host::start_without(
        &[
            "--domain",
            "retail",
            "--flow",
            path.to_str().unwrap(),
            "--oracle",
            "jev",
            "--flow-decider",
            "habit",
            "--context",
            context.to_str().unwrap(),
            "--",
            DEMO,
            "--world",
            "retail",
        ],
        &["TYPESAFE_API_KEY"],
    );
    open_session(&mut host);
    let found = host.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c123@example.com"}),
    );
    let parts = texts(&found);
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!(
        parts[1].contains("get_user_details {\"user_id\":\"user_123\"}"),
        "{}",
        parts[1]
    );
    assert_eq!(host.finish(), 0);
    fs::remove_dir_all(&dir).unwrap();
}

/// Say `text` as the agent, in the conversation file the host keeps.
fn agent_says(context: &Path, text: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(context)
        .unwrap();
    writeln!(file, "{}", json!({"role": "assistant", "content": text})).unwrap();
}

/// The confirmation judge asks the questions `stretto confirm` asks, logs
/// each judgment, refuses in enforce mode a write it fails, refuses nothing
/// when it cannot answer, and never refuses in log mode.
#[test]
fn the_confirmation_judge_refuses_writes_the_customer_did_not_confirm() {
    use stretto_oracle::{Answer, ReplayCache, Response};
    use stretto_report::confirm;

    let dir = std::env::temp_dir().join(format!("stretto-judge-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let cache = dir.join("cache");
    // The judge's answers, seeded for the exact questions the proxy will ask.
    let seed = |proposal: &str, customer: &str, order: &str, p: f64| {
        let call = ToolCall {
            id: "pending".to_string(),
            name: "cancel_pending_order".to_string(),
            arguments: json!({"order_id": order, "reason": "no longer needed"}),
        };
        let request = confirm::request("jev-latest", proposal, customer, &call);
        let response = Response {
            model: "jev-test".to_string(),
            answers: BTreeMap::from([(confirm::QUESTION.to_string(), Answer::Noul { noul: p })]),
            usage: Default::default(),
        };
        ReplayCache::<MockOracle>::new(&cache, None)
            .insert(&stretto_oracle::request_key(&request), &response)
            .unwrap();
    };
    let offer = "I can cancel order #W7a because you no longer need it. Shall I go ahead?";
    let yes = "Yes, please go ahead.";
    let done = "Done: #W7a is cancelled. Anything else?";
    let also = "Also cancel #W7b.";
    seed(offer, yes, "#W7a", 0.92);
    seed(done, also, "#W7b", 0.15);

    for mode in ["enforce", "log"] {
        let logs = dir.join(format!("logs-{mode}"));
        let context = dir.join(format!("context-{mode}.jsonl"));
        let mut host = Host::start_without(
            &[
                "--record",
                logs.to_str().unwrap(),
                "--domain",
                "retail",
                "--guards",
                "--context",
                context.to_str().unwrap(),
                "--confirm-judge",
                mode,
                "--oracle",
                "replay",
                "--oracle-cache",
                cache.to_str().unwrap(),
                "--",
                DEMO,
                "--world",
                "retail",
            ],
            &["TYPESAFE_DEFAULT_MODEL"],
        );
        open_session(&mut host);
        say_to(
            &context,
            "Hi, I'm c7@example.com and I want to cancel two orders.",
        );
        host.call(
            3,
            "find_user_id_by_email",
            json!({"email": "c7@example.com"}),
        );
        host.call(4, "get_order_details", json!({"order_id": "#W7a"}));
        host.call(5, "get_order_details", json!({"order_id": "#W7b"}));

        // Confirmed: the judge says yes, so the write goes through.
        agent_says(&context, offer);
        say_to(&context, yes);
        let first = host.call(
            6,
            "cancel_pending_order",
            json!({"order_id": "#W7a", "reason": "no longer needed"}),
        );
        assert_eq!(first["isError"], false, "{first}");

        // A request, not a confirmation: the judge says no.
        agent_says(&context, done);
        say_to(&context, also);
        let second = host.call(
            7,
            "cancel_pending_order",
            json!({"order_id": "#W7b", "reason": "no longer needed"}),
        );
        if mode == "enforce" {
            assert_eq!(second["isError"], true, "{second}");
            let why = &texts(&second)[0];
            assert!(why.contains("confirmation judge: p_yes 0.15"), "{why}");

            // An exchange the cache has no answer for: the judge cannot
            // answer, so it refuses nothing. (In log mode #W7b is already
            // cancelled, and the policy check refuses before the judge asks.)
            agent_says(
                &context,
                "Shall I cancel order #W7b because you no longer need it?",
            );
            say_to(&context, "Yes, cancel it.");
            let third = host.call(
                8,
                "cancel_pending_order",
                json!({"order_id": "#W7b", "reason": "no longer needed"}),
            );
            assert_eq!(third["isError"], false, "{third}");
        } else {
            assert_eq!(second["isError"], false, "{second}");
        }
        assert_eq!(host.finish(), 0);

        let judged: Vec<Value> = fs::read_dir(&logs)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with(".confirm.jsonl"))
            .flat_map(|p| {
                fs::read_to_string(p)
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect::<Vec<Value>>()
            })
            .collect();
        assert_eq!(
            judged.len(),
            if mode == "enforce" { 3 } else { 2 },
            "{judged:?}"
        );
        assert_eq!(judged[0]["p_yes"], 0.92);
        assert_eq!(judged[0]["fails"], false);
        assert_eq!(judged[1]["p_yes"], 0.15);
        assert_eq!(judged[1]["fails"], true);
        assert_eq!(judged[1]["enforced"], mode == "enforce");
        if mode == "enforce" {
            assert!(judged[2]["error"].is_string(), "{}", judged[2]);
            assert_eq!(judged[2]["unknown"], true);
            assert_eq!(judged[2]["fails"], false);
        }
        // Every judgment names the question it asked.
        assert!(judged
            .iter()
            .all(|j| j["key"].as_str().is_some_and(|k| k.len() == 64)));
    }
    fs::remove_dir_all(&dir).unwrap();
}

/// Shadow mode: the flow decides and logs, the agent gets the server's
/// results unchanged, and `stretto promote` scores the logged session.
#[test]
fn a_shadow_flow_decides_and_logs_but_makes_no_lookups() {
    let dir = std::env::temp_dir().join(format!("stretto-shadow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let flow_path = learned_flow(&dir);
    let logs = dir.join("logs");
    let context = dir.join("context.jsonl");
    say_to(
        &context,
        "Hi, I'm c7@example.com and I want to cancel an order.",
    );
    let mut host = Host::start(&[
        "--record",
        logs.to_str().unwrap(),
        "--domain",
        "retail",
        "--flow",
        flow_path.to_str().unwrap(),
        "--oracle",
        "mock",
        "--flow-shadow",
        "--context",
        context.to_str().unwrap(),
        "--",
        DEMO,
        "--world",
        "retail",
    ]);
    open_session(&mut host);
    // The flow would read the user's details, but the agent gets only its
    // own result; then it reads them itself.
    let found = host.call(
        3,
        "find_user_id_by_email",
        json!({"email": "c7@example.com"}),
    );
    assert_eq!(texts(&found), ["user_7"]);
    let details = host.call(4, "get_user_details", json!({"user_id": "user_7"}));
    assert_eq!(texts(&details).len(), 1, "{details}");
    assert_eq!(host.finish(), 0);

    let flow_log = fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().ends_with(".flow.jsonl"))
        .expect("a flow log next to the session log");
    let decisions: Vec<Value> = fs::read_to_string(&flow_log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(decisions[0]["shadow"], true, "{decisions:?}");
    assert_eq!(decisions[0]["action"], "lookup");
    assert_eq!(decisions[0]["tool"], "get_user_details");
    assert_eq!(decisions[0]["arguments"], json!({"user_id": "user_7"}));

    // Scored on the recorded session, the would-be lookup was the agent's
    // next step.
    let session = fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            let name = p.to_string_lossy();
            name.ends_with(".jsonl") && !name.ends_with(".flow.jsonl")
        })
        .unwrap();
    let mut recorded = episode(&read_log(&session).unwrap());
    recorded.task_id = recorded.id.clone();
    let flow = stretto_report::flow::Flow::load(&flow_path).unwrap();
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let scored = stretto_report::promote::score(
        &flow,
        &[recorded],
        &oracle,
        stretto_report::flow::Decider::Arbiter,
        0.3,
    );
    let site = &scored.sites["find_user_id_by_email"];
    assert_eq!((site.lookups, site.used), (1, 1), "{scored:?}");
}
