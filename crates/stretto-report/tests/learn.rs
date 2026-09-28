//! Learning a live flow from episodes that are not τ²-bench's, as from
//! sessions `stretto-proxy` recorded.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use stretto_oracle::MockOracle;
use stretto_report::flow::{Decider, Proposal, PER_CALL};
use stretto_report::phase0::{
    compile_flow_from_episodes, compile_habit_flow_from_episodes, Config,
};
use stretto_report::promote::{Recorded, Serving};

/// Served on the habit alone at 0.3, with up to `per_call` lookups a call.
fn habit(per_call: usize) -> Serving {
    Serving {
        decider: Decider::Habit,
        threshold: 0.3,
        per_call,
    }
}
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

mod common;

use common::{manifest, say, session, session_with_rewards, write_sessions};

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
    // Sessions that all failed teach no habit.
    let failed: Vec<Episode> = (0..10)
        .map(|i| Episode {
            reward: 0.0,
            ..session(i)
        })
        .collect();
    let err = compile_flow_from_episodes(&mock_config(), &failed, &manifest(), &MOCK).unwrap_err();
    assert!(
        format!("{err:#}").starts_with("no successful training episodes"),
        "{err:#}"
    );
    // A decision log that cannot be written stops it.
    let mut config = mock_config();
    let dir = std::env::temp_dir().join(format!("stretto-no-dir-{}", std::process::id()));
    config.shadow.as_mut().unwrap().log = Some(dir.join("log.jsonl"));
    let err = compile_flow_from_episodes(&config, &three, &manifest(), &MOCK).unwrap_err();
    assert!(format!("{err:#}").starts_with("writing "), "{err:#}");
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
    // The audit judges it by its habit, and asks nothing.
    let new: Vec<Episode> = (100..105).map(session).collect();
    let a = stretto_report::audit::audit(&flow, &new, &MOCK);
    assert_eq!(a.decider, "habit");
    assert!(a.decisions >= 15, "{}", a.decisions);
    assert_eq!(a.unanswered, 0);
    assert!(a.agreement > 0.8, "{}", a.agreement);
}

