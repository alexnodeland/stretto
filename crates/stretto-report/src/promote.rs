//! Promoting a flow's sites (RFC-001 §3.7): the flow acts only after the
//! calls where it has shown, on recorded sessions, that its lookups are the
//! agent's own.
//!
//! Wherever the flow would decide in a recorded session, right after a tool
//! returns, [`score`] asks it what it would do, at the threshold it will be
//! served with. The flow decides after every call, and sees an agent's
//! parallel calls one at a time, so a turn of several calls is scored as
//! one call per turn, as the replays and the proxy present it. A lookup it
//! would make counts as used when the agent makes it later in the session
//! (the same tool, with the flow's arguments among the agent's), which
//! spares the agent that call, and as a detour when it never does, as in
//! the replays. [`promote`]
//! keeps the sites whose record meets a [`Bar`]: a share of used lookups, a
//! lower bound on that share, and a number of distinct tasks. The bound and
//! the tasks matter because agreement measured on a few tasks does not carry
//! to new ones (Phase 0's validated arbitration).

use crate::flow::{Bar, Decider, Flow, Promotion, Proposal, SiteRecord};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use stretto_oracle::Oracle;
use stretto_trace::{Episode, Event};

/// The z of a 90% two-sided interval.
const Z90: f64 = 1.644_853_6;

/// One site's decisions in the sessions scored.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Tally {
    /// Decisions the flow made there.
    pub decisions: usize,
    /// The lookups it would have made.
    pub lookups: usize,
    /// Of those, the ones the agent made later in the session.
    pub used: usize,
    /// The tasks (or sessions) the lookups came from.
    pub tasks: BTreeSet<String>,
}

/// What [`score`] found.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Scored {
    /// Each site's tally, by name.
    pub sites: BTreeMap<String, Tally>,
    /// Decisions left out: the System-One model gave no answer (a replay
    /// cache without the question, say).
    pub unanswered: usize,
    /// Episodes scored.
    pub episodes: usize,
}

/// Whether the agent makes the lookup `tool(arguments)` in `later`, the
/// rest of the session: a call of the same tool whose arguments hold each
/// of the flow's.
fn made(later: &[Event], tool: &str, arguments: &Value) -> bool {
    let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    later.iter().any(|e| {
        let Event::Assistant { calls, .. } = e else {
            return false;
        };
        calls.iter().any(|c| {
            c.name == tool
                && arguments.as_object().is_none_or(|flow| {
                    flow.iter()
                        .all(|(k, v)| c.arguments.get(k).is_some_and(|a| text(a) == text(v)))
                })
        })
    })
}

/// `episode` with each turn of several calls split into one turn per call,
/// each followed by its result: the calls as the flow sees them, one at a
/// time. A turn whose results cannot all be matched to its calls is kept.
pub fn one_call_per_turn(episode: &Episode) -> Episode {
    let events = &episode.events;
    let mut out = Vec::with_capacity(events.len());
    let mut i = 0;
    while i < events.len() {
        let Event::Assistant { text, calls, usage } = &events[i] else {
            out.push(events[i].clone());
            i += 1;
            continue;
        };
        let results = &events[i + 1..(i + 1 + calls.len()).min(events.len())];
        let matched: Option<Vec<&Event>> = calls
            .iter()
            .map(|c| {
                results
                    .iter()
                    .find(|r| matches!(r, Event::ToolResult { call_id, .. } if *call_id == c.id))
            })
            .collect();
        match matched {
            Some(matched) if calls.len() > 1 => {
                for (k, (call, result)) in calls.iter().zip(matched).enumerate() {
                    out.push(Event::Assistant {
                        text: if k == 0 { text.clone() } else { None },
                        calls: vec![call.clone()],
                        usage: if k == 0 { *usage } else { None },
                    });
                    out.push(result.clone());
                }
                i += 1 + calls.len();
            }
            _ => {
                out.push(events[i].clone());
                i += 1;
            }
        }
    }
    Episode {
        events: out,
        ..episode.clone()
    }
}

