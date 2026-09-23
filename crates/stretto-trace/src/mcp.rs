//! Logs recorded by `stretto-proxy`, a stdio MCP proxy.
//!
//! The proxy sits between an MCP host, where the agent runs, and one MCP
//! server. It forwards every line unchanged and records it in a JSONL log: a
//! [`LogHeader`], then one [`LogEntry`] per line, in the order it read them.
//!
//! ```text
//! {"stretto_mcp_log":1,"session":"20260923T212000.000Z-4242","started_unix_ms":1790198400000,"server_command":["npx","some-mcp-server"],"domain":null,"agent_model":null}
//! {"t_ms":3,"from":"client","message":{"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}}
//! {"t_ms":41,"from":"server","message":{"jsonrpc":"2.0","id":1,"result":{…}}}
//! {"t_ms":42,"from":"server","raw":"listening on stdio"}
//! ```
//!
//! `message` is the line as JSON: a JSON-RPC message, or an array of them (a
//! batch). A line that was not valid JSON is kept as `raw` text instead.
//! [`episode`] turns a log into an [`Episode`], and [`manifest`] reads the
//! tools the server listed.
//!
//! The proxy sees only MCP traffic, so an episode from a log is a partial
//! view:
//!
//! - **No conversation.** The user's messages and the LLM's replies never
//!   reach an MCP server, so there are no [`Event::User`] events, and
//!   assistant turns have tool calls but no text.
//! - **Inferred turns.** A tool call sent while an earlier call of the
//!   current turn still awaits its response joins that turn (parallel
//!   calls); any other call starts a new turn. A host that runs one LLM
//!   turn's calls one at a time therefore shows one turn per call, and a
//!   call that is never answered (nor cancelled) keeps its turn open.
//! - **No usage or outcome.** Token usage is unknown, and so is whether the
//!   task succeeded: `reward` is `0.0` unless the caller sets it.
//! - **One server.** Each wrapped server has its own log.

use crate::{Episode, Event, ToolCall, ToolKind, ToolManifest};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// The log format version this module reads and `stretto-proxy` writes.
pub const LOG_VERSION: u32 = 1;

/// The first line of a log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogHeader {
    /// Format version; see [`LOG_VERSION`].
    pub stretto_mcp_log: u32,
    /// Session id, unique per proxy run. The proxy names the log file after it.
    pub session: String,
    /// When the proxy started, in milliseconds since the Unix epoch.
    pub started_unix_ms: u64,
    /// The server's command line, with values that look like credentials
    /// redacted.
    #[serde(default)]
    pub server_command: Vec<String>,
    /// The domain the proxy was given, if any.
    pub domain: Option<String>,
    /// The model the proxy was told drives the agent, if any.
    pub agent_model: Option<String>,
}

/// Which side of the connection sent a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Peer {
    /// The MCP client, i.e. the host the agent runs in.
    Client,
    /// The MCP server.
    Server,
}

/// One forwarded line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// When the proxy read the line, in milliseconds after it started.
    pub t_ms: u64,
    /// Who sent the line.
    pub from: Peer,
    /// The line, when it was valid JSON: a JSON-RPC message or a batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Value>,
    /// The line as text, when it was not valid JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

/// A parsed log.
#[derive(Clone, Debug, PartialEq)]
pub struct McpLog {
    /// The first line.
    pub header: LogHeader,
    /// The lines after it, in log order.
    pub entries: Vec<LogEntry>,
}

impl McpLog {
    /// Every JSON-RPC message with its sender, in log order. Batches are
    /// flattened and raw lines skipped.
    pub fn messages(&self) -> impl Iterator<Item = (Peer, &Value)> {
        self.entries.iter().flat_map(|e| {
            let items: &[Value] = match &e.message {
                Some(Value::Array(batch)) => batch,
                Some(message) => std::slice::from_ref(message),
                None => &[],
            };
            items.iter().map(move |m| (e.from, m))
        })
    }
}