#[test]
fn a_learned_flow_counts_what_comes_before_the_next_write() {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let flow = compile_habit_flow_from_episodes(&config, &episodes, &manifest()).unwrap();
    assert!(flow.has_reach());
    let live = new_customer();
    // A lookup the agent makes next is one it makes before its next write,
    // so the reach decider takes it too.
    let habit = flow.next_with(&live, &MOCK, 0.3, Decider::Habit).unwrap();
    let reach = flow.next_with(&live, &MOCK, 0.3, Decider::Reach).unwrap();
    assert_eq!(reach.proposal, habit.proposal, "{reach:?}");
    assert!(reach.prob.unwrap() > 0.99, "{reach:?}");
    // Between its chance and that chance times its arguments', it hands
    // back for the arguments.
    let (p, chance) = (reach.prob.unwrap(), reach.binding.unwrap());
    assert!(chance < 1.0, "{reach:?}");
    let between = flow
        .next_with(&live, &MOCK, p * (1.0 + chance) / 2.0, Decider::Reach)
        .unwrap();
    assert!(
        matches!(&between.proposal, Proposal::HandBack { reason } if reason.contains("for its arguments")),
        "{between:?}"
    );
    // Above every chance, it hands back.
    let high = flow.next_with(&live, &MOCK, 1.01, Decider::Reach).unwrap();
    assert!(
        matches!(high.proposal, Proposal::HandBack { .. }),
        "{high:?}"
    );
    // The counts survive the IR, and a flow without them refuses the decider.
    let back =
        stretto_report::flow::Flow::from_json(&serde_json::to_string(&flow).unwrap()).unwrap();
    assert!(back.has_reach());
    let mut ir: Value = serde_json::to_value(&flow).unwrap();
    ir.as_object_mut().unwrap().remove("reach");
    let old = stretto_report::flow::Flow::from_json(&ir.to_string()).unwrap();
    assert!(!old.has_reach());
    assert!(old.next_with(&live, &MOCK, 0.3, Decider::Reach).is_err());
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

#[test]
fn an_arbiter_ships_on_its_own_and_serves_a_habit_learned_elsewhere() {
    use stretto_report::flow::{Arbiter, Flow};
    // A learned flow's folds share one fit, so its arbiter can ship.
    let elsewhere = learned_flow();
    let arbiter = elsewhere.arbiter().unwrap();
    assert_eq!(arbiter.domain(), "shop");
    assert_eq!(
        arbiter.provenance().arbiter_cases,
        elsewhere.provenance().arbiter_cases
    );
    let text = serde_json::to_string(&arbiter).unwrap();
    let arbiter = Arbiter::from_json(&text).unwrap();
    let mut newer: Value = serde_json::from_str(&text).unwrap();
    newer["stretto_arbiter"] = json!(2);
    assert!(Arbiter::from_json(&newer.to_string()).is_err());

    // Served with a habit from three new sessions, it judges them as the
    // flow it came from does.
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let three: Vec<Episode> = (0..3).map(session).collect();
    let habit = compile_habit_flow_from_episodes(&config, &three, &manifest()).unwrap();
    let flow = habit.with_arbiter(arbiter);
    assert!(flow.has_arbiter());
    assert_eq!(flow.provenance().habit_episodes, 3);
    assert!(flow
        .provenance()
        .sources
        .iter()
        .any(|s| s.starts_with("arbiter (shop): ")));
    let as_json = |f: &Flow| serde_json::to_value(f).unwrap();
    assert_eq!(as_json(&flow)["folds"], as_json(&elsewhere)["folds"]);
    assert!(flow.next(&new_customer(), &MOCK, 0.3).is_ok());

    // Folds fitted apart are no one arbiter, and a habit alone has none.
    let mut apart = as_json(&elsewhere);
    apart["folds"][1]["weights"][0] = json!(9.0);
    let apart = Flow::from_json(&apart.to_string()).unwrap();
    let err = apart.arbiter().unwrap_err();
    assert!(format!("{err:#}").contains("--pooled-arbiter"), "{err:#}");
    let none = compile_habit_flow_from_episodes(&config, &three, &manifest()).unwrap();
    assert!(none.arbiter().is_err());
}

/// An oracle an attacker controls: whatever it is asked, it names tools the
/// flow never offered (a write among them) with all its probability, and
/// says every lookup should go on.
struct Hostile;

impl stretto_oracle::Oracle for Hostile {
    fn ask(&self, request: &stretto_oracle::Request) -> anyhow::Result<stretto_oracle::Response> {
        use stretto_oracle::{Answer, Response};
        let pushed = BTreeMap::from([
            ("close_order".to_string(), 0.6),
            ("delete_account".to_string(), 0.4),
        ]);
        let answers = request
            .questions
            .iter()
            .map(|(id, q)| {
                let answer = match q {
                    stretto_oracle::Question::Choice { .. } => Answer::Choice {
                        choice: "close_order".to_string(),
                        probabilities: pushed.clone(),
                        confidence: 1.0,
                    },
                    _ => Answer::Noul { noul: 1.0 },
                };
                (id.clone(), answer)
            })
            .collect();
        Ok(Response {
            model: "hostile".to_string(),
            answers,
            usage: Default::default(),
        })
    }
}

#[test]
fn a_flow_never_proposes_a_tool_outside_its_compiled_set() {
    // RFC-001 §3.8: whatever the System-One model says, a flow chooses among
    // the lookups compiled for the site, and binds their arguments itself.
    let flow = learned_flow();
    let offered = |site: &str| -> Vec<&str> {
        match site {
            "find_account" => vec!["get_account"],
            "get_account" | "get_order" => vec!["get_order"],
            _ => vec![],
        }
    };
    let mut decided = 0;
    for i in [999, 1000, 1001] {
        let full = session(i);
        for end in 1..=full.events.len() {
            let mut prefix = full.clone();
            prefix.events.truncate(end);
            if !matches!(prefix.events.last(), Some(Event::ToolResult { .. })) {
                continue;
            }
            // At threshold 0 any lookup the flow can make, it makes.
            let Ok(next) = flow.next(&prefix, &Hostile, 0.0) else {
                continue;
            };
            decided += 1;
            if let Proposal::Lookup { tool, .. } = &next.proposal {
                let site = next.site.clone().unwrap_or_default();
                assert!(
                    offered(&site).contains(&tool.as_str()),
                    "{tool} proposed after {site}: {next:?}"
                );
                assert_eq!(manifest().tools[tool], ToolKind::Read);
            }
            // The hostile options are never among the flow's own.
            assert!(!next.probs.contains_key("close_order"), "{next:?}");
            assert!(!next.probs.contains_key("delete_account"), "{next:?}");
        }
    }
    assert!(decided >= 9, "{decided}");
}

// Reviewing flows as code (`stretto flow-show`, `stretto flow-diff`).

fn habit_flow(episodes: &[Episode], manifest: &ToolManifest) -> stretto_report::flow::Flow {
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    compile_habit_flow_from_episodes(&config, episodes, manifest).unwrap()
}

#[test]
fn a_flow_shows_what_it_may_call_and_where_its_arguments_come_from() {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let md = stretto_report::review::show(&habit_flow(&episodes, &manifest()), 0.3);
    for line in [
        "- **Read, the only tools it may call:** `find_account`, `get_account`, `get_order`",
        "- **Write, never called:** `close_order`",
        // After finding the account the agent always read it, and the flow
        // binds the account's id from what the search returned.
        "| `find_account` | `get_account` (60) | get_account 100% (of 60) | looks up `get_account` 1.00 × 0.98 = 0.98 |",
        "| `get_account` | `account_id` | `find_account` at `$` (60 of 60) | 0.98 (60/60), 0.50 (0/0) |",
        // After an order, the agent read the next one half the time.
        "| `get_order` | `get_order` (60) | get_order 50%, respond 50% (of 120) | looks up `get_order` 0.50 × 0.99 = 0.50 |",
        "| `get_order` | `order_id` | `get_account` at `$.orders[*]` (120 of 120) |",
        // Only the customer knows their email.
        "| `find_account` | `email` | nothing: the flow never makes this lookup | — |",
        // It holds reach's counts, so it is served, and shown, with them.
        "It has no arbiter and asks no one: it decides by how often each lookup came before the agent's next write (`reach`).",
        "what the flow does there with `reach` at a threshold of 0.3, which is the likeliest lookup's share of the times the agent made it before its next write, times the chance",
        "| After | Lookups it may make next (times seen) | What the agent did next in training | With `reach` |",
    ] {
        assert!(md.contains(line), "missing {line:?} in\n{md}");
    }
    // Served at a higher threshold, it hands back after an order.
    let strict = stretto_report::review::show(&habit_flow(&episodes, &manifest()), 0.6);
    assert!(
        strict.contains("hands back: `get_order` 0.50 × 0.99 = 0.50"),
        "{strict}"
    );
    // A flow with an arbiter shows its weights and the model's record.
    let md = stretto_report::review::show(&learned_flow(), 0.3);
    for line in [
        "asks `jev-latest`",
        "One fit, which judges every session.",
        "| ln the habit's probability |",
        "| the model's record at the site, on its pick |",
        "The System-One model's record, at the held-out decisions of 3 sites",
    ] {
        assert!(md.contains(line), "missing {line:?} in\n{md}");
    }
}

#[test]
fn a_diff_flags_a_new_read_tool_its_lookups_and_its_binding() {
    let old = habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest());
    let mut with_rewards = manifest();
    with_rewards
        .tools
        .insert("get_rewards".to_string(), ToolKind::Read);
    let new = habit_flow(
        &(0..60).map(session_with_rewards).collect::<Vec<_>>(),
        &with_rewards,
    );
    let d = stretto_report::review::diff(&old, &new, 0.05, 0.3);
    assert_eq!(
        d.needs_review,
        [
            "`get_rewards` is now marked read-only, so the flow may call it",
            "a new lookup: `get_rewards` after `get_account`",
            "a new lookup: `get_order` after `get_rewards`",
            "a new binding: `get_rewards`'s `account_id` from `get_account` at `$.account_id`",
        ]
    );
    for line in [
        "- `get_rewards`: absent → read",
        "- after `get_account`: no longer looks up `get_order`",
        "- after `get_account`: looks up `get_rewards` 1.00 × 0.98 = 0.98 (was: looks up `get_order` 1.00 × 0.99 = 0.99)",
        "- after `get_account`: get_rewards 0% → 100%",
    ] {
        assert!(d.markdown.contains(line), "missing {line:?} in\n{}", d.markdown);
    }
}

