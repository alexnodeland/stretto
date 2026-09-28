//! A miniature τ²-bench checkout, and a shop's sessions as stretto-proxy
//! records them, so tests need no data.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

pub const TOOLS_PY: &str = r#"
    @is_tool(ToolType.READ)
    def find_user(self, email: str) -> str:
        ...
    @is_tool(ToolType.READ)
    def get_order(self, order_id: str) -> Order:
        ...
    @is_tool(ToolType.WRITE)
    def cancel_order(self, order_id: str, reason: str) -> Order:
        ...
"#;

/// One simulation: find the user, look up the order, confirm, cancel.
pub fn simulation(id: usize, task: usize, confirm: &str) -> serde_json::Value {
    let order = format!("#W{task:04}");
    serde_json::json!({
        "id": format!("s{id}"), "task_id": task.to_string(), "trial": id % 2,
        "reward_info": {"reward": if id.is_multiple_of(5) { 0.0 } else { 1.0 }},
        "messages": [
            {"role": "assistant", "content": "Hi! How can I help you today?", "cost": 0.0},
            {"role": "user", "content": format!("Cancel {order}, I am a@b.com")},
            {"role": "assistant", "cost": 0.01, "usage": usage(1000), "tool_calls": [
                {"id": format!("{id}a"), "name": "find_user", "arguments": {"email": "a@b.com"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}a"), "content": "\"u_1\"", "error": false},
            {"role": "assistant", "cost": 0.01, "usage": usage(1110), "tool_calls": [
                {"id": format!("{id}b"), "name": "get_order", "arguments": {"order_id": order},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}b"),
             "content": format!("{{\"order_id\": \"{order}\", \"status\": \"pending\"}}"),
             "error": false},
            {"role": "assistant", "content": "Cancel it? (yes/no)", "cost": 0.01,
             "usage": usage(1220)},
            {"role": "user", "content": confirm},
            {"role": "assistant", "cost": 0.01, "usage": usage(1330), "tool_calls": [
                {"id": format!("{id}c"), "name": "cancel_order",
                 "arguments": {"order_id": order, "reason": "no longer needed"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}c"), "content": "{\"status\": \"cancelled\"}",
             "error": false},
            {"role": "assistant", "content": "Done.", "cost": 0.01, "usage": usage(1440)}
        ]
    })
}

pub fn usage(prompt_tokens: u64) -> serde_json::Value {
    serde_json::json!({"prompt_tokens": prompt_tokens, "completion_tokens": 10})
}

/// A results file for `model` in `domain`.
pub fn results_file(model: &str, domain: &str, user: &str) -> serde_json::Value {
    let sims: Vec<_> = (0..16)
        .map(|i| {
            simulation(
                i,
                i / 2,
                if i.is_multiple_of(3) {
                    "go ahead"
                } else {
                    "yes"
                },
            )
        })
        .collect();
    serde_json::json!({
        "info": {
            "agent_info": {"llm": model},
            "user_info": {"llm": user},
            "environment_info": {"domain_name": domain, "policy": "p", "tool_defs": null}
        },
        "tasks": [],
        "simulations": sims
    })
}

pub fn write_checkout(root: &Path) {
    let domain_src = root.join("src/tau2/domains/retail");
    let domain_data = root.join("data/tau2/domains/retail");
    let results = root.join("data/tau2/results/final");
    for d in [&domain_src, &domain_data, &results] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(domain_src.join("tools.py"), TOOLS_PY).unwrap();
    fs::write(
        domain_data.join("split_tasks.json"),
        r#"{"train": ["0", "1", "2", "3", "4", "5"], "test": ["6", "7"], "base": []}"#,
    )
    .unwrap();
    for model in ["model-a", "model-b"] {
        fs::write(
            results.join(format!("{model}_retail_default_user-sim_2trials.json")),
            serde_json::to_vec(&results_file(model, "retail", "user-sim")).unwrap(),
        )
        .unwrap();
    }
    // Transfer targets outside the checkout: one for this domain, one not.
    let targets = root.join("targets");
    fs::create_dir_all(&targets).unwrap();
    for domain in ["retail", "airline"] {
        fs::write(
            targets.join(format!("model-t_{domain}.json")),
            serde_json::to_vec(&results_file("vendor/model-t", domain, "other-sim")).unwrap(),
        )
        .unwrap();
    }
}

// A shop's sessions, as stretto-proxy records them.

/// The shop's tools.
pub fn manifest() -> ToolManifest {
    ToolManifest {
        domain: "shop".to_string(),
        tools: BTreeMap::from([
            ("find_account".to_string(), ToolKind::Read),
            ("get_account".to_string(), ToolKind::Read),
            ("get_order".to_string(), ToolKind::Read),
            ("close_order".to_string(), ToolKind::Write),
        ]),
        docs: BTreeMap::new(),
    }
}

pub fn call(id: &str, name: &str, arguments: Value) -> Event {
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

pub fn result(id: &str, name: &str, content: &str) -> Event {
    Event::ToolResult {
        call_id: id.to_string(),
        name: name.to_string(),
        error: false,
        content: content.to_string(),
    }
}

pub fn say(text: &str) -> Event {
    Event::Assistant {
        text: Some(text.to_string()),
        calls: Vec::new(),
        usage: None,
    }
}

/// Customer `i` finds their account, the agent reads it and each of its
/// orders, then closes the first once the customer agrees.
pub fn session(i: usize) -> Episode {
    let account = format!("acct_{i}");
    let orders = [format!("o{i}a"), format!("o{i}b")];
    Episode {
        id: format!("session-{i}"),
        task_id: String::new(),
        trial: 0,
        domain: "shop".to_string(),
        agent_model: "agent".to_string(),
        reward: 1.0,
        events: vec![
            Event::User {
                text: format!("Hi, I'm c{i}@example.com and I want to close an order."),
            },
            call(
                "1",
                "find_account",
                json!({"email": format!("c{i}@example.com")}),
            ),
            result("1", "find_account", &account),
            call("2", "get_account", json!({"account_id": account})),
            result(
                "2",
                "get_account",
                &json!({"account_id": account, "orders": orders}).to_string(),
            ),
            call("3", "get_order", json!({"order_id": orders[0]})),
            result(
                "3",
                "get_order",
                &json!({"order_id": orders[0], "status": "open"}).to_string(),
            ),
            call("4", "get_order", json!({"order_id": orders[1]})),
            result(
                "4",
                "get_order",
                &json!({"order_id": orders[1], "status": "open"}).to_string(),
            ),
            say("You have two open orders. Close the first?"),
            Event::User {
                text: "Yes, please.".to_string(),
            },
            call("5", "close_order", json!({"order_id": orders[0]})),
            result("5", "close_order", "closed"),
            say("Done."),
        ],
    }
}

/// Session `i`, where the agent also reads the account's rewards before
/// its orders.
pub fn session_with_rewards(i: usize) -> Episode {
    let mut ep = session(i);
    let account = format!("acct_{i}");
    ep.events.splice(
        5..5,
        [
            call("r", "get_rewards", json!({"account_id": account})),
            result(
                "r",
                "get_rewards",
                &json!({"account_id": account, "points": 10}).to_string(),
            ),
        ],
    );
    ep
}

/// `episode` as the session log stretto-proxy would write, named to sort
/// `n`th: each call a second after the last answer, an LLM turn of its own.
pub fn session_log(episode: &Episode, n: usize) -> (String, String) {
    let session = format!("20260928T{n:06}.000Z-{}", episode.id);
    let header = json!({"stretto_mcp_log": 2, "session": session,
        "started_unix_ms": 1_790_000_000_000u64 + n as u64 * 60_000,
        "server_command": ["shop"], "domain": null, "agent_model": null});
    let mut lines = vec![header.to_string()];
    let (mut t, mut ids) = (0, BTreeMap::new());
    for e in &episode.events {
        match e {
            Event::Assistant { calls, .. } => {
                for c in calls {
                    t += 1000;
                    let id = ids.len() + 1;
                    ids.insert(c.id.clone(), id);
                    lines.push(
                        json!({"t_ms": t, "from": "client", "message": {"jsonrpc": "2.0",
                            "id": id, "method": "tools/call",
                            "params": {"name": c.name, "arguments": c.arguments}}})
                        .to_string(),
                    );
                }
            }
            Event::ToolResult {
                call_id,
                content,
                error,
                ..
            } => {
                t += 10;
                lines.push(
                    json!({"t_ms": t, "from": "server", "message": {"jsonrpc": "2.0",
                        "id": ids[call_id],
                        "result": {"content": [{"type": "text", "text": content}], "isError": error}}})
                    .to_string(),
                );
            }
            _ => {}
        }
    }
    (format!("{session}.jsonl"), lines.join("\n") + "\n")
}

/// `episodes` as session logs in `dir`, in their order.
pub fn write_sessions(dir: &std::path::Path, episodes: &[Episode]) {
    std::fs::create_dir_all(dir).unwrap();
    for (n, ep) in episodes.iter().enumerate() {
        let (name, text) = session_log(ep, n);
        std::fs::write(dir.join(name), text).unwrap();
    }
}
