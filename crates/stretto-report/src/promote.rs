//! Promoting a flow's sites (RFC-001 §3.7): the flow acts only after the
//! calls where it has shown, on recorded sessions, that its lookups are the
//! agent's own.
//!
//! Wherever the flow would decide in a recorded session, right after a tool
//! returns, [`score`] asks it what it would do, at the threshold it will be
//! served with, and decides as the proxy would:
//!
//! - **As each result arrives** ([`Episode::after_call`]). The calls of the
//!   turn the host had sent and that have not returned are asked for
//!   already, so the flow does not propose them. The calls it sent later, as
//!   a host that runs a turn's calls as the model streams them does, the
//!   proxy could not know of ([`Recorded::sent_after`]).
//! - **Again after each lookup,** up to `per_call` of them, as the proxy
//!   chains them: a lookup the agent made later has its result, so the flow
//!   decides after it as it would have served. A chain can spare a whole
//!   parallel turn of the agent's, as the flow reads one record after
//!   another. The proxy makes a lookup only at a promoted site, so
//!   [`promote_served`] scores again with chains through the promoted sites
//!   alone, until the promotion no longer changes.
//!
//! A lookup counts as used when the agent makes it in a later LLM turn (the
//! same tool, with the flow's arguments among the agent's), which spares the
//! agent that call, and as a detour when it never does, as in the replays; a
//! hand-back is neither. [`promote`] keeps the sites whose record meets a
//! [`Bar`]: a share of used lookups, a lower bound on that share, and a
//! number of distinct tasks. The bound and the tasks matter because
//! agreement measured on a few tasks does not carry to new ones (Phase 0's
//! validated arbitration).

use crate::flow::{Bar, Decider, Flow, Promotion, Proposal, SiteRecord};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use stretto_oracle::Oracle;
use stretto_trace::{Episode, Event, ToolCall};

/// The z of a 90% two-sided interval.
const Z90: f64 = 1.644_853_6;

/// A session to score.
#[derive(Clone, Debug)]
pub struct Recorded {
    /// The session.
    pub episode: Episode,
    /// For each call, by id, how many of its turn's results had come back
    /// when the host sent it ([`stretto_trace::mcp::episode_sent`]). A call
    /// not listed was sent with its turn's first, as τ²-bench's harness
    /// sends a turn's calls.
    pub sent_after: HashMap<String, usize>,
}

impl From<Episode> for Recorded {
    fn from(episode: Episode) -> Self {
        Recorded {
            episode,
            sent_after: HashMap::new(),
        }
    }
}

/// One site's decisions in the sessions scored.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Tally {
    /// Decisions the flow made there.
    pub decisions: usize,
    /// The lookups it would have made.
    pub lookups: usize,
    /// Of those, the ones the agent made in a later LLM turn.
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

/// Where the agent makes the lookup `tool(arguments)` in `later`, the rest
/// of the session: a call of the same tool whose arguments hold each of the
/// flow's, with its result if one came.
fn made<'a>(later: &'a [Event], tool: &str, arguments: &Value) -> Option<Option<&'a Event>> {
    let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    let call = later
        .iter()
        .flat_map(|e| match e {
            Event::Assistant { calls, .. } => calls.as_slice(),
            _ => &[],
        })
        .find(|c| {
            c.name == tool
                && arguments.as_object().is_none_or(|flow| {
                    flow.iter()
                        .all(|(k, v)| c.arguments.get(k).is_some_and(|a| text(a) == text(v)))
                })
        })?;
    Some(
        later
            .iter()
            .find(|e| matches!(e, Event::ToolResult { call_id, .. } if *call_id == call.id)),
    )
}

/// `view` with the lookup `id`, `tool(arguments)`, in the turn at `turn`,
/// and `result`'s outcome as its result: the session as the proxy sees it
/// once its lookup returns.
fn with_lookup(mut view: Episode, turn: usize, lookup: ToolCall, result: &Event) -> Episode {
    if let Event::ToolResult { error, content, .. } = result {
        view.events.push(Event::ToolResult {
            call_id: lookup.id.clone(),
            name: lookup.name.clone(),
            error: *error,
            content: content.clone(),
        });
    }
    if let Some(Event::Assistant { calls, .. }) = view.events.get_mut(turn) {
        calls.push(lookup);
    }
    view
}

