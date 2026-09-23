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
//! turn is the reply it would have written anyway. A habit decision that
//! disagrees with the agent counts as a pause. Handing back too early is safe
//! (the LLM takes the next step, one turn). Choosing a different tool, or
//! carrying on when the agent stopped, is the risk that the gate's success-loss
//! bound has to absorb.
//!
//! That holds when flows may call any tool ([`Design::AnyTool`]). Under the
//! RFC's plan/commit rule ([`Design::ReadOnly`]) flows only read between LLM
//! turns: a write, or any tool not known to be read-only, is handed back to
//! the LLM (a pause), and a flow that picks one hands back instead. A wrong
//! pick is then an extra lookup, a *detour*: it costs a pause, and its output
//! is charged to every later prompt, but it changes nothing, so it is not a
//! risk.

use crate::abstraction::{Action, Step};
use crate::provenance::Source;
use crate::world::{argmax, BackoffModel, EncodedEpisode, GroupedModel, Predictor, Symbol};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use stretto_trace::TurnUsage;

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
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Scenario {
    /// Nobody: the flow pauses and the LLM decides.
    HabitOnly,
    /// A System-One model that always agrees with the agent: the upper bound
    /// on what Phase 0b can find.
    HabitThenPerfectOracle,
    /// A real System-One model, from the answers in each
    /// [`ProjectionInput`]: trusted when its pick has at least this
    /// probability; below it, or where it was not asked, the flow pauses.
    HabitThenOracle(f64),
    /// Two keys: as [`Scenario::HabitThenOracle`], but a next-step pick is
    /// trusted only when it is also the habit's top option. Argument picks,
    /// which the habit does not predict, need the probability alone.
    TwoKeys(f64),
    /// As [`Scenario::HabitThenOracle`], for answers the caller has already
    /// combined with the habit (each [`ProjectionInput`] carries the
    /// arbitrated pick and its posterior probability).
    Arbitrated(f64),
    /// For read-only flows: as [`Scenario::Arbitrated`], but each answer is
    /// the most likely *lookup* and its probability, so the flow continues
    /// whenever a lookup is at least this likely and hands back otherwise.
    /// Handing back costs a turn; a wrong lookup costs only a detour.
    LookupFirst(f64),
}

/// What flows may do between LLM turns.
#[derive(Clone, Copy, Debug)]
pub enum Design<'a> {
    /// Any tool; every disagreement with the agent is a risk.
    AnyTool,
    /// Reads only (plan/commit): the `high` action ids, the writes and
    /// anything not known to be read-only, are always handed back to the LLM.
    /// Each detour is charged `detour_tokens` of input in every later prompt.
    ReadOnly {
        /// Action ids a flow never takes.
        high: &'a HashSet<u32>,
        /// Tokens of a typical lookup's output.
        detour_tokens: f64,
    },
}

/// A System-One answer to "what does the agent do next?".
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct OracleStep {
    /// The action id it picked (0 is respond).
    pub top: u32,
    /// The probability it put on that pick.
    pub prob: f64,
}

/// A System-One answer for a step's closed-set arguments.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct OracleArgs {
    /// Whether every pick matched the agent's value.
    pub agrees: bool,
    /// The lowest probability among its picks.
    pub prob: f64,
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
    /// Tokens and cost of each assistant turn (zeros where unrecorded).
    pub usage: Vec<TurnUsage>,
    /// Dollars per input token for this episode's model (see [`fit_prices`]).
    pub input_price: f64,
    /// System-One answers to the next-step question at each step, where one
    /// was asked; empty when none were. Read by [`Scenario::HabitThenOracle`].
    pub oracle_steps: Vec<Option<OracleStep>>,
    /// System-One answers for each step's closed-set arguments, where asked.
    pub oracle_args: Vec<Option<OracleArgs>>,
}

/// Contexts where the habit may act, validated by cross-validation.
#[derive(Clone, Debug, Default)]
pub struct ValidatedContexts {
    order: usize,
    keys: HashMap<Vec<Symbol>, ContextRecord>,
}

/// How the habit did in one context under cross-validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ContextRecord {
    /// Held-out decisions in the context.
    pub n: usize,
    /// Those where the habit's top option matched the agent.
    pub agreed: usize,
    /// Distinct tasks those decisions came from.
    pub tasks: usize,
    /// The agent's most common action there.
    pub action: u32,
}

impl ValidatedContexts {
    /// Number of validated contexts.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no context was validated.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Whether the habit may act at step `t` of `ep`.
    pub fn allows(&self, ep: &EncodedEpisode, t: usize) -> bool {
        self.keys
            .contains_key(&BackoffModel::context(&ep.symbols[..t], self.order))
    }

    /// The context at step `t` of `ep`, if validated.
    pub fn context_at(&self, ep: &EncodedEpisode, t: usize) -> Option<Vec<Symbol>> {
        let c = BackoffModel::context(&ep.symbols[..t], self.order);
        self.keys.contains_key(&c).then_some(c)
    }

