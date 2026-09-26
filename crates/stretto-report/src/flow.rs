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

/// The flow file format this build writes, unless a flow has per-site
/// thresholds.
pub const FLOW_VERSION: u32 = 1;

/// The format of a flow with per-site thresholds (`thresholds`) or with
/// bindings scored apart where the customer named another value
/// (`bindings.named_other`), which a build that reads only [`FLOW_VERSION`]
/// must refuse rather than ignore. This build reads both.
pub const FLOW_THRESHOLDS_VERSION: u32 = 2;

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
    /// The flow's run after each call, as a fugue program
    /// ([`crate::program`]). A flow written before flows held their program
    /// runs the standard one.
    #[serde(default = "crate::program::standard")]
    pub(crate) program: fugue::program::Program,
    /// Where the flow may act, once promoted (`stretto promote`). Absent, it
    /// acts after any call where a lookup clears the threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) promoted: Option<Promotion>,
    /// Per-site thresholds (`stretto search`), in place of the served one at
    /// those sites. Above 1, the flow never acts at the site.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) thresholds: BTreeMap<String, f64>,
    /// Each tool's input contract when the flow was learned from recorded
    /// sessions ([`stretto_trace::mcp::contract`]): its arguments, types and
    /// which are required. The proxy makes no lookup of a tool whose server
    /// now lists another. Empty for a flow compiled from τ²-bench results.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) contracts: BTreeMap<String, String>,
}

/// Where a flow may act (RFC-001 §3.7), from `stretto promote`: each site
/// scored on recorded sessions, and the bar a site had to meet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Promotion {
    /// The bar.
    pub bar: Bar,
    /// Every site scored, by name (the tool, and ` (error)` when it failed).
    pub sites: BTreeMap<String, SiteRecord>,
}

impl Promotion {
    /// Whether the flow may act after `site`. A site never scored may not.
    pub fn allows(&self, site: &str) -> bool {
        self.sites.get(site).is_some_and(|s| s.promoted)
    }
}

/// What a site's record must show for the flow to act there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// The threshold the flow was scored at, as it will be served.
    pub threshold: f64,
    /// The least share of the flow's lookups there that the agent made later
    /// in the session.
    pub min_used: f64,
    /// The least lower bound on that share (Wilson, 90% two-sided).
    pub min_lower: f64,
    /// The fewest distinct tasks the lookups came from (sessions, for
    /// recorded sessions).
    pub min_tasks: usize,
}

/// One site's record on the sessions a flow was promoted on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteRecord {
    /// Decisions the flow made there.
    pub decisions: usize,
    /// The lookups it would have made.
    pub lookups: usize,
    /// Of those, the ones the agent made later in the session.
    pub used: usize,
    /// The distinct tasks the lookups came from.
    pub tasks: usize,
    /// The lower bound on `used / lookups` (Wilson, 90% two-sided).
    pub lower: f64,
    /// Whether the site met the bar.
    pub promoted: bool,
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

/// Exploration at a flow's decisions (RFC-001 §3.7): with probability
/// `epsilon`, the flow takes a lookup other than the one its rule picks,
/// drawn in proportion to the decider's probabilities among the lookups it
/// can bind. A wrong lookup at a read-only site is a detour, so this is the
/// cheap, reversible exploration §3.7 allows. Each decision then logs its
/// [`PolicyView`]: every option, and the chance that the flow took what it
/// took, which `stretto evaluate` reweights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Explore {
    /// The chance of exploring at a decision that has an alternative. Zero
    /// explores nothing, but still logs the view.
    pub epsilon: f64,
    /// Seeds the draws: the same seed, flow and episode give the same ones.
    pub seed: u64,
}

/// Every option at one decision, and how the flow chose among them, for
/// counterfactual evaluation (`stretto evaluate`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolicyView {
    /// Who decided: `arbiter` or `habit`.
    pub decider: String,
    /// The rule's threshold.
    pub threshold: f64,
    /// The exploration rate.
    pub epsilon: f64,
    /// Whether this decision explored.
    pub explored: bool,
    /// What the rule alone does here: the lookup's tool, or none to hand
    /// back.
    pub greedy: Option<String>,
    /// The chance that the flow took what it took: `1 - epsilon` for the
    /// rule's choice when there was an alternative, `epsilon` times an
    /// explored lookup's share of the alternatives, and 1 when there was
    /// none.
    pub propensity: f64,
    /// The episode's task.
    pub task_id: String,
    /// The events before the decision.
    pub events: usize,
    /// Each lookup the site offers, in the decider's order.
    pub options: Vec<OptionView>,
}

/// One lookup a site offers, at one decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionView {
    /// The tool.
    pub tool: String,
    /// The decider's probability that the agent calls it next.
    pub p: f64,
    /// The habit's probability, which the decider `habit` uses.
    pub habit: f64,
    /// The chance that its bound arguments are the agent's, if it binds.
    pub binding: Option<f64>,
    /// Its bound arguments, if it binds.
    pub arguments: Option<Value>,
    /// Why it does not bind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unbound: Option<String>,
}

