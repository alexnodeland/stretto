//! Learning a live flow from episodes that are not τ²-bench's, as from
//! sessions `stretto-proxy` recorded.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use stretto_oracle::MockOracle;
use stretto_report::flow::{Decider, Proposal};
use stretto_report::phase0::{
    compile_flow_from_episodes, compile_habit_flow_from_episodes, Config,
};
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

pub fn manifest() -> ToolManifest {
    ToolManifest {
        domain: "shop".to_string(),
        tools: BTreeMap::from([
            ("find_account".to_string(), ToolKind::Read),
            ("get_account".to_string(), ToolKind::Read),
            ("get_order".to_string(), ToolKind::Read),
            ("close_order".to_string(), ToolKind::Write),
        ]),
        docs: BTreeMap::new(),
    }
}

fn call(id: &str, name: &str, arguments: Value) -> Event {
    Event::Assistant {
        text: None,
        calls: vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }],
        usage: None,
    }
}

fn result(id: &str, name: &str, content: &str) -> Event {
    Event::ToolResult {
        call_id: id.to_string(),
        name: name.to_string(),
        error: false,
        content: content.to_string(),
    }
}

fn say(text: &str) -> Event {
    Event::Assistant {
        text: Some(text.to_string()),
        calls: Vec::new(),
        usage: None,
    }
}

/// Customer `i` finds their account, the agent reads it and each of its
/// orders, then closes the first once the customer agrees.
fn session(i: usize) -> Episode {
    let account = format!("acct_{i}");
    let orders = [format!("o{i}a"), format!("o{i}b")];
    Episode {
        id: format!("session-{i}"),
        task_id: String::new(),
        trial: 0,
        domain: "shop".to_string(),
        agent_model: "agent".to_string(),
        reward: 1.0,
        events: vec![
            Event::User {
                text: format!("Hi, I'm c{i}@example.com and I want to close an order."),
            },
            call(
                "1",
                "find_account",
                json!({"email": format!("c{i}@example.com")}),
            ),
            result("1", "find_account", &account),
            call("2", "get_account", json!({"account_id": account})),
            result(
                "2",
                "get_account",
                &json!({"account_id": account, "orders": orders}).to_string(),
            ),
            call("3", "get_order", json!({"order_id": orders[0]})),
            result(
                "3",
                "get_order",
                &json!({"order_id": orders[0], "status": "open"}).to_string(),
            ),
            call("4", "get_order", json!({"order_id": orders[1]})),
            result(
                "4",
                "get_order",
                &json!({"order_id": orders[1], "status": "open"}).to_string(),
            ),
            say("You have two open orders. Close the first?"),
            Event::User {
                text: "Yes, please.".to_string(),
            },
            call("5", "close_order", json!({"order_id": orders[0]})),
            result("5", "close_order", "closed"),
            say("Done."),
        ],
    }
}

fn mock_config() -> Config {
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    config
}

const MOCK: MockOracle = MockOracle {
    confidence: 0.6,
    noul: 0.5,
};

pub fn learned_flow() -> stretto_report::flow::Flow {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    compile_flow_from_episodes(&mock_config(), &episodes, &manifest(), &MOCK).unwrap()
}

/// A new customer's session, just after the agent found their account.
fn new_customer() -> Episode {
    let mut live = session(999);
    live.events.truncate(3);
    live
}

#[test]
fn a_flow_learned_from_sessions_continues_a_live_one() {
    let flow = learned_flow();
    assert_eq!(flow.domain(), "shop");
    assert!(flow.provenance().habit_episodes > 30);
    assert!(flow.provenance().arbiter_cases > 30);

    // A new customer: after the agent finds the account, the flow reads it,
    // with the id bound from the first result.
    let mut live = session(999);
    live.events.truncate(3);
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let next = flow.next(&live, &oracle, 0.3).unwrap();
    assert_eq!(
        next.proposal,
        Proposal::Lookup {
            tool: "get_account".to_string(),
            arguments: json!({"account_id": "acct_999"}),
        },
        "{:?}",
        next
    );

    // It survives its IR.
    let back =
        stretto_report::flow::Flow::from_json(&serde_json::to_string(&flow).unwrap()).unwrap();
    assert_eq!(
        back.next(&live, &oracle, 0.3).unwrap().proposal,
        next.proposal
    );

    // A write is never a flow's to make: after the orders, it hands back
    // or looks up, but never closes an order.
    let mut later = session(999);
    later.events.truncate(9);
    match flow.next(&later, &oracle, 0.3).unwrap().proposal {
        Proposal::Lookup { tool, .. } => assert_ne!(tool, "close_order"),
        Proposal::HandBack { .. } => {}
    }
}

