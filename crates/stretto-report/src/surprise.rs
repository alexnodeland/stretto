//! Handing back when a session surprises the flow (RFC-001 §3.6).
//!
//! A flow's habit says what the agent did next after each call in the
//! sessions it learned from. A session unlike those shows up as steps the
//! habit thought unlikely. A flow with a [`SurpriseGate`] scores each of the
//! agent's own steps where the flow decides by its surprise, −ln p under
//! the habit ([`steps`]). Once the mean over any `window` of those steps in
//! a row is above the gate's threshold, the flow hands back for the rest of
//! the session, however likely its lookups ([`tripped`]): the session has
//! left the traces the flow learned from. The flow's own lookups (the
//! proxy's `stretto-N` calls) are the flow's steps, not the agent's, and are
//! not scored.
//!
//! `stretto learn --surprise Q` sets the threshold from the training
//! sessions ([`learn`]). Each successful session is scored by a habit
//! learned without its task, and the threshold is the Q-quantile of each
//! one's most surprising window, so that about 1 − Q of such sessions would
//! trip it. `--surprise off|T` in `serve` and `flow-serve`, and
//! `--flow-surprise` in the proxy, override it ([`Override`]).
//!
//! The surprise is the habit's, whatever decides the flow's lookups: the
//! gate asks the System-One model nothing, and it is the same whether the
//! flow serves with the arbiter, the habit or reach.

use crate::flow::Flow;
use crate::phase0::{compile_habit_flow_from_episodes, task_group, Config};
use crate::shadow::RESPOND;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use stretto_model::features::FOLDS;
use stretto_trace::mcp::is_flow_lookup;
use stretto_trace::{Episode, Event, ToolManifest};

/// Probabilities below this are raised to it: a step the habit thought
/// impossible costs about 13.8 nats rather than infinity.
const FLOOR: f64 = 1e-6;

/// The steps `learn --surprise` averages over unless `--surprise-window`
/// says otherwise, and those a threshold set by hand averages over when
/// the flow has no gate.
pub const DEFAULT_WINDOW: usize = 5;

/// How the reason of a hand-back for surprise begins.
pub const SURPRISED: &str = "the session surprised the flow";

/// When a flow hands back for the rest of a session that surprises it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SurpriseGate {
    /// How many of the agent's steps in a row the mean is over.
    pub window: usize,
    /// The mean surprise, in nats, above which the flow hands back.
    pub threshold: f64,
    /// The quantile of held-out training sessions' most surprising windows
    /// the threshold was learned at; absent when it was set by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantile: Option<f64>,
}

impl SurpriseGate {
    /// What the gate does, in a sentence's words.
    pub fn describe(&self) -> String {
        let learned = match self.quantile {
            Some(q) => format!(
                ", the {} quantile of held-out training sessions' most surprising windows",
                q
            ),
            None => String::new(),
        };
        format!(
            "hands back for the rest of a session once {} of the agent's steps in a row average more than {:.2} nats of surprise{learned}",
            self.window, self.threshold
        )
    }
}

/// The surprise of each of the agent's own steps in `episode` where `flow`
/// decides, in order: −ln of the habit's probability of the step at the
/// decision just before it, over the options the flow has there. A step the flow does not
/// offer there (a reply, a write, another tool) counts as handing back, as
/// the audit counts it for the habit. Where the flow's own lookup came
/// next, the agent took no step.
pub fn steps(flow: &Flow, episode: &Episode) -> Vec<f64> {
    let events = &episode.events;
    let mut prefix = Episode {
        events: Vec::with_capacity(events.len()),
        ..episode.clone()
    };
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        prefix.events.push(e.clone());
        // A decision point, as the audit's: a tool result with every call
        // of its turn answered, and a step after it.
        let (Event::ToolResult { .. }, Some(after)) = (e, events.get(i + 1)) else {
            continue;
        };
        let step = match after {
            Event::ToolResult { .. } => continue,
            Event::Assistant { calls, .. } if !calls.is_empty() => {
                if is_flow_lookup(&calls[0].id) {
                    continue;
                }
                calls[0].name.as_str()
            }
            // A reply to the customer, or the customer speaking first.
            _ => RESPOND,
        };
        let Some((_, probs)) = flow.habit_at(&prefix) else {
            continue;
        };
        let p = probs.get(step).or_else(|| probs.get(RESPOND)).copied();
        out.push(-p.unwrap_or(0.0).max(FLOOR).ln());
    }
    out
}