#[test]
fn more_sessions_of_the_same_kind_need_no_review() {
    let fewer = habit_flow(&(0..30).map(session).collect::<Vec<_>>(), &manifest());
    let more = habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest());
    let d = stretto_report::review::diff(&fewer, &more, 0.05, 0.3);
    assert!(d.needs_review.is_empty(), "{:?}", d.needs_review);
    assert!(
        d.markdown.contains("Nothing needs review"),
        "{}",
        d.markdown
    );
    assert!(
        d.markdown
            .contains("the habit learned from 30 → 60 successful sessions or episodes"),
        "{}",
        d.markdown
    );
    assert!(stretto_report::review::diff(&more, &more, 0.05, 0.3)
        .needs_review
        .is_empty());
}

#[test]
fn a_diff_flags_a_flow_that_starts_asking_a_system_one_model() {
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let habit = habit_flow(&episodes, &manifest());
    let asking = habit.clone().with_arbiter_of(learned_flow()).unwrap();
    let d = stretto_report::review::diff(&habit, &asking, 0.05, 0.3);
    assert_eq!(
        d.needs_review,
        ["the flow now asks `jev-latest` at each decision"]
    );
    // Dropping the arbiter only takes a question away.
    assert!(stretto_report::review::diff(&asking, &habit, 0.05, 0.3)
        .needs_review
        .is_empty());
    // A predicate asked with each question is sent to the model too, so a
    // new or reworded one needs review.
    let mut v = serde_json::to_value(&asking).unwrap();
    v["predicates"] = json!([{
        "id": "must_ask",
        "favors": "hand_back",
        "question": "Does the agent need something only the customer can give?",
        "yes": "It must ask first.",
        "no": "It can look something up."
    }]);
    let asking_more = stretto_report::flow::Flow::from_json(&v.to_string()).unwrap();
    assert_eq!(
        stretto_report::review::diff(&asking, &asking_more, 0.05, 0.3).needs_review,
        ["the flow asks new or reworded predicates: `must_ask`"]
    );
    assert!(stretto_report::review::show(&asking_more, 0.3).contains(
        "- `must_ask` (asked, not weighed): Does the agent need something only the customer can give?"
    ));
    assert!(
        stretto_report::review::diff(&asking_more, &asking, 0.05, 0.3)
            .markdown
            .contains("- no longer asks `must_ask`")
    );
}

