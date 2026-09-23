//! Phase 0b: System-One questions at held-out decisions (replayed shadow mode).
//!
//! Every decision a compiled flow would hand to a System-One model becomes a
//! typed question about the state the flow would have at that point:
//!
//! - **Next step.** Right after a tool returns: which tool the agent calls
//!   next, or [`RESPOND`] (hand back to the LLM). A `Choice` over the domain's
//!   tools plus "respond", each described by its docstring.
//! - **Closed-set arguments.** For a tool call inside a run whose argument
//!   takes one of a few values (a cancellation reason, a cabin class): a
//!   `Choice` over the values seen in successful training episodes.
//!
//! The state is a transcript of the episode so far (the customer's first
//! message, then the most recent messages, tool calls and results, clipped),
//! plus the flow's goal: the write operations the episode goes on to make,
//! standing in for what the LLM names when it calls a macro-tool.
//!
//! Answers are scored for agreement with the agent and calibration, and fed
//! back into [`stretto_model::projection`].

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use stretto_model::projection::{ArgNeed, OracleArgs, OracleStep};
use stretto_model::provenance::Source;
use stretto_model::{Action, Step, Vocab};
use stretto_oracle::{request_key, Answer, MockOracle, Oracle, Question, Request, Response};
use stretto_trace::{Episode, Event, ToolManifest};

/// The option that hands the decision back to the LLM.
pub const RESPOND: &str = "respond";

/// Dollars per million input tokens (TypeSafe's published Jev price).
pub const PRICE_PER_MTOK: f64 = 0.042;

/// Arguments with more distinct training values than this are not asked
/// about: they are not closed sets.
const MAX_ARG_OPTIONS: usize = 12;
/// Transcript items kept before a decision, besides the customer's first
/// message.
const MAX_ITEMS: usize = 12;
const MAX_TEXT: usize = 1000;
const MAX_RESULT: usize = 2500;

const NEXT_INSTRUCTIONS: &str = "A customer-service agent is working on the customer's request \
     using the tools listed as options. It has just received the result of its last tool call. \
     What does the agent do next? Pick the tool it calls next, or 'respond' if it now writes to \
     the customer instead.";
const RESPOND_CRITERION: &str = "Write to the customer instead of calling a tool: ask for \
     missing information or for explicit confirmation, report what was done, or answer a \
     question.";

/// Which System-One oracle answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OracleKind {
    /// A deterministic stand-in: always picks the first option. For
    /// plumbing checks only; free, and not cached.
    Mock,
    /// TypeSafe's Jev, through the replay cache (pays for each distinct
    /// question once).
    Jev,
    /// The replay cache alone: a question not asked before is an error.
    Replay,
}

/// How to run Phase 0b.
#[derive(Clone, Debug)]
pub struct ShadowConfig {
    /// Who answers.
    pub oracle: OracleKind,
    /// Model id to request.
    pub model: String,
    /// Replay cache directory.
    pub cache_dir: PathBuf,
    /// Requests in flight at once.
    pub concurrency: usize,
    /// Ask at most this many distinct questions (a deterministic sample).
    pub limit: Option<usize>,
    /// Refuse to start if the estimated cost of uncached questions exceeds
    /// this many dollars.
    pub budget: f64,
    /// Probabilities at which the System-One pick is trusted, for the
    /// projection.
    pub thresholds: Vec<f64>,
    /// Write every distinct request here as JSON lines, to audit what the
    /// oracle is shown.
    pub dump: Option<PathBuf>,
}

impl ShadowConfig {
    /// Defaults for `oracle`.
    pub fn new(oracle: OracleKind) -> Self {
        Self {
            oracle,
            model: std::env::var("TYPESAFE_DEFAULT_MODEL")
                .unwrap_or_else(|_| "jev-latest".to_string()),
            cache_dir: ".oracle-cache".into(),
            concurrency: 8,
            limit: None,
            budget: 5.0,
            thresholds: vec![0.5, 0.7, 0.9],
            dump: None,
        }
    }

