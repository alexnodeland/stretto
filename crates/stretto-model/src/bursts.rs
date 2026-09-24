//! Runs of tool calls with no message to the user in between.
//!
//! τ²-bench's policies allow one tool call per assistant turn, so a run that
//! spans `n` assistant turns costs `n` LLM calls. A macro-tool that performs
//! the run costs one. Summing `n − 1` over runs therefore bounds how many LLM
//! turns macro-tools could remove. `plan_*` and `commit_*` are separate calls,
//! but the user's confirmation already splits a write from the lookups before
//! it, so the bound needs no adjustment for them.
//!
//! [`lookups_in_runs`] splits the part of that bound a read-only flow can
//! reach by where the lookups' arguments came from: a flow behind the tools
//! binds values from earlier tool outputs, and only a macro-tool the LLM
//! names could take a lookup that needs a value from the conversation.

use crate::provenance::{call_sources, Source};
use serde::Serialize;
use stretto_trace::{Episode, Event};

/// One run of tool calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Run {
    /// Tool names in order.
    pub tools: Vec<String>,
    /// Assistant turns the run spans.
    pub turns: usize,
}

/// The maximal runs of tool-calling assistant turns in `ep`.
pub fn runs(ep: &Episode) -> Vec<Run> {
    let mut out = Vec::new();
    let mut current = Run {
        tools: Vec::new(),
        turns: 0,
    };
    let close = |current: &mut Run, out: &mut Vec<Run>| {
        if current.turns > 0 {
            out.push(std::mem::replace(
                current,
                Run {
                    tools: Vec::new(),
                    turns: 0,
                },
            ));
        }
    };
    for e in &ep.events {
        match e {
            Event::Assistant { calls, .. } if calls.is_empty() => close(&mut current, &mut out),
            Event::Assistant { calls, .. } => {
                current.turns += 1;
                current.tools.extend(calls.iter().map(|c| c.name.clone()));
            }
            Event::User { .. } => close(&mut current, &mut out),
            Event::ToolResult { .. } => {}
        }
    }
    close(&mut current, &mut out);
    out
}

/// Totals over many episodes.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RunSummary {
    /// All assistant turns.
    pub assistant_turns: usize,
    /// Assistant turns that called tools.
    pub tool_turns: usize,
    /// Runs found.
    pub runs: usize,
    /// Σ (turns − 1) over runs: the turns macro-tools could remove at most.
    pub removable_turns: usize,
}

impl RunSummary {
    /// Removable turns as a share of all assistant turns.
    pub fn removable_share(&self) -> f64 {
        if self.assistant_turns == 0 {
            0.0
        } else {
            self.removable_turns as f64 / self.assistant_turns as f64
        }
    }
}

/// Summarize the runs in `episodes`.
pub fn summarize<'a>(episodes: impl IntoIterator<Item = &'a Episode>) -> RunSummary {
    let mut s = RunSummary::default();
    for ep in episodes {
        s.assistant_turns += ep.assistant_turns();
        for r in runs(ep) {
            s.runs += 1;
            s.tool_turns += r.turns;
            s.removable_turns += r.turns - 1;
        }
    }
    s
}

/// LLM turns that only look something up and follow another tool call in
/// the same run, by where their arguments came from (see
/// [`crate::provenance`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LookupsInRuns {
    /// All assistant turns.
    pub assistant_turns: usize,
    /// Every argument value was in an earlier tool output, or is a literal or
    /// too short to trace: a read-only flow behind the tools can bind them.
    pub from_outputs: usize,
    /// Some value only the user said, or the agent produced: a flow cannot
    /// bind it, and a macro-tool the LLM names could take the turn only if
    /// the LLM passed the value.
    pub from_conversation: usize,
}

impl LookupsInRuns {
    /// `n` as a share of all assistant turns.
    pub fn share(&self, n: usize) -> f64 {
        if self.assistant_turns == 0 {
            0.0
        } else {
            n as f64 / self.assistant_turns as f64
        }
    }

    /// Add `other`'s counts.
    pub fn add(&mut self, other: &Self) {
        self.assistant_turns += other.assistant_turns;
        self.from_outputs += other.from_outputs;
        self.from_conversation += other.from_conversation;
    }
}