#[test]
fn flow_diff_exits_with_1_when_a_change_needs_review() {
    let dir = std::env::temp_dir().join(format!("stretto-review-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (old, new) = (dir.join("old.flow.json"), dir.join("new.flow.json"));
    habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest())
        .save(&old)
        .unwrap();
    let mut with_rewards = manifest();
    with_rewards
        .tools
        .insert("get_rewards".to_string(), ToolKind::Read);
    habit_flow(
        &(0..60).map(session_with_rewards).collect::<Vec<_>>(),
        &with_rewards,
    )
    .save(&new)
    .unwrap();
    let stretto = env!("CARGO_BIN_EXE_stretto");
    let run = |args: &[&std::path::Path]| {
        std::process::Command::new(stretto)
            .arg("flow-diff")
            .args(args)
            .output()
            .unwrap()
    };
    let changed = run(&[&old, &new]);
    assert_eq!(changed.status.code(), Some(1), "{changed:?}");
    assert!(String::from_utf8_lossy(&changed.stdout).contains("**Needs review:**"));
    assert_eq!(run(&[&old, &old]).status.code(), Some(0));
    assert_eq!(
        run(&[&old, &dir.join("missing.json")]).status.code(),
        Some(2)
    );
    let shown = std::process::Command::new(stretto)
        .arg("flow-show")
        .arg(&new)
        .output()
        .unwrap();
    assert!(shown.status.success(), "{shown:?}");
    assert!(String::from_utf8_lossy(&shown.stdout).starts_with("# Flow: shop\n"));
    std::fs::remove_dir_all(&dir).ok();
}

// Promoting a flow's sites (`stretto promote`).

/// Customer `i`'s session with both orders read in one LLM turn, the
/// second answered first.
fn batched(i: usize) -> Episode {
    let mut ep = session(i);
    let calls: Vec<ToolCall> = [&ep.events[5], &ep.events[7]]
        .into_iter()
        .flat_map(|e| match e {
            Event::Assistant { calls, .. } => calls.clone(),
            _ => Vec::new(),
        })
        .collect();
    let (first, second) = (ep.events[6].clone(), ep.events[8].clone());
    let turn = Event::Assistant {
        text: None,
        calls,
        usage: None,
    };
    ep.events.splice(5..=8, [turn, second, first]);
    ep
}

#[test]
fn a_lookup_of_a_call_the_agent_asked_for_in_the_same_turn_is_not_scored() {
    let flow = habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest());
    let sessions: Vec<Recorded> = (100..110).map(|i| batched(i).into()).collect();
    let scored = stretto_report::promote::score(&flow, &sessions, &MOCK, habit(PER_CALL), None);
    // After the account, the agent's read of it or the flow's own after
    // finding it, the flow reads the first order, which the agent asks for
    // in its next turn: used.
    let account = &scored.sites["get_account"];
    assert_eq!((account.lookups, account.used), (20, 20));
    // Each chain goes on to the second order, which spares the rest of that
    // turn: used. When either of the agent's own reads returns, the other is
    // on its way, so the flow proposes nothing: it would spare no call.
    let order = &scored.sites["get_order"];
    assert_eq!(
        (order.decisions, order.lookups, order.used),
        (60, 20, 20),
        "{order:?}"
    );
    // Allowed one lookup per call, it makes no chain.
    let one = stretto_report::promote::score(&flow, &sessions, &MOCK, habit(1), None);
    assert_eq!(one.sites["get_order"].lookups, 0);
    // A host that streams a turn's calls sends the second order only after
    // the first returned: the proxy could not know of it, and a lookup of
    // it would spare nothing.
    let streamed: Vec<Recorded> = (100..110)
        .map(|i| {
            let mut ep = batched(i);
            ep.events.swap(6, 7);
            let mut r = Recorded::from(ep);
            r.sent_after.insert("4".to_string(), 1);
            r
        })
        .collect();
    let scored = stretto_report::promote::score(&flow, &streamed, &MOCK, habit(PER_CALL), None);
    let order = &scored.sites["get_order"];
    assert_eq!((order.lookups, order.used), (30, 20), "{order:?}");
    // A result whose call no turn holds is no decision.
    let mut orphan = session(200);
    orphan.events.remove(3);
    let scored =
        stretto_report::promote::score(&flow, &[orphan.into()], &MOCK, habit(PER_CALL), None);
    assert!(!scored.sites.contains_key("get_account"), "{scored:?}");
}