    /// The oracle, ready to share across threads.
    pub fn build(&self) -> Result<Box<dyn Oracle + Sync>> {
        Ok(match self.oracle {
            OracleKind::Mock => Box::new(MockOracle {
                confidence: 0.6,
                noul: 0.5,
            }),
            OracleKind::Jev => Box::new(stretto_oracle::ReplayCache::new(
                &self.cache_dir,
                Some(stretto_oracle::jev::JevClient::from_env()?),
            )),
            OracleKind::Replay => Box::new(stretto_oracle::ReplayCache::<MockOracle>::new(
                &self.cache_dir,
                None,
            )),
        })
    }
}

/// What a question asks about.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Kind {
    /// Which tool comes next, or "respond".
    Next,
    /// The value of one closed-set argument of the step's tool call.
    Arg(String),
}

/// One question at one decision.
#[derive(Clone, Debug)]
pub struct Decision {
    /// Index of the episode among those passed to [`decisions`].
    pub episode: usize,
    /// The step the decision is about.
    pub step: usize,
    /// What is asked.
    pub kind: Kind,
    /// For argument questions, the tool being called.
    pub tool: Option<String>,
    /// The option the agent took.
    pub actual: String,
    /// The request to send, if the decision is asked about.
    pub request: Option<Request>,
    /// The answer when it is certain without asking: an argument that took
    /// a single value in training. With no request and no fixed answer, the
    /// decision cannot be answered (the flow pauses).
    pub fixed: Option<String>,
}

/// A held-out episode, as the question builder needs it.
pub struct ShadowEpisode<'a> {
    /// The episode.
    pub episode: &'a Episode,
    /// Its abstract steps.
    pub steps: &'a [Step],
    /// Argument leaves of each tool call, in call order.
    pub sources: &'a [Vec<(String, Source, String)>],
    /// What each step's arguments need.
    pub needs: &'a [ArgNeed],
    /// The flow's goal, as the LLM would name it.
    pub goal: &'a str,
}

/// Values each `(tool, argument)` took in `episodes`, keyed as the questions
/// key them; arguments with too many values are left out.
pub fn closed_values<'a>(
    episodes: impl IntoIterator<Item = &'a Episode>,
) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut values: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for ep in episodes {
        for call in ep.tool_calls() {
            if let Value::Object(args) = &call.arguments {
                for (arg, v) in args {
                    values
                        .entry((call.name.clone(), arg.clone()))
                        .or_default()
                        .insert(value_key(v));
                }
            }
        }
    }
    values.retain(|_, v| v.len() <= MAX_ARG_OPTIONS);
    values
}

