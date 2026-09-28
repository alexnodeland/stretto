//! Health and what the console is: its version, data directory, mode, and
//! the binaries it found.

use crate::{Shared, VERSION};
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::path::{Path, PathBuf};
#[cfg(feature = "ts")]
use ts_rs::TS;

/// `GET /api/health`, which needs no token.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Health {
    pub ok: bool,
    pub version: String,
}

/// A binary of stretto's the console found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Binary {
    pub path: String,
    /// What `--version` printed, such as `stretto 0.1.0`; `null` if it did
    /// not answer.
    pub version: Option<String>,
}

/// `GET /api/meta`
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Meta {
    pub version: String,
    pub data_dir: String,
    pub read_only: bool,
    /// Whether `/api` needs the token (false with `--no-auth`).
    pub auth: bool,
    /// Whether `TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` is set; never
    /// its value.
    pub key_set: bool,
    /// The `stretto` CLI the jobs run.
    pub stretto: Option<Binary>,
    pub proxy: Option<Binary>,
}

pub async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        version: VERSION.to_string(),
    })
}

pub async fn meta(State(state): State<Shared>) -> Json<Meta> {
    let c = &state.config;
    Json(Meta {
        version: VERSION.to_string(),
        data_dir: c.data_dir.display().to_string(),
        read_only: c.read_only,
        auth: c.token.is_some(),
        key_set: state.key_set(),
        stretto: c.binaries.stretto.clone(),
        proxy: c.binaries.proxy.clone(),
    })
}

/// `name` beside the running console, else on `path` (PATH); `explicit`
/// wins when given (`--stretto`). Its version is what `--version` prints.
pub fn find(
    name: &str,
    explicit: Option<&Path>,
    beside: Option<&Path>,
    path: &std::ffi::OsStr,
) -> Option<Binary> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let path: PathBuf = match explicit {
        Some(p) => p.to_path_buf(),
        None => beside
            .map(|dir| dir.join(&file))
            .filter(|p| p.is_file())
            .or_else(|| stretto_report::doctor::which(name, path))?,
    };
    Some(Binary {
        version: stretto_report::doctor::version_of(&path),
        path: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binary_is_found_as_given_beside_the_console_or_on_path() {
        let dir = std::env::temp_dir().join(format!("stretto-console-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join(format!("tool{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&tool, "").unwrap();
        let found = |explicit: Option<&Path>, beside: Option<&Path>, path: &str| {
            find("tool", explicit, beside, path.as_ref()).map(|b| b.path)
        };
        let at = Some(tool.display().to_string());
        let elsewhere = dir.join("elsewhere");
        assert_eq!(found(Some(&tool), Some(&elsewhere), ""), at);
        assert_eq!(found(None, Some(&dir), ""), at);
        assert_eq!(found(None, Some(&elsewhere), dir.to_str().unwrap()), at);
        assert_eq!(found(None, None, ""), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
