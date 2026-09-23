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
//! It answers `initialize`, `ping`, `tools/list` and `tools/call`, rejects
//! other requests with "method not found", ignores notifications, and exits
//! when its input ends. It accepts whichever protocol version the client
//! asks for, since the little it implements is common to every revision.

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::time::Duration;

/// Protocol version to offer when the client names none.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

fn main() -> io::Result<()> {
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
        if let Some(response) = respond(&text) {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
}

/// The response to one line, if it needs one.
fn respond(line: &str) -> Option<Value> {
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
        "tools/list" => json!({"tools": [
            tool("lookup", "Look something up. Read-only; answers with its arguments.", true),
            tool("update", "Pretend to change something; answers with its arguments.", false),
        ]}),
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
