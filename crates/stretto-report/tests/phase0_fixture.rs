//! Phase 0 end to end on a miniature τ²-bench checkout, so CI needs no data.

use std::fs;
use std::path::Path;
use stretto_model::projection::Scenario;
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_report::{phase0, render};

const TOOLS_PY: &str = r#"
    @is_tool(ToolType.READ)
    def find_user(self, email: str) -> str:
        ...
    @is_tool(ToolType.READ)
    def get_order(self, order_id: str) -> Order:
        ...
    @is_tool(ToolType.WRITE)
    def cancel_order(self, order_id: str, reason: str) -> Order:
        ...
"#;

/// One simulation: find the user, look up the order, confirm, cancel.
fn simulation(id: usize, task: usize, confirm: &str) -> serde_json::Value {
    let order = format!("#W{task:04}");
    serde_json::json!({
        "id": format!("s{id}"), "task_id": task.to_string(), "trial": id % 2,
        "reward_info": {"reward": if id.is_multiple_of(5) { 0.0 } else { 1.0 }},
        "messages": [
            {"role": "assistant", "content": "Hi! How can I help you today?", "cost": 0.0},
            {"role": "user", "content": format!("Cancel {order}, I am a@b.com")},
            {"role": "assistant", "cost": 0.01, "usage": usage(1000), "tool_calls": [
                {"id": format!("{id}a"), "name": "find_user", "arguments": {"email": "a@b.com"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}a"), "content": "\"u_1\"", "error": false},
            {"role": "assistant", "cost": 0.01, "usage": usage(1110), "tool_calls": [
                {"id": format!("{id}b"), "name": "get_order", "arguments": {"order_id": order},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}b"),
             "content": format!("{{\"order_id\": \"{order}\", \"status\": \"pending\"}}"),
             "error": false},
            {"role": "assistant", "content": "Cancel it? (yes/no)", "cost": 0.01,
             "usage": usage(1220)},
            {"role": "user", "content": confirm},
            {"role": "assistant", "cost": 0.01, "usage": usage(1330), "tool_calls": [
                {"id": format!("{id}c"), "name": "cancel_order",
                 "arguments": {"order_id": order, "reason": "no longer needed"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}c"), "content": "{\"status\": \"cancelled\"}",
             "error": false},
            {"role": "assistant", "content": "Done.", "cost": 0.01, "usage": usage(1440)}
        ]
    })
}

fn usage(prompt_tokens: u64) -> serde_json::Value {
    serde_json::json!({"prompt_tokens": prompt_tokens, "completion_tokens": 10})
}

/// A results file for `model` in `domain`.
fn results_file(model: &str, domain: &str, user: &str) -> serde_json::Value {
    let sims: Vec<_> = (0..16)
        .map(|i| {
            simulation(
                i,
                i / 2,
                if i.is_multiple_of(3) {
                    "go ahead"
                } else {
                    "yes"
                },
            )
        })
        .collect();
    serde_json::json!({
        "info": {
            "agent_info": {"llm": model},
            "user_info": {"llm": user},
            "environment_info": {"domain_name": domain, "policy": "p", "tool_defs": null}
        },
        "tasks": [],
        "simulations": sims
    })
}

fn write_checkout(root: &Path) {
    let domain_src = root.join("src/tau2/domains/retail");
    let domain_data = root.join("data/tau2/domains/retail");
    let results = root.join("data/tau2/results/final");
    for d in [&domain_src, &domain_data, &results] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(domain_src.join("tools.py"), TOOLS_PY).unwrap();
    fs::write(
        domain_data.join("split_tasks.json"),
        r#"{"train": ["0", "1", "2", "3", "4", "5"], "test": ["6", "7"], "base": []}"#,
    )
    .unwrap();
    for model in ["model-a", "model-b"] {
        fs::write(
            results.join(format!("{model}_retail_default_user-sim_2trials.json")),
            serde_json::to_vec(&results_file(model, "retail", "user-sim")).unwrap(),
        )
        .unwrap();
    }
    // Transfer targets outside the checkout: one for this domain, one not.
    let targets = root.join("targets");
    fs::create_dir_all(&targets).unwrap();
    for domain in ["retail", "airline"] {
        fs::write(
            targets.join(format!("model-t_{domain}.json")),
            serde_json::to_vec(&results_file("vendor/model-t", domain, "other-sim")).unwrap(),
        )
        .unwrap();
    }
}

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
    // One question, two keys and combined, per threshold; read-only flows
    // take no risks.
    assert_eq!(sh.projection.len(), 3 * sh.thresholds.len());
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
