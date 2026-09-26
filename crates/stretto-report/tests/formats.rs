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
    "bindings.named_other",
    "contracts",
];

/// Fields holding another project's format, which that project documents:
/// the flow's program is in fugue's program format.
const OTHER_FORMATS: &[&str] = &["program"];

/// Paths whose string values are the names of kinds, which the page names too.
const KIND_VALUES: &[&str] = &["manifest.tools.{}", "predicates.*.favors"];

/// Fields the examples do not show: written only when set, or only in flows
/// with code features.
const NOT_IN_EXAMPLES: &[&str] = &[
    "program",
    "every_read",
    "Scalar",
    "Len",
    "promoted",
    "bar",
    "threshold",
    "min_used",
    "min_lower",
    "min_tasks",
    "decisions",
    "lookups",
    "used",
    "tasks",
    "lower",
    "named_other",
    "contracts",
];

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
                if !OTHER_FORMATS.contains(&at(k).as_str()) {
                    names(v, &at(k), out);
                }
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

#[test]
fn an_arbiter_fitted_on_logged_cases_saves_and_loads() {
    use stretto_report::arbitrate::Case;
    // Two options (a lookup, handing back) with the habit's column and
    // three more; the agent took the lookup when the habit favoured it.
    let case = |habit: f64, actual: usize| Case {
        group: 0,
        site: "get_order_details".to_string(),
        features: vec![vec![habit, 0.0, 0.0, 0.0], vec![-habit, 0.0, 0.0, 1.0]],
        pick: 0,
        actual: Some(actual),
        fit: true,
    };
    let cases: Vec<Case> = (0..40)
        .map(|i| case(if i % 4 == 0 { -1.0 } else { 1.0 }, usize::from(i % 4 == 0)))
        .collect();
    let arbiter = Arbiter::fit(
        "a+b",
        vec!["m".to_string()],
        &cases,
        Vec::new(),
        Vec::new(),
        "jev-latest".to_string(),
    );
    assert_eq!(arbiter.provenance().arbiter_cases, 40);
    let dir = std::env::temp_dir().join(format!("stretto-fit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a+b.json");
    arbiter.save(&path).unwrap();
    let loaded = Arbiter::load(&path).unwrap();
    assert_eq!(loaded.domain(), "a+b");
    assert_eq!(
        serde_json::to_value(&loaded).unwrap()["fitted"],
        serde_json::to_value(stretto_report::arbitrate::fit_pooled(&cases)).unwrap()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_review_page_shows_what_flow_show_and_flow_diff_print() {
    let page = repo("docs/review.md");
    let example =
        |name: &str| Flow::load(&repo(&format!("docs/examples/{name}.flow.json"))).unwrap();
    let fenced = |md: &str| format!("```markdown\n{md}```\n");
    let mut sections = vec![(
        "show-5",
        stretto_report::review::show(&example("retail-5-sessions"), 0.3),
    )];
    for (name, old, new) in [
        ("diff-5-10", "retail-5-sessions", "retail-10-sessions"),
        (
            "diff-shipped",
            "retail-5-sessions",
            "retail-5-sessions-shipped-arbiter",
        ),
    ] {
        let d = stretto_report::review::diff(&example(old), &example(new), 0.05, 0.3);
        sections.push((name, d.markdown));
    }
    for (name, md) in sections {
        if let Err(e) = stretto_report::cli_doc::check_page(&page, name, &fenced(&md)) {
            panic!("{e}");
        }
    }
}
