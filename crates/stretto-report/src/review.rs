//! Reviewing flows as code (RFC-001 §6, question 4).
//!
//! A raw diff of two flow files shows habit counts and fold weights. [`show`]
//! and [`diff`] say instead what a flow does: which tools it may call, which
//! lookups it may make after each call, where their arguments come from, and
//! how it decides. A change a reviewer must look at, such as a tool newly
//! marked read-only or a lookup the flow could not make before, is listed as
//! one to review.

use crate::flow::{Bar, Decider, Flow, LookupBinding, Provenance, SiteRecord};
use crate::shadow::{Predicate, Sites, RESPOND};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use stretto_model::world::{decode, BackoffModel};
use stretto_model::Action;
use stretto_trace::ToolKind;

/// What the agent did next after a call to `tool` that failed or not, in
/// training: each action's share, and how many steps that is.
fn followed(flow: &Flow, tool: &str, failed: bool) -> (BTreeMap<String, f64>, f64) {
    shares(flow, flow.habit.base(), tool, failed)
}

/// Whether the review weighs a site with `reach`: for a flow served with it
/// by default ([`Flow::served_decider`]). Otherwise it weighs the habit
/// alone, which for a flow with an arbiter leaves out the model it asks.
fn weighs_reach(flow: &Flow) -> bool {
    flow.served_decider() == Decider::Reach
}

/// The name of what the review weighs a site with.
fn weighed_by(flow: &Flow) -> &'static str {
    if weighs_reach(flow) {
        "`reach`"
    } else {
        "the habit alone"
    }
}

/// The shares the flow weighs after a site: with `reach`, of the times the
/// agent made the call in training, how often each action came before its
/// next write; else `next`, what it did next.
fn weighed(
    flow: &Flow,
    tool: &str,
    failed: bool,
    next: &BTreeMap<String, f64>,
) -> BTreeMap<String, f64> {
    match &flow.reach {
        Some(reach) if weighs_reach(flow) => shares(flow, reach, tool, failed).0,
        _ => next.clone(),
    }
}

/// Of `model`'s training steps just after a call to `tool` that failed or
/// not, each action's share, and how many steps that is.
fn shares(
    flow: &Flow,
    model: &BackoffModel,
    tool: &str,
    failed: bool,
) -> (BTreeMap<String, f64>, f64) {
    let id = flow.vocab.id(&Action::Tool(tool.to_string()));
    let outcome = u32::from(failed);
    let (counts, total) =
        model.followed(|s| matches!(decode(s), Some((a, o, _)) if a == id && o == outcome));
    let shares = counts
        .into_iter()
        .filter_map(|(a, n)| {
            let name = match flow.vocab.action(a)? {
                Action::Respond => RESPOND.to_string(),
                Action::Tool(t) => t.clone(),
            };
            Some((name, if total > 0.0 { n / total } else { 0.0 }))
        })
        .collect();
    (shares, total)
}

/// Every lookup's binding, by tool.
fn bindings(flow: &Flow) -> BTreeMap<String, LookupBinding> {
    flow.bindings
        .review()
        .into_iter()
        .map(|b| (b.tool.clone(), b))
        .collect()
}

/// The likeliest lookup after a site for a flow deciding with the habit
/// alone or with `reach`, pooled over training.
#[derive(Clone, Debug, PartialEq)]
struct Likely {
    tool: String,
    /// Its share of what the agent did next there, or with `reach` of the
    /// times the agent made it before its next write.
    share: f64,
    /// The chance that its bound arguments are the agent's, if the flow can
    /// bind them.
    chance: Option<f64>,
}

impl Likely {
    /// The share times the binding's chance: what the flow compares with
    /// the threshold.
    fn prob(&self) -> f64 {
        self.share * self.chance.unwrap_or(0.0)
    }

    fn describe(&self) -> String {
        match self.chance {
            Some(c) => format!(
                "`{}` {:.2} × {c:.2} = {:.2}",
                self.tool,
                self.share,
                self.prob()
            ),
            None => format!("`{}` {:.2}, which it cannot bind", self.tool, self.share),
        }
    }
}

/// Of the lookups offered after a site, the one with the largest share in
/// `shares` (the first of equals, as the flow takes it), with the chance of
/// binding its arguments. It is what the habit or `reach` weighs there,
/// pooled over the calls before the site and the code features that
/// sharpen it, so a live decision near the threshold can go either way.
fn likely(
    flow: &Flow,
    tool: &str,
    failed: bool,
    shares: &BTreeMap<String, f64>,
    bound: &BTreeMap<String, LookupBinding>,
) -> Option<Likely> {
    let mut best: Option<Likely> = None;
    for o in flow.sites.options(tool, failed) {
        if o == RESPOND {
            continue;
        }
        let share = shares.get(&o).copied().unwrap_or(0.0);
        if best.as_ref().is_some_and(|b| b.share >= share) {
            continue;
        }
        let chance = binding_chance(flow, &o, bound);
        best = Some(Likely {
            tool: o,
            share,
            chance,
        });
    }
    best
}

/// The chance that the arguments the flow binds for the lookup `tool` are
/// the agent's own, over whether the customer mentioned them or not, if it
/// can bind them. A lookup training never made is bound by argument name,
/// with a chance of 1/2 (1 without arguments).
fn binding_chance(flow: &Flow, tool: &str, bound: &BTreeMap<String, LookupBinding>) -> Option<f64> {
    match bound.get(tool) {
        Some(b) => b.bindable().then(|| b.chance()[2]),
        None => flow
            .manifest
            .docs
            .get(tool)
            .map(|d| if d.args.is_empty() { 1.0 } else { 0.5 }),
    }
}

/// What the flow does after a site, as [`weighed_by`] says, at `threshold`.
fn verdict(likely: Option<&Likely>, threshold: f64) -> String {
    match likely {
        Some(l) if l.prob() >= threshold => format!("looks up {}", l.describe()),
        Some(l) => format!("hands back: {}", l.describe()),
        None => "hands back: no lookup offered".to_string(),
    }
}

/// Whether `flow` may act after a call to `tool` that failed or not: always,
/// unless it was promoted and the site was not, or the site is switched off.
fn acts(flow: &Flow, tool: &str, failed: bool) -> bool {
    flow.promotion()
        .is_none_or(|p| p.allows(&Sites::name(tool, failed)))
        && !switched_off(flow, tool, failed)
}

/// Whether a search switched the site off.
fn switched_off(flow: &Flow, tool: &str, failed: bool) -> bool {
    flow.thresholds()
        .get(&Sites::name(tool, failed))
        .is_some_and(|t| *t > 1.0)
}

/// The threshold `flow` acts on after `tool`: the site's own, if a search
/// set one, else the one it is served with.
fn threshold_at(flow: &Flow, tool: &str, failed: bool, served: f64) -> f64 {
    flow.thresholds()
        .get(&Sites::name(tool, failed))
        .copied()
        .unwrap_or(served)
}

/// What the flow does after a site, promoted or not.
fn action(
    flow: &Flow,
    tool: &str,
    failed: bool,
    likely: Option<&Likely>,
    threshold: f64,
) -> String {
    if acts(flow, tool, failed) {
        verdict(likely, threshold_at(flow, tool, failed, threshold))
    } else if switched_off(flow, tool, failed) {
        "hands back: switched off".to_string()
    } else {
        "hands back: not promoted".to_string()
    }
}

fn site_name(tool: &str, failed: bool) -> String {
    if failed {
        format!("`{tool}` (failed)")
    } else {
        format!("`{tool}`")
    }
}