/// Read and parse a log file.
pub fn read_log(path: &Path) -> Result<McpLog> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_log(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Parse the text of a log.
///
/// Blank lines are ignored. A last line that lacks its newline and does not
/// parse, as a crash mid-write could leave, is dropped; any other malformed
/// line is an error.
pub fn parse_log(text: &str) -> Result<McpLog> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| (i + 1, line))
        .collect();
    let Some((&(n, first), rest)) = lines.split_first() else {
        bail!("empty log: no header line");
    };
    let header: LogHeader = serde_json::from_str(first)
        .with_context(|| format!("line {n}: not a stretto MCP log header"))?;
    ensure!(
        header.stretto_mcp_log == LOG_VERSION,
        "unsupported log version {} (this build reads version {LOG_VERSION})",
        header.stretto_mcp_log
    );

    let truncated = !text.ends_with('\n');
    let mut entries = Vec::with_capacity(rest.len());
    for (k, &(n, line)) in rest.iter().enumerate() {
        match serde_json::from_str::<LogEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(_) if truncated && k + 1 == rest.len() => {}
            Err(e) => return Err(e).with_context(|| format!("line {n}")),
        }
    }
    Ok(McpLog { header, entries })
}

/// The tools the server listed, with kinds from their annotations.
///
/// Every `tools/list` response, matched to the client's request by JSON-RPC
/// id, adds its tools; when a tool is listed again, the later listing wins.
/// A tool's kind comes from its `readOnlyHint` annotation: `true` is
/// [`ToolKind::Read`], `false` is [`ToolKind::Write`], and no hint is
/// [`ToolKind::Generic`], which here means *unknown*. MCP itself tells
/// clients to assume an unannotated tool may write, and
/// [`ToolManifest::is_write`] is false for it, so treat `Generic` with care.
/// Annotations are the server's own claims, not guarantees.
pub fn manifest(log: &McpLog, domain: &str) -> ToolManifest {
    let mut awaiting = HashSet::new();
    let mut tools = BTreeMap::new();
    for (from, m) in log.messages() {
        match from {
            Peer::Client => {
                if method(m) == Some("tools/list") {
                    if let Some(id) = request_id(m) {
                        awaiting.insert(id_key(id));
                    }
                }
            }
            Peer::Server => {
                let Some(id) = response_id(m) else { continue };
                if !awaiting.remove(&id_key(id)) {
                    continue;
                }
                let listed = m.pointer("/result/tools").and_then(Value::as_array);
                for tool in listed.into_iter().flatten() {
                    if let Some(name) = tool.get("name").and_then(Value::as_str) {
                        tools.insert(name.to_string(), tool_kind(tool));
                    }
                }
            }
        }
    }
    ToolManifest {
        domain: domain.to_string(),
        tools,
    }
}