/// Every question Phase 0b asks about `episodes`.
pub fn decisions(
    episodes: &[ShadowEpisode<'_>],
    manifest: &ToolManifest,
    closed: &BTreeMap<(String, String), BTreeSet<String>>,
    model: &str,
) -> Vec<Decision> {
    let next = next_question(manifest);
    let mut out = Vec::new();
    for (i, se) in episodes.iter().enumerate() {
        let (items, step_items) = timeline(se.episode);
        let is_tool = |k: usize| matches!(se.steps[k].action, Action::Tool(_));
        let mut call = 0;
        for k in 0..se.steps.len() {
            let this_call = is_tool(k).then(|| {
                call += 1;
                call - 1
            });
            if k == 0 || !is_tool(k - 1) {
                continue;
            }
            let snapshot = state(&items[..step_items[k]], se.goal, None);
            out.push(Decision {
                episode: i,
                step: k,
                kind: Kind::Next,
                tool: None,
                actual: option_of(&se.steps[k].action),
                request: Some(Request {
                    model: model.to_string(),
                    state: snapshot,
                    questions: BTreeMap::from([("next".to_string(), next.clone())]),
                }),
                fixed: None,
            });
            // Arguments: only for calls inside a run that need nothing but
            // closed-set choices.
            let (Some(c), ArgNeed::ClosedSet) = (this_call, se.needs[k]) else {
                continue;
            };
            let Item::Tool {
                name, arguments, ..
            } = &items[step_items[k]]
            else {
                continue;
            };
            let leaves = se.sources.get(c).map(Vec::as_slice).unwrap_or(&[]);
            let asked: BTreeSet<&str> = leaves
                .iter()
                .filter(|(arg, source, _)| match source {
                    Source::Literal | Source::Short => true,
                    Source::Generated => closed.contains_key(&(name.clone(), arg.clone())),
                    _ => false,
                })
                .map(|(arg, _, _)| arg.as_str())
                .collect();
            for arg in asked {
                let actual = arguments.get(arg).map(value_key).unwrap_or_default();
                let options = closed.get(&(name.clone(), arg.to_string()));
                let fixed = options
                    .filter(|o| o.len() == 1)
                    .and_then(|o| o.iter().next().cloned());
                let request = match options {
                    Some(o) if o.len() >= 2 => {
                        let others: serde_json::Map<String, Value> = match arguments {
                            Value::Object(m) => m
                                .iter()
                                .filter(|(k, _)| k.as_str() != arg)
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                            _ => Default::default(),
                        };
                        let call = json!({"tool": name, "other_arguments": others});
                        Some(Request {
                            model: model.to_string(),
                            state: state(&items[..step_items[k]], se.goal, Some(call)),
                            questions: BTreeMap::from([(
                                "value".to_string(),
                                arg_question(manifest, name, arg, o),
                            )]),
                        })
                    }
                    // One value ever seen: the flow passes it without asking.
                    // None seen: nothing to choose from, so the flow pauses.
                    _ => None,
                };
                out.push(Decision {
                    episode: i,
                    step: k,
                    kind: Kind::Arg(arg.to_string()),
                    tool: Some(name.clone()),
                    actual,
                    request,
                    fixed,
                });
            }
        }
    }
    out
}

/// Answers to [`decisions`], by request key; failures counted, not fatal.
pub struct Asked {
    /// Responses by request key.
    pub responses: HashMap<String, Response>,
    /// Distinct requests.
    pub distinct: usize,
    /// Requests sent (or served from the cache) this run.
    pub attempted: usize,
    /// Requests that failed.
    pub errors: usize,
    /// The first error, if any.
    pub first_error: Option<String>,
}

/// Rough input tokens of a request (four characters per token).
pub fn estimate_tokens(request: &Request) -> u64 {
    serde_json::to_string(request).map_or(0, |s| s.len() as u64 / 4)
}

/// Ask every distinct request in `decisions` (up to `config.limit`, chosen by
/// request key so the sample is stable), `config.concurrency` at a time.
/// Aborts early when most requests fail, which is what a bad key or a
/// missing cache looks like.
pub fn ask(
    oracle: &(dyn Oracle + Sync),
    decisions: &[Decision],
    config: &ShadowConfig,
) -> Result<Asked> {
    let mut unique: BTreeMap<String, &Request> = BTreeMap::new();
    for d in decisions {
        if let Some(r) = &d.request {
            unique.entry(request_key(r)).or_insert(r);
        }
    }
    let distinct = unique.len();
    let mut todo: Vec<(String, &Request)> = unique.into_iter().collect();
    if let Some(limit) = config.limit {
        todo.truncate(limit);
    }
    if let Some(path) = &config.dump {
        let mut lines = String::new();
        for (key, r) in &todo {
            lines.push_str(&serde_json::to_string(&json!({"key": key, "request": r}))?);
            lines.push('\n');
        }
        std::fs::write(path, lines)?;
    }
    let tokens: u64 = todo.iter().map(|(_, r)| estimate_tokens(r)).sum();
    let dollars = tokens as f64 / 1e6 * PRICE_PER_MTOK;
    eprintln!(
        "stretto: {} distinct questions to ask (of {distinct}), about {:.1}M input tokens, \
         at most ${dollars:.2} at Jev's price before cache hits",
        todo.len(),
        tokens as f64 / 1e6
    );
    if config.oracle == OracleKind::Jev && dollars > config.budget {
        bail!(
            "estimated cost ${dollars:.2} exceeds the budget of ${:.2}; raise --oracle-budget \
             or lower --oracle-limit",
            config.budget
        );
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<Result<Response>>>> =
        (0..todo.len()).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..config.concurrency.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= todo.len() || abort.load(Ordering::SeqCst) {
                    break;
                }
                let result = oracle.ask(todo[i].1);
                let fails = failed.load(Ordering::SeqCst) + result.is_err() as usize;
                if result.is_err() {
                    failed.fetch_add(1, Ordering::SeqCst);
                }
                let finished = done.fetch_add(1, Ordering::SeqCst) + 1;
                if fails >= 20 && fails * 2 > finished {
                    abort.store(true, Ordering::SeqCst);
                }
                if finished.is_multiple_of(500) {
                    eprintln!("stretto: {finished}/{} answered", todo.len());
                }
                *slots[i].lock().expect("no thread panics holding a slot") = Some(result);
            });
        }
    });
    let mut asked = Asked {
        responses: HashMap::new(),
        distinct,
        attempted: 0,
        errors: 0,
        first_error: None,
    };
    for ((key, _), slot) in todo.into_iter().zip(slots) {
        match slot.into_inner().expect("no thread panics holding a slot") {
            Some(Ok(r)) => {
                asked.attempted += 1;
                asked.responses.insert(key, r);
            }
            Some(Err(e)) => {
                asked.attempted += 1;
                asked.errors += 1;
                asked.first_error.get_or_insert_with(|| format!("{e:#}"));
            }
            None => {}
        }
    }
    if abort.load(Ordering::SeqCst) {
        bail!(
            "stopped after {} of {} questions failed: {}",
            asked.errors,
            asked.attempted,
            asked.first_error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(asked)
}

/// One answered decision.
#[derive(Clone, Debug)]
pub struct Scored {
    /// The oracle's pick.
    pub pick: String,
    /// The probability it put on its pick.
    pub prob: f64,
    /// The probability it put on the agent's option.
    pub prob_actual: f64,
    /// Brier score of its whole distribution.
    pub brier: f64,
}

/// The oracle's answer to `d`, if it has one.
pub fn score(d: &Decision, asked: &Asked) -> Option<Scored> {
    let Some(request) = &d.request else {
        // Certain without asking, or not answerable at all.
        let only = d.fixed.clone()?;
        let right = only == d.actual;
        return Some(Scored {
            pick: only,
            prob: 1.0,
            prob_actual: if right { 1.0 } else { 0.0 },
            brier: if right { 0.0 } else { 2.0 },
        });
    };
    let response = asked.responses.get(&request_key(request))?;
    let (_, answer) = response.answers.iter().next()?;
    let Answer::Choice {
        choice,
        probabilities,
        ..
    } = answer
    else {
        return None;
    };
    let options: Vec<&String> = match request.questions.values().next()? {
        Question::Choice { criteria, .. } => criteria.keys().collect(),
        _ => return None,
    };
    let p = |o: &str| probabilities.get(o).copied().unwrap_or(0.0);
    let brier = options
        .iter()
        .map(|o| (p(o) - if **o == d.actual { 1.0 } else { 0.0 }).powi(2))
        .sum::<f64>()
        + if options.iter().any(|o| **o == d.actual) {
            0.0
        } else {
            1.0
        };
    Some(Scored {
        pick: choice.clone(),
        prob: p(choice),
        prob_actual: p(&d.actual),
        brier,
    })
}

/// Agreement and calibration over a set of answered decisions.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Agreement {
    /// Decisions answered.
    pub n: usize,
    /// Answers that picked the agent's option.
    pub agreed: usize,
    /// For next-step questions: answers right about stopping versus going on.
    pub stop_agreed: usize,
    /// Mean Brier score of the answer distributions (0 is perfect, 2 worst).
    pub brier: f64,
    /// Mean negative log probability of the agent's option, in nats
    /// (floored at 1e-6).
    pub log_loss: f64,
    /// Expected calibration error of the pick's probability, over ten bins.
    pub ece: f64,
    /// `(threshold, share at or above it, agreement on those)`.
    pub curve: Vec<(f64, f64, f64)>,
}

