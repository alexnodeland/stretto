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
//!
//! That is the v1 question set. [`QuestionSet::V2`] asks what RFC-001 §3.5
//! specifies instead: flows only read between LLM turns (writes go through
//! plan/commit), so the options at a site are the lookups the agent made
//! there in training plus "hand back"; the state is a slice rather than a
//! transcript; and the stop decision is also asked on its own.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use stretto_model::projection::{ArgNeed, OracleArgs, OracleStep};
use stretto_model::provenance::Source;
use stretto_model::{Action, Outcome, Step, Vocab};
use stretto_oracle::{
    request_key, Answer, MockOracle, NoulCriteria, Oracle, Question, Request, Response,
};
use stretto_trace::{Episode, Event, ToolKind, ToolManifest};

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

/// Latest tool results the v2 state shows in full.
const RECENT_RESULTS: usize = 4;
/// Older lookups the v2 state lists, one line each.
const MAX_EARLIER: usize = 16;
const MAX_RESULT_V2: usize = 1500;
const MAX_MESSAGE_V2: usize = 600;
/// Rubric length the v2 questions keep to.
const MAX_RUBRIC: usize = 250;

const V2_CONTEXT: &str = "A customer-service agent is working on the customer's request. Between \
     its messages to the customer it can look things up with tools. It makes changes \
     (cancellations, modifications, exchanges, returns, bookings) and transfers only after the \
     customer confirms them.";
const HAND_BACK: &str = "Hand back: the agent writes to the customer now (to ask for missing \
     details or a confirmation, report what it found, or answer), or it makes a change or a \
     transfer.";
const GO_ON_YES: &str = "It still needs information it can look up before it can reply or act.";
const GO_ON_NO: &str =
    "It has what it needs for now: it writes to the customer, or makes a change.";

/// Which questions Phase 0b asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum QuestionSet {
    /// v1: after each tool result, one `Choice` over every tool plus
    /// "respond", about the last twelve transcript items, for flows that may
    /// call any tool.
    V1,
    /// v2, as RFC-001 §3.5 specifies, for read-only flows:
    ///
    /// - the options at a site are the lookups the agent made after the same
    ///   tool (succeeding or failing) in training, plus "respond" (hand back);
    ///   a site with none hands back without asking;
    /// - the agent's next step is scored as what a read-only flow should do:
    ///   the lookup, or "respond" for a reply or a write;
    /// - the stop decision is also asked on its own (a `Noul`), with the
    ///   lookup as a separate `Choice`, in the same request;
    /// - the state is a slice: the customer's messages, the agent's last
    ///   message, the latest results in full and older lookups in one line;
    /// - numeric arguments are not asked about (Jev does no arithmetic), and
    ///   writes, handed back, need no arguments from the flow.
    V2,
}

/// A yes/no question about the state, asked alongside the v2 next-step
/// questions, whose answer the arbiter weighs (RFC-001 §3.4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Predicate {
    /// Short id; the question is asked as `pred_<id>`.
    pub id: String,
    /// Which options the answer bears on.
    pub favors: Favors,
    /// The question.
    pub question: String,
    /// What "yes" means.
    pub yes: String,
    /// What "no" means.
    pub no: String,
}