    /// Validated contexts and their records, most decisions first.
    pub fn records(&self) -> Vec<(&[Symbol], ContextRecord)> {
        let mut v: Vec<_> = self.keys.iter().map(|(k, r)| (k.as_slice(), *r)).collect();
        v.sort_by(|a, b| b.1.n.cmp(&a.1.n).then_with(|| a.0.cmp(b.0)));
        v
    }
}

/// Validate contexts by `folds`-fold cross-validation grouped by `train`'s
/// first element (the task): a context is kept when the held-out top-1
/// agreement of a [`GroupedModel`] fitted on the other folds is at least
/// `min_agreement` over at least `min_n` held-out decisions from at least
/// `min_tasks` distinct tasks.
///
/// The task count matters more than the decision count: every task repeats
/// across trials and agent models, so its decisions are near-copies, and a
/// context seen in two tasks can reach dozens of decisions.
#[allow(clippy::too_many_arguments)]
pub fn validated_contexts(
    train: &[(u64, EncodedEpisode)],
    order: usize,
    alpha: f64,
    vocab_size: usize,
    folds: u64,
    min_n: usize,
    min_tasks: usize,
    min_agreement: f64,
) -> ValidatedContexts {
    #[derive(Default)]
    struct Tally {
        n: usize,
        agreed: usize,
        actions: HashMap<u32, usize>,
        tasks: HashSet<u64>,
    }
    let mut stats: HashMap<Vec<Symbol>, Tally> = HashMap::new();
    for fold in 0..folds {
        let fit: Vec<EncodedEpisode> = train
            .iter()
            .filter(|(g, _)| g % folds != fold)
            .map(|(_, e)| e.clone())
            .collect();
        let model = GroupedModel::fit(order, alpha, alpha, vocab_size, &fit);
        for (task, ep) in train.iter().filter(|(g, _)| g % folds == fold) {
            for t in 0..ep.actions.len() {
                let top = argmax(&model.predict_at(ep, t));
                let e = stats
                    .entry(BackoffModel::context(&ep.symbols[..t], order))
                    .or_default();
                e.n += 1;
                e.agreed += (top == ep.actions[t] as usize) as usize;
                *e.actions.entry(ep.actions[t]).or_insert(0) += 1;
                e.tasks.insert(*task);
            }
        }
    }
    ValidatedContexts {
        order,
        keys: stats
            .into_iter()
            .filter(|(_, e)| {
                e.n >= min_n
                    && e.tasks.len() >= min_tasks
                    && e.agreed as f64 >= min_agreement * e.n as f64
            })
            .map(|(k, e)| {
                let action = e
                    .actions
                    .into_iter()
                    .max_by_key(|&(a, c)| (c, std::cmp::Reverse(a)))
                    .map_or(0, |(a, _)| a);
                let record = ContextRecord {
                    n: e.n,
                    agreed: e.agreed,
                    tasks: e.tasks.len(),
                    action,
                };
                (k, record)
            })
            .collect(),
    }
}

/// Dollars per input and per output token, by least squares over recorded
/// turns (`cost ≈ a · prompt + b · completion`).
pub fn fit_prices(turns: impl IntoIterator<Item = TurnUsage>) -> (f64, f64) {
    let (mut spp, mut spc, mut scc, mut syp, mut syc) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for u in turns {
        let (p, c, y) = (u.prompt_tokens as f64, u.completion_tokens as f64, u.cost);
        spp += p * p;
        spc += p * c;
        scc += c * c;
        syp += y * p;
        syc += y * c;
    }
    let det = spp * scc - spc * spc;
    if det.abs() < 1e-9 {
        return (if spp > 0.0 { syp / spp } else { 0.0 }, 0.0);
    }
    ((syp * scc - syc * spc) / det, (syc * spp - syp * spc) / det)
}

/// When the habit may act.
#[derive(Clone, Copy, Debug)]
pub enum Gate<'a> {
    /// When its top option has at least this probability (and enough
    /// evidence).
    Threshold(f64),
    /// Only in validated contexts.
    Validated(&'a ValidatedContexts),
}

/// A serializable description of a [`Gate`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum GateKind {
    /// Global confidence threshold.
    Threshold(f64),
    /// Per-context validation, with the number of validated contexts.
    Validated(usize),
}