/// The mean of each `window` of `steps` in a row, in order.
pub fn window_means(steps: &[f64], window: usize) -> impl Iterator<Item = f64> + '_ {
    steps
        .windows(window.max(1))
        .map(|w| w.iter().sum::<f64>() / w.len() as f64)
}

/// The mean of the most surprising `window` steps in a row; `None` if there
/// are fewer.
pub fn peak(steps: &[f64], window: usize) -> Option<f64> {
    window_means(steps, window).reduce(f64::max)
}

/// The first window's mean above `gate`'s threshold in `episode`, if one
/// is: the flow then hands back for the rest of the session.
pub fn tripped(flow: &Flow, episode: &Episode, gate: &SurpriseGate) -> Option<f64> {
    window_means(&steps(flow, episode), gate.window).find(|&m| m > gate.threshold)
}

/// The gate `learn --surprise quantile` stores, averaging over `window`
/// steps. Its threshold is the `quantile` of the most surprising window in
/// each successful session of `episodes` with `window` of the agent's steps
/// to score. Each session is scored by a habit learned, as `config` says,
/// without the sessions of its fold of tasks, the folds the arbiter is
/// cross-fitted on: a habit that learned from a session would find it too
/// little surprising.
pub fn learn(
    config: &Config,
    episodes: &[Episode],
    manifest: &ToolManifest,
    quantile: f64,
    window: usize,
) -> Result<SurpriseGate> {
    let fold_of = |e: &Episode| {
        let task = if e.task_id.is_empty() {
            &e.id
        } else {
            &e.task_id
        };
        task_group(task) % FOLDS
    };
    let mut peaks = Vec::new();
    for fold in 0..FOLDS {
        let (held, rest): (Vec<&Episode>, Vec<&Episode>) =
            episodes.iter().partition(|e| fold_of(e) == fold);
        if !held.iter().any(|e| e.succeeded()) || !rest.iter().any(|e| e.succeeded()) {
            continue;
        }
        let rest: Vec<Episode> = rest.into_iter().cloned().collect();
        let flow = compile_habit_flow_from_episodes(config, &rest, manifest)?;
        peaks.extend(
            held.iter()
                .filter(|e| e.succeeded())
                .filter_map(|e| peak(&steps(&flow, e), window)),
        );
    }
    if peaks.is_empty() {
        bail!(
            "no held-out training session had {window} of the agent's steps to score: \
             the surprise gate needs successful sessions of several tasks, and fewer steps (--surprise-window)"
        );
    }
    peaks.sort_by(f64::total_cmp);
    let rank = (quantile * peaks.len() as f64).ceil() as usize;
    Ok(SurpriseGate {
        window,
        threshold: peaks[rank.clamp(1, peaks.len()) - 1],
        quantile: Some(quantile),
    })
}

/// `gate`, for a flow learned again from `episodes` (`stretto stage`):
/// learned again at its quantile, over as many steps, or as it is, if it
/// was set by hand.
pub fn again(
    gate: Option<&SurpriseGate>,
    config: &Config,
    episodes: &[Episode],
    manifest: &ToolManifest,
) -> Result<Option<SurpriseGate>> {
    match gate {
        Some(&SurpriseGate {
            window,
            quantile: Some(q),
            ..
        }) => learn(config, episodes, manifest, q, window).map(Some),
        _ => Ok(gate.copied()),
    }
}

/// `--surprise` in `serve` and `flow-serve`, and `--flow-surprise` in the
/// proxy: `off`, or a threshold in nats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Override {
    /// Serve the flow with no surprise gate, whatever it stores.
    Off,
    /// Serve it with this threshold, over the gate's window, or
    /// [`DEFAULT_WINDOW`] steps if it has none.
    Threshold(f64),
}

impl FromStr for Override {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<f64>() {
            _ if s == "off" => Ok(Self::Off),
            Ok(t) if t.is_finite() && t > 0.0 => Ok(Self::Threshold(t)),
            _ => Err(format!("`off`, or a threshold in nats above 0, not `{s}`")),
        }
    }
}

/// As [`Override::from_str`] reads it: `off`, or the threshold.
impl std::fmt::Display for Override {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::Threshold(t) => write!(f, "{t}"),
        }
    }
}

