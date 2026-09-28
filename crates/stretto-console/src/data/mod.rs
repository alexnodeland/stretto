//! What the console reads from the data directory.
//!
//! [`catalog`] walks it as `stretto doctor` does
//! ([`stretto_report::doctor::scan_with`]): three levels down, past the
//! answer cache, and here also past the console's own `console/` directory
//! and any symbolic link that leads out of the data directory. Every session
//! log and flow it finds gets a [`key`](keys): its file's stem, made unique
//! where two files share one. [`sessions`] and [`flows`] parse them,
//! [`registry`] keeps `servers.json`, and [`paths`] resolves the paths a
//! request names without letting them leave the data directory.

pub mod flows;
pub mod keys;
pub mod paths;
pub mod registry;
pub mod sessions;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// The console's own directory in the data directory, which the scan leaves
/// out: jobs and the trash.
pub const CONSOLE_DIR: &str = "console";

/// A session log the scan found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFile {
    pub key: String,
    /// The log.
    pub path: PathBuf,
    /// The log, relative to the data directory, with `/` between parts.
    pub rel: String,
    /// `<session>.flow.jsonl` beside it, if there is one.
    pub flow_log: Option<PathBuf>,
    /// `<session>.confirm.jsonl` beside it, if there is one.
    pub confirm_log: Option<PathBuf>,
}

/// A flow file the scan found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowFile {
    pub key: String,
    pub path: PathBuf,
    pub rel: String,
    /// Its name: the file's name without `.flow.json`.
    pub name: String,
}

/// Everything the scan found.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub sessions: Vec<SessionFile>,
    pub flows: Vec<FlowFile>,
}

impl Catalog {
    pub fn session(&self, key: &str) -> Option<&SessionFile> {
        self.sessions.iter().find(|s| s.key == key)
    }

    pub fn flow(&self, key: &str) -> Option<&FlowFile> {
        self.flows.iter().find(|f| f.key == key)
    }

    /// The flow at `path`, compared as the file it is when both exist.
    pub fn flow_at(&self, path: &Path) -> Option<&FlowFile> {
        let wanted = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.flows
            .iter()
            .find(|f| f.path == path || std::fs::canonicalize(&f.path).is_ok_and(|p| p == wanted))
    }
}

/// Keys the flow routes take for themselves (`/api/flows/diff`), which a
/// flow's key never is.
pub const RESERVED_FLOW_KEYS: &[&str] = &["diff"];

/// Walk `root` and key what it holds.
pub fn catalog(root: &Path) -> Catalog {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let found =
        stretto_report::doctor::scan_with(root, &|path| paths::keep(root, &canonical, path));
    let sessions: Vec<(String, PathBuf)> = found
        .logs
        .into_iter()
        .map(|p| (stem(&p, ".jsonl"), p))
        .collect();
    let session_keys = keys::assign(
        &sessions
            .iter()
            .map(|(stem, p)| (stem.clone(), paths::rel(root, p)))
            .collect::<Vec<_>>(),
        &[],
    );
    let flows: Vec<(String, PathBuf)> = found
        .flows
        .into_iter()
        .map(|p| (stem(&p, ".flow.json"), p))
        .collect();
    let flow_keys = keys::assign(
        &flows
            .iter()
            .map(|(stem, p)| (stem.clone(), paths::rel(root, p)))
            .collect::<Vec<_>>(),
        RESERVED_FLOW_KEYS,
    );
    Catalog {
        sessions: sessions
            .into_iter()
            .zip(session_keys)
            .map(|((stem, path), key)| {
                let beside = |suffix: &str| {
                    let p = path.with_file_name(format!("{stem}{suffix}"));
                    (p.is_file() && paths::keep(root, &canonical, &p)).then_some(p)
                };
                SessionFile {
                    key,
                    rel: paths::rel(root, &path),
                    flow_log: beside(".flow.jsonl"),
                    confirm_log: beside(".confirm.jsonl"),
                    path,
                }
            })
            .collect(),
        flows: flows
            .into_iter()
            .zip(flow_keys)
            .map(|((name, path), key)| FlowFile {
                key,
                rel: paths::rel(root, &path),
                name,
                path,
            })
            .collect(),
    }
}

/// A file's name without `suffix`.
fn stem(path: &Path, suffix: &str) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.strip_suffix(suffix).unwrap_or(&name).to_string()
}

/// A file's length and modification time, to tell when it changed.
pub type Stamp = Option<(u64, SystemTime)>;

/// The stamp of `path`, if it exists.
pub fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
}

/// Parsed sessions and flows, each kept while the files it came from are
/// unchanged.
#[derive(Default)]
pub struct Cache {
    sessions: HashMap<PathBuf, (Vec<Stamp>, Arc<sessions::Loaded>)>,
    flows: HashMap<PathBuf, (Stamp, Arc<flows::Loaded>)>,
}

impl Cache {
    /// The session in `file`, parsed now or kept from before.
    pub fn session(&mut self, file: &SessionFile) -> Arc<sessions::Loaded> {
        let stamps = vec![
            stamp(&file.path),
            file.flow_log.as_deref().and_then(stamp),
            file.confirm_log.as_deref().and_then(stamp),
        ];
        if let Some((kept, loaded)) = self.sessions.get(&file.path) {
            if *kept == stamps {
                return loaded.clone();
            }
        }
        let loaded = Arc::new(sessions::load(file));
        self.sessions
            .insert(file.path.clone(), (stamps, loaded.clone()));
        loaded
    }

