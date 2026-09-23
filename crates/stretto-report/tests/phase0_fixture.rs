//! Phase 0 end to end on a miniature τ²-bench checkout, so CI needs no data.

use std::fs;
use std::path::Path;
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
            {"role": "assistant", "cost": 0.01, "tool_calls": [
                {"id": format!("{id}a"), "name": "find_user", "arguments": {"email": "a@b.com"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}a"), "content": "\"u_1\"", "error": false},
            {"role": "assistant", "cost": 0.01, "tool_calls": [
                {"id": format!("{id}b"), "name": "get_order", "arguments": {"order_id": order},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}b"),
             "content": format!("{{\"order_id\": \"{order}\", \"status\": \"pending\"}}"),
             "error": false},
            {"role": "assistant", "content": "Cancel it? (yes/no)", "cost": 0.01},
            {"role": "user", "content": confirm},
            {"role": "assistant", "cost": 0.01, "tool_calls": [
                {"id": format!("{id}c"), "name": "cancel_order",
                 "arguments": {"order_id": order, "reason": "no longer needed"},
                 "requestor": "assistant"}]},
            {"role": "tool", "id": format!("{id}c"), "content": "{\"status\": \"cancelled\"}",
             "error": false},
            {"role": "assistant", "content": "Done.", "cost": 0.01}
        ]
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
        let run = serde_json::json!({
            "info": {
                "agent_info": {"llm": model},
                "user_info": {"llm": "user-sim"},
                "environment_info": {"domain_name": "retail", "policy": "p", "tool_defs": null}
            },
            "tasks": [],
            "simulations": sims
        });
        fs::write(
            results.join(format!("{model}_retail_default_user-sim_2trials.json")),
            serde_json::to_vec(&run).unwrap(),
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
    let report = phase0::run(&config).unwrap();

    let d = &report.domains[0];
    assert_eq!((d.train_tasks, d.test_tasks), (6, 2));
    assert_eq!(d.models.len(), 2);
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

    let md = render::markdown(&report);
    assert!(md.contains("## retail"));
    assert!(md.contains("Macro-tool headroom"));
    let _ = fs::remove_dir_all(&root);
}
