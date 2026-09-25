//! Counterfactual evaluation of a flow's decisions (RFC-001 §3.7): what
//! another rule would have done on the decisions a flow logged, and what
//! that would have gained.
//!
//! A decision logged with exploration (`--explore`, `--flow-explore`)
//! carries its [`PolicyView`]: every lookup the site offered, with the
//! decider's and the habit's probabilities and the binding's chance, and
//! the chance (the propensity) that the flow took what it took. A target
//! rule, the decider's or the habit's probabilities at a threshold, takes
//! an option at each decision ([`PolicyView::rule`]). Each option has an
//! outcome, a [`Label`], from whoever replayed or recorded the session: was
//! the lookup one the agent made later (used), one it never made (a detour),
//! and what share of a turn it spared.
//!
//! Four estimates of a target's totals, per site and overall:
//!
//! - **Direct:** the target's own option's label at every decision. It
//!   needs every option labelled, as a replay or a shadow session labels
//!   them, and then it is exact on the logged decisions.
//! - **IPS:** the taken option's label, weighted by one over its propensity
//!   where the target would have taken it too, and by zero elsewhere.
//! - **SNIPS:** IPS divided by the mean weight, which trades a little bias
//!   for much less variance.
//! - **Doubly robust:** a model of each option's label, fitted on the taken
//!   options' labels (the mean label of each lookup at each site), plus the
//!   IPS correction of its error.
//!
//! The last three read only the taken option's label, as a log of a flow
//! that acts has only that one. Handing back is worth nothing on every
//! measure, so they weigh only the decisions where the target looks
//! something up. The flow explores only lookups, so a target lookup that the
//! logging flow could never have taken (an option the decider gave no
//! probability) has no support, and is counted apart. Each site and the
//! total report their effective sample size (`(Σw)² / Σw²`); where it is
//! below the floor, those three are refused. The total's IPS and doubly
//! robust estimates are the sums of the sites', and its SNIPS normalizes
//! over every site's weights at once.
//!
//! A decision's context is the one the logging flow reached: after a
//! lookup the target would not have made, the target would have decided
//! somewhere else. None of these estimates sees that; the validation
//! replays measure how much it matters.

use crate::flow::{PolicyView, Proposal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;

/// One option's outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// The agent made this lookup later in the session.
    pub used: bool,
    /// The agent never made it.
    pub detour: bool,
    /// The share of an agent's turn it spared: one over the calls in the
    /// turn it pre-made, or 0.
    #[serde(default)]
    pub turn: f64,
}

/// One logged decision with its options' outcomes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    /// `lookup` or `hand_back`.
    pub action: String,
    /// The lookup's tool.
    #[serde(default)]
    pub tool: Option<String>,
    /// The site.
    #[serde(default)]
    pub site: Option<String>,
    /// Every option, and how the flow chose.
    pub policy: PolicyView,
    /// Each option's outcome, in the order of `policy.options`.
    pub labels: Vec<Label>,
    /// The episode, for reports.
    #[serde(default)]
    pub episode: String,
}

impl Record {
    /// The option the flow took, or `None` if it handed back.
    pub fn taken(&self) -> Option<usize> {
        match (&self.action[..], &self.tool) {
            ("lookup", Some(tool)) => self.policy.taken(&Proposal::Lookup {
                tool: tool.clone(),
                arguments: serde_json::Value::Null,
            }),
            _ => None,
        }
    }

    fn site(&self) -> String {
        self.site.clone().unwrap_or_default()
    }
}

/// A rule to evaluate: the logged decider's probabilities (`arbiter`) or
/// the habit's, at a threshold.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Target {
    /// Its name in the report.
    pub name: String,
    /// Decide with the habit's probabilities.
    pub habit: bool,
    /// The threshold.
    pub threshold: f64,
}

impl Target {
    /// Parse `NAME=DECIDER@THRESHOLD` or `DECIDER@THRESHOLD`, the decider
    /// `arbiter` or `habit`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (name, rule) = match text.split_once('=') {
            Some((name, rule)) => (name.to_string(), rule),
            None => (text.to_string(), text),
        };
        let (decider, threshold) = rule
            .split_once('@')
            .ok_or_else(|| format!("{text}: expected DECIDER@THRESHOLD"))?;
        let habit = match decider {
            "habit" => true,
            "arbiter" => false,
            other => return Err(format!("{text}: unknown decider {other}")),
        };
        let threshold: f64 = threshold
            .parse()
            .map_err(|_| format!("{text}: bad threshold {threshold}"))?;
        Ok(Target {
            name,
            habit,
            threshold,
        })
    }

    /// The option this target takes at `record`, or `None` to hand back.
    pub fn act(&self, record: &Record) -> Option<usize> {
        record.policy.rule(self.habit, self.threshold)
    }
}