/// Count the lookups inside runs in `episodes`; `is_lookup` says which tools
/// only read.
pub fn lookups_in_runs<'a>(
    episodes: impl IntoIterator<Item = &'a Episode>,
    is_lookup: impl Fn(&str) -> bool,
) -> LookupsInRuns {
    let mut out = LookupsInRuns::default();
    for ep in episodes {
        out.assistant_turns += ep.assistant_turns();
        let sources = call_sources(ep);
        let mut next = 0;
        let mut in_run = false;
        for e in &ep.events {
            match e {
                Event::Assistant { calls, .. } if calls.is_empty() => in_run = false,
                Event::Assistant { calls, .. } => {
                    let these = sources.get(next..next + calls.len()).unwrap_or(&[]);
                    next += calls.len();
                    if in_run && calls.iter().all(|c| is_lookup(&c.name)) {
                        let conversation = these
                            .iter()
                            .flatten()
                            .any(|(_, s, _)| matches!(s, Source::User | Source::Generated));
                        if conversation {
                            out.from_conversation += 1;
                        } else {
                            out.from_outputs += 1;
                        }
                    }
                    in_run = true;
                }
                Event::User { .. } => in_run = false,
                Event::ToolResult { .. } => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_trace::ToolCall;

    fn tool(id: &str, name: &str) -> Event {
        Event::Assistant {
            usage: None,
            text: None,
            calls: vec![ToolCall {
                id: id.into(),
                name: name.into(),
                arguments: serde_json::json!({}),
            }],
        }
    }

    fn reply() -> Event {
        Event::Assistant {
            usage: None,
            text: Some("ok".into()),
            calls: vec![],
        }
    }

    #[test]
    fn splits_runs_at_messages() {
        let ep = Episode {
            id: "e".into(),
            task_id: "t".into(),
            trial: 0,
            domain: "d".into(),
            agent_model: "m".into(),
            reward: 1.0,
            events: vec![
                Event::User { text: "hi".into() },
                tool("1", "a"),
                tool("2", "b"),
                tool("3", "c"),
                reply(),
                Event::User { text: "yes".into() },
                tool("4", "w"),
                reply(),
            ],
        };
        let r = runs(&ep);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].tools, vec!["a", "b", "c"]);
        assert_eq!(r[0].turns, 3);
        assert_eq!(r[1].tools, vec!["w"]);
        let s = summarize([&ep]);
        assert_eq!(s.assistant_turns, 6);
        assert_eq!(s.tool_turns, 4);
        assert_eq!(s.removable_turns, 2);
        assert!((s.removable_share() - 2.0 / 6.0).abs() < 1e-12);
    }

    fn call(id: &str, name: &str, arguments: serde_json::Value) -> Event {
        Event::Assistant {
            usage: None,
            text: None,
            calls: vec![ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            }],
        }
    }

    fn output(id: &str, content: &str) -> Event {
        Event::ToolResult {
            call_id: id.into(),
            name: "t".into(),
            error: false,
            content: content.into(),
        }
    }

    #[test]
    fn splits_lookups_in_runs_by_where_their_arguments_came_from() {
        use serde_json::json;
        let ep = Episode {
            id: "e".into(),
            task_id: "t".into(),
            trial: 0,
            domain: "d".into(),
            agent_model: "m".into(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "i fly from jfk on may 20".into(),
                },
                // First in its run: the LLM's own turn either way.
                call("1", "get_user", json!({"user_id": "ann_1"})),
                output("1", r#"{"reservations": ["R1"]}"#),
                // From an earlier output: a flow can bind it.
                call("2", "get_reservation", json!({"reservation_id": "R1"})),
                output("2", r#"{"origin": "SEA"}"#),
                // A city the customer named, and a date the agent worked out.
                call(
                    "3",
                    "search",
                    json!({"origin": "JFK", "date": "2024-05-20"}),
                ),
                output("3", "[]"),
                // A write is not a lookup, but continues the run.
                call("4", "book", json!({"reservation_id": "R1"})),
                output("4", "ok"),
                call("5", "get_reservation", json!({"reservation_id": "R1"})),
                output("5", "{}"),
                reply(),
                Event::User {
                    text: "thanks".into(),
                },
                // First again after the customer's message.
                call("6", "get_reservation", json!({"reservation_id": "R1"})),
                output("6", "{}"),
            ],
        };
        let is_lookup = |t: &str| t != "book";
        let l = lookups_in_runs([&ep], is_lookup);
        assert_eq!(l.assistant_turns, 7);
        assert_eq!(l.from_outputs, 2);
        assert_eq!(l.from_conversation, 1);
        assert!((l.share(l.from_conversation) - 1.0 / 7.0).abs() < 1e-12);
    }
}
