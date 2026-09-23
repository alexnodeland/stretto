//! Replaying held-out episodes through macro-tool flows.
//!
//! Every run of consecutive tool calls (no message to the user in between) is
//! treated as one `plan_*` macro-tool call. The LLM spends one turn calling it.
//! Inside the run the flow has to make every decision the agent made.
//!
//! - **Which tool comes next, and when to stop.** The habit decides when its
//!   top option clears the threshold with enough evidence. Otherwise, depending
//!   on the [`Scenario`], either a System-One model decides (assumed to agree
//!   with the agent, so this is an upper bound) or the flow pauses and the LLM
//!   decides, at the cost of one turn.
//! - **Each call's arguments.** A value seen earlier, in a tool output or in
//!   the user's words (which the LLM passes in), is bound by the flow. A
//!   closed-set value (a boolean, a short code, or an argument that took few
//!   distinct values in training) is a System-One `Choice`. Anything else the
//!   agent generated forces a pause.
//!
//! A run that spanned `t` LLM turns then costs `min(t, 1 + pauses)`. Stopping
//! costs nothing: when the flow hands back at the end of a run, the LLM's next
//! turn is the reply it would have written anyway. A confident habit decision
//! that disagrees with the agent counts as a pause, and is also tallied as the
//! risk that the gate's success-loss bound has to absorb.

use crate::abstraction::{Action, Step};
use crate::provenance::Source;
use crate::world::{argmax, EncodedEpisode, Predictor};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// What a step's arguments need from outside the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ArgNeed {
    /// Nothing: every value was seen earlier (or the step is a reply).
    Bound,
    /// A closed-set choice a System-One model can make.
    ClosedSet,
    /// A value only the LLM can produce.
    Llm,
}

/// Who resolves the decisions the habit is not confident about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Scenario {
    /// Nobody: the flow pauses and the LLM decides.
    HabitOnly,
    /// A System-One model that always agrees with the agent: the upper bound
    /// on what Phase 0b can find.
    HabitThenPerfectOracle,
}

/// Arguments that took at most `max_values` distinct values over at least
/// `min_uses` uses, from `(tool, argument, value)` triples.
pub fn closed_sets(
    uses: impl IntoIterator<Item = (String, String, String)>,
    max_values: usize,
    min_uses: usize,
) -> HashSet<(String, String)> {
    let mut seen: BTreeMap<(String, String), (usize, BTreeSet<String>)> = BTreeMap::new();
    for (tool, arg, value) in uses {
        let e = seen.entry((tool, arg)).or_default();
        e.0 += 1;
        e.1.insert(value);
    }
    seen.into_iter()
        .filter(|(_, (n, values))| *n >= min_uses && values.len() <= max_values)
        .map(|(k, _)| k)
        .collect()
}

/// What each step's arguments need, aligned with the steps. `sources` holds
/// each tool call's `(argument, source, value)` leaves, in call order.
pub fn arg_needs(
    steps: &[Step],
    sources: &[Vec<(String, Source, String)>],
    closed: &HashSet<(String, String)>,
) -> Vec<ArgNeed> {
    let mut calls = sources.iter();
    steps
        .iter()
        .map(|s| match &s.action {
            Action::Respond => ArgNeed::Bound,
            Action::Tool(tool) => calls
                .next()
                .map(|leaves| {
                    leaves
                        .iter()
                        .map(|(arg, source, _)| match source {
                            Source::User | Source::ToolOutput | Source::Both => ArgNeed::Bound,
                            Source::Literal | Source::Short => ArgNeed::ClosedSet,
                            Source::Generated if closed.contains(&(tool.clone(), arg.clone())) => {
                                ArgNeed::ClosedSet
                            }
                            Source::Generated => ArgNeed::Llm,
                        })
                        .max()
                        .unwrap_or(ArgNeed::Bound)
                })
                .unwrap_or(ArgNeed::Bound),
        })
        .collect()
}

/// One held-out episode, prepared for replay.
pub struct ProjectionInput<'a> {
    /// The encoded episode (with whatever features and group the model uses).
    pub encoded: &'a EncodedEpisode,
    /// The assistant turn of each step.
    pub turns: Vec<usize>,
    /// What each step's arguments need.
    pub args: Vec<ArgNeed>,
    /// All assistant turns in the episode.
    pub assistant_turns: usize,
}

