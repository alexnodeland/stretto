//! Live read-only flows (RFC-001 §3.5, §3.13).
//!
//! A [`Flow`] keeps what Phase 0b v2 learned for one domain, goal free (see
//! [`crate::phase0::compile_flow`]): the habit, the sites and the lookups
//! seen at each, the System-One questions, the arbiter of each fold, and
//! where each lookup's arguments came from in training. [`Flow::next`] takes
//! a live episode whose last step is a tool call that has returned and says
//! what the flow does next: a lookup, with its arguments, or hand back to
//! the LLM.
//!
//! The flow asks the questions Phase 0b asked offline and takes the
//! arbiter's most likely lookup when its probability clears a threshold
//! (the "lookup first" rule). With [`Decider::Habit`] it asks nothing and
//! acts on the habit's prediction alone. An episode is judged by the
//! arbiter of its task's fold, which never saw the task, and the habit,
//! trained on the train split, never saw a test task.
//!
//! Offline, a lookup whose arguments were all seen earlier in the episode
//! counted as one a flow could make. Live, the flow has to pick them:
//! [`Bindings`] learns, for each lookup argument, which earlier outputs its
//! values came from in training (a tool and a path in its result, such as
//! `get_user_details` at `$.orders[*]`), and binds the first such value the
//! episode has not looked up yet, preferring one the customer mentioned.

use crate::arbitrate::Fitted;
use crate::phase0::{case_of, habit_prior, task_group};
use crate::shadow::{self, Asked, Decision, Kind, Predicate, Sites, RESPOND};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use stretto_model::features::{step_outputs, FeatureMap, FOLDS};
use stretto_model::world::Predictor;
use stretto_model::{steps, EncodedEpisode, GroupedModel, Vocab};
use stretto_oracle::{request_key, Oracle, Question};
use stretto_trace::{Episode, Event, ToolCall, ToolKind, ToolManifest};

/// A share of a lookup argument's training values a source must account for
/// before the flow binds from it.
const MIN_SOURCE_SHARE: f64 = 0.1;
/// An argument the lookup took in at least this share of its training calls
/// is required.
const REQUIRED_SHARE: f64 = 0.9;
/// Sibling values shorter than this are not matched against what the
/// customer said.
const MIN_MENTION: usize = 4;

/// The flow file format this build reads and writes.
pub const FLOW_VERSION: u32 = 1;

/// The arbiter file format this build reads and writes.
pub const ARBITER_VERSION: u32 = 1;

/// A flow's arbiter on its own: the predicates it weighs, the System-One
/// model it asks, and one fit of its weights and of Jev's record. It ships
/// with the compiler and serves a habit learned elsewhere
/// ([`Flow::with_arbiter`]), such as one learned from a deployment's first
/// few sessions. [`Flow::arbiter`] takes it from a flow whose folds share
/// one fit ([`crate::phase0::Config::pooled_arbiter`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arbiter {
    /// The format version; see [`ARBITER_VERSION`].
    stretto_arbiter: u32,
    /// The domain whose decisions it was fitted on.
    domain: String,
    /// The flow it was taken from.
    provenance: Provenance,
    predicates: Vec<Predicate>,
    weighed: Vec<Predicate>,
    fitted: Fitted,
    model: String,
}

impl Arbiter {
    /// One arbiter fitted on `cases`, such as the held-out decisions of
    /// several compiles' decision logs (`--oracle-log`), for a domain none of
    /// them is. `sources` label where the cases came from; `weighed` are the
    /// predicates their features hold, in order.
    pub fn fit(
        domain: &str,
        sources: Vec<String>,
        cases: &[crate::arbitrate::Case],
        predicates: Vec<Predicate>,
        weighed: Vec<Predicate>,
        model: String,
    ) -> Self {
        Arbiter {
            stretto_arbiter: ARBITER_VERSION,
            domain: domain.to_string(),
            provenance: Provenance {
                stretto: env!("CARGO_PKG_VERSION").to_string(),
                sources,
                habit_episodes: 0,
                arbiter_cases: cases.iter().filter(|c| c.fit).count(),
                compiled_unix_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64),
            },
            predicates,
            weighed,
            fitted: crate::arbitrate::fit_pooled(cases),
            model,
        }
    }

    /// The domain whose decisions it was fitted on.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Where it came from: the flow's sources, and the held-out decisions
    /// it was fitted on.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Write the arbiter to `path` as JSON.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let file = std::fs::File::create(path)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", path.display()))?;
        serde_json::to_writer_pretty(std::io::BufWriter::new(file), self)?;
        Ok(())
    }

    /// Read an arbiter written by [`Arbiter::save`].
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        Self::from_json(&text)
    }

    /// Parse an arbiter file.
    pub fn from_json(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Version {
            stretto_arbiter: u32,
        }
        let v: Version = serde_json::from_str(text)
            .map_err(|e| anyhow::anyhow!("not a stretto arbiter: {e}"))?;
        if v.stretto_arbiter != ARBITER_VERSION {
            anyhow::bail!(
                "arbiter format {} is not supported (this build reads {ARBITER_VERSION})",
                v.stretto_arbiter
            );
        }
        Ok(serde_json::from_str(text)?)
    }
}

/// A compiled live flow for one domain. It serializes as the flow IR (see
/// [`Flow::save`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flow {
    /// The format version; see [`FLOW_VERSION`].
    pub(crate) stretto_flow: u32,
    /// Where the flow came from.
    pub(crate) provenance: Provenance,
    pub(crate) vocab: Vocab,
    pub(crate) manifest: ToolManifest,
    pub(crate) map: FeatureMap,
    /// The habit's group for a goal-free episode.
    pub(crate) group: u32,
    pub(crate) habit: GroupedModel,
    pub(crate) sites: Sites,
    /// The predicates asked with each next-step question.
    pub(crate) predicates: Vec<Predicate>,
    /// The predicates the arbiter weighs.
    pub(crate) weighed: Vec<Predicate>,
    /// The arbiter of each fold.
    pub(crate) folds: Vec<Fitted>,
    pub(crate) bindings: Bindings,
    /// The System-One model id to request.
    pub(crate) model: String,
}