/// Convert a log into an [`Episode`].
///
/// - Each client `tools/call` request becomes a [`ToolCall`]. Its id is the
///   JSON-RPC id (a string as is, a number in decimal), and its arguments
///   are `{}` when the request has none.
/// - Calls are grouped into [`Event::Assistant`] turns as the module docs
///   describe. A turn's event sits where its first call was sent.
/// - Each server response to a call becomes an [`Event::ToolResult`]. It is
///   an error when the response is a JSON-RPC error, whose message becomes
///   the content, or when its result has `isError: true`. Otherwise the
///   content is the `text` of the result's text items, joined with newlines;
///   without any, it is `structuredContent`, else the whole result, as JSON.
/// - A call the client cancels (`notifications/cancelled`) stops holding its
///   turn open, and gets a result only if the server answers anyway. A call
///   still unanswered when the log ends has no result.
///
/// Events are in log order. The episode's id is the session id; its domain
/// is the header's, else `mcp`; its agent model is the header's, else the
/// `clientInfo.name` the client sent in `initialize` (a stand-in: it names
/// the host application, not the LLM), else `unknown`. A log does not know
/// the task, trial or outcome, so `task_id` is empty, `trial` is 0 and
/// `reward` is 0.0; set them where they matter.
pub fn episode(log: &McpLog) -> Episode {
    let mut events = Vec::new();
    let mut pending: HashMap<String, Pending> = HashMap::new();
    // The current turn's index in `events`, and how many of its calls still
    // await a response. A new turn starts only when none do, so every call
    // that holds a turn open belongs to the current one.
    let mut turn = 0;
    let mut open = 0;
    let mut client_name = None;

    for (from, m) in log.messages() {
        match from {
            Peer::Client => match method(m) {
                Some("initialize") if client_name.is_none() => {
                    client_name = m
                        .pointer("/params/clientInfo/name")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                Some("tools/call") => {
                    let name = m.pointer("/params/name").and_then(Value::as_str);
                    let (Some(id), Some(name)) = (request_id(m), name) else {
                        continue;
                    };
                    // A reused id replaces the earlier call, which can no
                    // longer be told apart from this one.
                    if pending.remove(&id_key(id)).is_some_and(|p| p.holds_turn) {
                        open -= 1;
                    }
                    if open == 0 {
                        events.push(Event::Assistant {
                            text: None,
                            calls: Vec::new(),
                            usage: None,
                        });
                        turn = events.len() - 1;
                    }
                    if let Some(Event::Assistant { calls, .. }) = events.get_mut(turn) {
                        calls.push(ToolCall {
                            id: id_string(id),
                            name: name.to_string(),
                            arguments: m
                                .pointer("/params/arguments")
                                .cloned()
                                .unwrap_or_else(|| json!({})),
                        });
                    }
                    open += 1;
                    pending.insert(
                        id_key(id),
                        Pending {
                            call_id: id_string(id),
                            name: name.to_string(),
                            holds_turn: true,
                        },
                    );
                }
                Some("notifications/cancelled") => {
                    let cancelled = m.pointer("/params/requestId").map(id_key);
                    if let Some(p) = cancelled.and_then(|key| pending.get_mut(&key)) {
                        if std::mem::take(&mut p.holds_turn) {
                            open -= 1;
                        }
                    }
                }
                _ => {}
            },
            Peer::Server => {
                let Some(id) = response_id(m) else { continue };
                let Some(call) = pending.remove(&id_key(id)) else {
                    continue;
                };
                if call.holds_turn {
                    open -= 1;
                }
                let (error, content) = outcome(m);
                events.push(Event::ToolResult {
                    call_id: call.call_id,
                    name: call.name,
                    error,
                    content,
                });
            }
        }
    }

    Episode {
        id: log.header.session.clone(),
        task_id: String::new(),
        trial: 0,
        domain: log
            .header
            .domain
            .clone()
            .unwrap_or_else(|| "mcp".to_string()),
        agent_model: log
            .header
            .agent_model
            .clone()
            .or(client_name)
            .unwrap_or_else(|| "unknown".to_string()),
        reward: 0.0,
        events,
    }
}

/// A tool call awaiting its response.
struct Pending {
    call_id: String,
    name: String,
    /// Whether it keeps its turn open; false once the client cancels it.
    holds_turn: bool,
}

/// The method of a request or notification.
fn method(m: &Value) -> Option<&str> {
    m.get("method").and_then(Value::as_str)
}

/// The id of a request: a message with a method and an id.
fn request_id(m: &Value) -> Option<&Value> {
    method(m)?;
    m.get("id").filter(|id| !id.is_null())
}

/// The id of a response: a message with an id and a result or an error, and
/// no method.
fn response_id(m: &Value) -> Option<&Value> {
    let answers = m.get("result").is_some() || m.get("error").is_some_and(|e| !e.is_null());
    if m.get("method").is_some() || !answers {
        return None;
    }
    m.get("id").filter(|id| !id.is_null())
}

/// Key for matching a response to its request: the id as JSON, so the number
/// `1` and the string `"1"` stay distinct.
fn id_key(id: &Value) -> String {
    id.to_string()
}

/// The id as [`ToolCall::id`] holds it: a string as is, anything else as JSON.
fn id_string(id: &Value) -> String {
    match id {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A tool's kind, from its `readOnlyHint` annotation.
fn tool_kind(tool: &Value) -> ToolKind {
    match tool
        .pointer("/annotations/readOnlyHint")
        .and_then(Value::as_bool)
    {
        Some(true) => ToolKind::Read,
        Some(false) => ToolKind::Write,
        None => ToolKind::Generic,
    }
}

/// Whether a `tools/call` response reports an error, and its content as text.
fn outcome(response: &Value) -> (bool, String) {
    if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
        let content = match error.get("message").and_then(Value::as_str) {
            Some(message) => message.to_string(),
            None => error.to_string(),
        };
        return (true, content);
    }
    let result = &response["result"];
    let error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let texts: Vec<&str> = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect();
    let content = if !texts.is_empty() {
        texts.join("\n")
    } else if let Some(structured) = result.get("structuredContent") {
        structured.to_string()
    } else {
        result.to_string()
    };
    (error, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Peer::{Client, Server};

    const HEADER: &str = r#"{"stretto_mcp_log":1,"session":"s1","started_unix_ms":1790198400000,"server_command":["demo-server","--verbose"],"domain":null,"agent_model":null}"#;

    /// A log of `lines`, 10 ms apart.
    fn log_of(lines: &[(Peer, Value)]) -> McpLog {
        let mut text = format!("{HEADER}\n");
        for (i, (from, message)) in lines.iter().enumerate() {
            let entry = LogEntry {
                t_ms: 10 * i as u64,
                from: *from,
                message: Some(message.clone()),
                raw: None,
            };
            text.push_str(&serde_json::to_string(&entry).unwrap());
            text.push('\n');
        }
        parse_log(&text).unwrap()
    }

    fn call(id: Value, name: &str) -> (Peer, Value) {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                             "params": {"name": name, "arguments": {"n": 1}}});
        (Client, message)
    }

    fn reply(id: Value, text: &str) -> (Peer, Value) {
        let message = json!({"jsonrpc": "2.0", "id": id,
                             "result": {"content": [{"type": "text", "text": text}]}});
        (Server, message)
    }

    /// Each event as a short string: `turn(a,b)` or `result:<call id>`.
    fn outline(ep: &Episode) -> Vec<String> {
        ep.events
            .iter()
            .map(|e| match e {
                Event::Assistant { calls, .. } => {
                    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
                    format!("turn({})", names.join(","))
                }
                Event::ToolResult { call_id, .. } => format!("result:{call_id}"),
                Event::User { .. } => "user".to_string(),
            })
            .collect()
    }

    /// `(call id, error, content)` of each result.
    fn results(ep: &Episode) -> Vec<(&str, bool, &str)> {
        ep.events
            .iter()
            .filter_map(|e| match e {
                Event::ToolResult {
                    call_id,
                    error,
                    content,
                    ..
                } => Some((call_id.as_str(), *error, content.as_str())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_the_header_and_entries() {
        let text = [
            HEADER,
            r#"{"t_ms":0,"from":"client","message":{"jsonrpc":"2.0","method":"notifications/initialized"}}"#,
            "",
            r#"{"t_ms":5,"from":"server","raw":"server starting"}"#,
            "",
        ]
        .join("\n");
        let log = parse_log(&text).unwrap();
        assert_eq!(log.header.session, "s1");
        assert_eq!(log.header.started_unix_ms, 1_790_198_400_000);
        assert_eq!(log.header.server_command, ["demo-server", "--verbose"]);
        assert_eq!(
            (
                log.header.domain.as_deref(),
                log.header.agent_model.as_deref()
            ),
            (None, None)
        );
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.entries[1].from, Server);
        assert_eq!(log.entries[1].raw.as_deref(), Some("server starting"));
        assert_eq!(log.messages().count(), 1);
    }

    #[test]
    fn rejects_what_is_not_a_log() {
        assert!(parse_log("").is_err());
        assert!(parse_log("{\"jsonrpc\":\"2.0\",\"id\":1}\n").is_err());
        let v2 = HEADER.replace("\"stretto_mcp_log\":1", "\"stretto_mcp_log\":2");
        assert!(format!("{:#}", parse_log(&v2).unwrap_err()).contains("version 2"));
        let bad =
            format!("{HEADER}\nnot an entry\n{{\"t_ms\":1,\"from\":\"client\",\"raw\":\"x\"}}\n");
        assert!(format!("{:#}", parse_log(&bad).unwrap_err()).contains("line 2"));
    }

    #[test]
    fn drops_a_last_line_cut_off_mid_write() {
        let text = format!(
            "{HEADER}\n{{\"t_ms\":1,\"from\":\"client\",\"raw\":\"x\"}}\n{{\"t_ms\":2,\"from\":\"serv"
        );
        assert_eq!(parse_log(&text).unwrap().entries.len(), 1);
    }

    #[test]
    fn reads_tool_kinds_from_annotations() {
        let schema = json!({"type": "object"});
        let log = log_of(&[
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": [
                    {"name": "get_order", "inputSchema": schema,
                     "annotations": {"readOnlyHint": true}},
                    {"name": "cancel_order", "inputSchema": schema,
                     "annotations": {"readOnlyHint": false, "destructiveHint": true}},
                    {"name": "calculate", "inputSchema": schema},
                    {"name": "lookup", "inputSchema": schema, "annotations": {"title": "Lookup"}}
                ]}}),
            ),
            // A later listing, under a string id, re-annotates one tool.
            (
                Client,
                json!({"jsonrpc": "2.0", "id": "list-2", "method": "tools/list"}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": "list-2", "result": {"tools": [
                    {"name": "calculate", "inputSchema": schema,
                     "annotations": {"readOnlyHint": true}}
                ]}}),
            ),
            // A response to another request lists no tools, whatever it holds.
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 3, "method": "prompts/list"}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 3, "result": {"tools": [{"name": "not_a_tool"}]}}),
            ),
        ]);
        let m = manifest(&log, "shop");
        assert_eq!(m.domain, "shop");
        assert_eq!(m.tools.len(), 4);
        assert_eq!(m.kind("get_order"), Some(ToolKind::Read));
        assert!(m.is_write("cancel_order"));
        assert_eq!(m.kind("lookup"), Some(ToolKind::Generic));
        assert_eq!(m.kind("calculate"), Some(ToolKind::Read));
        assert_eq!(m.kind("not_a_tool"), None);
    }

    #[test]
    fn groups_parallel_calls_into_one_turn() {
        let log = log_of(&[
            call(json!(1), "get_user"),
            // Sent before either response: the same turn.
            call(json!(2), "get_order"),
            reply(json!(2), "order"),
            reply(json!(1), "user"),
            // Everything answered: a new turn.
            call(json!(3), "cancel_order"),
            reply(json!(3), "cancelled"),
            call(json!(4), "get_user"),
            call(json!(5), "get_order"),
            reply(json!(4), "user"),
            // Call 5 is still in flight, so this joins its turn.
            call(json!(6), "get_payment"),
            reply(json!(5), "order"),
            reply(json!(6), "payment"),
            // Never answered: a call without a result.
            call(json!(7), "get_user"),
        ]);
        let ep = episode(&log);
        assert_eq!(
            outline(&ep),
            [
                "turn(get_user,get_order)",
                "result:2",
                "result:1",
                "turn(cancel_order)",
                "result:3",
                "turn(get_user,get_order,get_payment)",
                "result:4",
                "result:5",
                "result:6",
                "turn(get_user)",
            ]
        );
        assert_eq!(ep.assistant_turns(), 4);
        assert_eq!(ep.tool_calls().count(), 7);
    }

    #[test]
    fn marks_tool_errors_and_protocol_errors() {
        let log = log_of(&[
            call(json!(1), "cancel_order"),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"isError": true, "content": [
                    {"type": "text", "text": "order is not pending"},
                    {"type": "text", "text": "try get_order"}
                ]}}),
            ),
            call(json!(2), "no_such_tool"),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 2,
                       "error": {"code": -32602, "message": "Unknown tool: no_such_tool"}}),
            ),
            call(json!(3), "get_order"),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 3, "result": {
                    "content": [{"type": "image", "data": "iVBO", "mimeType": "image/png"}],
                    "structuredContent": {"status": "pending"}
                }}),
            ),
            call(json!(4), "get_order"),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 4, "result": {"content": []}}),
            ),
        ]);
        assert_eq!(
            results(&episode(&log)),
            [
                ("1", true, "order is not pending\ntry get_order"),
                ("2", true, "Unknown tool: no_such_tool"),
                ("3", false, r#"{"status":"pending"}"#),
                ("4", false, r#"{"content":[]}"#),
            ]
        );
    }

    #[test]
    fn matches_string_and_numeric_ids() {
        let log = log_of(&[
            call(json!(7), "get_user"),
            call(json!("abc-1"), "get_order"),
            // The server's own requests use its own ids, which may collide.
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 7, "method": "roots/list"}),
            ),
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 7, "result": {"roots": []}}),
            ),
            // The string "7" is not the number 7.
            reply(json!("7"), "not mine"),
            reply(json!("abc-1"), "order"),
            reply(json!(7), "user"),
        ]);
        let ep = episode(&log);
        let ids: Vec<&str> = ep.tool_calls().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["7", "abc-1"]);
        assert_eq!(
            results(&ep),
            [("abc-1", false, "order"), ("7", false, "user")]
        );
        match &ep.events[2] {
            Event::ToolResult { name, .. } => assert_eq!(name, "get_user"),
            other => panic!("expected a tool result, got {other:?}"),
        }
    }

    #[test]
    fn skips_raw_lines_and_flattens_batches() {
        let text = [
            HEADER,
            r#"{"t_ms":0,"from":"client","message":{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"example-host","version":"1.0"}}}}"#,
            r#"{"t_ms":1,"from":"server","raw":"Debugger listening on ws://127.0.0.1:9229"}"#,
            r#"{"t_ms":2,"from":"client","message":[{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"a"}},{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"b","arguments":{"x":1}}}]}"#,
            r#"{"t_ms":3,"from":"client","raw":"{\"jsonrpc\":\"2.0\",\"id\":3,"}"#,
            r#"{"t_ms":4,"from":"server","message":[{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"A"}]}},{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"B"}]}}]}"#,
            "",
        ]
        .join("\n");
        let ep = episode(&parse_log(&text).unwrap());
        assert_eq!(outline(&ep), ["turn(a,b)", "result:1", "result:2"]);
        let args: Vec<&Value> = ep.tool_calls().map(|c| &c.arguments).collect();
        assert_eq!(args, [&json!({}), &json!({"x": 1})]);
        assert_eq!(
            (ep.id.as_str(), ep.task_id.as_str(), ep.trial),
            ("s1", "", 0)
        );
        assert_eq!(ep.domain, "mcp");
        assert_eq!(ep.agent_model, "example-host");
        assert!(!ep.succeeded());

        // The header's names win over the defaults.
        let named = text.replacen(
            r#""domain":null,"agent_model":null"#,
            r#""domain":"shop","agent_model":"glm-5""#,
            1,
        );
        let ep = episode(&parse_log(&named).unwrap());
        assert_eq!(
            (ep.domain.as_str(), ep.agent_model.as_str()),
            ("shop", "glm-5")
        );
    }

    #[test]
    fn a_cancelled_call_releases_its_turn() {
        let log = log_of(&[
            call(json!(1), "slow_search"),
            (
                Client,
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                       "params": {"requestId": 1, "reason": "timeout"}}),
            ),
            call(json!(2), "get_order"),
            reply(json!(2), "order"),
            // Answered anyway, after the cancellation.
            reply(json!(1), "late"),
        ]);
        let ep = episode(&log);
        assert_eq!(
            outline(&ep),
            [
                "turn(slow_search)",
                "turn(get_order)",
                "result:2",
                "result:1"
            ]
        );
    }
}