/// Totals from replaying episodes under one scenario and gate.
#[derive(Clone, Debug, Serialize)]
pub struct Projection {
    /// Who resolves decisions the habit may not take.
    pub scenario: Scenario,
    /// When the habit may act.
    pub gate: GateKind,
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
    /// Habit decisions to hand back while the agent carried on: safe, but
    /// the LLM takes the step (a pause).
    pub early_stops: usize,
    /// Habit decisions that chose a different tool than the agent, or carried
    /// on when the agent stopped: the risk.
    pub disagreements: usize,
    /// System-One decisions to hand back while the agent carried on.
    pub oracle_early_stops: usize,
    /// System-One decisions that chose another tool, carried on when the
    /// agent stopped, or picked another argument value: the risk.
    pub oracle_disagreements: usize,
    /// Episodes with at least one risky decision, by the habit or System-One.
    pub episodes_with_disagreement: usize,
    /// Whether flows only read ([`Design::ReadOnly`]).
    pub read_only: bool,
    /// Steps a read-only flow hands back because the agent's next step is a
    /// write (or not known to be read-only); each is a pause.
    pub handoffs: usize,
    /// Read-only habit decisions that made a lookup the agent did not: safe,
    /// but a pause.
    pub detours: usize,
    /// Read-only System-One decisions that made a lookup the agent did not,
    /// or passed a lookup another argument value.
    pub oracle_detours: usize,
    /// Episodes with at least one detour.
    pub episodes_with_detour: usize,
    /// Input tokens charged for detours (already taken off the savings).
    pub detour_tokens: f64,
    /// Input tokens across all turns.
    pub input_tokens: f64,
    /// Output tokens across all turns.
    pub output_tokens: f64,
    /// Dollars across all turns.
    pub cost: f64,
    /// Input tokens saved: removed turns, plus intermediate tool outputs of
    /// collapsed runs no longer carried in later prompts.
    pub input_saved: f64,
    /// Output tokens saved by removed turns.
    pub output_saved: f64,
    /// Dollars saved.
    pub cost_saved: f64,
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

    /// Share of episodes with at least one detour.
    pub fn detour_share(&self) -> f64 {
        self.episodes_with_detour as f64 / self.episodes.max(1) as f64
    }

    /// Input tokens saved as a share of all input tokens.
    pub fn input_saved_share(&self) -> f64 {
        self.input_saved / self.input_tokens.max(1.0)
    }

    /// Output tokens saved as a share of all output tokens.
    pub fn output_saved_share(&self) -> f64 {
        self.output_saved / self.output_tokens.max(1.0)
    }

    /// Dollars saved as a share of all dollars.
    pub fn cost_saved_share(&self) -> f64 {
        if self.cost > 0.0 {
            self.cost_saved / self.cost
        } else {
            0.0
        }
    }
}

enum Verdict {
    /// The habit decided and agreed with the agent.
    Habit,
    /// The habit decided to hand back while the agent carried on.
    EarlyStop,
    /// The habit decided and disagreed.
    Wrong,
    /// The System-One model decided and agreed with the agent.
    Oracle,
    /// The System-One model decided to hand back while the agent carried on.
    OracleEarlyStop,
    /// The System-One model decided and disagreed.
    OracleWrong,
    /// A read-only habit decision made a lookup the agent did not.
    Detour,
    /// A read-only System-One decision made a lookup the agent did not.
    OracleDetour,
    /// The agent's next step is a write: a read-only flow hands it back.
    Handoff,
    /// Nobody inside the flow could decide.
    Pause,
}

/// A run's outcome, before token accounting.
struct RunPlan {
    /// Assistant turns the run spans, in order.
    turns: Vec<usize>,
    /// Turns the flow still needs (`1 + pauses`, capped at the run's turns).
    kept: usize,
}