/// The chance that the flow that logged `view` takes `option` (`None`:
/// hand back), as [`crate::flow::Flow::next_explored`] explores: the rule's
/// choice with `1 - epsilon`, and each lookup that binds and has a
/// probability, other than the rule's choice, with `epsilon` times its
/// share of them. Without such an alternative the rule's choice is certain.
pub fn propensity(view: &PolicyView, option: Option<usize>) -> f64 {
    let greedy = view
        .greedy
        .as_ref()
        .and_then(|g| view.options.iter().position(|o| o.tool == *g));
    let alternatives: Vec<(usize, f64)> = view
        .options
        .iter()
        .enumerate()
        .filter(|(i, o)| o.arguments.is_some() && o.p > 0.0 && Some(*i) != greedy)
        .map(|(i, o)| (i, o.p))
        .collect();
    let total: f64 = alternatives.iter().map(|(_, w)| w).sum();
    if alternatives.is_empty() || view.epsilon <= 0.0 {
        return if option == greedy { 1.0 } else { 0.0 };
    }
    if option == greedy {
        return 1.0 - view.epsilon;
    }
    alternatives
        .iter()
        .find(|(i, _)| Some(*i) == option)
        .map_or(0.0, |(_, w)| view.epsilon * w / total)
}

/// The measures, each a total over decisions.
pub const METRICS: [&str; 4] = ["lookups", "used", "detours", "turns"];

/// An option's value of each of [`METRICS`]; handing back is all zeros.
fn values(record: &Record, option: Option<usize>) -> [f64; 4] {
    match option {
        None => [0.0; 4],
        Some(i) => {
            let l = record.labels.get(i).copied().unwrap_or_default();
            [
                1.0,
                f64::from(u8::from(l.used)),
                f64::from(u8::from(l.detour)),
                l.turn,
            ]
        }
    }
}

/// Each estimate of each of [`METRICS`], and the logging flow's own totals.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Estimates {
    /// Decisions.
    pub decisions: usize,
    /// Decisions where the target looks something up.
    pub looks: usize,
    /// Of those, the ones where the flow took the same lookup.
    pub matched: usize,
    /// Of those, the ones where the flow could never have taken it.
    pub unsupported: usize,
    /// The effective sample size, `(Σw)² / Σw²`.
    pub ess: f64,
    /// The weighted estimates rest on too small a sample, and are left out.
    pub refused: bool,
    /// The logging flow's own totals.
    pub logged: BTreeMap<String, f64>,
    /// The direct estimate, from every option's label.
    pub direct: BTreeMap<String, f64>,
    /// Inverse propensity scoring; `None` when the sample is too small.
    pub ips: Option<BTreeMap<String, f64>>,
    /// Self-normalized IPS.
    pub snips: Option<BTreeMap<String, f64>>,
    /// Doubly robust.
    pub dr: Option<BTreeMap<String, f64>>,
    /// The standard error of the IPS and doubly robust totals, from the
    /// spread of their terms over the target's lookups.
    pub ips_se: Option<BTreeMap<String, f64>>,
    /// See `ips_se`.
    pub dr_se: Option<BTreeMap<String, f64>>,
}

/// A target's estimates, per site and overall.
#[derive(Clone, Debug, Serialize)]
pub struct Evaluation {
    /// The target.
    pub target: Target,
    /// Per site.
    pub sites: BTreeMap<String, Estimates>,
    /// Over every decision. IPS, SNIPS and DR are the sums of the sites',
    /// and refused when any site's are.
    pub total: Estimates,
}

/// The mean label of each lookup at each site, and of each lookup
/// anywhere, over the decisions where the flow took it: the doubly robust
/// estimate's model.
struct Model {
    at_site: BTreeMap<(String, String), ([f64; 4], usize)>,
    anywhere: BTreeMap<String, ([f64; 4], usize)>,
}