#[test]
fn an_audit_scores_new_sessions_under_the_flow() {
    use stretto_report::audit::{audit, decisions, score};
    let flow = learned_flow();
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    let new: Vec<Episode> = (100..110)
        .map(|i| {
            let mut ep = session(i);
            ep.task_id = ep.id.clone();
            ep
        })
        .collect();
    let a = audit(&flow, &new, &oracle);
    assert_eq!(a.episodes, 10);
    assert!(a.decisions >= 30, "{}", a.decisions);
    assert_eq!(a.unanswered, 0);
    // The sessions follow the training script, so the flow fits them well.
    assert!(a.agreement > 0.8, "{}", a.agreement);
    // fugue's score is the sum of the flow's log-probabilities of the
    // agent's steps.
    let (ds, _) = decisions(&flow, &new[0], &oracle);
    let by_hand: f64 = ds.iter().map(|d| d.probs[d.actual].ln()).sum();
    assert!((score(&ds).log_prior - by_hand).abs() < 1e-9);
    // An agent that skips the orders surprises it more.
    let mut odd = session(200);
    odd.task_id = odd.id.clone();
    odd.events.drain(5..9);
    let surprised = audit(&flow, &[odd], &oracle);
    assert!(surprised.mean_log_prob < a.mean_log_prob);
}

#[test]
fn a_learned_flow_judges_every_new_session_with_one_arbiter() {
    // The sessions a learned flow serves are new tasks, so its arbiter is
    // fitted on every held-out decision, whatever fold a session falls in.
    let flow = learned_flow();
    let probs: Vec<_> = (0..20)
        .map(|k| {
            let mut live = new_customer();
            live.task_id = format!("new-{k}");
            flow.next(&live, &MOCK, 0.3).unwrap().probs
        })
        .collect();
    assert!(!probs[0].is_empty());
    assert!(probs.windows(2).all(|w| w[0] == w[1]), "{probs:?}");
}

#[test]
fn a_flow_learns_from_three_sessions_but_not_from_one() {
    let three: Vec<Episode> = (0..3).map(session).collect();
    let flow = compile_flow_from_episodes(&mock_config(), &three, &manifest(), &MOCK).unwrap();
    assert_eq!(flow.provenance().habit_episodes, 2);
    assert!(flow.provenance().arbiter_cases > 0);
    let err =
        compile_flow_from_episodes(&mock_config(), &[session(0)], &manifest(), &MOCK).unwrap_err();
    assert!(format!("{err:#}").contains("too few"), "{err:#}");
}

#[test]
fn a_habit_only_flow_learns_from_every_session_and_has_no_arbiter() {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let flow = compile_habit_flow_from_episodes(&config, &episodes, &manifest()).unwrap();
    assert!(!flow.has_arbiter());
    assert_eq!(flow.provenance().habit_episodes, 60);
    assert_eq!(flow.provenance().arbiter_cases, 0);
    let live = new_customer();
    let next = flow.next_with(&live, &MOCK, 0.3, Decider::Habit).unwrap();
    assert_eq!(
        next.proposal,
        Proposal::Lookup {
            tool: "get_account".to_string(),
            arguments: json!({"account_id": "acct_999"}),
        },
        "{next:?}"
    );
    // It has nothing to arbitrate with.
    assert!(flow.next(&live, &MOCK, 0.3).is_err());
}

#[test]
fn a_refitted_flow_keeps_its_arbiter_and_learns_its_habit_from_every_session() {
    let three: Vec<Episode> = (0..3).map(session).collect();
    let split = compile_flow_from_episodes(&mock_config(), &three, &manifest(), &MOCK).unwrap();
    let mut config = mock_config();
    config.refit_habit = true;
    let refit = compile_flow_from_episodes(&config, &three, &manifest(), &MOCK).unwrap();
    // The habit learns from all three sessions, as a habit-only flow's does.
    assert_eq!(split.provenance().habit_episodes, 2);
    assert_eq!(refit.provenance().habit_episodes, 3);
    // The arbiter is the one fitted on the held-out session.
    assert!(refit.has_arbiter());
    assert_eq!(
        refit.provenance().arbiter_cases,
        split.provenance().arbiter_cases
    );
    let as_json = |f: &stretto_report::flow::Flow| serde_json::to_value(f).unwrap();
    assert_eq!(as_json(&refit)["folds"], as_json(&split)["folds"]);
    let live = new_customer();
    assert!(refit.next(&live, &MOCK, 0.3).is_ok());
}

#[test]
fn a_habit_learned_from_few_sessions_can_serve_an_arbiter_fitted_elsewhere() {
    let elsewhere = learned_flow();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let three: Vec<Episode> = (0..3).map(session).collect();
    let habit = compile_habit_flow_from_episodes(&config, &three, &manifest()).unwrap();
    // Only a flow with an arbiter can lend one.
    let none = compile_habit_flow_from_episodes(&config, &three, &manifest()).unwrap();
    assert!(habit.clone().with_arbiter_of(none).is_err());
    let flow = habit.with_arbiter_of(elsewhere.clone()).unwrap();
    assert!(flow.has_arbiter());
    assert_eq!(flow.provenance().habit_episodes, 3);
    assert_eq!(
        flow.provenance().arbiter_cases,
        elsewhere.provenance().arbiter_cases
    );
    assert!(flow
        .provenance()
        .sources
        .iter()
        .any(|s| s.starts_with("arbiter: ")));
    let as_json = |f: &stretto_report::flow::Flow| serde_json::to_value(f).unwrap();
    assert_eq!(as_json(&flow)["folds"], as_json(&elsewhere)["folds"]);
    let live = new_customer();
    assert!(flow.next(&live, &MOCK, 0.3).is_ok());
}