fn kind_name(kind: Option<ToolKind>) -> &'static str {
    match kind {
        Some(ToolKind::Read) => "read",
        Some(ToolKind::Write) => "write",
        Some(ToolKind::Generic) => "neither",
        None => "absent",
    }
}

/// The names of an arbiter's weights, in order.
fn weight_names(flow: &Flow) -> Vec<String> {
    [
        "ln the habit's probability",
        "ln the one-question probability",
        "ln the split question's probability",
        "handing back",
    ]
    .into_iter()
    .map(String::from)
    .chain(flow.weighed.iter().map(|q| format!("predicate `{}`", q.id)))
    .chain(["the model's record at the site, on its pick".to_string()])
    .collect()
}

/// Predicates' ids, as "`a`, `b`" ("none" for none).
fn ids(predicates: &[Predicate]) -> String {
    if predicates.is_empty() {
        return "none".to_string();
    }
    predicates
        .iter()
        .map(|q| format!("`{}`", q.id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// An arbiter's weights, averaged over its folds.
fn weights(flow: &Flow) -> Vec<f64> {
    let n = flow.folds.len().max(1) as f64;
    let mut mean = vec![0.0; flow.folds.first().map_or(0, |f| f.weights.len())];
    for fold in &flow.folds {
        for (m, w) in mean.iter_mut().zip(&fold.weights) {
            *m += w / n;
        }
    }
    mean
}

fn percent(x: f64) -> String {
    format!("{:.0}%", 100.0 * x)
}

/// A count with thousands separated, as "4,513".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Each action's share, largest first (and by name among equals).
fn by_share(shares: &BTreeMap<String, f64>) -> Vec<NextShare> {
    let mut v: Vec<NextShare> = shares
        .iter()
        .map(|(action, share)| NextShare {
            action: action.clone(),
            share: *share,
        })
        .collect();
    v.sort_by(|a, b| b.share.total_cmp(&a.share).then(a.action.cmp(&b.action)));
    v
}

/// The first few of `next`, as "a 71%, respond 20%, …".
fn top_shares(next: &[NextShare], n: usize) -> String {
    next.iter()
        .take(n)
        .map(|s| format!("{} {}", s.action, percent(s.share)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether a flow's folds share one fit.
fn one_fit(flow: &Flow) -> bool {
    let fits: Vec<serde_json::Value> = flow
        .folds
        .iter()
        .map(|f| serde_json::to_value(f).unwrap_or_default())
        .collect();
    fits.windows(2).all(|w| w[0] == w[1])
}

/// Each tool's code feature fields, as "`status`", "`len(items)`".
fn feature_fields(flow: &Flow) -> BTreeMap<String, Vec<String>> {
    flow.map
        .fields()
        .iter()
        .map(|(t, fs)| (t.clone(), fs.iter().map(|f| format!("`{f}`")).collect()))
        .collect()
}

/// A flow as a reviewer reads it, as data, for `stretto-console`: the tools
/// it knows, what it does after each call and why, where its lookups'
/// arguments come from, and where it came from. [`view`] builds it, and
/// [`show`] renders its sites from it, so the two cannot disagree.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FlowView {
    /// The threshold the flow will be served with, which the sites are
    /// judged at.
    pub threshold: f64,
    /// Every tool the flow knows, by name.
    pub tools: Vec<FlowTool>,
    /// After each call: the lookups the flow may make next, and what it
    /// does there.
    pub sites: Vec<SiteView>,
    /// Where each lookup's arguments come from, by lookup.
    pub bindings: Vec<BindingView>,
    /// Where the flow came from.
    pub provenance: Provenance,
    /// Where it may act, if it was promoted.
    pub promotion: Option<PromotionView>,
}

/// A tool the flow knows.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FlowTool {
    pub name: String,
    /// `read` tools are the only ones the flow may call.
    pub kind: ToolKind,
    /// What the server's documentation says it does, if anything.
    pub summary: Option<String>,
    /// Each argument's description, where the server gave one.
    pub args: BTreeMap<String, String>,
    /// Its input contract as the flow pins it (`contracts`), such as
    /// `order_id:string!`.
    pub contract: Option<String>,
}

/// What a site's shares are weighed with: `reach` for a flow served with it
/// by default, else the habit alone (for a flow with an arbiter, that
/// leaves out the model it asks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum WeighedBy {
    /// How often each lookup came before the agent's next write.
    Reach,
    /// What the agent did next.
    Habit,
}

/// One site: the calls to a tool, failed or not, after which the flow may
/// make a lookup.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SiteView {
    /// The site's name: the tool, with ` (error)` after a failed call.
    pub name: String,
    pub tool: String,
    pub failed: bool,
    /// The agent's steps just after such a call in training.
    pub steps: usize,
    pub weighed_by: WeighedBy,
    /// What the agent did next in training: each action's share of `steps`,
    /// largest first. `respond` is a message to the customer.
    pub next: Vec<NextShare>,
    /// The lookups the flow may make here.
    pub lookups: Vec<LookupView>,
    /// The share of those steps that were none of the lookups, which the
    /// flow can only hand back for.
    pub hand_back_share: f64,
    /// The likeliest lookup, as `weighed_by` weighs it, and whether the flow
    /// makes it at the threshold; `None` where no lookup is offered.
    pub choice: Option<Choice>,
    /// The site's record, if the flow was promoted and the site scored.
    pub promoted: Option<SiteRecord>,
    /// The site's own threshold, if a search set one; above 1 the site is
    /// switched off.
    pub threshold: Option<f64>,
    /// Whether the flow may act here at all: false after a site a promotion
    /// left out, or one a search switched off.
    pub active: bool,
    /// What the flow does here, as `stretto flow-show` words it.
    pub verdict: String,
}

/// An action's share of the steps after a site.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NextShare {
    pub action: String,
    pub share: f64,
}

/// A lookup offered at a site.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LookupView {
    pub tool: String,
    /// How often the agent made it next here in training.
    pub count: usize,
    /// Its share of what the agent did next here.
    pub share: f64,
    /// Its share as the site is weighed: with `reach`, of the times the agent
    /// made the call before its next write; else `share`.
    pub weighed_share: f64,
    /// The chance that the arguments the flow binds are the agent's, if it
    /// can bind them.
    pub binding_chance: Option<f64>,
    pub bindable: bool,
    /// `weighed_share` times `binding_chance`: what the flow compares with
    /// the threshold.
    pub prob: f64,
    /// Whether the flow makes this lookup here at the threshold: it is the
    /// site's choice, clears the threshold, and the site is active.
    pub acts: bool,
}

/// What the flow picks at a site.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Choice {
    /// The likeliest lookup.
    pub tool: Option<String>,
    /// Its probability, as [`LookupView::prob`].
    pub prob: f64,
    /// Whether the flow makes it; otherwise it hands back.
    pub acts: bool,
}

/// How a lookup's arguments are bound.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BindingView {
    /// The lookup.
    pub tool: String,
    /// The agent's calls to it in training.
    pub calls: usize,
    /// The chance that the bound arguments are the agent's: when the
    /// customer had not mentioned the values picked, when they had, and
    /// over both; `None` where the flow cannot bind them.
    pub chance: Option<[f64; 3]>,
    /// `[agreed, tried]` behind the chances, not mentioned and mentioned.
    pub agreed: [[usize; 2]; 2],
    /// Every argument the agent passed.
    pub args: Vec<ArgView>,
}