    /// The flow in `file`, parsed now or kept from before.
    pub fn flow(&mut self, file: &FlowFile) -> Arc<flows::Loaded> {
        let now = stamp(&file.path);
        if let Some((kept, loaded)) = self.flows.get(&file.path) {
            if *kept == now {
                return loaded.clone();
            }
        }
        let loaded = Arc::new(flows::load(&file.path));
        self.flows.insert(file.path.clone(), (now, loaded.clone()));
        loaded
    }

    /// Forget what is no longer in `catalog`.
    pub fn retain(&mut self, catalog: &Catalog) {
        let sessions: std::collections::HashSet<&PathBuf> =
            catalog.sessions.iter().map(|s| &s.path).collect();
        let flows: std::collections::HashSet<&PathBuf> =
            catalog.flows.iter().map(|f| &f.path).collect();
        self.sessions.retain(|p, _| sessions.contains(p));
        self.flows.retain(|p, _| flows.contains(p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stretto-console-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const HEADER: &str = r#"{"stretto_mcp_log":2,"session":"s","started_unix_ms":1,"server_command":[],"domain":"shop","agent_model":null}"#;

    #[test]
    fn the_catalog_keys_sessions_and_flows_and_finds_the_logs_beside_them() {
        let root = temp("catalog");
        for dir in ["logs/shop", "shadow/shop", "console/trash", "oracle-cache"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let log = |p: &str| std::fs::write(root.join(p), format!("{HEADER}\n")).unwrap();
        log("logs/shop/a.jsonl");
        log("logs/shop/b.jsonl");
        log("shadow/shop/b.jsonl");
        std::fs::write(root.join("shadow/shop/b.flow.jsonl"), "").unwrap();
        std::fs::write(root.join("shadow/shop/b.confirm.jsonl"), "").unwrap();
        // The console's own files, and the cache, are not scanned.
        log("console/trash/c.jsonl");
        log("oracle-cache/d.jsonl");
        std::fs::write(root.join("shop.flow.json"), "{}").unwrap();
        std::fs::write(root.join("diff.flow.json"), "{}").unwrap();
        let c = catalog(&root);
        let keys: Vec<(&str, &str)> = c
            .sessions
            .iter()
            .map(|s| (s.key.as_str(), s.rel.as_str()))
            .collect();
        assert_eq!(keys.len(), 3, "{keys:?}");
        assert_eq!(keys[0], ("a", "logs/shop/a.jsonl"));
        // Two `b`s: each gets the hash of its path.
        assert!(keys[1].0.starts_with("b~") && keys[2].0.starts_with("b~"));
        assert_ne!(keys[1].0, keys[2].0);
        assert_eq!(
            keys[1].0,
            keys::assign(&[("b".into(), "logs/shop/b.jsonl".into())], &["b"])[0]
        );
        let b = c
            .sessions
            .iter()
            .find(|s| s.rel == "shadow/shop/b.jsonl")
            .unwrap();
        assert_eq!(b.flow_log, Some(root.join("shadow/shop/b.flow.jsonl")));
        assert_eq!(
            b.confirm_log,
            Some(root.join("shadow/shop/b.confirm.jsonl"))
        );
        assert_eq!(c.sessions[0].flow_log, None);
        // `diff` is the diff route's, so that flow gets a hash too.
        let flows: Vec<&str> = c.flows.iter().map(|f| f.key.as_str()).collect();
        assert!(flows[0].starts_with("diff~"), "{flows:?}");
        assert_eq!(flows[1], "shop");
        assert_eq!(c.flows[0].name, "diff");
        // Keys are stable: the same files, the same keys.
        let again = catalog(&root);
        assert_eq!(again.sessions, c.sessions);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_catalog_follows_no_symbolic_link_out_of_the_data_directory() {
        let root = temp("links");
        let outside = temp("links-outside");
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(outside.join("x.jsonl"), format!("{HEADER}\n")).unwrap();
        std::fs::write(outside.join("x.flow.json"), "{}").unwrap();
        std::fs::write(root.join("logs/in.jsonl"), format!("{HEADER}\n")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("logs/elsewhere")).unwrap();
        std::os::unix::fs::symlink(outside.join("x.flow.json"), root.join("x.flow.json")).unwrap();
        // A link that stays inside is followed.
        std::os::unix::fs::symlink(root.join("logs/in.jsonl"), root.join("logs/alias.jsonl"))
            .unwrap();
        let c = catalog(&root);
        let rels: Vec<&str> = c.sessions.iter().map(|s| s.rel.as_str()).collect();
        assert_eq!(rels, ["logs/alias.jsonl", "logs/in.jsonl"]);
        assert!(c.flows.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn the_cache_parses_again_only_when_a_file_changes() {
        let root = temp("cache");
        std::fs::write(root.join("s.jsonl"), format!("{HEADER}\n")).unwrap();
        let c = catalog(&root);
        let mut cache = Cache::default();
        let first = cache.session(&c.sessions[0]);
        assert!(Arc::ptr_eq(&first, &cache.session(&c.sessions[0])));
        std::fs::write(
            root.join("s.jsonl"),
            format!("{HEADER}\n{{\"t_ms\":1,\"from\":\"server\",\"raw\":\"x\"}}\n"),
        )
        .unwrap();
        assert!(!Arc::ptr_eq(&first, &cache.session(&c.sessions[0])));
        cache.retain(&Catalog::default());
        assert!(cache.sessions.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
