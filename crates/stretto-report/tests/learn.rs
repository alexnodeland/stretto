//! Learning a live flow from episodes that are not τ²-bench's, as from
//! sessions `stretto-proxy` recorded.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use stretto_oracle::MockOracle;
use stretto_report::flow::Proposal;
use stretto_report::phase0::{compile_flow_from_episodes, Config};
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

pub fn learned_flow() -> stretto_report::flow::Flow {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let mut sc = ShadowConfig::new(OracleKind::Mock);
    sc.questions = QuestionSet::V2;
    config.shadow = Some(sc);
    let oracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };
    compile_flow_from_episodes(&config, &episodes, &manifest(), &oracle).unwrap()
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