/// The options a [`Predicate`]'s answer bears on, as the arbiter's feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Favors {
    /// The lookup the agent has just made, made again.
    SameLookup,
    /// Every lookup.
    AnyLookup,
    /// Handing back.
    HandBack,
}

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
    /// Write every decision here as JSON lines: who took it, the agent's
    /// option, the oracle's pick and the request key (no state text).
    pub log: Option<PathBuf>,
    /// Which questions to ask.
    pub questions: QuestionSet,
    /// v2: describe each lookup by what its results supply, learned from
    /// argument dataflow in training.
    pub hints: bool,
    /// v2: yes/no questions about the state to ask with every next-step
    /// question and weigh in the arbiter.
    pub predicates: Vec<Predicate>,
    /// v2: whether the arbiter weighs the predicates' answers (off: they are
    /// asked but ignored, to measure what they add).
    pub predicate_features: bool,
    /// v2: offer every read-only tool at every site (see
    /// [`Sites::offer_every_read`]).
    pub manifest_options: bool,
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
            thresholds: vec![0.5, 0.7, 0.9, 0.95, 0.99],
            dump: None,
            log: None,
            questions: QuestionSet::V1,
            hints: false,
            predicates: Vec::new(),
            predicate_features: true,
            manifest_options: false,
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
    /// The option the agent took, as the flow should take it: for v2
    /// next-step questions a reply or a write is "respond" (hand back).
    pub actual: String,
    /// What the agent itself did next (a tool, "respond", or an argument
    /// value).
    pub agent: String,
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
/// key them. Arguments with too many values are left out, and so are
/// arguments that ever took an array or an object: a composite value (a list
/// of flights, a split payment) is assembled from the episode's own records,
/// so other episodes' values are no options for it.
pub fn closed_values<'a>(
    episodes: impl IntoIterator<Item = &'a Episode>,
) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut values: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut composite: BTreeSet<(String, String)> = BTreeSet::new();
    for ep in episodes {
        for call in ep.tool_calls() {
            if let Value::Object(args) = &call.arguments {
                for (arg, v) in args {
                    let key = (call.name.clone(), arg.clone());
                    if matches!(v, Value::Array(_) | Value::Object(_)) {
                        composite.insert(key.clone());
                    }
                    values.entry(key).or_default().insert(value_key(v));
                }
            }
        }
    }
    values.retain(|k, v| v.len() <= MAX_ARG_OPTIONS && !composite.contains(k));
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
                agent: option_of(&se.steps[k].action),
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
                    agent: actual.clone(),
                    actual,
                    request,
                    fixed,
                });
            }
        }
    }
    out
}

/// The lookups a read-only flow may make at each site: after a call to a
/// tool that succeeded (or failed), every read-only tool the agent called
/// next in training, or every read-only tool the manifest lists (see
/// [`Sites::offer_every_read`]). Optionally also what each lookup is for
/// (see [`Sites::learn_feeds`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sites {
    reads: BTreeSet<String>,
    #[serde(with = "stretto_model::pairs")]
    next: BTreeMap<(String, bool), BTreeMap<String, usize>>,
    #[serde(with = "feeds_pairs")]
    feeds: BTreeMap<String, BTreeMap<(String, String), usize>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    every_read: bool,
}

/// [`Sites`]' feeds, with each lookup's `(write, argument)` counts as pairs.
mod feeds_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    type Use = (String, String);
    type Feeds = BTreeMap<String, BTreeMap<Use, usize>>;

    pub fn serialize<S: Serializer>(feeds: &Feeds, s: S) -> Result<S::Ok, S::Error> {
        let flat: BTreeMap<&String, Vec<(&Use, &usize)>> = feeds
            .iter()
            .map(|(tool, uses)| (tool, uses.iter().collect()))
            .collect();
        flat.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Feeds, D::Error> {
        let flat: BTreeMap<String, Vec<(Use, usize)>> = Deserialize::deserialize(d)?;
        Ok(flat
            .into_iter()
            .map(|(tool, uses)| (tool, uses.into_iter().collect()))
            .collect())
    }
}

