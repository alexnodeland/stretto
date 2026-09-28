//! Servers: the registry of the MCP servers stretto fronts, the host
//! configuration for each (as `stretto init` prints it), and a live test of
//! the connection.

use super::sessions::{ToolInfo, UpstreamView};
use super::{blocking, ApiError, ApiResult, Ok as OkBody};
use crate::api::flows::FlowSummary;
use crate::data::registry::{
    self, JudgeMode, Registry, ServerEntry, ServerInput, ServerMode, Upstream,
};
use crate::data::{self, flows, paths};
use crate::{probe, Shared, State};
use axum::body::Bytes;
use axum::extract::{Path, Query, State as AxumState};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use stretto_report::init::{self, Host, Served, Setup};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// A server in the registry, with what the console knows about it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ServerView {
    #[serde(flatten)]
    pub entry: ServerEntry,
    /// The `stretto-proxy` arguments that run it, as `stretto init` writes
    /// them (without the program itself).
    pub proxy_args: Vec<String>,
    /// Recorded sessions whose domain is the server's name.
    pub sessions: usize,
    pub last_session_unix_ms: Option<u64>,
    /// Its flow, if it names one that is in the data directory.
    pub flow_summary: Option<FlowSummary>,
    /// What stands in the way of running it as configured.
    pub issues: Vec<String>,
}

/// An upstream seen in recorded sessions' headers.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct DiscoveredUpstream {
    pub domain: String,
    pub upstream: UpstreamView,
    pub sessions: usize,
    pub last_seen_unix_ms: u64,
    /// Whether the registry has a server of that name.
    pub registered: bool,
}

/// `GET /api/servers`
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ServerList {
    pub items: Vec<ServerView>,
    pub discovered: Vec<DiscoveredUpstream>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum ConfigLanguage {
    Shell,
    Json,
}

/// What to give an MCP host: `stretto init`'s output for the server.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct HostConfig {
    /// `claude-code`, `claude-desktop`, `cursor` or `vscode`.
    pub host: String,
    /// `shell` for Claude Code's `claude mcp add`, else `json`.
    pub language: ConfigLanguage,
    pub snippet: String,
    /// Where it goes.
    pub placement: String,
    /// The steps from here to a served flow.
    pub next_steps: String,
}

/// `POST /api/servers/:name/probe`: the server, asked `initialize` and
/// `tools/list` now.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ProbeResult {
    pub ok: bool,
    /// How long the whole exchange took.
    pub ms: u64,
    pub error: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub protocol_version: Option<String>,
    pub instructions: Option<String>,
    pub tools: Vec<ToolInfo>,
    /// What the listed tools say against the server's flow.
    pub flow_warnings: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ConfigQuery {
    pub host: Option<String>,
}

/// `GET /api/servers`
pub async fn list(AxumState(state): AxumState<Shared>) -> ApiResult<Json<ServerList>> {
    blocking(&state, |state| {
        let registry = load(state)?;
        let sessions = data::sessions::all(state);
        let items = registry
            .servers
            .iter()
            .map(|e| view(state, e, &sessions))
            .collect();
        let mut discovered: BTreeMap<(String, String), DiscoveredUpstream> = BTreeMap::new();
        for (s, _) in &sessions {
            let (Some(domain), Some(upstream)) = (&s.domain, &s.upstream) else {
                continue;
            };
            let key = (
                domain.clone(),
                serde_json::to_string(upstream).unwrap_or_default(),
            );
            let d = discovered.entry(key).or_insert_with(|| DiscoveredUpstream {
                domain: domain.clone(),
                upstream: upstream.clone(),
                sessions: 0,
                last_seen_unix_ms: 0,
                registered: registry.servers.iter().any(|e| e.name == *domain),
            });
            d.sessions += 1;
            d.last_seen_unix_ms = d.last_seen_unix_ms.max(s.started_unix_ms);
        }
        let mut discovered: Vec<DiscoveredUpstream> = discovered.into_values().collect();
        discovered.sort_by_key(|d| std::cmp::Reverse(d.last_seen_unix_ms));
        Ok(Json(ServerList { items, discovered }))
    })
    .await
}