impl PolicyView {
    /// What the rule does with probabilities `p` (the logged decider's, or
    /// the habit's with `habit`) at `threshold`: the index of the lookup it
    /// takes, or `None` to hand back. As [`Flow::next_with`] does: the
    /// likeliest lookup (the first of equals), if its probability, and that
    /// times its binding's chance, reach the threshold.
    pub fn rule(&self, habit: bool, threshold: f64) -> Option<usize> {
        let p = |o: &OptionView| if habit { o.habit } else { o.p };
        let (best, pb) = self.options.iter().enumerate().fold(
            None::<(usize, f64)>,
            |best, (i, o)| match best {
                Some((_, q)) if q >= p(o) => best,
                _ => Some((i, p(o))),
            },
        )?;
        if pb < threshold {
            return None;
        }
        let chance = self.options[best].binding?;
        (pb * chance >= threshold).then_some(best)
    }

    /// The index of the option the flow took, or `None` if it handed back.
    pub fn taken(&self, proposal: &Proposal) -> Option<usize> {
        match proposal {
            Proposal::Lookup { tool, .. } => self.options.iter().position(|o| o.tool == *tool),
            Proposal::HandBack { .. } => None,
        }
    }
}

/// `events` as JSON without call ids and token usage, which differ between
/// runs of the same conversation.
fn content_key(events: &[Event]) -> String {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(map) => {
                for key in ["id", "call_id", "usage"] {
                    map.remove(key);
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut v = serde_json::to_value(events).unwrap_or_default();
    strip(&mut v);
    v.to_string()
}

/// A small deterministic generator (splitmix64) for exploration draws.
struct Draws(u64);

impl Draws {
    fn new(seed: u64, key: &str) -> Self {
        Draws(seed ^ task_group(key))
    }

    /// A uniform draw in `[0, 1)`.
    fn uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A site's options and the probabilities a decision weighs them by.
#[derive(Clone, Copy)]
struct Chosen<'a> {
    /// The options, handing back among them.
    options: &'a [String],
    /// The decider's probability of each.
    probs: &'a [f64],
    /// The habit's.
    habit: &'a [f64],
    /// Who decided.
    decider: Decider,
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
    /// With exploration: every option and how the flow chose ([`Explore`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyView>,
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

    /// Where the flow may act, if it was promoted.
    pub fn promotion(&self) -> Option<&Promotion> {
        self.promoted.as_ref()
    }

    /// This flow, acting only where `promotion` allows (everywhere, with
    /// `None`).
    pub fn with_promotion(mut self, promotion: Option<Promotion>) -> Self {
        self.promoted = promotion;
        self
    }

    /// Whether the flow has an arbiter. One learned without a System-One
    /// model ([`crate::phase0::compile_habit_flow_from_episodes`]) has none
    /// and decides with the habit alone.
    pub fn has_arbiter(&self) -> bool {
        !self.folds.is_empty()
    }

    /// The sites where the flow may make a lookup.
    pub fn sites(&self) -> Vec<String> {
        self.sites.names()
    }

    /// Its per-site thresholds (see [`Flow::with_thresholds`]).
    pub fn thresholds(&self) -> &BTreeMap<String, f64> {
        &self.thresholds
    }

    /// The input contracts it pins, by tool (see [`Flow::with_contracts`]).
    pub fn contracts(&self) -> &BTreeMap<String, String> {
        &self.contracts
    }

    /// The flow pinning `contracts`, the input contract of each tool as the
    /// server listed it when the flow's sessions were recorded.
    pub fn with_contracts(mut self, contracts: BTreeMap<String, String>) -> Self {
        self.contracts = contracts;
        self
    }

    /// The flow with `thresholds` in place of the served threshold at those
    /// sites; above 1, it never acts at a site.
    pub fn with_thresholds(mut self, thresholds: BTreeMap<String, f64>) -> Self {
        self.thresholds = thresholds;
        self.stretto_flow = self.format_version();
        self
    }

    /// The format version the flow's fields need: [`FLOW_THRESHOLDS_VERSION`]
    /// with per-site thresholds or bindings scored where the customer named
    /// another value, else [`FLOW_VERSION`].
    pub(crate) fn format_version(&self) -> u32 {
        if self.thresholds.is_empty()
            && !self.bindings.scores_named_other()
            && !self.bindings.orders_sources_by_site()
        {
            FLOW_VERSION
        } else {
            FLOW_THRESHOLDS_VERSION
        }
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

    /// Parse a flow IR, and check its program against the flow's
    /// distributions ([`crate::program::FlowProgram`]).
    pub fn from_json(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Version {
            stretto_flow: u32,
        }
        let v: Version =
            serde_json::from_str(text).map_err(|e| anyhow::anyhow!("not a stretto flow: {e}"))?;
        if v.stretto_flow != FLOW_VERSION && v.stretto_flow != FLOW_THRESHOLDS_VERSION {
            anyhow::bail!(
                "flow format {} is not supported (this build reads {FLOW_VERSION} and \
                 {FLOW_THRESHOLDS_VERSION})",
                v.stretto_flow
            );
        }
        let flow: Self = serde_json::from_str(text)?;
        if flow.stretto_flow == FLOW_VERSION && !flow.thresholds.is_empty() {
            anyhow::bail!("a flow with per-site thresholds is format {FLOW_THRESHOLDS_VERSION}");
        }
        if flow.stretto_flow == FLOW_VERSION
            && (flow.bindings.scores_named_other() || flow.bindings.orders_sources_by_site())
        {
            anyhow::bail!(
                "a flow with bindings scored where another value was named, or with sources \
                 ordered by site, is format {FLOW_THRESHOLDS_VERSION}"
            );
        }
        crate::program::FlowProgram::new(&flow)?;
        Ok(flow)
    }

    /// The flow's run after each call, as a fugue program.
    pub fn program(&self) -> &fugue::program::Program {
        &self.program
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
        self.next_explored(episode, oracle, threshold, decider, None)
    }

    /// [`Flow::next_with`], exploring as `explore` says, and logging the
    /// decision's [`PolicyView`] when it is given.
    pub fn next_explored(
        &self,
        episode: &Episode,
        oracle: &dyn Oracle,
        threshold: f64,
        decider: Decider,
        explore: Option<Explore>,
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
            policy: None,
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
        let site = Sites::name(&live.prev, live.failed);
        next.site = Some(site.clone());
        if self.promoted.as_ref().is_some_and(|p| !p.allows(&site)) {
            return hand_back(next, "the site is not promoted".to_string());
        }
        // A site's own threshold, if a search set one; above 1 the site is
        // switched off. (The served threshold may be above 1 on purpose: the
        // audit reads the probabilities without acting.)
        let threshold = match self.thresholds.get(&site) {
            Some(&t) if t > 1.0 => {
                return hand_back(next, "the site is switched off".to_string());
            }
            Some(&t) => t,
            None => threshold,
        };
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
            let chosen = Chosen {
                options: &options,
                probs: &prior,
                habit: &prior,
                decider,
            };
            return self.look_up(next, episode, chosen, threshold, explore);
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
        let habit = habit_prior(&options, &predicted, &self.vocab)
            .unwrap_or_else(|| vec![0.0; options.len()]);
        let chosen = Chosen {
            options: &options,
            probs: &judged.probs,
            habit: &habit,
            decider,
        };
        self.look_up(next, episode, chosen, threshold, explore)
    }

    /// The rule's choice ([`Flow::rule`]), explored as `explore` says: with
    /// probability epsilon, a lookup other than the rule's choice, drawn in
    /// proportion to its probability among those that bind.
    fn look_up(
        &self,
        next: Next,
        episode: &Episode,
        chosen: Chosen,
        threshold: f64,
        explore: Option<Explore>,
    ) -> Result<Next> {
        let mut next = self.rule(next, episode, chosen.options, chosen.probs, threshold)?;
        let Some(explore) = explore else {
            return Ok(next);
        };
        let options: Vec<OptionView> = chosen
            .options
            .iter()
            .zip(chosen.probs)
            .zip(chosen.habit)
            .filter(|((o, _), _)| o.as_str() != RESPOND)
            .map(|((tool, &p), &habit)| {
                let bound = self.bind_lookup(tool, episode);
                OptionView {
                    tool: tool.clone(),
                    p,
                    habit,
                    binding: bound.as_ref().ok().map(|(_, c)| *c),
                    arguments: bound.as_ref().ok().map(|(a, _)| a.clone()),
                    unbound: bound.err(),
                }
            })
            .collect();
        let greedy = match &next.proposal {
            Proposal::Lookup { tool, .. } => Some(tool.clone()),
            Proposal::HandBack { .. } => None,
        };
        // The alternatives: lookups that bind, other than the rule's choice.
        let alternatives: Vec<(usize, f64)> = options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.arguments.is_some() && o.p > 0.0 && Some(&o.tool) != greedy.as_ref())
            .map(|(i, o)| (i, o.p))
            .collect();
        let total: f64 = alternatives.iter().map(|(_, w)| w).sum();
        // Keyed by everything so far but the call ids, which hosts make up
        // afresh each run: the same episode draws the same, and episodes
        // that differ draw independently.
        let mut draws = Draws::new(
            explore.seed,
            &format!("{}/{}", episode.task_id, content_key(&episode.events)),
        );
        let (explored, propensity) = if alternatives.is_empty() || explore.epsilon <= 0.0 {
            (false, 1.0)
        } else if draws.uniform() < explore.epsilon {
            let mut u = draws.uniform() * total;
            let &(i, w) = alternatives
                .iter()
                .find(|(_, w)| {
                    u -= w;
                    u < 0.0
                })
                .unwrap_or(alternatives.last().expect("not empty"));
            let o = &options[i];
            next.proposal = Proposal::Lookup {
                tool: o.tool.clone(),
                arguments: o.arguments.clone().expect("alternatives bind"),
            };
            (true, explore.epsilon * w / total)
        } else {
            (false, 1.0 - explore.epsilon)
        };
        next.policy = Some(PolicyView {
            decider: match chosen.decider {
                Decider::Arbiter => "arbiter",
                Decider::Habit => "habit",
            }
            .to_string(),
            threshold,
            epsilon: explore.epsilon,
            explored,
            greedy,
            propensity,
            task_id: episode.task_id.clone(),
            events: episode.events.len(),
            options,
        });
        Ok(next)
    }

    /// A lookup's arguments bound from `episode`, and the chance that they
    /// are the agent's. One that training never made (offered from the
    /// manifest) is bound by argument name.
    fn bind_lookup(
        &self,
        tool: &str,
        episode: &Episode,
    ) -> std::result::Result<(Value, f64), String> {
        if self.bindings.knows(tool) {
            self.bindings.bind(tool, episode)
        } else {
            match self.manifest.docs.get(tool) {
                Some(doc) => self.bindings.bind_by_name(tool, &doc.args, episode),
                None => Err("never called in training, and its arguments are unknown".into()),
            }
        }
    }

    /// Lookup first: take the most likely lookup among `options` (the first
    /// of equals, as offline) if its probability in `probs`, times the
    /// chance that its bound arguments are the agent's, is at least
    /// `threshold`; else hand back.
    fn rule(
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
        // arguments are its own.
        match self.bind_lookup(tool, episode) {
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
    /// Per lookup, `(used, picks)`: where the customer had mentioned a value
    /// at the lookup's sources and the binding would pass another, such as
    /// a second reservation after the customer asked about one, how many of
    /// the distinct values it would pass there the agent went on to pass
    /// itself. The binding's chance there is `(used + 2c) / (picks + 2)`,
    /// where `c` is its chance when nothing was mentioned: few picks keep
    /// it near that, and many decide it.
    /// Empty in flows learned before it was counted, which give such picks
    /// the chance of an unmentioned one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    named_other: BTreeMap<String, (usize, usize)>,
    /// Per lookup argument with more than one source, and per site (the
    /// tool whose result came last before the call): how many of the
    /// argument's values the agent took from each source there. The binding
    /// tries a site's sources in that order, and the most recent output
    /// first within each, where the order learned elsewhere is recency
    /// alone: after reading a phone line, an agent working through the
    /// customer's lines takes the next line id, not the plan id of the line
    /// it just read. Empty in flows learned before it was counted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    site_sources: Vec<SiteSource>,
}

/// How many values of a lookup's argument the agent took from one source at
/// one site, in training ([`Bindings::site_sources`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SiteSource {
    tool: String,
    arg: String,
    /// The tool whose result came last before the call.
    site: String,
    /// The source: a tool's output, and the path in it.
    source: (String, String),
    values: usize,
}

/// Values a site must have taken from its sources before their order there
/// replaces recency.
const MIN_SITE_VALUES: usize = 5;

/// Whether the customer had mentioned the values a binding picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mention {
    /// Some value picked was not mentioned, and neither was any other value
    /// at its sources.
    None,
    /// Every value picked was.
    Picked,
    /// Some value picked was not, while another value at its sources was:
    /// the customer named a record, and this is a different one.
    Other,
}

/// How one lookup's arguments are bound, for review ([`Bindings::review`]).
#[derive(Clone, Debug, PartialEq)]
pub struct LookupBinding {
    /// The lookup.
    pub tool: String,
    /// The agent's calls to it in training.
    pub calls: usize,
    /// Each required argument, and where the flow binds it from.
    pub arguments: Vec<ArgumentBinding>,
    /// At the agent's own calls in training, `(agreed, tried)`: how often
    /// the binding gave the agent's arguments, when the customer had not
    /// mentioned the values picked and when they had.
    pub agreed: [(usize, usize); 2],
    /// Where the customer had mentioned another value at the sources and
    /// not the one picked, `(used, picks)`: how many of the values the
    /// binding would pass there the agent went on to pass. `None` for a flow
    /// without the count, which gives such picks the unmentioned chance.
    pub named_other: Option<(usize, usize)>,
}

/// Where one required argument of a lookup comes from, for review.
#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentBinding {
    /// The argument.
    pub name: String,
    /// How many string values it took in training.
    pub values: usize,
    /// The sources the flow binds it from: `(tool, path, values found
    /// there)`.
    pub sources: Vec<(String, String, usize)>,
}

impl LookupBinding {
    /// Whether every required argument has a source. A lookup with an
    /// argument that has none, such as an email only the customer knows, is
    /// never made by the flow.
    pub fn bindable(&self) -> bool {
        self.arguments.iter().all(|a| !a.sources.is_empty())
    }

    /// The chance that the bound arguments are the agent's own, as
    /// [`Bindings::bind`] gives it: when the customer had not mentioned the
    /// values picked, when they had, and over both. 1 for a lookup without
    /// arguments.
    pub fn chance(&self) -> [f64; 3] {
        if self.arguments.is_empty() {
            return [1.0; 3];
        }
        let smoothed = |(agreed, n): (usize, usize)| (agreed as f64 + 1.0) / (n as f64 + 2.0);
        let [a, b] = self.agreed;
        [smoothed(a), smoothed(b), smoothed((a.0 + b.0, a.1 + b.1))]
    }

    /// The chance where the customer had mentioned another value at the
    /// sources, if the flow scores it apart (see [`Bindings::bind`]).
    pub fn chance_named_other(&self) -> Option<f64> {
        let unmentioned = self.chance()[0];
        self.named_other
            .map(|(used, n)| named_other_chance(used, n, unmentioned))
    }
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
    /// Every lookup's binding: its required arguments, the sources the flow
    /// binds them from, and the binding's chance of matching the agent.
    pub fn review(&self) -> Vec<LookupBinding> {
        self.args
            .iter()
            .map(|(tool, (calls, args))| {
                let arguments = args
                    .iter()
                    .filter(|(_, &n)| n as f64 >= REQUIRED_SHARE * *calls as f64)
                    .map(|(arg, _)| {
                        let traced = self.sources.get(&(tool.clone(), arg.clone()));
                        let values = traced.map_or(0, |t| t.values);
                        let sources = traced
                            .map(|t| {
                                t.found
                                    .iter()
                                    .filter(|(_, &n)| {
                                        n >= 2 && n as f64 >= MIN_SOURCE_SHARE * values as f64
                                    })
                                    .map(|((from, path), &n)| (from.clone(), path.clone(), n))
                                    .collect()
                            })
                            .unwrap_or_default();
                        ArgumentBinding {
                            name: arg.clone(),
                            values,
                            sources,
                        }
                    })
                    .collect();
                LookupBinding {
                    tool: tool.clone(),
                    calls: *calls,
                    arguments,
                    agreed: self.agreed.get(tool).copied().unwrap_or_default(),
                    named_other: self.named_other.get(tool).copied(),
                }
            })
            .collect()
    }

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
        let mut by_site: BTreeMap<(String, String, String, (String, String)), usize> =
            BTreeMap::new();
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
                        // A flow makes a batch of reads one after another, so
                        // each read of a batch is at the site of the read before
                        // it, as the flow will be when it makes the next.
                        let mut before: Option<&str> = None;
                        for c in calls.iter().filter(|c| is_read(c)) {
                            let site = before.or(outputs.last().map(|(t, _)| *t));
                            before = Some(c.name.as_str());
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
                                    *entry.found.entry(source.clone()).or_insert(0) += 1;
                                    if let Some(site) = site {
                                        *by_site
                                            .entry((
                                                c.name.clone(),
                                                arg.clone(),
                                                site.to_string(),
                                                source,
                                            ))
                                            .or_insert(0) += 1;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // Kept only for arguments with more than one source, where the order
        // matters.
        let multi: BTreeSet<(String, String)> = b
            .sources
            .iter()
            .filter(|(_, t)| t.found.len() > 1)
            .map(|(k, _)| k.clone())
            .collect();
        b.site_sources = by_site
            .into_iter()
            .filter(|((tool, arg, _, _), _)| multi.contains(&(tool.clone(), arg.clone())))
            .map(|((tool, arg, site, source), values)| SiteSource {
                tool,
                arg,
                site,
                source,
                values,
            })
            .collect();
        let mut agreed: BTreeMap<String, [(usize, usize); 2]> = BTreeMap::new();
        for ep in &episodes {
            for (i, e) in ep.events.iter().enumerate() {
                let Event::Assistant { calls, .. } = e else {
                    continue;
                };
                for (j, c) in calls.iter().enumerate() {
                    // A lookup the proxy made on its own was bound by this
                    // very rule: it would agree with itself.
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
                    let named = usize::from(mentioned == Mention::Picked);
                    let tally = &mut agreed.entry(c.name.clone()).or_default()[named];
                    tally.0 += same as usize;
                    tally.1 += 1;
                }
            }
        }
        b.agreed = agreed;
        b.named_other = b.named_other_use(&episodes);
        b
    }

    /// Per lookup, `(used, picks)` over the training episodes: after each
    /// successful result, every distinct value the binding would pass where
    /// the customer had mentioned another value at its sources, and whether
    /// the agent went on to pass it. Scored at the agent's own calls, as
    /// `agreed` is, such picks look right, since an agent that reads a
    /// second record picks it as the binding does; what they miss is that
    /// the agent mostly reads only the record the customer named.
    fn named_other_use(&self, episodes: &[&Episode]) -> BTreeMap<String, (usize, usize)> {
        let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for ep in episodes {
            let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
            for (i, e) in ep.events.iter().enumerate() {
                if !matches!(e, Event::ToolResult { error: false, .. }) {
                    continue;
                }
                for tool in self.args.keys() {
                    let Ok((args, Mention::Other)) = self.pick(tool, &ep.events[..=i], &[]) else {
                        continue;
                    };
                    let text = serde_json::to_string(&args).unwrap_or_default();
                    if !seen.insert((tool.clone(), text)) {
                        continue;
                    }
                    let used = ep.events[i + 1..].iter().any(|e| match e {
                        Event::Assistant { calls, .. } => calls.iter().any(|c| {
                            c.name == *tool
                                && args.iter().all(|(k, v)| {
                                    c.arguments.get(k).map(value_text) == Some(value_text(v))
                                })
                        }),
                        _ => false,
                    });
                    let t = tally.entry(tool.clone()).or_default();
                    t.0 += used as usize;
                    t.1 += 1;
                }
            }
        }
        tally
    }

    /// Whether picks where the customer had named another value are scored
    /// apart (`named_other`), as flows learned by this build do.
    pub(crate) fn scores_named_other(&self) -> bool {
        !self.named_other.is_empty()
    }

    /// Whether the binding orders an argument's sources by site
    /// (`site_sources`), as flows learned by this build do.
    pub(crate) fn orders_sources_by_site(&self) -> bool {
        !self.site_sources.is_empty()
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
            let tallies = self.agreed.get(tool).copied().unwrap_or_default();
            let smoothed = |(agreed, n): (usize, usize)| (agreed as f64 + 1.0) / (n as f64 + 2.0);
            match mentioned {
                Mention::None => smoothed(tallies[0]),
                Mention::Picked => smoothed(tallies[1]),
                // Where the customer named another value: how often the agent
                // went on to pass one it had not named, if training counted
                // it; else as an unmentioned pick, as older flows did.
                Mention::Other => match self.named_other.get(tool) {
                    Some(&(used, n)) => named_other_chance(used, n, smoothed(tallies[0])),
                    None => smoothed(tallies[0]),
                },
            }
        };
        Ok((Value::Object(args), chance))
    }

    /// Arguments for a call to `tool` after `events` (and `also`, calls
    /// already made in the same turn), and whether the customer mentioned
    /// every value picked, or another value at its sources instead; or why
    /// there are none. Each required argument
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
    ) -> std::result::Result<(serde_json::Map<String, Value>, Mention), String> {
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
                Ok((Default::default(), Mention::Picked))
            };
        }
        let mut bound = serde_json::Map::new();
        let mut all_mentioned = true;
        let mut other_named = false;
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
            // Where the agent took this argument at this site, if it did so
            // often enough: its sources in that order, each the most recent
            // output first. Otherwise every source by recency.
            let site = outputs.last().map(|(t, _)| *t);
            let at_site: Vec<(&(String, String), usize)> = rules
                .iter()
                .map(|&r| {
                    let n = self
                        .site_sources
                        .iter()
                        .filter(|s| {
                            s.tool == tool
                                && &s.arg == arg
                                && Some(s.site.as_str()) == site
                                && &s.source == r
                        })
                        .map(|s| s.values)
                        .sum::<usize>();
                    (r, n)
                })
                .collect();
            let order: Vec<(&str, &Value, &String)> =
                if at_site.iter().map(|(_, n)| n).sum::<usize>() >= MIN_SITE_VALUES {
                    let mut ranked = at_site.clone();
                    // Stable: equal counts keep the sources' own order.
                    ranked.sort_by(|a, b| b.1.cmp(&a.1));
                    ranked
                        .iter()
                        .flat_map(|((t, path), _)| {
                            outputs
                                .iter()
                                .rev()
                                .filter(move |(source, _)| source == t)
                                .map(move |(source, out)| (*source, out, path))
                        })
                        .collect()
                } else {
                    outputs
                        .iter()
                        .rev()
                        .flat_map(|(source, out)| {
                            rules
                                .iter()
                                .filter(move |(t, _)| t == source)
                                .map(move |(_, path)| (*source, out, path))
                        })
                        .collect()
                };
            let mut candidates: Vec<(String, bool)> = Vec::new();
            // Whether the customer mentioned any value at the sources, one
            // already passed included.
            let mut any_mentioned = false;
            for (_, out, path) in order {
                for (value, siblings) in at_path(out, path) {
                    let mentioned = mentions(&customer, &value)
                        || siblings
                            .iter()
                            .any(|s| s.chars().count() >= MIN_MENTION && mentions(&customer, s));
                    any_mentioned |= mentioned;
                    if used.contains(&(arg.as_str(), value.clone()))
                        || candidates.iter().any(|(v, _)| *v == value)
                    {
                        continue;
                    }
                    candidates.push((value, mentioned));
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
                    other_named |= !mentioned && any_mentioned;
                    bound.insert(arg.clone(), Value::String(v));
                }
                None if rules.is_empty() => {
                    return Err(format!("no source for `{arg}` (the LLM supplies it)"));
                }
                None => return Err(format!("nothing left to pass as `{arg}`")),
            }
        }
        let mention = if all_mentioned {
            Mention::Picked
        } else if other_named {
            Mention::Other
        } else {
            Mention::None
        };
        Ok((bound, mention))
    }
}

