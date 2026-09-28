//! Flows: `*.flow.json`, loaded by [`Flow::load`](stretto_report::flow::Flow).

use crate::api::flows::{FlowSummary, PromotedCounts, ToolCounts};
use crate::api::sessions::ToolInfo;
use crate::data::registry::{self, DeciderName, Registry};
use crate::data::{paths, FlowFile};
use crate::State;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use stretto_report::flow::Flow;
use stretto_report::review;
use stretto_trace::ToolKind;

/// A flow file, read.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The flow, or why it does not load.
    pub flow: Result<Arc<Flow>, String>,
    /// The file's `stretto_flow`, when it has one.
    pub format_version: Option<u32>,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
    /// The lookups it may make, over every site.
    pub lookups: BTreeSet<String>,
}

/// Read the flow at `path`.
pub fn load(path: &Path) -> Loaded {
    let meta = std::fs::metadata(path).ok();
    let size_bytes = meta.as_ref().map_or(0, |m| m.len());
    let modified_unix_ms = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    let text = std::fs::read_to_string(path);
    #[derive(Deserialize)]
    struct Version {
        stretto_flow: u32,
    }
    let format_version = text
        .as_ref()
        .ok()
        .and_then(|t| serde_json::from_str::<Version>(t).ok())
        .map(|v| v.stretto_flow);
    let flow = match text {
        Ok(text) => Flow::from_json(&text)
            .map(Arc::new)
            .map_err(|e| format!("{e:#}")),
        Err(e) => Err(format!("reading it: {e}")),
    };
    let lookups = match &flow {
        Ok(f) => lookups(f),
        Err(_) => BTreeSet::new(),
    };
    Loaded {
        flow,
        format_version,
        size_bytes,
        modified_unix_ms,
        lookups,
    }
}

/// The lookups `flow` may make after any call.
pub fn lookups(flow: &Flow) -> BTreeSet<String> {
    review::view(flow, 0.3)
        .sites
        .into_iter()
        .flat_map(|s| s.lookups.into_iter().map(|l| l.tool))
        .collect()
}

/// Every flow in the data directory, most recently changed first.
pub fn all(state: &State) -> Vec<(FlowFile, Arc<Loaded>)> {
    let catalog = crate::data::catalog(state.data_dir());
    let mut cache = state
        .cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut out: Vec<(FlowFile, Arc<Loaded>)> = catalog
        .flows
        .iter()
        .map(|f| (f.clone(), cache.flow(f)))
        .collect();
    cache.retain(&catalog);
    out.sort_by(|(a, x), (b, y)| {
        y.modified_unix_ms
            .cmp(&x.modified_unix_ms)
            .then_with(|| a.rel.cmp(&b.rel))
    });
    out
}

/// The names of the servers in `registry` whose flow is `file`.
pub fn served_by(file: &FlowFile, registry: &Registry, state: &State) -> Vec<String> {
    let wanted = paths::canonical(&file.path);
    registry
        .servers
        .iter()
        .filter(|s| {
            registry::flow_path(s, state.data_dir(), state.config.home.as_deref()).is_some_and(
                |p| p == file.path || std::fs::canonicalize(&p).is_ok_and(|p| p == wanted),
            )
        })
        .map(|s| s.name.clone())
        .collect()
}