/// Where a flow's probability for each option comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decider {
    /// The arbiter: the habit, the System-One model's answers and the
    /// predicates, combined (arm D0).
    #[default]
    Arbiter,
    /// The habit alone, which never asks the System-One model: a flow
    /// compiled from traces and nothing else. At a high threshold it goes on
    /// only where training shows no branch, and hands every branch back, as
    /// TraceCompiler does (arm C).
    Habit,
}

/// What the flow does next.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Proposal {
    /// Look something up.
    Lookup {
        /// The lookup.
        tool: String,
        /// Its arguments.
        arguments: Value,
    },
    /// Hand back to the LLM.
    HandBack {
        /// Why.
        reason: String,
    },
}

/// The flow's answer after one tool call, with what it rests on.
#[derive(Clone, Debug, Serialize)]
pub struct Next {
    /// What to do.
    #[serde(flatten)]
    pub proposal: Proposal,
    /// The site: the tool that just returned, marked when it failed.
    pub site: Option<String>,
    /// The arbiter's probability of each option; the rest is the chance
    /// that the agent would do none of them.
    pub probs: BTreeMap<String, f64>,
    /// The probability of the most likely lookup.
    pub prob: Option<f64>,
    /// The chance that its bound arguments are the agent's own (see
    /// [`Bindings::bind`]).
    pub binding: Option<f64>,
    /// The System-One model's answer to the one question.
    pub oracle: BTreeMap<String, f64>,
    /// The predicates' answers (the probability of "yes").
    pub predicates: BTreeMap<String, f64>,
    /// The request's key in the replay cache.
    pub key: Option<String>,
}

impl Flow {
    /// The domain the flow was compiled for.
    pub fn domain(&self) -> &str {
        &self.manifest.domain
    }

    /// Where the flow came from.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The tools the flow knows, with their kinds.
    pub fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    /// Whether the flow has an arbiter. One learned without a System-One
    /// model ([`crate::phase0::compile_habit_flow_from_episodes`]) has none
    /// and decides with the habit alone.
    pub fn has_arbiter(&self) -> bool {
        !self.folds.is_empty()
    }

    /// How the flow decides unless told otherwise: with its arbiter, or with
    /// the habit alone if it has none.
    pub fn default_decider(&self) -> Decider {
        if self.has_arbiter() {
            Decider::Arbiter
        } else {
            Decider::Habit
        }
    }

    /// This flow's habit, sites and bindings, with the arbiter of `other`: a
    /// flow learned from a few sessions without a System-One model can take
    /// the arbiter fitted where decisions are plentiful, such as a flow
    /// compiled from other agents' traces. The sources record where the
    /// arbiter came from.
    pub fn with_arbiter_of(mut self, other: Flow) -> Result<Self> {
        if !other.has_arbiter() {
            anyhow::bail!("the flow to take an arbiter from has none");
        }
        let from: Vec<String> = other
            .provenance
            .sources
            .iter()
            .map(|s| format!("arbiter: {s}"))
            .collect();
        self.take_arbiter(other);
        self.provenance.sources.extend(from);
        Ok(self)
    }

    /// This flow's arbiter on its own, to ship ([`Arbiter`]). Its folds must
    /// share one fit: a flow compiled with
    /// [`crate::phase0::Config::pooled_arbiter`], or learned from sessions.
    /// Cross-fitted folds are fitted apart, each for the tasks the others
    /// saw, so none of them is the arbiter for new tasks.
    pub fn arbiter(&self) -> Result<Arbiter> {
        let Some(first) = self.folds.first() else {
            anyhow::bail!("the flow has no arbiter");
        };
        let one = serde_json::to_value(first)?;
        for fold in &self.folds[1..] {
            if serde_json::to_value(fold)? != one {
                anyhow::bail!(
                    "the flow's folds were fitted apart; compile it with --pooled-arbiter \
                     to fit one arbiter on every held-out decision"
                );
            }
        }
        Ok(Arbiter {
            stretto_arbiter: ARBITER_VERSION,
            domain: self.domain().to_string(),
            provenance: self.provenance.clone(),
            predicates: self.predicates.clone(),
            weighed: self.weighed.clone(),
            fitted: first.clone(),
            model: self.model.clone(),
        })
    }

    /// This flow's habit, sites and bindings, with a shipped arbiter
    /// ([`Arbiter`]), which judges every session. The sources record where
    /// the arbiter came from. At a site its fitting never saw, such as any
    /// site of another domain, it weighs Jev's answers by their record over
    /// all the sites it was fitted on.
    pub fn with_arbiter(mut self, arbiter: Arbiter) -> Self {
        let from = arbiter
            .provenance
            .sources
            .iter()
            .map(|s| format!("arbiter ({}): {s}", arbiter.domain));
        self.provenance.sources.extend(from);
        self.provenance.arbiter_cases = arbiter.provenance.arbiter_cases;
        self.predicates = arbiter.predicates;
        self.weighed = arbiter.weighed;
        self.folds = vec![arbiter.fitted; FOLDS as usize];
        self.model = arbiter.model;
        self
    }

    /// Serve `other`'s arbiter: its predicates, its fitted weights and the
    /// System-One model it asks, with this flow's habit, sites and bindings.
    pub(crate) fn take_arbiter(&mut self, other: Flow) {
        self.predicates = other.predicates;
        self.weighed = other.weighed;
        self.folds = other.folds;
        self.model = other.model;
        self.provenance.arbiter_cases = other.provenance.arbiter_cases;
        if other.sites.offers_every_read() {
            self.sites.offer_every_read();
        }
    }