impl Model {
    fn fit(records: &[Record]) -> Self {
        let mut at_site: BTreeMap<(String, String), ([f64; 4], usize)> = BTreeMap::new();
        let mut anywhere: BTreeMap<String, ([f64; 4], usize)> = BTreeMap::new();
        for r in records {
            let Some(i) = r.taken() else { continue };
            let v = values(r, Some(i));
            let tool = r.policy.options[i].tool.clone();
            for (sum, n) in [
                at_site.entry((r.site(), tool.clone())).or_default(),
                anywhere.entry(tool).or_default(),
            ] {
                for (s, x) in sum.iter_mut().zip(v) {
                    *s += x;
                }
                *n += 1;
            }
        }
        Model { at_site, anywhere }
    }

    /// The predicted values of `option` at `record`.
    fn predict(&self, record: &Record, option: Option<usize>) -> [f64; 4] {
        let Some(i) = option else { return [0.0; 4] };
        let tool = &record.policy.options[i].tool;
        let mean = |(sum, n): &([f64; 4], usize)| sum.map(|s| s / *n as f64);
        self.at_site
            .get(&(record.site(), tool.clone()))
            .or_else(|| self.anywhere.get(tool))
            .map(mean)
            // A lookup the flow never took: counted as made and a detour.
            .unwrap_or([1.0, 0.0, 1.0, 0.0])
    }
}

/// Evaluate `target` on `records`, refusing the weighted estimates where
/// the effective sample size is below `min_ess`.
pub fn evaluate(records: &[Record], target: &Target, min_ess: f64) -> Evaluation {
    let model = Model::fit(records);
    let mut by_site: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
    for r in records {
        by_site.entry(r.site()).or_default().push(r);
    }
    let sites: BTreeMap<String, Sums> = by_site
        .into_iter()
        .map(|(site, rs)| (site, sums(&rs, target, &model)))
        .collect();
    let mut total = Sums::default();
    for s in sites.values() {
        total.add(s);
    }
    Evaluation {
        target: target.clone(),
        sites: sites
            .into_iter()
            .map(|(site, s)| (site, s.estimates(min_ess)))
            .collect(),
        total: total.estimates(min_ess),
    }
}

/// Running sums over a set of decisions, from which [`Estimates`] follow.
#[derive(Clone, Default)]
struct Sums {
    decisions: usize,
    looks: usize,
    matched: usize,
    unsupported: usize,
    sw: f64,
    sw2: f64,
    logged: [f64; 4],
    direct: [f64; 4],
    ips: [f64; 4],
    dr: [f64; 4],
    /// The variance of the IPS and doubly robust sums.
    ips_var: [f64; 4],
    dr_var: [f64; 4],
}

impl Sums {
    fn add(&mut self, o: &Sums) {
        self.decisions += o.decisions;
        self.looks += o.looks;
        self.matched += o.matched;
        self.unsupported += o.unsupported;
        self.sw += o.sw;
        self.sw2 += o.sw2;
        for k in 0..4 {
            self.logged[k] += o.logged[k];
            self.direct[k] += o.direct[k];
            self.ips[k] += o.ips[k];
            self.dr[k] += o.dr[k];
            self.ips_var[k] += o.ips_var[k];
            self.dr_var[k] += o.dr_var[k];
        }
    }

    fn estimates(&self, min_ess: f64) -> Estimates {
        let ess = if self.sw2 > 0.0 {
            self.sw * self.sw / self.sw2
        } else {
            0.0
        };
        let named = |v: [f64; 4]| -> BTreeMap<String, f64> {
            METRICS.iter().map(|m| m.to_string()).zip(v).collect()
        };
        // A target that never looks up is exactly zero; otherwise the
        // weighted estimates need enough weight.
        let refused = self.looks > 0 && ess < min_ess;
        // SNIPS: IPS over the mean weight of the target's lookups.
        let snips = if self.sw > 0.0 {
            self.ips.map(|x| x * self.looks as f64 / self.sw)
        } else {
            [0.0; 4]
        };
        Estimates {
            decisions: self.decisions,
            looks: self.looks,
            matched: self.matched,
            unsupported: self.unsupported,
            ess,
            refused,
            logged: named(self.logged),
            direct: named(self.direct),
            ips: (!refused).then(|| named(self.ips)),
            snips: (!refused).then(|| named(snips)),
            dr: (!refused).then(|| named(self.dr)),
            ips_se: (!refused).then(|| named(self.ips_var.map(f64::sqrt))),
            dr_se: (!refused).then(|| named(self.dr_var.map(f64::sqrt))),
        }
    }
}