/// How the flow will be served, which [`score`] mirrors.
#[derive(Clone, Copy, Debug)]
pub struct Serving {
    /// Where the tool's probability comes from.
    pub decider: Decider,
    /// The threshold it is served with (`stretto-proxy --flow-threshold`).
    pub threshold: f64,
    /// Lookups after one call, at most (`stretto-proxy --flow-per-call`).
    pub per_call: usize,
}

/// Score each lookup `flow` would make in `sessions`, served as `serving`,
/// against the rest of the session. A chain goes on after a lookup made at
/// a site of `chains`, or at any site without it. The flow's own promotion,
/// if it has one, is set aside, so every site is scored.
pub fn score(
    flow: &Flow,
    sessions: &[Recorded],
    oracle: &dyn Oracle,
    serving: Serving,
    chains: Option<&BTreeSet<String>>,
) -> Scored {
    let Serving {
        decider,
        threshold,
        per_call,
    } = serving;
    let flow = flow.clone().with_promotion(None);
    let mut out = Scored {
        episodes: sessions.len(),
        ..Scored::default()
    };
    for Recorded {
        episode,
        sent_after,
    } in sessions
    {
        let task = if episode.task_id.is_empty() {
            episode.id.clone()
        } else {
            episode.task_id.clone()
        };
        let events = &episode.events;
        for (i, e) in events.iter().enumerate() {
            // A decision point: a tool result with something after it.
            let (Event::ToolResult { call_id, .. }, Some(_)) = (e, events.get(i + 1)) else {
                continue;
            };
            // The call's LLM turn: only a later one's calls can be spared.
            let Some((turn, ids)) = events.iter().enumerate().find_map(|(t, e)| match e {
                Event::Assistant { calls, .. } if calls.iter().any(|c| &c.id == call_id) => Some((
                    t,
                    calls.iter().map(|c| c.id.as_str()).collect::<HashSet<_>>(),
                )),
                _ => None,
            }) else {
                continue;
            };
            // How many of the turn's results have come back, this one too.
            let back = events[turn..=i]
                .iter()
                .filter(|e| matches!(e, Event::ToolResult { call_id, .. } if ids.contains(call_id.as_str())))
                .count();
            let prefix = Episode {
                events: events[..=i].to_vec(),
                ..episode.clone()
            };
            let (mut view, pending) = prefix.after_call(call_id);
            // The calls the host had sent by now; the proxy knows no others.
            let pending: Vec<ToolCall> = pending
                .into_iter()
                .filter(|c| sent_after.get(&c.id).copied().unwrap_or(0) < back)
                .collect();
            let later = &events[turn + 1..];
            let mut looked = 0;
            loop {
                let Ok(next) =
                    flow.next_explored(&view, &pending, oracle, threshold, decider, None)
                else {
                    out.unanswered += 1;
                    break;
                };
                let Some(site) = next.site.clone() else {
                    break;
                };
                if next.key.is_some() && next.probs.is_empty() {
                    out.unanswered += 1;
                    break;
                }
                // The proxy makes a lookup only at a promoted site.
                let chains_on = chains.is_none_or(|c| c.contains(&site));
                let tally = out.sites.entry(site).or_default();
                tally.decisions += 1;
                let Proposal::Lookup { tool, arguments } = next.proposal else {
                    break;
                };
                tally.lookups += 1;
                tally.tasks.insert(task.clone());
                let Some(result) = made(later, &tool, &arguments) else {
                    break;
                };
                tally.used += 1;
                looked += 1;
                // The proxy makes it and decides again once it returns.
                let Some(result) = result.filter(|_| looked < per_call && chains_on) else {
                    break;
                };
                let id = format!("stretto-{looked}");
                let lookup = ToolCall {
                    id: id.clone(),
                    name: tool,
                    arguments,
                };
                view = with_lookup(view, turn, lookup, result).after_call(&id).0;
            }
        }
    }
    out
}

