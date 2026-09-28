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
//! Version 2 adds two senders besides `client` and `server`:
//!
//! - `proxy`: a message the proxy sent on its own. A request to the server,
//!   such as a flow's lookup, whose response the server's entry carries; or
//!   a response to the client, such as a refused write or a commit's
//!   result, which is then the call's result. When a call has two responses
//!   (the server's, then the proxy's copy with a flow's lookups appended),
//!   the first is its result.
//! - `context`: a message of the conversation, `{"role": "user" | "assistant",
//!   "content": text}`, which the host handed the proxy (MCP itself never
//!   carries the conversation). It is logged just before the next tool call
//!   after the proxy saw it.
//!
//! The proxy sees only MCP traffic, so an episode from a log is a partial
//! view:
//!
//! - **No conversation, unless the host provides it.** The user's messages
//!   and the LLM's replies never reach an MCP server. Without `context`
//!   entries there are no [`Event::User`] events, and assistant turns have
//!   tool calls but no text.
//! - **Inferred turns.** A tool call sent while an earlier call of the
//!   current turn still awaits its response joins that turn (parallel
//!   calls). So does a call sent within [`SAME_TURN_MS`] of the turn's last
//!   response, with nothing said in between, unless it passes a value that
//!   first appeared in what the turn returned, which it could not have been
//!   written without. Hosts send one LLM turn's calls that closely when
//!   they run them as the model streams them, or one after another: Claude
//!   Code does both, running reads together and writes one at a time. A new
//!   LLM turn takes longer, since the model reads the results first. The
//!   proxy's own requests, a flow's lookups, join a turn only while a call
//!   of it awaits its response: the flow decides each after the last one's
//!   result. Any other call starts a new turn, and a call that is never
//!   answered (nor cancelled) keeps its turn open.
//! - **No usage or outcome.** Token usage is unknown, and so is whether the
//!   task succeeded: `reward` is `0.0` unless the caller sets it.
//! - **One server per log.** Each wrapped server has its own log. [`merge`]
//!   makes one session of the logs an agent's host session left with its
//!   servers, naming each tool after its server (`docs::search`), and
//!   [`episode`] then infers the LLM turns across all of them.

use crate::{Episode, Event, ToolCall, ToolDoc, ToolKind, ToolManifest};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// The log format version `stretto-proxy` writes. This module also reads
/// version 1, which had no `proxy` or `context` entries.
pub const LOG_VERSION: u32 = 2;

/// How soon after its turn's last response a call may still belong to that
/// LLM turn, in milliseconds ([`episode`]). In 285 Claude Code sessions,
/// whose own record groups each call by the model's message, a call of the
/// same turn came a median of 218 ms after the last response and a new turn
/// never sooner than 850 ms (`docs/results/turns-2026-09-28.md`).
pub const SAME_TURN_MS: u64 = 500;

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
    /// The host session the proxy ran in, which every proxy the host
    /// started shares: `STRETTO_SESSION` when the host set it, else the
    /// host's process, `host-<pid>`. [`merge`] makes one session of a host
    /// session's logs. Absent in logs of proxies that did not know it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_session: Option<String>,
    /// The server's name, if the proxy was given one (`--server-name`); see
    /// [`server_name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// Which side of the connection sent a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Peer {
    /// The MCP client, i.e. the host the agent runs in.
    Client,
    /// The MCP server.
    Server,
    /// The proxy itself, sending the server a request of its own (version 2).
    Proxy,
    /// The host, handing the proxy a message of the conversation (version 2).
    Context,
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
    /// The log as `stretto-proxy` writes it: the header, then one entry per
    /// line.
    pub fn to_jsonl(&self) -> String {
        let mut out = serde_json::to_string(&self.header).unwrap_or_default();
        out.push('\n');
        for entry in &self.entries {
            out.push_str(&serde_json::to_string(entry).unwrap_or_default());
            out.push('\n');
        }
        out
    }

    /// Every JSON-RPC message with its sender, in log order. Batches are
    /// flattened and raw lines skipped.
    pub fn messages(&self) -> impl Iterator<Item = (Peer, &Value)> {
        self.timed_messages().map(|(_, from, m)| (from, m))
    }

    /// [`McpLog::messages`], each with when the proxy read it.
    fn timed_messages(&self) -> impl Iterator<Item = (u64, Peer, &Value)> {
        self.entries.iter().flat_map(|e| {
            let items: &[Value] = match &e.message {
                Some(Value::Array(batch)) => batch,
                Some(message) => std::slice::from_ref(message),
                None => &[],
            };
            items.iter().map(move |m| (e.t_ms, e.from, m))
        })
    }
}

/// Read and parse a log file.
pub fn read_log(path: &Path) -> Result<McpLog> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_log(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Whether `path` is a session log: `*.jsonl`, but not one of the logs the
/// proxy writes beside it (`*.flow.jsonl`, `*.confirm.jsonl`).
pub fn is_session_log(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".jsonl") && !name.ends_with(".flow.jsonl") && !name.ends_with(".confirm.jsonl")
}

/// Every session log in `dir` (see [`is_session_log`]), in session order.
pub fn read_sessions(dir: &Path) -> Result<Vec<McpLog>> {
    let mut logs = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();
        if is_session_log(&path) {
            logs.push(read_log(&path)?);
        }
    }
    logs.sort_by(|a, b| a.header.session.cmp(&b.header.session));
    Ok(logs)
}

/// The tools every log's server listed, as one manifest for `domain`.
pub fn manifest_of(logs: &[McpLog], domain: &str) -> ToolManifest {
    let mut all = ToolManifest {
        domain: domain.to_string(),
        ..Default::default()
    };
    for log in logs {
        let listed = manifest(log, domain);
        all.tools.extend(listed.tools);
        all.docs.extend(listed.docs);
    }
    all
}

// ---- sessions across servers --------------------------------------------------------------

/// Between a server's name and its tool's in a session that spans several
/// servers ([`merge`]): `docs::search`.
pub const SERVER_SEPARATOR: &str = "::";

/// `tool` of `server` as a session that spans several servers names it:
/// `docs::search`.
pub fn qualify(server: &str, tool: &str) -> String {
    format!("{server}{SERVER_SEPARATOR}{tool}")
}

/// The server a qualified tool name names ([`qualify`]): `docs` for
/// `docs::search`, and `None` for a tool its server's name is not part of.
pub fn server_of(tool: &str) -> Option<&str> {
    tool.split_once(SERVER_SEPARATOR).map(|(server, _)| server)
}

/// A tool's name on its own server: `search` for `docs::search`, and the
/// name as it is when it is not qualified.
pub fn unqualified(tool: &str) -> &str {
    tool.split_once(SERVER_SEPARATOR)
        .map_or(tool, |(_, name)| name)
}

