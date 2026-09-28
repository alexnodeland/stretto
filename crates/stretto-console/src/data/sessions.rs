//! Sessions: a proxy's log, with the flow's decisions and the confirmation
//! judge's log beside it.
//!
//! [`analyze`] reads one in full. The log is parsed by
//! [`stretto_trace::mcp::read_log`], and the agent's LLM turns are inferred
//! by [`stretto_trace::mcp::episode`] over the log without the proxy's own
//! requests (the flow's lookups are not the agent's turns). Every
//! `tools/call` becomes a [`CallView`]: the agent's, from the client, and
//! the proxy's, which are the flow's lookups, except those it makes while
//! answering the agent's `stretto_commit`, which are the agent's writes. A
//! call's result is the first answer to it, as in `stretto_trace`. Each of
//! the flow's lookups is matched to the decision that made it: the next
//! `lookup` decision after the same call of the agent's.

use crate::api::sessions::{
    CallBy, CallView, ContextMessage, DecisionAction, EventFrom, EventKind, FlowDecision, FlowRun,
    Role, SessionDetail, SessionMode, SessionSummary, TimelineEvent, ToolInfo, Turn, UpstreamView,
};
use crate::data::SessionFile;
use crate::State;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;
use stretto_trace::mcp::{self, McpLog, Peer};
use stretto_trace::{Event, ToolKind};

/// Results longer than this are cut.
pub const RESULT_LIMIT: usize = 64 * 1024;
/// A timeline event's summary is cut at this many characters.
const SUMMARY_LIMIT: usize = 200;
/// The tool `stretto-proxy --commit` adds.
const COMMIT_TOOL: &str = "stretto_commit";

/// What the list needs of a session, kept in the cache.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The summary, or why the log could not be read. Its `key` and `path`
    /// are the scan's, set when it is listed.
    pub summary: Result<SessionSummary, String>,
    /// The tools its server listed.
    pub tools: Vec<ToolInfo>,
    /// The tools called in it, the agent's and the flow's.
    pub called: BTreeSet<String>,
    /// Decisions logged in shadow, lookups and hand-backs.
    pub shadow_decisions: usize,
}

/// Read `file` for the list.
pub fn load(file: &SessionFile) -> Loaded {
    match analyze(file) {
        Ok(a) => Loaded {
            called: a.calls.iter().map(|c| c.tool.clone()).collect(),
            summary: Ok(a.summary),
            tools: a.tools,
            shadow_decisions: a.shadow_decisions,
        },
        Err(e) => Loaded {
            summary: Err(e),
            tools: Vec::new(),
            called: BTreeSet::new(),
            shadow_decisions: 0,
        },
    }
}

/// Every session in the data directory that can be read, newest first,
/// with what the cache holds for it.
pub fn all(state: &State) -> Vec<(SessionSummary, Arc<Loaded>)> {
    let catalog = crate::data::catalog(state.data_dir());
    let mut cache = state.cache.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<(SessionSummary, Arc<Loaded>)> = catalog
        .sessions
        .iter()
        .filter_map(|file| {
            let loaded = cache.session(file);
            let mut summary = loaded.summary.clone().ok()?;
            summary.key = file.key.clone();
            summary.path = file.rel.clone();
            Some((summary, loaded))
        })
        .collect();
    cache.retain(&catalog);
    out.sort_by(|(a, _), (b, _)| {
        b.started_unix_ms
            .cmp(&a.started_unix_ms)
            .then_with(|| b.session_id.cmp(&a.session_id))
    });
    out
}

/// The logs in the data directory that could not be read, with why.
pub fn unreadable(state: &State) -> Vec<(String, String)> {
    let catalog = crate::data::catalog(state.data_dir());
    let mut cache = state.cache.lock().unwrap_or_else(|e| e.into_inner());
    catalog
        .sessions
        .iter()
        .filter_map(|file| {
            let loaded = cache.session(file);
            loaded.summary.clone().err().map(|e| (file.rel.clone(), e))
        })
        .collect()
}

/// Whether `needle` (lower case) is in the session's id, domain, agent or
/// the name of a tool called in it.
pub fn matches(summary: &SessionSummary, loaded: &Loaded, needle: &str) -> bool {
    let has = |s: &str| s.to_lowercase().contains(needle);
    has(&summary.session_id)
        || summary.domain.as_deref().is_some_and(has)
        || summary.agent.as_deref().is_some_and(has)
        || loaded.called.iter().any(|t| has(t))
}