impl Agreement {
    /// Summarize `(scored, actual)` pairs at `thresholds`.
    pub fn of<'a>(
        answers: impl IntoIterator<Item = (&'a Scored, &'a str)>,
        thresholds: &[f64],
    ) -> Self {
        let answers: Vec<(&Scored, &str)> = answers.into_iter().collect();
        let n = answers.len();
        let mut a = Agreement {
            n,
            ..Default::default()
        };
        if n == 0 {
            return a;
        }
        let mut bins = [(0usize, 0.0f64, 0usize); 10];
        for (s, actual) in &answers {
            let right = s.pick == *actual;
            a.agreed += right as usize;
            a.stop_agreed += ((s.pick == RESPOND) == (*actual == RESPOND)) as usize;
            a.brier += s.brier;
            a.log_loss -= s.prob_actual.max(1e-6).ln();
            let b = ((s.prob * 10.0) as usize).min(9);
            bins[b].0 += 1;
            bins[b].1 += s.prob;
            bins[b].2 += right as usize;
        }
        a.brier /= n as f64;
        a.log_loss /= n as f64;
        a.ece = bins
            .iter()
            .filter(|b| b.0 > 0)
            .map(|&(count, conf, right)| {
                (count as f64 / n as f64)
                    * (conf / count as f64 - right as f64 / count as f64).abs()
            })
            .sum();
        a.curve = thresholds
            .iter()
            .map(|&t| {
                let taken: Vec<_> = answers.iter().filter(|(s, _)| s.prob >= t).collect();
                let right = taken.iter().filter(|(s, a)| s.pick == *a).count();
                (
                    t,
                    taken.len() as f64 / n as f64,
                    if taken.is_empty() {
                        0.0
                    } else {
                        right as f64 / taken.len() as f64
                    },
                )
            })
            .collect();
        a
    }

    /// Share of answers that picked the agent's option.
    pub fn rate(&self) -> f64 {
        self.agreed as f64 / self.n.max(1) as f64
    }
}