/// The promotion `flow` earns on `sessions` at `bar`, served as `serving`,
/// and the scores it rests on. Chains go through every site at first; then
/// through the sites promoted, scored again, until the promotion repeats
/// (or eight times, if it keeps changing).
pub fn promote_served(
    flow: &Flow,
    sessions: &[Recorded],
    oracle: &dyn Oracle,
    serving: Serving,
    bar: Bar,
) -> (Scored, Promotion) {
    let mut scored = score(flow, sessions, oracle, serving, None);
    let mut promotion = promote(&scored, bar);
    for _ in 0..8 {
        let promoted: BTreeSet<String> = promotion
            .sites
            .iter()
            .filter(|(_, r)| r.promoted)
            .map(|(s, _)| s.clone())
            .collect();
        let again = score(flow, sessions, oracle, serving, Some(&promoted));
        let next = promote(&again, bar);
        let same = next == promotion;
        (scored, promotion) = (again, next);
        if same {
            break;
        }
    }
    (scored, promotion)
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
        "A site is promoted when, at a threshold of {}, at least {:.0}% of the flow's lookups there were ones the agent made in a later LLM turn, the 90% interval's lower bound on that share is at least {:.2}, and the lookups came from at least {} tasks.\n",
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

    /// A decision the flow cannot make is left out: its decider needs what
    /// the flow does not hold, the System-One model has no answer, or the
    /// session's last step is a reply, not a call.
    #[test]
    fn decisions_the_flow_cannot_make_are_left_out() {
        struct Silent;
        impl Oracle for Silent {
            fn ask(&self, _: &stretto_oracle::Request) -> anyhow::Result<stretto_oracle::Response> {
                anyhow::bail!("no answer")
            }
        }
        let flow = crate::flow::tests::toy_flow();
        let result = |id: &str, name: &str, content: Value| Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: content.to_string(),
        };
        let session = Episode {
            id: "s".to_string(),
            task_id: "t".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "agent".to_string(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "help with my orders".to_string(),
                },
                turn(&[("get_user_details", json!({"user_id": "ann_1"}))]),
                result("0", "get_user_details", json!({"orders": ["#W1", "#W2"]})),
                turn(&[("get_order_details", json!({"order_id": "#W1"}))]),
                result("0", "get_order_details", json!({"order_id": "#W1"})),
            ],
        };
        let scored = |episode: &Episode, decider| {
            let serving = Serving {
                decider,
                threshold: 0.3,
                per_call: crate::flow::PER_CALL,
            };
            score(&flow, &[episode.clone().into()], &Silent, serving, None)
        };
        // The toy flow holds no counts for reach, and its arbiter's
        // question goes unanswered.
        for decider in [Decider::Reach, Decider::Arbiter] {
            let s = scored(&session, decider);
            assert!(s.sites.is_empty(), "{decider:?}");
            assert_eq!(s.unanswered, 1, "{decider:?}");
        }
        // A reply before the result: no call to decide after.
        let mut replied = session.clone();
        replied.events.insert(
            2,
            Event::Assistant {
                text: Some("One moment.".to_string()),
                calls: Vec::new(),
                usage: None,
            },
        );
        let s = scored(&replied, Decider::Habit);
        assert!(s.sites.is_empty());
        assert_eq!(s.unanswered, 0);
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
        assert!(made(&one(&[("get_order", order.clone())]), "get_order", &order).is_some());
        // After a reply, among parallel calls, and with more arguments than
        // the flow bound.
        let later = [
            reply.clone(),
            turn(&[
                ("get_user", json!({"user_id": "u"})),
                ("get_order", json!({"order_id": "#W1", "verbose": true})),
            ]),
        ];
        assert!(made(&later, "get_order", &order).is_some());
        // Another order, another tool, or no call at all is a detour.
        let other = one(&[("get_order", json!({"order_id": "#W2"}))]);
        assert!(made(&other, "get_order", &order).is_none());
        assert!(made(&one(&[("get_user", order.clone())]), "get_order", &order).is_none());
        assert!(made(&[reply], "get_order", &order).is_none());
        // Numbers and the strings that spell them are the same.
        let numbered = one(&[("get_order", json!({"n": 5}))]);
        assert!(made(&numbered, "get_order", &json!({"n": "5"})).is_some());
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
        let md = markdown(&p);
        assert!(md.contains("| `broad` | 41 | 40 | 36 (90%) |"), "{md}");
    }
}