/// A session, read in full.
#[derive(Clone, Debug)]
pub struct Analysis {
    pub summary: SessionSummary,
    pub header: Value,
    pub tools: Vec<ToolInfo>,
    pub turns: Vec<Turn>,
    pub calls: Vec<CallView>,
    pub context: Vec<ContextMessage>,
    pub decisions: Vec<FlowDecision>,
    pub runs: Vec<FlowRun>,
    pub confirmations: Vec<Value>,
    pub events: Vec<TimelineEvent>,
    pub truncated: bool,
    pub shadow_decisions: usize,
}

impl Analysis {
    pub fn detail(self) -> SessionDetail {
        SessionDetail {
            summary: self.summary,
            header: self.header,
            tools: self.tools,
            turns: self.turns,
            calls: self.calls,
            context: self.context,
            decisions: self.decisions,
            runs: self.runs,
            confirmations: self.confirmations,
            events: self.events,
            truncated: self.truncated,
        }
    }
}

/// A call as it is read, with where its request came from.
struct Pending {
    view: CallView,
    from_client: bool,
}

/// Read `file` in full.
pub fn analyze(file: &SessionFile) -> Result<Analysis, String> {
    let log = mcp::read_log(&file.path).map_err(|e| format!("{e:#}"))?;
    let size_bytes = std::fs::metadata(&file.path).map_or(0, |m| m.len());
    let header = first_line(&file.path).unwrap_or(Value::Null);
    let (decisions, runs) = file
        .flow_log
        .as_deref()
        .map(read_flow_log)
        .unwrap_or_default();
    let confirmations = file
        .confirm_log
        .as_deref()
        .map(read_json_lines)
        .unwrap_or_default();

    let mut listed: BTreeMap<String, Value> = BTreeMap::new();
    let mut awaiting_lists: HashSet<String> = HashSet::new();
    let mut calls: Vec<Pending> = Vec::new();
    let mut pending: HashMap<String, usize> = HashMap::new();
    // Requests to the server (from the client or the proxy), and to the
    // client (from the server), by id, with their method.
    let mut to_server: HashMap<String, String> = HashMap::new();
    let mut to_client: HashMap<String, String> = HashMap::new();
    // The agent's `stretto_commit` calls the proxy has not answered yet.
    let mut commits: Vec<String> = Vec::new();
    // The agent's call the server answered last: a flow runs after it.
    let mut last_answered: Option<String> = None;
    let mut client_name: Option<String> = None;
    let mut context = Vec::new();
    let mut events = Vec::new();
    let mut truncated = false;
    let mut proxy_entries = false;
    let mut duration_ms = 0;

    for entry in &log.entries {
        let t_ms = entry.t_ms;
        duration_ms = duration_ms.max(t_ms);
        let from = entry.from;
        proxy_entries |= from == Peer::Proxy;
        if let Some(raw) = &entry.raw {
            events.push(TimelineEvent {
                t_ms,
                from: event_from(from),
                kind: EventKind::Raw,
                method: None,
                id: None,
                summary: cut(raw, SUMMARY_LIMIT),
            });
        }
        let messages: Vec<&Value> = match &entry.message {
            Some(Value::Array(batch)) => batch.iter().collect(),
            Some(m) => vec![m],
            None => Vec::new(),
        };
        for m in messages {
            if from == Peer::Context {
                let content = m
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let role = match m.get("role").and_then(Value::as_str) {
                    Some("user") => Some(Role::User),
                    Some("assistant") => Some(Role::Assistant),
                    _ => None,
                };
                events.push(TimelineEvent {
                    t_ms,
                    from: EventFrom::Context,
                    kind: EventKind::Context,
                    method: None,
                    id: None,
                    summary: cut(
                        &format!(
                            "{}: {content}",
                            m.get("role").and_then(Value::as_str).unwrap_or("?")
                        ),
                        SUMMARY_LIMIT,
                    ),
                });
                if let Some(role) = role {
                    context.push(ContextMessage {
                        t_ms,
                        role,
                        content,
                    });
                }
                continue;
            }
            let method = m.get("method").and_then(Value::as_str);
            let id = m.get("id").filter(|id| !id.is_null());
            let answers = m.get("result").is_some() || m.get("error").is_some_and(|e| !e.is_null());
            match (method, id) {
                (Some(method), Some(id)) => {
                    let key = id.to_string();
                    let params = m.get("params").unwrap_or(&Value::Null);
                    let summary = if method == "tools/call" {
                        format!(
                            "{} {}",
                            params.get("name").and_then(Value::as_str).unwrap_or("?"),
                            compact(params.get("arguments").unwrap_or(&Value::Null))
                        )
                    } else {
                        format!("{method} {}", compact(params))
                    };
                    events.push(TimelineEvent {
                        t_ms,
                        from: event_from(from),
                        kind: EventKind::Request,
                        method: Some(method.to_string()),
                        id: Some(id_string(id)),
                        summary: cut(summary.trim_end(), SUMMARY_LIMIT),
                    });
                    if from == Peer::Server {
                        to_client.insert(key, method.to_string());
                        continue;
                    }
                    to_server.insert(key.clone(), method.to_string());
                    match (from, method) {
                        (Peer::Client, "initialize") if client_name.is_none() => {
                            client_name = params
                                .pointer("/clientInfo/name")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                        }
                        (Peer::Client, "tools/list") => {
                            awaiting_lists.insert(key);
                        }
                        (_, "tools/call") => {
                            let Some(tool) = params.get("name").and_then(Value::as_str) else {
                                continue;
                            };
                            let from_client = from == Peer::Client;
                            let (by, after) = if from_client {
                                (CallBy::Agent, None)
                            } else if let Some(commit) = commits.last() {
                                (CallBy::Agent, Some(commit_id(commit)))
                            } else {
                                (CallBy::Flow, last_answered.clone())
                            };
                            if from_client && tool == COMMIT_TOOL {
                                commits.push(key.clone());
                            }
                            // A reused id replaces the call it named before,
                            // as stretto_trace reads it.
                            pending.insert(key, calls.len());
                            calls.push(Pending {
                                view: CallView {
                                    id: id_string(id),
                                    by,
                                    tool: tool.to_string(),
                                    kind: ToolKind::Generic,
                                    arguments: params
                                        .get("arguments")
                                        .cloned()
                                        .unwrap_or_else(|| Value::Object(Default::default())),
                                    t_ms,
                                    result_t_ms: None,
                                    latency_ms: None,
                                    ok: None,
                                    result_text: None,
                                    result_json: None,
                                    result_truncated: false,
                                    turn: None,
                                    after,
                                    decision: None,
                                },
                                from_client,
                            });
                        }
                        _ => {}
                    }
                }
                (Some(method), None) => events.push(TimelineEvent {
                    t_ms,
                    from: event_from(from),
                    kind: EventKind::Notification,
                    method: Some(method.to_string()),
                    id: None,
                    summary: cut(
                        format!(
                            "{method} {}",
                            compact(m.get("params").unwrap_or(&Value::Null))
                        )
                        .trim_end(),
                        SUMMARY_LIMIT,
                    ),
                }),
                (None, Some(id)) if answers => {
                    let key = id.to_string();
                    let (error, text) = outcome(m);
                    let asked = if from == Peer::Client {
                        to_client.get(&key).cloned()
                    } else {
                        to_server.get(&key).cloned()
                    };
                    if from != Peer::Client {
                        if let Some(i) = pending.remove(&key) {
                            let call = &mut calls[i];
                            let (cut_text, was_cut) = cut_bytes(&text, RESULT_LIMIT);
                            truncated |= was_cut;
                            call.view.result_t_ms = Some(t_ms);
                            call.view.latency_ms = Some(t_ms.saturating_sub(call.view.t_ms));
                            call.view.ok = Some(!error);
                            call.view.result_json = if was_cut {
                                None
                            } else {
                                serde_json::from_str(&cut_text).ok()
                            };
                            call.view.result_text = Some(cut_text);
                            call.view.result_truncated = was_cut;
                            if from == Peer::Server && call.from_client {
                                last_answered = Some(call.view.id.clone());
                            }
                        }
                        if from == Peer::Proxy {
                            commits.retain(|c| *c != key);
                        }
                        if from == Peer::Server && awaiting_lists.remove(&key) {
                            let tools = m.pointer("/result/tools").and_then(Value::as_array);
                            for tool in tools.into_iter().flatten() {
                                if let Some(name) = tool.get("name").and_then(Value::as_str) {
                                    listed.insert(name.to_string(), tool.clone());
                                }
                            }
                        }
                    }
                    let summary = match m.get("error").filter(|e| !e.is_null()) {
                        Some(e) => format!(
                            "error {}: {}",
                            e.get("code").map(|c| c.to_string()).unwrap_or_default(),
                            e.get("message").and_then(Value::as_str).unwrap_or_default()
                        ),
                        None if asked.as_deref() == Some("tools/call") => text,
                        None => compact(&m["result"]),
                    };
                    events.push(TimelineEvent {
                        t_ms,
                        from: event_from(from),
                        kind: if error {
                            EventKind::Error
                        } else {
                            EventKind::Response
                        },
                        method: asked,
                        id: Some(id_string(id)),
                        summary: cut(&summary, SUMMARY_LIMIT),
                    });
                }
                _ => events.push(TimelineEvent {
                    t_ms,
                    from: event_from(from),
                    kind: EventKind::Raw,
                    method: None,
                    id: id.map(id_string),
                    summary: cut(&compact(m), SUMMARY_LIMIT),
                }),
            }
        }
    }

    // Tools, and each call's kind from them.
    let tools: Vec<ToolInfo> = listed.values().map(tool_info).collect();
    let kinds: HashMap<&str, ToolKind> = tools.iter().map(|t| (t.name.as_str(), t.kind)).collect();
    for call in &mut calls {
        call.view.kind = kinds
            .get(call.view.tool.as_str())
            .copied()
            .unwrap_or(ToolKind::Generic);
    }

    // The agent's turns, as stretto_trace infers them from the log without
    // the proxy's own requests.
    let turns = turns(&log, &mut calls, &context);

    // Each of the flow's lookups, and the decision that made it.
    let mut lookups: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (i, d) in decisions.iter().enumerate() {
        if d.action == DecisionAction::Lookup && !d.shadow {
            lookups.entry(d.after.as_str()).or_default().push_back(i);
        }
    }
    for call in calls.iter_mut().filter(|c| c.view.by == CallBy::Flow) {
        call.view.decision = call
            .view
            .after
            .as_deref()
            .and_then(|a| lookups.get_mut(a))
            .and_then(VecDeque::pop_front);
    }

    let agent: Vec<&CallView> = calls
        .iter()
        .filter(|c| c.from_client)
        .map(|c| &c.view)
        .collect();
    let shadow_decisions = decisions.iter().filter(|d| d.shadow).count();
    let mode = if proxy_entries || decisions.iter().any(|d| !d.shadow) {
        SessionMode::Served
    } else if shadow_decisions > 0 {
        SessionMode::Shadow
    } else {
        SessionMode::Recorded
    };
    let summary = SessionSummary {
        key: file.key.clone(),
        path: file.rel.clone(),
        session_id: log.header.session.clone(),
        domain: log.header.domain.clone().filter(|d| !d.is_empty()),
        agent: log
            .header
            .agent_model
            .clone()
            .filter(|a| !a.is_empty())
            .or(client_name),
        started_unix_ms: log.header.started_unix_ms,
        duration_ms,
        mode,
        tool_calls: agent.len(),
        llm_turns: turns.len(),
        errors: agent.iter().filter(|c| c.ok == Some(false)).count(),
        flow_lookups: calls.iter().filter(|c| c.view.by == CallBy::Flow).count(),
        hand_backs: decisions
            .iter()
            .filter(|d| d.action == DecisionAction::HandBack && !d.shadow)
            .count(),
        shadow_lookups: decisions
            .iter()
            .filter(|d| d.action == DecisionAction::Lookup && d.shadow)
            .count(),
        writes: agent.iter().filter(|c| c.kind == ToolKind::Write).count(),
        upstream: upstream(&log.header.server_command),
        size_bytes,
        has_flow_log: file.flow_log.is_some(),
        has_confirm_log: file.confirm_log.is_some(),
    };
    Ok(Analysis {
        summary,
        header,
        tools,
        turns,
        calls: calls.into_iter().map(|c| c.view).collect(),
        context,
        decisions,
        runs,
        confirmations,
        events,
        truncated,
        shadow_decisions,
    })
}