/// Per-step oracle answers for one episode, as the projection reads them.
pub fn projection_answers(
    decisions: &[&Decision],
    scored: &[Option<Scored>],
    n_steps: usize,
    vocab: &Vocab,
) -> (Vec<Option<OracleStep>>, Vec<Option<OracleArgs>>) {
    let mut steps = vec![None; n_steps];
    let mut args: Vec<Option<Option<OracleArgs>>> = vec![None; n_steps];
    for (d, s) in decisions.iter().zip(scored) {
        match &d.kind {
            Kind::Next => {
                steps[d.step] = s.as_ref().map(|s| OracleStep {
                    top: vocab.id(&action_of(&s.pick)),
                    prob: s.prob,
                });
            }
            Kind::Arg(_) => {
                // Every argument of the step must be answered; any gap
                // leaves the step unanswered.
                let this = s.as_ref().map(|s| OracleArgs {
                    agrees: s.pick == d.actual,
                    prob: s.prob,
                });
                args[d.step] = Some(match (args[d.step].take(), this) {
                    (None, this) => this,
                    (Some(Some(a)), Some(b)) => Some(OracleArgs {
                        agrees: a.agrees && b.agrees,
                        prob: a.prob.min(b.prob),
                    }),
                    _ => None,
                });
            }
        }
    }
    (steps, args.into_iter().map(Option::flatten).collect())
}

/// The option name for an action.
pub fn option_of(action: &Action) -> String {
    match action {
        Action::Respond => RESPOND.to_string(),
        Action::Tool(name) => name.clone(),
    }
}

fn action_of(option: &str) -> Action {
    if option == RESPOND {
        Action::Respond
    } else {
        Action::Tool(option.to_string())
    }
}

/// How argument values are keyed: strings as themselves, anything else as
/// compact JSON.
fn value_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn next_question(manifest: &ToolManifest) -> Question {
    let mut criteria = BTreeMap::from([(RESPOND.to_string(), RESPOND_CRITERION.to_string())]);
    for name in manifest.tools.keys() {
        let summary = manifest
            .docs
            .get(name)
            .map(|d| clip(&d.summary, 400))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Call {name}."));
        criteria.insert(name.clone(), summary);
    }
    Question::Choice {
        instructions: NEXT_INSTRUCTIONS.to_string(),
        criteria,
    }
}

fn arg_question(
    manifest: &ToolManifest,
    tool: &str,
    arg: &str,
    options: &BTreeSet<String>,
) -> Question {
    let about = manifest
        .docs
        .get(tool)
        .and_then(|d| d.args.get(arg))
        .map(|d| format!(" The argument: {}", clip(d, 400)))
        .unwrap_or_default();
    Question::Choice {
        instructions: format!(
            "The agent is calling the tool `{tool}` now. Which value does it pass for the \
             argument `{arg}`?{about}"
        ),
        criteria: options
            .iter()
            .map(|o| (o.clone(), format!("{arg} = {o}")))
            .collect(),
    }
}