/// `POST /api/servers`: 409 if the name is taken.
pub async fn create(
    AxumState(state): AxumState<Shared>,
    body: Bytes,
) -> ApiResult<Json<ServerView>> {
    let input = parse_input(&body)?;
    let _held = state.registry_lock.lock().await;
    blocking(&state, move |state| {
        registry::validate(&input, state.data_dir(), state.config.home.as_deref())
            .map_err(ApiError::bad_request)?;
        let mut registry = load(state)?;
        if registry.servers.iter().any(|e| e.name == input.name) {
            return Err(ApiError::conflict(format!(
                "a server named {:?} is registered already",
                input.name
            )));
        }
        let now = state.now();
        let entry = input.entry(now, now);
        registry.servers.push(entry.clone());
        save(state, &registry)?;
        let sessions = data::sessions::all(state);
        Ok(Json(view(state, &entry, &sessions)))
    })
    .await
}

/// `PUT /api/servers/:name`: the server replaced, keeping when it was
/// created; a new name renames it, unless that name is taken (409).
pub async fn update(
    AxumState(state): AxumState<Shared>,
    Path(name): Path<String>,
    body: Bytes,
) -> ApiResult<Json<ServerView>> {
    let input = parse_input(&body)?;
    let _held = state.registry_lock.lock().await;
    blocking(&state, move |state| {
        registry::validate(&input, state.data_dir(), state.config.home.as_deref())
            .map_err(ApiError::bad_request)?;
        let mut registry = load(state)?;
        let Some(at) = registry.servers.iter().position(|e| e.name == name) else {
            return Err(ApiError::not_found(format!("no server {name:?}")));
        };
        if input.name != name && registry.servers.iter().any(|e| e.name == input.name) {
            return Err(ApiError::conflict(format!(
                "a server named {:?} is registered already",
                input.name
            )));
        }
        let entry = input.entry(registry.servers[at].created_unix_ms, state.now());
        registry.servers[at] = entry.clone();
        save(state, &registry)?;
        let sessions = data::sessions::all(state);
        Ok(Json(view(state, &entry, &sessions)))
    })
    .await
}

/// `DELETE /api/servers/:name`: out of the registry; its sessions and flow
/// stay.
pub async fn delete(
    AxumState(state): AxumState<Shared>,
    Path(name): Path<String>,
) -> ApiResult<Json<OkBody>> {
    let _held = state.registry_lock.lock().await;
    blocking(&state, move |state| {
        let mut registry = load(state)?;
        let before = registry.servers.len();
        registry.servers.retain(|e| e.name != name);
        if registry.servers.len() == before {
            return Err(ApiError::not_found(format!("no server {name:?}")));
        }
        save(state, &registry)?;
        Ok(Json(OkBody { ok: true }))
    })
    .await
}

/// `GET /api/servers/:name/config?host=`
pub async fn config(
    AxumState(state): AxumState<Shared>,
    Path(name): Path<String>,
    Query(query): Query<ConfigQuery>,
) -> ApiResult<Json<HostConfig>> {
    let host = match query.host.as_deref().unwrap_or("claude-code") {
        "claude-code" => Host::ClaudeCode,
        "claude-desktop" => Host::ClaudeDesktop,
        "cursor" => Host::Cursor,
        "vscode" => Host::VsCode,
        other => {
            return Err(ApiError::bad_request(format!(
                "host {other:?}: expected claude-code, claude-desktop, cursor or vscode"
            )))
        }
    };
    blocking(&state, move |state| {
        let registry = load(state)?;
        let entry = registry
            .servers
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| ApiError::not_found(format!("no server {name:?}")))?;
        let setup = setup(state, entry, host);
        let again = format!(
            "stretto init --host {} --domain {}",
            host.name(),
            entry.name
        );
        Ok(Json(HostConfig {
            host: host.name().to_string(),
            language: if host == Host::ClaudeCode {
                ConfigLanguage::Shell
            } else {
                ConfigLanguage::Json
            },
            snippet: init::snippet(host, &setup),
            placement: host.placement().to_string(),
            next_steps: init::next_steps(&setup, &again),
        }))
    })
    .await
}