/// The agent's LLM turns: [`mcp::episode`] over the log without the
/// proxy's requests. Each call of the agent's gets its turn's index.
fn turns(log: &McpLog, calls: &mut [Pending], context: &[ContextMessage]) -> Vec<Turn> {
    let agent_log = McpLog {
        header: log.header.clone(),
        entries: log
            .entries
            .iter()
            .filter(|e| {
                e.from != Peer::Proxy
                    || !e
                        .message
                        .as_ref()
                        .is_some_and(|m| m.get("method").is_some())
            })
            .cloned()
            .collect(),
    };
    let episode = mcp::episode(&agent_log);
    // The agent's calls in order, as the episode has them.
    let mut agent = calls
        .iter_mut()
        .filter(|c| c.from_client)
        .map(|c| &mut c.view);
    let mut messages = context
        .iter()
        .filter(|c| c.role == Role::Assistant && !c.content.is_empty())
        .map(|c| c.t_ms);
    let mut turns = Vec::new();
    for event in &episode.events {
        let Event::Assistant { calls: made, .. } = event else {
            continue;
        };
        let index = turns.len();
        if made.is_empty() {
            let t = messages.next().unwrap_or(0);
            turns.push(Turn {
                index,
                start_ms: t,
                end_ms: t,
                calls: Vec::new(),
            });
            continue;
        }
        let (mut start, mut end, mut ids) = (u64::MAX, 0, Vec::new());
        for _ in made {
            let Some(call) = agent.next() else { break };
            call.turn = Some(index);
            start = start.min(call.t_ms);
            end = end.max(call.result_t_ms.unwrap_or(call.t_ms));
            ids.push(call.id.clone());
        }
        turns.push(Turn {
            index,
            start_ms: if start == u64::MAX { 0 } else { start },
            end_ms: end.max(if start == u64::MAX { 0 } else { start }),
            calls: ids,
        });
    }
    turns
}

