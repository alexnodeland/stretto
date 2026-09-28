//! The API's types as TypeScript, for the UI: `console/src/api/generated/`,
//! one file per type as ts-rs writes it, and `index.ts` exporting them all.
//!
//! `cargo test --features ts` (`make types`) writes them, and fails when
//! what was there differed, so a change to a type that is not committed with
//! its TypeScript fails CI.
//! Integers are `number`: every count and time the API sends fits one.

use super::flows::{FlowDetail, FlowDiffView, FlowList};
use super::jobs::{Job, JobList, JobRequest};
use super::meta::{Health, Meta};
use super::overview::Overview;
use super::servers::{HostConfig, ProbeResult, ServerList, ServerView};
use super::sessions::{SessionDetail, SessionList};
use super::settings::Settings;
use super::{ErrorBody, Ok};
use crate::data::registry::{Registry, ServerInput};
use crate::watch::Changed;
use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use ts_rs::{Config, TypeVisitor, TS};

/// Every type and those it refers to, as `file name → text`.
struct Collect {
    cfg: Config,
    seen: HashSet<TypeId>,
    files: BTreeMap<PathBuf, String>,
}

impl TypeVisitor for Collect {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        let Some(path) = T::output_path() else {
            return;
        };
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        let text = T::export_to_string(&self.cfg)
            .unwrap_or_else(|e| panic!("exporting {}: {e}", std::any::type_name::<T>()));
        self.files.insert(path, text);
        T::visit_dependencies(self);
    }
}

/// The generated files, by name.
fn generate() -> BTreeMap<PathBuf, String> {
    let mut c = Collect {
        cfg: Config::new().with_large_int("number"),
        seen: HashSet::new(),
        files: BTreeMap::new(),
    };
    // What each endpoint answers or takes, and the events.
    c.visit::<Health>();
    c.visit::<Meta>();
    c.visit::<Overview>();
    c.visit::<SessionList>();
    c.visit::<SessionDetail>();
    c.visit::<FlowList>();
    c.visit::<FlowDetail>();
    c.visit::<FlowDiffView>();
    c.visit::<ServerList>();
    c.visit::<ServerView>();
    c.visit::<ServerInput>();
    c.visit::<Registry>();
    c.visit::<HostConfig>();
    c.visit::<ProbeResult>();
    c.visit::<JobList>();
    c.visit::<Job>();
    c.visit::<JobRequest>();
    c.visit::<Settings>();
    c.visit::<ErrorBody>();
    c.visit::<Ok>();
    c.visit::<Changed>();
    let names: Vec<String> = c
        .files
        .keys()
        .filter_map(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let mut index = String::from(
        "// The API's types, generated from the Rust by ts-rs (crates/stretto-console/src/api/typescript.rs).\n\
         // Do not edit these files: `make types` writes them.\n",
    );
    for name in names {
        index.push_str(&format!("export type * from \"./{name}\";\n"));
    }
    c.files.insert(PathBuf::from("index.ts"), index);
    c.files
}

/// Write `files` to `dir`, removing any other `.ts` file there; the names of
/// the files that were not already as they are now.
fn write(dir: &Path, files: &BTreeMap<PathBuf, String>) -> Vec<String> {
    std::fs::create_dir_all(dir).unwrap();
    let mut changed = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = PathBuf::from(entry.file_name());
        if name.extension().is_some_and(|e| e == "ts") && !files.contains_key(&name) {
            std::fs::remove_file(entry.path()).unwrap();
            changed.push(format!("{} (removed)", name.display()));
        }
    }
    for (name, text) in files {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).unwrap();
            changed.push(name.display().to_string());
        }
    }
    changed
}

#[test]
fn the_typescript_types_are_current() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../console/src/api/generated");
    let files = generate();
    // Integers are numbers, and nothing is left without a type.
    for (name, text) in &files {
        assert!(!text.contains("bigint"), "{}: {text}", name.display());
    }
    assert!(files[&PathBuf::from("SessionSummary.ts")].contains("started_unix_ms: number"));
    assert!(files[&PathBuf::from("SiteView.ts")].contains("weighed_by: WeighedBy"));
    let changed = write(&dir, &files);
    assert!(
        changed.is_empty(),
        "the TypeScript in console/src/api/generated/ was out of date and has been rewritten: \
         commit it. Changed: {}",
        changed.join(", ")
    );
}