/// One entry of an episode's transcript.
enum Item {
    Customer(String),
    Agent(String),
    Tool {
        name: String,
        arguments: Value,
        result: String,
        error: bool,
    },
}

/// The episode as transcript items, and the item of each step.
fn timeline(ep: &Episode) -> (Vec<Item>, Vec<usize>) {
    let results: HashMap<&str, (&str, bool)> = ep
        .events
        .iter()
        .filter_map(|e| match e {
            Event::ToolResult {
                call_id,
                error,
                content,
                ..
            } => Some((call_id.as_str(), (content.as_str(), *error))),
            _ => None,
        })
        .collect();
    let mut items = Vec::new();
    let mut step_items = Vec::new();
    for e in &ep.events {
        match e {
            Event::User { text } => items.push(Item::Customer(text.clone())),
            Event::Assistant { calls, text, .. } if calls.is_empty() => {
                step_items.push(items.len());
                items.push(Item::Agent(text.clone().unwrap_or_default()));
            }
            Event::Assistant { calls, .. } => {
                for c in calls {
                    step_items.push(items.len());
                    let (result, error) = results
                        .get(c.id.as_str())
                        .copied()
                        .unwrap_or(("(no result recorded)", true));
                    items.push(Item::Tool {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                        result: result.to_string(),
                        error,
                    });
                }
            }
            Event::ToolResult { .. } => {}
        }
    }
    (items, step_items)
}

/// The state a flow would have before a decision: the customer's first
/// message, the most recent transcript items, the goal, and the pending call
/// for argument questions.
fn state(before: &[Item], goal: &str, pending: Option<Value>) -> Value {
    let first = before.iter().find_map(|i| match i {
        Item::Customer(t) => Some(clip(t, MAX_TEXT)),
        _ => None,
    });
    let recent: Vec<Value> = before[before.len().saturating_sub(MAX_ITEMS)..]
        .iter()
        .map(|i| match i {
            Item::Customer(t) => json!({"customer": clip(t, MAX_TEXT)}),
            Item::Agent(t) => json!({"agent": clip(t, MAX_TEXT)}),
            Item::Tool {
                name,
                arguments,
                result,
                error,
            } => {
                let mut v = json!({
                    "tool_call": name,
                    "arguments": arguments,
                    "result": clip(result, MAX_RESULT),
                });
                if *error {
                    v["error"] = json!(true);
                }
                v
            }
        })
        .collect();
    let mut s = json!({
        "customer_request": first,
        "flow_goal": goal,
        "transcript": recent,
    });
    if let Some(p) = pending {
        s["pending_call"] = p;
    }
    s
}