/// Replay `inputs` under `scenario` and `gate`, with flows that may call any
/// tool.
pub fn project(
    model: &impl Predictor,
    inputs: &[ProjectionInput<'_>],
    scenario: Scenario,
    gate: Gate<'_>,
    min_evidence: f64,
) -> Projection {
    project_design(model, inputs, scenario, gate, min_evidence, Design::AnyTool)
}

/// Replay `inputs` under `scenario` and `gate`, with flows built to `design`.
pub fn project_design(
    model: &impl Predictor,
    inputs: &[ProjectionInput<'_>],
    scenario: Scenario,
    gate: Gate<'_>,
    min_evidence: f64,
    design: Design<'_>,
) -> Projection {
    let (read_only, detour_tokens) = match design {
        Design::ReadOnly { detour_tokens, .. } => (true, detour_tokens),
        Design::AnyTool => (false, 0.0),
    };
    // Whether a read-only flow must hand this action back.
    let high = |a: u32| matches!(design, Design::ReadOnly { high, .. } if high.contains(&a));
    let mut p = Projection {
        scenario,
        gate: match gate {
            Gate::Threshold(t) => GateKind::Threshold(t),
            Gate::Validated(v) => GateKind::Validated(v.len()),
        },
        episodes: inputs.len(),
        assistant_turns: 0,
        run_turns: 0,
        turns_saved: 0,
        ceiling: 0,
        pauses: 0,
        habit_decisions: 0,
        oracle_decisions: 0,
        early_stops: 0,
        disagreements: 0,
        oracle_early_stops: 0,
        oracle_disagreements: 0,
        episodes_with_disagreement: 0,
        read_only,
        handoffs: 0,
        detours: 0,
        oracle_detours: 0,
        episodes_with_detour: 0,
        detour_tokens: 0.0,
        input_tokens: 0.0,
        output_tokens: 0.0,
        cost: 0.0,
        input_saved: 0.0,
        output_saved: 0.0,
        cost_saved: 0.0,
    };
    for input in inputs {
        let ep = input.encoded;
        let n = ep.actions.len();
        let is_tool = |k: usize| ep.actions[k] != 0; // id 0 is Respond
        let decide = |k: usize| -> Verdict {
            let actual = ep.actions[k];
            if high(actual) {
                return Verdict::Handoff;
            }
            // What the flow does with a pick: a read-only flow hands back
            // instead of writing.
            let flow = |a: u32| if high(a) { 0 } else { a };
            let judge = |pick: u32, by_habit: bool| {
                let pick = flow(pick);
                match (pick == actual, pick == 0, read_only, by_habit) {
                    (true, _, _, true) => Verdict::Habit,
                    (true, _, _, false) => Verdict::Oracle,
                    (_, true, _, true) => Verdict::EarlyStop,
                    (_, true, _, false) => Verdict::OracleEarlyStop,
                    (_, _, true, true) => Verdict::Detour,
                    (_, _, true, false) => Verdict::OracleDetour,
                    (_, _, false, true) => Verdict::Wrong,
                    (_, _, false, false) => Verdict::OracleWrong,
                }
            };
            let probs = model.predict_at(ep, k);
            let top = argmax(&probs);
            let may_act = match gate {
                Gate::Threshold(t) => model.evidence_at(ep, k) >= min_evidence && probs[top] >= t,
                Gate::Validated(v) => v.allows(ep, k),
            };
            if may_act {
                judge(top as u32, true)
            } else {
                match scenario {
                    Scenario::HabitOnly => Verdict::Pause,
                    Scenario::HabitThenPerfectOracle => Verdict::Oracle,
                    Scenario::HabitThenOracle(t)
                    | Scenario::TwoKeys(t)
                    | Scenario::Arbitrated(t)
                    | Scenario::LookupFirst(t) => {
                        let both = |o: OracleStep| {
                            !matches!(scenario, Scenario::TwoKeys(_))
                                || flow(o.top) == flow(top as u32)
                        };
                        match input.oracle_steps.get(k).copied().flatten() {
                            Some(o) if o.prob >= t && both(o) => judge(o.top, false),
                            _ => Verdict::Pause,
                        }
                    }
                }
            }
        };

        let mut plans: Vec<RunPlan> = Vec::new();
        let mut wrong_here = 0;
        // Steps where the flow made a lookup the agent did not.
        let mut detour_at: Vec<usize> = Vec::new();
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
            let mut pauses = 0;
            for k in s + 1..e {
                let mut pause = false;
                let verdict = decide(k);
                let handed_off = matches!(verdict, Verdict::Handoff);
                match verdict {
                    Verdict::Habit => p.habit_decisions += 1,
                    Verdict::EarlyStop => {
                        p.habit_decisions += 1;
                        p.early_stops += 1;
                        pause = true;
                    }
                    Verdict::Wrong => {
                        p.habit_decisions += 1;
                        p.disagreements += 1;
                        wrong_here += 1;
                        pause = true;
                    }
                    Verdict::Oracle => p.oracle_decisions += 1,
                    Verdict::OracleEarlyStop => {
                        p.oracle_decisions += 1;
                        p.oracle_early_stops += 1;
                        pause = true;
                    }
                    Verdict::OracleWrong => {
                        p.oracle_decisions += 1;
                        p.oracle_disagreements += 1;
                        wrong_here += 1;
                        pause = true;
                    }
                    Verdict::Detour => {
                        p.habit_decisions += 1;
                        p.detours += 1;
                        detour_at.push(k);
                        pause = true;
                    }
                    Verdict::OracleDetour => {
                        p.oracle_decisions += 1;
                        p.oracle_detours += 1;
                        detour_at.push(k);
                        pause = true;
                    }
                    Verdict::Handoff => {
                        p.handoffs += 1;
                        pause = true;
                    }
                    Verdict::Pause => pause = true,
                }
                // A handed-off call is the LLM's, arguments included.
                let need = if handed_off {
                    ArgNeed::Bound
                } else {
                    input.args[k]
                };
                match (need, scenario) {
                    (ArgNeed::Bound, _) => {}
                    (ArgNeed::ClosedSet, Scenario::HabitThenPerfectOracle) => {
                        p.oracle_decisions += 1
                    }
                    (
                        ArgNeed::ClosedSet,
                        Scenario::HabitThenOracle(t)
                        | Scenario::TwoKeys(t)
                        | Scenario::Arbitrated(t)
                        | Scenario::LookupFirst(t),
                    ) => match input.oracle_args.get(k).copied().flatten() {
                        Some(a) if a.prob >= t => {
                            p.oracle_decisions += 1;
                            if !a.agrees {
                                // A read with another argument value looks
                                // up something else: a detour.
                                if read_only {
                                    p.oracle_detours += 1;
                                    detour_at.push(k);
                                } else {
                                    p.oracle_disagreements += 1;
                                    wrong_here += 1;
                                }
                                pause = true;
                            }
                        }
                        _ => pause = true,
                    },
                    (ArgNeed::ClosedSet, Scenario::HabitOnly) | (ArgNeed::Llm, _) => pause = true,
                }
                pauses += pause as usize;
            }
            if e < n {
                // The stop decision: free to hand back, but a confident wrong
                // "continue" is still a risk.
                match decide(e) {
                    // The agent stopped here, so the habit cannot stop early.
                    Verdict::Habit | Verdict::EarlyStop => p.habit_decisions += 1,
                    Verdict::Wrong => {
                        p.habit_decisions += 1;
                        p.disagreements += 1;
                        wrong_here += 1;
                    }
                    Verdict::Oracle | Verdict::OracleEarlyStop => p.oracle_decisions += 1,
                    Verdict::OracleWrong => {
                        p.oracle_decisions += 1;
                        p.oracle_disagreements += 1;
                        wrong_here += 1;
                    }
                    // One lookup too many before handing back.
                    Verdict::Detour => {
                        p.habit_decisions += 1;
                        p.detours += 1;
                        detour_at.push(e);
                    }
                    Verdict::OracleDetour => {
                        p.oracle_decisions += 1;
                        p.oracle_detours += 1;
                        detour_at.push(e);
                    }
                    // A reply is never a write.
                    Verdict::Handoff | Verdict::Pause => {}
                }
            }
            let t = turns.len();
            p.run_turns += t;
            p.ceiling += t - 1;
            p.pauses += pauses;
            let kept = t.min(1 + pauses);
            p.turns_saved += t - kept;
            plans.push(RunPlan { turns, kept });
            j = e;
        }
        p.episodes_with_disagreement += (wrong_here > 0) as usize;
        p.episodes_with_detour += (!detour_at.is_empty()) as usize;
        p.assistant_turns += input.usage.len();
        account_tokens(&mut p, input, &plans, &detour_at, detour_tokens);
    }
    p
}

/// Token and dollar accounting for one episode's runs.
///
/// A removed turn saves its whole prompt, completion and cost. A run that
/// collapses to one call also stops carrying its intermediate tool outputs
/// (all but the last) in every later prompt. Their size is read off the
/// provider's own counts: the growth of the prompt between consecutive turns
/// of the run, net of the completion in between. A detour at a step adds a
/// typical lookup's output (`detour_tokens`) to that step's prompt and every
/// later one.
fn account_tokens(
    p: &mut Projection,
    input: &ProjectionInput<'_>,
    plans: &[RunPlan],
    detour_at: &[usize],
    detour_tokens: f64,
) {
    let u = &input.usage;
    for x in u {
        p.input_tokens += x.prompt_tokens as f64;
        p.output_tokens += x.completion_tokens as f64;
        p.cost += x.cost;
    }
    let mut removed = vec![false; u.len()];
    for plan in plans {
        for &turn in &plan.turns[plan.kept..] {
            if turn < u.len() {
                removed[turn] = true;
            }
        }
    }
    for (turn, gone) in removed.iter().enumerate() {
        if *gone {
            p.input_saved += u[turn].prompt_tokens as f64;
            p.output_saved += u[turn].completion_tokens as f64;
            p.cost_saved += u[turn].cost;
        }
    }
    for plan in plans
        .iter()
        .filter(|plan| plan.kept == 1 && plan.turns.len() > 1)
    {
        let dropped: f64 = plan
            .turns
            .windows(2)
            .filter(|w| w[1] < u.len())
            .map(|w| {
                let (a, b) = (u[w[0]], u[w[1]]);
                (b.prompt_tokens as f64 - a.prompt_tokens as f64 - a.completion_tokens as f64)
                    .max(0.0)
            })
            .sum();
        let last = *plan.turns.last().expect("runs are non-empty");
        let later = (last + 1..u.len()).filter(|&turn| !removed[turn]).count() as f64;
        p.input_saved += dropped * later;
        p.cost_saved += dropped * later * input.input_price;
    }
    for &k in detour_at {
        let from = input.turns.get(k).copied().unwrap_or(u.len());
        let later = (from..u.len()).filter(|&turn| !removed[turn]).count() as f64;
        let extra = detour_tokens * later;
        p.detour_tokens += extra;
        p.input_saved -= extra;
        p.cost_saved -= extra * input.input_price;
    }
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
        // Prompts grow by 100 tokens per tool output; completions are 10.
        let usage: Vec<TurnUsage> = (0..5)
            .map(|i| TurnUsage {
                prompt_tokens: 1000 + 110 * i,
                completion_tokens: 10,
                cost: 0.01,
            })
            .collect();
        let input = ProjectionInput {
            encoded: &enc[0],
            turns: vec![0, 1, 2, 3, 4],
            args: vec![ArgNeed::Bound; 5],
            usage,
            input_price: 1e-5,
            oracle_steps: vec![],
            oracle_args: vec![],
        };
        let p = project(
            &model,
            &[input],
            Scenario::HabitOnly,
            Gate::Threshold(0.8),
            5.0,
        );
        assert_eq!(p.run_turns, 3);
        assert_eq!(p.ceiling, 2);
        assert_eq!(p.turns_saved, 2);
        assert_eq!(p.pauses, 0);
        assert_eq!(p.disagreements, 0);
        assert!((p.saved_share() - 0.4).abs() < 1e-12);
        // Turns 2 and 3 are removed; turn 4 no longer carries the two
        // intermediate outputs (100 tokens each).
        assert_eq!(p.input_saved, (1220 + 1330 + 200) as f64);
        assert_eq!(p.output_saved, 20.0);
        assert!((p.cost_saved - (0.02 + 200.0 * 1e-5)).abs() < 1e-12);
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
            usage: vec![TurnUsage::default(); 5],
            input_price: 0.0,
            oracle_steps: vec![],
            oracle_args: vec![],
        };
        let habit_only = project(
            &empty,
            &[mk(vec![ArgNeed::Bound; 5])],
            Scenario::HabitOnly,
            Gate::Threshold(0.8),
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
            Gate::Threshold(0.8),
            0.0,
        );
        // b's closed-set argument goes to the oracle; c's generated one pauses.
        assert_eq!(oracle.pauses, 1);
        assert_eq!(oracle.turns_saved, 1);
        assert_eq!(oracle.oracle_decisions, 2 + 1 + 1);
    }

    #[test]
    fn a_real_oracle_is_trusted_only_above_its_threshold() {
        let steps = vec![reply(), tool("a"), tool("b"), tool("c"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c"]);
        let enc = EncodedEpisode::encode(&steps, &vocab, true);
        let empty = BackoffModel::new(2, 1.0, vocab.len());
        let args = vec![
            ArgNeed::Bound,
            ArgNeed::Bound,
            ArgNeed::ClosedSet,
            ArgNeed::Bound,
            ArgNeed::Bound,
        ];
        let replay = |oracle_steps: Vec<Option<OracleStep>>, agrees: bool, scenario| {
            let mut oracle_args = vec![None; 5];
            oracle_args[2] = Some(OracleArgs { agrees, prob: 0.9 });
            project(
                &empty,
                &[ProjectionInput {
                    encoded: &enc,
                    turns: vec![0, 1, 2, 3, 4],
                    args: args.clone(),
                    usage: vec![TurnUsage::default(); 5],
                    input_price: 0.0,
                    oracle_steps,
                    oracle_args,
                }],
                scenario,
                Gate::Threshold(0.8),
                0.0,
            )
        };
        let truth = |p: f64| -> Vec<Option<OracleStep>> {
            enc.actions
                .iter()
                .map(|&top| Some(OracleStep { top, prob: p }))
                .collect()
        };
        // Confident and always right: exactly the perfect-oracle upper bound.
        let perfect = replay(vec![], true, Scenario::HabitThenPerfectOracle);
        let real = replay(truth(0.95), true, Scenario::HabitThenOracle(0.9));
        assert_eq!(
            (real.turns_saved, real.pauses, real.oracle_decisions),
            (
                perfect.turns_saved,
                perfect.pauses,
                perfect.oracle_decisions
            )
        );
        assert_eq!(real.oracle_disagreements, 0);
        // Unsure: every decision pauses, as with no System-One model at all.
        let unsure = replay(truth(0.5), true, Scenario::HabitThenOracle(0.9));
        assert_eq!((unsure.turns_saved, unsure.oracle_decisions), (0, 1));
        // Confident but wrong: after a, it calls c instead of b, and it gets
        // b's argument wrong. Both are risks; being the same step, they cost
        // one pause, so the three-turn run still loses one turn.
        let mut wrong = truth(0.95);
        wrong[2] = Some(OracleStep {
            top: enc.actions[3],
            prob: 0.95,
        });
        let wrong = replay(wrong, false, Scenario::HabitThenOracle(0.9));
        assert_eq!(wrong.oracle_disagreements, 2);
        assert_eq!(wrong.episodes_with_disagreement, 1);
        assert_eq!((wrong.pauses, wrong.turns_saved), (1, 1));
        // Handing back early is safe.
        let mut early = truth(0.95);
        early[3] = Some(OracleStep { top: 0, prob: 0.95 });
        let early = replay(early, true, Scenario::HabitThenOracle(0.9));
        assert_eq!(
            (early.oracle_early_stops, early.oracle_disagreements),
            (1, 0)
        );
        assert_eq!(early.turns_saved, 1);
    }

    #[test]
    fn two_keys_need_the_habit_to_agree() {
        // The habit has learned a → b → c; the oracle is confident everywhere
        // but, after a, picks c.
        let steps = vec![reply(), tool("a"), tool("b"), tool("c"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c"]);
        let enc: Vec<EncodedEpisode> = (0..30)
            .map(|_| EncodedEpisode::encode(&steps, &vocab, true))
            .collect();
        let habit = BackoffModel::fit(2, 0.5, vocab.len(), &enc);
        let mut picks: Vec<Option<OracleStep>> = enc[0]
            .actions
            .iter()
            .map(|&top| Some(OracleStep { top, prob: 0.95 }))
            .collect();
        picks[2] = Some(OracleStep {
            top: enc[0].actions[3],
            prob: 0.95,
        });
        let replay = |scenario| {
            project(
                &habit,
                &[ProjectionInput {
                    encoded: &enc[0],
                    turns: vec![0, 1, 2, 3, 4],
                    args: vec![ArgNeed::Bound; 5],
                    usage: vec![TurnUsage::default(); 5],
                    input_price: 0.0,
                    oracle_steps: picks.clone(),
                    oracle_args: vec![],
                }],
                scenario,
                // No validated contexts: every decision goes past the habit.
                Gate::Threshold(2.0),
                0.0,
            )
        };
        let alone = replay(Scenario::HabitThenOracle(0.9));
        assert_eq!(alone.oracle_disagreements, 1);
        // With two keys the wrong pick disagrees with the habit, so the flow
        // pauses there instead; the rest still goes through.
        let two = replay(Scenario::TwoKeys(0.9));
        assert_eq!(two.oracle_disagreements, 0);
        assert_eq!((two.pauses, two.turns_saved), (1, 1));
    }

    #[test]
    fn prices_are_recovered_from_costs() {
        let turns = (1..50).map(|i| TurnUsage {
            prompt_tokens: 1000 + 37 * i,
            completion_tokens: 20 + (i * 7) % 50,
            cost: 3e-6 * (1000 + 37 * i) as f64 + 1.5e-5 * (20 + (i * 7) % 50) as f64,
        });
        let (a, b) = fit_prices(turns);
        assert!(
            (a - 3e-6).abs() < 1e-12 && (b - 1.5e-5).abs() < 1e-12,
            "{a} {b}"
        );
    }

    #[test]
    fn validated_contexts_admit_only_reliable_ones() {
        // After 1 the agent always does 2; after 3 it flips between 1 and 2.
        let vocab = Vocab::build(std::iter::empty(), ["a", "b", "c"]);
        let steady = EncodedEpisode {
            actions: vec![1, 2],
            outcomes: vec![0, 0],
            symbols: vec![4, 8],
            success: true,
            group: 0,
        };
        let fickle = |a: u32| EncodedEpisode {
            actions: vec![3, a],
            outcomes: vec![0, 0],
            symbols: vec![12, a * 4],
            success: true,
            group: 0,
        };
        let train: Vec<(u64, EncodedEpisode)> = (0..60u64)
            .flat_map(|i| [(i, steady.clone()), (i, fickle(1 + (i % 2) as u32))])
            .collect();
        let v = validated_contexts(&train, 1, 0.5, vocab.len(), 5, 20, 10, 0.99);
        assert!(v.allows(&steady, 1));
        assert!(!v.allows(&fickle(1), 1));
        assert_eq!(v.context_at(&steady, 1), Some(vec![4]));
        let records = v.records();
        assert_eq!(records.len(), 1);
        let (context, r) = records[0];
        assert_eq!(context, &[4]);
        assert_eq!((r.n, r.agreed, r.tasks, r.action), (60, 60, 60, 2));
        // The same decisions from only three tasks do not validate.
        let few: Vec<(u64, EncodedEpisode)> = (0..60u64)
            .flat_map(|i| [(i % 3, steady.clone()), (i % 3, fickle(1 + (i % 2) as u32))])
            .collect();
        let v = validated_contexts(&few, 1, 0.5, vocab.len(), 3, 20, 10, 0.99);
        assert!(!v.allows(&steady, 1));
    }

    #[test]
    fn early_stops_are_safe_and_other_calls_are_risky() {
        // The agent always runs a → b, then replies.
        let train = vec![reply(), tool("a"), tool("b"), reply()];
        let vocab = Vocab::build(train.iter(), ["a", "b", "c"]);
        let enc: Vec<EncodedEpisode> = (0..30)
            .map(|_| EncodedEpisode::encode(&train, &vocab, true))
            .collect();
        let model = BackoffModel::fit(2, 0.5, vocab.len(), &enc);
        let replay = |steps: Vec<Step>| {
            let enc = EncodedEpisode::encode(&steps, &vocab, true);
            let n = steps.len();
            project(
                &model,
                &[ProjectionInput {
                    encoded: &enc,
                    turns: (0..n).collect(),
                    args: vec![ArgNeed::Bound; n],
                    usage: vec![TurnUsage::default(); n],
                    input_price: 0.0,
                    oracle_steps: vec![],
                    oracle_args: vec![],
                }],
                Scenario::HabitOnly,
                Gate::Threshold(0.8),
                5.0,
            )
        };
        // After a, the habit calls b but the agent called c: a risk.
        let other = replay(vec![reply(), tool("a"), tool("c"), reply()]);
        assert_eq!((other.disagreements, other.early_stops), (1, 0));
        assert_eq!(other.episodes_with_disagreement, 1);
        assert_eq!(other.pauses, 1);
        // After a → b, the habit hands back but the agent went on to c: safe,
        // and the LLM takes that step.
        let early = replay(vec![reply(), tool("a"), tool("b"), tool("c"), reply()]);
        assert_eq!((early.disagreements, early.early_stops), (0, 1));
        assert_eq!(early.episodes_with_disagreement, 0);
        assert_eq!(early.pauses, 1);
        assert_eq!(early.turns_saved, 1);
    }

    #[test]
    fn read_only_flows_hand_writes_back_and_detour_instead_of_risking() {
        // The agent looks up a, then b, then writes w, then replies.
        let steps = vec![reply(), tool("a"), tool("b"), tool("w"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c", "w"]);
        let enc = EncodedEpisode::encode(&steps, &vocab, true);
        let empty = BackoffModel::new(2, 1.0, vocab.len());
        let (c, w) = (
            vocab.id(&Action::Tool("c".into())),
            vocab.id(&Action::Tool("w".into())),
        );
        let high = HashSet::from([w]);
        let ro_design = Design::ReadOnly {
            high: &high,
            detour_tokens: 0.0,
        };
        let replay = |oracle_steps: Vec<Option<OracleStep>>, scenario, design| {
            project_design(
                &empty,
                &[ProjectionInput {
                    encoded: &enc,
                    turns: vec![0, 1, 2, 3, 4],
                    args: vec![ArgNeed::Bound; 5],
                    usage: vec![TurnUsage::default(); 5],
                    input_price: 0.0,
                    oracle_steps,
                    oracle_args: vec![],
                }],
                scenario,
                Gate::Threshold(2.0),
                0.0,
                design,
            )
        };
        // With a perfect System-One model, any-tool flows run a → b → w in
        // one turn; read-only flows hand w back to the LLM.
        let any = replay(vec![], Scenario::HabitThenPerfectOracle, Design::AnyTool);
        assert_eq!((any.turns_saved, any.pauses, any.handoffs), (2, 0, 0));
        let ro = replay(vec![], Scenario::HabitThenPerfectOracle, ro_design);
        assert_eq!((ro.turns_saved, ro.pauses, ro.handoffs), (1, 1, 1));
        assert!(ro.read_only && !any.read_only);
        // A System-One model that looks up c where the agent looked up b, and
        // picks w where the agent replied.
        let picks = vec![
            None,
            None,
            Some(OracleStep { top: c, prob: 0.95 }),
            None,
            Some(OracleStep { top: w, prob: 0.95 }),
        ];
        // Any tool: both are risks.
        let any = replay(
            picks.clone(),
            Scenario::HabitThenOracle(0.9),
            Design::AnyTool,
        );
        assert_eq!(any.oracle_disagreements, 2);
        assert_eq!(any.episodes_with_disagreement, 1);
        // Read only: c is a detour, and w becomes the hand-back the agent made.
        let ro = replay(picks.clone(), Scenario::HabitThenOracle(0.9), ro_design);
        assert_eq!(
            (ro.oracle_disagreements, ro.oracle_detours, ro.handoffs),
            (0, 1, 1)
        );
        assert_eq!(
            (ro.episodes_with_disagreement, ro.episodes_with_detour),
            (0, 1)
        );
        assert!((ro.detour_share() - 1.0).abs() < 1e-12);
        // Arbitrated answers are trusted the same way.
        let arb = replay(picks, Scenario::Arbitrated(0.9), ro_design);
        assert_eq!(
            (arb.oracle_detours, arb.pauses, arb.turns_saved),
            (ro.oracle_detours, ro.pauses, ro.turns_saved)
        );
    }

    #[test]
    fn detours_are_charged_their_output_in_later_prompts() {
        // The agent looks up a, then b, then replies; the flow looks up c
        // instead of b, with probability 0.35.
        let steps = vec![reply(), tool("a"), tool("b"), reply()];
        let vocab = Vocab::build(steps.iter(), ["a", "b", "c"]);
        let enc = EncodedEpisode::encode(&steps, &vocab, true);
        let empty = BackoffModel::new(2, 1.0, vocab.len());
        let c = vocab.id(&Action::Tool("c".into()));
        let high = HashSet::new();
        let design = Design::ReadOnly {
            high: &high,
            detour_tokens: 100.0,
        };
        let replay = |scenario| {
            project_design(
                &empty,
                &[ProjectionInput {
                    encoded: &enc,
                    turns: vec![0, 1, 2, 3],
                    args: vec![ArgNeed::Bound; 4],
                    usage: vec![
                        TurnUsage {
                            prompt_tokens: 1000,
                            completion_tokens: 10,
                            cost: 0.0,
                        };
                        4
                    ],
                    input_price: 1e-5,
                    oracle_steps: vec![None, None, Some(OracleStep { top: c, prob: 0.35 }), None],
                    oracle_args: vec![],
                }],
                scenario,
                Gate::Threshold(2.0),
                0.0,
                design,
            )
        };
        // Trusted from 0.3: a detour at step 2, whose output rides along in
        // the prompts of turns 2 and 3.
        let lenient = replay(Scenario::LookupFirst(0.3));
        assert_eq!((lenient.oracle_detours, lenient.pauses), (1, 1));
        assert_eq!(lenient.detour_tokens, 200.0);
        assert_eq!(lenient.input_saved, -200.0);
        assert!((lenient.cost_saved + 200.0 * 1e-5).abs() < 1e-12);
        // From 0.4 the flow hands back instead: the same pause, no detour.
        let strict = replay(Scenario::LookupFirst(0.4));
        assert_eq!((strict.oracle_detours, strict.pauses), (0, 1));
        assert_eq!(strict.detour_tokens, 0.0);
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
