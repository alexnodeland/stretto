//! Jobs: the `stretto` CLI's `learn`, `promote`, `audit`, `redact` and
//! `doctor`, run one at a time as subprocesses ([`crate::jobs`]).

use super::{blocking, ApiError, ApiResult};
use crate::data::registry::DeciderName;
use crate::data::{self, paths};
use crate::{Shared, State};
use axum::body::{Body, Bytes};
use axum::extract::{Path, State as AxumState};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use stretto_report::init::check_domain;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Learn,
    Promote,
    Audit,
    Redact,
    Doctor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A flow the job wrote.
    Flow,
    /// A report: Markdown or JSON.
    Report,
    /// A directory of files, such as redacted sessions.
    Dir,
}

/// Something a job writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Artifact {
    pub kind: ArtifactKind,
    /// Relative to the data directory, or `~/…` outside it.
    pub path: String,
    /// For a flow in the data directory, its key.
    pub key: Option<String>,
}

/// A run of the `stretto` CLI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub title: String,
    /// The request, with its defaults filled in.
    #[ts(type = "unknown")]
    pub params: Value,
    pub status: JobStatus,
    pub created_unix_ms: u64,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
    pub exit_code: Option<i32>,
    /// What it printed, stdout and stderr as they came: the last 64 KiB in
    /// lists and events, all of it from `GET /api/jobs/:id`.
    pub output: String,
    /// What it writes; after it ends, only what exists.
    pub artifacts: Vec<Artifact>,
}

/// `GET /api/jobs`
#[derive(Clone, Debug, Serialize, TS)]
pub struct JobList {
    pub items: Vec<Job>,
}

/// `POST /api/jobs`: what to run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobRequest {
    /// `stretto learn`: a flow from recorded sessions.
    Learn {
        domain: String,
        /// The sessions' directory: relative to the data directory, or `~/…`.
        sessions: String,
        /// Where to write the flow; by default `<domain>.flow.json` in the
        /// data directory.
        #[serde(default)]
        #[ts(optional = nullable)]
        out: Option<String>,
        /// Replace `out` if it exists; else a job that would is refused.
        #[serde(default)]
        #[ts(optional = nullable)]
        overwrite: Option<bool>,
        /// Ask no System-One model (the default): without it, an arbiter is
        /// fitted, which needs a key.
        #[serde(default)]
        #[ts(optional = nullable)]
        habit_only: Option<bool>,
        #[serde(default)]
        #[ts(optional = nullable)]
        constants: Option<bool>,
    },
    /// `stretto promote`: keep a flow to the sites where its lookups were the
    /// agent's own.
    Promote {
        /// The flow's key.
        flow: String,
        sessions: String,
        /// The answers the proxy cached in shadow; by default `oracle-cache`
        /// in the data directory.
        #[serde(default)]
        #[ts(optional = nullable)]
        oracle_cache: Option<String>,
        #[serde(default)]
        #[ts(optional = nullable)]
        threshold: Option<f64>,
        #[serde(default)]
        #[ts(optional = nullable)]
        min_used: Option<f64>,
        #[serde(default)]
        #[ts(optional = nullable)]
        min_lower: Option<f64>,
        #[serde(default)]
        #[ts(optional = nullable)]
        min_tasks: Option<usize>,
        /// By default `<flow name>.promoted.flow.json` beside the flow.
        #[serde(default)]
        #[ts(optional = nullable)]
        out: Option<String>,
        #[serde(default)]
        #[ts(optional = nullable)]
        overwrite: Option<bool>,
    },
    /// `stretto audit`, with its report as JSON.
    Audit {
        flow: String,
        sessions: String,
        #[serde(default)]
        #[ts(optional = nullable)]
        decider: Option<DeciderName>,
    },
    /// `stretto redact`: needs `STRETTO_REDACT_SALT` in the console's
    /// environment.
    Redact {
        sessions: String,
        out: String,
        #[serde(default)]
        #[ts(optional = nullable)]
        keep_shared: Option<usize>,
        #[serde(default)]
        #[ts(optional = nullable)]
        hash_fields: Option<Vec<String>>,
    },
    /// `stretto doctor`.
    Doctor,
}