/// One argument of a lookup.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ArgView {
    pub name: String,
    /// Whether the flow passes it: the agent passed it in at least 90% of
    /// its calls.
    pub required: bool,
    /// The agent's calls that passed it.
    pub calls: usize,
    /// The value the binding passes as the agent always did
    /// (`learn --constants`).
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub constant: Option<Value>,
    /// Where the flow binds a required argument from.
    pub sources: Vec<SourceView>,
}

/// A source of an argument's values: an earlier output of `tool`, at `path`.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SourceView {
    pub tool: String,
    pub path: String,
    /// Training values found there.
    pub count: usize,
    /// Their share of the argument's values.
    pub share: f64,
}

/// A promotion, in short.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PromotionView {
    /// The bar each site had to meet.
    pub bar: Bar,
    pub sites_promoted: usize,
    pub sites_scored: usize,
}

/// `flow` as a reviewer reads it, as data ([`FlowView`]), judged at
/// `threshold`, the one it will be served with.
pub fn view(flow: &Flow, threshold: f64) -> FlowView {
    let bound = bindings(flow);
    let weighed_by = if weighs_reach(flow) {
        WeighedBy::Reach
    } else {
        WeighedBy::Habit
    };
    let sites = flow
        .sites
        .next()
        .iter()
        .map(|((tool, failed), seen)| {
            let failed = *failed;
            let name = Sites::name(tool, failed);
            let (shares, total) = followed(flow, tool, failed);
            let weighs = weighed(flow, tool, failed, &shares);
            let best = likely(flow, tool, failed, &weighs, &bound);
            let active = acts(flow, tool, failed);
            let at = threshold_at(flow, tool, failed, threshold);
            let choice = best.as_ref().map(|l| Choice {
                tool: Some(l.tool.clone()),
                prob: l.prob(),
                acts: active && l.prob() >= at,
            });
            let options = flow.sites.options(tool, failed);
            let lookups: Vec<LookupView> = options
                .iter()
                .map(|o| {
                    let chance = binding_chance(flow, o, &bound);
                    let weighed_share = weighs.get(o).copied().unwrap_or(0.0);
                    LookupView {
                        tool: o.clone(),
                        count: seen.get(o).copied().unwrap_or(0),
                        share: shares.get(o).copied().unwrap_or(0.0),
                        weighed_share,
                        binding_chance: chance,
                        bindable: chance.is_some(),
                        prob: weighed_share * chance.unwrap_or(0.0),
                        acts: choice
                            .as_ref()
                            .is_some_and(|c| c.acts && c.tool.as_deref() == Some(o.as_str())),
                    }
                })
                .collect();
            let looked: f64 = options
                .iter()
                .map(|o| shares.get(o).copied().unwrap_or(0.0))
                .sum();
            SiteView {
                tool: tool.clone(),
                failed,
                steps: total.round() as usize,
                weighed_by,
                next: by_share(&shares),
                lookups,
                hand_back_share: (1.0 - looked).max(0.0),
                promoted: flow.promotion().and_then(|p| p.sites.get(&name).cloned()),
                threshold: flow.thresholds().get(&name).copied(),
                active,
                verdict: action(flow, tool, failed, best.as_ref(), threshold),
                choice,
                name,
            }
        })
        .collect();
    let bindings = bound
        .values()
        .map(|b| {
            let (calls, passed) = flow
                .bindings
                .passed(&b.tool)
                .map_or((b.calls, BTreeMap::new()), |(n, args)| (n, args.clone()));
            let args = passed
                .iter()
                .map(|(arg, n)| {
                    let required = b.arguments.iter().find(|a| a.name == *arg);
                    ArgView {
                        name: arg.clone(),
                        required: required.is_some(),
                        calls: *n,
                        constant: flow
                            .bindings
                            .constants()
                            .get(&(b.tool.clone(), arg.clone()))
                            .cloned(),
                        sources: required
                            .map(|a| {
                                a.sources
                                    .iter()
                                    .map(|(t, path, count)| SourceView {
                                        tool: t.clone(),
                                        path: path.clone(),
                                        count: *count,
                                        share: if a.values > 0 {
                                            *count as f64 / a.values as f64
                                        } else {
                                            0.0
                                        },
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    }
                })
                .collect();
            let [(a, n), (c, m)] = b.agreed;
            BindingView {
                tool: b.tool.clone(),
                calls,
                chance: b.bindable().then(|| b.chance()),
                agreed: [[a, n], [c, m]],
                args,
            }
        })
        .collect();
    let tools = flow
        .manifest
        .tools
        .iter()
        .map(|(name, kind)| {
            let doc = flow.manifest.docs.get(name);
            FlowTool {
                name: name.clone(),
                kind: *kind,
                summary: doc.map(|d| d.summary.clone()).filter(|s| !s.is_empty()),
                args: doc.map(|d| d.args.clone()).unwrap_or_default(),
                contract: flow.contracts().get(name).cloned(),
            }
        })
        .collect();
    FlowView {
        threshold,
        tools,
        sites,
        bindings,
        provenance: flow.provenance.clone(),
        promotion: flow.promotion().map(|p| PromotionView {
            bar: p.bar,
            sites_promoted: p.sites.values().filter(|r| r.promoted).count(),
            sites_scored: p.sites.len(),
        }),
    }
}

/// One flow, as a reviewer reads it (Markdown). `threshold` is the one the
/// flow will be served with (`stretto-proxy --flow-threshold`).
pub fn show(flow: &Flow, threshold: f64) -> String {
    let mut md = String::new();
    let p = &flow.provenance;
    let _ = writeln!(md, "# Flow: {}\n", flow.domain());
    let _ = writeln!(
        md,
        "Written by stretto {} from {}. The habit learned from {} successful sessions or episodes. {}{}\n",
        p.stretto,
        if p.sources.is_empty() {
            "no named source".to_string()
        } else {
            p.sources.join(", ")
        },
        thousands(p.habit_episodes),
        match flow.served_decider() {
            Decider::Arbiter => format!(
                "Its arbiter was fitted on {} held-out decisions and asks `{}`.",
                thousands(p.arbiter_cases),
                flow.model
            ),
            Decider::Reach => "It has no arbiter and asks no one: it decides by how often each lookup came before the agent's next write (`reach`).".to_string(),
            Decider::Habit => "It has no arbiter: it decides with the habit alone and asks no one.".to_string(),
        },
        if flow.has_arbiter() && flow.has_reach() {
            " It also counts how often each action came before the agent's next write, which `--flow-decider reach` serves."
        } else {
            ""
        }
    );
    let _ = writeln!(md, "## Tools\n");
    for (label, kind) in [
        ("Read, the only tools it may call", ToolKind::Read),
        ("Write, never called", ToolKind::Write),
        ("Neither, never called", ToolKind::Generic),
    ] {
        let tools: Vec<String> = flow
            .manifest
            .tools
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(t, _)| format!("`{t}`"))
            .collect();
        if !tools.is_empty() {
            let _ = writeln!(md, "- **{label}:** {}", tools.join(", "));
        }
    }
    let _ = writeln!(md, "\n## Run\n");
    let _ = writeln!(
        md,
        "What the flow does after each call, as a fugue program (`program`): `Decide` is a decision between handing back (0) and the lookups offered after the call just made, and `Outcome` whether a lookup succeeds. The proxy decides with the decider it serves the flow with (`--flow-decider`: the arbiter, the habit alone, or `reach`) and takes each outcome from the server. {}\n",
        if flow.program == crate::program::standard() {
            "This is the standard run: decide, look up, and decide again, until the flow hands back or has made `max_lookups` lookups (`--flow-per-call`)."
        } else {
            "**This is not the standard run.** Review it as code the flow runs."
        }
    );
    let _ = writeln!(md, "~~~text\n{}\n~~~", flow.program);
    let _ = writeln!(md, "\n## Sites\n");
    if flow.sites.offers_every_read() {
        let _ = writeln!(
            md,
            "Every read tool is offered after every call (`--manifest-options`); the table lists those training showed.\n"
        );
    }
    let _ = writeln!(
        md,
        "After each call: the lookups the flow may make next, what the agent did next in training, and what the flow does there with {} at a threshold of {threshold}, which is the likeliest lookup's share{} times the chance its bound arguments are the agent's. The share pools over the calls before and the code features, so a live decision near the threshold can go either way.\n",
        weighed_by(flow),
        if weighs_reach(flow) {
            " of the times the agent made it before its next write,"
        } else {
            ""
        }
    );
    if !flow.thresholds().is_empty() {
        let _ = writeln!(
            md,
            "A search set the threshold at {} of these sites (`thresholds`), and the table uses those. A site set above 1 is switched off.\n",
            flow.thresholds().len()
        );
    }
    let _ = writeln!(
        md,
        "| After | Lookups it may make next (times seen) | What the agent did next in training | With {} |\n|---|---|---|---|",
        weighed_by(flow)
    );
    for site in view(flow, threshold).sites {
        // The lookups training showed; with every read offered, the others
        // were never seen here.
        let offered: Vec<String> = site
            .lookups
            .iter()
            .filter(|l| l.count > 0)
            .map(|l| format!("`{}` ({})", l.tool, l.count))
            .collect();
        let _ = writeln!(
            md,
            "| {} | {} | {} (of {}) | {} |",
            site_name(&site.tool, site.failed),
            offered.join(", "),
            top_shares(&site.next, 4),
            thousands(site.steps),
            site.verdict
        );
    }
    let bound = bindings(flow);
    if let Some(p) = flow.promotion() {
        let _ = writeln!(md, "\n## Promotion\n");
        let _ = writeln!(
            md,
            "The flow acts only after the promoted calls, and hands back after the rest (`stretto promote`).\n"
        );
        md.push_str(&crate::promote::markdown(p));
    }
    let _ = writeln!(md, "\n## Bindings\n");
    let named_other = bound.values().any(|b| b.named_other.is_some());
    let described = bound.values().any(|b| b.described_read.is_some());
    let _ = writeln!(
        md,
        "Where each lookup's required arguments come from, and how often binding them that way gave the agent's own arguments in training, when the customer had not mentioned the values and when they had. With nothing tried, the chance is 1/2.{}{}\n",
        if named_other {
            " Where the customer had named another value at the sources, a column counts the values the binding would pass instead, and how many of them the agent went on to pass: the chance there."
        } else {
            ""
        },
        if described {
            " Where the record the customer described had been read (another of the same list returned a value they gave, such as their phone number), a column counts the same for the next of that list."
        } else {
            ""
        }
    );
    let mut header = "| Lookup | Argument | Bound from (values found there, of the values it took) | Chance it is the agent's (right/tried): not mentioned, mentioned |".to_string();
    let mut rule = "|---|---|---|---|".to_string();
    if named_other {
        header.push_str(" Another named (used/passed) |");
        rule.push_str("---|");
    }
    if described {
        header.push_str(" The described record read (used/passed) |");
        rule.push_str("---|");
    }
    let _ = writeln!(md, "{header}\n{rule}");
    for b in bound.values() {
        binding_rows(&mut md, b, named_other, described);
    }
    let constants = flow.bindings.constants();
    if !constants.is_empty() {
        let _ = writeln!(md, "\n## Constants\n");
        let _ = writeln!(
            md,
            "Arguments the agent passed with one value in every call of a lookup (`learn --constants`), which the binding passes as it did:\n"
        );
        for ((tool, arg), value) in constants {
            let _ = writeln!(md, "- `{tool}` `{arg}`: `{value}`");
        }
    }
    let orders = flow.bindings.site_orders();
    if !orders.is_empty() {
        let _ = writeln!(md, "\n## Sources by site\n");
        let _ = writeln!(
            md,
            "Where an argument has more than one source, the order the binding tries them at a site: the sources the agent took its values from there, most used first, each the most recent output first. At other sites, the most recent output first.\n"
        );
        for o in &orders {
            let list: Vec<String> = o
                .sources
                .iter()
                .map(|((t, p), n)| format!("`{t}` `{p}` ({n})"))
                .collect();
            let _ = writeln!(
                md,
                "- `{}` `{}`, after `{}`: {}",
                o.tool,
                o.arg,
                o.site,
                list.join(", ")
            );
        }
    }
    let pinned: Vec<(&String, &String)> = flow
        .contracts()
        .iter()
        .filter(|(t, _)| bound.contains_key(*t))
        .collect();
    if !pinned.is_empty() {
        let _ = writeln!(md, "\n## Pinned inputs\n");
        let _ = writeln!(
            md,
            "Each lookup's arguments as the server listed them when the sessions were recorded. `stretto-proxy` makes no lookup of a tool whose server lists other arguments now.\n"
        );
        for (tool, contract) in pinned {
            let _ = writeln!(md, "- `{tool}`: `{contract}`");
        }
    }
    let features = feature_fields(flow);
    if !features.is_empty() {
        let _ = writeln!(md, "\n## Code features\n");
        let _ = writeln!(
            md,
            "Output fields that sharpen the habit. Their values are copied from training outputs into the flow (`map.ids`).\n"
        );
        for (tool, fields) in &features {
            let _ = writeln!(
                md,
                "- `{tool}`: {}; {} combinations of values",
                fields.join(", "),
                flow.map.combinations(tool)
            );
        }
    }
    if flow.has_arbiter() {
        let _ = writeln!(md, "\n## Arbiter\n");
        let _ = writeln!(
            md,
            "{}\n",
            if one_fit(flow) {
                "One fit, which judges every session."
            } else {
                "Folds fitted apart, each judging the tasks the others saw; the weights are their mean."
            }
        );
        let _ = writeln!(md, "| Weight on | Value |\n|---|---|");
        for (name, w) in weight_names(flow).iter().zip(weights(flow)) {
            let _ = writeln!(md, "| {name} | {w:.3} |");
        }
        let n = flow.folds.len() as f64;
        let (agree, cover) = flow.folds.iter().fold((0.0, 0.0), |(a, c), f| {
            let (x, y, _) = f.record();
            (a + x / n, c + y / n)
        });
        let sites: BTreeSet<String> = flow
            .folds
            .iter()
            .flat_map(|f| f.record().2.into_keys())
            .collect();
        let _ = writeln!(
            md,
            "\nThe System-One model's record, at the held-out decisions of {} sites: its pick was the agent's step at {}, and the agent's step was among its options at {}.",
            sites.len(),
            percent(agree),
            percent(cover)
        );
        if !flow.predicates.is_empty() {
            let _ = writeln!(
                md,
                "\nThe predicates it asks with each next-step question, with the conversation so far:\n"
            );
            for q in &flow.predicates {
                let weighed = if flow.weighed.contains(q) {
                    "weighed"
                } else {
                    "asked, not weighed"
                };
                let _ = writeln!(md, "- `{}` ({weighed}): {}", q.id, q.question);
            }
        }
    }
    md
}

fn binding_rows(md: &mut String, b: &LookupBinding, named_other: bool, described: bool) {
    let [not, mentioned, _] = b.chance();
    let [(a, n), (c, m)] = b.agreed;
    let mut chance = if b.bindable() {
        format!("{not:.2} ({a}/{n}), {mentioned:.2} ({c}/{m})")
    } else {
        "—".to_string()
    };
    if named_other {
        let other = match (b.bindable(), b.named_other, b.chance_named_other()) {
            (true, Some((u, k)), Some(p)) => format!("{p:.2} ({u}/{k})"),
            (true, _, _) => format!("{not:.2}, as not mentioned"),
            _ => "—".to_string(),
        };
        chance = format!("{chance} | {other}");
    }
    if described {
        let read = match (b.bindable(), b.described_read, b.chance_described_read()) {
            (true, Some((u, k)), Some(p)) => format!("{p:.2} ({u}/{k})"),
            (true, _, _) => format!("{not:.2}, as not mentioned"),
            _ => "—".to_string(),
        };
        chance = format!("{chance} | {read}");
    }
    if b.arguments.is_empty() {
        let _ = writeln!(md, "| `{}` | none | made once | {chance} |", b.tool);
    }
    for arg in &b.arguments {
        let from = if arg.sources.is_empty() {
            "nothing: the flow never makes this lookup".to_string()
        } else {
            arg.sources
                .iter()
                .map(|(t, path, n)| format!("`{t}` at `{path}` ({n} of {})", arg.values))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let _ = writeln!(md, "| `{}` | `{}` | {from} | {chance} |", b.tool, arg.name);
    }
}

/// What changed between two flows.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FlowDiff {
    /// The change list (Markdown).
    pub markdown: String,
    /// The changes a reviewer must look at: the flow runs another program,
    /// or may call a tool, make a lookup, bind an argument from a source, or
    /// ask a model or a question it did not before.
    pub needs_review: Vec<String>,
    /// Every change, under the headings the change list gives them; only
    /// the headings with changes.
    pub sections: Vec<DiffSection>,
}

/// The changes under one heading of a change list.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiffSection {
    pub title: String,
    pub changes: Vec<String>,
}

impl FlowDiff {
    /// Every change, heading by heading.
    pub fn changes(&self) -> Vec<String> {
        self.sections
            .iter()
            .flat_map(|s| s.changes.iter().cloned())
            .collect()
    }
}

/// What changed from `old` to `new`. Shares, chances and weights that moved
/// by less than `tolerance` are left out; `threshold` is as for [`show`].
pub fn diff(old: &Flow, new: &Flow, tolerance: f64, threshold: f64) -> FlowDiff {
    let mut review: Vec<String> = Vec::new();
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();

    // Tools and their kinds.
    let mut tools = Vec::new();
    let names: BTreeSet<&String> = old
        .manifest
        .tools
        .keys()
        .chain(new.manifest.tools.keys())
        .collect();
    for t in names {
        let (a, b) = (old.manifest.kind(t), new.manifest.kind(t));
        if a != b {
            tools.push(format!("`{t}`: {} → {}", kind_name(a), kind_name(b)));
            if b == Some(ToolKind::Read) {
                review.push(format!(
                    "`{t}` is now marked read-only, so the flow may call it"
                ));
            }
        }
    }
    sections.push(("Tools".to_string(), tools));

    // The run: the program, line by line.
    let mut run = Vec::new();
    if old.program != new.program {
        let (a, b) = (old.program.to_string(), new.program.to_string());
        let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
        for line in la.iter().filter(|l| !lb.contains(l)) {
            run.push(format!("no longer runs `{}`", line.trim()));
        }
        for line in lb.iter().filter(|l| !la.contains(l)) {
            run.push(format!("now runs `{}`", line.trim()));
        }
        if run.is_empty() {
            run.push("runs the same lines in another order".to_string());
        }
        review.push("the flow's run changed: its program is not the one it was".to_string());
    }
    sections.push(("Run".to_string(), run));

    // Sites and the lookups offered there.
    let mut sites = Vec::new();
    if new.sites.offers_every_read() && !old.sites.offers_every_read() {
        sites.push("every read tool is now offered after every call".to_string());
        review.push("every read tool is now offered after every call".to_string());
    }
    if old.sites.offers_every_read() && !new.sites.offers_every_read() {
        sites.push("only the lookups training showed are offered now".to_string());
    }
    let keys: BTreeSet<&(String, bool)> = old
        .sites
        .next()
        .keys()
        .chain(new.sites.next().keys())
        .collect();
    for key in keys {
        let site = site_name(&key.0, key.1);
        let (a, b) = (old.sites.next().get(key), new.sites.next().get(key));
        let la: BTreeSet<&String> = a.map(|m| m.keys().collect()).unwrap_or_default();
        let lb: BTreeSet<&String> = b.map(|m| m.keys().collect()).unwrap_or_default();
        for l in lb.difference(&la) {
            sites.push(format!("after {site}: may now look up `{l}`"));
            review.push(format!("a new lookup: `{l}` after {site}"));
        }
        for l in la.difference(&lb) {
            sites.push(format!("after {site}: no longer looks up `{l}`"));
        }
        // Where it acts: a site promoted, or promotion lifted.
        if a.is_some() && b.is_some() {
            match (acts(old, &key.0, key.1), acts(new, &key.0, key.1)) {
                (false, true) => {
                    sites.push(format!("after {site}: now acts"));
                    review.push(format!(
                        "the flow now acts after {site}, where it handed back"
                    ));
                }
                (true, false) => sites.push(format!(
                    "after {site}: no longer acts ({})",
                    if switched_off(new, &key.0, key.1) {
                        "switched off"
                    } else {
                        "not promoted"
                    }
                )),
                _ => {}
            }
        }
    }
    // Per-site thresholds a search set.
    let set: BTreeSet<&String> = old
        .thresholds()
        .keys()
        .chain(new.thresholds().keys())
        .collect();
    let describe = |t: Option<&f64>| match t {
        None => "the served threshold".to_string(),
        Some(t) if *t > 1.0 => "off".to_string(),
        Some(t) => format!("{t:.2}"),
    };
    for site in set {
        let (a, b) = (old.thresholds().get(site), new.thresholds().get(site));
        if a != b {
            sites.push(format!(
                "after `{site}`: {} (was: {})",
                describe(b),
                describe(a)
            ));
        }
    }
    sections.push(("Sites".to_string(), sites));

    // What the flow does after each call with the habit alone or `reach`,
    // and what the agent did next in training.
    let (bound_old, bound_new) = (bindings(old), bindings(new));
    let mut habit = Vec::new();
    let mut next = Vec::new();
    for key in new.sites.next().keys() {
        let (tool, failed) = (&key.0, key.1);
        let site = site_name(tool, failed);
        let (sb, _) = followed(new, tool, failed);
        let lb = likely(
            new,
            tool,
            failed,
            &weighed(new, tool, failed, &sb),
            &bound_new,
        );
        let clears_b = lb
            .as_ref()
            .is_some_and(|l| l.prob() >= threshold_at(new, tool, failed, threshold));
        let clears_b = clears_b && acts(new, tool, failed);
        if !old.sites.next().contains_key(key) {
            if clears_b {
                habit.push(format!(
                    "after {site}, a new site: {}",
                    action(new, tool, failed, lb.as_ref(), threshold)
                ));
            }
            continue;
        }
        let (sa, _) = followed(old, tool, failed);
        let la = likely(
            old,
            tool,
            failed,
            &weighed(old, tool, failed, &sa),
            &bound_old,
        );
        let clears_a = la
            .as_ref()
            .is_some_and(|l| l.prob() >= threshold_at(old, tool, failed, threshold))
            && acts(old, tool, failed);
        let same_tool = la.as_ref().map(|l| &l.tool) == lb.as_ref().map(|l| &l.tool);
        if clears_a != clears_b || (clears_b && !same_tool) {
            habit.push(format!(
                "after {site}: {} (was: {})",
                action(new, tool, failed, lb.as_ref(), threshold),
                action(old, tool, failed, la.as_ref(), threshold)
            ));
        }
        let actions: BTreeSet<&String> = sa.keys().chain(sb.keys()).collect();
        for act in actions {
            let (x, y) = (
                sa.get(act).copied().unwrap_or(0.0),
                sb.get(act).copied().unwrap_or(0.0),
            );
            if (x - y).abs() >= tolerance {
                next.push(format!(
                    "after {site}: {act} {} → {}",
                    percent(x),
                    percent(y)
                ));
            }
        }
    }
    sections.push((format!("With {}, at {threshold}", weighed_by(new)), habit));
    sections.push(("What the agent did next in training".to_string(), next));

    // Bindings.
    let mut binds = Vec::new();
    let lookups: BTreeSet<&String> = bound_old.keys().chain(bound_new.keys()).collect();
    let sources = |b: Option<&LookupBinding>| -> BTreeSet<(String, String, String)> {
        b.map(|b| {
            b.arguments
                .iter()
                .flat_map(|a| {
                    a.sources
                        .iter()
                        .map(move |(t, p, _)| (a.name.clone(), t.clone(), p.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
    };
    for l in lookups {
        let (x, y) = (bound_old.get(l), bound_new.get(l));
        let (sa, sb) = (sources(x), sources(y));
        for (arg, t, p) in sb.difference(&sa) {
            binds.push(format!("`{l}` binds `{arg}` from `{t}` at `{p}`"));
            review.push(format!(
                "a new binding: `{l}`'s `{arg}` from `{t}` at `{p}`"
            ));
        }
        for (arg, t, p) in sa.difference(&sb) {
            binds.push(format!("`{l}` no longer binds `{arg}` from `{t}` at `{p}`"));
        }
        if let (Some(x), Some(y)) = (x, y) {
            let (cx, cy) = (x.chance(), y.chance());
            for (i, when) in ["not mentioned", "mentioned"].iter().enumerate() {
                if (cx[i] - cy[i]).abs() >= tolerance {
                    binds.push(format!(
                        "`{l}`'s chance ({when}): {:.2} → {:.2}",
                        cx[i], cy[i]
                    ));
                }
            }
            // Where another value was named, a flow without the count used
            // the unmentioned chance.
            let (ox, oy) = (
                x.chance_named_other().unwrap_or(cx[0]),
                y.chance_named_other().unwrap_or(cy[0]),
            );
            if (x.named_other.is_some() || y.named_other.is_some()) && (ox - oy).abs() >= tolerance
            {
                binds.push(format!("`{l}`'s chance (another named): {ox:.2} → {oy:.2}"));
            }
            let (dx, dy) = (
                x.chance_described_read().unwrap_or(cx[0]),
                y.chance_described_read().unwrap_or(cy[0]),
            );
            if (x.described_read.is_some() || y.described_read.is_some())
                && (dx - dy).abs() >= tolerance
            {
                binds.push(format!(
                    "`{l}`'s chance (the described record read): {dx:.2} → {dy:.2}"
                ));
            }
        }
    }
    sections.push(("Bindings".to_string(), binds));

    // The source a binding tries first at each site where it orders them.
    let first = |f: &Flow| -> BTreeMap<(String, String, String), (String, String)> {
        f.bindings
            .site_orders()
            .into_iter()
            .map(|o| ((o.tool, o.arg, o.site), o.sources[0].0.clone()))
            .collect()
    };
    let (was, now) = (first(old), first(new));
    let mut by_site = Vec::new();
    for k in was.keys().chain(now.keys()).collect::<BTreeSet<_>>() {
        let (tool, arg, site) = k;
        match (was.get(k), now.get(k)) {
            (Some((a, p)), Some((b, q))) if (a, p) != (b, q) => by_site.push(format!(
                "`{tool}` `{arg}` after `{site}`: first from `{a}` `{p}` → `{b}` `{q}`"
            )),
            (None, Some((b, q))) => by_site.push(format!(
                "`{tool}` `{arg}` after `{site}`: first from `{b}` `{q}` (was: the most recent output)"
            )),
            (Some((a, p)), None) => by_site.push(format!(
                "`{tool}` `{arg}` after `{site}`: the most recent output first (was: `{a}` `{p}`)"
            )),
            _ => {}
        }
    }
    sections.push(("Sources by site".to_string(), by_site));

    // Constants the binding passes as the agent always did (`learn --constants`).
    // A new or changed one needs review: it could be a value of one user's
    // that every training session happened to share.
    let mut constants = Vec::new();
    let (ca, cb) = (old.bindings.constants(), new.bindings.constants());
    for key in ca.keys().chain(cb.keys()).collect::<BTreeSet<_>>() {
        let (tool, arg) = key;
        match (ca.get(key), cb.get(key)) {
            (Some(a), Some(b)) if a != b => {
                constants.push(format!("`{tool}` `{arg}`: `{a}` → `{b}`"));
                review.push(format!(
                    "a changed constant: `{tool}`'s `{arg}` is now `{b}`"
                ));
            }
            (None, Some(b)) => {
                constants.push(format!("`{tool}` `{arg}`: `{b}`"));
                review.push(format!(
                    "a new constant: `{tool}`'s `{arg}` is always `{b}`"
                ));
            }
            (Some(a), None) => {
                constants.push(format!("`{tool}` `{arg}` no longer passes `{a}`"));
            }
            _ => {}
        }
    }
    sections.push(("Constants".to_string(), constants));

    // Pinned input contracts: the server's arguments when each flow learned.
    let mut pinned = Vec::new();
    let tools: BTreeSet<&String> = old
        .contracts()
        .keys()
        .chain(new.contracts().keys())
        .collect();
    for t in tools {
        match (old.contracts().get(t), new.contracts().get(t)) {
            (Some(a), Some(b)) if a != b => pinned.push(format!("`{t}`: `{a}` → `{b}`")),
            (None, Some(b)) if !old.contracts().is_empty() => {
                pinned.push(format!("`{t}` pinned: `{b}`"))
            }
            (Some(_), None) if !new.contracts().is_empty() => {
                pinned.push(format!("`{t}` no longer pinned"))
            }
            _ => {}
        }
    }
    sections.push(("Pinned inputs".to_string(), pinned));

    // Code features.
    let mut features = Vec::new();
    let (fa, fb) = (feature_fields(old), feature_fields(new));
    let with_features: BTreeSet<&String> = fa.keys().chain(fb.keys()).collect();
    for t in with_features {
        let (a, b) = (fa.get(t), fb.get(t));
        if a != b {
            let list = |f: Option<&Vec<String>>| f.map_or("none".to_string(), |f| f.join(", "));
            features.push(format!("`{t}`: {} → {}", list(a), list(b)));
        }
    }
    sections.push(("Code features".to_string(), features));

    // How it decides.
    let mut arbiter = Vec::new();
    match (old.has_arbiter(), new.has_arbiter()) {
        (false, true) => {
            let asked = if new.predicates.is_empty() {
                String::new()
            } else {
                format!(", with the predicates {}", ids(&new.predicates))
            };
            arbiter.push(format!(
                "now has an arbiter, fitted on {} held-out decisions, which asks `{}`{asked}",
                thousands(new.provenance.arbiter_cases),
                new.model
            ));
            review.push(format!(
                "the flow now asks `{}` at each decision{asked}",
                new.model
            ));
        }
        (true, false) => arbiter.push(format!(
            "no longer has an arbiter: {} decides",
            weighed_by(new)
        )),
        (true, true) => {
            if old.model != new.model {
                arbiter.push(format!("asks `{}` instead of `{}`", new.model, old.model));
                review.push(format!("the System-One model is now `{}`", new.model));
            }
            let asked: Vec<Predicate> = new
                .predicates
                .iter()
                .filter(|q| !old.predicates.contains(q))
                .cloned()
                .collect();
            if !asked.is_empty() {
                arbiter.push(format!("asks new or reworded predicates: {}", ids(&asked)));
                review.push(format!(
                    "the flow asks new or reworded predicates: {}",
                    ids(&asked)
                ));
            }
            let dropped: Vec<Predicate> = old
                .predicates
                .iter()
                .filter(|q| !new.predicates.iter().any(|n| n.id == q.id))
                .cloned()
                .collect();
            if !dropped.is_empty() {
                arbiter.push(format!("no longer asks {}", ids(&dropped)));
            }
            let same_weights = old
                .weighed
                .iter()
                .map(|q| &q.id)
                .eq(new.weighed.iter().map(|q| &q.id));
            if same_weights {
                for (name, (a, b)) in weight_names(new)
                    .iter()
                    .zip(weights(old).into_iter().zip(weights(new)))
                {
                    if (a - b).abs() >= tolerance {
                        arbiter.push(format!("weight on {name}: {a:.3} → {b:.3}"));
                    }
                }
            } else {
                arbiter.push(format!(
                    "weighs {} (was {})",
                    ids(&new.weighed),
                    ids(&old.weighed)
                ));
            }
        }
        (false, false) => {}
    }
    sections.push(("Arbiter".to_string(), arbiter));

    let mut promotion = Vec::new();
    let summary = |f: &Flow| match f.promotion() {
        None => "none: it may act after every call".to_string(),
        Some(p) => format!(
            "{} of {} sites scored (at least {:.0}% used, a lower bound of {:.2}, {} tasks, at {})",
            p.sites.values().filter(|r| r.promoted).count(),
            p.sites.len(),
            100.0 * p.bar.min_used,
            p.bar.min_lower,
            p.bar.min_tasks,
            p.bar.threshold
        ),
    };
    if old.promotion() != new.promotion() {
        promotion.push(format!("{} → {}", summary(old), summary(new)));
    }
    sections.push(("Promotion".to_string(), promotion));

    let (pa, pb) = (&old.provenance, &new.provenance);
    let mut provenance = Vec::new();
    if pa.sources != pb.sources {
        provenance.push(format!(
            "sources: {} → {}",
            pa.sources.join(", "),
            pb.sources.join(", ")
        ));
    }
    if pa.habit_episodes != pb.habit_episodes {
        provenance.push(format!(
            "the habit learned from {} → {} successful sessions or episodes",
            thousands(pa.habit_episodes),
            thousands(pb.habit_episodes)
        ));
    }
    if pa.arbiter_cases != pb.arbiter_cases {
        provenance.push(format!(
            "held-out decisions behind the arbiter: {} → {}",
            thousands(pa.arbiter_cases),
            thousands(pb.arbiter_cases)
        ));
    }
    sections.push(("Provenance".to_string(), provenance));

    let mut md = format!("# Flow diff: {}\n\n", new.domain());
    if review.is_empty() {
        md.push_str("Nothing needs review: the flow runs the same program, and calls no tool, makes no lookup, binds no argument and asks no model or question it did not before.\n");
    } else {
        md.push_str("**Needs review:**\n\n");
        for r in &review {
            let _ = writeln!(md, "- {r}");
        }
    }
    let sections: Vec<DiffSection> = sections
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(title, changes)| DiffSection { title, changes })
        .collect();
    for section in &sections {
        let _ = writeln!(md, "\n## {}\n", section.title);
        for i in &section.changes {
            let _ = writeln!(md, "- {i}");
        }
    }
    FlowDiff {
        markdown: md,
        needs_review: review,
        sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> Flow {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../docs/examples/{name}.flow.json"));
        Flow::load(&path).unwrap()
    }

    #[test]
    fn per_site_thresholds_show_and_diff() {
        let old = example("retail-5-sessions");
        let site = "get_user_details".to_string();
        assert!(old.sites().contains(&site));
        let new = old
            .clone()
            .with_thresholds(BTreeMap::from([(site.clone(), 2.0)]));
        assert!(show(&old, 0.3)
            .lines()
            .any(|l| l.contains("`get_user_details`") && l.contains("looks up")));
        assert!(show(&new, 0.3)
            .lines()
            .any(|l| l.contains("`get_user_details`") && l.contains("hands back: switched off")));
        assert!(!show(&old, 0.3).contains("A search set the threshold"));
        assert!(show(&new, 0.3).contains("A search set the threshold at 1 of these sites"));
        let d = diff(&old, &new, 0.05, 0.3);
        assert!(
            d.markdown
                .contains("- after `get_user_details`: off (was: the served threshold)"),
            "{}",
            d.markdown
        );
        assert!(d
            .markdown
            .contains("- after `get_user_details`: no longer acts (switched off)"));
        // Switching a site off acts less, so it needs no review; switching it
        // back on does.
        assert!(d.needs_review.is_empty(), "{:?}", d.needs_review);
        let back = diff(&new, &old, 0.05, 0.3);
        assert_eq!(back.needs_review.len(), 1, "{:?}", back.needs_review);
        assert!(back.needs_review[0].contains("now acts after `get_user_details`"));
        // Lowering a site's threshold moves the bar for a lookup the flow
        // could already make: listed, not flagged.
        let lower = old
            .clone()
            .with_thresholds(BTreeMap::from([(site.clone(), 0.1)]));
        let d = diff(&old, &lower, 0.05, 0.3);
        assert!(d
            .markdown
            .contains("- after `get_user_details`: 0.10 (was: the served threshold)"));
        assert!(d.needs_review.is_empty(), "{:?}", d.needs_review);
    }

    #[test]
    fn sources_ordered_by_site_show_and_diff() {
        let old = example("retail-10-sessions");
        assert!(!show(&old, 0.3).contains("## Sources by site"));
        let mut new = old.clone();
        let source = |from: &str, values| crate::flow::SiteSource {
            tool: "get_user_details".to_string(),
            arg: "user_id".to_string(),
            site: "find_user_id_by_email".to_string(),
            source: (from.to_string(), "$".to_string()),
            values,
        };
        new.bindings.site_sources = vec![
            source("find_user_id_by_email", 2),
            source("find_user_id_by_name_zip", 6),
        ];
        let md = show(&new, 0.3);
        assert!(
            md.contains("- `get_user_details` `user_id`, after `find_user_id_by_email`: `find_user_id_by_name_zip` `$` (6), `find_user_id_by_email` `$` (2)"),
            "{md}"
        );
        let d = diff(&old, &new, 0.05, 0.3);
        assert!(
            d.markdown.contains(
                "- `get_user_details` `user_id` after `find_user_id_by_email`: first from `find_user_id_by_name_zip` `$` (was: the most recent output)"
            ),
            "{}",
            d.markdown
        );
        assert!(d.needs_review.is_empty(), "{:?}", d.needs_review);
    }

    #[test]
    fn a_new_constant_needs_review() {
        let old = example("retail-5-sessions");
        let mut new = old.clone();
        new.bindings.constants.insert(
            ("get_order_details".to_string(), "page_size".to_string()),
            serde_json::json!(100),
        );
        let d = diff(&old, &new, 0.05, 0.3);
        assert_eq!(d.needs_review.len(), 1, "{:?}", d.needs_review);
        assert!(d.needs_review[0]
            .contains("a new constant: `get_order_details`'s `page_size` is always `100`"));
        assert!(show(&new, 0.3).contains("- `get_order_details` `page_size`: `100`"));
        // Dropping it makes fewer lookups the agent's own: listed, not flagged.
        let back = diff(&new, &old, 0.05, 0.3);
        assert!(back.needs_review.is_empty(), "{:?}", back.needs_review);
        assert!(back.markdown.contains("no longer passes `100`"));
    }

    #[test]
    fn the_view_and_the_review_agree() {
        for (name, threshold) in [
            ("retail-5-sessions", 0.3),
            ("retail-10-sessions", 0.3),
            ("retail-10-sessions", 0.9),
            ("retail-5-sessions-shipped-arbiter", 0.3),
        ] {
            let flow = example(name);
            let v = view(&flow, threshold);
            let md = show(&flow, threshold);
            assert_eq!(v.threshold, threshold);
            assert_eq!(
                v.sites.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                flow.sites
                    .next()
                    .keys()
                    .map(|(t, f)| Sites::name(t, *f))
                    .collect::<Vec<_>>()
            );
            for site in &v.sites {
                // Each site's row in the review ends with the view's verdict.
                let row = md
                    .lines()
                    .find(|l| l.starts_with(&format!("| {} |", site_name(&site.tool, site.failed))))
                    .unwrap_or_else(|| panic!("{name}: no row for {}\n{md}", site.name));
                assert!(row.ends_with(&format!("| {} |", site.verdict)), "{row}");
                let acting: Vec<&LookupView> = site.lookups.iter().filter(|l| l.acts).collect();
                match &site.choice {
                    Some(c) if c.acts => {
                        assert!(site.verdict.starts_with("looks up"), "{}", site.verdict);
                        assert_eq!(acting.len(), 1);
                        assert_eq!(Some(&acting[0].tool), c.tool.as_ref());
                        assert!(c.prob >= threshold);
                    }
                    _ => {
                        assert!(site.verdict.starts_with("hands back"), "{}", site.verdict);
                        assert!(acting.is_empty());
                    }
                }
                for l in &site.lookups {
                    let expected = l.weighed_share * l.binding_chance.unwrap_or(0.0);
                    assert!((l.prob - expected).abs() < 1e-12);
                    assert_eq!(l.bindable, l.binding_chance.is_some());
                }
                let shares: f64 = site.next.iter().map(|n| n.share).sum();
                assert!(site.steps == 0 || (shares - 1.0).abs() < 1e-9, "{shares}");
                assert!(site.next.windows(2).all(|w| w[0].share >= w[1].share));
                assert!((0.0..=1.0).contains(&site.hand_back_share));
                assert!(site.active && site.promoted.is_none() && site.threshold.is_none());
            }
            // Every lookup the review lists a binding for, with its required
            // arguments' sources.
            for b in &v.bindings {
                assert!(md.contains(&format!("| `{}` |", b.tool)), "{}", b.tool);
                assert!(b.args.iter().all(|a| a.required || a.sources.is_empty()));
            }
            assert_eq!(v.tools.len(), flow.manifest.tools.len());
            assert_eq!(v.provenance, flow.provenance);
            assert!(v.promotion.is_none());
        }
    }

    #[test]
    fn the_view_of_a_promoted_flow_and_of_a_site_switched_off() {
        let flow = example("retail-5-sessions");
        let site = "get_user_details".to_string();
        let record = |promoted| SiteRecord {
            decisions: 4,
            lookups: 3,
            used: 3,
            tasks: 3,
            lower: 0.5,
            promoted,
        };
        let promoted = flow.clone().with_promotion(Some(crate::flow::Promotion {
            bar: Bar {
                threshold: 0.3,
                min_used: 0.7,
                min_lower: 0.5,
                min_tasks: 3,
            },
            sites: BTreeMap::from([
                (site.clone(), record(true)),
                ("get_order_details".to_string(), record(false)),
            ]),
        }));
        let v = view(&promoted, 0.3);
        let p = v.promotion.as_ref().unwrap();
        assert_eq!((p.sites_promoted, p.sites_scored), (1, 2));
        for s in &v.sites {
            assert_eq!(s.active, s.name == site, "{}", s.name);
            assert_eq!(
                s.promoted.as_ref().map(|r| r.promoted),
                match s.name.as_str() {
                    "get_user_details" => Some(true),
                    "get_order_details" => Some(false),
                    _ => None,
                }
            );
            if !s.active {
                assert_eq!(s.verdict, "hands back: not promoted");
                assert!(s.lookups.iter().all(|l| !l.acts));
            }
        }
        let off = flow.with_thresholds(BTreeMap::from([(site.clone(), 2.0)]));
        let s = view(&off, 0.3)
            .sites
            .into_iter()
            .find(|s| s.name == site)
            .unwrap();
        assert_eq!((s.active, s.threshold), (false, Some(2.0)));
        assert_eq!(s.verdict, "hands back: switched off");
        assert!(!s.choice.unwrap().acts);
    }

    #[test]
    fn a_diff_lists_its_changes_by_heading() {
        let old = example("retail-5-sessions");
        let new = example("retail-10-sessions");
        let d = diff(&old, &new, 0.05, 0.3);
        assert!(!d.sections.is_empty());
        for section in &d.sections {
            assert!(!section.changes.is_empty());
            assert!(d.markdown.contains(&format!("\n## {}\n", section.title)));
            for c in &section.changes {
                assert!(d.markdown.contains(&format!("- {c}\n")), "{c}");
            }
        }
        assert_eq!(
            d.changes().len(),
            d.sections.iter().map(|s| s.changes.len()).sum::<usize>()
        );
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["needs_review"], serde_json::json!(d.needs_review));
        assert!(diff(&old, &old, 0.05, 0.3).sections.is_empty());
    }

    #[test]
    fn a_changed_program_needs_review() {
        let old = example("retail-5-sessions");
        let mut new = old.clone();
        assert!(diff(&old, &new, 0.05, 0.3).needs_review.is_empty());
        assert!(show(&old, 0.3).contains("This is the standard run"));
        new.program = fugue::program::Program::parse(
            &crate::program::PROGRAM.replace("0..max_lookups", "0..1"),
        )
        .unwrap();
        let d = diff(&old, &new, 0.05, 0.3);
        assert_eq!(d.needs_review.len(), 1, "{:?}", d.needs_review);
        assert!(
            d.markdown.contains("- now runs `for i in 0..1 {`"),
            "{}",
            d.markdown
        );
        assert!(d
            .markdown
            .contains("- no longer runs `for i in 0..max_lookups {`"));
        assert!(show(&new, 0.3).contains("**This is not the standard run.**"));
    }
}