#[test]
fn a_chain_counts_only_where_the_proxy_would_make_it() {
    let flow = habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest());
    // Ten sessions where the agent reads the account and both orders after
    // finding it, and ten where it finds the account and answers at once.
    let mut sessions: Vec<Recorded> = (100..110).map(|i| batched(i).into()).collect();
    sessions.extend((110..120).map(|i| {
        let mut ep = session(i);
        ep.events.truncate(3);
        ep.events.push(say("Found it."));
        Recorded::from(ep)
    }));
    // Scored with every chain, the account read after finding it counts
    // at the account's site too.
    let all = stretto_report::promote::score(&flow, &sessions, &MOCK, habit(PER_CALL), None);
    assert_eq!(all.sites["get_account"].lookups, 20);
    // But finding the account is used only half the time, so it is not
    // promoted, and the proxy would never read the account after it.
    let bar = stretto_report::flow::Bar {
        threshold: 0.3,
        min_used: 0.7,
        min_lower: 0.5,
        min_tasks: 3,
    };
    let (scored, promotion) =
        stretto_report::promote::promote_served(&flow, &sessions, &MOCK, habit(PER_CALL), bar);
    assert!(!promotion.sites["find_account"].promoted);
    assert_eq!(scored.sites["get_account"].lookups, 10);
    assert!(promotion.sites["get_account"].promoted && promotion.sites["get_order"].promoted);
}

