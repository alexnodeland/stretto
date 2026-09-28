//! Phase 0 end to end on a miniature τ²-bench checkout, so CI needs no data.

use std::fs;
use stretto_model::features::{Field, Selected};
use stretto_model::projection::Scenario;
use stretto_report::phase0::{ArgRow, ValidatedContext};
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_report::{phase0, render};
use stretto_trace::ToolKind;

mod common;

use common::write_checkout;

#[test]
fn phase0_runs_end_to_end() {
    let root = std::env::temp_dir().join(format!("stretto-phase0-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_checkout(&root);

    let mut config = phase0::Config::new(&root);
    config.domains = vec!["retail".into()];
    config.alpha_samples = 40;
    config.targets = ["retail", "airline"]
        .iter()
        .map(|d| phase0::Target {
            label: None,
            path: root.join(format!("targets/model-t_{d}.json")),
        })
        .collect();
    let report = phase0::run(&config).unwrap();

    let d = &report.domains[0];
    assert_eq!((d.train_tasks, d.test_tasks), (6, 2));
    // The airline target is skipped; the retail one is reported, marked.
    assert_eq!(d.models.len(), 3);
    assert!(!d.models[0].target && d.models[2].target);
    assert_eq!(d.models[2].user_model.as_deref(), Some("other-sim"));
    let m = &d.models[0];
    assert_eq!(m.episodes, 16);
    // Each episode: find_user, get_order (one run), reply, cancel (one run), reply.
    assert!((m.assistant_turns_per_episode - 5.0).abs() < 1e-9);
    assert_eq!(m.runs.removable_turns, 16);
    assert_eq!(m.writes, 16);
    assert!(m.writes_after_yes < m.writes && m.writes_after_assent == m.writes);
    // A fully regular process: the habit should predict it well.
    let (_, k2) = m.by_order.iter().find(|(k, _)| *k == 2).unwrap();
    assert!(k2.top1 > 0.9, "top-1 {}", k2.top1);
    assert!(d.alpha.is_some());
    assert!(d
        .provenance
        .iter()
        .any(|p| p.tool == "cancel_order" && p.arg == "order_id"));

    // Replay: 4 held-out episodes per source model; the target stays out of
    // the pooled projection but gets its own row.
    let f = d.featured.as_ref().expect("features are on by default");
    let pooled = f
        .projection
        .iter()
        .find(|p| p.scenario == Scenario::HabitThenPerfectOracle)
        .unwrap();
    assert_eq!(pooled.episodes, 8);
    assert!(pooled.turns_saved > 0, "{pooled:?}");
    assert!(
        pooled.input_saved > 0.0 && pooled.cost_saved > 0.0,
        "{pooled:?}"
    );
    let target = f.by_model.iter().find(|m| m.target).unwrap();
    assert_eq!(target.model, "vendor/model-t");
    assert_eq!(target.projection.episodes, 4);
    assert!(f.by_model.iter().filter(|m| !m.target).count() == 2);

    let md = render::markdown(&report);
    assert!(md.contains("## retail"));
    assert!(md.contains("Macro-tool headroom"));
    assert!(md.contains("model-t *(target)*"));
    assert!(f.shadow.is_none() && !md.contains("### Phase 0b"));

    // Phase 0b with the mock oracle: every question is built, answered and
    // fed back into the projection.
    config.shadow = Some(ShadowConfig::new(OracleKind::Mock));
    let report = phase0::run(&config).unwrap();
    let f = report.domains[0].featured.as_ref().unwrap();
    let sh = f.shadow.as_ref().expect("Phase 0b ran");
    assert_eq!(sh.oracle, "mock");
    assert!(sh.decisions > 0 && sh.errors == 0, "{sh:?}");
    // One row per model, then the pooled row.
    assert_eq!(sh.rows.len(), 4);
    assert!(sh.rows.iter().all(|r| r.next.n > 0));
    assert_eq!(sh.projection.len(), 2 * sh.thresholds.len());
    assert!(f
        .by_model
        .iter()
        .all(|m| m.with_oracle.len() == 2 * sh.thresholds.len()));
    let md = render::markdown(&report);
    assert!(md.contains("### Phase 0b"));
    assert!(md.contains("**Mock oracle.**"));

    // The v2 questions: read-only flows, with split and combined answers.
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    let report = phase0::run(&config).unwrap();
    let f = report.domains[0].featured.as_ref().unwrap();
    let sh = f.shadow.as_ref().expect("Phase 0b v2 ran");
    assert_eq!(sh.questions, QuestionSet::V2);
    assert!(sh.decisions > 0 && sh.errors == 0, "{sh:?}");
    assert!(sh
        .rows
        .iter()
        .all(|r| r.split.is_some() && r.combined.is_some()));
    assert_eq!(sh.weights.len(), 5);
    assert!((0.0..=1.0).contains(&sh.offered));
    // One question, two keys and combined per threshold, then three
    // lookup-first thresholds; read-only flows take no risks.
    assert_eq!(sh.projection.len(), 3 * sh.thresholds.len() + 3);
    assert!(f.lookup_tokens > 0.0);
    assert!(sh
        .projection
        .iter()
        .all(|p| p.read_only && p.disagreements + p.oracle_disagreements == 0));
    assert!(f.by_model.iter().all(|m| m.read_only.read_only));
    let md = render::markdown(&report);
    assert!(md.contains("Questions v2"));
    assert!(md.contains("Read-only flows"));
    let _ = fs::remove_dir_all(&root);
}

/// `stretto learn` and `stretto promote` on τ²-bench results: a flow learned
/// from the checkout's training tasks, scored on a test task's episodes.
#[test]
fn a_flow_learned_from_results_is_promoted_on_test_tasks() {
    let root = std::env::temp_dir().join(format!("stretto-promote-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_checkout(&root);
    let stretto = env!("CARGO_BIN_EXE_stretto");
    let results = root.join("data/tau2/results/final/model-a_retail_default_user-sim_2trials.json");
    let flow = root.join("retail.flow.json");
    let learned = std::process::Command::new(stretto)
        .args(["learn", "--domain", "retail", "--habit-only", "--tau2"])
        .arg(&root)
        .arg("--results")
        .arg(&results)
        .arg("--out")
        .arg(&flow)
        .output()
        .unwrap();
    assert!(learned.status.success(), "{learned:?}");
    let promoted = root.join("retail-promoted.flow.json");
    let run = std::process::Command::new(stretto)
        .args(["promote", "--flow"])
        .arg(&flow)
        .arg("--results")
        .arg(&results)
        // Another domain's results are skipped.
        .arg("--results")
        .arg(root.join("targets/model-t_airline.json"))
        .arg("--tau2")
        .arg(&root)
        .args(["--task-ids", "6", "--oracle-cache"])
        .arg(root.join("cache"))
        .arg("--out")
        .arg(&promoted)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    // Task 6's two trials, and only those: a decision after each call.
    let report = String::from_utf8_lossy(&run.stdout);
    for site in ["find_user", "get_order", "cancel_order"] {
        assert!(report.contains(&format!("| `{site}` | 2 |")), "{report}");
    }
    let written: serde_json::Value = serde_json::from_slice(&fs::read(&promoted).unwrap()).unwrap();
    assert_eq!(
        written["promoted"]["sites"]["find_user"]["decisions"], 2,
        "{written}"
    );
    fs::remove_dir_all(&root).ok();
}

/// Every part of the report renders: the report of each Phase 0b question
/// set on the fixture, varied where the fixture has nothing to show.
#[test]
fn every_part_of_the_report_renders() {
    let root = std::env::temp_dir().join(format!("stretto-render-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_checkout(&root);
    let mut config = phase0::Config::new(&root);
    config.domains = vec!["retail".into()];
    config.alpha_samples = 0;
    config.shadow = Some(ShadowConfig::new(OracleKind::Mock));
    let v1 = phase0::run(&config).unwrap();
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    let v2 = phase0::run(&config).unwrap();
    fs::remove_dir_all(&root).ok();

    let mut r = v1.clone();
    let d = &mut r.domains[0];
    // A run without a user model or a write; arguments of tools of any kind.
    d.models[0].user_model = None;
    d.models[0].writes = 0;
    d.provenance[0].kind = Some(ToolKind::Generic);
    d.provenance[1].kind = None;
    let f = d.featured.as_mut().unwrap();
    f.selected = vec![Selected {
        tool: "get_order".into(),
        field: Field::Scalar("status".into()),
        values: 2,
        gain: 1.5,
    }];
    let context = |test_n, test_agreed| ValidatedContext {
        context: vec!["start".into(), "find_user".into()],
        action: "get_order".into(),
        cv_n: 10,
        cv_tasks: 4,
        cv_agreed: 9,
        test_n,
        test_agreed,
    };
    f.validated = vec![context(0, 0), context(4, 3)];
    // By model: one with no usage recorded, one with tokens but no cost;
    // one that passes the gate, one below it even with a perfect model.
    let (a, b) = f.by_model.split_at_mut(1);
    let (a, b) = (&mut a[0], &mut b[0]);
    (a.projection.input_tokens, a.projection.output_tokens) = (0.0, 0.0);
    a.projection.cost = 0.0;
    b.projection.cost = 0.0;
    for p in &mut a.with_oracle {
        (p.turns_saved, p.episodes_with_disagreement) = (p.assistant_turns, 0);
    }
    b.projection.turns_saved = 0;
    for p in &mut b.with_oracle {
        p.turns_saved = 0;
    }
    let sh = f.shadow.as_mut().unwrap();
    sh.versions.clear();
    sh.first_error = Some("the `mock` failed".into());
    sh.rows[0].args.n = 3;
    sh.by_arg = vec![ArgRow {
        tool: "cancel_order".into(),
        arg: "reason".into(),
        agreement: sh.rows[0].args.clone(),
    }];
    let md = render::markdown(&r);
    for part in [
        "| ? |",
        "| other |",
        "Kept, in order",
        "| `get_order` | `status` | 2 | 1.5 |",
        "| `start` → `find_user` | get_order | 90.0% of 10 (4 tasks) | not reached |",
        "75.0% of 4",
        "no usage recorded",
        "no cost recorded",
        "First failure: `the 'mock' failed`",
        "Closed-set arguments:",
        "Closed-set arguments by argument",
        "**passes** (",
        "fails: below 20% even with a perfect System-One model",
    ] {
        assert!(md.contains(part), "{part}\n{md}");
    }
    // v2: no cost recorded, and no weights.
    let mut r = v2.clone();
    let sh = r.domains[0]
        .featured
        .as_mut()
        .unwrap()
        .shadow
        .as_mut()
        .unwrap();
    sh.projection[0].cost = 0.0;
    sh.weights.clear();
    let md = render::markdown(&r);
    assert!(!md.contains("The arbiter's weights"), "{md}");
    // Nothing to project, per model or with the System-One model.
    for report in [v1, v2] {
        let mut r = report.clone();
        let f = r.domains[0].featured.as_mut().unwrap();
        f.by_model.clear();
        f.shadow.as_mut().unwrap().projection.clear();
        let md = render::markdown(&r);
        assert!(!md.contains("By agent model"), "{md}");
        let mut r = report;
        let f = r.domains[0].featured.as_mut().unwrap();
        f.projection.clear();
        let md = render::markdown(&r);
        assert!(!md.contains("### Projection:"), "{md}");
    }
}

/// What Phase 0 refuses, and why: a checkout without baselines, a domain
/// without any, a domain without tools, a baseline of another domain, no
/// training sources at all, a live flow that knows the goal, and a decision
/// log it cannot write.
#[test]
fn phase0_says_what_it_cannot_run() {
    let root = std::env::temp_dir().join(format!("stretto-refusals-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let listing = format!("{:#}", phase0::result_files(&root, "retail").unwrap_err());
    assert!(listing.starts_with("listing "), "{listing}");
    write_checkout(&root);
    let results = root.join("data/tau2/results/final");
    fs::write(results.join("notes.json"), "{}").unwrap();
    let none = format!("{:#}", phase0::result_files(&root, "airline").unwrap_err());
    assert!(none.starts_with("no results files for airline"), "{none}");
    let error = |config: &phase0::Config| format!("{:#}", phase0::run(config).unwrap_err());
    let mut config = phase0::Config::new(&root);
    config.alpha_samples = 0;
    config.domains = vec!["airline".into()];
    let tools = error(&config);
    assert!(tools.contains("airline/tools.py"), "{tools}");
    config.domains = vec!["retail".into()];
    let airline = common::results_file("model-c", "airline", "user-sim");
    let misnamed = results.join("model-c_retail_default_user-sim_2trials.json");
    fs::write(&misnamed, serde_json::to_vec(&airline).unwrap()).unwrap();
    let other = error(&config);
    assert!(other.contains("is for airline, not retail"), "{other}");
    fs::remove_file(&misnamed).unwrap();
    config.baselines = false;
    let bare = error(&config);
    assert!(bare.starts_with("no training sources for retail"), "{bare}");
    config.baselines = true;
    config.flow = true;
    let goal = error(&config);
    assert!(goal.starts_with("a live flow is goal free"), "{goal}");
    config.flow = false;
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.log = Some(root.join("no-such-dir/log.jsonl"));
    config.shadow = Some(sc);
    let log = error(&config);
    assert!(log.starts_with("writing "), "{log}");
    // Without features there is nothing to ask about.
    config.features = false;
    let report = phase0::run(&config).unwrap();
    assert!(report.domains[0].featured.is_none());
    fs::remove_dir_all(&root).ok();
}

/// Phase 0 with its other options: a relabeled target, customers who chose
/// among reasons, an episode that writes nothing, and a live flow that
/// describes each lookup by what it feeds, offers every read at every site
/// and weighs no predicate.
#[test]
fn phase0_runs_with_every_option() {
    let root = std::env::temp_dir().join(format!("stretto-options-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_checkout(&root);
    // Half the customers cancel for another reason, some without being
    // asked to confirm, and one does not cancel at all: in a source's
    // results and in the target's.
    let vary = |path: std::path::PathBuf| {
        let mut results: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let sims = results["simulations"].as_array_mut().unwrap();
        for (i, sim) in sims.iter_mut().enumerate() {
            let messages = sim["messages"].as_array_mut().unwrap();
            if i % 2 == 1 {
                messages[8]["tool_calls"][0]["arguments"]["reason"] = "ordered by mistake".into();
            }
            if i % 4 < 2 {
                messages.drain(6..8);
            }
            if i == 3 {
                messages.truncate(7);
            }
        }
        fs::write(&path, serde_json::to_vec(&results).unwrap()).unwrap();
    };
    vary(root.join("data/tau2/results/final/model-a_retail_default_user-sim_2trials.json"));
    vary(root.join("targets/model-t_retail.json"));
    let mut config = phase0::Config::new(&root);
    config.domains = vec!["retail".into()];
    config.alpha_samples = 0;
    config.targets = vec![phase0::Target {
        label: Some("renamed".into()),
        path: root.join("targets/model-t_retail.json"),
    }];
    config.shadow = Some(ShadowConfig::new(OracleKind::Mock));
    let report = phase0::run(&config).unwrap();
    let f = report.domains[0].featured.as_ref().unwrap();
    assert_eq!(
        f.by_model.iter().find(|m| m.target).unwrap().model,
        "renamed"
    );
    let sh = f.shadow.as_ref().unwrap();
    assert!(
        sh.by_arg
            .iter()
            .any(|r| r.tool == "cancel_order" && r.arg == "reason"),
        "{:?}",
        sh.by_arg
    );
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    sc.hints = true;
    sc.manifest_options = true;
    sc.predicate_features = false;
    config.shadow = Some(sc);
    let mock = stretto_oracle::MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let flow = phase0::compile_flow(&config, "retail", &mock).unwrap();
    assert!(flow.has_arbiter());
    let sites = flow.sites();
    assert!(!sites.is_empty(), "{sites:?}");
    fs::remove_dir_all(&root).ok();
}