/// Score each lookup `flow` would make in `episodes`, deciding by `decider`
/// at `threshold`, against the rest of the session. The flow's own
/// promotion, if it has one, is set aside, so every site is scored.
pub fn score(
    flow: &Flow,
    episodes: &[Episode],
    oracle: &dyn Oracle,
    decider: Decider,
    threshold: f64,
) -> Scored {
    let flow = flow.clone().with_promotion(None);
    let mut out = Scored {
        episodes: episodes.len(),
        ..Scored::default()
    };
    for episode in episodes {
        let episode = &one_call_per_turn(episode);
        let task = if episode.task_id.is_empty() {
            episode.id.clone()
        } else {
            episode.task_id.clone()
        };
        let events = &episode.events;
        for (i, e) in events.iter().enumerate() {
            // A decision point: a tool result with something after it.
            let (Event::ToolResult { .. }, Some(_)) = (e, events.get(i + 1)) else {
                continue;
            };
            let prefix = Episode {
                events: events[..=i].to_vec(),
                ..episode.clone()
            };
            let Ok(next) = flow.next_with(&prefix, oracle, threshold, decider) else {
                out.unanswered += 1;
                continue;
            };
            let Some(site) = next.site.clone() else {
                continue;
            };
            if next.key.is_some() && next.probs.is_empty() {
                out.unanswered += 1;
                continue;
            }
            let tally = out.sites.entry(site).or_default();
            tally.decisions += 1;
            if let Proposal::Lookup { tool, arguments } = &next.proposal {
                tally.lookups += 1;
                tally.tasks.insert(task.clone());
                if made(&events[i + 1..], tool, arguments) {
                    tally.used += 1;
                }
            }
        }
    }
    out
}

/// The lower bound of the Wilson interval for `k` successes in `n` trials.
pub fn wilson_lower(k: usize, n: usize, z: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let (k, n) = (k as f64, n as f64);
    let p = k / n;
    let z2 = z * z;
    let centre = p + z2 / (2.0 * n);
    let margin = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((centre - margin) / (1.0 + z2 / n)).max(0.0)
}

/// The promotion `scored` earns at `bar`: every site scored, and whether it
/// met the bar.
pub fn promote(scored: &Scored, bar: Bar) -> Promotion {
    let sites = scored
        .sites
        .iter()
        .map(|(site, t)| {
            let lower = wilson_lower(t.used, t.lookups, Z90);
            let share = if t.lookups > 0 {
                t.used as f64 / t.lookups as f64
            } else {
                0.0
            };
            let promoted = t.lookups > 0
                && share >= bar.min_used
                && lower >= bar.min_lower
                && t.tasks.len() >= bar.min_tasks;
            (
                site.clone(),
                SiteRecord {
                    decisions: t.decisions,
                    lookups: t.lookups,
                    used: t.used,
                    tasks: t.tasks.len(),
                    lower,
                    promoted,
                },
            )
        })
        .collect();
    Promotion { bar, sites }
}