/// `POST /api/servers/:name/probe`
pub async fn probe(
    AxumState(state): AxumState<Shared>,
    Path(name): Path<String>,
) -> ApiResult<Json<ProbeResult>> {
    let entry = blocking(&state, move |state| {
        load(state)?
            .servers
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| ApiError::not_found(format!("no server {name:?}")))
    })
    .await?;
    let mut result = probe::probe(&entry.upstream).await;
    if result.ok {
        let upstream = entry.clone();
        let tools = result.tools.clone();
        result.flow_warnings = blocking(&state, move |state| {
            let Some(path) =
                registry::flow_path(&upstream, state.data_dir(), state.config.home.as_deref())
            else {
                return Ok(Vec::new());
            };
            let flow = flows::load(&path).flow;
            Ok(flow
                .map(|flow| flows::tool_warnings(&flow, &tools, "the server"))
                .unwrap_or_default())
        })
        .await?;
    }
    Ok(Json(result))
}

/// The request's server, or a 400 saying what is wrong with it.
fn parse_input(body: &[u8]) -> ApiResult<ServerInput> {
    serde_json::from_slice(body)
        .map_err(|e| ApiError::bad_request(format!("the server is not as expected: {e}")))
}

fn load(state: &State) -> ApiResult<Registry> {
    registry::load(state.data_dir()).map_err(ApiError::internal)
}

fn save(state: &State, registry: &Registry) -> ApiResult<()> {
    registry::save(state.data_dir(), registry).map_err(ApiError::internal)
}

/// `entry` with the console's knowledge of it.
pub fn view(
    state: &State,
    entry: &ServerEntry,
    sessions: &[(
        super::sessions::SessionSummary,
        std::sync::Arc<data::sessions::Loaded>,
    )],
) -> ServerView {
    let mine: Vec<u64> = sessions
        .iter()
        .filter(|(s, _)| s.domain.as_deref() == Some(entry.name.as_str()))
        .map(|(s, _)| s.started_unix_ms)
        .collect();
    let home = state.config.home.as_deref();
    let mut issues = Vec::new();
    let flow_path = registry::flow_path(entry, state.data_dir(), home);
    let mut flow_summary = None;
    let loaded = flow_path.as_ref().map(|p| flows::load(p));
    match (&entry.flow, &flow_path, &loaded) {
        (None, _, _) if entry.mode != ServerMode::Record => {
            let mode = if entry.mode == ServerMode::Shadow {
                "shadow"
            } else {
                "serve"
            };
            issues.push(format!("mode is {mode} but no flow is set"))
        }
        (Some(_), Some(path), Some(loaded)) => {
            if !path.is_file() {
                issues.push(format!("flow not found: {}", paths::display(path, home)));
            } else {
                match &loaded.flow {
                    Err(e) => issues.push(format!("the flow does not load: {e}")),
                    Ok(flow) => {
                        if flow.domain() != entry.name {
                            issues.push(format!(
                                "the flow's domain is {}, not {}: learn it from this server's \
                                 sessions",
                                flow.domain(),
                                entry.name
                            ));
                        }
                        match entry.decider.map(|d| d.decider()) {
                            Some(stretto_report::flow::Decider::Arbiter) if !flow.has_arbiter() => {
                                issues
                                    .push("decider is arbiter, but the flow has no arbiter".into())
                            }
                            Some(stretto_report::flow::Decider::Reach) if !flow.has_reach() => {
                                issues.push(
                                    "decider is reach, but the flow holds no reach counts: \
                                     learn it again"
                                        .into(),
                                )
                            }
                            _ => {}
                        }
                        if entry.mode == ServerMode::Record {
                            issues.push(
                                "mode is record, so the flow is not run: choose shadow or serve"
                                    .into(),
                            );
                        }
                    }
                }
                let catalog = data::catalog(state.data_dir());
                if let Some(file) = catalog.flow_at(path) {
                    let registry = registry::load(state.data_dir()).unwrap_or_default();
                    let mut summary =
                        flows::summary(file, loaded, flows::served_by(file, &registry, state));
                    summary.stage = flows::stage_link(file, &catalog.flows);
                    flow_summary = Some(summary);
                }
            }
        }
        (Some(flow), None, _) => issues.push(format!("the flow's path cannot be used: {flow}")),
        _ => {}
    }
    if let Upstream::Stdio { command, .. } = &entry.upstream {
        if let Some(program) = command.first() {
            let p = std::path::Path::new(program);
            let found = if p.components().count() > 1 {
                p.is_file()
            } else {
                stretto_report::doctor::which(program, &state.config.env.path).is_some()
            };
            if !found {
                issues.push(format!(
                    "{program} is not found on the console's PATH: the host must find it"
                ));
            }
        }
    }
    ServerView {
        proxy_args: setup(state, entry, Host::ClaudeCode).args(),
        sessions: mine.len(),
        last_session_unix_ms: mine.iter().max().copied(),
        flow_summary,
        issues,
        entry: entry.clone(),
    }
}

