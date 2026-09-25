//! `docs/formats.md` names every field a flow or an arbiter file holds, and
//! the published examples still load.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use stretto_report::flow::{Arbiter, Flow};

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Paths (`.` between keys, `*` for any list element, `{}` for any value of
/// a map) whose objects are keyed by data, such as tool names, not fields.
const DATA_MAPS: &[&str] = &[
    "manifest.tools",
    "manifest.docs",
    "manifest.docs.{}.args",
    "map.fields",
    "sites.next.*.*",
    "sites.feeds",
    "bindings.args",
    "bindings.args.{}.*",
    "bindings.agreed",
];

/// Paths whose string values are the names of kinds, which the page names too.
const KIND_VALUES: &[&str] = &["manifest.tools.{}", "predicates.*.favors"];

/// Fields the examples do not show: written only when set, or only in flows
/// with code features.
const NOT_IN_EXAMPLES: &[&str] = &["every_read", "Scalar", "Len"];

fn names(v: &Value, path: &str, out: &mut BTreeSet<String>) {
    let at = |key: &str| {
        if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        }
    };
    match v {
        Value::Object(map) if DATA_MAPS.contains(&path) => {
            for v in map.values() {
                names(v, &at("{}"), out);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.clone());
                names(v, &at(k), out);
            }
        }
        Value::Array(items) => {
            for v in items {
                names(v, &at("*"), out);
            }
        }
        Value::String(s) if KIND_VALUES.contains(&path) => {
            out.insert(s.clone());
        }
        _ => {}
    }
}

/// Every identifier inside the page's inline code.
fn documented(page: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, span) in page.split('`').enumerate() {
        if i % 2 == 1 {
            for word in span.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if !word.is_empty() {
                    out.insert(word.to_string());
                }
            }
        }
    }
    out
}

#[test]
fn the_formats_page_names_every_field() {
    let page = std::fs::read_to_string(repo("docs/formats.md")).unwrap();
    let documented = documented(&page);
    let mut fields = BTreeSet::new();
    for example in [
        "docs/results/cold-start-2026-09-24-live.flow.json",
        "data/arbiters/retail.json",
        "data/arbiters/airline.json",
    ] {
        let json: Value =
            serde_json::from_str(&std::fs::read_to_string(repo(example)).unwrap()).unwrap();
        names(&json, "", &mut fields);
    }
    fields.extend(NOT_IN_EXAMPLES.iter().map(|s| s.to_string()));
    let missing: Vec<&String> = fields.difference(&documented).collect();
    assert!(
        missing.is_empty(),
        "docs/formats.md does not name {missing:?}"
    );
}

#[test]
fn the_published_examples_load() {
    let flow = Flow::load(&repo("docs/results/cold-start-2026-09-24-live.flow.json")).unwrap();
    assert!(flow.has_arbiter());
    assert_eq!(flow.domain(), "retail");
    for domain in ["retail", "airline"] {
        let arbiter = Arbiter::load(&repo(&format!("data/arbiters/{domain}.json"))).unwrap();
        assert_eq!(arbiter.domain(), domain);
    }
}