/// Name the tools of `episode`, all of them `server`'s, as a session that
/// spans several servers names them ([`qualify`]).
pub fn qualify_episode(episode: &mut Episode, server: &str) {
    for event in &mut episode.events {
        match event {
            Event::Assistant { calls, .. } => {
                for call in calls {
                    call.name = qualify(server, &call.name);
                }
            }
            Event::ToolResult { name, .. } => *name = qualify(server, name),
            Event::User { .. } => {}
        }
    }
}

/// The name a log's server goes by in a session that spans several servers:
/// the proxy's `--server-name`, else its `--domain`, else the name the
/// server gave itself when it answered `initialize` (`serverInfo.name`).
/// Characters other than ASCII letters, digits, `-`, `_` and `.` become
/// `_`, so that a qualified tool name ([`qualify`]) splits back into server
/// and tool. `None` for a log with none of them, such as one cut off before
/// the server answered.
pub fn server_name(log: &McpLog) -> Option<String> {
    let given = [&log.header.server_name, &log.header.domain]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .find(|name| !name.is_empty());
    let name = given.or_else(|| {
        log.messages().find_map(|(from, m)| {
            let name = m.pointer("/result/serverInfo/name")?.as_str()?;
            (from == Peer::Server && !name.is_empty()).then_some(name)
        })
    })?;
    let plain = |c: char| {
        if c.is_ascii_alphanumeric() || "-_.".contains(c) {
            c
        } else {
            '_'
        }
    };
    Some(name.chars().map(plain).collect())
}

/// Sessions made of logs ([`merge`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sessions {
    /// Each session's log, in the order of the logs it came from: a log as
    /// it was, or a host session's logs merged into one.
    pub logs: Vec<McpLog>,
    /// How many logs were merged.
    pub merged_logs: usize,
    /// How many sessions they were merged into.
    pub merged_sessions: usize,
    /// Host sessions whose logs were kept apart, since two of them ran
    /// servers of the same name at once.
    pub apart: Vec<String>,
}

/// The sessions `logs` make up: the logs of each host session that spans
/// several servers ([`LogHeader::host_session`]) merged into one, and each
/// other log as it is.
///
/// - **Which logs.** Logs are one host session's when their headers name
///   the same one, their servers have names ([`server_name`]), and they ran
///   at overlapping times, directly or through each other: a host process's
///   id is used again once it has ended. A host session in which two
///   servers of the same name ran at once was not one agent's (it may be a
///   harness running several sessions from one process), so its logs stay
///   apart, and [`Sessions::apart`] names it.
/// - **The merged log.** Its lines are the logs' lines in the order the
///   proxies read them, timed from the first proxy's start. Each tool is
///   named after its server ([`qualify`]), in calls and in answers to
///   `tools/list`, so that the manifest spans the servers, and so is each
///   JSON-RPC id, since each proxy's ids are its own. The conversation,
///   which each proxy logs from the host's one file, is kept once: a proxy's
///   n-th `context` line is dropped when another proxy logged the same line
///   n-th already. The header's session is the host session, with `#k`
///   added when the host session's id stood for several (the k-th, from 0);
///   its start is the first proxy's; its domain is the logs' when they all
///   name the same one; its agent model is the first one named.
/// - **The other logs.** Once any host session is merged, every other log's
///   tools and ids are named after its server too, so that a tool has one
///   name across the sessions. When none is, the logs are returned as they
///   are.
pub fn merge(logs: Vec<McpLog>) -> Sessions {
    let names: Vec<Option<String>> = logs.iter().map(server_name).collect();
    let (groups, apart) = sessions_of(&logs, &names);
    let host = |group: &[usize]| {
        logs[group[0]]
            .header
            .host_session
            .clone()
            .unwrap_or_default()
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    for group in groups.iter().filter(|g| g.len() > 1) {
        *ids.entry(host(group)).or_default() += 1;
    }
    if ids.is_empty() {
        return Sessions {
            logs,
            apart,
            ..Sessions::default()
        };
    }
    let mut numbered: HashMap<String, usize> = HashMap::new();
    let mut sessions = Sessions {
        apart,
        ..Sessions::default()
    };
    for group in &groups {
        let parts: Vec<(&McpLog, &str)> = group
            .iter()
            .filter_map(|&i| Some((&logs[i], names[i].as_deref()?)))
            .collect();
        let log = match parts.as_slice() {
            [] => logs[group[0]].clone(),
            [(log, name)] => qualified(log, name),
            _ => {
                let host = host(group);
                let k = numbered.entry(host.clone()).or_default();
                let session = if ids[&host] > 1 {
                    format!("{host}#{k}")
                } else {
                    host
                };
                *k += 1;
                sessions.merged_logs += parts.len();
                sessions.merged_sessions += 1;
                merged(session, &parts)
            }
        };
        sessions.logs.push(log);
    }
    sessions
}

/// The sessions `logs` make up, as the indices of their logs, in the order
/// of the logs: a host session's logs together, in the order they started,
/// and each other log alone ([`merge`]).
pub fn session_groups(logs: &[McpLog]) -> Vec<Vec<usize>> {
    let names: Vec<Option<String>> = logs.iter().map(server_name).collect();
    sessions_of(logs, &names).0
}

/// [`session_groups`], with the servers' `names`, and the host sessions
/// kept apart.
fn sessions_of(logs: &[McpLog], names: &[Option<String>]) -> (Vec<Vec<usize>>, Vec<String>) {
    let mut sessions = Vec::new();
    let mut hosts: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, log) in logs.iter().enumerate() {
        match (&log.header.host_session, &names[i]) {
            (Some(host), Some(_)) => hosts.entry(host).or_default().push(i),
            _ => sessions.push(vec![i]),
        }
    }
    let span = |i: usize| (logs[i].header.started_unix_ms, ended_unix_ms(&logs[i]));
    let mut apart = Vec::new();
    for (host, mut members) in hosts {
        members.sort_by_key(|&i| span(i).0);
        // Logs that ran at overlapping times, directly or through each other.
        let mut runs: Vec<Vec<usize>> = Vec::new();
        let mut end = 0;
        for i in members {
            let (start, stop) = span(i);
            match runs.last_mut() {
                Some(run) if start <= end => run.push(i),
                _ => runs.push(vec![i]),
            }
            end = end.max(stop);
        }
        for run in runs {
            let at_once = |i: usize, j: usize| span(j).0 <= span(i).1 && span(i).0 <= span(j).1;
            let clash = run.iter().enumerate().any(|(k, &i)| {
                run[k + 1..]
                    .iter()
                    .any(|&j| names[i] == names[j] && at_once(i, j))
            });
            if clash {
                if !apart.iter().any(|a| a == host) {
                    apart.push(host.to_string());
                }
                sessions.extend(run.into_iter().map(|i| vec![i]));
            } else {
                sessions.push(run);
            }
        }
    }
    sessions.sort_by_key(|s| s.iter().min().copied());
    (sessions, apart)
}

