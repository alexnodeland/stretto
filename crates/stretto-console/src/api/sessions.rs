//! Sessions: the proxy's recorded logs, as a list and one by one.

use super::{blocking, ApiError, ApiResult, Ok as OkBody};
use crate::data::{self, sessions as parse};
use crate::Shared;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use stretto_trace::ToolKind;
#[cfg(feature = "ts")]
use ts_rs::TS;

/// Whether a session was only recorded, ran a flow in shadow, or was served
/// one: `served` if the flow looked anything up (or the proxy answered a
/// call itself), `shadow` if its decisions were logged in shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Recorded,
    Shadow,
    Served,
}

impl SessionMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "recorded" => Some(Self::Recorded),
            "shadow" => Some(Self::Shadow),
            "served" => Some(Self::Served),
            _ => None,
        }
    }
}

/// The server a session's proxy fronted, from its log's header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamView {
    /// A command the proxy started, credential-looking arguments redacted.
    Stdio { command: Vec<String> },
    /// A Streamable HTTP server, its URL without credentials.
    Http { url: String },
}

/// One session, in short.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct SessionSummary {
    pub key: String,
    /// The log, relative to the data directory.
    pub path: String,
    pub session_id: String,
    pub domain: Option<String>,
    /// The header's `agent_model`, else the host's name from `initialize`
    /// (`clientInfo.name`).
    pub agent: Option<String>,
    pub started_unix_ms: u64,
    /// From the proxy's start to its last logged line.
    pub duration_ms: u64,
    pub mode: SessionMode,
    /// The agent's own `tools/call`s.
    pub tool_calls: usize,
    /// LLM turns, inferred as `stretto_trace` infers them: calls sent while
    /// another of the turn awaits its answer share it, and each message of
    /// the agent's own (`--context`) is one.
    pub llm_turns: usize,
    /// The agent's calls that failed.
    pub errors: usize,
    /// Lookups the proxy made for the flow.
    pub flow_lookups: usize,
    /// Decisions to hand back, served (not shadow).
    pub hand_backs: usize,
    /// Lookups the flow would have made, in shadow.
    pub shadow_lookups: usize,
    /// Whether the flow handed back in it, served or in shadow, because the
    /// session surprised it: its surprise gate tripped.
    pub surprised: bool,
    /// The agent's calls to tools the session's `tools/list` marks
    /// `readOnlyHint: false`.
    pub writes: usize,
    pub upstream: Option<UpstreamView>,
    /// The log's size.
    pub size_bytes: u64,
    pub has_flow_log: bool,
    pub has_confirm_log: bool,
}

/// A page of sessions, newest first.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct SessionList {
    /// Sessions matching the filters, on every page.
    pub total: usize,
    pub items: Vec<SessionSummary>,
}

/// A tool a server listed, as `tools/list` described it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ToolInfo {
    pub name: String,
    /// From `readOnlyHint`: `true` is `read`, `false` is `write`, none is
    /// `generic` (unknown).
    pub kind: ToolKind,
    pub description: Option<String>,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    /// Its input contract, as a flow pins it: `order_id:string!, reason:string`.
    pub contract: Option<String>,
}

/// An LLM turn: calls the agent sent together, or a message of its own.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Turn {
    pub index: usize,
    /// When its first call was sent (or its message logged), in
    /// milliseconds after the proxy started.
    pub start_ms: u64,
    /// When its last answer came.
    pub end_ms: u64,
    /// Its calls' ids; none for a message.
    pub calls: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum CallBy {
    /// The agent's own call (and the calls `stretto_commit` made for it).
    Agent,
    /// A lookup the proxy made for the flow.
    Flow,
}

/// One `tools/call`, the agent's or the flow's.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct CallView {
    /// The JSON-RPC id: a string as is, a number in decimal.
    pub id: String,
    pub by: CallBy,
    pub tool: String,
    pub kind: ToolKind,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub arguments: Value,
    pub t_ms: u64,
    pub result_t_ms: Option<u64>,
    pub latency_ms: Option<u64>,
    /// Whether it succeeded; `null` when no answer came.
    pub ok: Option<bool>,
    /// The result's text items, joined; cut at 64 KiB.
    pub result_text: Option<String>,
    /// The text, when it parses as JSON (and was not cut).
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub result_json: Option<Value>,
    pub result_truncated: bool,
    /// The agent's turn, an index into `turns`.
    pub turn: Option<usize>,
    /// For the flow's lookups: the agent's call they followed.
    pub after: Option<String>,
    /// For the flow's lookups: the decision that made it, an index into
    /// `decisions`.
    pub decision: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// A message of the conversation, as the host handed it to the proxy
/// (`--context`).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ContextMessage {
    pub t_ms: u64,
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Lookup,
    HandBack,
}

/// One decision of the flow, from `<session>.flow.jsonl`.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowDecision {
    /// The agent's call it followed.
    pub after: String,
    /// Its site in the flow's run, such as `decide#0`.
    pub address: String,
    /// The call just made: the tool, with ` (error)` if it failed.
    pub site: String,
    pub action: DecisionAction,
    /// The lookup, for `lookup`.
    pub tool: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub arguments: Option<Value>,
    /// The lookup's probability under the flow's decider. The flow makes the
    /// lookup when this, and this times `binding`, both reach the threshold.
    pub prob: Option<f64>,
    /// The decider's probability of each option.
    pub probs: BTreeMap<String, f64>,
    /// The chance that the lookup's bound arguments are the agent's own.
    pub binding: Option<f64>,
    /// Why it handed back.
    pub reason: Option<String>,
    /// Logged in shadow: nothing was looked up.
    pub shadow: bool,
    /// How long the decision took.
    pub ms: Option<u64>,
}