    /// Write the flow IR to `path` as JSON.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        let file = std::fs::File::create(path)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", path.display()))?;
        serde_json::to_writer(std::io::BufWriter::new(file), self)?;
        Ok(())
    }

    /// Read a flow IR written by [`Flow::save`].
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        Self::from_json(&text)
    }

    /// Parse a flow IR.
    pub fn from_json(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Version {
            stretto_flow: u32,
        }
        let v: Version =
            serde_json::from_str(text).map_err(|e| anyhow::anyhow!("not a stretto flow: {e}"))?;
        if v.stretto_flow != FLOW_VERSION {
            anyhow::bail!(
                "flow format {} is not supported (this build reads {FLOW_VERSION})",
                v.stretto_flow
            );
        }
        Ok(serde_json::from_str(text)?)
    }

    /// How often each lookup's binding agreed with the agent in training
    /// (see [`Bindings::agreement`]).
    pub fn binding_agreement(&self) -> &BTreeMap<String, [(usize, usize); 2]> {
        self.bindings.agreement()
    }

    /// What the flow does after the last step of `episode`, a tool call that
    /// has returned: take the most likely lookup if its probability (the
    /// arbiter's for the tool, times the chance that the bound arguments are
    /// the agent's) is at least `threshold`, else hand back (also if
    /// `oracle`, asked one request, fails).
    pub fn next(&self, episode: &Episode, oracle: &dyn Oracle, threshold: f64) -> Result<Next> {
        self.next_with(episode, oracle, threshold, Decider::Arbiter)
    }

    /// [`Flow::next`], with the tool's probability from `decider`. The habit
    /// alone never asks `oracle`.
    pub fn next_with(
        &self,
        episode: &Episode,
        oracle: &dyn Oracle,
        threshold: f64,
        decider: Decider,
    ) -> Result<Next> {
        let mut next = Next {
            proposal: Proposal::HandBack {
                reason: String::new(),
            },
            site: None,
            probs: BTreeMap::new(),
            prob: None,
            binding: None,
            oracle: BTreeMap::new(),
            predicates: BTreeMap::new(),
            key: None,
        };
        let hand_back = |mut next: Next, reason: String| {
            next.proposal = Proposal::HandBack { reason };
            Ok(next)
        };
        let Some(live) = shadow::live_request(
            episode,
            "",
            &self.manifest,
            &self.sites,
            &self.predicates,
            &self.model,
        ) else {
            return hand_back(next, "the last step is not a tool call".to_string());
        };
        next.site = Some(Sites::name(&live.prev, live.failed));
        let Some(request) = live.request else {
            return hand_back(next, "no lookups followed here in training".to_string());
        };
        let st = steps(episode);
        let features = self.map.features(&step_outputs(episode));
        let encoded =
            EncodedEpisode::encode_with_features(&st, Some(&features), &self.vocab, false)
                .with_group(self.group);
        let predicted = self.habit.predict_at(&encoded, st.len());
        if decider == Decider::Habit {
            let Some(Question::Choice { criteria, .. }) = request.questions.get("next") else {
                return hand_back(next, "the site offers no options".to_string());
            };
            let options: Vec<String> = criteria.keys().cloned().collect();
            let Some(prior) = habit_prior(&options, &predicted, &self.vocab) else {
                return hand_back(next, "handing back was not an option".to_string());
            };
            return self.look_up(next, episode, &options, &prior, threshold);
        }
        if !self.has_arbiter() {
            anyhow::bail!("this flow has no arbiter, so it decides with the habit alone");
        }
        let key = request_key(&request);
        next.key = Some(key.clone());
        let response = match oracle.ask(&request) {
            Ok(r) => r,
            Err(e) => return hand_back(next, format!("the System-One model failed: {e:#}")),
        };
        let decision = Decision {
            episode: 0,
            step: st.len(),
            kind: Kind::Next,
            tool: None,
            actual: String::new(),
            agent: String::new(),
            request: Some(request),
            fixed: None,
        };
        let asked = Asked {
            responses: HashMap::from([(key, response)]),
            distinct: 1,
            attempted: 1,
            errors: 0,
            first_error: None,
        };
        let (Some(one), Some(two)) = (
            shadow::score(&decision, &asked),
            shadow::score_split(&decision, &asked),
        ) else {
            return hand_back(next, "the System-One answer was incomplete".to_string());
        };
        next.oracle = one.probs.clone();
        next.predicates = shadow::predicate_answers(&decision, &asked);
        let group = task_group(&episode.task_id);
        let Some((case, options)) = case_of(
            &one,
            &two,
            &next.predicates,
            &self.weighed,
            &predicted,
            &live.prev,
            live.failed,
            group,
            &self.vocab,
        ) else {
            return hand_back(next, "handing back was not an option".to_string());
        };
        let judged = self.folds[(group % FOLDS) as usize].judge(&case);
        self.look_up(next, episode, &options, &judged.probs, threshold)
    }

    /// Lookup first: take the most likely lookup among `options` (the first
    /// of equals, as offline) if its probability in `probs`, times the
    /// chance that its bound arguments are the agent's, is at least
    /// `threshold`; else hand back.
    fn look_up(
        &self,
        mut next: Next,
        episode: &Episode,
        options: &[String],
        probs: &[f64],
        threshold: f64,
    ) -> Result<Next> {
        let hand_back = |mut next: Next, reason: String| {
            next.proposal = Proposal::HandBack { reason };
            Ok(next)
        };
        next.probs = options.iter().cloned().zip(probs.iter().copied()).collect();
        let best = options
            .iter()
            .zip(probs)
            .filter(|(o, _)| o.as_str() != RESPOND)
            .fold(None::<(&String, f64)>, |best, (o, &p)| match best {
                Some((_, q)) if q >= p => best,
                _ => Some((o, p)),
            });
        let Some((tool, p)) = best else {
            return hand_back(next, "no lookup to make".to_string());
        };
        next.prob = Some(p);
        if p < threshold {
            return hand_back(next, format!("{tool} at {p:.2}, below {threshold}"));
        }
        // The lookup is the agent's next step only if the tool is and the
        // arguments are its own. One training never made (offered from the
        // manifest) is bound by argument name.
        let bound = if self.bindings.knows(tool) {
            self.bindings.bind(tool, episode)
        } else {
            match self.manifest.docs.get(tool) {
                Some(doc) => self.bindings.bind_by_name(tool, &doc.args, episode),
                None => Err("never called in training, and its arguments are unknown".into()),
            }
        };
        match bound {
            Ok((arguments, chance)) => {
                next.binding = Some(chance);
                if p * chance < threshold {
                    return hand_back(
                        next,
                        format!("{tool} at {p:.2}, times {chance:.2} for its arguments, below {threshold}"),
                    );
                }
                next.proposal = Proposal::Lookup {
                    tool: tool.clone(),
                    arguments,
                };
                Ok(next)
            }
            Err(why) => hand_back(next, format!("{tool}: {why}")),
        }
    }
}