/// The sums over one site's decisions.
fn sums(records: &[&Record], target: &Target, model: &Model) -> Sums {
    let mut out = Sums {
        decisions: records.len(),
        ..Sums::default()
    };
    let (mut ips2, mut dr2) = ([0.0; 4], [0.0; 4]);
    for r in records {
        let taken = r.taken();
        let seen = values(r, taken);
        for (sum, v) in out.logged.iter_mut().zip(seen) {
            *sum += v;
        }
        // Handing back is worth nothing on every measure.
        let act = target.act(r);
        if act.is_none() {
            continue;
        }
        out.looks += 1;
        if propensity(&r.policy, act) <= 0.0 {
            out.unsupported += 1;
        }
        let wanted = values(r, act);
        let w = if act == taken {
            out.matched += 1;
            1.0 / r.policy.propensity.max(1e-12)
        } else {
            0.0
        };
        out.sw += w;
        out.sw2 += w * w;
        let predicted_act = model.predict(r, act);
        let predicted_taken = model.predict(r, taken);
        for k in 0..4 {
            let (i, d) = (
                w * seen[k],
                predicted_act[k] + w * (seen[k] - predicted_taken[k]),
            );
            out.direct[k] += wanted[k];
            out.ips[k] += i;
            ips2[k] += i * i;
            out.dr[k] += d;
            dr2[k] += d * d;
        }
    }
    // The variance of a sum of n independent terms: n times theirs.
    let n = out.looks as f64;
    if n > 1.0 {
        for k in 0..4 {
            let var = |sum: f64, sq: f64| (sq - sum * sum / n).max(0.0) * n / (n - 1.0);
            out.ips_var[k] = var(out.ips[k], ips2[k]);
            out.dr_var[k] = var(out.dr[k], dr2[k]);
        }
    }
    out
}

