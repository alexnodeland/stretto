//! Flows: the list, one flow as a reviewer reads it, and what changed
//! between two.

use super::{blocking, ApiError, ApiResult, Ok as OkBody};
use crate::data::registry::{self, DeciderName};
use crate::data::{self, flows as parse};
use crate::Shared;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use stretto_report::flow::Provenance;
use stretto_report::review::{self, BindingView, DiffSection, FlowTool, PromotionView, SiteView};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// A flow's tools, by kind.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ToolCounts {
    pub read: usize,
    pub write: usize,
    pub generic: usize,
}

/// A promoted flow's sites.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct PromotedCounts {
    pub sites_promoted: usize,
    pub sites_scored: usize,
}

/// A flow's part in staged learning (`stretto stage`): set on a staged
/// flow, `NAME.staged.flow.json`, and on the committed flow `NAME.flow.json`
/// beside one.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct StageLink {
    /// Whether this is the staged flow; else it is the committed one.
    pub staged: bool,
    /// The other of the two, by key: none for a staged flow before its
    /// first commit.
    pub other: Option<String>,
    /// Whether the staged flow differs from the committed one, so that
    /// there is something to commit.
    pub pending: bool,
}

/// One flow file, in short. When the file does not load, `error` says why
/// and the fields the flow would give are empty or zero.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowSummary {
    pub key: String,
    /// The file, relative to the data directory.
    pub path: String,
    /// The file's name without `.flow.json`.
    pub name: String,
    pub domain: String,
    /// The file's `stretto_flow`.
    pub format_version: u32,
    /// The stretto that wrote it.
    pub stretto_version: String,
    pub sources: Vec<String>,
    pub habit_episodes: usize,
    pub arbiter_cases: usize,
    pub compiled_unix_ms: u64,
    pub modified_unix_ms: u64,
    pub size_bytes: u64,
    /// How the proxy serves it unless told otherwise.
    pub decider: DeciderName,
    pub has_arbiter: bool,
    pub has_reach: bool,
    pub tools: ToolCounts,
    /// Sites where it may make a lookup.
    pub sites: usize,
    /// Distinct lookups it may make.
    pub lookups: usize,
    pub promoted: Option<PromotedCounts>,
    /// The registry's servers whose flow is this file.
    pub served_by: Vec<String>,
    /// Its part in staged learning, if it has one.
    pub stage: Option<StageLink>,
    pub error: Option<String>,
}

/// `GET /api/flows`
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowList {
    pub items: Vec<FlowSummary>,
}

/// One flow as a reviewer reads it, at a threshold: `stretto flow-show`'s
/// review ([`review::view`], and the same text in `review_markdown`).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowDetail {
    pub summary: FlowSummary,
    pub threshold: f64,
    pub tools: Vec<FlowTool>,
    pub sites: Vec<SiteView>,
    pub bindings: Vec<BindingView>,
    pub provenance: Provenance,
    pub promotion: Option<PromotionView>,
    /// `stretto flow-show` at this threshold.
    pub review_markdown: String,
    /// What the recorded sessions say against the flow, such as a tool it
    /// reads that the server now marks `readOnlyHint: false`.
    pub warnings: Vec<String>,
}

/// What changed from one flow to another, as `stretto flow-diff` lists it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct FlowDiffView {
    /// The flow before, by key.
    pub from: String,
    /// The flow after.
    pub to: String,
    /// The changes a reviewer must look at.
    pub review: Vec<String>,
    /// Every change, heading by heading.
    pub changes: Vec<String>,
    /// The same changes under their headings.
    pub sections: Vec<DiffSection>,
    /// `stretto flow-diff`'s change list.
    pub markdown: String,
}

/// The threshold flows are served with by default (`--flow-threshold`).
pub const DEFAULT_THRESHOLD: f64 = 0.3;

#[derive(Debug, Default, Deserialize)]
pub struct ThresholdQuery {
    pub threshold: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DiffQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub threshold: Option<f64>,
    /// Shares, chances and weights that moved by less are left out (0.05).
    pub tolerance: Option<f64>,
}

fn threshold(t: Option<f64>) -> ApiResult<f64> {
    let t = t.unwrap_or(DEFAULT_THRESHOLD);
    if t.is_finite() && (0.0..=1.0).contains(&t) {
        Ok(t)
    } else {
        Err(ApiError::bad_request(format!(
            "threshold {t}: a probability, from 0 to 1"
        )))
    }
}

