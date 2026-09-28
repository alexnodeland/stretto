//! A flow's staged learning ([`stretto_report::stage`]): the staged flow
//! beside it, how the two flows compared on the sessions as they arrived,
//! and every committed version, with `stretto flow-commit` and
//! `flow-rollback`. A `stage` job ([`super::jobs`]) learns the staged flow.
//!
//! Each endpoint takes the committed flow's key or the staged flow's, and
//! answers for the two together.

use super::flows::{FlowDiffView, FlowSummary};
use super::{blocking, ApiError, ApiResult};
use crate::data::{self, flows as parse, registry, FlowFile};
use crate::Shared;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stretto_report::review;
use stretto_report::stage::{self, Comparison, Count, Kind, Record, Refused};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// A share of lookups the agent made, with its 90% interval (Wilson).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Share {
    pub share: f64,
    pub lower: f64,
    pub upper: f64,
}

/// One flow's lookups at a site, or at every site, over the sessions
/// compared.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Counts {
    /// Decisions the flow made.
    pub decisions: usize,
    /// The lookups it would have made.
    pub lookups: usize,
    /// Of those, the ones the agent made in a later LLM turn.
    pub used: usize,
    /// The ones the proxy had made, serving a flow: neither used nor
    /// detours.
    pub served: usize,
    /// The ones the agent never made.
    pub detours: usize,
    /// The share of the lookups whose use is known that the agent made;
    /// none without such lookups.
    pub used_share: Option<Share>,
}

impl From<&Count> for Counts {
    fn from(c: &Count) -> Self {
        Counts {
            decisions: c.decisions,
            lookups: c.lookups,
            used: c.used,
            served: c.served,
            detours: c.detours(),
            used_share: c.used_share().map(|(share, lower, upper)| Share {
                share,
                lower,
                upper,
            }),
        }
    }
}

/// Both flows' lookups at one site.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct SiteCounts {
    /// The site: the call the flow decides after.
    pub site: String,
    pub committed: Counts,
    pub staged: Counts,
}

/// How the staged flow compared with the committed one on the last
/// sessions: `stretto stage`'s report, as data.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct ComparisonView {
    /// When the staged flow was learned.
    pub learned_unix_ms: u64,
    /// The sessions it learned from.
    pub sessions: usize,
    /// Of those, the ones that run took in.
    pub new: usize,
    /// Whether there was a committed flow to compare with.
    pub committed: bool,
    /// The sessions compared: the last ones both flows were scored on as
    /// they arrived, before either learned from them.
    pub compared: usize,
    pub sites: Vec<SiteCounts>,
    /// Over every site.
    pub total: SiteCounts,
    /// Decisions left out, the committed flow's and the staged's: the
    /// System-One model gave no answer.
    pub unanswered: [usize; 2],
    /// What the staged flow took from the committed one rather than learned.
    pub carried: Vec<String>,
}

impl From<&Comparison> for ComparisonView {
    fn from(c: &Comparison) -> Self {
        let pair = |site: &str, p: &stage::Pair| SiteCounts {
            site: site.to_string(),
            committed: Counts::from(&p.committed),
            staged: Counts::from(&p.staged),
        };
        ComparisonView {
            learned_unix_ms: c.learned_unix_ms,
            sessions: c.sessions,
            new: c.new,
            committed: c.committed,
            compared: c.compared,
            sites: c.sites.iter().map(|(site, p)| pair(site, p)).collect(),
            total: pair("", &c.total),
            unanswered: c.unanswered,
            carried: c.carried.clone(),
        }
    }
}

/// One version of the committed flow, as `stretto flow-log` lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct VersionView {
    /// The version, from 1.
    pub version: u32,
    pub kind: Kind,
    /// When it became the committed flow.
    pub unix_ms: u64,
    /// For a rollback, the version it restored.
    pub restored: Option<u32>,
    /// Why, in the committer's words.
    pub note: Option<String>,
    /// What changed from the version before, as `stretto flow-diff` lists
    /// it.
    pub changes: Vec<String>,
    /// For a commit, the comparison it rested on.
    pub evidence: Option<ComparisonView>,
    /// Whether it is the committed flow now.
    pub current: bool,
}