#[test]
fn a_promoted_flow_acts_only_where_its_lookups_were_the_agents_own() {
    let flow = habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest());
    // New sessions where the agent, having read the account, answers
    // without reading any order.
    let brief: Vec<Episode> = (100..110)
        .map(|i| {
            let mut ep = session(i);
            ep.events.truncate(5);
            ep.events.push(say("You have two orders."));
            ep
        })
        .collect();
    let brief: Vec<Recorded> = brief.into_iter().map(Recorded::from).collect();
    let scored = stretto_report::promote::score(&flow, &brief, &MOCK, habit(PER_CALL), None);
    let bar = stretto_report::flow::Bar {
        threshold: 0.3,
        min_used: 0.5,
        min_lower: 0.3,
        min_tasks: 3,
    };
    let promotion = stretto_report::promote::promote(&scored, bar);
    let found = &promotion.sites["find_account"];
    assert_eq!((found.lookups, found.used, found.tasks), (10, 10, 10));
    assert!(found.promoted);
    // Its lookups there are detours, whether after the agent's own read of
    // the account or after its own, which the agent then made: 10 each.
    let account = &promotion.sites["get_account"];
    assert_eq!((account.lookups, account.used), (20, 0));
    assert!(!account.promoted);

    // Served, it still reads the account, but no longer the orders.
    let promoted = flow.clone().with_promotion(Some(promotion));
    let next = promoted
        .next_with(&new_customer(), &MOCK, 0.3, Decider::Habit)
        .unwrap();
    assert!(
        matches!(&next.proposal, Proposal::Lookup { tool, .. } if tool == "get_account"),
        "{next:?}"
    );
    let mut later = session(999);
    later.events.truncate(5);
    let next = promoted
        .next_with(&later, &MOCK, 0.3, Decider::Habit)
        .unwrap();
    assert_eq!(
        next.proposal,
        Proposal::HandBack {
            reason: "the site is not promoted".to_string()
        }
    );
    let unpromoted = flow.next_with(&later, &MOCK, 0.3, Decider::Habit).unwrap();
    assert!(matches!(unpromoted.proposal, Proposal::Lookup { .. }));

    // The promotion is part of the IR, and review shows it.
    let back =
        stretto_report::flow::Flow::from_json(&serde_json::to_string(&promoted).unwrap()).unwrap();
    assert_eq!(back.promotion(), promoted.promotion());
    let md = stretto_report::review::show(&promoted, 0.3);
    assert!(
        md.contains("| `get_account` | `get_order` (60) | get_order 100% (of 60) | hands back: not promoted |"),
        "{md}"
    );
    assert!(
        md.contains("| `get_account` | 20 | 20 | 0 (0%) | 0.00 | 10 | no |"),
        "{md}"
    );
    // Lifting the promotion lets the flow act where it handed back: after
    // the account, and after an order, a site the new sessions never
    // reached, so never scored.
    assert!(!promoted
        .promotion()
        .unwrap()
        .sites
        .contains_key("get_order"));
    let d = stretto_report::review::diff(&promoted, &flow, 0.05, 0.3);
    assert_eq!(
        d.needs_review,
        [
            "the flow now acts after `get_account`, where it handed back",
            "the flow now acts after `get_order`, where it handed back"
        ]
    );
    assert!(stretto_report::review::diff(&flow, &promoted, 0.05, 0.3)
        .needs_review
        .is_empty());
}

#[test]
fn a_flow_is_served_with_its_arbiter_else_reach_else_its_habit() {
    // What the proxy and `stretto promote` decide with when not told.
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let habit = habit_flow(&episodes, &manifest());
    assert!(habit.has_reach());
    assert_eq!(habit.served_decider(), Decider::Reach);
    // The audit scores the agent's next step, which the habit predicts.
    assert_eq!(habit.default_decider(), Decider::Habit);
    assert_eq!(learned_flow().served_decider(), Decider::Arbiter);
    // A flow learned before flows held reach's counts.
    let older = stretto_report::flow::Flow::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/examples/retail-5-sessions.flow.json"),
    )
    .unwrap();
    assert!(!older.has_reach());
    assert_eq!(older.served_decider(), Decider::Habit);
}

// Drift alarms (`stretto drift`) and forgetting (`learn --half-life`).