/// When the proxy read a log's last line, in milliseconds since the Unix
/// epoch.
fn ended_unix_ms(log: &McpLog) -> u64 {
    let last = log.entries.last().map_or(0, |e| e.t_ms);
    log.header.started_unix_ms.saturating_add(last)
}

/// One log of a host session's logs, `parts`, each with its server's name,
/// as [`merge`] makes it.
fn merged(session: String, parts: &[(&McpLog, &str)]) -> McpLog {
    let start = parts
        .iter()
        .map(|(log, _)| log.header.started_unix_ms)
        .min()
        .unwrap_or_default();
    let mut lines: Vec<(usize, LogEntry)> = Vec::new();
    for (k, (log, name)) in parts.iter().enumerate() {
        let offset = log.header.started_unix_ms - start;
        let own = qualified(log, name);
        let end = ended_unix_ms(&own) - start;
        let cut_off = unanswered(&own);
        for entry in own.entries {
            let t_ms = entry.t_ms.saturating_add(offset);
            lines.push((k, LogEntry { t_ms, ..entry }));
        }
        // A call its proxy's log ended without an answer to, when its
        // server died, say, never gets one: it is cancelled there, so that
        // it does not hold the other servers' turns open.
        for id in cut_off {
            let message = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                                 "params": {"requestId": id}});
            lines.push((
                k,
                LogEntry {
                    t_ms: end,
                    from: Peer::Proxy,
                    message: Some(message),
                    raw: None,
                },
            ));
        }
    }
    // A stable sort keeps each log's own order.
    lines.sort_by_key(|(k, entry)| (entry.t_ms, *k));
    // Each proxy logs the host's conversation from the start of its file.
    let mut said: Vec<(Option<Value>, Option<String>)> = Vec::new();
    let mut heard = vec![0; parts.len()];
    let mut entries = Vec::with_capacity(lines.len());
    for (k, entry) in lines {
        if entry.from == Peer::Context {
            let line = (entry.message.clone(), entry.raw.clone());
            let n = heard[k];
            heard[k] += 1;
            if said.get(n) == Some(&line) {
                continue;
            }
            if n == said.len() {
                said.push(line);
            }
        }
        entries.push(entry);
    }
    let first = &parts[0].0.header;
    let same_domain = parts
        .iter()
        .all(|(log, _)| log.header.domain == first.domain);
    McpLog {
        header: LogHeader {
            stretto_mcp_log: parts
                .iter()
                .map(|(log, _)| log.header.stretto_mcp_log)
                .max()
                .unwrap_or(LOG_VERSION),
            session,
            started_unix_ms: start,
            server_command: Vec::new(),
            domain: first.domain.clone().filter(|_| same_domain),
            agent_model: parts
                .iter()
                .find_map(|(log, _)| log.header.agent_model.clone()),
            host_session: first.host_session.clone(),
            server_name: None,
        },
        entries,
    }
}

/// The ids of the calls in `log` that no response answered and no
/// cancellation released, in the order they were made.
fn unanswered(log: &McpLog) -> Vec<Value> {
    let mut open: Vec<Value> = Vec::new();
    for (from, m) in log.messages() {
        match (from, method(m)) {
            (Peer::Client | Peer::Proxy, Some("tools/call")) => open.extend(request_id(m).cloned()),
            (Peer::Client | Peer::Proxy, Some("notifications/cancelled")) => {
                let released = m.pointer("/params/requestId");
                open.retain(|id| Some(id) != released);
            }
            (Peer::Server | Peer::Proxy, None) => {
                let answered = response_id(m);
                open.retain(|id| Some(id) != answered);
            }
            _ => {}
        }
    }
    open
}

/// `log`, its tools and JSON-RPC ids named after its server, `server`
/// ([`merge`]). The conversation is left as it is.
fn qualified(log: &McpLog, server: &str) -> McpLog {
    let entries = log
        .entries
        .iter()
        .map(|entry| {
            if entry.from == Peer::Context {
                return entry.clone();
            }
            let message = entry.message.as_ref().map(|m| match m {
                Value::Array(batch) => {
                    Value::Array(batch.iter().map(|m| qualified_message(m, server)).collect())
                }
                m => qualified_message(m, server),
            });
            LogEntry {
                message,
                ..entry.clone()
            }
        })
        .collect();
    McpLog {
        header: log.header.clone(),
        entries,
    }
}

/// A JSON-RPC message with its tools and ids named after `server`: the id,
/// a called tool's name, the request a cancellation names, and the tools a
/// `tools/list` answer lists.
fn qualified_message(message: &Value, server: &str) -> Value {
    let mut m = message.clone();
    let id = |id: &mut Value| {
        if !id.is_null() {
            *id = Value::String(qualify(server, &id_string(id)));
        }
    };
    if let Some(v) = m.get_mut("id") {
        id(v);
    }
    match method(message) {
        Some("tools/call") => {
            if let Some(Value::String(name)) = m.pointer_mut("/params/name") {
                *name = qualify(server, name);
            }
        }
        Some("notifications/cancelled") => {
            if let Some(v) = m.pointer_mut("/params/requestId") {
                id(v);
            }
        }
        _ => {}
    }
    if method(message).is_none() {
        let listed = m.pointer_mut("/result/tools").and_then(Value::as_array_mut);
        for tool in listed.into_iter().flatten() {
            if let Some(Value::String(name)) = tool.get_mut("name") {
                *name = qualify(server, name);
            }
        }
    }
    m
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
        (1..=LOG_VERSION).contains(&header.stretto_mcp_log),
        "unsupported log version {} (this build reads versions 1 to {LOG_VERSION})",
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
///
/// Each tool's `description` and the `description` of each property in its
/// `inputSchema` become its [`ToolDoc`].
pub fn manifest(log: &McpLog, domain: &str) -> ToolManifest {
    let mut awaiting = HashSet::new();
    let mut tools = BTreeMap::new();
    let mut docs = BTreeMap::new();
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
                        match tool_doc(tool) {
                            Some(doc) => docs.insert(name.to_string(), doc),
                            None => docs.remove(name),
                        };
                    }
                }
            }
            // The proxy lists no tools of its own, and the conversation has none.
            Peer::Proxy | Peer::Context => {}
        }
    }
    ToolManifest {
        domain: domain.to_string(),
        tools,
        docs,
    }
}

/// A listed tool's input contract: each argument of its `inputSchema`, in
/// order of name, with its JSON type and `!` when required, such as
/// `order_id:string!, reason:string`. `None` for a tool without a schema.
/// A flow pins the contracts of the tools it learned from
/// ([`contracts_of`]), and `stretto-proxy` stops looking up a tool whose
/// contract has changed since.
pub fn contract(tool: &Value) -> Option<String> {
    let schema = tool.get("inputSchema")?;
    let required: HashSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut args: Vec<String> = schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, p)| {
            let ty = match p.get("type") {
                Some(Value::String(t)) => t.clone(),
                Some(Value::Array(ts)) => ts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("|"),
                _ => "any".to_string(),
            };
            let mark = if required.contains(name.as_str()) {
                "!"
            } else {
                ""
            };
            format!("{name}:{ty}{mark}")
        })
        .collect();
    args.sort();
    Some(args.join(", "))
}