impl VersionView {
    fn of(r: &Record, current: bool) -> Self {
        VersionView {
            version: r.version,
            kind: r.kind,
            unix_ms: r.unix_ms,
            restored: r.restored,
            note: r.note.clone(),
            changes: r.changes.clone(),
            evidence: r.evidence.as_ref().map(ComparisonView::from),
            current,
        }
    }
}

/// `GET /api/flows/:key/stage`: a deployment's two flows, how they
/// compared, and every committed version.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct StageView {
    /// The committed flow, `NAME.flow.json`, by key: none before the first
    /// commit.
    pub committed: Option<String>,
    /// Where it is, or will be, relative to the data directory.
    pub committed_path: String,
    /// The staged flow, `NAME.staged.flow.json`, when there is one.
    pub staged: Option<FlowSummary>,
    /// Where it is, or would be.
    pub staged_path: String,
    /// The last run of `stretto stage`: its comparison
    /// (`NAME.stage.json`).
    pub last: Option<ComparisonView>,
    /// That run's report, as `stretto stage` writes it.
    pub report_markdown: Option<String>,
    /// Whether a commit would record that comparison as its evidence: it is
    /// the staged flow's.
    pub evidence: bool,
    /// Why the staged flow cannot be committed, when it cannot.
    pub refused: Option<String>,
    /// What committing it would change, as `stretto flow-diff` lists it at
    /// its defaults, and as the commit records it.
    pub diff: Option<FlowDiffView>,
    /// Every committed version, the latest first.
    pub versions: Vec<VersionView>,
    /// Whether the committed flow is not the latest version: it changed
    /// since, by other means than a commit. A commit or a rollback keeps it
    /// as a version first.
    pub unrecorded: bool,
}

/// `POST /api/flows/:key/commit`
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct CommitRequest {
    /// Why, in the committer's words.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub note: Option<String>,
}

/// `POST /api/flows/:key/rollback`
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct RollbackRequest {
    /// The version to restore; by default the one before the committed
    /// flow, as `stretto flow-rollback` chooses it.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub to: Option<u32>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub note: Option<String>,
}

/// The files of the deployment `file` belongs to, as its committed flow or
/// its staged one.
pub fn paths_of(file: &FlowFile) -> ApiResult<stage::Paths> {
    let name = file.path.file_name().unwrap_or_default().to_string_lossy();
    let committed = match name.strip_suffix(".staged.flow.json") {
        Some(stem) if !stem.is_empty() => file.path.with_file_name(format!("{stem}.flow.json")),
        _ => file.path.clone(),
    };
    stage::Paths::of(&committed).map_err(|e| ApiError::bad_request(format!("{e:#}")))
}

/// The flow `key` and its deployment's files.
fn find(state: &crate::State, key: &str) -> ApiResult<(data::Catalog, stage::Paths)> {
    let catalog = data::catalog(state.data_dir());
    let file = catalog
        .flow(key)
        .ok_or_else(|| ApiError::not_found(format!("no flow {key:?}")))?;
    let paths = paths_of(file)?;
    Ok((catalog, paths))
}

/// A file beside the flow that cannot be read: 422.
fn unreadable(e: anyhow::Error) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}"))
}

/// A commit or rollback that did nothing: 409 when refused, else 422.
fn failed(e: anyhow::Error) -> ApiError {
    if e.downcast_ref::<Refused>().is_some() {
        ApiError::conflict(e.to_string())
    } else {
        unreadable(e)
    }
}

/// A note as given, trimmed; none when empty.
fn note(note: Option<String>) -> Option<String> {
    note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// The request's body, or its defaults when there is none.
fn body<T: DeserializeOwned + Default>(bytes: &Bytes) -> ApiResult<T> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(bytes)
        .map_err(|e| ApiError::bad_request(format!("the request is not as expected: {e}")))
}