/// What a flow was compiled from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The stretto version that compiled it.
    pub stretto: String,
    /// The training sources, by label.
    pub sources: Vec<String>,
    /// Successful training episodes the habit learned from.
    pub habit_episodes: usize,
    /// Held-out decisions the arbiter was fitted on.
    pub arbiter_cases: usize,
    /// When it was compiled, in milliseconds since the Unix epoch.
    pub compiled_unix_ms: u64,
}

/// Where each lookup's arguments came from in training, and how often
/// binding them that way picked the agent's own values.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bindings {
    /// Per lookup: calls seen, and how many of them passed each argument.
    args: BTreeMap<String, (usize, BTreeMap<String, usize>)>,
    /// Per lookup and argument: see [`Traced`].
    #[serde(with = "stretto_model::pairs")]
    sources: BTreeMap<(String, String), Traced>,
    /// Per lookup: how often the binding picked the agent's own arguments at
    /// the agent's calls in training, `(agreed, calls)`, when its pick was
    /// not (`[0]`) or was (`[1]`) mentioned by the customer.
    agreed: BTreeMap<String, [(usize, usize); 2]>,
}

/// Where one lookup argument's values came from in training.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Traced {
    /// String values the argument took.
    values: usize,
    /// How many of them were found in an earlier output of each
    /// `(tool, path)`.
    #[serde(with = "stretto_model::pairs")]
    found: BTreeMap<(String, String), usize>,
}

