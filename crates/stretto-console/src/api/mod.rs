//! The HTTP API, under `/api`, and the router that serves it with the UI.
//!
//! Every handler answers JSON (`snake_case` fields; times in milliseconds
//! since the Unix epoch, `*_unix_ms`; sizes in bytes; probabilities from 0
//! to 1), and every error is `{"error": "message"}` with its status. Each
//! type a handler answers with derives [`ts_rs::TS`], and the tests export
//! them to `console/src/api/generated/`, which the UI imports.

pub mod events;
pub mod flows;
pub mod jobs;
pub mod meta;
pub mod overview;
pub mod servers;
pub mod sessions;
pub mod settings;

use crate::{assets, auth, Shared, State};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{middleware, Json, Router};
use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// The whole HTTP surface: the API, then the UI for every other path,
/// behind the token, the CSRF header, the Host check and the security
/// headers ([`auth`]).
pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/api/health", get(meta::health))
        .route("/api/meta", get(meta::meta))
        .route("/api/overview", get(overview::overview))
        .route("/api/settings", get(settings::settings))
        .route("/api/sessions", get(sessions::list))
        .route(
            "/api/sessions/{key}",
            get(sessions::detail).delete(sessions::delete),
        )
        .route("/api/sessions/{key}/raw", get(sessions::raw))
        .route("/api/flows", get(flows::list))
        .route("/api/flows/diff", get(flows::diff))
        .route("/api/flows/{key}", get(flows::detail).delete(flows::delete))
        .route("/api/flows/{key}/raw", get(flows::raw))
        .route("/api/servers", get(servers::list).post(servers::create))
        .route(
            "/api/servers/{name}",
            put(servers::update).delete(servers::delete),
        )
        .route("/api/servers/{name}/config", get(servers::config))
        .route("/api/servers/{name}/probe", post(servers::probe))
        .route("/api/jobs", get(jobs::list).post(jobs::create))
        .route("/api/jobs/{id}", get(jobs::detail))
        .route("/api/jobs/{id}/artifacts/{index}", get(jobs::artifact))
        .route("/api/events", get(events::events))
        .route("/api/logout", post(auth::logout))
        .fallback(fallback)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(state.clone(), auth::guard))
        .layer(middleware::from_fn(auth::security_headers))
        .with_state(state)
}

/// An unknown path: JSON under `/api`, else the UI.
async fn fallback(method: Method, uri: Uri) -> Response {
    if uri.path() == "/api" || uri.path().starts_with("/api/") {
        return ApiError::not_found(format!("no such endpoint: {method} {}", uri.path()))
            .into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return ApiError::new(StatusCode::METHOD_NOT_ALLOWED, "the UI is read with GET")
            .into_response();
    }
    assets::serve(uri.path()).await
}

async fn method_not_allowed(method: Method, uri: Uri) -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        format!("{method} is not allowed on {}", uri.path()),
    )
}

/// The body of every error: `{"error": "message"}`.
#[derive(Clone, Debug, Serialize, TS)]
pub struct ErrorBody {
    pub error: String,
}

/// `{"ok": true}`.
#[derive(Clone, Debug, Serialize, TS)]
pub struct Ok {
    pub ok: bool,
}

/// An error, as the API answers it.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        ApiError {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::internal(format!("{e:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

/// Run `f` on a thread that may block (reading files, parsing logs).
pub async fn blocking<T, F>(state: &Shared, f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&State) -> ApiResult<T> + Send + 'static,
{
    let state = state.clone();
    tokio::task::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| ApiError::internal(format!("the handler failed: {e}")))?
}

/// Move `paths` (in the data directory) to `console/trash/<now>/`, each at
/// its path relative to the data directory, so nothing is unlinked and
/// nothing collides.
pub fn trash(state: &State, paths: &[PathBuf]) -> ApiResult<()> {
    let root = state.data_dir();
    let bin = root
        .join(crate::data::CONSOLE_DIR)
        .join("trash")
        .join(state.now().to_string());
    for path in paths {
        let rel = path
            .strip_prefix(root)
            .map_err(|_| ApiError::forbidden("only files in the data directory go to the trash"))?;
        let to = bin.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::internal(format!("creating the trash: {e}")))?;
        }
        std::fs::rename(path, &to).map_err(|e| {
            ApiError::internal(format!(
                "moving {} to the trash: {e}",
                crate::data::paths::rel(root, path)
            ))
        })?;
    }
    Ok(())
}

/// The console's own directory, created if needed.
pub fn console_dir(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join(crate::data::CONSOLE_DIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod typescript;