/// The input contract ([`contract`]) of every tool the logs' servers
/// listed. A later listing replaces an earlier one.
pub fn contracts_of(logs: &[McpLog]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for log in logs {
        let mut awaiting = HashSet::new();
        for (from, m) in log.messages() {
            match from {
                Peer::Client if method(m) == Some("tools/list") => {
                    if let Some(id) = request_id(m) {
                        awaiting.insert(id_key(id));
                    }
                }
                Peer::Server => {
                    let Some(id) = response_id(m) else { continue };
                    if !awaiting.remove(&id_key(id)) {
                        continue;
                    }
                    let listed = m.pointer("/result/tools").and_then(Value::as_array);
                    for tool in listed.into_iter().flatten() {
                        if let (Some(name), Some(c)) =
                            (tool.get("name").and_then(Value::as_str), contract(tool))
                        {
                            out.insert(name.to_string(), c);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// A listed tool's description and argument descriptions, if it has any.
fn tool_doc(tool: &Value) -> Option<ToolDoc> {
    let summary = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let args: BTreeMap<String, String> = tool
        .pointer("/inputSchema/properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(arg, schema)| {
            let d = schema.get("description").and_then(Value::as_str)?;
            Some((
                arg.clone(),
                d.split_whitespace().collect::<Vec<_>>().join(" "),
            ))
        })
        .collect();
    (!summary.is_empty() || !args.is_empty()).then_some(ToolDoc { summary, args })
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
    episode_sent(log).0
}

/// [`episode`], and for each call, by its id, how many results of its turn
/// had come back when it was sent. A host that runs a turn's calls as the
/// model streams them sends some only after others have returned; a
/// decision made when a result came back could not have known of those.
pub fn episode_sent(log: &McpLog) -> (Episode, HashMap<String, usize>) {
    let mut events = Vec::new();
    let mut pending: HashMap<String, Pending> = HashMap::new();
    // The current turn's index in `events`, and how many of its calls still
    // await a response. A new turn starts only when none do, so every call
    // that holds a turn open belongs to the current one.
    let mut turn = 0;
    let mut open = 0;
    // When the current turn's last open call was answered, what the turn
    // returned, and everything the session held before that: what was said,
    // every call's arguments, and what earlier turns returned.
    let mut answered_ms: Option<u64> = None;
    let mut returned = String::new();
    let mut known = String::new();
    // How many of the current turn's calls have been answered, and for each
    // call how many had been when it was sent.
    let mut answered = 0;
    let mut sent_after = HashMap::new();
    let mut client_name = None;

    for (t_ms, from, m) in log.timed_messages() {
        match from {
            Peer::Context => {
                let text = m
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                known.push_str(&text);
                match m.get("role").and_then(Value::as_str) {
                    Some("user") => events.push(Event::User { text }),
                    Some("assistant") if !text.is_empty() => events.push(Event::Assistant {
                        text: Some(text),
                        calls: Vec::new(),
                        usage: None,
                    }),
                    _ => {}
                }
            }
            Peer::Client | Peer::Proxy if method(m).is_some() => match method(m) {
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
                    let arguments = m
                        .pointer("/params/arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    // The agent's call closely after the turn's last
                    // response, with nothing said since, and needing nothing
                    // it returned: the rest of the same LLM turn. The proxy's
                    // own requests, a flow's lookups, are its decisions one
                    // after another.
                    let continues = from == Peer::Client
                        && answered_ms.is_some_and(|at| t_ms.saturating_sub(at) <= SAME_TURN_MS)
                        && events[turn + 1..]
                            .iter()
                            .all(|e| matches!(e, Event::ToolResult { .. }))
                        && !uses_new_values(&arguments, &returned, &known);
                    if open == 0 && !continues {
                        known.push_str(&std::mem::take(&mut returned));
                        answered_ms = None;
                        answered = 0;
                        events.push(Event::Assistant {
                            text: None,
                            calls: Vec::new(),
                            usage: None,
                        });
                        turn = events.len() - 1;
                    }
                    known.push_str(&arguments.to_string());
                    sent_after.insert(id_string(id), answered);
                    if let Some(Event::Assistant { calls, .. }) = events.get_mut(turn) {
                        calls.push(ToolCall {
                            id: id_string(id),
                            name: name.to_string(),
                            arguments,
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
            // The client's responses answer the server's own requests.
            Peer::Client => {}
            Peer::Server | Peer::Proxy => {
                let Some(id) = response_id(m) else { continue };
                let Some(call) = pending.remove(&id_key(id)) else {
                    continue;
                };
                if call.holds_turn {
                    open -= 1;
                    answered += 1;
                    if open == 0 {
                        answered_ms = Some(t_ms);
                    }
                }
                let (error, content) = outcome(m);
                returned.push_str(&content);
                events.push(Event::ToolResult {
                    call_id: call.call_id,
                    name: call.name,
                    error,
                    content,
                });
            }
        }
    }

    let episode = Episode {
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
    };
    (episode, sent_after)
}

/// Whether `arguments` pass a value that first appeared in `returned`: one
/// of at least two characters that `known`, the session's text before it,
/// does not hold. The agent could not have written such a call before it
/// saw what was returned, so it cannot be in the same LLM turn.
fn uses_new_values(arguments: &Value, returned: &str, known: &str) -> bool {
    let mut values = Vec::new();
    scalars(arguments, &mut values);
    values.iter().any(|v| {
        v.chars().count() >= 2 && returned.contains(v.as_str()) && !known.contains(v.as_str())
    })
}

/// The strings and numbers in `value`, as text.
fn scalars(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Array(items) => items.iter().for_each(|v| scalars(v, out)),
        Value::Object(fields) => fields.values().for_each(|v| scalars(v, out)),
        Value::Bool(_) | Value::Null => {}
    }
}

/// The id of a call as [`ToolCall::id`] holds it: a string as is, anything
/// else, such as a number, as JSON.
pub fn call_id(id: &Value) -> String {
    id_string(id)
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

    /// A log of `lines`, a second apart, as long as an LLM turn takes.
    fn log_of(lines: &[(Peer, Value)]) -> McpLog {
        let timed: Vec<(u64, Peer, Value)> = lines
            .iter()
            .enumerate()
            .map(|(i, (from, message))| (1000 * i as u64, *from, message.clone()))
            .collect();
        log_at(&timed)
    }

    /// A log of `(t_ms, from, message)` lines.
    fn log_at(lines: &[(u64, Peer, Value)]) -> McpLog {
        let mut text = format!("{HEADER}\n");
        for (t_ms, from, message) in lines {
            let entry = LogEntry {
                t_ms: *t_ms,
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
    fn tells_session_logs_from_the_logs_beside_them() {
        let kinds: Vec<bool> = ["s.jsonl", "s.flow.jsonl", "s.confirm.jsonl", "s.json"]
            .iter()
            .map(|n| is_session_log(Path::new(n)))
            .collect();
        assert_eq!(kinds, [true, false, false, false]);
    }

    #[test]
    fn rejects_what_is_not_a_log() {
        assert!(parse_log("").is_err());
        assert!(parse_log("{\"jsonrpc\":\"2.0\",\"id\":1}\n").is_err());
        let v3 = HEADER.replace("\"stretto_mcp_log\":1", "\"stretto_mcp_log\":3");
        assert!(format!("{:#}", parse_log(&v3).unwrap_err()).contains("version 3"));
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
                    {"name": "cancel_order",
                     "description": "Cancel a pending\n   order.",
                     "inputSchema": {"type": "object", "properties": {
                         "order_id": {"type": "string", "description": "The order id."},
                         "reason": {"type": "string"}}},
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
        // Descriptions become docs; undocumented tools have none.
        let doc = &m.docs["cancel_order"];
        assert_eq!(doc.summary, "Cancel a pending order.");
        assert_eq!(doc.args.len(), 1);
        assert_eq!(doc.args["order_id"], "The order id.");
        assert!(!m.docs.contains_key("get_order"));
    }

    #[test]
    fn version_2_adds_the_conversation_and_the_proxys_own_calls() {
        let mut log = log_of(&[
            (
                Peer::Context,
                json!({"role": "user", "content": "Cancel #W1 please"}),
            ),
            call(json!(1), "get_user"),
            reply(json!(1), "user"),
            (
                Peer::Proxy,
                json!({"jsonrpc": "2.0", "id": "stretto-flow-1", "method": "tools/call",
                       "params": {"name": "get_order", "arguments": {"order_id": "#W1"}}}),
            ),
            reply(json!("stretto-flow-1"), "order"),
            (
                Peer::Context,
                json!({"role": "assistant", "content": "It is pending."}),
            ),
        ]);
        log.header.stretto_mcp_log = 2;
        let ep = episode(&log);
        assert_eq!(
            outline(&ep),
            [
                "user",
                "turn(get_user)",
                "result:1",
                "turn(get_order)",
                "result:stretto-flow-1",
                "turn()"
            ]
        );
        assert!(
            matches!(&ep.events[5], Event::Assistant { text: Some(t), .. } if t == "It is pending.")
        );
        // Neither kind of entry lists tools.
        assert!(manifest(&log, "retail").tools.is_empty());
        // Version 2 logs parse; later versions do not.
        let text = format!(
            "{}\n",
            HEADER.replace("\"stretto_mcp_log\":1", "\"stretto_mcp_log\":2")
        );
        assert!(parse_log(&text).is_ok());
        let text = format!(
            "{}\n",
            HEADER.replace("\"stretto_mcp_log\":1", "\"stretto_mcp_log\":3")
        );
        assert!(parse_log(&text).is_err());
    }

    #[test]
    fn the_proxys_own_answers_count_and_a_calls_first_response_wins() {
        let log = log_of(&[
            call(json!(1), "get_user"),
            reply(json!(1), "user"),
            // The proxy's copy, with a flow's lookups appended: not a second
            // result.
            (
                Peer::Proxy,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"content": [
                    {"type": "text", "text": "user"},
                    {"type": "text", "text": "--- Also looked up automatically ---"}
                ], "isError": false}}),
            ),
            call(json!(2), "cancel_order"),
            // Refused by a guard: the proxy answers, and the server never
            // sees the call.
            (
                Peer::Proxy,
                json!({"jsonrpc": "2.0", "id": 2, "result": {"content": [
                    {"type": "text", "text": "Refused"}
                ], "isError": true}}),
            ),
        ]);
        let ep = episode(&log);
        assert_eq!(
            outline(&ep),
            [
                "turn(get_user)",
                "result:1",
                "turn(cancel_order)",
                "result:2"
            ]
        );
        assert!(matches!(
            &ep.events[1],
            Event::ToolResult { content, .. } if content == "user"
        ));
        assert!(matches!(
            &ep.events[3],
            Event::ToolResult { error: true, content, .. } if content == "Refused"
        ));
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
        let result = &ep.events[2];
        assert!(
            matches!(result, Event::ToolResult { name, .. } if name == "get_user"),
            "{result:?}"
        );
    }

    /// Lines that make no event: a message of the conversation in another
    /// role, a call without a name, a cancellation of nothing and a request
    /// of the client's own. A call that reuses the id of one still awaiting
    /// its result replaces it, and an error without a message is shown whole.
    #[test]
    fn lines_that_make_no_event_change_nothing() {
        let log = log_of(&[
            (
                Peer::Context,
                json!({"role": "system", "content": "Be brief."}),
            ),
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {}}),
            ),
            call(json!(2), "get_user"),
            call(json!(2), "get_order"),
            (
                Client,
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                       "params": {"requestId": 9}}),
            ),
            (Client, json!({"jsonrpc": "2.0", "id": 3, "method": "ping"})),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -32000}}),
            ),
        ]);
        let ep = episode(&log);
        assert_eq!(
            outline(&ep),
            ["turn(get_user)", "turn(get_order)", "result:2"]
        );
        assert_eq!(results(&ep), [("2", true, r#"{"code":-32000}"#)]);
    }

    #[test]
    fn a_listed_tool_without_a_name_is_left_out() {
        let log = log_of(&[
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": [
                    {"description": "nameless"}, {"name": "get"}]}}),
            ),
        ]);
        let m = manifest(&log, "d");
        assert_eq!(m.tools.keys().collect::<Vec<_>>(), ["get"]);
        let missing = Path::new("/nonexistent/session.jsonl");
        let e = format!("{:#}", read_log(missing).unwrap_err());
        assert!(e.starts_with("reading /nonexistent/session.jsonl"), "{e}");
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

    /// A call with `arguments`.
    fn call_with(id: Value, name: &str, arguments: Value) -> (Peer, Value) {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                             "params": {"name": name, "arguments": arguments}});
        (Client, message)
    }

    /// `lines` at the given times.
    fn at(lines: Vec<(u64, (Peer, Value))>) -> McpLog {
        let timed: Vec<(u64, Peer, Value)> = lines
            .into_iter()
            .map(|(t, (from, message))| (t, from, message))
            .collect();
        log_at(&timed)
    }

    #[test]
    fn a_call_close_after_its_turns_last_response_is_the_same_llm_turn() {
        // As in a Claude Code session: two reads of the turn reach the
        // server together, and the other two, streamed later, 120 ms after
        // the last response. The model's next turn comes seconds later.
        let channel =
            |id: u64, name: &str, c: &str| call_with(json!(id), name, json!({"channel": c}));
        let log = at(vec![
            (0, channel(7, "get_users", "general")),
            (0, channel(8, "get_users", "External_0")),
            (7, reply(json!(7), "[\"Alice\"]")),
            (9, reply(json!(8), "[\"Bob\"]")),
            (129, channel(9, "get_users", "random")),
            (129, channel(10, "get_users", "private")),
            (136, reply(json!(9), "[]")),
            (137, reply(json!(10), "[]")),
            // A turn of writes, run one at a time, 7 ms after each response.
            (
                9937,
                call_with(json!(11), "add_user", json!({"user": "Alice"})),
            ),
            (9941, reply(json!(11), "ok")),
            (
                9948,
                call_with(json!(12), "add_user", json!({"user": "Bob"})),
            ),
            (9952, reply(json!(12), "ok")),
            // The next turn, after the model read the results.
            (17963, channel(13, "get_users", "External_0")),
            (17970, reply(json!(13), "[\"Alice\",\"Bob\"]")),
        ]);
        let ep = episode(&log);
        let turns: Vec<String> = outline(&ep)
            .into_iter()
            .filter(|e| e.starts_with("turn"))
            .collect();
        assert_eq!(
            turns,
            [
                "turn(get_users,get_users,get_users,get_users)",
                "turn(add_user,add_user)",
                "turn(get_users)"
            ]
        );
        // The last two reads were sent after two of their turn's results.
        let (_, sent) = episode_sent(&log);
        let after = |id: &str| sent[id];
        assert_eq!(
            (after("7"), after("8"), after("9"), after("10")),
            (0, 0, 2, 2)
        );
        assert_eq!((after("11"), after("12"), after("13")), (0, 1, 0));
        // Half a second is the limit.
        let late = at(vec![
            (0, call(json!(1), "a")),
            (5, reply(json!(1), "one")),
            (505, call(json!(2), "b")),
            (510, reply(json!(2), "two")),
            (1011, call(json!(3), "c")),
        ]);
        assert_eq!(episode(&late).assistant_turns(), 2);
    }

    #[test]
    fn a_call_that_needs_what_its_turn_returned_starts_a_new_one() {
        // A scripted agent calls as soon as it has a result: finding the
        // user comes first, and reading them needs the id it returned.
        let log = at(vec![
            (
                0,
                call_with(json!(1), "find_user", json!({"email": "c1@example.com"})),
            ),
            (3, reply(json!(1), "user_1")),
            (
                5,
                call_with(json!(2), "get_user", json!({"user_id": "user_1"})),
            ),
            (8, reply(json!(2), "{\"orders\": [\"#W1a\", \"#W1b\"]}")),
            (
                10,
                call_with(json!(3), "get_order", json!({"order_id": "#W1a"})),
            ),
            (13, reply(json!(3), "{\"status\": \"pending\"}")),
            // The customer's other order was known before this read: the
            // agent could have asked for both at once.
            (
                15,
                call_with(json!(4), "get_order", json!({"order_id": "#W1b"})),
            ),
            (18, reply(json!(4), "{\"status\": \"delivered\"}")),
        ]);
        assert_eq!(
            outline(&episode(&log))
                .into_iter()
                .filter(|e| e.starts_with("turn"))
                .collect::<Vec<_>>(),
            [
                "turn(find_user)",
                "turn(get_user)",
                "turn(get_order,get_order)"
            ]
        );
        // A value the turn returned, but that the session held before, is
        // not new: a number and a short value count only when new.
        assert!(!uses_new_values(
            &json!({"a": "user_1"}),
            "user_1",
            "\"user_1\""
        ));
        assert!(uses_new_values(&json!({"a": [7, "x"], "b": 12}), "12", ""));
        assert!(!uses_new_values(
            &json!({"a": "x", "b": true, "c": null}),
            "x",
            ""
        ));
    }

    #[test]
    fn something_said_between_calls_ends_the_turn() {
        let said = (
            Peer::Context,
            json!({"role": "user", "content": "And my other order?"}),
        );
        let log = at(vec![
            (0, call(json!(1), "get_order")),
            (5, reply(json!(1), "one")),
            (50, said),
            (60, call(json!(2), "get_order")),
        ]);
        assert_eq!(
            outline(&episode(&log)),
            ["turn(get_order)", "result:1", "user", "turn(get_order)"]
        );
        assert_eq!(call_id(&json!(7)), "7");
        assert_eq!(call_id(&json!("stretto-1")), "stretto-1");
    }

    #[test]
    fn the_proxys_lookups_one_after_another_are_turns_of_their_own() {
        // A flow's chain: each lookup follows the last one's result within
        // milliseconds, needing nothing it returned.
        let lookup = |id: &str| {
            let (_, m) = call_with(json!(id), "get_order", json!({"order_id": "#W1"}));
            (Peer::Proxy, m)
        };
        let log = at(vec![
            (0, call(json!(1), "get_user")),
            (5, reply(json!(1), "#W1")),
            (6, lookup("stretto-1")),
            (8, reply(json!("stretto-1"), "one")),
            (9, lookup("stretto-2")),
            (11, reply(json!("stretto-2"), "two")),
        ]);
        assert_eq!(
            outline(&episode(&log)),
            [
                "turn(get_user)",
                "result:1",
                "turn(get_order)",
                "result:stretto-1",
                "turn(get_order)",
                "result:stretto-2"
            ]
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

    #[test]
    fn a_tools_contract_names_its_arguments_types_and_which_are_required() {
        let tool = json!({"name": "get_order", "inputSchema": {"type": "object",
            "properties": {"order_id": {"type": "string"}, "limit": {"type": ["integer", "null"]}, "x": {}},
            "required": ["order_id"]}});
        assert_eq!(
            contract(&tool).as_deref(),
            Some("limit:integer|null, order_id:string!, x:any")
        );
        assert_eq!(contract(&json!({"name": "no_schema"})), None);
        let mut log = log_of(&[
            (
                Peer::Client,
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            ),
            (
                Peer::Server,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": [tool]}}),
            ),
        ]);
        log.header.stretto_mcp_log = 2;
        assert_eq!(
            contracts_of(&[log])["get_order"],
            "limit:integer|null, order_id:string!, x:any"
        );
    }

    // ---- sessions across servers ------------------------------------------------------

    /// A log of `server`'s that started at `start`, in host session `host`.
    fn server_log(
        server: &str,
        host: Option<&str>,
        start: u64,
        lines: &[(u64, Peer, Value)],
    ) -> McpLog {
        let mut log = log_at(lines);
        log.header.stretto_mcp_log = 2;
        log.header.session = format!("{start}-{server}");
        log.header.started_unix_ms = start;
        log.header.host_session = host.map(str::to_string);
        log.header.server_name = Some(server.to_string());
        log
    }

    fn timed(t_ms: u64, (from, message): (Peer, Value)) -> (u64, Peer, Value) {
        (t_ms, from, message)
    }

    fn said(text: &str) -> (Peer, Value) {
        (Peer::Context, json!({"role": "user", "content": text}))
    }

    /// A short call and its answer, the whole of a log.
    fn one_call(tool: &str, at: u64, took: u64) -> Vec<(u64, Peer, Value)> {
        vec![
            timed(at, call(json!(1), tool)),
            timed(at + took, reply(json!(1), "done")),
        ]
    }

    fn sessions(merged: &Sessions) -> Vec<&str> {
        merged
            .logs
            .iter()
            .map(|l| l.header.session.as_str())
            .collect()
    }

    #[test]
    fn a_host_sessions_logs_are_one_session_across_servers() {
        let list = |id: u64, tool: &str| {
            [
                (
                    Client,
                    json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"}),
                ),
                (
                    Server,
                    json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [
                        {"name": tool, "annotations": {"readOnlyHint": true}}]}}),
                ),
            ]
        };
        let [ask, listed] = list(1, "get_ticket");
        let mut tickets = server_log(
            "tickets",
            Some("h1"),
            1_000,
            &[
                timed(0, ask),
                timed(5, listed),
                timed(90, said("My ticket is T-1.")),
                timed(100, call_with(json!(2), "get_ticket", json!({"id": "T-1"}))),
                timed(150, reply(json!(2), "T-1: the printer is on fire")),
                timed(1_990, said("Thanks. And T-2?")),
                timed(
                    2_000,
                    call_with(json!(3), "get_ticket", json!({"id": "T-2"})),
                ),
                timed(2_050, reply(json!(3), "T-2: toner low")),
            ],
        );
        let [ask, listed] = list(1, "search");
        let mut docs = server_log(
            "docs",
            Some("h1"),
            1_020,
            &[
                timed(0, ask),
                timed(4, listed),
                // The docs proxy reads what was said before its own calls.
                timed(100, said("My ticket is T-1.")),
                timed(101, call_with(json!(2), "search", json!({"q": "printer"}))),
                timed(140, reply(json!(2), "Printer fires: unplug it.")),
                timed(2_080, said("Thanks. And T-2?")),
                timed(
                    2_081,
                    call_with(json!(3), "search", json!({"q": "printer toner"})),
                ),
                timed(2_120, reply(json!(3), "Toner: order more.")),
            ],
        );
        docs.header.agent_model = Some("glm-5.3".to_string());
        tickets.header.domain = Some("tickets".to_string());
        docs.header.domain = Some("docs".to_string());

        let merged = merge(vec![tickets, docs]);
        assert_eq!(
            (
                merged.merged_logs,
                merged.merged_sessions,
                merged.apart.len()
            ),
            (2, 1, 0)
        );
        assert_eq!(sessions(&merged), ["h1"]);
        let log = &merged.logs[0];
        let h = &log.header;
        assert_eq!(
            (
                h.session.as_str(),
                h.started_unix_ms,
                h.host_session.as_deref()
            ),
            ("h1", 1_000, Some("h1"))
        );
        assert_eq!(
            (
                h.domain.as_deref(),
                h.agent_model.as_deref(),
                h.server_name.as_deref()
            ),
            (None, Some("glm-5.3"), None)
        );
        assert!(h.server_command.is_empty());
        assert_eq!(log.entries[0].message.as_ref().unwrap()["id"], "tickets::1");
        assert_eq!(log.entries[2].t_ms, 20);

        // The tools of both servers, each named after its own.
        let m = manifest_of(&merged.logs, "support");
        assert_eq!(
            m.tools.keys().collect::<Vec<_>>(),
            ["docs::search", "tickets::get_ticket"]
        );
        // What was said once, and the LLM turns across the servers: the
        // docs search went out while the ticket was still being read, and
        // the second came right after the second ticket, needing nothing
        // it returned.
        let ep = episode(log);
        assert_eq!(ep.id, "h1");
        assert_eq!(
            outline(&ep),
            [
                "user",
                "turn(tickets::get_ticket,docs::search)",
                "result:tickets::2",
                "result:docs::2",
                "user",
                "turn(tickets::get_ticket,docs::search)",
                "result:tickets::3",
                "result:docs::3"
            ]
        );
    }

    #[test]
    fn a_reused_host_id_names_each_of_its_sessions() {
        let mut a1 = server_log("a", Some("host-7"), 0, &one_call("x", 0, 10));
        let mut b1 = server_log("b", Some("host-7"), 5, &one_call("y", 0, 3));
        // The host's process id, used again by a later host.
        let mut a2 = server_log("a", Some("host-7"), 100, &one_call("x", 0, 10));
        let mut b2 = server_log("b", Some("host-7"), 101, &one_call("y", 0, 3));
        for (log, domain) in [
            (&mut a1, "support"),
            (&mut b1, "support"),
            (&mut a2, "support"),
            (&mut b2, "other"),
        ] {
            log.header.domain = Some(domain.to_string());
        }
        // A log of no host session, and one whose server has no name.
        let lone = server_log("c", None, 50, &one_call("z", 0, 1));
        let unnamed = log_at(&one_call("w", 0, 1));
        let logs = vec![a1, b1, lone, a2, b2, unnamed];
        assert_eq!(
            session_groups(&logs),
            [vec![0, 1], vec![2], vec![3, 4], vec![5]]
        );

        let merged = merge(logs);
        assert_eq!(sessions(&merged), ["host-7#0", "50-c", "host-7#1", "s1"]);
        assert_eq!((merged.merged_logs, merged.merged_sessions), (4, 2));
        let domains: Vec<Option<&str>> = merged
            .logs
            .iter()
            .map(|l| l.header.domain.as_deref())
            .collect();
        assert_eq!(domains, [Some("support"), None, None, None]);
        // Once a host session is merged, every tool has its server's name,
        // where the server has one.
        let names = |log: &McpLog| -> Vec<String> {
            episode(log)
                .events
                .iter()
                .filter_map(|e| match e {
                    Event::ToolResult { call_id, name, .. } => Some(format!("{call_id} {name}")),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names(&merged.logs[1]), ["c::1 c::z"]);
        assert_eq!(names(&merged.logs[3]), ["1 w"]);
        assert_eq!(names(&merged.logs[0]), ["b::1 b::y", "a::1 a::x"]);
    }

    #[test]
    fn two_servers_of_one_name_at_once_were_not_one_hosts() {
        // A harness ran two sessions of the same server from one process.
        let x1 = || server_log("a", Some("h"), 0, &one_call("x", 0, 50));
        let x2 = || server_log("a", Some("h"), 10, &one_call("x", 0, 20));
        let later1 = server_log("a", Some("h"), 1_000, &one_call("x", 0, 50));
        let later2 = server_log("a", Some("h"), 1_010, &one_call("x", 0, 20));
        let as_is = merge(vec![x1(), x2(), later1, later2]);
        assert_eq!(as_is.apart, ["h"]);
        assert_eq!(
            as_is.logs,
            vec![x1(), x2(), as_is.logs[2].clone(), as_is.logs[3].clone()]
        );
        assert_eq!(as_is.merged_sessions, 0);

        // A server that died and was started again while another ran: one
        // host's session, where the call it died on holds no turn open.
        let mut y1 = server_log("a", Some("r"), 0, &[timed(0, call(json!(1), "x"))]);
        y1.entries.push(LogEntry {
            t_ms: 30,
            from: Server,
            message: None,
            raw: Some("crashed".to_string()),
        });
        let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                            "params": {"requestId": 2}});
        let z = server_log(
            "b",
            Some("r"),
            0,
            &[
                timed(500, call(json!(1), "y")),
                timed(600, reply(json!(1), "done")),
                timed(700, call(json!(2), "slow")),
                timed(750, (Client, cancel)),
            ],
        );
        let y2 = server_log(
            "a",
            Some("r"),
            100,
            &[
                timed(1_100, call(json!(1), "x")),
                timed(1_110, reply(json!(1), "done")),
            ],
        );
        let merged = merge(vec![x1(), x2(), y1, z, y2]);
        assert_eq!(merged.apart, ["h"]);
        assert_eq!(sessions(&merged), ["0-a", "10-a", "r"]);
        assert_eq!((merged.merged_logs, merged.merged_sessions), (3, 1));
        assert_eq!(
            outline(&episode(&merged.logs[2])),
            [
                "turn(a::x)",
                "turn(b::y,b::slow)",
                "result:b::1",
                "turn(a::x)",
                "result:a::1"
            ]
        );
        // Merged or not, the logs apart are named alike.
        assert_eq!(
            outline(&episode(&merged.logs[0])),
            ["turn(a::x)", "result:a::1"]
        );
    }

    #[test]
    fn a_proxy_that_logs_a_different_line_keeps_it() {
        let a = server_log(
            "a",
            Some("h"),
            0,
            &[
                timed(0, said("one")),
                timed(1, said("two")),
                timed(2, (Peer::Context, json!("not a message"))),
            ],
        );
        let mut b = server_log(
            "b",
            Some("h"),
            0,
            &[timed(5, said("one")), timed(6, said("other"))],
        );
        b.entries.push(LogEntry {
            t_ms: 7,
            from: Peer::Context,
            message: None,
            raw: Some("not json".to_string()),
        });
        let merged = merge(vec![a, b]);
        let lines: Vec<String> = merged.logs[0]
            .entries
            .iter()
            .map(|e| match (&e.message, &e.raw) {
                (Some(m), _) => m.get("content").unwrap_or(m).to_string(),
                (None, raw) => format!("raw {}", raw.as_deref().unwrap_or_default()),
            })
            .collect();
        assert_eq!(
            lines,
            [
                "\"one\"",
                "\"two\"",
                "\"not a message\"",
                "\"other\"",
                "raw not json"
            ]
        );
    }

    #[test]
    fn a_servers_name_is_its_proxys_or_its_own() {
        let mut log = log_of(&[
            (
                Client,
                json!({"jsonrpc": "2.0", "id": 9, "result": {"serverInfo": {"name": "not the server"}}}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 0, "result": {"serverInfo": {"name": ""}}}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 1, "result": {"serverInfo": {"name": "My Server::v2"}}}),
            ),
        ]);
        assert_eq!(server_name(&log).as_deref(), Some("My_Server__v2"));
        log.header.domain = Some("retail".to_string());
        assert_eq!(server_name(&log).as_deref(), Some("retail"));
        log.header.server_name = Some("orders".to_string());
        assert_eq!(server_name(&log).as_deref(), Some("orders"));
        log.header.server_name = Some(String::new());
        assert_eq!(server_name(&log).as_deref(), Some("retail"));
        assert_eq!(server_name(&log_of(&[])), None);
        // The header reads the same without the fields it did not have.
        let text = serde_json::to_string(&log_of(&[]).header).unwrap();
        assert!(
            !text.contains("host_session") && !text.contains("server_name"),
            "{text}"
        );
    }

    #[test]
    fn a_qualified_name_splits_back_into_server_and_tool() {
        assert_eq!(qualify("docs", "search"), "docs::search");
        assert_eq!(server_of("docs::search"), Some("docs"));
        assert_eq!(server_of("search"), None);
        assert_eq!(unqualified("docs::search"), "search");
        assert_eq!(unqualified("search"), "search");
        let mut ep = episode(&log_of(&[
            said("hi"),
            call(json!(1), "search"),
            reply(json!(1), "found"),
        ]));
        qualify_episode(&mut ep, "docs");
        assert_eq!(outline(&ep), ["user", "turn(docs::search)", "result:1"]);
        assert!(matches!(&ep.events[2], Event::ToolResult { name, .. } if name == "docs::search"));
    }

    #[test]
    fn every_id_and_tool_of_a_log_is_named_after_its_server() {
        let log = log_of(&[
            (
                Client,
                json!([
                    {"jsonrpc": "2.0", "id": "c1", "method": "tools/call", "params": {"name": "get"}},
                    {"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": "c1"}}
                ]),
            ),
            (
                Client,
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {}}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": 4, "result": {"tools": [{"name": "get"}, {"title": "no name"}]}}),
            ),
            (
                Server,
                json!({"jsonrpc": "2.0", "id": null, "error": {"message": "bad"}}),
            ),
            (Server, json!("a string")),
        ]);
        let mut log = log;
        log.entries.push(LogEntry {
            t_ms: 9,
            from: Server,
            message: None,
            raw: Some("starting".to_string()),
        });
        let q = qualified(&log, "s");
        let messages: Vec<Value> = q
            .entries
            .iter()
            .map(|e| e.message.clone().unwrap_or_default())
            .collect();
        assert_eq!(
            messages[0],
            json!([
                {"jsonrpc": "2.0", "id": "s::c1", "method": "tools/call", "params": {"name": "s::get"}},
                {"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": "s::c1"}}
            ])
        );
        assert_eq!(messages[1], log.entries[1].message.clone().unwrap());
        assert_eq!(
            messages[2],
            json!({"jsonrpc": "2.0", "id": "s::4", "result": {"tools": [{"name": "s::get"}, {"title": "no name"}]}})
        );
        assert_eq!(messages[3]["id"], Value::Null);
        assert_eq!(messages[4], json!("a string"));
        assert_eq!(q.entries[5].raw.as_deref(), Some("starting"));
    }
}
