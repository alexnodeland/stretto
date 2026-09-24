//! `stretto-mcp-demo`: a tiny MCP server for trying `stretto-proxy`.
//!
//! It speaks MCP over stdio (newline-delimited JSON-RPC) and has two tools
//! that store nothing and answer with their arguments as JSON text:
//!
//! - `lookup`, annotated read-only (`readOnlyHint: true`);
//! - `update`, annotated as not read-only (`readOnlyHint: false`), i.e. a
//!   write.
//!
//! `"fail": true` in the arguments makes either tool return an error result
//! (`isError: true`), and `"delay_ms": n` makes it wait `n` milliseconds
//! (at most 10 s) before answering, to try parallel calls.
//!
//! With `--world retail` it serves a tiny shop instead, with four of
//! τ²-bench retail's tool names and canned data, so that stretto's flows
//! and retail guards can be tried end to end:
//!
//! - `find_user_id_by_email`: `cN@example.com` is user `user_N`;
//! - `get_user_details`: user `user_N` has orders `#WNa` and `#WNb` and pays
//!   with `credit_card_N`;
//! - `get_order_details`: every order is pending;
//! - `cancel_pending_order`, a write: accepts only the reasons "no longer
//!   needed" and "ordered by mistake", as τ²-bench's tool does. Nothing is
//!   stored, so a cancelled order still reads as pending.
//!
//! It answers `initialize`, `ping`, `tools/list` and `tools/call`, rejects
//! other requests with "method not found", ignores notifications, and exits
//! when its input ends. It accepts whichever protocol version the client
//! asks for, since the little it implements is common to every revision.

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::time::Duration;

/// Protocol version to offer when the client names none.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

/// Which tools the server has.
#[derive(Clone, Copy, PartialEq, Eq)]
enum World {
    /// `lookup` and `update`, which echo their arguments.
    Echo,
    /// A tiny retail shop.
    Retail,
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["--world", "echo"] => World::Echo,
        ["--world", "retail"] => World::Retail,
        _ => {
            eprintln!("usage: stretto-mcp-demo [--world echo|retail]");
            std::process::exit(2);
        }
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&line);
        if text.trim().is_empty() {
            continue;
        }
        if let Some(response) = respond(&text, world) {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
}

/// The response to one line, if it needs one.
fn respond(line: &str, world: World) -> Option<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Some(error(Value::Null, -32700, "Parse error"));
    };
    if !message.is_object() {
        return Some(error(
            Value::Null,
            -32600,
            "Invalid Request: expected one JSON-RPC object",
        ));
    }
    // Notifications, and responses to requests this server never sends,
    // need no answer.
    let id = message.get("id").filter(|id| !id.is_null())?.clone();
    let method = message.get("method").and_then(Value::as_str)?;
    let params = message.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => json!({
            "protocolVersion": params.get("protocolVersion").cloned()
                .unwrap_or_else(|| json!(DEFAULT_PROTOCOL_VERSION)),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "stretto-mcp-demo", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "A demo server for stretto-proxy: two tools that echo their arguments."
        }),
        "ping" => json!({}),
        "tools/list" if world == World::Retail => json!({"tools": retail::tools()}),
        "tools/list" => json!({"tools": [
            tool("lookup", "Look something up. Read-only; answers with its arguments.", true),
            tool("update", "Pretend to change something; answers with its arguments.", false),
        ]}),
        "tools/call" if world == World::Retail => return Some(retail::call(id, params)),
        "tools/call" => return Some(call_tool(id, params)),
        other => return Some(error(id, -32601, &format!("Method not found: {other}"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// A tool definition.
fn tool(name: &str, description: &str, read_only: bool) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Anything; echoed back."},
                "fail": {"type": "boolean", "description": "Return an error result instead."},
                "delay_ms": {
                    "type": "integer", "minimum": 0,
                    "description": "Wait this long before answering (at most 10 000)."
                }
            }
        },
        "annotations": {"readOnlyHint": read_only}
    })
}

