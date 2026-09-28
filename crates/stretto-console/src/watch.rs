//! Live updates: a watcher that polls the data directory every second, by
//! the lengths and modification times of its files, and says what changed.
//!
//! Its events, and each job's progress ([`crate::jobs`]), go to every
//! client of `/api/events`. It does nothing while nobody listens.

use crate::api::jobs::Job;
use crate::data::{self, stamp, Stamp};
use crate::Shared;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Duration;
#[cfg(feature = "ts")]
use ts_rs::TS;

/// How often the data directory is looked at.
pub const EVERY: Duration = Duration::from_secs(1);

/// What `/api/events` sends.
#[derive(Clone, Debug)]
pub enum Event {
    /// `event: changed`
    Changed(Changed),
    /// `event: job`: a job, whose status or output moved on.
    Job(Box<Job>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum ChangedWhat {
    Sessions,
    Flows,
    Servers,
    Jobs,
}

/// `event: changed`: which items were added, changed or removed, by key
/// (servers: `servers.json`; jobs: by id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(TS))]
pub struct Changed {
    pub what: ChangedWhat,
    pub keys: Vec<String>,
}

/// What the watcher compares from one look to the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    sessions: BTreeMap<String, Vec<Stamp>>,
    flows: BTreeMap<String, Stamp>,
    servers: Stamp,
    jobs: BTreeMap<String, Stamp>,
}

/// The data directory's files as they are now.
pub fn snapshot(root: &std::path::Path) -> Snapshot {
    let catalog = data::catalog(root);
    let jobs_dir = root.join(data::CONSOLE_DIR).join("jobs");
    let jobs = std::fs::read_dir(&jobs_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?;
            (!id.starts_with('.') && !id.ends_with(".audit"))
                .then(|| (id.to_string(), stamp(&e.path())))
        })
        .collect();
    Snapshot {
        sessions: catalog
            .sessions
            .iter()
            .map(|s| {
                (
                    s.key.clone(),
                    vec![
                        stamp(&s.path),
                        s.flow_log.as_deref().and_then(stamp),
                        s.confirm_log.as_deref().and_then(stamp),
                    ],
                )
            })
            .collect(),
        flows: catalog
            .flows
            .iter()
            .map(|f| (f.key.clone(), stamp(&f.path)))
            .collect(),
        servers: stamp(&root.join(data::registry::FILE)),
        jobs,
    }
}

/// What changed from `before` to `now`.
pub fn changes(before: &Snapshot, now: &Snapshot) -> Vec<Changed> {
    fn keys<V: PartialEq>(a: &BTreeMap<String, V>, b: &BTreeMap<String, V>) -> Vec<String> {
        let mut out: Vec<String> = b
            .iter()
            .filter(|(k, v)| a.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .chain(a.keys().filter(|k| !b.contains_key(*k)).cloned())
            .collect();
        out.sort();
        out
    }
    let mut out = Vec::new();
    for (what, keys) in [
        (ChangedWhat::Sessions, keys(&before.sessions, &now.sessions)),
        (ChangedWhat::Flows, keys(&before.flows, &now.flows)),
        (ChangedWhat::Jobs, keys(&before.jobs, &now.jobs)),
    ] {
        if !keys.is_empty() {
            out.push(Changed { what, keys });
        }
    }
    if before.servers != now.servers {
        out.push(Changed {
            what: ChangedWhat::Servers,
            keys: vec![data::registry::FILE.to_string()],
        });
    }
    out
}

/// Watch `state`'s data directory until the process ends.
pub fn spawn(state: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut before: Option<Snapshot> = None;
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if state.events.receiver_count() == 0 {
                before = None;
                continue;
            }
            let root = state.data_dir().to_path_buf();
            let now = tokio::task::spawn_blocking(move || snapshot(&root)).await;
            let Ok(now) = now else { continue };
            if let Some(before) = &before {
                for change in changes(before, &now) {
                    let _ = state.events.send(Event::Changed(change));
                }
            }
            before = Some(now);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_said_by_key() {
        let root =
            std::env::temp_dir().join(format!("stretto-console-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("logs")).unwrap();
        let header = r#"{"stretto_mcp_log":2,"session":"a","started_unix_ms":1,"server_command":[],"domain":null,"agent_model":null}"#;
        std::fs::write(root.join("logs/a.jsonl"), format!("{header}\n")).unwrap();
        let first = snapshot(&root);
        assert!(changes(&first, &snapshot(&root)).is_empty());
        std::fs::write(root.join("logs/a.jsonl"), format!("{header}\n{header}\n")).unwrap();
        std::fs::write(root.join("logs/b.jsonl"), format!("{header}\n")).unwrap();
        std::fs::write(root.join("s.flow.json"), "{}").unwrap();
        std::fs::write(root.join("servers.json"), "{}").unwrap();
        let second = snapshot(&root);
        let c = changes(&first, &second);
        assert_eq!(
            c,
            [
                Changed {
                    what: ChangedWhat::Sessions,
                    keys: vec!["a".into(), "b".into()]
                },
                Changed {
                    what: ChangedWhat::Flows,
                    keys: vec!["s".into()]
                },
                Changed {
                    what: ChangedWhat::Servers,
                    keys: vec!["servers.json".into()]
                },
            ]
        );
        std::fs::remove_file(root.join("logs/b.jsonl")).unwrap();
        let c = changes(&second, &snapshot(&root));
        assert_eq!(
            c,
            [Changed {
                what: ChangedWhat::Sessions,
                keys: vec!["b".into()]
            }]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