/// What a request runs: the CLI's arguments, what it writes, and how the
/// list names it.
#[derive(Clone, Debug)]
pub struct Plan {
    pub kind: JobKind,
    pub title: String,
    pub params: Value,
    pub args: Vec<String>,
    pub artifacts: Vec<Artifact>,
}

/// `GET /api/jobs`
pub async fn list(AxumState(state): AxumState<Shared>) -> Json<JobList> {
    Json(JobList {
        items: state.jobs.list(),
    })
}

/// `GET /api/jobs/:id`
pub async fn detail(
    AxumState(state): AxumState<Shared>,
    Path(id): Path<String>,
) -> ApiResult<Json<Job>> {
    state
        .jobs
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("no job {id:?}")))
}

/// `POST /api/jobs`: 202 with the job, queued.
pub async fn create(
    AxumState(state): AxumState<Shared>,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<Job>)> {
    let request: JobRequest = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("the job is not as expected: {e}")))?;
    let id = crate::jobs::new_id(state.now());
    let plan = {
        let id = id.clone();
        blocking(&state, move |state| plan(state, &id, request)).await?
    };
    Ok((
        StatusCode::ACCEPTED,
        Json(state.jobs.submit(id, plan, state.now())),
    ))
}

/// `GET /api/jobs/:id/artifacts/:index`: a report or flow the job wrote,
/// to show.
pub async fn artifact(
    AxumState(state): AxumState<Shared>,
    Path((id, index)): Path<(String, usize)>,
) -> ApiResult<Response> {
    let job = state
        .jobs
        .get(&id)
        .ok_or_else(|| ApiError::not_found(format!("no job {id:?}")))?;
    let artifact = job
        .artifacts
        .get(index)
        .ok_or_else(|| ApiError::not_found(format!("job {id} has no artifact {index}")))?;
    if artifact.kind == ArtifactKind::Dir {
        return Err(ApiError::bad_request(format!(
            "{} is a directory",
            artifact.path
        )));
    }
    let path = paths::resolve(
        state.data_dir(),
        state.config.home.as_deref(),
        &artifact.path,
    )
    .map_err(ApiError::forbidden)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::not_found(format!("{}: {e}", artifact.path)))?;
    let kind = if artifact.path.ends_with(".json") {
        "application/json"
    } else if artifact.path.ends_with(".md") {
        "text/markdown; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    };
    let mut response = Body::from(bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    Ok(response)
}

/// A request's plan, or why it cannot run.
pub fn plan(state: &State, id: &str, request: JobRequest) -> ApiResult<Plan> {
    let root = state.data_dir();
    let home = state.config.home.as_deref();
    let resolve = |what: &str, p: &str| {
        paths::resolve(root, home, p).map_err(|e| ApiError::bad_request(format!("{what}: {e}")))
    };
    let dir = |what: &str, p: &str| -> ApiResult<PathBuf> {
        let path = resolve(what, p)?;
        if !path.is_dir() {
            return Err(ApiError::bad_request(format!(
                "{what}: {p} is not a directory"
            )));
        }
        Ok(path)
    };
    // A file to write: not over one that exists unless asked, and in a
    // directory that exists, as the CLI does not create it.
    let fresh = |path: &PathBuf, overwrite: bool| -> ApiResult<()> {
        if path.exists() && !overwrite {
            return Err(ApiError::conflict(format!(
                "{} exists: choose another path, or overwrite it",
                shown(state, path)
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::internal(format!("creating {}: {e}", shown(state, parent)))
            })?;
        }
        Ok(())
    };
    let s = |p: &PathBuf| p.display().to_string();
    let reports = root.join(data::CONSOLE_DIR).join("jobs");
    let report = |ext: &str| reports.join(format!("{id}.{ext}"));
    let flow_of = |key: &str| -> ApiResult<data::FlowFile> {
        data::catalog(root)
            .flow(key)
            .cloned()
            .ok_or_else(|| ApiError::bad_request(format!("no flow {key:?}")))
    };
    let probability = |what: &str, x: f64| -> ApiResult<f64> {
        if x.is_finite() && (0.0..=1.0).contains(&x) {
            Ok(x)
        } else {
            Err(ApiError::bad_request(format!("{what} {x}: from 0 to 1")))
        }
    };
    let artifact = |kind: ArtifactKind, path: &PathBuf| Artifact {
        kind,
        path: shown(state, path),
        key: None,
    };
    let params = |r: &JobRequest| serde_json::to_value(r).unwrap_or(Value::Null);
    Ok(match request {
        JobRequest::Learn {
            domain,
            sessions,
            out,
            overwrite,
            habit_only,
            constants,
        } => {
            let (overwrite, habit_only, constants) = (
                overwrite.unwrap_or(false),
                habit_only.unwrap_or(true),
                constants.unwrap_or(false),
            );
            check_domain(&domain).map_err(|e| ApiError::bad_request(e.to_string()))?;
            if !habit_only && !state.key_set() {
                return Err(ApiError::bad_request(
                    "fitting an arbiter asks TypeSafe's Jev, and neither TYPESAFE_API_KEY nor \
                     TYPESAFE_API_KEY_FILE is set in the console's environment: learn with \
                     habit_only",
                ));
            }
            let from = dir("sessions", &sessions)?;
            let to = match &out {
                Some(o) => resolve("out", o)?,
                None => root.join(format!("{domain}.flow.json")),
            };
            fresh(&to, overwrite)?;
            let mut args = vec![
                "learn".into(),
                "--sessions".into(),
                s(&from),
                "--domain".into(),
                domain.clone(),
                "--out".into(),
                s(&to),
            ];
            if habit_only {
                args.push("--habit-only".into());
            } else {
                args.extend(["--oracle-cache".into(), s(&root.join("oracle-cache"))]);
            }
            if constants {
                args.push("--constants".into());
            }
            Plan {
                kind: JobKind::Learn,
                title: format!("Learn {domain} from {}", shown(state, &from)),
                params: params(&JobRequest::Learn {
                    domain,
                    sessions: shown(state, &from),
                    out: Some(shown(state, &to)),
                    overwrite: Some(overwrite),
                    habit_only: Some(habit_only),
                    constants: Some(constants),
                }),
                args,
                artifacts: vec![artifact(ArtifactKind::Flow, &to)],
            }
        }
        JobRequest::Promote {
            flow,
            sessions,
            oracle_cache,
            threshold,
            min_used,
            min_lower,
            min_tasks,
            out,
            overwrite,
        } => {
            let overwrite = overwrite.unwrap_or(false);
            let file = flow_of(&flow)?;
            let from = dir("sessions", &sessions)?;
            let cache = match &oracle_cache {
                Some(c) => resolve("oracle_cache", c)?,
                None => root.join("oracle-cache"),
            };
            let threshold = probability("threshold", threshold.unwrap_or(0.3))?;
            let min_used = probability("min_used", min_used.unwrap_or(0.7))?;
            let min_lower = probability("min_lower", min_lower.unwrap_or(0.5))?;
            let min_tasks = min_tasks.unwrap_or(3);
            if min_tasks == 0 {
                return Err(ApiError::bad_request("min_tasks: at least 1"));
            }
            let to = match &out {
                Some(o) => resolve("out", o)?,
                None => file
                    .path
                    .with_file_name(format!("{}.promoted.flow.json", file.name)),
            };
            fresh(&to, overwrite)?;
            let md = report("promote.md");
            Plan {
                kind: JobKind::Promote,
                title: format!("Promote {} on {}", file.name, shown(state, &from)),
                params: params(&JobRequest::Promote {
                    flow,
                    sessions: shown(state, &from),
                    oracle_cache: Some(shown(state, &cache)),
                    threshold: Some(threshold),
                    min_used: Some(min_used),
                    min_lower: Some(min_lower),
                    min_tasks: Some(min_tasks),
                    out: Some(shown(state, &to)),
                    overwrite: Some(overwrite),
                }),
                args: vec![
                    "promote".into(),
                    "--flow".into(),
                    s(&file.path),
                    "--sessions".into(),
                    s(&from),
                    "--oracle-cache".into(),
                    s(&cache),
                    "--threshold".into(),
                    threshold.to_string(),
                    "--min-used".into(),
                    min_used.to_string(),
                    "--min-lower".into(),
                    min_lower.to_string(),
                    "--min-tasks".into(),
                    min_tasks.to_string(),
                    "--out".into(),
                    s(&to),
                    "--report".into(),
                    s(&md),
                ],
                artifacts: vec![
                    artifact(ArtifactKind::Flow, &to),
                    artifact(ArtifactKind::Report, &md),
                ],
            }
        }
        JobRequest::Audit {
            flow,
            sessions,
            decider,
        } => {
            let file = flow_of(&flow)?;
            let from = dir("sessions", &sessions)?;
            let (json, md) = (report("audit.json"), report("audit.md"));
            let mut args = vec![
                "audit".into(),
                "--flow".into(),
                s(&file.path),
                "--sessions".into(),
                s(&from),
                "--oracle".into(),
                "replay".into(),
                "--oracle-cache".into(),
                s(&root.join("oracle-cache")),
                "--out".into(),
                s(&md),
                "--json".into(),
                s(&json),
            ];
            if let Some(d) = decider {
                args.extend(["--decider".into(), d.decider().name().into()]);
            }
            Plan {
                kind: JobKind::Audit,
                title: format!("Audit {} on {}", file.name, shown(state, &from)),
                params: params(&JobRequest::Audit {
                    flow,
                    sessions: shown(state, &from),
                    decider,
                }),
                args,
                artifacts: vec![
                    artifact(ArtifactKind::Report, &json),
                    artifact(ArtifactKind::Report, &md),
                ],
            }
        }
        JobRequest::Redact {
            sessions,
            out,
            keep_shared,
            hash_fields,
        } => {
            if !crate::env_set("STRETTO_REDACT_SALT") {
                return Err(ApiError::bad_request(
                    "redacting needs a salt: set STRETTO_REDACT_SALT in the console's \
                     environment (docs/privacy.md)",
                ));
            }
            let from = dir("sessions", &sessions)?;
            let to = resolve("out", &out)?;
            if to == from || to.starts_with(&from) || from.starts_with(&to) {
                return Err(ApiError::bad_request(
                    "out: the redacted copy goes in a directory of its own, apart from the \
                     sessions",
                ));
            }
            let keep = keep_shared.unwrap_or(3);
            if keep == 0 {
                return Err(ApiError::bad_request("keep_shared: at least 1"));
            }
            let fields: Vec<String> = hash_fields
                .unwrap_or_default()
                .iter()
                .flat_map(|f| f.split(','))
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect();
            if fields.iter().any(|f| f.chars().any(char::is_whitespace)) {
                return Err(ApiError::bad_request(
                    "hash_fields: field names, without spaces",
                ));
            }
            let mut args = vec![
                "redact".into(),
                "--sessions".into(),
                s(&from),
                "--out".into(),
                s(&to),
                "--keep-shared".into(),
                keep.to_string(),
            ];
            for f in &fields {
                args.extend(["--hash-field".into(), f.clone()]);
            }
            Plan {
                kind: JobKind::Redact,
                title: format!("Redact {}", shown(state, &from)),
                params: params(&JobRequest::Redact {
                    sessions: shown(state, &from),
                    out: shown(state, &to),
                    keep_shared: Some(keep),
                    hash_fields: Some(fields),
                }),
                args,
                artifacts: vec![artifact(ArtifactKind::Dir, &to)],
            }
        }
        JobRequest::Doctor => Plan {
            kind: JobKind::Doctor,
            title: "Check the installation".into(),
            params: params(&JobRequest::Doctor),
            args: vec!["doctor".into()],
            artifacts: Vec::new(),
        },
    })
}

/// `path` as the API shows it: relative to the data directory, else `~/…`,
/// else as it is.
pub fn shown(state: &State, path: &std::path::Path) -> String {
    let root = state.data_dir();
    if path.starts_with(root) {
        paths::rel(root, path)
    } else {
        paths::display(path, state.config.home.as_deref())
    }
}