/// A listed tool, as the API shows it.
pub fn tool_info(tool: &Value) -> ToolInfo {
    let hint = |name: &str| {
        tool.pointer(&format!("/annotations/{name}"))
            .and_then(Value::as_bool)
    };
    let read_only_hint = hint("readOnlyHint");
    ToolInfo {
        name: tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: match read_only_hint {
            Some(true) => ToolKind::Read,
            Some(false) => ToolKind::Write,
            None => ToolKind::Generic,
        },
        description: tool
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        read_only_hint,
        destructive_hint: hint("destructiveHint"),
        contract: mcp::contract(tool),
    }
}

/// The server a log's header names: an HTTP URL (the proxy writes the
/// upstream's URL as the only word), else a command.
pub fn upstream(command: &[String]) -> Option<UpstreamView> {
    match command {
        [] => None,
        [url] if url.starts_with("http://") || url.starts_with("https://") => {
            Some(UpstreamView::Http { url: url.clone() })
        }
        _ => Some(UpstreamView::Stdio {
            command: command.to_vec(),
        }),
    }
}

/// A `tools/call` response's outcome: whether it failed, and its content as
/// text, as `stretto_trace` reads it.
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

/// Read `<session>.flow.jsonl`: its decisions and its runs. A line that is
/// neither, or not JSON (as the last line of a log cut mid-write can be), is
/// skipped.
pub fn read_flow_log(path: &Path) -> (Vec<FlowDecision>, Vec<FlowRun>) {
    let (mut decisions, mut runs) = (Vec::new(), Vec::new());
    for v in read_json_lines(path) {
        let after = v.get("after").map(id_string).unwrap_or_default();
        if let Some(run) = v.get("run") {
            runs.push(FlowRun {
                after,
                call: run
                    .get("call")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                failed: run.get("failed").and_then(Value::as_bool).unwrap_or(false),
                max_lookups: run.get("max_lookups").and_then(Value::as_u64).unwrap_or(0) as usize,
                sites: run
                    .get("sites")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        let s = s.as_array()?;
                        Some((
                            s.first()?.as_str()?.to_string(),
                            s.get(1).cloned().unwrap_or(Value::Null),
                            s.get(2).and_then(Value::as_f64).unwrap_or(0.0),
                        ))
                    })
                    .collect(),
                surprise: run.get("surprise").and_then(Value::as_f64),
            });
            continue;
        }
        let action = match v.get("action").and_then(Value::as_str) {
            Some("lookup") => DecisionAction::Lookup,
            Some("hand_back") => DecisionAction::HandBack,
            _ => continue,
        };
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        decisions.push(FlowDecision {
            after,
            address: text("address").unwrap_or_default(),
            site: text("site").unwrap_or_default(),
            action,
            tool: text("tool"),
            arguments: v.get("arguments").filter(|a| !a.is_null()).cloned(),
            prob: v.get("prob").and_then(Value::as_f64),
            probs: v
                .get("probs")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter_map(|(k, p)| Some((k.clone(), p.as_f64()?)))
                .collect(),
            binding: v.get("binding").and_then(Value::as_f64),
            reason: text("reason"),
            shadow: v.get("shadow").and_then(Value::as_bool).unwrap_or(false),
            ms: v
                .get("ms")
                .and_then(|m| m.as_u64().or_else(|| m.as_f64().map(|f| f.max(0.0) as u64))),
        });
    }
    (decisions, runs)
}