/// Totals from replaying episodes under one scenario and threshold.
#[derive(Clone, Debug, Serialize)]
pub struct Projection {
    /// Who resolves unconfident decisions.
    pub scenario: Scenario,
    /// Confidence the habit needs to act.
    pub threshold: f64,
    /// Episodes replayed.
    pub episodes: usize,
    /// All LLM turns in those episodes.
    pub assistant_turns: usize,
    /// LLM turns inside tool runs.
    pub run_turns: usize,
    /// LLM turns the macro-tools would remove.
    pub turns_saved: usize,
    /// Turns saved if every run collapsed to one call.
    pub ceiling: usize,
    /// Pauses (LLM decisions inside runs).
    pub pauses: usize,
    /// Decisions the habit took.
    pub habit_decisions: usize,
    /// Decisions and argument choices left to the System-One model.
    pub oracle_decisions: usize,
    /// Confident habit decisions that disagreed with the agent.
    pub disagreements: usize,
    /// Episodes with at least one such disagreement.
    pub episodes_with_disagreement: usize,
}

impl Projection {
    /// Turns saved as a share of all LLM turns.
    pub fn saved_share(&self) -> f64 {
        self.turns_saved as f64 / self.assistant_turns.max(1) as f64
    }

    /// The ceiling as a share of all LLM turns.
    pub fn ceiling_share(&self) -> f64 {
        self.ceiling as f64 / self.assistant_turns.max(1) as f64
    }

    /// Share of episodes with at least one disagreement.
    pub fn risky_share(&self) -> f64 {
        self.episodes_with_disagreement as f64 / self.episodes.max(1) as f64
    }
}

enum Verdict {
    /// The habit decided and agreed with the agent.
    Habit,
    /// The habit decided and disagreed.
    Wrong,
    /// The System-One model decided.
    Oracle,
    /// Nobody inside the flow could decide.
    Pause,
}