/// The list's summary of a flow file.
pub fn summary(file: &FlowFile, loaded: &Loaded, served_by: Vec<String>) -> FlowSummary {
    let mut s = FlowSummary {
        key: file.key.clone(),
        path: file.rel.clone(),
        name: file.name.clone(),
        domain: String::new(),
        format_version: loaded.format_version.unwrap_or(0),
        stretto_version: String::new(),
        sources: Vec::new(),
        habit_episodes: 0,
        arbiter_cases: 0,
        compiled_unix_ms: 0,
        modified_unix_ms: loaded.modified_unix_ms,
        size_bytes: loaded.size_bytes,
        decider: DeciderName::Habit,
        has_arbiter: false,
        has_reach: false,
        tools: ToolCounts::default(),
        sites: 0,
        lookups: 0,
        promoted: None,
        served_by,
        error: None,
    };
    let flow = match &loaded.flow {
        Ok(flow) => flow,
        Err(e) => {
            s.error = Some(e.clone());
            return s;
        }
    };
    let p = flow.provenance();
    s.domain = flow.domain().to_string();
    s.stretto_version = p.stretto.clone();
    s.sources = p.sources.clone();
    s.habit_episodes = p.habit_episodes;
    s.arbiter_cases = p.arbiter_cases;
    s.compiled_unix_ms = p.compiled_unix_ms;
    s.decider = DeciderName::of(flow.served_decider());
    s.has_arbiter = flow.has_arbiter();
    s.has_reach = flow.has_reach();
    for kind in flow.manifest().tools.values() {
        match kind {
            ToolKind::Read => s.tools.read += 1,
            ToolKind::Write => s.tools.write += 1,
            ToolKind::Generic => s.tools.generic += 1,
        }
    }
    s.sites = flow.sites().len();
    s.lookups = loaded.lookups.len();
    s.promoted = flow.promotion().map(|p| PromotedCounts {
        sites_promoted: p.sites.values().filter(|r| r.promoted).count(),
        sites_scored: p.sites.len(),
    });
    s
}

/// What a server's tools, as listed now (`tools`, described as `source`),
/// say against what `flow` was learned with: a tool the flow reads that the
/// server marks `readOnlyHint: false`, which the proxy then never looks up;
/// a lookup the server no longer lists; and a lookup whose input contract
/// changed since the flow pinned it, which the proxy no longer makes.
pub fn tool_warnings(flow: &Flow, tools: &[ToolInfo], source: &str) -> Vec<String> {
    let listed: HashMap<&str, &ToolInfo> = tools.iter().map(|t| (t.name.as_str(), t)).collect();
    let lookups = lookups(flow);
    let reads: BTreeSet<String> = flow
        .manifest()
        .tools
        .iter()
        .filter(|(_, k)| **k == ToolKind::Read)
        .map(|(t, _)| t.clone())
        .chain(lookups.iter().cloned())
        .collect();
    let mut out = Vec::new();
    for t in &reads {
        if listed
            .get(t.as_str())
            .is_some_and(|l| l.read_only_hint == Some(false))
        {
            out.push(format!(
                "`{t}` is read-only in the flow, but {source} marks it readOnlyHint: false, so \
                 the proxy will not look it up"
            ));
        }
    }
    if !tools.is_empty() {
        for t in &lookups {
            if !listed.contains_key(t.as_str()) {
                out.push(format!(
                    "the flow looks up `{t}`, which {source} does not list"
                ));
            }
        }
    }
    for (t, pinned) in flow.contracts() {
        if !lookups.contains(t) {
            continue;
        }
        if let Some(now) = listed.get(t.as_str()).and_then(|l| l.contract.as_ref()) {
            if now != pinned {
                out.push(format!(
                    "`{t}`'s input changed since the flow was learned ({pinned} → {now} in \
                     {source}), so the proxy no longer looks it up"
                ));
            }
        }
    }
    out
}