/// The chance of a pick where the customer named another value: `used` of
/// `picks` such values the agent went on to pass, shrunk towards the
/// unmentioned chance by two picks' worth.
fn named_other_chance(used: usize, picks: usize, unmentioned: f64) -> f64 {
    (used as f64 + 2.0 * unmentioned) / (picks as f64 + 2.0)
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
pub(crate) mod tests {
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
    fn a_record_the_customer_did_not_name_is_scored_by_use() {
        // In training the customer names one of three orders, and the agent
        // reads only that one.
        let training: Vec<Episode> = (0..4)
            .map(|_| {
                episode(vec![
                    Event::User {
                        text: "my order W2 never came".to_string(),
                    },
                    call("a", "get_user_details", json!({"user_id": "ann_1"})),
                    result("a", "get_user_details", user(&["#W1", "#W2", "#W3"])),
                    call("b", "get_order_details", json!({"order_id": "#W2"})),
                    result("b", "get_order_details", json!({"order_id": "#W2"})),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest());
        // At the agent's own lookups the binding picked the named order.
        assert_eq!(b.agreement()["get_order_details"], [(0, 0), (4, 4)]);
        // After it, the binding would pass #W1, which no agent read.
        assert_eq!(b.named_other.get("get_order_details"), Some(&(0, 4)));
        let live = |extra: Vec<Event>| {
            let mut events = vec![
                Event::User {
                    text: "where is W8?".to_string(),
                },
                call("a", "get_user_details", json!({"user_id": "bob_2"})),
                result("a", "get_user_details", user(&["#W7", "#W8", "#W9"])),
            ];
            events.extend(extra);
            episode(events)
        };
        // The named order first, at the chance of a named pick.
        let (args, chance) = b.bind("get_order_details", &live(vec![])).unwrap();
        assert_eq!(args, json!({"order_id": "#W8"}));
        assert!((chance - 5.0 / 6.0).abs() < 1e-9, "{chance}");
        // Then another, at the chance that the agent reads one it was not
        // asked about.
        let after = live(vec![
            call("b", "get_order_details", json!({"order_id": "#W8"})),
            result("b", "get_order_details", json!({"order_id": "#W8"})),
        ]);
        let (args, chance) = b.bind("get_order_details", &after).unwrap();
        assert_eq!(args, json!({"order_id": "#W7"}));
        assert!((chance - 1.0 / 6.0).abs() < 1e-9, "{chance}");
        // A flow learned before the count gives it the unmentioned chance.
        let older = Bindings {
            named_other: BTreeMap::new(),
            ..b.clone()
        };
        let (_, chance) = older.bind("get_order_details", &after).unwrap();
        assert!((chance - 0.5).abs() < 1e-9, "{chance}");
        // And a flow with the count is written as format 2.
        assert!(b.scores_named_other());
        assert!(!older.scores_named_other());
    }

    #[test]
    fn takes_the_source_the_agent_used_at_the_site() {
        // Each order names a related order. Mostly the agent walks the user's
        // list; twice it follows the first order's related one instead.
        let order = |id: &str, related: &str| json!({"order_id": id, "related_order": related});
        let training: Vec<Episode> = (0..10)
            .map(|i| {
                let next = if i < 8 { "#W2" } else { "#W9" };
                episode(vec![
                    Event::User {
                        text: "help with my orders".to_string(),
                    },
                    call("a", "get_user_details", json!({"user_id": "ann_1"})),
                    result("a", "get_user_details", user(&["#W1", "#W2", "#W3"])),
                    call("b", "get_order_details", json!({"order_id": "#W1"})),
                    result("b", "get_order_details", order("#W1", "#W9")),
                    call("c", "get_order_details", json!({"order_id": next})),
                    result("c", "get_order_details", order(next, "#W0")),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest());
        assert!(b.orders_sources_by_site());
        let after_one = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("a", "get_user_details", json!({"user_id": "bob_2"})),
            result("a", "get_user_details", user(&["#W7", "#W8"])),
            call("b", "get_order_details", json!({"order_id": "#W7"})),
            result("b", "get_order_details", order("#W7", "#W5")),
        ]);
        let args = |b: &Bindings| b.bind("get_order_details", &after_one).map(|(v, _)| v);
        // At this site the agent took the next order of the list 8 times in
        // 10: the binding does too, where recency alone takes the related
        // order of the one just read.
        assert_eq!(args(&b), Ok(json!({"order_id": "#W8"})));
        let older = Bindings {
            site_sources: Vec::new(),
            ..b.clone()
        };
        assert_eq!(args(&older), Ok(json!({"order_id": "#W5"})));
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
            program: crate::program::standard(),
            promoted: None,
            thresholds: BTreeMap::new(),
            contracts: BTreeMap::new(),
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
    fn per_site_thresholds_take_the_served_ones_place() {
        let flow = toy_flow();
        let live = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("a", "get_user_details", json!({"user_id": "bob_2"})),
            result("a", "get_user_details", user(&["#W7", "#W8"])),
        ]);
        let site = "get_user_details".to_string();
        assert!(flow.sites().contains(&site));
        let at = |t: f64| {
            flow.clone()
                .with_thresholds(BTreeMap::from([(site.clone(), t)]))
                .next_with(&live, &Unasked, 0.3, Decider::Habit)
                .unwrap()
        };
        let served = flow
            .next_with(&live, &Unasked, 0.3, Decider::Habit)
            .unwrap();
        assert!(matches!(served.proposal, Proposal::Lookup { .. }));
        // The same lookup, but above what the site now asks for.
        assert!(matches!(at(0.99).proposal, Proposal::HandBack { .. }));
        assert_eq!(at(0.3).proposal, served.proposal);
        // Switched off, the site hands back before asking anything, with
        // either decider.
        let off = flow
            .clone()
            .with_thresholds(BTreeMap::from([(site.clone(), 2.0)]));
        for next in [
            off.next(&live, &Unasked, 0.3).unwrap(),
            off.next_with(&live, &Unasked, 0.3, Decider::Habit).unwrap(),
        ] {
            assert!(next.key.is_none());
            assert!(matches!(
                next.proposal,
                Proposal::HandBack { ref reason } if reason.contains("switched off")
            ));
        }
        // Another site's threshold changes nothing here.
        let other = flow
            .clone()
            .with_thresholds(BTreeMap::from([("get_order_details".to_string(), 2.0)]))
            .next_with(&live, &Unasked, 0.3, Decider::Habit)
            .unwrap();
        assert_eq!(other.proposal, served.proposal);
    }

    #[test]
    fn exploration_logs_the_chance_of_what_the_flow_took() {
        let flow = toy_flow();
        let live = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("a", "get_user_details", json!({"user_id": "bob_2"})),
            result("a", "get_user_details", user(&["#W7", "#W8"])),
        ]);
        let explore = |epsilon: f64, seed: u64| Some(Explore { epsilon, seed });
        // At 0.99 the rule hands back, so the one lookup that binds is the
        // alternative: taken with probability epsilon, and logged so.
        let (runs, epsilon) = (4000, 0.3);
        let mut explored = 0;
        for seed in 0..runs {
            let next = flow
                .next_explored(
                    &live,
                    &Unasked,
                    0.99,
                    Decider::Habit,
                    explore(epsilon, seed),
                )
                .unwrap();
            let view = next.policy.as_ref().unwrap();
            assert_eq!(view.greedy, None);
            assert_eq!(view.rule(true, 0.99), None);
            match &next.proposal {
                Proposal::Lookup { tool, arguments } => {
                    explored += 1;
                    assert!(view.explored);
                    assert!((view.propensity - epsilon).abs() < 1e-12);
                    assert_eq!(tool, "get_order_details");
                    assert_eq!(arguments, &json!({"order_id": "#W7"}));
                }
                Proposal::HandBack { .. } => {
                    assert!(!view.explored);
                    assert!((view.propensity - (1.0 - epsilon)).abs() < 1e-12);
                }
            }
            // The logged propensity is the one `evaluate` computes.
            let taken = view.taken(&next.proposal);
            assert!((crate::evaluate::propensity(view, taken) - view.propensity).abs() < 1e-12);
        }
        let share = explored as f64 / runs as f64;
        assert!((share - epsilon).abs() < 0.03, "{share}");
        // The same seed draws the same, whatever ids the host gave the calls.
        let renamed = episode(vec![
            Event::User {
                text: "I want to cancel something".to_string(),
            },
            call("zz9", "get_user_details", json!({"user_id": "bob_2"})),
            result("zz9", "get_user_details", user(&["#W7", "#W8"])),
        ]);
        for seed in 0..50 {
            let a = flow.next_explored(&live, &Unasked, 0.99, Decider::Habit, explore(0.5, seed));
            let b =
                flow.next_explored(&renamed, &Unasked, 0.99, Decider::Habit, explore(0.5, seed));
            assert_eq!(a.unwrap().proposal, b.unwrap().proposal);
        }
        // The view's rule is the flow's, for either decider, at any
        // threshold; epsilon 0 changes nothing but the log.
        let oracle = stretto_oracle::MockOracle {
            confidence: 0.6,
            noul: 0.5,
        };
        for decider in [Decider::Habit, Decider::Arbiter] {
            for t in [0.05, 0.3, 0.5, 0.7, 0.95] {
                let plain = flow.next_with(&live, &oracle, t, decider).unwrap();
                let logged = flow
                    .next_explored(&live, &oracle, t, decider, explore(0.0, 0))
                    .unwrap();
                assert_eq!(plain.proposal, logged.proposal);
                assert!(plain.policy.is_none());
                let view = logged.policy.unwrap();
                assert_eq!(view.propensity, 1.0);
                let rule = view
                    .rule(decider == Decider::Habit, t)
                    .map(|i| view.options[i].tool.clone());
                let own = match plain.proposal {
                    Proposal::Lookup { tool, .. } => Some(tool),
                    Proposal::HandBack { .. } => None,
                };
                assert_eq!(rule, own, "{decider:?} at {t}");
                assert_eq!(view.greedy, own);
            }
        }
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