impl Bindings {
    /// Learn from training episodes: each string argument of each lookup is
    /// traced to the most recent successful output that holds it as a value.
    /// Then, at each of the agent's lookups, the binding is tried on what
    /// came before, and scored against the agent's own arguments.
    pub fn learn<'a>(
        episodes: impl IntoIterator<Item = &'a Episode>,
        manifest: &ToolManifest,
    ) -> Self {
        let episodes: Vec<&Episode> = episodes.into_iter().collect();
        let is_read = |c: &ToolCall| manifest.tools.get(&c.name) == Some(&ToolKind::Read);
        let mut b = Bindings::default();
        for ep in &episodes {
            let mut outputs: Vec<(&str, Value)> = Vec::new();
            for e in &ep.events {
                match e {
                    Event::ToolResult {
                        name,
                        content,
                        error: false,
                        ..
                    } => outputs.push((name.as_str(), parse(content))),
                    Event::Assistant { calls, .. } => {
                        for c in calls.iter().filter(|c| is_read(c)) {
                            let Value::Object(args) = &c.arguments else {
                                continue;
                            };
                            let seen = b.args.entry(c.name.clone()).or_default();
                            seen.0 += 1;
                            for arg in args.keys() {
                                *seen.1.entry(arg.clone()).or_insert(0) += 1;
                            }
                            for (arg, value) in args {
                                let Value::String(value) = value else {
                                    continue;
                                };
                                let entry =
                                    b.sources.entry((c.name.clone(), arg.clone())).or_default();
                                entry.values += 1;
                                let found = outputs.iter().rev().find_map(|(tool, out)| {
                                    let mut paths = Vec::new();
                                    find(out, value, "$", &mut paths);
                                    paths.into_iter().next().map(|p| (tool.to_string(), p))
                                });
                                if let Some(source) = found {
                                    *entry.found.entry(source).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut agreed: BTreeMap<String, [(usize, usize); 2]> = BTreeMap::new();
        for ep in &episodes {
            for (i, e) in ep.events.iter().enumerate() {
                let Event::Assistant { calls, .. } = e else {
                    continue;
                };
                for (j, c) in calls.iter().enumerate() {
                    if !is_read(c) {
                        continue;
                    }
                    // Calls earlier in the same turn count as made.
                    let Ok((args, mentioned)) = b.pick(&c.name, &ep.events[..i], &calls[..j])
                    else {
                        continue;
                    };
                    if args.is_empty() {
                        continue;
                    }
                    let same = args
                        .iter()
                        .all(|(k, v)| c.arguments.get(k).map(value_text) == Some(value_text(v)));
                    let tally = &mut agreed.entry(c.name.clone()).or_default()[mentioned as usize];
                    tally.0 += same as usize;
                    tally.1 += 1;
                }
            }
        }
        b.agreed = agreed;
        b
    }

    /// Per lookup: how often the binding picked the agent's own arguments in
    /// training, `(agreed, calls)` for unmentioned and mentioned picks.
    pub fn agreement(&self) -> &BTreeMap<String, [(usize, usize); 2]> {
        &self.agreed
    }

    /// Whether training called `tool`.
    pub fn knows(&self, tool: &str) -> bool {
        self.args.contains_key(tool)
    }

    /// Arguments for `tool`, which training never called, by name: each of
    /// `args` takes the first string under a key of that name in an earlier
    /// output (most recent first) that has not been passed to it, preferring
    /// one the customer mentioned (or whose record they did). With no calls
    /// to go on, the chance that they are the agent's own is 1/2, as
    /// [`Bindings::bind`] would smooth it.
    pub fn bind_by_name(
        &self,
        tool: &str,
        args: &BTreeMap<String, String>,
        episode: &Episode,
    ) -> std::result::Result<(Value, f64), String> {
        let mut outputs: Vec<Value> = Vec::new();
        let mut customer = String::new();
        let mut used: BTreeSet<(&str, String)> = BTreeSet::new();
        let mut called = false;
        for e in &episode.events {
            match e {
                Event::User { text } => {
                    customer.push_str(&text.to_lowercase());
                    customer.push('\n');
                }
                Event::ToolResult {
                    content,
                    error: false,
                    ..
                } => outputs.push(parse(content)),
                Event::Assistant { calls, .. } => {
                    for c in calls.iter().filter(|c| c.name == tool) {
                        called = true;
                        if let Value::Object(a) = &c.arguments {
                            for (k, v) in a {
                                used.insert((k.as_str(), value_text(v)));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if args.is_empty() {
            return if called {
                Err("already looked up".to_string())
            } else {
                Ok((Value::Object(Default::default()), 1.0))
            };
        }
        let mut bound = serde_json::Map::new();
        for arg in args.keys() {
            let mut candidates: Vec<(String, bool)> = Vec::new();
            for out in outputs.iter().rev() {
                let mut found = Vec::new();
                named(out, arg, &mut found);
                for (value, siblings) in found {
                    if used.contains(&(arg.as_str(), value.clone()))
                        || candidates.iter().any(|(v, _)| *v == value)
                    {
                        continue;
                    }
                    let mentioned = mentions(&customer, &value)
                        || siblings
                            .iter()
                            .any(|s| s.chars().count() >= MIN_MENTION && mentions(&customer, s));
                    candidates.push((value, mentioned));
                }
            }
            let Some((value, _)) = candidates
                .iter()
                .find(|(_, m)| *m)
                .or(candidates.first())
                .cloned()
            else {
                return Err(format!("no `{arg}` in earlier results to pass"));
            };
            bound.insert(arg.clone(), Value::String(value));
        }
        Ok((Value::Object(bound), 0.5))
    }

    /// Arguments for a call to `tool` after the last step of `episode`, with
    /// the chance that they are the agent's own: how often the binding
    /// picked the agent's arguments in training, for picks the customer
    /// mentioned or not (Laplace-smoothed; 1 for a lookup without
    /// arguments). See [`Bindings::pick`] for the rule.
    pub fn bind(&self, tool: &str, episode: &Episode) -> std::result::Result<(Value, f64), String> {
        let (args, mentioned) = self.pick(tool, &episode.events, &[])?;
        let chance = if args.is_empty() {
            1.0
        } else {
            let (agreed, n) = self
                .agreed
                .get(tool)
                .map_or((0, 0), |a| a[mentioned as usize]);
            (agreed as f64 + 1.0) / (n as f64 + 2.0)
        };
        Ok((Value::Object(args), chance))
    }

    /// Arguments for a call to `tool` after `events` (and `also`, calls
    /// already made in the same turn), and whether the customer mentioned
    /// every value picked; or why there are none. Each required argument
    /// takes the first value found at one of its sources (most recent output
    /// first, in document order) that has not been passed to it already,
    /// preferring a value the customer mentioned (or one whose record they
    /// did, by another of its fields). A lookup without arguments is made
    /// once.
    fn pick(
        &self,
        tool: &str,
        events: &[Event],
        also: &[ToolCall],
    ) -> std::result::Result<(serde_json::Map<String, Value>, bool), String> {
        let Some((calls, args)) = self.args.get(tool) else {
            return Err("never called in training".to_string());
        };
        let required: Vec<&String> = args
            .iter()
            .filter(|(_, &n)| n as f64 >= REQUIRED_SHARE * *calls as f64)
            .map(|(a, _)| a)
            .collect();
        let mut outputs: Vec<(&str, Value)> = Vec::new();
        let mut customer = String::new();
        let mut made: Vec<&ToolCall> = Vec::new();
        for e in events {
            match e {
                Event::User { text } => {
                    customer.push_str(&text.to_lowercase());
                    customer.push('\n');
                }
                Event::ToolResult {
                    name,
                    content,
                    error: false,
                    ..
                } => outputs.push((name.as_str(), parse(content))),
                Event::Assistant { calls, .. } => made.extend(calls.iter()),
                _ => {}
            }
        }
        made.extend(also.iter());
        let mut used: BTreeSet<(&str, String)> = BTreeSet::new();
        let mut called = false;
        for c in made.iter().filter(|c| c.name == tool) {
            called = true;
            if let Value::Object(a) = &c.arguments {
                for (k, v) in a {
                    used.insert((k.as_str(), value_text(v)));
                }
            }
        }
        if required.is_empty() {
            return if called {
                Err("already looked up".to_string())
            } else {
                Ok((Default::default(), true))
            };
        }
        let mut bound = serde_json::Map::new();
        let mut all_mentioned = true;
        for arg in required {
            let Some(Traced {
                values,
                found: sources,
            }) = self.sources.get(&(tool.to_string(), arg.clone()))
            else {
                return Err(format!("`{arg}` never took a string in training"));
            };
            let rules: Vec<&(String, String)> = sources
                .iter()
                .filter(|(_, &n)| n >= 2 && n as f64 >= MIN_SOURCE_SHARE * *values as f64)
                .map(|(s, _)| s)
                .collect();
            let mut candidates: Vec<(String, bool)> = Vec::new();
            for (source, out) in outputs.iter().rev() {
                for (_, path) in rules.iter().filter(|(t, _)| t == source) {
                    for (value, siblings) in at_path(out, path) {
                        if used.contains(&(arg.as_str(), value.clone()))
                            || candidates.iter().any(|(v, _)| *v == value)
                        {
                            continue;
                        }
                        let mentioned = mentions(&customer, &value)
                            || siblings.iter().any(|s| {
                                s.chars().count() >= MIN_MENTION && mentions(&customer, s)
                            });
                        candidates.push((value, mentioned));
                    }
                }
            }
            let pick = candidates
                .iter()
                .find(|(_, m)| *m)
                .or(candidates.first())
                .cloned();
            match pick {
                Some((v, mentioned)) => {
                    all_mentioned &= mentioned;
                    bound.insert(arg.clone(), Value::String(v));
                }
                None if rules.is_empty() => {
                    return Err(format!("no source for `{arg}` (the LLM supplies it)"));
                }
                None => return Err(format!("nothing left to pass as `{arg}`")),
            }
        }
        Ok((bound, all_mentioned))
    }
}

/// A tool result as JSON, or as one string if it is not JSON.
/// A tool output as JSON. Text that is not JSON is one string, or, when it
/// has several non-empty lines, the list of them: many servers list paths or
/// ids one per line, and a lookup can then bind one of them (at `$[*]`).
fn parse(content: &str) -> Value {
    serde_json::from_str(content).unwrap_or_else(|_| {
        let lines: Vec<&str> = content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.len() > 1 {
            Value::Array(
                lines
                    .into_iter()
                    .map(|l| Value::String(l.to_string()))
                    .collect(),
            )
        } else {
            Value::String(content.to_string())
        }
    })
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether the customer's (lower-cased) text mentions `value`, with or
/// without a leading `#`.
fn mentions(customer: &str, value: &str) -> bool {
    let v = value.to_lowercase();
    let v = v.trim();
    let bare = v.trim_start_matches('#');
    !bare.is_empty() && (customer.contains(v) || customer.contains(bare))
}

/// The string values under key `key` anywhere in `v`, in document order,
/// each with the other string values of its object.
fn named(v: &Value, key: &str, out: &mut Vec<(String, Vec<String>)>) {
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                match child {
                    Value::String(s) if k == key => {
                        let siblings = m
                            .values()
                            .filter_map(|x| x.as_str())
                            .filter(|x| *x != s)
                            .map(String::from)
                            .collect();
                        out.push((s.clone(), siblings));
                    }
                    _ => named(child, key, out),
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                named(child, key, out);
            }
        }
        _ => {}
    }
}

/// The paths at which `target` is a string value of `v`, array indices
/// written `[*]`.
fn find(v: &Value, target: &str, path: &str, out: &mut Vec<String>) {
    match v {
        Value::String(s) if s == target => out.push(path.to_string()),
        Value::Object(m) => {
            for (k, child) in m {
                find(child, target, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(items) => {
            for child in items {
                find(child, target, &format!("{path}[*]"), out);
            }
        }
        _ => {}
    }
}

/// The string values of `v` at `path` (as [`find`] writes it), in document
/// order, each with the other string values of its parent object.
fn at_path(v: &Value, path: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    walk(v, path.strip_prefix('$').unwrap_or(path), None, &mut out);
    out
}

fn walk(v: &Value, rest: &str, parent: Option<&Value>, out: &mut Vec<(String, Vec<String>)>) {
    if rest.is_empty() {
        if let Value::String(s) = v {
            let siblings = match parent {
                Some(Value::Object(m)) => m
                    .values()
                    .filter_map(|x| x.as_str())
                    .filter(|x| *x != s)
                    .map(String::from)
                    .collect(),
                _ => Vec::new(),
            };
            out.push((s.clone(), siblings));
        }
        return;
    }
    if let Some(rest) = rest.strip_prefix("[*]") {
        if let Value::Array(items) = v {
            for item in items {
                walk(item, rest, Some(v), out);
            }
        }
        return;
    }
    if let Some(rest) = rest.strip_prefix('.') {
        let end = rest.find(['.', '[']).unwrap_or(rest.len());
        let (key, rest) = rest.split_at(end);
        if let Value::Object(m) = v {
            if let Some(child) = m.get(key) {
                walk(child, rest, Some(v), out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stretto_trace::ToolCall;

    #[test]
    fn a_lookup_training_never_made_is_bound_by_argument_name() {
        let b = Bindings::default();
        assert!(!b.knows("get_item"));
        let result = |content: serde_json::Value| Event::ToolResult {
            call_id: "1".to_string(),
            name: "get_order".to_string(),
            error: false,
            content: content.to_string(),
        };
        let mut ep = Episode {
            id: "e".to_string(),
            task_id: "t".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "m".to_string(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "The lamp in my order, please.".to_string(),
                },
                result(json!({"items": [
                    {"item_id": "111", "name": "Chair"},
                    {"item_id": "222", "name": "Lamp"},
                ]})),
            ],
        };
        let args = BTreeMap::from([("item_id".to_string(), "The item's id.".to_string())]);
        // The item whose record the customer mentioned, at a chance of 1/2.
        let (bound, chance) = b.bind_by_name("get_item", &args, &ep).unwrap();
        assert_eq!(bound, json!({"item_id": "222"}));
        assert_eq!(chance, 0.5);
        // Once it has been looked up, the next one.
        ep.events.push(Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: "2".to_string(),
                name: "get_item".to_string(),
                arguments: json!({"item_id": "222"}),
            }],
            usage: None,
        });
        let (bound, _) = b.bind_by_name("get_item", &args, &ep).unwrap();
        assert_eq!(bound, json!({"item_id": "111"}));
        // Nothing of that name: no lookup.
        let other = BTreeMap::from([("user_id".to_string(), String::new())]);
        assert!(b.bind_by_name("get_user", &other, &ep).is_err());
    }

    fn manifest() -> ToolManifest {
        ToolManifest {
            domain: "retail".to_string(),
            tools: BTreeMap::from([
                ("get_user_details".to_string(), ToolKind::Read),
                ("get_order_details".to_string(), ToolKind::Read),
                ("cancel_pending_order".to_string(), ToolKind::Write),
            ]),
            docs: BTreeMap::new(),
        }
    }

    fn call(id: &str, name: &str, arguments: Value) -> Event {
        Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments,
            }],
            usage: None,
        }
    }

    fn result(id: &str, name: &str, content: Value) -> Event {
        Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: content.to_string(),
        }
    }

    fn episode(events: Vec<Event>) -> Episode {
        Episode {
            id: "e".to_string(),
            task_id: "1".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "m".to_string(),
            reward: 1.0,
            events,
        }
    }

    fn user(orders: &[&str]) -> Value {
        json!({"user_id": "ann_1", "orders": orders})
    }

    #[test]
    fn binds_the_next_order_not_yet_looked_up() {
        let training: Vec<Episode> = (0..3)
            .map(|_| {
                episode(vec![
                    Event::User {
                        text: "help with my orders".to_string(),
                    },
                    call("a", "get_user_details", json!({"user_id": "ann_1"})),
                    result("a", "get_user_details", user(&["#W1", "#W2"])),
                    call("b", "get_order_details", json!({"order_id": "#W1"})),
                    result("b", "get_order_details", json!({"order_id": "#W1"})),
                    call("c", "get_order_details", json!({"order_id": "#W2"})),
                    result("c", "get_order_details", json!({"order_id": "#W2"})),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest());
        let live = |extra: Vec<Event>| {
            let mut events = vec![
                Event::User {
                    text: "I want to cancel something".to_string(),
                },
                call("a", "get_user_details", json!({"user_id": "bob_2"})),
                result("a", "get_user_details", user(&["#W7", "#W8", "#W9"])),
            ];
            events.extend(extra);
            episode(events)
        };
        let args = |e: &Episode| b.bind("get_order_details", e).map(|(v, _)| v);
        assert_eq!(args(&live(vec![])), Ok(json!({"order_id": "#W7"})));
        let after_one = live(vec![
            call("b", "get_order_details", json!({"order_id": "#W7"})),
            result("b", "get_order_details", json!({"order_id": "#W7"})),
        ]);
        assert_eq!(args(&after_one), Ok(json!({"order_id": "#W8"})));
        // The customer names an order: it goes first.
        let named = live(vec![Event::User {
            text: "It's order W9".to_string(),
        }]);
        assert_eq!(args(&named), Ok(json!({"order_id": "#W9"})));
        // In training the agent walked the list in order: the binding agreed
        // at both of its order lookups in all three episodes.
        let (_, chance) = b.bind("get_order_details", &live(vec![])).unwrap();
        assert!((chance - 7.0 / 8.0).abs() < 1e-9, "{chance}");
        // A user id comes from the customer, never from an output: no rule.
        assert!(b.bind("get_user_details", &live(vec![])).is_err());
    }

    #[test]
    fn binds_each_line_of_a_text_listing() {
        let manifest = ToolManifest {
            domain: "files".to_string(),
            tools: BTreeMap::from([
                ("search_files".to_string(), ToolKind::Read),
                ("read_text_file".to_string(), ToolKind::Read),
            ]),
            docs: BTreeMap::new(),
        };
        let text = |id: &str, name: &str, content: &str| Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: content.to_string(),
        };
        let read = |id: &str, path: &str| call(id, "read_text_file", json!({"path": path}));
        let training: Vec<Episode> = ["alpha", "beta"]
            .iter()
            .map(|dir| {
                let (a, b) = (format!("/n/{dir}/a.md"), format!("/n/{dir}/b.md"));
                episode(vec![
                    call(
                        "s",
                        "search_files",
                        json!({"path": "/n", "pattern": "*.md"}),
                    ),
                    text("s", "search_files", &format!("{a}\n{b}\n")),
                    read("r1", &a),
                    text("r1", "read_text_file", "# A"),
                    read("r2", &b),
                    text("r2", "read_text_file", "# B"),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest);
        let live = |extra: Vec<Event>| {
            let mut events = vec![
                call(
                    "s",
                    "search_files",
                    json!({"path": "/n", "pattern": "*.md"}),
                ),
                text("s", "search_files", "/n/gamma/c.md\n/n/gamma/d.md"),
            ];
            events.extend(extra);
            episode(events)
        };
        let path = |e: &Episode| b.bind("read_text_file", e).map(|(v, _)| v);
        assert_eq!(path(&live(vec![])), Ok(json!({"path": "/n/gamma/c.md"})));
        let after_one = live(vec![
            read("r1", "/n/gamma/c.md"),
            text("r1", "read_text_file", "# C"),
        ]);
        assert_eq!(path(&after_one), Ok(json!({"path": "/n/gamma/d.md"})));
        // One line stays one value, as before.
        assert_eq!(parse("/n/only.md"), json!("/n/only.md"));
        assert_eq!(parse("{\"a\": 1}"), json!({"a": 1}));
    }

    #[test]
    fn a_pick_the_customer_did_not_mention_earns_less_trust() {
        let mut manifest = manifest();
        manifest
            .tools
            .insert("get_product_details".to_string(), ToolKind::Read);
        let order = json!({"items": [
            {"name": "Desk Lamp", "product_id": "p1"},
            {"name": "Water Bottle", "product_id": "p2"},
        ]});
        // The agent always looks up the water bottle; half the time the
        // customer named it.
        let training: Vec<Episode> = (0..8)
            .map(|i| {
                let said = if i % 2 == 0 {
                    "my water bottle leaks"
                } else {
                    "an item broke"
                };
                episode(vec![
                    Event::User {
                        text: said.to_string(),
                    },
                    call("a", "get_order_details", json!({"order_id": "#W1"})),
                    result("a", "get_order_details", order.clone()),
                    call("b", "get_product_details", json!({"product_id": "p2"})),
                    result("b", "get_product_details", json!({"product_id": "p2"})),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest);
        assert_eq!(b.agreement()["get_product_details"], [(0, 4), (4, 4)]);
        let live = |said: &str| {
            episode(vec![
                Event::User {
                    text: said.to_string(),
                },
                call("a", "get_order_details", json!({"order_id": "#W5"})),
                result("a", "get_order_details", order.clone()),
            ])
        };
        let (args, chance) = b
            .bind("get_product_details", &live("the desk lamp"))
            .unwrap();
        assert_eq!(args, json!({"product_id": "p1"}));
        assert!((chance - 5.0 / 6.0).abs() < 1e-9, "{chance}");
        let (args, chance) = b.bind("get_product_details", &live("hello")).unwrap();
        assert_eq!(args, json!({"product_id": "p1"}));
        assert!((chance - 1.0 / 6.0).abs() < 1e-9, "{chance}");
    }

    /// A small but complete flow: a habit, sites and bindings learned from
    /// three training episodes, and arbiters fitted on synthetic cases laid
    /// out as [`case_of`] lays them out.
    pub(crate) fn toy_flow() -> Flow {
        use crate::arbitrate::{fit_folds, Case};
        use stretto_model::{GroupedModel, Vocab};
        let manifest = manifest();
        let training: Vec<Episode> = (0..3)
            .map(|_| {
                episode(vec![
                    Event::User {
                        text: "help with my orders".to_string(),
                    },
                    call("a", "get_user_details", json!({"user_id": "ann_1"})),
                    result("a", "get_user_details", user(&["#W1", "#W2"])),
                    call("b", "get_order_details", json!({"order_id": "#W1"})),
                    result("b", "get_order_details", json!({"order_id": "#W1"})),
                    call("c", "get_order_details", json!({"order_id": "#W2"})),
                    result("c", "get_order_details", json!({"order_id": "#W2"})),
                ])
            })
            .collect();
        let steps: Vec<Vec<stretto_model::Step>> = training.iter().map(steps).collect();
        let vocab = Vocab::build(
            steps.iter().flatten(),
            manifest.tools.keys().map(String::as_str),
        );
        let map = FeatureMap::default();
        let encoded: Vec<EncodedEpisode> = training
            .iter()
            .zip(&steps)
            .map(|(ep, st)| {
                let f = map.features(&step_outputs(ep));
                EncodedEpisode::encode_with_features(st, Some(&f), &vocab, true).with_group(1)
            })
            .collect();
        let cases: Vec<Case> = (0..60u64)
            .map(|g| Case {
                group: g,
                site: "get_user_details".to_string(),
                features: vec![
                    vec![0.2f64.ln(), 0.3f64.ln(), 0.3f64.ln(), 1.0],
                    vec![0.8f64.ln(), 0.7f64.ln(), 0.7f64.ln(), 0.0],
                ],
                pick: 1,
                actual: Some(if g % 4 == 0 { 0 } else { 1 }),
                fit: true,
            })
            .collect();
        Flow {
            stretto_flow: FLOW_VERSION,
            provenance: Provenance::default(),
            habit: GroupedModel::fit(2, 1.0, 1.0, vocab.len(), &encoded),
            sites: Sites::learn(steps.iter().map(Vec::as_slice), &manifest),
            bindings: Bindings::learn(&training, &manifest),
            folds: fit_folds(&cases, FOLDS),
            vocab,
            map,
            group: 1,
            predicates: Vec::new(),
            weighed: Vec::new(),
            model: "jev-test".to_string(),
            manifest,
        }
    }

    #[test]
    fn a_flow_survives_its_ir() {
        let flow = toy_flow();
        let json = serde_json::to_string(&flow).unwrap();
        let back = Flow::from_json(&json).unwrap();
        let live = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("a", "get_user_details", json!({"user_id": "bob_2"})),
            result("a", "get_user_details", user(&["#W7", "#W8"])),
        ]);
        let oracle = stretto_oracle::MockOracle {
            confidence: 0.6,
            noul: 0.5,
        };
        let a = flow.next(&live, &oracle, 0.3).unwrap();
        let b = back.next(&live, &oracle, 0.3).unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap()
        );
        assert_eq!(
            a.proposal,
            Proposal::Lookup {
                tool: "get_order_details".to_string(),
                arguments: json!({"order_id": "#W7"}),
            }
        );
        // Saving is stable, and the format is checked on load.
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
        let newer = json.replacen("\"stretto_flow\":1", "\"stretto_flow\":99", 1);
        assert!(Flow::from_json(&newer).is_err());
    }

    /// An oracle that fails if asked.
    struct Unasked;

    impl Oracle for Unasked {
        fn ask(&self, _: &stretto_oracle::Request) -> Result<stretto_oracle::Response> {
            anyhow::bail!("asked")
        }
    }

    #[test]
    fn the_habit_alone_never_asks() {
        let flow = toy_flow();
        let live = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("a", "get_user_details", json!({"user_id": "bob_2"})),
            result("a", "get_user_details", user(&["#W7", "#W8"])),
        ]);
        let next = flow
            .next_with(&live, &Unasked, 0.3, Decider::Habit)
            .unwrap();
        assert!(next.key.is_none() && next.oracle.is_empty());
        assert!((next.probs.values().sum::<f64>() - 1.0).abs() < 1e-9);
        // In training an order lookup always followed the user's details.
        assert_eq!(
            next.proposal,
            Proposal::Lookup {
                tool: "get_order_details".to_string(),
                arguments: json!({"order_id": "#W7"}),
            }
        );
        // It acts on the tool's probability times its arguments' agreement.
        let p = next.prob.unwrap() * next.binding.unwrap();
        let strict = flow
            .next_with(&live, &Unasked, p + 0.01, Decider::Habit)
            .unwrap();
        assert!(matches!(strict.proposal, Proposal::HandBack { .. }));
        // The arbiter asks, and hands back when the answer fails.
        let arbiter = flow.next(&live, &Unasked, 0.3).unwrap();
        assert!(arbiter.key.is_some());
        assert!(matches!(
            arbiter.proposal,
            Proposal::HandBack { ref reason } if reason.contains("System-One model failed")
        ));
    }

    #[test]
    fn paths_generalize_array_indices() {
        let v = json!({"items": [{"name": "Water Bottle", "product_id": "p1"},
                                 {"name": "Desk Lamp", "product_id": "p2"}]});
        let mut paths = Vec::new();
        find(&v, "p2", "$", &mut paths);
        assert_eq!(paths, vec!["$.items[*].product_id"]);
        let found = at_path(&v, "$.items[*].product_id");
        assert_eq!(found[0].0, "p1");
        assert_eq!(found[1], ("p2".to_string(), vec!["Desk Lamp".to_string()]));
        assert!(mentions("the desk lamp i bought\n", "Desk Lamp"));
        assert_eq!(at_path(&json!("u1"), "$"), vec![("u1".to_string(), vec![])]);
    }
}