/// [`tool_warnings`] against the tools the newest recorded session of the
/// flow's domain listed, and whether a flow with an arbiter will find a key.
pub fn warnings(state: &State, flow: &Flow) -> Vec<String> {
    let sessions = crate::data::sessions::all(state);
    let newest = sessions
        .iter()
        .find(|(s, loaded)| s.domain.as_deref() == Some(flow.domain()) && !loaded.tools.is_empty());
    let mut out = match newest {
        Some((s, loaded)) => tool_warnings(
            flow,
            &loaded.tools,
            &format!("the latest recorded tools/list (session {})", s.session_id),
        ),
        None => Vec::new(),
    };
    if flow.has_arbiter() && !state.key_set() {
        out.push(
            "the flow has an arbiter, which asks TypeSafe's Jev when served: TYPESAFE_API_KEY \
             is not set here, so give the proxy one, or serve it with the habit or reach decider"
                .to_string(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::sessions::tool_info;
    use serde_json::json;

    fn fixture_flow(name: &str) -> Arc<Flow> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/home")
            .join(name);
        load(&path).flow.unwrap()
    }

    #[test]
    fn a_flow_summary() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/home");
        let catalog = crate::data::catalog(&root);
        let file = catalog.flow("shop").unwrap();
        let loaded = load(&file.path);
        let s = summary(file, &loaded, vec!["shop".into()]);
        assert_eq!(
            (s.key.as_str(), s.name.as_str(), s.domain.as_str()),
            ("shop", "shop", "shop")
        );
        assert_eq!(s.format_version, 1);
        assert_eq!(s.decider, DeciderName::Reach);
        assert!(!s.has_arbiter && s.has_reach);
        assert_eq!((s.tools.read, s.tools.write, s.tools.generic), (3, 1, 0));
        assert_eq!((s.sites, s.lookups), (3, 2));
        assert_eq!(s.habit_episodes, 6);
        assert_eq!(s.sources, ["quickstart"]);
        assert!(s.promoted.is_none() && s.error.is_none());
        assert_eq!(s.served_by, ["shop"]);

        let promoted = catalog.flow("shop.promoted").unwrap();
        let s = summary(promoted, &load(&promoted.path), Vec::new());
        let p = s.promoted.unwrap();
        assert_eq!((p.sites_promoted, p.sites_scored), (3, 4));
    }

    #[test]
    fn a_flow_that_does_not_load_has_its_error_and_defaults() {
        let dir =
            std::env::temp_dir().join(format!("stretto-console-flows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.flow.json"), "{\"stretto_flow\": 9}").unwrap();
        let catalog = crate::data::catalog(&dir);
        let file = &catalog.flows[0];
        let s = summary(file, &load(&file.path), Vec::new());
        assert!(s
            .error
            .as_deref()
            .unwrap()
            .contains("flow format 9 is not supported"));
        assert_eq!((s.format_version, s.domain.as_str(), s.sites), (9, "", 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_server_that_changed_its_tools_warns() {
        let flow = fixture_flow("shop.flow.json");
        let tool = |name: &str, read_only: bool, args: &[&str]| {
            let props: serde_json::Map<String, serde_json::Value> = args
                .iter()
                .map(|a| (a.to_string(), json!({"type": "string"})))
                .collect();
            tool_info(
                &json!({"name": name, "annotations": {"readOnlyHint": read_only},
                "inputSchema": {"type": "object", "properties": props, "required": args}}),
            )
        };
        let same = [
            tool("find_user_id_by_email", true, &["email"]),
            tool("get_user_details", true, &["user_id"]),
            tool("get_order_details", true, &["order_id"]),
            tool("cancel_pending_order", false, &["order_id", "reason"]),
        ];
        assert!(tool_warnings(&flow, &same, "the server").is_empty());
        let changed = [
            tool("find_user_id_by_email", true, &["email"]),
            tool("get_user_details", false, &["user_id"]),
            tool("cancel_pending_order", false, &["order_id", "reason"]),
        ];
        let w = tool_warnings(&flow, &changed, "the server");
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("`get_user_details` is read-only in the flow, but the server marks it readOnlyHint: false"));
        assert!(
            w[1].contains("the flow looks up `get_order_details`, which the server does not list")
        );
        let renamed = [
            tool("get_user_details", true, &["id"]),
            tool("get_order_details", true, &["order_id"]),
        ];
        let w = tool_warnings(&flow, &renamed, "the server");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("`get_user_details`'s input changed since the flow was learned (user_id:string! → id:string!"));
        // A server that lists no tools is not said to lack the lookups.
        assert!(tool_warnings(&flow, &[], "the server").is_empty());
    }
}
