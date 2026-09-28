//! Settings: the data directory and what it holds, the binaries, the key,
//! and how the console runs.

use super::{blocking, ApiResult};
use crate::data::{self, paths};
use crate::{Shared, State, VERSION};
use axum::extract::State as AxumState;
use axum::Json;
use serde::Serialize;
use std::path::Path;
#[cfg(feature = "ts")]
use ts_rs::TS;

/// The data directory's size, by what its files are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Disk {
    /// Session logs and the flow and confirmation logs beside them.
    pub logs_bytes: u64,
    /// `*.flow.json`
    pub flows_bytes: u64,
    /// The System-One model's answers, `oracle-cache/`.
    pub cache_bytes: u64,
    /// Everything else, the console's jobs and trash included.
    pub other_bytes: u64,
    pub total_bytes: u64,
}

/// A binary of stretto's, found or not.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct BinaryStatus {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// `GET /api/settings`
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Settings {
    pub data_dir: String,
    pub disk: Disk,
    pub sessions: usize,
    pub flows: usize,
    /// Whether `TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` is set; never
    /// its value.
    pub key_set: bool,
    /// What is kept, and for how long.
    pub retention_note: String,
    pub binaries: Vec<BinaryStatus>,
    pub version: String,
    pub read_only: bool,
    pub auth: bool,
}

/// What the console says about keeping data.
pub const RETENTION_NOTE: &str = "stretto-proxy --retain-days N deletes, when it starts, the \
    session logs (with the flow and confirmation logs beside them) and the cached answers older \
    than N days in its --record and --oracle-cache directories. The console deletes nothing: a \
    session or flow deleted here is moved to console/trash/ in the data directory, to empty \
    when you choose. docs/privacy.md lists what each file holds.";

pub async fn settings(AxumState(state): AxumState<Shared>) -> ApiResult<Json<Settings>> {
    blocking(&state, |state| Ok(Json(build(state)))).await
}

fn build(state: &State) -> Settings {
    let catalog = data::catalog(state.data_dir());
    let b = &state.config.binaries;
    let binaries = [
        ("stretto", &b.stretto),
        ("stretto-proxy", &b.proxy),
        ("stretto-procedure", &b.procedure),
        ("stretto-mcp-demo", &b.demo),
    ]
    .into_iter()
    .map(|(name, found)| BinaryStatus {
        name: name.to_string(),
        path: found.as_ref().map(|f| f.path.clone()),
        version: found.as_ref().and_then(|f| f.version.clone()),
    })
    .collect();
    Settings {
        data_dir: state.data_dir().display().to_string(),
        disk: disk(state.data_dir()),
        sessions: catalog.sessions.len(),
        flows: catalog.flows.len(),
        key_set: state.key_set(),
        retention_note: RETENTION_NOTE.to_string(),
        binaries,
        version: VERSION.to_string(),
        read_only: state.config.read_only,
        auth: state.config.token.is_some(),
    }
}

/// The size of everything under `root`, by kind. Symbolic links are not
/// followed.
pub fn disk(root: &Path) -> Disk {
    fn walk(root: &Path, dir: &Path, disk: &mut Disk) {
        // What cannot be read (gone since it was listed) is not counted.
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            // As `symlink_metadata`: a link is not followed.
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(root, &path, disk);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let rel = paths::rel(root, &path);
            let size = meta.len();
            if rel.starts_with("oracle-cache/") {
                disk.cache_bytes += size;
            } else if rel.starts_with(&format!("{}/", data::CONSOLE_DIR)) {
                disk.other_bytes += size;
            } else if rel.ends_with(".flow.json") {
                disk.flows_bytes += size;
            } else if rel.ends_with(".jsonl") {
                disk.logs_bytes += size;
            } else {
                disk.other_bytes += size;
            }
            disk.total_bytes += size;
        }
    }
    let mut out = Disk::default();
    walk(root, root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixtures_disk() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/home");
        let d = disk(&root);
        assert!(d.logs_bytes > 50_000 && d.flows_bytes > 10_000, "{d:?}");
        assert_eq!(d.cache_bytes, 0);
        assert_eq!(
            d.total_bytes,
            d.logs_bytes + d.flows_bytes + d.cache_bytes + d.other_bytes
        );
        // servers.json
        assert!(d.other_bytes > 0);
    }

    #[test]
    fn each_file_is_counted_once_by_what_it_is() {
        let root =
            std::env::temp_dir().join(format!("stretto-console-disk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["logs/shop", "oracle-cache", "console/jobs"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let write = |rel: &str, n: usize| std::fs::write(root.join(rel), "x".repeat(n)).unwrap();
        write("logs/shop/a.jsonl", 1);
        write("shop.flow.json", 10);
        write("oracle-cache/q.json", 100);
        write("console/jobs/j.json", 1000);
        write("servers.json", 10_000);
        // A link is not followed, so what it names is counted once.
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("shop.flow.json"), root.join("alias.flow.json"))
            .unwrap();
        let d = disk(&root);
        assert_eq!(
            (d.logs_bytes, d.flows_bytes, d.cache_bytes, d.other_bytes),
            (1, 10, 100, 11_000)
        );
        assert_eq!(d.total_bytes, 11_111);
        assert_eq!(disk(&root.join("missing")), Disk::default());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