/// A promotion as Markdown: the bar, then each site's record.
pub fn markdown(promotion: &Promotion) -> String {
    let b = &promotion.bar;
    let mut md = String::new();
    let _ = writeln!(
        md,
        "A site is promoted when, at a threshold of {}, at least {:.0}% of the flow's lookups there were ones the agent made later in the session, the 90% interval's lower bound on that share is at least {:.2}, and the lookups came from at least {} tasks.\n",
        b.threshold,
        100.0 * b.min_used,
        b.min_lower,
        b.min_tasks
    );
    let _ = writeln!(
        md,
        "| After | Decisions | Lookups it would make | Made by the agent later | Lower bound | Tasks | Promoted |\n|---|---|---|---|---|---|---|"
    );
    for (site, r) in &promotion.sites {
        let share = if r.lookups > 0 {
            format!(
                "{} ({:.0}%)",
                r.used,
                100.0 * r.used as f64 / r.lookups as f64
            )
        } else {
            "—".to_string()
        };
        let _ = writeln!(
            md,
            "| `{site}` | {} | {} | {share} | {:.2} | {} | {} |",
            r.decisions,
            r.lookups,
            r.lower,
            r.tasks,
            if r.promoted { "yes" } else { "no" }
        );
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stretto_trace::ToolCall;

    fn turn(calls: &[(&str, Value)]) -> Event {
        Event::Assistant {
            text: None,
            calls: calls
                .iter()
                .enumerate()
                .map(|(i, (name, arguments))| ToolCall {
                    id: i.to_string(),
                    name: name.to_string(),
                    arguments: arguments.clone(),
                })
                .collect(),
            usage: None,
        }
    }

    #[test]
    fn a_lookup_is_used_when_the_agent_makes_it_later_with_the_flows_arguments() {
        let order = json!({"order_id": "#W1"});
        let reply = Event::Assistant {
            text: Some("Here it is.".to_string()),
            calls: Vec::new(),
            usage: None,
        };
        let one = |calls: &[(&str, Value)]| vec![turn(calls)];
        assert!(made(
            &one(&[("get_order", order.clone())]),
            "get_order",
            &order
        ));
        // After a reply, among parallel calls, and with more arguments than
        // the flow bound.
        let later = [
            reply.clone(),
            turn(&[
                ("get_user", json!({"user_id": "u"})),
                ("get_order", json!({"order_id": "#W1", "verbose": true})),
            ]),
        ];
        assert!(made(&later, "get_order", &order));
        // Another order, another tool, or no call at all is a detour.
        let other = one(&[("get_order", json!({"order_id": "#W2"}))]);
        assert!(!made(&other, "get_order", &order));
        assert!(!made(
            &one(&[("get_user", order.clone())]),
            "get_order",
            &order
        ));
        assert!(!made(&[reply], "get_order", &order));
    }

    #[test]
    fn parallel_calls_are_scored_one_at_a_time() {
        let result = |id: &str| Event::ToolResult {
            call_id: id.to_string(),
            name: "get_order".to_string(),
            error: false,
            content: "{}".to_string(),
        };
        let episode = Episode {
            id: "e".to_string(),
            task_id: "1".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "agent".to_string(),
            reward: 1.0,
            events: vec![
                turn(&[
                    ("get_order", json!({"order_id": "#W1"})),
                    ("get_order", json!({"order_id": "#W2"})),
                ]),
                result("1"),
                result("0"),
            ],
        };
        let split = one_call_per_turn(&episode);
        let shape: Vec<String> = split
            .events
            .iter()
            .map(|e| match e {
                Event::Assistant { calls, .. } => format!("call {}", calls[0].id),
                Event::ToolResult { call_id, .. } => format!("result {call_id}"),
                Event::User { .. } => "user".to_string(),
            })
            .collect();
        assert_eq!(shape, ["call 0", "result 0", "call 1", "result 1"]);
    }

    #[test]
    fn the_wilson_bound_is_below_the_share_and_rises_with_evidence() {
        assert_eq!(wilson_lower(0, 0, Z90), 0.0);
        let few = wilson_lower(4, 5, Z90);
        let many = wilson_lower(80, 100, Z90);
        assert!(few < 0.8 && many < 0.8 && few < many, "{few} {many}");
        assert!((many - 0.7267).abs() < 1e-4, "{many}");
    }

    #[test]
    fn a_site_is_promoted_only_when_it_meets_every_part_of_the_bar() {
        let tally = |lookups: usize, used: usize, tasks: usize| Tally {
            decisions: lookups + 1,
            lookups,
            used,
            tasks: (0..tasks).map(|t| t.to_string()).collect(),
        };
        let scored = Scored {
            sites: BTreeMap::from([
                ("broad".to_string(), tally(40, 36, 12)),
                ("narrow".to_string(), tally(40, 36, 2)),
                ("few".to_string(), tally(3, 3, 3)),
                ("wrong".to_string(), tally(40, 12, 12)),
                ("idle".to_string(), tally(0, 0, 0)),
            ]),
            unanswered: 0,
            episodes: 12,
        };
        let bar = Bar {
            threshold: 0.3,
            min_used: 0.6,
            min_lower: 0.6,
            min_tasks: 3,
        };
        let p = promote(&scored, bar);
        let promoted: Vec<&String> = p
            .sites
            .iter()
            .filter(|(_, r)| r.promoted)
            .map(|(s, _)| s)
            .collect();
        assert_eq!(promoted, ["broad"]);
        assert!(p.allows("broad") && !p.allows("narrow") && !p.allows("never scored"));
        assert!(
            markdown(&p).contains("| `broad` | 41 | 40 | 36 (90%) |"),
            "{}",
            markdown(&p)
        );
    }
}