/// Each line of `path` that is JSON.
pub fn read_json_lines(path: &Path) -> Vec<Value> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

/// The first non-empty line of a file, as JSON.
fn first_line(path: &Path) -> Option<Value> {
    let file = std::fs::File::open(path).ok()?;
    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str(&l).ok())
}

/// An id as the API shows it: a string as is, anything else as JSON.
pub fn id_string(id: &Value) -> String {
    match id {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The call id of a commit, from the key it is held by (the id as JSON).
fn commit_id(key: &str) -> String {
    serde_json::from_str::<Value>(key).map_or_else(|_| key.to_string(), |v| id_string(&v))
}

fn event_from(peer: Peer) -> EventFrom {
    match peer {
        Peer::Client => EventFrom::Client,
        Peer::Server => EventFrom::Server,
        Peer::Proxy => EventFrom::Proxy,
        Peer::Context => EventFrom::Context,
    }
}

/// `v` on one line; nothing for null.
fn compact(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `text` cut at `max` characters, marked with `…` when cut.
pub fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// `text` cut at `max` bytes, on a character's boundary, and whether it was.
pub fn cut_bytes(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::keys;
    use serde_json::json;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stretto-console-sessions-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/home")
            .join(rel)
    }

    /// The fixture session in `dir` (relative to the fixtures' home).
    fn fixture_session(dir: &str, which: usize) -> SessionFile {
        let root = fixture("");
        let catalog = crate::data::catalog(&root);
        catalog
            .sessions
            .iter()
            .filter(|s| s.rel.starts_with(dir))
            .nth(which)
            .cloned()
            .unwrap()
    }

    #[test]
    fn a_recorded_session_has_the_agents_calls_turns_and_conversation() {
        let file = fixture_session("logs/shop/", 0);
        let a = analyze(&file).unwrap();
        let s = &a.summary;
        assert_eq!(s.mode, SessionMode::Recorded);
        assert_eq!(s.domain.as_deref(), Some("shop"));
        assert_eq!(s.agent.as_deref(), Some("quickstart"));
        // find the user, their details, two orders, and a cancellation.
        assert_eq!((s.tool_calls, s.writes, s.errors), (5, 1, 0));
        assert_eq!((s.flow_lookups, s.hand_backs, s.shadow_lookups), (0, 0, 0));
        // One turn per call (the scripted agent waits for each answer), and
        // the agent's two messages.
        assert_eq!(s.llm_turns, 7);
        assert_eq!(a.turns.len(), 7);
        assert_eq!(
            s.upstream,
            Some(UpstreamView::Stdio {
                command: vec!["stretto-mcp-demo".into(), "--world".into(), "retail".into()]
            })
        );
        assert!(!s.has_flow_log && !s.has_confirm_log);
        assert_eq!(a.tools.len(), 4);
        let cancel = a
            .tools
            .iter()
            .find(|t| t.name == "cancel_pending_order")
            .unwrap();
        assert_eq!(
            (cancel.kind, cancel.read_only_hint),
            (ToolKind::Write, Some(false))
        );
        assert_eq!(
            cancel.contract.as_deref(),
            Some("order_id:string!, reason:string!")
        );
        assert!(a
            .calls
            .iter()
            .all(|c| c.by == CallBy::Agent && c.ok == Some(true)));
        let first = &a.calls[0];
        assert_eq!(first.tool, "find_user_id_by_email");
        assert_eq!(first.kind, ToolKind::Read);
        assert_eq!(first.result_text.as_deref(), Some("user_1"));
        assert_eq!(first.turn, Some(0));
        let details = &a.calls[1];
        assert_eq!(
            details.result_json.as_ref().unwrap()["orders"],
            json!(["#W1a", "#W1b"])
        );
        assert_eq!(a.context.len(), 4);
        assert_eq!(a.context[0].role, Role::User);
        // Every line of the log is an event.
        let lines = std::fs::read_to_string(&file.path).unwrap().lines().count();
        assert_eq!(a.events.len(), lines - 1);
        assert!(a.events.iter().any(|e| e.kind == EventKind::Context));
        let response = a
            .events
            .iter()
            .find(|e| e.kind == EventKind::Response && e.method.as_deref() == Some("tools/call"))
            .unwrap();
        assert_eq!(response.summary, "user_1");
        assert_eq!(a.header["stretto_mcp_log"], 2);
    }

    #[test]
    fn a_served_sessions_lookups_follow_the_agents_call_and_their_decisions() {
        let file = fixture_session("served/shop/", 0);
        let a = analyze(&file).unwrap();
        let s = &a.summary;
        assert_eq!(s.mode, SessionMode::Served);
        assert!(s.has_flow_log);
        // The flow looked up the details and both orders after the first
        // call; the agent called the email lookup and the cancellation.
        assert_eq!((s.tool_calls, s.flow_lookups), (2, 3));
        assert_eq!(s.hand_backs, 2);
        let lookups: Vec<&CallView> = a.calls.iter().filter(|c| c.by == CallBy::Flow).collect();
        assert_eq!(lookups.len(), 3);
        for (k, l) in lookups.iter().enumerate() {
            assert_eq!(l.id, format!("stretto-{}", k + 1));
            assert_eq!(l.after.as_deref(), Some("3"));
            assert_eq!(l.turn, None);
            let d = &a.decisions[l.decision.unwrap()];
            assert_eq!(d.action, DecisionAction::Lookup);
            assert_eq!(d.tool.as_deref(), Some(l.tool.as_str()));
            assert_eq!(d.arguments.as_ref(), Some(&l.arguments));
        }
        assert_eq!(a.runs.len(), 2);
        assert_eq!(a.runs[0].call, "find_user_id_by_email");
        assert_eq!(a.runs[0].sites[0].0, "decide#0");
        // The proxy's copy of the first answer, with the lookups appended,
        // is an event, not a second result.
        assert!(a.events.iter().any(|e| e.from == EventFrom::Proxy
            && e.kind == EventKind::Response
            && e.summary.contains("Also looked up")));
        assert_eq!(a.calls[0].result_text.as_deref(), Some("user_41"));
        // The agent's turns leave the flow's lookups out.
        assert_eq!(s.llm_turns, 2 + 2);
    }

    #[test]
    fn shadow_and_guarded_sessions() {
        let shadow = analyze(&fixture_session("shadow/shop/", 0)).unwrap();
        assert_eq!(shadow.summary.mode, SessionMode::Shadow);
        assert_eq!(shadow.summary.flow_lookups, 0);
        assert!(shadow.summary.shadow_lookups >= 3);
        assert!(shadow.decisions.iter().all(|d| d.shadow));
        assert!(shadow.calls.iter().all(|c| c.decision.is_none()));

        let retail = analyze(&fixture_session("logs/retail/", 0)).unwrap();
        let s = &retail.summary;
        assert_eq!(s.mode, SessionMode::Recorded);
        assert!(s.has_confirm_log);
        assert_eq!(s.errors, 1);
        assert_eq!(retail.calls[0].ok, Some(false));
        assert_eq!(retail.confirmations.len(), 1);
        assert_eq!(retail.confirmations[0]["tool"], "cancel_pending_order");
        assert_eq!(s.agent.as_deref(), Some("fixtures"));
    }

    #[test]
    fn a_hand_written_log_with_a_commit_a_batch_an_error_and_a_long_result() {
        let dir = temp("handmade");
        let long = "x".repeat(RESULT_LIMIT + 10);
        let lines = [
            json!({"stretto_mcp_log":2,"session":"s1","started_unix_ms":5,"server_command":["https://h.example/mcp"],"domain":"d","agent_model":"m"}),
            json!({"t_ms":0,"from":"client","message":{"jsonrpc":"2.0","id":1,"method":"tools/list"}}),
            json!({"t_ms":1,"from":"server","message":{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"get","annotations":{"readOnlyHint":true}},{"name":"put","annotations":{"readOnlyHint":false,"destructiveHint":true}}]}}}),
            json!({"t_ms":2,"from":"client","message":[
                {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get","arguments":{"a":1}}},
                {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get"}}]}),
            json!({"t_ms":3,"from":"server","message":{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"bad"}}}),
            json!({"t_ms":4,"from":"server","message":{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":long}]}}}),
            json!({"t_ms":5,"from":"client","message":{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"stretto_commit","arguments":{"calls":[{"name":"put","arguments":{}}]}}}}),
            json!({"t_ms":6,"from":"proxy","message":{"jsonrpc":"2.0","id":"stretto-1","method":"tools/call","params":{"name":"put","arguments":{}}}}),
            json!({"t_ms":7,"from":"server","message":{"jsonrpc":"2.0","id":"stretto-1","result":{"content":[{"type":"text","text":"done"}]}}}),
            json!({"t_ms":8,"from":"proxy","message":{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"put {}:\ndone"}]}}}),
            json!({"t_ms":9,"from":"server","raw":"not json"}),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(dir.join("s1.jsonl"), text).unwrap();
        let file = SessionFile {
            key: keys::assign(&[("s1".into(), "s1.jsonl".into())], &[])[0].clone(),
            path: dir.join("s1.jsonl"),
            rel: "s1.jsonl".into(),
            flow_log: None,
            confirm_log: None,
        };
        let a = analyze(&file).unwrap();
        let s = &a.summary;
        assert_eq!(
            s.upstream,
            Some(UpstreamView::Http {
                url: "https://h.example/mcp".into()
            })
        );
        assert_eq!(s.agent.as_deref(), Some("m"));
        // The two calls of the batch share a turn; the commit is its own.
        assert_eq!((s.tool_calls, s.llm_turns, s.errors), (3, 2, 1));
        assert_eq!(a.turns[0].calls, ["2", "3"]);
        // The commit's call is the agent's write, not a flow lookup, and the
        // session was served (the proxy answered).
        assert_eq!(s.flow_lookups, 0);
        assert_eq!(s.mode, SessionMode::Served);
        let put = a.calls.iter().find(|c| c.id == "stretto-1").unwrap();
        assert_eq!(
            (put.by, put.kind, put.after.as_deref()),
            (CallBy::Agent, ToolKind::Write, Some("4"))
        );
        let bad = &a.calls[0];
        assert_eq!(
            (bad.ok, bad.result_text.as_deref()),
            (Some(false), Some("bad"))
        );
        let long_call = &a.calls[1];
        assert!(long_call.result_truncated && a.truncated);
        assert_eq!(long_call.result_text.as_ref().unwrap().len(), RESULT_LIMIT);
        assert!(long_call.result_json.is_none());
        assert_eq!(long_call.latency_ms, Some(2));
        assert!(a
            .events
            .iter()
            .any(|e| e.kind == EventKind::Raw && e.summary == "not json"));
        assert!(a
            .events
            .iter()
            .any(|e| e.kind == EventKind::Error && e.summary == "error -32602: bad"));
        let tools: Vec<(&str, Option<bool>)> = a
            .tools
            .iter()
            .map(|t| (t.name.as_str(), t.destructive_hint))
            .collect();
        assert_eq!(tools, [("get", None), ("put", Some(true))]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_is_not_a_log_says_why() {
        let dir = temp("broken");
        std::fs::write(dir.join("x.jsonl"), "{\"not\":\"a log\"}\n").unwrap();
        let file = SessionFile {
            key: "x".into(),
            path: dir.join("x.jsonl"),
            rel: "x.jsonl".into(),
            flow_log: None,
            confirm_log: None,
        };
        let loaded = load(&file);
        assert!(loaded
            .summary
            .unwrap_err()
            .contains("not a stretto MCP log header"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn text_is_cut_on_character_boundaries() {
        assert_eq!(cut("héllo", 2), "hé…");
        assert_eq!(cut("hi", 2), "hi");
        assert_eq!(cut_bytes("héllo", 2), ("h".to_string(), true));
        assert_eq!(cut_bytes("hi", 2), ("hi".to_string(), false));
    }
}