impl Sites {
    /// Learn from training episodes' steps; `manifest` says which tools only
    /// read.
    pub fn learn<'a>(train: impl IntoIterator<Item = &'a [Step]>, manifest: &ToolManifest) -> Self {
        let reads: BTreeSet<String> = manifest
            .tools
            .iter()
            .filter(|(_, kind)| **kind == ToolKind::Read)
            .map(|(name, _)| name.clone())
            .collect();
        let mut next: BTreeMap<(String, bool), BTreeMap<String, usize>> = BTreeMap::new();
        for steps in train {
            for w in steps.windows(2) {
                if let (Action::Tool(prev), Action::Tool(t)) = (&w[0].action, &w[1].action) {
                    if reads.contains(t) {
                        let site = (prev.clone(), w[0].outcome == Outcome::Err);
                        *next.entry(site).or_default().entry(t.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        Self {
            reads,
            next,
            feeds: BTreeMap::new(),
            every_read: false,
        }
    }

    /// Offer every read-only tool at every site, not only the lookups seen
    /// there in training, and at sites training never showed a lookup. The
    /// System-One model can then choose a lookup the traces never made; the
    /// habit gives such a lookup little weight, and the arbiter weighs the
    /// two.
    pub fn offer_every_read(&mut self) {
        self.every_read = true;
    }

    /// Whether every read-only tool is offered at every site.
    pub fn offers_every_read(&self) -> bool {
        self.every_read
    }

    /// Each site, `(tool, failed)`, with the lookups made next there in
    /// training and how often.
    pub fn next(&self) -> &BTreeMap<(String, bool), BTreeMap<String, usize>> {
        &self.next
    }

    /// Learn which write arguments each lookup's results supply, from
    /// argument dataflow: every value (of three characters or more) passed to
    /// a write is traced to the most recent successful tool output that
    /// contains it, counted once per episode.
    pub fn learn_feeds<'a>(
        &mut self,
        train: impl IntoIterator<Item = &'a Episode>,
        manifest: &ToolManifest,
    ) {
        for ep in train {
            let mut outputs: Vec<(&str, String)> = Vec::new();
            let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
            for e in &ep.events {
                match e {
                    Event::ToolResult {
                        name,
                        content,
                        error: false,
                        ..
                    } => outputs.push((name.as_str(), content.to_lowercase())),
                    Event::Assistant { calls, .. } => {
                        for c in calls {
                            if manifest.tools.get(&c.name) != Some(&ToolKind::Write) {
                                continue;
                            }
                            let Value::Object(args) = &c.arguments else {
                                continue;
                            };
                            for (arg, v) in args {
                                let mut leaves = Vec::new();
                                leaf_strings(v, &mut leaves);
                                for leaf in leaves.iter().filter(|l| l.chars().count() >= 3) {
                                    let needle = leaf.to_lowercase();
                                    let source = outputs
                                        .iter()
                                        .rev()
                                        .find(|(_, out)| out.contains(&needle))
                                        .map(|(tool, _)| *tool);
                                    if let Some(tool) = source.filter(|t| self.reads.contains(*t)) {
                                        seen.insert((
                                            tool.to_string(),
                                            c.name.clone(),
                                            arg.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            for (read, write, arg) in seen {
                *self
                    .feeds
                    .entry(read)
                    .or_default()
                    .entry((write, arg))
                    .or_insert(0) += 1;
            }
        }
    }

    /// What a lookup is for, from [`Sites::learn_feeds`]: the two write
    /// arguments its results supplied most often (in at least three training
    /// episodes each), with up to two writes each.
    pub fn hint(&self, tool: &str) -> Option<String> {
        // Per argument: total uses, and uses per write.
        type Uses<'a> = (usize, Vec<(usize, &'a str)>);
        let mut by_arg: BTreeMap<&str, Uses> = BTreeMap::new();
        for ((write, arg), &n) in self.feeds.get(tool)? {
            if n >= 3 {
                let e = by_arg.entry(arg.as_str()).or_default();
                e.0 += n;
                e.1.push((n, write.as_str()));
            }
        }
        let mut args: Vec<(&str, Uses)> = by_arg.into_iter().collect();
        args.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(b.0)));
        let parts: Vec<String> = args
            .into_iter()
            .take(2)
            .map(|(arg, (_, mut writes))| {
                writes.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
                let writes: Vec<String> = writes
                    .iter()
                    .take(2)
                    .map(|(_, w)| format!("`{w}`"))
                    .collect();
                format!("`{arg}` for {}", writes.join(" or "))
            })
            .collect();
        (!parts.is_empty()).then(|| format!("Its results supply {}.", parts.join(", and ")))
    }

    /// Whether `tool` only reads.
    pub fn is_read(&self, tool: &str) -> bool {
        self.reads.contains(tool)
    }

    /// Every read-only tool, by name: every lookup a flow could make.
    pub fn reads(&self) -> impl Iterator<Item = &String> {
        self.reads.iter()
    }

    /// The lookups seen after `tool` (failed or not), by name: every
    /// read-only tool, if the sites offer every one.
    pub fn options(&self, tool: &str, failed: bool) -> Vec<String> {
        if self.every_read {
            return self.reads.iter().cloned().collect();
        }
        self.next
            .get(&(tool.to_string(), failed))
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// A site's name: the tool, marked when it failed.
    pub fn name(tool: &str, failed: bool) -> String {
        if failed {
            format!("{tool} (error)")
        } else {
            tool.to_string()
        }
    }
}

/// Every question Phase 0b v2 asks about `episodes` (see [`QuestionSet::V2`]),
/// with `predicates` asked alongside each next-step question.
pub fn decisions_v2(
    episodes: &[ShadowEpisode<'_>],
    manifest: &ToolManifest,
    closed: &BTreeMap<(String, String), BTreeSet<String>>,
    sites: &Sites,
    predicates: &[Predicate],
    model: &str,
) -> Vec<Decision> {
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
            if k == 0 {
                continue;
            }
            let Action::Tool(prev) = &se.steps[k - 1].action else {
                continue;
            };
            let failed = se.steps[k - 1].outcome == Outcome::Err;
            let agent = option_of(&se.steps[k].action);
            let actual = if sites.is_read(&agent) {
                agent.clone()
            } else {
                RESPOND.to_string()
            };
            let before = &items[..step_items[k]];
            // A site where the agent never looked anything up hands back.
            let request = next_step_request(
                before, se.goal, manifest, sites, predicates, prev, failed, model,
            );
            let fixed = request.is_none().then(|| RESPOND.to_string());
            out.push(Decision {
                episode: i,
                step: k,
                kind: Kind::Next,
                tool: None,
                actual,
                agent,
                request,
                fixed,
            });
            // Arguments: only for lookups inside a run that need nothing but
            // closed-set choices. A write is handed back, arguments and all.
            let (Some(c), ArgNeed::ClosedSet) = (this_call, se.needs[k]) else {
                continue;
            };
            let Item::Tool {
                name, arguments, ..
            } = &items[step_items[k]]
            else {
                continue;
            };
            if !sites.is_read(name) {
                continue;
            }
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
                let value = arguments.get(arg);
                let actual = value.map(value_key).unwrap_or_default();
                let options = closed.get(&(name.clone(), arg.to_string()));
                let fixed = options
                    .filter(|o| o.len() == 1)
                    .and_then(|o| o.iter().next().cloned());
                // Jev does no arithmetic: a number is left to the LLM.
                let numeric = value.is_some_and(Value::is_number);
                let request = match options {
                    Some(o) if o.len() >= 2 && !numeric => {
                        let others: serde_json::Map<String, Value> = match arguments {
                            Value::Object(m) => m
                                .iter()
                                .filter(|(k, _)| k.as_str() != arg)
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                            _ => Default::default(),
                        };
                        let pending = json!({"tool": name, "other_arguments": others});
                        Some(Request {
                            model: model.to_string(),
                            state: slice(before, se.goal, Some(pending)),
                            questions: BTreeMap::from([(
                                "value".to_string(),
                                arg_question(manifest, name, arg, o),
                            )]),
                        })
                    }
                    _ => None,
                };
                out.push(Decision {
                    episode: i,
                    step: k,
                    kind: Kind::Arg(arg.to_string()),
                    tool: Some(name.clone()),
                    agent: actual.clone(),
                    actual,
                    request,
                    fixed,
                });
            }
        }
    }
    out
}

/// The v2 next-step request after `before`, at the site after `prev`
/// (failed or not), or `None` at a site with no lookups (hand back).
#[allow(clippy::too_many_arguments)]
fn next_step_request(
    before: &[Item],
    goal: &str,
    manifest: &ToolManifest,
    sites: &Sites,
    predicates: &[Predicate],
    prev: &str,
    failed: bool,
    model: &str,
) -> Option<Request> {
    let options = sites.options(prev, failed);
    if options.is_empty() {
        return None;
    }
    let mut questions = next_questions_v2(manifest, sites, prev, failed, &options);
    for p in predicates {
        questions.insert(
            format!("pred_{}", p.id),
            Question::Noul {
                instructions: format!("{V2_CONTEXT} {}", p.question),
                criteria: Some(NoulCriteria {
                    yes: p.yes.clone(),
                    no: p.no.clone(),
                }),
            },
        );
    }
    Some(Request {
        model: model.to_string(),
        state: slice(before, goal, None),
        questions,
    })
}

/// The decision a live flow faces once the last tool call of `episode` has
/// returned.
#[derive(Clone, Debug)]
pub struct LiveSite {
    /// The tool that just returned.
    pub prev: String,
    /// Whether it failed.
    pub failed: bool,
    /// The v2 request, or `None` at a site with no lookups (hand back).
    pub request: Option<Request>,
}

/// The v2 question a live flow asks after the last step of `episode`, which
/// must be a tool call that has returned; `None` otherwise.
pub fn live_request(
    episode: &Episode,
    goal: &str,
    manifest: &ToolManifest,
    sites: &Sites,
    predicates: &[Predicate],
    model: &str,
) -> Option<LiveSite> {
    let st = stretto_model::steps(episode);
    let last = st.last()?;
    let Action::Tool(prev) = &last.action else {
        return None;
    };
    let failed = last.outcome == Outcome::Err;
    let (items, _) = timeline(episode);
    let request = next_step_request(
        &items, goal, manifest, sites, predicates, prev, failed, model,
    );
    Some(LiveSite {
        prev: prev.clone(),
        failed,
        request,
    })
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

/// `path` with `-<domain>` added to its file stem, so each domain's run
/// writes its own file.
pub fn per_domain(path: &std::path::Path, domain: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem}-{domain}.{}", ext.to_string_lossy()),
        None => format!("{stem}-{domain}"),
    };
    path.with_file_name(name)
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
    domain: &str,
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
        let path = per_domain(path, domain);
        let mut lines = String::new();
        for (key, r) in &todo {
            lines.push_str(&serde_json::to_string(&json!({"key": key, "request": r}))?);
            lines.push('\n');
        }
        std::fs::write(&path, lines)?;
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
    /// Its whole distribution over the options.
    pub probs: BTreeMap<String, f64>,
}

impl Scored {
    /// Score the distribution `probs` over the options (its keys), with
    /// `pick` the chosen option, against the agent's option `actual`.
    pub fn of(probs: BTreeMap<String, f64>, pick: String, actual: &str) -> Self {
        let p = |o: &str| probs.get(o).copied().unwrap_or(0.0);
        let brier = probs
            .keys()
            .map(|o| (p(o) - if o == actual { 1.0 } else { 0.0 }).powi(2))
            .sum::<f64>()
            + if probs.contains_key(actual) { 0.0 } else { 1.0 };
        Scored {
            prob: p(&pick),
            prob_actual: p(actual),
            pick,
            brier,
            probs,
        }
    }
}

/// The question id a decision's answer is read from.
fn answer_id(kind: &Kind) -> &'static str {
    match kind {
        Kind::Next => "next",
        Kind::Arg(_) => "value",
    }
}

/// The oracle's answer to `d`, if it has one.
pub fn score(d: &Decision, asked: &Asked) -> Option<Scored> {
    let Some(request) = &d.request else {
        // Certain without asking, or not answerable at all.
        let only = d.fixed.clone()?;
        return Some(Scored::of(
            BTreeMap::from([(only.clone(), 1.0)]),
            only,
            &d.actual,
        ));
    };
    let response = asked.responses.get(&request_key(request))?;
    let id = answer_id(&d.kind);
    let Some(Answer::Choice {
        choice,
        probabilities,
        ..
    }) = response.answers.get(id)
    else {
        return None;
    };
    let Question::Choice { criteria, .. } = request.questions.get(id)? else {
        return None;
    };
    let probs = criteria
        .keys()
        .map(|o| (o.clone(), probabilities.get(o).copied().unwrap_or(0.0)))
        .collect();
    Some(Scored::of(probs, choice.clone(), &d.actual))
}

/// The predicates' answers to `d`, by predicate id: the probability of "yes".
pub fn predicate_answers(d: &Decision, asked: &Asked) -> BTreeMap<String, f64> {
    let Some(response) = d
        .request
        .as_ref()
        .and_then(|r| asked.responses.get(&request_key(r)))
    else {
        return BTreeMap::new();
    };
    response
        .answers
        .iter()
        .filter_map(|(id, a)| match (id.strip_prefix("pred_"), a) {
            (Some(p), Answer::Noul { noul }) => Some((p.to_string(), *noul)),
            _ => None,
        })
        .collect()
}

/// The v2 split answer to a next-step decision: "respond" gets one minus the
/// stop question's probability of going on, and each lookup gets that
/// probability times its share in the lookup question (all of it at a site
/// with one lookup).
pub fn score_split(d: &Decision, asked: &Asked) -> Option<Scored> {
    if d.kind != Kind::Next {
        return None;
    }
    let Some(request) = &d.request else {
        return score(d, asked);
    };
    let response = asked.responses.get(&request_key(request))?;
    let Some(Answer::Noul { noul: go_on }) = response.answers.get("go_on") else {
        return None;
    };
    let Question::Choice { criteria, .. } = request.questions.get("next")? else {
        return None;
    };
    let lookups: Vec<&String> = criteria.keys().filter(|o| *o != RESPOND).collect();
    let mut probs = BTreeMap::from([(RESPOND.to_string(), 1.0 - go_on)]);
    match (response.answers.get("tool"), lookups.as_slice()) {
        (Some(Answer::Choice { probabilities, .. }), _) => {
            for l in &lookups {
                let share = probabilities.get(*l).copied().unwrap_or(0.0);
                probs.insert((*l).clone(), go_on * share);
            }
        }
        (None, [only]) => {
            probs.insert((*only).clone(), *go_on);
        }
        _ => return None,
    }
    let pick = probs
        .iter()
        .fold(None::<(&String, f64)>, |best, (o, &p)| match best {
            Some((_, q)) if q >= p => best,
            _ => Some((o, p)),
        })
        .map(|(o, _)| o.clone())?;
    Some(Scored::of(probs, pick, &d.actual))
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
    /// Reliability bins over the pick's probability, tenths from 0 to 1:
    /// `(answers, mean probability, agreement)`; empty bins are left out.
    pub bins: Vec<(usize, f64, f64)>,
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
        a.bins = bins
            .iter()
            .filter(|b| b.0 > 0)
            .map(|&(count, conf, right)| (count, conf / count as f64, right as f64 / count as f64))
            .collect();
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

/// Every string or number inside `v`, as text.
fn leaf_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Array(items) => items.iter().for_each(|i| leaf_strings(i, out)),
        Value::Object(m) => m.values().for_each(|i| leaf_strings(i, out)),
        _ => {}
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

/// The v2 next-step questions at a site after `prev`: which lookup or hand
/// back (`next`), whether to look anything else up (`go_on`), and which
/// lookup (`tool`, at sites with several). Each lookup is described by its
/// docstring and, when learned, what its results supply.
fn next_questions_v2(
    manifest: &ToolManifest,
    sites: &Sites,
    prev: &str,
    failed: bool,
    lookups: &[String],
) -> BTreeMap<String, Question> {
    let doc = |name: &str| {
        let summary = manifest
            .docs
            .get(name)
            .map(|d| d.summary.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Call {name}."));
        match sites.hint(name) {
            Some(hint) => clip(&format!("{summary} {hint}"), MAX_RUBRIC),
            None => clip(&summary, MAX_RUBRIC),
        }
    };
    let last = if failed {
        format!("Its last tool call, `{prev}`, returned an error.")
    } else {
        format!("It has just received the result of `{prev}`.")
    };
    let mut next = BTreeMap::from([(RESPOND.to_string(), HAND_BACK.to_string())]);
    for l in lookups {
        next.insert(l.clone(), doc(l));
    }
    let mut questions = BTreeMap::from([
        (
            "next".to_string(),
            Question::Choice {
                instructions: format!(
                    "{V2_CONTEXT} {last} What does it do next? Pick the lookup it makes now, or \
                     'respond' if it hands back."
                ),
                criteria: next,
            },
        ),
        (
            "go_on".to_string(),
            Question::Noul {
                instructions: format!(
                    "{V2_CONTEXT} {last} Does it make another lookup now, before it writes to \
                     the customer or makes any change?"
                ),
                criteria: Some(NoulCriteria {
                    yes: GO_ON_YES.to_string(),
                    no: GO_ON_NO.to_string(),
                }),
            },
        ),
    ]);
    if lookups.len() >= 2 {
        questions.insert(
            "tool".to_string(),
            Question::Choice {
                instructions: format!(
                    "{V2_CONTEXT} {last} Suppose it makes one more lookup now: which one?"
                ),
                criteria: lookups.iter().map(|l| (l.clone(), doc(l))).collect(),
            },
        );
    }
    questions
}

/// The v2 state before a decision: the customer's first and latest
/// messages, the goal, the agent's last message, the latest tool results in
/// full and older lookups one line each, and the pending call for argument
/// questions.
fn slice(before: &[Item], goal: &str, pending: Option<Value>) -> Value {
    let customer: Vec<&String> = before
        .iter()
        .filter_map(|i| match i {
            Item::Customer(t) => Some(t),
            _ => None,
        })
        .collect();
    let agent = before.iter().rev().find_map(|i| match i {
        Item::Agent(t) if !t.trim().is_empty() => Some(t),
        _ => None,
    });
    let tools: Vec<&Item> = before
        .iter()
        .filter(|i| matches!(i, Item::Tool { .. }))
        .collect();
    let split = tools.len().saturating_sub(RECENT_RESULTS);
    let earlier: Vec<String> = tools[split.saturating_sub(MAX_EARLIER)..split]
        .iter()
        .filter_map(|i| match i {
            Item::Tool {
                name,
                arguments,
                error,
                ..
            } => Some(format!(
                "{name}({}) → {}",
                clip(&arguments.to_string(), 160),
                if *error { "error" } else { "ok" }
            )),
            _ => None,
        })
        .collect();
    let latest: Vec<Value> = tools[split..]
        .iter()
        .filter_map(|i| match i {
            Item::Tool {
                name,
                arguments,
                result,
                error,
            } => {
                let mut v = json!({
                    "tool_call": name,
                    "arguments": arguments,
                    "result": clip(result, MAX_RESULT_V2),
                });
                if *error {
                    v["error"] = json!(true);
                }
                Some(v)
            }
            _ => None,
        })
        .collect();
    let mut s = json!({
        "customer_request": customer.first().map(|t| clip(t, MAX_TEXT)),
    });
    // Without a named goal (a live flow nobody named) the field is left out.
    if !goal.is_empty() {
        s["flow_goal"] = json!(goal);
    }
    if customer.len() > 1 {
        let later: Vec<String> = customer[1..]
            .iter()
            .rev()
            .take(4)
            .rev()
            .map(|t| clip(t, MAX_MESSAGE_V2))
            .collect();
        s["later_customer_messages"] = json!(later);
    }
    if let Some(t) = agent {
        s["agent_last_message"] = json!(clip(t, MAX_MESSAGE_V2));
    }
    if !earlier.is_empty() {
        s["earlier_lookups"] = json!(earlier);
    }
    s["latest_results"] = json!(latest);
    if let Some(p) = pending {
        s["pending_call"] = p;
    }
    s
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
        let asked = ask(oracle.as_ref(), &ds, &config, "retail").unwrap();
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
    fn v2_offers_a_sites_lookups_and_scores_writes_as_hand_backs() {
        // Training: after get_order the agent sometimes looks up another
        // order, so that is the site's one lookup.
        let call = |name: &str| Step {
            action: Action::Tool(name.into()),
            outcome: Outcome::Ok,
        };
        let train = vec![call("get_order"), call("get_order"), call("cancel")];
        let sites = Sites::learn([train.as_slice()], &manifest());
        assert_eq!(sites.options("get_order", false), vec!["get_order"]);
        assert!(sites.options("cancel", false).is_empty());
        // Offering every read-only tool reaches a site training never
        // showed, and says so in the flow IR only when on.
        let mut every = sites.clone();
        assert!(!serde_json::to_string(&every)
            .unwrap()
            .contains("every_read"));
        every.offer_every_read();
        assert_eq!(every.options("cancel", false), vec!["get_order"]);
        assert_eq!(every.options("get_order", true), vec!["get_order"]);
        let back: Sites = serde_json::from_str(&serde_json::to_string(&every).unwrap()).unwrap();
        assert_eq!(back.options("cancel", false), vec!["get_order"]);

        // Held out: get_order, then cancel (a write), then a reply.
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
        let ds = decisions_v2(&[se], &manifest(), &BTreeMap::new(), &sites, &[], "m");
        assert_eq!(ds.len(), 2);
        // After get_order the agent wrote: a read-only flow hands back.
        assert_eq!(
            (ds[0].agent.as_str(), ds[0].actual.as_str()),
            ("cancel", RESPOND)
        );
        let request = ds[0].request.as_ref().unwrap();
        let ids: Vec<&String> = request.questions.keys().collect();
        assert_eq!(ids, ["go_on", "next"]);
        assert_eq!(request.state["latest_results"][0]["tool_call"], "get_order");
        // After cancel the agent never looked anything up: no question.
        assert!(ds[1].request.is_none());
        assert_eq!(ds[1].fixed.as_deref(), Some(RESPOND));

        let config = ShadowConfig::new(OracleKind::Mock);
        let asked = ask(config.build().unwrap().as_ref(), &ds, &config, "retail").unwrap();
        // The mock puts 0.6 on the first option and says 0.5 to going on.
        let one = score(&ds[0], &asked).unwrap();
        assert_eq!((one.pick.as_str(), one.prob), ("get_order", 0.6));
        let split = score_split(&ds[0], &asked).unwrap();
        assert_eq!(split.probs[RESPOND], 0.5);
        assert_eq!(split.probs["get_order"], 0.5);
        assert_eq!(score(&ds[1], &asked).unwrap().pick, RESPOND);
    }

    #[test]
    fn predicates_ride_along_with_next_step_questions() {
        let call = |name: &str| Step {
            action: Action::Tool(name.into()),
            outcome: Outcome::Ok,
        };
        let train = vec![call("get_order"), call("get_order")];
        let sites = Sites::learn([train.as_slice()], &manifest());
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
        let pending = Predicate {
            id: "list_pending".into(),
            favors: Favors::SameLookup,
            question: "Are records still unchecked?".into(),
            yes: "Some are.".into(),
            no: "None are.".into(),
        };
        let ds = decisions_v2(
            &[se],
            &manifest(),
            &BTreeMap::new(),
            &sites,
            &[pending],
            "m",
        );
        let ids: Vec<&String> = ds[0].request.as_ref().unwrap().questions.keys().collect();
        assert_eq!(ids, ["go_on", "next", "pred_list_pending"]);
        let config = ShadowConfig::new(OracleKind::Mock);
        let asked = ask(config.build().unwrap().as_ref(), &ds, &config, "retail").unwrap();
        assert_eq!(
            predicate_answers(&ds[0], &asked),
            BTreeMap::from([("list_pending".to_string(), 0.5)])
        );
        // A decision settled without asking has no answers.
        assert!(predicate_answers(&ds[1], &asked).is_empty());
    }

    #[test]
    fn dataflow_hints_say_what_a_lookup_supplies() {
        // The cancel's order id was read from get_order's output; its reason
        // came from the customer, so it says nothing about the lookup.
        let ep = episode();
        let st = steps(&ep);
        let mut sites = Sites::learn([st.as_slice()], &manifest());
        sites.learn_feeds([&ep, &ep], &manifest());
        // Two episodes are not enough.
        assert_eq!(sites.hint("get_order"), None);
        sites.learn_feeds([&ep], &manifest());
        assert_eq!(
            sites.hint("get_order").as_deref(),
            Some("Its results supply `order_id` for `cancel`.")
        );
        assert_eq!(sites.hint("cancel"), None);
    }

    #[test]
    fn composite_arguments_are_not_closed_sets() {
        let call = |args: Value| Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: "1".into(),
                name: "book".into(),
                arguments: args,
            }],
            usage: None,
        };
        let mut ep = episode();
        ep.events = vec![
            call(json!({"cabin": "economy", "flights": [{"id": "HAT1"}]})),
            call(json!({"cabin": "business", "flights": [{"id": "HAT2"}]})),
        ];
        let values = closed_values([&ep]);
        assert_eq!(
            values[&("book".to_string(), "cabin".to_string())],
            BTreeSet::from(["business".to_string(), "economy".to_string()])
        );
        assert!(!values.contains_key(&("book".to_string(), "flights".to_string())));
    }

    #[test]
    fn clip_marks_what_it_cuts() {
        assert_eq!(clip("  abcdef ", 3), "abc…");
        assert_eq!(clip("ab", 3), "ab");
        assert_eq!(clip("ééé", 2), "éé…");
    }
}