/// `s` cut to at most `max` characters, marked when cut.
fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_model::projection::arg_needs;
    use stretto_model::provenance::call_sources;
    use stretto_model::steps;
    use stretto_trace::{ToolCall, ToolDoc, ToolKind};

    fn episode() -> Episode {
        let call = |id: &str, name: &str, args: Value| Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: id.into(),
                name: name.into(),
                arguments: args,
            }],
            usage: None,
        };
        let result = |id: &str, name: &str, content: &str| Event::ToolResult {
            call_id: id.into(),
            name: name.into(),
            error: false,
            content: content.into(),
        };
        Episode {
            id: "e".into(),
            task_id: "1".into(),
            trial: 0,
            domain: "retail".into(),
            agent_model: "m".into(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "Cancel #W1, I ordered it by mistake".into(),
                },
                call("1", "get_order", json!({"order_id": "#W1"})),
                result(
                    "1",
                    "get_order",
                    r##"{"order_id": "#W1", "status": "pending"}"##,
                ),
                call(
                    "2",
                    "cancel",
                    json!({"order_id": "#W1", "reason": "ordered by mistake"}),
                ),
                result("2", "cancel", r##"{"status": "cancelled"}"##),
                Event::Assistant {
                    text: Some("Done.".into()),
                    calls: vec![],
                    usage: None,
                },
            ],
        }
    }

    fn manifest() -> ToolManifest {
        ToolManifest {
            domain: "retail".into(),
            tools: BTreeMap::from([
                ("get_order".to_string(), ToolKind::Read),
                ("cancel".to_string(), ToolKind::Write),
            ]),
            docs: BTreeMap::from([(
                "cancel".to_string(),
                ToolDoc {
                    summary: "Cancel a pending order.".into(),
                    args: BTreeMap::from([(
                        "reason".to_string(),
                        "Either 'no longer needed' or 'ordered by mistake'.".to_string(),
                    )]),
                },
            )]),
        }
    }

    #[test]
    fn questions_follow_tool_results_and_closed_arguments() {
        let ep = episode();
        let st = steps(&ep);
        let sources = call_sources(&ep);
        // The reason was generated (the user said it differently), and is a
        // closed set in training.
        let closed_pairs =
            std::collections::HashSet::from([("cancel".to_string(), "reason".to_string())]);
        let needs = arg_needs(&st, &sources, &closed_pairs);
        let closed = BTreeMap::from([(
            ("cancel".to_string(), "reason".to_string()),
            BTreeSet::from([
                "no longer needed".to_string(),
                "ordered by mistake".to_string(),
            ]),
        )]);
        let se = ShadowEpisode {
            episode: &ep,
            steps: &st,
            sources: &sources,
            needs: &needs,
            goal: "cancel",
        };
        let ds = decisions(&[se], &manifest(), &closed, "jev-latest");
        let kinds: Vec<(usize, &Kind, &str)> = ds
            .iter()
            .map(|d| (d.step, &d.kind, d.actual.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (1, &Kind::Next, "cancel"),
                (1, &Kind::Arg("reason".into()), "ordered by mistake"),
                (2, &Kind::Next, RESPOND),
            ]
        );
        // The state before the cancel call holds the lookup's result, and the
        // argument question names the pending call without the asked value.
        let arg = ds[1].request.as_ref().unwrap();
        assert_eq!(arg.state["transcript"][1]["tool_call"], "get_order");
        assert_eq!(
            arg.state["pending_call"]["other_arguments"]["order_id"],
            "#W1"
        );
        assert!(arg.state["pending_call"]["other_arguments"]
            .get("reason")
            .is_none());
        match &ds[0].request.as_ref().unwrap().questions["next"] {
            Question::Choice { criteria, .. } => {
                assert_eq!(criteria.len(), 3);
                assert_eq!(criteria["cancel"], "Cancel a pending order.");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mock_answers_are_scored_and_projected() {
        let ep = episode();
        let st = steps(&ep);
        let sources = call_sources(&ep);
        let needs = vec![ArgNeed::Bound; st.len()];
        let se = ShadowEpisode {
            episode: &ep,
            steps: &st,
            sources: &sources,
            needs: &needs,
            goal: "cancel",
        };
        let ds = decisions(&[se], &manifest(), &BTreeMap::new(), "m");
        let config = ShadowConfig::new(OracleKind::Mock);
        let oracle = config.build().unwrap();
        let asked = ask(oracle.as_ref(), &ds, &config).unwrap();
        assert_eq!((asked.distinct, asked.errors), (2, 0));
        // The mock picks the first option in key order: "cancel".
        let scored: Vec<Option<Scored>> = ds.iter().map(|d| score(d, &asked)).collect();
        assert_eq!(scored[0].as_ref().unwrap().pick, "cancel");
        let agreement = Agreement::of(
            ds.iter()
                .zip(&scored)
                .map(|(d, s)| (s.as_ref().unwrap(), d.actual.as_str())),
            &[0.5],
        );
        assert_eq!(
            (agreement.n, agreement.agreed, agreement.stop_agreed),
            (2, 1, 1)
        );
        assert_eq!(agreement.curve, vec![(0.5, 1.0, 0.5)]);
        let vocab = Vocab::build(st.iter(), ["get_order", "cancel"]);
        let refs: Vec<&Decision> = ds.iter().collect();
        let (steps_answers, _) = projection_answers(&refs, &scored, st.len(), &vocab);
        assert_eq!(
            steps_answers[1],
            Some(OracleStep {
                top: vocab.id(&Action::Tool("cancel".into())),
                prob: 0.6
            })
        );
        assert_eq!(steps_answers[0], None);
    }

    #[test]
    fn clip_marks_what_it_cuts() {
        assert_eq!(clip("  abcdef ", 3), "abc…");
        assert_eq!(clip("ab", 3), "ab");
        assert_eq!(clip("ééé", 2), "éé…");
    }
}