#[test]
fn drift_sounds_when_another_agent_takes_over_and_a_half_life_follows_it() {
    let dir = std::env::temp_dir().join(format!("stretto-drift-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let flow = dir.join("shop.flow.json");
    habit_flow(&(0..60).map(session).collect::<Vec<_>>(), &manifest())
        .save(&flow)
        .unwrap();
    // Agent A, then agent B, who also reads the customer's rewards.
    let a: Vec<Episode> = (100..130).map(session).collect();
    let b: Vec<Episode> = (0..8).map(session_with_rewards).collect();
    write_sessions(&dir.join("a"), &a);
    write_sessions(&dir.join("b"), &b);
    write_sessions(&dir.join("ab"), &[a, b].concat());
    let stretto = env!("CARGO_BIN_EXE_stretto");
    let drift = |flow: &std::path::Path, sessions: &str, extra: &[&str]| {
        std::process::Command::new(stretto)
            .arg("drift")
            .arg("--flow")
            .arg(flow)
            .arg("--sessions")
            .arg(dir.join(sessions))
            .args(extra)
            .output()
            .unwrap()
    };
    // One agent throughout: no alarm.
    let quiet = drift(&flow, "a", &[]);
    assert_eq!(quiet.status.code(), Some(0), "{quiet:?}");
    assert!(String::from_utf8_lossy(&quiet.stdout).contains("**No alarm now:**"));
    // B takes over: the alarm sounds at the splice, and the report names
    // B's new tool.
    let json = dir.join("drift.json");
    let out = dir.join("drift.md");
    let loud = drift(
        &flow,
        "ab",
        &[
            "--json",
            json.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(loud.status.code(), Some(1), "{loud:?}");
    let md = std::fs::read_to_string(&out).unwrap();
    assert!(
        md.contains("**Alarm: the agent changed at session 31"),
        "{md}"
    );
    assert!(md.contains("| `get_rewards` | 31 "), "{md}");
    let report: Value = serde_json::from_slice(&std::fs::read(&json).unwrap()).unwrap();
    assert_eq!(report["alarms"][0]["change"], 30);
    // What the flow cannot decide by, and settings the alarm cannot use.
    let no_arbiter = drift(&flow, "a", &["--decider", "arbiter"]);
    assert_eq!(no_arbiter.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&no_arbiter.stderr).contains("no arbiter"));
    let mut old: Value = serde_json::from_slice(&std::fs::read(&flow).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("reach");
    let old_flow = dir.join("old.flow.json");
    std::fs::write(&old_flow, old.to_string()).unwrap();
    let no_reach = drift(&old_flow, "a", &["--decider", "reach"]);
    assert!(String::from_utf8_lossy(&no_reach.stderr).contains("no counts"));
    for bad in [["--window", "2"], ["--threshold", "0"], ["--hazard", "1.5"]] {
        assert_eq!(drift(&flow, "a", &bad).status.code(), Some(2), "{bad:?}");
    }
    // Relearned with recent sessions weighed more, the flow fits B better.
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest()).unwrap()).unwrap();
    let learn = |extra: &[&str], out: &std::path::Path| {
        std::process::Command::new(stretto)
            .args(["learn", "--domain", "shop", "--habit-only", "--manifest"])
            .arg(&manifest_path)
            .arg("--sessions")
            .arg(dir.join("ab"))
            .args(extra)
            .arg("--out")
            .arg(out)
            .output()
            .unwrap()
    };
    let (all, recent) = (dir.join("all.flow.json"), dir.join("recent.flow.json"));
    assert!(learn(&[], &all).status.success());
    assert!(learn(&["--half-life", "4"], &recent).status.success());
    let surprise = |flow: &std::path::Path| {
        let json = dir.join("b.json");
        drift(flow, "b", &["--json", json.to_str().unwrap()]);
        let d: Value = serde_json::from_slice(&std::fs::read(&json).unwrap()).unwrap();
        d["sessions"][0]["surprise"].as_f64().unwrap()
    };
    assert!(surprise(&recent) < surprise(&all));
    assert_eq!(learn(&["--half-life", "0"], &recent).status.code(), Some(2));
    std::fs::remove_dir_all(&dir).ok();
}

/// An oracle that answers no question.
struct Mute;

impl stretto_oracle::Oracle for Mute {
    fn ask(&self, _: &stretto_oracle::Request) -> anyhow::Result<stretto_oracle::Response> {
        Ok(stretto_oracle::Response {
            model: "mute".to_string(),
            answers: BTreeMap::new(),
            usage: Default::default(),
        })
    }
}

#[test]
fn a_flow_hands_back_where_it_cannot_or_may_not_decide() {
    let flow = learned_flow();
    let reason = |next: stretto_report::flow::Next| match next.proposal {
        Proposal::HandBack { reason } => reason,
        other => panic!("{other:?}"),
    };
    // After the agent replies there is no call to decide after.
    let mut spoken = new_customer();
    spoken.events.push(say("One moment."));
    let next = flow.next(&spoken, &MOCK, 0.3).unwrap();
    assert_eq!(reason(next), "the last step is not a tool call");
    // A site a search switched off, and one it set a threshold for.
    let site = |t: f64| BTreeMap::from([("find_account".to_string(), t)]);
    let off = flow.clone().with_thresholds(site(2.0));
    let next = off.next(&new_customer(), &MOCK, 0.3).unwrap();
    assert_eq!(reason(next), "the site is switched off");
    let low = flow.clone().with_thresholds(site(0.01));
    let next = low.next(&new_customer(), &MOCK, 0.9).unwrap();
    assert!(matches!(next.proposal, Proposal::Lookup { .. }), "{next:?}");
    // An answer without the question's answers.
    let next = flow.next(&new_customer(), &Mute, 0.3).unwrap();
    assert_eq!(reason(next), "the System-One answer was incomplete");
}

/// What an audit cannot score it leaves out: the first of two results in a
/// row (the calls of one turn), a question the oracle left unanswered, and
/// every decision of a flow asked to decide as it cannot. A read the flow
/// does not offer is the agent's step all the same, as `other`.
#[test]
fn an_audit_leaves_out_what_it_cannot_score() {
    use stretto_report::audit::{audit, audit_with, decisions, OTHER};
    let flow = learned_flow();
    // After finding the account the agent reads an order, which the flow
    // does not offer there.
    let mut odd = session(300);
    odd.events.drain(3..5);
    let (ds, _) = decisions(&flow, &odd, &MOCK);
    assert!(ds.iter().any(|d| d.options[d.actual] == OTHER), "{ds:?}");
    // The flow decides after the last of two results in a row.
    let mut both = session(301);
    both.events.insert(3, both.events[2].clone());
    let (ds, _) = decisions(&flow, &both, &MOCK);
    assert_eq!(ds, decisions(&flow, &session(301), &MOCK).0);
    let mute = audit(&flow, &[session(302)], &Mute);
    assert!(mute.unanswered > 0 && mute.decisions == 0, "{mute:?}");
    // A flow without an arbiter cannot decide with one, but it can by reach.
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let habit = compile_habit_flow_from_episodes(&config, &episodes, &manifest()).unwrap();
    let new = [session(303)];
    let a = audit_with(&habit, &new, &MOCK, Decider::Arbiter);
    assert!(a.unanswered > 0 && a.decisions == 0, "{a:?}");
    let a = audit_with(&habit, &new, &MOCK, Decider::Reach);
    assert_eq!(a.decider, "reach");
    assert!(a.decisions > 0, "{a:?}");
}

/// What scoring for promotion cannot decide it leaves out, as the proxy
/// would: a question the oracle left unanswered, every decision of a flow
/// asked to decide as it cannot, and a result that came after a reply.
#[test]
fn promotion_leaves_out_what_the_flow_cannot_decide() {
    use stretto_report::promote::score;
    let flow = learned_flow();
    let arbiter = Serving {
        decider: Decider::Arbiter,
        threshold: 0.3,
        per_call: PER_CALL,
    };
    let sessions: Vec<Recorded> = (400..403).map(|i| session(i).into()).collect();
    assert!(score(&flow, &sessions, &Mute, arbiter, None).unanswered > 0);
    let episodes: Vec<Episode> = (0..60).map(session).collect();
    let mut config = Config::new(PathBuf::new());
    config.alpha_samples = 0;
    let habit_flow = compile_habit_flow_from_episodes(&config, &episodes, &manifest()).unwrap();
    let s = score(&habit_flow, &sessions, &MOCK, arbiter, None);
    // Only where no lookup followed in training is there nothing to ask.
    assert!(
        s.unanswered > 0 && s.sites.keys().eq(["close_order"]),
        "{s:?}"
    );
    // The first result comes after the agent's reply: nothing to decide.
    let decisions = |s: stretto_report::promote::Scored| -> usize {
        s.sites.values().map(|t| t.decisions).sum()
    };
    let full = score(&flow, &[session(403).into()], &MOCK, habit(PER_CALL), None);
    let mut replied = session(403);
    replied.events.insert(2, say("One moment."));
    let s = score(&flow, &[replied.into()], &MOCK, habit(PER_CALL), None);
    assert!(decisions(s) < decisions(full));
}