/// `entry` as `stretto init` sets a server up for `host`: paths as the host
/// passes them (`~/…` under the home directory, else absolute), and the
/// proxy named as `stretto init` names it.
pub fn setup(state: &State, entry: &ServerEntry, host: Host) -> Setup {
    let root = state.data_dir();
    let home = state.config.home.as_deref();
    let shown = |p: &str| match paths::resolve(root, home, p) {
        Ok(path) => paths::display(&path, home),
        Err(_) => p.to_string(),
    };
    let shadow = entry.mode == ServerMode::Shadow;
    let record = match &entry.record_dir {
        Some(dir) => shown(dir),
        None => paths::display(
            &root
                .join(if shadow { "shadow" } else { "logs" })
                .join(&entry.name),
            home,
        ),
    };
    let flow = match (&entry.flow, entry.mode) {
        (Some(path), ServerMode::Shadow | ServerMode::Serve) => {
            let loaded =
                registry::flow_path(entry, root, home).and_then(|p| flows::load(&p).flow.ok());
            Some(Served {
                path: shown(path),
                arbiter: loaded.as_ref().is_some_and(|f| f.has_arbiter()),
                reach: loaded.as_ref().is_none_or(|f| f.has_reach()),
                shadow,
                decide_with: entry.decider.map(|d| d.decider()),
                threshold: entry.threshold,
            })
        }
        _ => None,
    };
    let (server, upstream) = match &entry.upstream {
        Upstream::Stdio { command, .. } => (command.clone(), None),
        Upstream::Http { url, headers } => (
            Vec::new(),
            Some(init::Upstream {
                url: url.clone(),
                headers: headers
                    .iter()
                    .map(|h| (h.name.clone(), h.env.clone()))
                    .collect(),
            }),
        ),
    };
    Setup {
        domain: entry.name.clone(),
        record,
        flow,
        proxy: proxy_command(state, host),
        server,
        upstream,
        policy: init::Policy {
            guards: entry.guards,
            judge: entry.judge.as_ref().map(|j| init::Judge {
                enforce: j.mode == JudgeMode::Enforce,
                context: j.context.clone(),
            }),
            commit: entry.commit,
            retain_days: entry.retain_days,
        },
    }
}

/// How the host should start `stretto-proxy`, as `stretto init` decides:
/// by name when it is on PATH, except for Claude Desktop, which starts
/// servers with a minimal PATH and gets its full path; by its full path
/// when it is only beside the console.
fn proxy_command(state: &State, host: Host) -> String {
    let name = "stretto-proxy";
    if let Some(found) = stretto_report::doctor::which(name, &state.config.env.path) {
        if host == Host::ClaudeDesktop {
            return found.display().to_string();
        }
        return name.to_string();
    }
    match &state.config.binaries.proxy {
        Some(proxy) => proxy.path.clone(),
        None => name.to_string(),
    }
}
