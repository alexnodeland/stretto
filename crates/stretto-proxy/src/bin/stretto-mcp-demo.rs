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
//!
//! With `--http ADDR` (such as `127.0.0.1:0`) it serves the same tools over
//! Streamable HTTP instead, at `/mcp`, and prints its URL on stdout. It is
//! strict, to test clients: every message after `initialize` must carry the
//! session id it assigned and the protocol version it agreed. A tool call is
//! answered on an event stream, a log message first; everything else as one
//! JSON body. A GET stream sends one log message of the server's own and
//! stays open until the session is deleted. With `--require-auth VALUE`,
//! a request without `Authorization: VALUE` is refused (401).

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
    let (mut world, mut http, mut auth) = (Some(World::Echo), None, None);
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        match (arg, rest.next()) {
            ("--world", Some("echo")) => world = Some(World::Echo),
            ("--world", Some("retail")) => world = Some(World::Retail),
            ("--http", Some(addr)) => http = Some(addr.to_string()),
            ("--require-auth", Some(value)) => auth = Some(value.to_string()),
            _ => world = None,
        }
    }
    let Some(world) = world else {
        eprintln!(
            "usage: stretto-mcp-demo [--world echo|retail] [--http ADDR [--require-auth VALUE]]"
        );
        std::process::exit(2);
    };
    if let Some(addr) = http {
        return http::serve(&addr, world, auth);
    }
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

/// Streamable HTTP: the same answers, over MCP's HTTP transport.
mod http {
    use super::{respond, World, DEFAULT_PROTOCOL_VERSION};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Live sessions, by id, with the protocol version agreed.
    #[derive(Default)]
    struct Sessions {
        next: u64,
        live: HashMap<String, String>,
    }

    struct Request {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    pub(super) fn serve(addr: &str, world: World, auth: Option<String>) -> io::Result<()> {
        let listener = TcpListener::bind(addr)?;
        println!("http://{}/mcp", listener.local_addr()?);
        io::stdout().flush()?;
        let sessions = Arc::new(Mutex::new(Sessions::default()));
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (sessions, auth) = (sessions.clone(), auth.clone());
            std::thread::spawn(move || {
                let _ = handle(stream, world, &sessions, auth.as_deref());
            });
        }
        Ok(())
    }

    fn read_request(stream: &TcpStream) -> io::Result<Request> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let mut parts = line.split_whitespace();
        let (method, path) = (
            parts.next().unwrap_or_default().to_string(),
            parts.next().unwrap_or_default().to_string(),
        );
        let mut headers = HashMap::new();
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 || header.trim_end().is_empty() {
                break;
            }
            if let Some((name, value)) = header.trim_end().split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let length = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        Ok(Request {
            method,
            path,
            headers,
            body,
        })
    }

    fn reply(
        mut stream: &TcpStream,
        status: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        )?;
        for (name, value) in headers {
            write!(stream, "{name}: {value}\r\n")?;
        }
        write!(stream, "\r\n")?;
        stream.write_all(body)?;
        stream.flush()
    }

    fn open_events(mut stream: &TcpStream) -> io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
        )?;
        stream.flush()
    }

    fn event(mut stream: &TcpStream, message: &Value) -> io::Result<()> {
        write!(stream, "event: message\ndata: {message}\n\n")?;
        stream.flush()
    }

    fn log_message(text: &str) -> Value {
        json!({"jsonrpc": "2.0", "method": "notifications/message",
               "params": {"level": "info", "data": text}})
    }

    fn handle(
        stream: TcpStream,
        world: World,
        sessions: &Mutex<Sessions>,
        auth: Option<&str>,
    ) -> io::Result<()> {
        let request = read_request(&stream)?;
        if request.path.split('?').next() != Some("/mcp") {
            return reply(&stream, "404 Not Found", &[], b"");
        }
        if auth.is_some_and(|a| request.headers.get("authorization").map(String::as_str) != Some(a))
        {
            return reply(&stream, "401 Unauthorized", &[], b"");
        }
        let session = request.headers.get("mcp-session-id").cloned();
        let agreed = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| sessions.lock().unwrap().live.get(id).cloned())
        };
        match request.method.as_str() {
            "POST" => {
                let text = String::from_utf8_lossy(&request.body).to_string();
                let message: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let method = message.get("method").and_then(Value::as_str);
                if method == Some("initialize") {
                    let Some(answer) = respond(&text, world) else {
                        return reply(&stream, "400 Bad Request", &[], b"");
                    };
                    let version = answer["result"]["protocolVersion"]
                        .as_str()
                        .unwrap_or(DEFAULT_PROTOCOL_VERSION)
                        .to_string();
                    let id = {
                        let mut s = sessions.lock().unwrap();
                        s.next += 1;
                        let id = format!("demo-session-{}", s.next);
                        s.live.insert(id.clone(), version);
                        id
                    };
                    return reply(
                        &stream,
                        "200 OK",
                        &[
                            ("Content-Type", "application/json"),
                            ("Mcp-Session-Id", &id),
                        ],
                        answer.to_string().as_bytes(),
                    );
                }
                // Everything later names a live session and its version.
                let Some(version) = agreed(&session) else {
                    let status = if session.is_some() {
                        "404 Not Found"
                    } else {
                        "400 Bad Request"
                    };
                    return reply(&stream, status, &[], b"");
                };
                if request.headers.get("mcp-protocol-version") != Some(&version) {
                    return reply(&stream, "400 Bad Request", &[], b"");
                }
                match respond(&text, world) {
                    None => reply(&stream, "202 Accepted", &[], b""),
                    Some(answer) if method == Some("tools/call") => {
                        open_events(&stream)?;
                        let name = message["params"]["name"].as_str().unwrap_or_default();
                        event(&stream, &log_message(&format!("calling {name}")))?;
                        event(&stream, &answer)
                    }
                    Some(answer) => reply(
                        &stream,
                        "200 OK",
                        &[("Content-Type", "application/json")],
                        answer.to_string().as_bytes(),
                    ),
                }
            }
            "GET" => {
                if agreed(&session).is_none() {
                    return reply(&stream, "404 Not Found", &[], b"");
                }
                open_events(&stream)?;
                event(&stream, &log_message("hello from the server's own stream"))?;
                let started = Instant::now();
                while agreed(&session).is_some() && started.elapsed() < Duration::from_secs(60) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(())
            }
            "DELETE" => {
                if let Some(id) = &session {
                    sessions.lock().unwrap().live.remove(id);
                }
                reply(&stream, "200 OK", &[], b"")
            }
            _ => reply(
                &stream,
                "405 Method Not Allowed",
                &[("Allow", "GET, POST, DELETE")],
                b"",
            ),
        }
    }
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