/// `GET /api/flows`
pub async fn list(State(state): State<Shared>) -> ApiResult<Json<FlowList>> {
    blocking(&state, |state| {
        let registry = registry::load(state.data_dir()).unwrap_or_default();
        let all = parse::all(state);
        let files: Vec<data::FlowFile> = all.iter().map(|(f, _)| f.clone()).collect();
        let items = all
            .iter()
            .map(|(file, loaded)| {
                let mut s = parse::summary(file, loaded, parse::served_by(file, &registry, state));
                s.stage = parse::stage_link(file, &files);
                s
            })
            .collect();
        Ok(Json(FlowList { items }))
    })
    .await
}

/// `GET /api/flows/:key?threshold=`
pub async fn detail(
    State(state): State<Shared>,
    Path(key): Path<String>,
    Query(query): Query<ThresholdQuery>,
) -> ApiResult<Json<FlowDetail>> {
    let threshold = threshold(query.threshold)?;
    blocking(&state, move |state| {
        let (file, loaded) = find(state, &key)?;
        let flow = loaded.flow.clone().map_err(|e| unreadable(&file.rel, &e))?;
        let registry = registry::load(state.data_dir()).unwrap_or_default();
        let view = review::view(&flow, threshold);
        let mut summary = parse::summary(&file, &loaded, parse::served_by(&file, &registry, state));
        summary.stage = parse::stage_link(&file, &data::catalog(state.data_dir()).flows);
        Ok(Json(FlowDetail {
            summary,
            threshold,
            tools: view.tools,
            sites: view.sites,
            bindings: view.bindings,
            provenance: view.provenance,
            promotion: view.promotion,
            review_markdown: review::show(&flow, threshold),
            warnings: parse::warnings(state, &flow),
        }))
    })
    .await
}

/// `GET /api/flows/diff?from=&to=&threshold=`
pub async fn diff(
    State(state): State<Shared>,
    Query(query): Query<DiffQuery>,
) -> ApiResult<Json<FlowDiffView>> {
    let threshold = threshold(query.threshold)?;
    let tolerance = query.tolerance.unwrap_or(0.05);
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(ApiError::bad_request("tolerance: a share, 0 or more"));
    }
    let (Some(from), Some(to)) = (query.from, query.to) else {
        return Err(ApiError::bad_request(
            "name the two flows to compare: from=KEY&to=KEY",
        ));
    };
    blocking(&state, move |state| {
        let (a_file, a) = find(state, &from)?;
        let (b_file, b) = find(state, &to)?;
        let old = a.flow.clone().map_err(|e| unreadable(&a_file.rel, &e))?;
        let new = b.flow.clone().map_err(|e| unreadable(&b_file.rel, &e))?;
        let d = review::diff(&old, &new, tolerance, threshold);
        Ok(Json(FlowDiffView {
            from,
            to,
            changes: d.changes(),
            review: d.needs_review,
            sections: d.sections,
            markdown: d.markdown,
        }))
    })
    .await
}

/// `GET /api/flows/:key/raw`
pub async fn raw(State(state): State<Shared>, Path(key): Path<String>) -> ApiResult<Response> {
    let file = blocking(&state, move |state| {
        let catalog = data::catalog(state.data_dir());
        catalog
            .flow(&key)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("no flow {key:?}")))
    })
    .await?;
    let bytes = super::sessions::read(&file.path, &file.rel).await?;
    Ok(super::sessions::download(
        bytes,
        "application/json",
        &file.path,
    ))
}

/// `DELETE /api/flows/:key`: the file goes to the trash.
pub async fn delete(
    State(state): State<Shared>,
    Path(key): Path<String>,
) -> ApiResult<Json<OkBody>> {
    blocking(&state, move |state| {
        let catalog = data::catalog(state.data_dir());
        let file = catalog
            .flow(&key)
            .ok_or_else(|| ApiError::not_found(format!("no flow {key:?}")))?;
        super::trash(state, std::slice::from_ref(&file.path))?;
        Ok(Json(OkBody { ok: true }))
    })
    .await
}

/// The flow `key`, read.
fn find(
    state: &crate::State,
    key: &str,
) -> ApiResult<(data::FlowFile, std::sync::Arc<parse::Loaded>)> {
    let catalog = data::catalog(state.data_dir());
    let file = catalog
        .flow(key)
        .cloned()
        .ok_or_else(|| ApiError::not_found(format!("no flow {key:?}")))?;
    let loaded = state
        .cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .flow(&file);
    Ok((file, loaded))
}

fn unreadable(rel: &str, e: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("{rel} does not load: {e}"),
    )
}
