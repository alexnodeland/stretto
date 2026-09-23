//! Runs of tool calls with no message to the user in between.
//!
//! τ²-bench's policies allow one tool call per assistant turn, so a run that
//! spans `n` assistant turns costs `n` LLM calls. A macro-tool that performs
//! the run costs one. Summing `n − 1` over runs therefore bounds how many LLM
//! turns macro-tools could remove. `plan_*` and `commit_*` are separate calls,
//! but the user's confirmation already splits a write from the lookups before
//! it, so the bound needs no adjustment for them.

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
}