/// Answer a `tools/call`.
fn call_tool(id: Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if name != "lookup" && name != "update" {
        return error(id, -32602, &format!("Unknown tool: {name}"));
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(ms) = arguments.get("delay_ms").and_then(Value::as_u64) {
        std::thread::sleep(Duration::from_millis(ms.min(10_000)));
    }
    let failed = arguments.get("fail").and_then(Value::as_bool) == Some(true);
    let text = if failed {
        format!("{name} failed, as asked: {arguments}")
    } else {
        arguments.to_string()
    };
    json!({"jsonrpc": "2.0", "id": id, "result": {
        "content": [{"type": "text", "text": text}],
        "isError": failed
    }})
}

/// A JSON-RPC error response.
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The retail world.
mod retail {
    use super::error;
    use serde_json::{json, Value};

    fn tool(name: &str, description: &str, read_only: bool, arguments: &[&str]) -> Value {
        let properties: serde_json::Map<String, Value> = arguments
            .iter()
            .map(|a| (a.to_string(), json!({"type": "string"})))
            .collect();
        json!({
            "name": name,
            "description": description,
            "inputSchema": {"type": "object", "properties": properties, "required": arguments},
            "annotations": {"readOnlyHint": read_only}
        })
    }

    pub(super) fn tools() -> Vec<Value> {
        vec![
            tool(
                "find_user_id_by_email",
                "Find a user id by email.",
                true,
                &["email"],
            ),
            tool(
                "get_user_details",
                "Get a user's details, including their orders.",
                true,
                &["user_id"],
            ),
            tool(
                "get_order_details",
                "Get the status and details of an order.",
                true,
                &["order_id"],
            ),
            tool(
                "cancel_pending_order",
                "Cancel a pending order. The reason is 'no longer needed' or 'ordered by mistake'.",
                false,
                &["order_id", "reason"],
            ),
        ]
    }

    /// The `N` in `prefix` + `N` + `suffix`.
    fn number<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
        let n = text.strip_prefix(prefix)?.strip_suffix(suffix)?;
        (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(n)
    }

    /// The order's user number, for `#W<N>a` or `#W<N>b`.
    fn order_user(order: &str) -> Option<&str> {
        number(order, "#W", "a").or_else(|| number(order, "#W", "b"))
    }

    fn answer(name: &str, a: &Value) -> Result<Value, String> {
        let arg = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default();
        match name {
            "find_user_id_by_email" => number(arg("email"), "c", "@example.com")
                .map(|n| json!(format!("user_{n}")))
                .ok_or_else(|| "User not found".to_string()),
            "get_user_details" => number(arg("user_id"), "user_", "")
                .map(|n| {
                    json!({
                        "user_id": format!("user_{n}"),
                        "email": format!("c{n}@example.com"),
                        "orders": [format!("#W{n}a"), format!("#W{n}b")],
                        "payment_methods": {format!("credit_card_{n}"): {"source": "credit_card"}}
                    })
                })
                .ok_or_else(|| "User not found".to_string()),
            "get_order_details" => order_user(arg("order_id"))
                .map(|n| {
                    json!({
                        "order_id": arg("order_id"),
                        "user_id": format!("user_{n}"),
                        "status": "pending",
                        "items": [{"item_id": "1", "product_id": "p1", "name": "Desk lamp"}],
                        "payment_history": [{"payment_method_id": format!("credit_card_{n}")}]
                    })
                })
                .ok_or_else(|| "Order not found".to_string()),
            "cancel_pending_order" => {
                if order_user(arg("order_id")).is_none() {
                    Err("Order not found".to_string())
                } else if !matches!(arg("reason"), "no longer needed" | "ordered by mistake") {
                    Err("Invalid reason".to_string())
                } else {
                    Ok(json!({"order_id": arg("order_id"), "status": "cancelled"}))
                }
            }
            other => Err(format!("Unknown tool: {other}")),
        }
    }

    pub(super) fn call(id: Value, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        if !tools().iter().any(|t| t["name"] == name) {
            return error(id, -32602, &format!("Unknown tool: {name}"));
        }
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let (text, failed) = match answer(name, &arguments) {
            Ok(Value::String(s)) => (s, false),
            Ok(v) => (v.to_string(), false),
            Err(e) => (format!("Error: {e}"), true),
        };
        json!({"jsonrpc": "2.0", "id": id, "result": {
            "content": [{"type": "text", "text": text}],
            "isError": failed
        }})
    }
}