/// Replay `inputs` under `scenario`: the habit acts when its top option has
/// probability at least `threshold` and rests on at least `min_evidence`
/// observations.
pub fn project(
    model: &impl Predictor,
    inputs: &[ProjectionInput<'_>],
    scenario: Scenario,
    threshold: f64,
    min_evidence: f64,
) -> Projection {
    let mut p = Projection {
        scenario,
        threshold,
        episodes: inputs.len(),
        assistant_turns: 0,
        run_turns: 0,
        turns_saved: 0,
        ceiling: 0,
        pauses: 0,
        habit_decisions: 0,
        oracle_decisions: 0,
        disagreements: 0,
        episodes_with_disagreement: 0,
    };
    for input in inputs {
        let ep = input.encoded;
        let n = ep.actions.len();
        let is_tool = |k: usize| ep.actions[k] != 0; // id 0 is Respond
        let decide = |k: usize| -> Verdict {
            let probs = model.predict_at(ep, k);
            let top = argmax(&probs);
            if model.evidence_at(ep, k) >= min_evidence && probs[top] >= threshold {
                if top == ep.actions[k] as usize {
                    Verdict::Habit
                } else {
                    Verdict::Wrong
                }
            } else if scenario == Scenario::HabitThenPerfectOracle {
                Verdict::Oracle
            } else {
                Verdict::Pause
            }
        };
        p.assistant_turns += input.assistant_turns;
        let mut wrong_here = 0;
        let mut j = 0;
        while j < n {
            if !is_tool(j) {
                j += 1;
                continue;
            }
            let s = j;
            let mut e = j;
            while e < n && is_tool(e) {
                e += 1;
            }
            let mut turns: Vec<usize> = input.turns[s..e].to_vec();
            turns.dedup();
            let t = turns.len();
            let mut pauses = 0;
            for k in s + 1..e {
                let mut pause = false;
                match decide(k) {
                    Verdict::Habit => p.habit_decisions += 1,
                    Verdict::Wrong => {
                        p.habit_decisions += 1;
                        p.disagreements += 1;
                        wrong_here += 1;
                        pause = true;
                    }
                    Verdict::Oracle => p.oracle_decisions += 1,
                    Verdict::Pause => pause = true,
                }
                match input.args[k] {
                    ArgNeed::Bound => {}
                    ArgNeed::ClosedSet if scenario == Scenario::HabitThenPerfectOracle => {
                        p.oracle_decisions += 1
                    }
                    ArgNeed::ClosedSet | ArgNeed::Llm => pause = true,
                }
                pauses += pause as usize;
            }
            if e < n {
                // The stop decision: free to hand back, but a confident wrong
                // "continue" is still a risk.
                match decide(e) {
                    Verdict::Habit => p.habit_decisions += 1,
                    Verdict::Wrong => {
                        p.habit_decisions += 1;
                        p.disagreements += 1;
                        wrong_here += 1;
                    }
                    Verdict::Oracle => p.oracle_decisions += 1,
                    Verdict::Pause => {}
                }
            }
            p.run_turns += t;
            p.ceiling += t - 1;
            p.pauses += pauses;
            p.turns_saved += t - t.min(1 + pauses);
            j = e;
        }
        p.episodes_with_disagreement += (wrong_here > 0) as usize;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstraction::{Outcome, Vocab};
    use crate::world::BackoffModel;

    fn tool(name: &str) -> Step {
        Step {
            action: Action::Tool(name.into()),
            outcome: Outcome::Ok,
        }
    }

    fn reply() -> Step {
        Step {
            action: Action::Respond,
            outcome: Outcome::Reply,
        }
    }

    #[test]
    fn a_predictable_run_collapses_to_one_turn() {
        // Every episode: reply, then a run a → b → c, then reply.
        let steps = vec![reply(), tool("a"), tool("b"), tool("c"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c"]);
        let enc: Vec<EncodedEpisode> = (0..30)
            .map(|_| EncodedEpisode::encode(&steps, &vocab, true))
            .collect();
        let model = BackoffModel::fit(2, 0.5, vocab.len(), &enc);
        let input = ProjectionInput {
            encoded: &enc[0],
            turns: vec![0, 1, 2, 3, 4],
            args: vec![ArgNeed::Bound; 5],
            assistant_turns: 5,
        };
        let p = project(&model, &[input], Scenario::HabitOnly, 0.8, 5.0);
        assert_eq!(p.run_turns, 3);
        assert_eq!(p.ceiling, 2);
        assert_eq!(p.turns_saved, 2);
        assert_eq!(p.pauses, 0);
        assert_eq!(p.disagreements, 0);
        assert!((p.saved_share() - 0.4).abs() < 1e-12);
    }

    #[test]
    fn generated_arguments_and_unsure_decisions_pause() {
        let steps = vec![reply(), tool("a"), tool("b"), tool("c"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c"]);
        let enc = EncodedEpisode::encode(&steps, &vocab, true);
        // An untrained model is never confident.
        let empty = BackoffModel::new(2, 1.0, vocab.len());
        let mk = |args: Vec<ArgNeed>| ProjectionInput {
            encoded: &enc,
            turns: vec![0, 1, 2, 3, 4],
            args,
            assistant_turns: 5,
        };
        let habit_only = project(
            &empty,
            &[mk(vec![ArgNeed::Bound; 5])],
            Scenario::HabitOnly,
            0.8,
            0.0,
        );
        assert_eq!(habit_only.pauses, 2);
        assert_eq!(habit_only.turns_saved, 0);

        let oracle = project(
            &empty,
            &[mk(vec![
                ArgNeed::Bound,
                ArgNeed::Bound,
                ArgNeed::ClosedSet,
                ArgNeed::Llm,
                ArgNeed::Bound,
            ])],
            Scenario::HabitThenPerfectOracle,
            0.8,
            0.0,
        );
        // b's closed-set argument goes to the oracle; c's generated one pauses.
        assert_eq!(oracle.pauses, 1);
        assert_eq!(oracle.turns_saved, 1);
        assert_eq!(oracle.oracle_decisions, 2 + 1 + 1);
    }

    #[test]
    fn closed_sets_need_few_values_and_enough_uses() {
        let uses = (0..20)
            .map(|i| {
                (
                    "cancel".to_string(),
                    "reason".to_string(),
                    if i % 2 == 0 { "a" } else { "b" }.to_string(),
                )
            })
            .chain((0..20).map(|i| ("cancel".to_string(), "id".to_string(), format!("#{i}"))))
            .chain((0..3).map(|_| ("rare".to_string(), "x".to_string(), "v".to_string())));
        let c = closed_sets(uses, 8, 10);
        assert!(c.contains(&("cancel".to_string(), "reason".to_string())));
        assert!(!c.contains(&("cancel".to_string(), "id".to_string())));
        assert!(!c.contains(&("rare".to_string(), "x".to_string())));
    }
}
