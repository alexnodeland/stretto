//! Reviewing flows as code (RFC-001 §6, question 4).
//!
//! A raw diff of two flow files shows habit counts and fold weights. [`show`]
//! and [`diff`] say instead what a flow does: which tools it may call, which
//! lookups it may make after each call, where their arguments come from, and
//! how it decides. A change a reviewer must look at, such as a tool newly
//! marked read-only or a lookup the flow could not make before, is listed as
//! one to review.

use crate::flow::{Flow, LookupBinding};
use crate::shadow::{Predicate, Sites, RESPOND};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use stretto_model::world::decode;
use stretto_model::Action;
use stretto_trace::ToolKind;

/// What the agent did next after a call to `tool` that failed or not, in
/// training: each action's share, and how many steps that is.
fn followed(flow: &Flow, tool: &str, failed: bool) -> (BTreeMap<String, f64>, f64) {
    let id = flow.vocab.id(&Action::Tool(tool.to_string()));
    let outcome = u32::from(failed);
    let (counts, total) = flow
        .habit
        .base()
        .followed(|s| matches!(decode(s), Some((a, o, _)) if a == id && o == outcome));
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
/// alone, pooled over training.
#[derive(Clone, Debug, PartialEq)]
struct Likely {
    tool: String,
    /// Its share of what the agent did next there.
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

/// Of the lookups offered after a site, the one the agent made most often
/// next in training (the first of equals, as the flow takes it), with the
/// chance of binding its arguments. It is what the habit weighs there,
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
        // A lookup training never made is bound by argument name, with a
        // chance of 1/2 (1 without arguments).
        let chance = match bound.get(&o) {
            Some(b) => b.bindable().then(|| b.chance()[2]),
            None => flow
                .manifest
                .docs
                .get(&o)
                .map(|d| if d.args.is_empty() { 1.0 } else { 0.5 }),
        };
        best = Some(Likely {
            tool: o,
            share,
            chance,
        });
    }
    best
}

/// What the flow does after a site with the habit alone, at `threshold`.
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

/// The top few actions after a site, as "a 71%, respond 20%, …".
fn top_shares(shares: &BTreeMap<String, f64>, n: usize) -> String {
    let mut v: Vec<(&String, &f64)> = shares.iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
    v.iter()
        .take(n)
        .map(|(a, p)| format!("{a} {}", percent(**p)))
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

/// One flow, as a reviewer reads it (Markdown). `threshold` is the one the
/// flow will be served with (`stretto-proxy --flow-threshold`).
pub fn show(flow: &Flow, threshold: f64) -> String {
    let mut md = String::new();
    let p = &flow.provenance;
    let _ = writeln!(md, "# Flow: {}\n", flow.domain());
    let _ = writeln!(
        md,
        "Written by stretto {} from {}. The habit learned from {} successful sessions or episodes. {}\n",
        p.stretto,
        if p.sources.is_empty() {
            "no named source".to_string()
        } else {
            p.sources.join(", ")
        },
        thousands(p.habit_episodes),
        if flow.has_arbiter() {
            format!(
                "Its arbiter was fitted on {} held-out decisions and asks `{}`.",
                thousands(p.arbiter_cases),
                flow.model
            )
        } else {
            "It has no arbiter: it decides with the habit alone and asks no one.".to_string()
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
        "What the flow does after each call, as a fugue program (`program`): `Decide` is a decision between handing back (0) and the lookups offered after the call just made, and `Outcome` whether a lookup succeeds. The proxy decides with the arbiter and takes each outcome from the server. {}\n",
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
        "After each call: the lookups the flow may make next, what the agent did next in training, and what the flow does there with the habit alone at a threshold of {threshold}, which is the likeliest lookup's share times the chance its bound arguments are the agent's. The share pools over the calls before and the code features, so a live decision near the threshold can go either way.\n"
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
        "| After | Lookups it may make next (times seen) | What the agent did next in training | With the habit alone |\n|---|---|---|---|"
    );
    let bound = bindings(flow);
    for ((tool, failed), lookups) in flow.sites.next() {
        let (shares, total) = followed(flow, tool, *failed);
        let offered: Vec<String> = lookups
            .iter()
            .map(|(l, n)| format!("`{l}` ({n})"))
            .collect();
        let _ = writeln!(
            md,
            "| {} | {} | {} (of {}) | {} |",
            site_name(tool, *failed),
            offered.join(", "),
            top_shares(&shares, 4),
            thousands(total.round() as usize),
            action(
                flow,
                tool,
                *failed,
                likely(flow, tool, *failed, &shares, &bound).as_ref(),
                threshold
            )
        );
    }
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
#[derive(Clone, Debug, Default)]
pub struct FlowDiff {
    /// The change list (Markdown).
    pub markdown: String,
    /// The changes a reviewer must look at: the flow runs another program,
    /// or may call a tool, make a lookup, bind an argument from a source, or
    /// ask a model or a question it did not before.
    pub needs_review: Vec<String>,
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

    // What the flow does after each call with the habit alone, and what the
    // agent did next in training.
    let (bound_old, bound_new) = (bindings(old), bindings(new));
    let mut habit = Vec::new();
    let mut next = Vec::new();
    for key in new.sites.next().keys() {
        let (tool, failed) = (&key.0, key.1);
        let site = site_name(tool, failed);
        let (sb, _) = followed(new, tool, failed);
        let lb = likely(new, tool, failed, &sb, &bound_new);
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
        let la = likely(old, tool, failed, &sa, &bound_old);
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
    sections.push((format!("With the habit alone, at {threshold}"), habit));
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
        (true, false) => {
            arbiter.push("no longer has an arbiter: the habit alone decides".to_string())
        }
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
    for (title, items) in sections {
        if !items.is_empty() {
            let _ = writeln!(md, "\n## {title}\n");
            for i in items {
                let _ = writeln!(md, "- {i}");
            }
        }
    }
    FlowDiff {
        markdown: md,
        needs_review: review,
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