/// One run of the flow after a call, from `<session>.flow.jsonl`.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowRun {
    pub after: String,
    /// The tool whose call just returned.
    pub call: String,
    pub failed: bool,
    pub max_lookups: usize,
    /// Each site of the run: `[address, value, logp]`.
    #[cfg_attr(feature = "ts", ts(type = "Array<[string, unknown, number]>"))]
    pub sites: Vec<(String, Value, f64)>,
    /// How unexpected the server's answers were, in nats.
    pub surprise: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum EventFrom {
    Client,
    Server,
    Proxy,
    Context,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Request,
    Response,
    Notification,
    /// A JSON-RPC error, or a result with `isError`.
    Error,
    Context,
    /// A line that was not JSON.
    Raw,
}

/// A line of the log, in short.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct TimelineEvent {
    pub t_ms: u64,
    pub from: EventFrom,
    pub kind: EventKind,
    /// The method; for a response, the method of the request it answers.
    pub method: Option<String>,
    pub id: Option<String>,
    pub summary: String,
}

/// One session in full.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct SessionDetail {
    pub summary: SessionSummary,
    /// The log's header line, as written.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub header: Value,
    /// The tools the server listed, by name.
    pub tools: Vec<ToolInfo>,
    pub turns: Vec<Turn>,
    /// Every `tools/call`, the agent's and the flow's, in order.
    pub calls: Vec<CallView>,
    pub context: Vec<ContextMessage>,
    pub decisions: Vec<FlowDecision>,
    pub runs: Vec<FlowRun>,
    /// The lines of `<session>.confirm.jsonl`, as written.
    #[cfg_attr(feature = "ts", ts(type = "Array<unknown>"))]
    pub confirmations: Vec<Value>,
    pub events: Vec<TimelineEvent>,
    /// Whether any result was cut at 64 KiB.
    pub truncated: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub domain: Option<String>,
    pub mode: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// The most sessions one page holds.
pub const MAX_LIMIT: usize = 500;

/// `GET /api/sessions`
pub async fn list(
    State(state): State<Shared>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<SessionList>> {
    let mode = match query.mode.as_deref().filter(|m| !m.is_empty()) {
        Some(m) => Some(SessionMode::parse(m).ok_or_else(|| {
            ApiError::bad_request(format!("mode {m:?}: expected recorded, shadow or served"))
        })?),
        None => None,
    };
    let limit = query.limit.unwrap_or(50).min(MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    blocking(&state, move |state| {
        let all = parse::all(state);
        let needle = query
            .q
            .as_deref()
            .map(str::to_lowercase)
            .filter(|q| !q.is_empty());
        let domain = query.domain.filter(|d| !d.is_empty());
        let matching: Vec<SessionSummary> = all
            .into_iter()
            .filter(|(s, _)| domain.as_ref().is_none_or(|d| s.domain.as_ref() == Some(d)))
            .filter(|(s, _)| mode.is_none_or(|m| s.mode == m))
            .filter(|(s, loaded)| needle.as_ref().is_none_or(|q| parse::matches(s, loaded, q)))
            .map(|(s, _)| s)
            .collect();
        Ok(Json(SessionList {
            total: matching.len(),
            items: matching.into_iter().skip(offset).take(limit).collect(),
        }))
    })
    .await
}

/// `GET /api/sessions/:key`
pub async fn detail(
    State(state): State<Shared>,
    Path(key): Path<String>,
) -> ApiResult<Json<SessionDetail>> {
    blocking(&state, move |state| {
        let catalog = data::catalog(state.data_dir());
        let file = catalog
            .session(&key)
            .ok_or_else(|| ApiError::not_found(format!("no session {key:?}")))?;
        let analysis = parse::analyze(file).map_err(|e| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{} is not a session log stretto can read: {e}", file.rel),
            )
        })?;
        Ok(Json(analysis.detail()))
    })
    .await
}

/// `GET /api/sessions/:key/raw`: the log, to download.
pub async fn raw(State(state): State<Shared>, Path(key): Path<String>) -> ApiResult<Response> {
    let file = blocking(&state, move |state| {
        let catalog = data::catalog(state.data_dir());
        catalog
            .session(&key)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("no session {key:?}")))
    })
    .await?;
    let bytes = read(&file.path, &file.rel).await?;
    Ok(download(bytes, "application/x-ndjson", &file.path))
}

/// The bytes of the file at `path`, named `rel` if it cannot be read.
pub async fn read(path: &std::path::Path, rel: &str) -> ApiResult<Vec<u8>> {
    tokio::fs::read(path)
        .await
        .map_err(|e| ApiError::internal(format!("reading {rel}: {e}")))
}

/// `DELETE /api/sessions/:key`: the log and the logs beside it go to the
/// trash.
pub async fn delete(
    State(state): State<Shared>,
    Path(key): Path<String>,
) -> ApiResult<Json<OkBody>> {
    blocking(&state, move |state| {
        let catalog = data::catalog(state.data_dir());
        let file = catalog
            .session(&key)
            .ok_or_else(|| ApiError::not_found(format!("no session {key:?}")))?;
        let mut moving = vec![file.path.clone()];
        moving.extend(file.flow_log.clone());
        moving.extend(file.confirm_log.clone());
        super::trash(state, &moving)?;
        Ok(Json(OkBody { ok: true }))
    })
    .await
}

/// `bytes` as a file to save, named as `path` is.
pub fn download(bytes: Vec<u8>, content_type: &'static str, path: &std::path::Path) -> Response {
    let name: String = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut response = Body::from(bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    response
}