impl Flow {
    /// The flow, served with its surprise gate as `over` says; as it is,
    /// with `None`.
    pub fn with_surprise_override(self, over: Option<Override>) -> Self {
        match over {
            None => self,
            Some(Override::Off) => self.with_surprise(None),
            Some(Override::Threshold(threshold)) => {
                let window = self.surprise().map_or(DEFAULT_WINDOW, |g| g.window);
                self.with_surprise(Some(SurpriseGate {
                    window,
                    threshold,
                    quantile: None,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::decisions_with;
    use crate::flow::{Decider, FLOW_THRESHOLDS_VERSION, FLOW_VERSION};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use stretto_oracle::MockOracle;
    use stretto_trace::{ToolCall, ToolKind};

    const MOCK: MockOracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };

    fn manifest() -> ToolManifest {
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

    /// One LLM turn of `calls`, each `(id, tool, arguments)`, then their
    /// results.
    fn turn(calls: &[(&str, &str, Value)]) -> Vec<Event> {
        let mut events = vec![Event::Assistant {
            text: None,
            calls: calls
                .iter()
                .map(|(id, name, arguments)| ToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    arguments: arguments.clone(),
                })
                .collect(),
            usage: None,
        }];
        events.extend(calls.iter().map(|(id, name, _)| Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: json!({"id": format!("{name}-{id}")}).to_string(),
        }));
        events
    }

    fn call(id: &str, name: &str, arguments: Value) -> Vec<Event> {
        turn(&[(id, name, arguments)])
    }

    fn say(text: &str) -> Event {
        Event::Assistant {
            text: Some(text.to_string()),
            calls: Vec::new(),
            usage: None,
        }
    }

    /// Customer `i` of the agent the flow learned from: it finds their
    /// account, reads it and each of their one to three orders, and closes
    /// one once they agree.
    fn usual(i: usize) -> Episode {
        let mut events = vec![Event::User {
            text: format!("I'm c{i}@example.com; close an order, please."),
        }];
        events.extend(call("1", "find_account", json!({"email": i})));
        events.extend(call("2", "get_account", json!({"account": i})));
        for k in 0..1 + i % 3 {
            events.extend(call(&format!("o{k}"), "get_order", json!({"order": k})));
        }
        events.push(say("Close the first?"));
        events.push(Event::User {
            text: "Yes.".to_string(),
        });
        events.extend(call("9", "close_order", json!({"order": 0})));
        events.push(say("Done."));
        episode(format!("a-{i:03}"), events)
    }

    /// Another agent: it finds an account and reads an order straight
    /// away, where the flow's agent read the account first, then finds
    /// another account, where it read more orders or replied; six times.
    fn unusual(i: usize) -> Episode {
        let mut events = vec![Event::User {
            text: format!("I'm c{i}@example.com; check my orders, please."),
        }];
        for k in 0..6 {
            events.extend(call(
                &format!("f{k}"),
                "find_account",
                json!({"email": i + k}),
            ));
            events.extend(call(&format!("o{k}"), "get_order", json!({"order": k})));
        }
        events.push(say("Done."));
        episode(format!("b-{i:03}"), events)
    }

    fn episode(id: String, events: Vec<Event>) -> Episode {
        Episode {
            task_id: id.clone(),
            id,
            trial: 0,
            domain: "shop".to_string(),
            agent_model: "agent".to_string(),
            reward: 1.0,
            events,
        }
    }

    fn config() -> Config {
        let mut config = Config::new(std::path::PathBuf::new());
        config.alpha_samples = 0;
        config
    }

    fn flow() -> Flow {
        let train: Vec<Episode> = (0..60).map(usual).collect();
        compile_habit_flow_from_episodes(&config(), &train, &manifest()).unwrap()
    }

    fn gate(threshold: f64) -> SurpriseGate {
        SurpriseGate {
            window: 3,
            threshold,
            quantile: None,
        }
    }

    #[test]
    fn scores_the_agents_steps_where_the_flow_decides() {
        let flow = flow();
        let usual = steps(&flow, &usual(61));
        let unusual = steps(&flow, &unusual(1));
        // After each lookup of the usual session, the reply after its last
        // order included, but not after the write, which the customer's
        // words follow.
        assert_eq!(usual.len(), 4, "{usual:?}");
        assert!(usual.iter().all(|&s| s < 1.5), "{usual:?}");
        // After each call of the unusual one. An order read where the flow
        // expected the account, and an account found where it expected an
        // order or a reply, each count as handing back, which the habit
        // found unlikely.
        assert_eq!(unusual.len(), 12, "{unusual:?}");
        assert!(peak(&unusual, 3).unwrap() > peak(&usual, 3).unwrap() + 1.0);
        // A session too short for the window has no peak.
        assert_eq!(peak(&usual, 5), None);
        assert_eq!(peak(&[], 1), None);
    }

    #[test]
    fn the_flows_own_lookups_are_not_the_agents_steps() {
        let flow = flow();
        let own = steps(&flow, &usual(62));
        // The proxy read the account right after the agent found it, as
        // the flow would: the agent took no step there, and what it did
        // after the lookup is scored as what it did after its own call.
        let mut served = usual(62);
        served.events.splice(
            3..5,
            call("stretto-1", "get_account", json!({"account": 62})),
        );
        assert_eq!(steps(&flow, &served), own[1..].to_vec());
        // The agent's turn of two calls is one step, the first call, after
        // which the flow decides once both have returned.
        let mut together = usual(62);
        together.events.splice(
            3..7,
            turn(&[
                ("2", "get_account", json!({"account": 62})),
                ("o0", "get_order", json!({"order": 0})),
            ]),
        );
        assert_eq!(steps(&flow, &together).len(), own.len() - 1);
    }

    #[test]
    fn windows_average_steps_in_a_row() {
        let means: Vec<f64> = window_means(&[1.0, 2.0, 3.0, 6.0], 2).collect();
        assert_eq!(means, vec![1.5, 2.5, 4.5]);
        assert_eq!(peak(&[1.0, 2.0, 3.0, 6.0], 3), Some(11.0 / 3.0));
        // A window of none is one step.
        assert_eq!(peak(&[1.0, 2.0], 0), Some(2.0));
    }

    #[test]
    fn trips_on_a_surprising_session_and_hands_back_for_the_rest_of_it() {
        let flow = flow();
        let unusual = unusual(2);
        let threshold = peak(&steps(&flow, &usual(61)), 3).unwrap() + 0.5;
        assert!(tripped(&flow, &usual(61), &gate(threshold)).is_none());
        let mean = tripped(&flow, &unusual, &gate(threshold)).unwrap();
        assert!(mean > threshold);

        let gated = flow.clone().with_surprise(Some(gate(threshold)));
        // Early in the session nothing is known yet: the flow decides.
        let early = Episode {
            events: unusual.events[..3].to_vec(),
            ..unusual.clone()
        };
        let plain = flow.next_with(&early, &MOCK, 0.3, Decider::Habit).unwrap();
        let first = gated.next_with(&early, &MOCK, 0.3, Decider::Habit).unwrap();
        assert_eq!(first.proposal, plain.proposal);
        // Once enough of its steps surprised the flow, it hands back after
        // every call, however likely a lookup.
        let late = Episode {
            events: unusual.events[..unusual.events.len() - 3].to_vec(),
            ..unusual.clone()
        };
        let next = gated.next_with(&late, &MOCK, 0.0, Decider::Habit).unwrap();
        let answer = serde_json::to_value(&next).unwrap();
        assert_eq!(answer["action"], "hand_back");
        let reason = answer["reason"].as_str().unwrap();
        assert!(
            reason.starts_with("the session surprised the flow: 3 of the agent's steps"),
            "{reason}"
        );
        assert_eq!(next.site.as_deref(), Some("find_account"));
        assert!(next.probs.is_empty());
        // Where the flow does not decide, it hands back as it would.
        let ended = gated.next_with(&unusual, &MOCK, 0.0, Decider::Habit);
        assert_eq!(
            serde_json::to_value(ended.unwrap()).unwrap()["reason"],
            "the last step is not a tool call"
        );
        // The audit scores the flow's probabilities whatever the gate says.
        let audited = |f: &Flow| decisions_with(f, &unusual, &MOCK, Decider::Habit).0;
        assert_eq!(audited(&gated), audited(&flow));
    }

    #[test]
    fn learns_the_threshold_from_held_out_sessions() {
        let train: Vec<Episode> = (0..60).map(usual).collect();
        let at = |q: f64| learn(&config(), &train, &manifest(), q, 3).unwrap();
        let (low, high) = (at(0.5), at(1.0));
        assert_eq!(low.window, 3);
        assert_eq!(low.quantile, Some(0.5));
        assert!(low.threshold <= high.threshold);
        // At 1 it is the most surprising held-out window, which a habit
        // that learned from every session finds in none of them; at 0, the
        // least surprising.
        let flow = flow();
        assert!(train.iter().all(|e| tripped(&flow, e, &high).is_none()));
        assert!(at(0.0).threshold <= low.threshold);
        // A session without a task id is its own task.
        let untasked: Vec<Episode> = train
            .iter()
            .cloned()
            .map(|mut e| {
                e.task_id.clear();
                e
            })
            .collect();
        assert_eq!(
            learn(&config(), &untasked, &manifest(), 0.5, 3).unwrap(),
            low
        );
        // One task's sessions leave no habit to hold them out from, and a
        // window longer than any session leaves nothing to score.
        let one_task: Vec<Episode> = train
            .iter()
            .cloned()
            .map(|mut e| {
                e.task_id = "t".to_string();
                e
            })
            .collect();
        let err = learn(&config(), &one_task, &manifest(), 0.5, 3).unwrap_err();
        assert!(err.to_string().starts_with("no held-out training session"));
        assert!(learn(&config(), &train, &manifest(), 0.5, 40).is_err());
        // Learned again, a learned gate is learned at its quantile, and one
        // set by hand stays as it is.
        let again = |g: Option<&SurpriseGate>| again(g, &config(), &train, &manifest()).unwrap();
        assert_eq!(again(Some(&low)), Some(low));
        assert_eq!(again(Some(&gate(1.0))), Some(gate(1.0)));
        assert_eq!(again(None), None);
    }

    #[test]
    fn a_threshold_or_off_overrides_the_gate() {
        assert_eq!("off".parse::<Override>(), Ok(Override::Off));
        assert_eq!("1.5".parse::<Override>(), Ok(Override::Threshold(1.5)));
        // As the proxy's command line carries it.
        for over in [
            Override::Off,
            Override::Threshold(1.5),
            Override::Threshold(3.0),
        ] {
            assert_eq!(over.to_string().parse::<Override>(), Ok(over));
        }
        assert_eq!(Override::Threshold(3.0).to_string(), "3");
        for bad in ["0", "-1", "inf", "NaN", "never"] {
            let err = bad.parse::<Override>().unwrap_err();
            assert_eq!(
                err,
                format!("`off`, or a threshold in nats above 0, not `{bad}`")
            );
        }
        let flow = flow();
        assert_eq!(flow.surprise(), None);
        let set = flow
            .clone()
            .with_surprise_override(Some(Override::Threshold(2.0)));
        assert_eq!(
            set.surprise(),
            Some(&SurpriseGate {
                window: DEFAULT_WINDOW,
                threshold: 2.0,
                quantile: None
            })
        );
        let learned = flow.clone().with_surprise(Some(SurpriseGate {
            window: 3,
            threshold: 1.0,
            quantile: Some(0.9),
        }));
        let raised = learned
            .clone()
            .with_surprise_override(Some(Override::Threshold(4.0)));
        assert_eq!(raised.surprise(), Some(&gate(4.0)));
        assert_eq!(
            learned
                .clone()
                .with_surprise_override(None)
                .surprise()
                .copied(),
            learned.surprise().copied()
        );
        let off = learned.with_surprise_override(Some(Override::Off));
        assert_eq!(off.surprise(), None);
        assert_eq!(off.format_version(), FLOW_VERSION);
    }

    #[test]
    fn a_gate_makes_the_flow_format_2_and_round_trips() {
        let learned = SurpriseGate {
            window: 4,
            threshold: 1.25,
            quantile: Some(0.95),
        };
        let text = serde_json::to_string(&flow().with_surprise(Some(learned))).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["stretto_flow"], json!(FLOW_THRESHOLDS_VERSION));
        assert_eq!(
            json["surprise"],
            json!({"window": 4, "threshold": 1.25, "quantile": 0.95})
        );
        assert_eq!(Flow::from_json(&text).unwrap().surprise(), Some(&learned));
        assert_eq!(
            learned.describe(),
            "hands back for the rest of a session once 4 of the agent's steps in a row average more than 1.25 nats of surprise, the 0.95 quantile of held-out training sessions' most surprising windows"
        );
        assert_eq!(
            gate(2.0).describe(),
            "hands back for the rest of a session once 3 of the agent's steps in a row average more than 2.00 nats of surprise"
        );
        // A build that reads only version 1 would ignore the gate, so a
        // version 1 flow may not have one, and none may average over no
        // steps.
        let mut v1 = json.clone();
        v1["stretto_flow"] = json!(FLOW_VERSION);
        let err = Flow::from_json(&v1.to_string()).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("a flow with a surprise gate is format {FLOW_THRESHOLDS_VERSION}")
        );
        let mut empty = json;
        empty["surprise"]["window"] = json!(0);
        let err = Flow::from_json(&empty.to_string()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a surprise gate averages over one step or more"
        );
    }
}