/// Evaluations as Markdown: each target's totals, then its sites.
pub fn markdown(evaluations: &[Evaluation], min_ess: f64) -> String {
    let mut md = String::new();
    let num = |v: Option<f64>| v.map_or("refused".to_string(), |v| format!("{v:.1}"));
    for e in evaluations {
        let t = &e.target;
        let _ = writeln!(
            md,
            "## {} ({} at {})\n",
            t.name,
            if t.habit {
                "the habit"
            } else {
                "the logged decider"
            },
            t.threshold
        );
        let tot = &e.total;
        let _ = writeln!(
            md,
            "{} decisions. The target looks something up at {}, and at {} of those the flow took the same lookup (effective sample size {:.1}); at {} the flow could never have taken it.\n",
            tot.decisions, tot.looks, tot.matched, tot.ess, tot.unsupported
        );
        let _ = writeln!(
            md,
            "| Measure | Logged flow | Direct | IPS | SNIPS | Doubly robust |\n|---|---|---|---|---|---|"
        );
        for m in METRICS {
            let get = |x: &Option<BTreeMap<String, f64>>| num(x.as_ref().map(|x| x[m]));
            let pm = |x: &Option<BTreeMap<String, f64>>, se: &Option<BTreeMap<String, f64>>| match (
                x, se,
            ) {
                (Some(x), Some(se)) => format!("{:.1} ± {:.1}", x[m], se[m]),
                _ => "refused".to_string(),
            };
            let _ = writeln!(
                md,
                "| {m} | {:.1} | {:.1} | {} | {} | {} |",
                tot.logged[m],
                tot.direct[m],
                pm(&tot.ips, &tot.ips_se),
                get(&tot.snips),
                pm(&tot.dr, &tot.dr_se)
            );
        }
        let _ = writeln!(
            md,
            "\n| Site | Decisions | Matched | ESS | Used: direct | Used: SNIPS | Detours: direct | Detours: SNIPS |\n|---|---|---|---|---|---|---|---|"
        );
        for (site, s) in &e.sites {
            let get = |x: &Option<BTreeMap<String, f64>>, m: &str| num(x.as_ref().map(|x| x[m]));
            let _ = writeln!(
                md,
                "| `{site}` | {} | {} | {:.1} | {:.1} | {} | {:.1} | {} |",
                s.decisions,
                s.matched,
                s.ess,
                s.direct["used"],
                get(&s.snips, "used"),
                s.direct["detours"],
                get(&s.snips, "detours")
            );
        }
        let _ = writeln!(
            md,
            "\nIPS and doubly robust totals show one standard error. Weighted estimates, a site's or the total's, are refused below an effective sample size of {min_ess}.\n"
        );
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::OptionView;

    /// A small deterministic generator for the synthetic logs.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn option(tool: &str, p: f64, habit: f64, binding: Option<f64>) -> OptionView {
        OptionView {
            tool: tool.to_string(),
            p,
            habit,
            binding,
            arguments: binding.map(|_| serde_json::json!({})),
            unbound: None,
        }
    }

    fn view(options: Vec<OptionView>) -> PolicyView {
        PolicyView {
            decider: "arbiter".to_string(),
            threshold: 0.3,
            epsilon: 0.0,
            explored: false,
            greedy: None,
            propensity: 1.0,
            task_id: "t".to_string(),
            events: 0,
            options,
        }
    }

    #[test]
    fn the_rule_is_the_flows() {
        // The likeliest lookup, the first of equals.
        let v = view(vec![
            option("a", 0.4, 0.1, Some(1.0)),
            option("b", 0.4, 0.6, Some(1.0)),
        ]);
        assert_eq!(v.rule(false, 0.3), Some(0));
        assert_eq!(v.rule(true, 0.3), Some(1));
        assert_eq!(v.rule(false, 0.5), None);
        // Its probability times its binding's chance must reach the threshold.
        let v = view(vec![option("a", 0.6, 0.6, Some(0.4))]);
        assert_eq!(v.rule(false, 0.3), None);
        assert_eq!(v.rule(false, 0.2), Some(0));
        // A likeliest lookup that does not bind hands back, whatever comes second.
        let v = view(vec![
            option("a", 0.7, 0.7, None),
            option("b", 0.6, 0.6, Some(1.0)),
        ]);
        assert_eq!(v.rule(false, 0.3), None);
    }

    #[test]
    fn targets_parse() {
        let t = Target::parse("C=habit@0.9").unwrap();
        assert_eq!((t.name.as_str(), t.habit, t.threshold), ("C", true, 0.9));
        let t = Target::parse("arbiter@0.3").unwrap();
        assert_eq!((t.name.as_str(), t.habit), ("arbiter@0.3", false));
        assert!(Target::parse("oracle@0.3").is_err());
        assert!(Target::parse("habit").is_err());
    }

    /// Synthetic logs whose truth is known: three lookups per decision, with
    /// random probabilities and bindings, labels drawn from the arbiter's
    /// probabilities, and a flow that explores at `epsilon` as the flow
    /// does ([`crate::flow::Flow::next_explored`]).
    fn synthetic(n: usize, epsilon: f64, seed: u64) -> Vec<Record> {
        let mut g = Lcg(seed);
        let tools = ["get_a", "get_b", "get_c"];
        (0..n)
            .map(|i| {
                let site = format!("site{}", i % 3);
                let raw: Vec<f64> = (0..4).map(|_| g.next()).collect();
                let sum: f64 = raw.iter().sum();
                let options: Vec<OptionView> = tools
                    .iter()
                    .enumerate()
                    .map(|(k, t)| {
                        let p = raw[k] / sum;
                        let habit = (p + 0.3 * (g.next() - 0.5)).clamp(0.0, 1.0);
                        let binding = (g.next() > 0.1).then(|| 0.5 + 0.5 * g.next());
                        option(t, p, habit, binding)
                    })
                    .collect();
                let labels: Vec<Label> = options
                    .iter()
                    .map(|o| {
                        let used = g.next() < 0.3 + 0.6 * o.p;
                        Label {
                            used,
                            detour: !used && g.next() < 0.8,
                            turn: if used { 0.5 + 0.5 * g.next() } else { 0.0 },
                        }
                    })
                    .collect();
                let mut policy = view(options);
                let greedy = policy.rule(false, 0.3);
                let alternatives: Vec<(usize, f64)> = policy
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(k, o)| o.binding.is_some() && o.p > 0.0 && Some(*k) != greedy)
                    .map(|(k, o)| (k, o.p))
                    .collect();
                let total: f64 = alternatives.iter().map(|(_, w)| w).sum();
                let (taken, propensity) = if alternatives.is_empty() {
                    (greedy, 1.0)
                } else if g.next() < epsilon {
                    let mut u = g.next() * total;
                    let &(k, w) = alternatives
                        .iter()
                        .find(|(_, w)| {
                            u -= w;
                            u < 0.0
                        })
                        .unwrap_or(alternatives.last().unwrap());
                    (Some(k), epsilon * w / total)
                } else {
                    (greedy, 1.0 - epsilon)
                };
                policy.epsilon = epsilon;
                policy.propensity = propensity;
                policy.greedy = greedy.map(|k| policy.options[k].tool.clone());
                assert!((super::propensity(&policy, taken) - propensity).abs() < 1e-12);
                Record {
                    action: if taken.is_some() {
                        "lookup"
                    } else {
                        "hand_back"
                    }
                    .to_string(),
                    tool: taken.map(|k| policy.options[k].tool.clone()),
                    site: Some(site),
                    policy,
                    labels,
                    episode: String::new(),
                }
            })
            .collect()
    }

    #[test]
    fn weighted_estimates_find_the_truth_on_synthetic_logs() {
        let records = synthetic(40_000, 0.3, 7);
        for target in [
            Target::parse("habit@0.3").unwrap(),
            Target::parse("arbiter@0.5").unwrap(),
            Target::parse("arbiter@0.3").unwrap(),
        ] {
            let e = evaluate(&records, &target, 10.0);
            let truth = &e.total.direct;
            // Each estimate's standard error, from the IPS terms' second
            // moment: within four of them of the truth.
            let mut second = [0.0; 4];
            for r in &records {
                let act = target.act(r);
                if act.is_some() && act == r.taken() {
                    let w = 1.0 / r.policy.propensity;
                    for (s, v) in second.iter_mut().zip(values(r, act)) {
                        *s += (w * v) * (w * v);
                    }
                }
            }
            for est in [&e.total.ips, &e.total.snips, &e.total.dr] {
                let est = est.as_ref().expect("enough data");
                for (k, m) in METRICS.iter().enumerate() {
                    let (t, x) = (truth[*m], est[*m]);
                    let se = second[k].sqrt();
                    assert!(
                        (x - t).abs() <= 4.0 * se + 1e-9,
                        "{} {m}: estimated {x:.1}, true {t:.1}, standard error {se:.1}",
                        target.name
                    );
                }
            }
        }
        // The logging rule itself: its own totals are the direct ones.
        let e = evaluate(&records, &Target::parse("arbiter@0.3").unwrap(), 10.0);
        assert!(e.total.matched > 0);
    }

    #[test]
    fn a_rule_the_log_never_explores_is_refused() {
        // No exploration: where the target differs, nothing matches.
        let records = synthetic(3_000, 0.0, 11);
        let e = evaluate(&records, &Target::parse("arbiter@0.9").unwrap(), 10.0);
        // The direct estimate still stands: every option is labelled.
        assert!(e.total.direct["lookups"] < e.total.logged["lookups"]);
        // The logging rule itself is estimated exactly by all four.
        let same = evaluate(&records, &Target::parse("arbiter@0.3").unwrap(), 10.0);
        let ips = same.total.ips.as_ref().unwrap();
        for m in METRICS {
            assert!((ips[m] - same.total.logged[m]).abs() < 1e-6, "{m}");
            assert!(
                (same.total.direct[m] - same.total.logged[m]).abs() < 1e-6,
                "{m}"
            );
        }
        // A site whose weighted sample is too small is refused, and so is the total.
        let tiny = evaluate(&records[..6], &Target::parse("habit@0.3").unwrap(), 10.0);
        assert!(tiny.total.ips.is_none() && tiny.total.snips.is_none() && tiny.total.dr.is_none());
        assert!(tiny.sites.values().all(|s| s.ess < 10.0));
    }

    #[test]
    fn a_perfect_model_makes_the_doubly_robust_estimate_exact() {
        // Every decision alike: one lookup, always used, taken at random.
        let records: Vec<Record> = (0..200)
            .map(|i| {
                let mut policy = view(vec![option("get_a", 0.6, 0.6, Some(1.0))]);
                policy.propensity = 0.5;
                Record {
                    action: if i % 2 == 0 { "lookup" } else { "hand_back" }.to_string(),
                    tool: (i % 2 == 0).then(|| "get_a".to_string()),
                    site: Some("s".to_string()),
                    policy,
                    labels: vec![Label {
                        used: true,
                        detour: false,
                        turn: 1.0,
                    }],
                    episode: String::new(),
                }
            })
            .collect();
        let e = evaluate(&records, &Target::parse("arbiter@0.3").unwrap(), 10.0);
        let dr = e.total.dr.as_ref().unwrap();
        assert!((dr["used"] - 200.0).abs() < 1e-9);
        assert!((e.total.direct["used"] - 200.0).abs() < 1e-9);
        assert!((e.total.ips.as_ref().unwrap()["used"] - 200.0).abs() < 1e-9);
    }
}