/// `GET /api/flows/:key/stage`
pub async fn view(
    State(state): State<Shared>,
    Path(key): Path<String>,
) -> ApiResult<Json<StageView>> {
    blocking(&state, move |state| {
        let (catalog, paths) = find(state, &key)?;
        let root = state.data_dir();
        let committed = catalog.flow_at(&paths.committed).cloned();
        let staged_file = catalog.flow_at(&paths.staged).cloned();
        let registry = registry::load(root).unwrap_or_default();
        let staged = staged_file.as_ref().map(|f| {
            let mut s = parse::summary(f, &state.flow(f), parse::served_by(f, &registry, state));
            s.stage = parse::stage_link(f, &catalog.flows);
            s
        });
        let flows = [&committed, &staged_file].map(|f| f.as_ref().map(|f| state.flow(f)));
        let saved = stage::State::load(&paths.state).map_err(unreadable)?;
        let last = saved.and_then(|s| s.last);
        let (refused, learned) = match stage::candidate(&paths) {
            Ok(c) => (None, Some(c.learned_unix_ms)),
            Err(e) => (Some(format!("{e:#}")), None),
        };
        let diff = match (&committed, &staged_file, &flows) {
            (Some(from), Some(to), [Some(a), Some(b)]) => match (&a.flow, &b.flow) {
                (Ok(old), Ok(new)) => {
                    let d = review::diff(old, new, 0.05, 0.3);
                    Some(FlowDiffView {
                        from: from.key.clone(),
                        to: to.key.clone(),
                        changes: d.changes(),
                        review: d.needs_review,
                        sections: d.sections,
                        markdown: d.markdown,
                    })
                }
                _ => None,
            },
            _ => None,
        };
        let history = stage::history(&paths).map_err(unreadable)?;
        let unrecorded = stage::unrecorded(&paths, &history).map_err(unreadable)?;
        let latest = history.last().map(|r| r.version);
        let versions = history
            .iter()
            .rev()
            .map(|r| VersionView::of(r, Some(r.version) == latest && !unrecorded))
            .collect();
        let shown = |p: &std::path::Path| super::jobs::shown(state, p);
        Ok(Json(StageView {
            committed: committed.map(|f| f.key),
            committed_path: shown(&paths.committed),
            staged,
            staged_path: shown(&paths.staged),
            evidence: last
                .as_ref()
                .is_some_and(|c| Some(c.learned_unix_ms) == learned),
            report_markdown: last
                .as_ref()
                .map(|c| c.markdown(&paths.committed.display().to_string())),
            last: last.as_ref().map(ComparisonView::from),
            refused,
            diff,
            versions,
            unrecorded,
        }))
    })
    .await
}

/// `POST /api/flows/:key/commit`: the staged flow becomes the committed
/// one, as `stretto flow-commit` makes it, with the last comparison as its
/// evidence. 409 when there is nothing to commit.
pub async fn commit(
    State(state): State<Shared>,
    Path(key): Path<String>,
    bytes: Bytes,
) -> ApiResult<Json<VersionView>> {
    let request: CommitRequest = body(&bytes)?;
    blocking(&state, move |state| {
        let (_, paths) = find(state, &key)?;
        let evidence = stage::State::load(&paths.state)
            .map_err(unreadable)?
            .and_then(|s| s.last);
        let record =
            stage::commit(&paths, note(request.note), evidence, state.now()).map_err(failed)?;
        Ok(Json(VersionView::of(&record, true)))
    })
    .await
}

/// `POST /api/flows/:key/rollback`: an earlier version becomes the
/// committed flow again, as a version of its own, as `stretto
/// flow-rollback` makes it. 409 when there is no such version, or it is the
/// committed flow already.
pub async fn rollback(
    State(state): State<Shared>,
    Path(key): Path<String>,
    bytes: Bytes,
) -> ApiResult<Json<VersionView>> {
    let request: RollbackRequest = body(&bytes)?;
    blocking(&state, move |state| {
        let (_, paths) = find(state, &key)?;
        let record =
            stage::rollback(&paths, request.to, note(request.note), state.now()).map_err(failed)?;
        Ok(Json(VersionView::of(&record, true)))
    })
    .await
}
