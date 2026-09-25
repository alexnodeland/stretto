//! Predicate refinement (RFC-001 §3.4): find the sites where the arbiter's
//! state abstraction is weakest, show a proposer examples from them, and
//! keep a proposed predicate only when it explains the agents' steps better.
//!
//! The input is the decision log `phase0 --oracle-log` writes for one domain
//! with `--questions v2`. Each logged next-step decision carries the options,
//! the features the arbiter weighed, the agent's step and every predicate's
//! answer, including candidates asked alone (`phase0 --candidates`).
//! [`refine`] rebuilds each decision's case with any set of candidates as
//! extra features, fits the arbiter by cross-validation over tasks as Phase
//! 0b does, and scores the held-out log-likelihood of the agent's steps.
//! Candidates are added greedily, and one is kept only when it raises that
//! likelihood by more than a penalty per predicate, as model selection
//! charges a parameter.

use crate::arbitrate::{self, Case};
use crate::phase0::task_group;
use crate::shadow::{Favors, Predicate};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

/// One logged next-step decision the arbiter judged.
#[derive(Clone, Debug)]
pub struct Logged {
    /// The episode's id.
    pub episode: String,
    /// Its task, whose group sets the fold.
    pub task: String,
    /// The agent model.
    pub model: String,
    /// The step the decision came before.
    pub step: usize,
    /// Whether the episode is a transfer target (judged, never fitted on).
    pub target: bool,
    /// The site: the tool that just returned, with ` (error)` if it failed.
    pub site: String,
    /// The options, in the case's order.
    pub options: Vec<String>,
    /// The features the arbiter weighed for each option.
    pub features: Vec<Vec<f64>>,
    /// The option the System-One model picked.
    pub pick: usize,
    /// The agent's step, as a read-only flow takes it.
    pub actual: String,
    /// Every predicate's answer, candidates included: P(yes) by id.
    pub predicates: BTreeMap<String, f64>,
    /// The request's cache key, to find its state in a dump.
    pub key: Option<String>,
}

/// The arbiter's asked next-step decisions in a `phase0 --oracle-log` log.
pub fn read_log(text: &str) -> Result<Vec<Logged>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: Value = serde_json::from_str(line).with_context(|| format!("log line {}", n + 1))?;
        let Some(case) = e.get("case").filter(|c| !c.is_null()) else {
            continue;
        };
        if e.get("kind").and_then(Value::as_str) != Some("Next") {
            continue;
        }
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        out.push(Logged {
            episode: text(&e["episode"]),
            task: text(&e["task"]),
            model: text(&e["model"]),
            step: e["step"].as_u64().unwrap_or(0) as usize,
            target: e["target"].as_bool().unwrap_or(false),
            site: text(&case["site"]),
            options: serde_json::from_value(case["options"].clone())
                .with_context(|| format!("log line {}: options", n + 1))?,
            features: serde_json::from_value(case["features"].clone())
                .with_context(|| format!("log line {}: features", n + 1))?,
            pick: case["pick"].as_u64().unwrap_or(0) as usize,
            actual: text(&e["actual"]),
            predicates: serde_json::from_value(e["predicates"].clone()).unwrap_or_default(),
            key: e["key"].as_str().map(String::from),
        });
    }
    Ok(out)
}

/// Candidate `p`'s feature on option `a` of `d`, as Phase 0b builds a
/// predicate's: the logit of its answer on the options it bears on, zero
/// elsewhere and where it was not answered.
pub fn candidate_feature(d: &Logged, p: &Predicate, a: usize) -> f64 {
    let prev = d.site.strip_suffix(" (error)").unwrap_or(&d.site);
    match (p.favors.on(&d.options[a], prev), d.predicates.get(&p.id)) {
        (true, Some(&v)) => {
            let v = v.clamp(1e-4, 1.0 - 1e-4);
            (v / (1.0 - v)).ln()
        }
        _ => 0.0,
    }
}

/// The arbiter's cases for `log`, with `extra` candidates weighed after the
/// logged features.
pub fn cases(log: &[Logged], extra: &[&Predicate]) -> Vec<Case> {
    log.iter()
        .map(|d| Case {
            group: task_group(&d.task),
            site: d.site.clone(),
            features: d
                .features
                .iter()
                .enumerate()
                .map(|(a, x)| {
                    let mut x = x.clone();
                    x.extend(extra.iter().map(|p| candidate_feature(d, p, a)));
                    x
                })
                .collect(),
            pick: d.pick,
            actual: d.options.iter().position(|o| *o == d.actual),
            fit: !d.target,
        })
        .collect()
}

/// How well the arbiter, cross-fitted over task folds, explains the agents'
/// steps at the fittable decisions.
#[derive(Clone, Debug, Serialize)]
pub struct Fit {
    /// Held-out log-likelihood of the agent's steps (nats), at the
    /// decisions whose step was among the options: the conditional logit's,
    /// without the site's coverage, which no feature changes.
    pub loglik: f64,
    /// The decisions it is summed over.
    pub scored: usize,
    /// Fittable decisions.
    pub decisions: usize,
    /// Of those, where the arbiter's top option was the agent's step.
    pub agreed: usize,
    /// The weights, averaged over folds: the logged features, the extra
    /// candidates, then the site's reliability.
    pub weights: Vec<f64>,
    /// Per site: decisions, held-out log-likelihood, agreed.
    pub sites: BTreeMap<String, (usize, f64, usize)>,
    /// The same at transfer targets' decisions, which are judged but never
    /// fitted on (nor shown to a proposer): log-likelihood, scored,
    /// decisions and agreed.
    pub target: (f64, usize, usize, usize),
    /// Each case's held-out probability of the agent's step (`None` where
    /// it was not among the options or the case is not fittable), and the
    /// arbiter's top option.
    #[serde(skip)]
    pub judged: Vec<(Option<f64>, usize)>,
}

impl Fit {
    /// Agreement, as a share.
    pub fn agreement(&self) -> f64 {
        self.agreed as f64 / self.decisions.max(1) as f64
    }

    /// Mean surprise per scored decision (nats).
    pub fn surprise(&self) -> f64 {
        -self.loglik / self.scored.max(1) as f64
    }

    /// Agreement at the targets' decisions.
    pub fn target_agreement(&self) -> f64 {
        self.target.3 as f64 / self.target.2.max(1) as f64
    }

    /// Mean surprise at the targets' scored decisions (nats).
    pub fn target_surprise(&self) -> f64 {
        -self.target.0 / self.target.1.max(1) as f64
    }
}

/// Fit the arbiter on `cases` by cross-validation over `folds` task folds,
/// as Phase 0b does, and score it.
pub fn score(cases: &[Case], folds: u64) -> Fit {
    let (arbitrated, weights, _) = arbitrate::cross_fit_folds(cases, folds);
    let mut fit = Fit {
        loglik: 0.0,
        scored: 0,
        decisions: 0,
        agreed: 0,
        weights,
        sites: BTreeMap::new(),
        target: (0.0, 0, 0, 0),
        judged: Vec::with_capacity(cases.len()),
    };
    let prob = |c: &Case, a: &arbitrate::Arbitrated| {
        c.actual.map(|y| {
            let total: f64 = a.probs.iter().sum();
            (a.probs[y] / total.max(1e-300)).max(1e-12)
        })
    };
    for (c, a) in cases.iter().zip(&arbitrated) {
        let agreed = Some(a.top) == c.actual;
        if !c.fit {
            fit.target.2 += 1;
            fit.target.3 += agreed as usize;
            if let Some(p) = prob(c, a) {
                fit.target.0 += p.ln();
                fit.target.1 += 1;
            }
            fit.judged.push((None, a.top));
            continue;
        }
        fit.decisions += 1;
        fit.agreed += agreed as usize;
        let site = fit.sites.entry(c.site.clone()).or_default();
        site.0 += 1;
        site.2 += agreed as usize;
        let p = prob(c, a);
        if let Some(p) = p {
            fit.loglik += p.ln();
            fit.scored += 1;
            site.1 += p.ln();
        }
        fit.judged.push((p, a.top));
    }
    fit
}

/// One candidate, on its own and in the search.
#[derive(Clone, Debug, Serialize)]
pub struct CandidateRow {
    /// The candidate's id.
    pub id: String,
    /// The options its answer bears on.
    pub favors: Favors,
    /// The question.
    pub question: String,
    /// Fittable decisions where it was answered.
    pub answered: usize,
    /// Its mean probability of "yes" there.
    pub mean_yes: f64,
    /// Held-out log-likelihood gained over the logged features, with it
    /// alone added.
    pub gain_alone: f64,
    /// Its weight then, averaged over folds.
    pub weight_alone: f64,
    /// Agreement then.
    pub agreement_alone: f64,
    /// Log-likelihood gained at the targets' decisions, with it alone
    /// added.
    pub target_gain_alone: f64,
    /// Whether the search kept it.
    pub kept: bool,
}

/// One round of the greedy search: the best candidate left, and what it
/// gained over the set so far.
#[derive(Clone, Debug, Serialize)]
pub struct Round {
    /// The candidate.
    pub id: String,
    /// Held-out log-likelihood with it added.
    pub loglik: f64,
    /// Its gain over the set so far.
    pub gain: f64,
    /// Whether the gain cleared the penalty.
    pub kept: bool,
    /// Its gain at the targets' decisions.
    pub target_gain: f64,
}

/// One site, before and after refinement.
#[derive(Clone, Debug, Serialize)]
pub struct SiteRow {
    /// The site.
    pub site: String,
    /// Fittable decisions there.
    pub decisions: usize,
    /// Held-out log-likelihood with the logged features.
    pub base_loglik: f64,
    /// With the kept candidates.
    pub refined_loglik: f64,
    /// Agreement with the logged features.
    pub base_agreement: f64,
    /// With the kept candidates.
    pub refined_agreement: f64,
}

/// What [`refine`] found.
#[derive(Clone, Debug, Serialize)]
pub struct Refined {
    /// Task folds.
    pub folds: u64,
    /// Nats a candidate must gain to be kept.
    pub penalty: f64,
    /// The arbiter with the logged features.
    pub base: Fit,
    /// Every candidate.
    pub candidates: Vec<CandidateRow>,
    /// The greedy search, round by round; the last round's candidate was
    /// not kept unless every candidate was.
    pub rounds: Vec<Round>,
    /// The kept candidates, in the order they were added.
    pub kept: Vec<String>,
    /// The arbiter with them.
    pub refined: Fit,
    /// Per site, by decisions.
    pub sites: Vec<SiteRow>,
}

/// Half the log of the scored decisions: what BIC charges a parameter.
pub fn default_penalty(scored: usize) -> f64 {
    0.5 * (scored.max(1) as f64).ln()
}

/// Search `candidates` greedily for the set that best explains the agents'
/// steps in `log`, keeping each only when it gains more than `penalty`
/// nats of held-out log-likelihood (default: [`default_penalty`]).
pub fn refine(
    log: &[Logged],
    candidates: &[Predicate],
    folds: u64,
    penalty: Option<f64>,
) -> Refined {
    let base = score(&cases(log, &[]), folds);
    let penalty = penalty.unwrap_or_else(|| default_penalty(base.scored));
    let width = log
        .first()
        .map_or(0, |d| d.features.first().map_or(0, Vec::len));
    let mut rows: Vec<CandidateRow> = candidates
        .iter()
        .map(|p| {
            let fit = score(&cases(log, &[p]), folds);
            let answered: Vec<f64> = log
                .iter()
                .filter(|d| !d.target)
                .filter_map(|d| d.predicates.get(&p.id).copied())
                .collect();
            CandidateRow {
                id: p.id.clone(),
                favors: p.favors.clone(),
                question: p.question.clone(),
                answered: answered.len(),
                mean_yes: answered.iter().sum::<f64>() / answered.len().max(1) as f64,
                gain_alone: fit.loglik - base.loglik,
                weight_alone: fit.weights.get(width).copied().unwrap_or(0.0),
                agreement_alone: fit.agreement(),
                target_gain_alone: fit.target.0 - base.target.0,
                kept: false,
            }
        })
        .collect();
    let mut chosen: Vec<usize> = Vec::new();
    let mut current = base.clone();
    let mut rounds = Vec::new();
    while chosen.len() < candidates.len() {
        let best = (0..candidates.len())
            .filter(|j| !chosen.contains(j))
            .map(|j| {
                let extra: Vec<&Predicate> = chosen
                    .iter()
                    .chain(std::iter::once(&j))
                    .map(|&k| &candidates[k])
                    .collect();
                (j, score(&cases(log, &extra), folds))
            })
            .max_by(|a, b| a.1.loglik.total_cmp(&b.1.loglik));
        let Some((j, fit)) = best else { break };
        let gain = fit.loglik - current.loglik;
        let kept = gain > penalty;
        rounds.push(Round {
            id: candidates[j].id.clone(),
            loglik: fit.loglik,
            gain,
            kept,
            target_gain: fit.target.0 - current.target.0,
        });
        if !kept {
            break;
        }
        chosen.push(j);
        rows[j].kept = true;
        current = fit;
    }
    let sites = base
        .sites
        .iter()
        .map(|(site, &(n, ll, agreed))| {
            let (_, rll, ragreed) = current.sites.get(site).copied().unwrap_or_default();
            SiteRow {
                site: site.clone(),
                decisions: n,
                base_loglik: ll,
                refined_loglik: rll,
                base_agreement: agreed as f64 / n.max(1) as f64,
                refined_agreement: ragreed as f64 / n.max(1) as f64,
            }
        })
        .collect::<Vec<_>>();
    let mut sites = sites;
    sites.sort_by(|a, b| b.decisions.cmp(&a.decisions).then(a.site.cmp(&b.site)));
    Refined {
        folds,
        penalty,
        base,
        candidates: rows,
        rounds,
        kept: chosen.iter().map(|&j| candidates[j].id.clone()).collect(),
        refined: current,
        sites,
    }
}

/// The report of a [`refine`] run, in Markdown.
pub fn markdown(r: &Refined, domain: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Predicate refinement: {domain}\n");
    let _ = writeln!(
        s,
        "{} fittable next-step decisions, {} of them with the agent's step among the options. \
         The arbiter is fitted by cross-validation over {} task folds, as in Phase 0b, and \
         scored on the held-out log-likelihood of the agent's steps. A candidate is kept when \
         it raises that by more than {:.2} nats.\n",
        r.base.decisions, r.base.scored, r.folds, r.penalty
    );
    let targets = r.base.target.2 > 0;
    if targets {
        let _ = writeln!(
            s,
            "The transfer targets' {} decisions ({} scored) are judged by the same arbiters \
             but never fitted on, nor shown to the proposer: an out-of-sample check.\n",
            r.base.target.2, r.base.target.1
        );
        let _ = writeln!(
            s,
            "| | Held-out log-likelihood | Nats per decision | Agreement | Targets: log-likelihood | Nats per decision | Agreement |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|");
    } else {
        let _ = writeln!(
            s,
            "| | Held-out log-likelihood | Nats per decision | Agreement |"
        );
        let _ = writeln!(s, "|---|---|---|---|");
    }
    for (name, f) in [
        ("The logged predicates", &r.base),
        ("With the kept candidates", &r.refined),
    ] {
        let _ = write!(
            s,
            "| {name} | {:.1} | {:.4} | {:.1}% |",
            f.loglik,
            f.surprise(),
            100.0 * f.agreement()
        );
        if targets {
            let _ = write!(
                s,
                " {:.1} | {:.4} | {:.1}% |",
                f.target.0,
                f.target_surprise(),
                100.0 * f.target_agreement()
            );
        }
        let _ = writeln!(s);
    }
    let _ = writeln!(
        s,
        "\nKept: {}.\n",
        if r.kept.is_empty() {
            "none".to_string()
        } else {
            r.kept
                .iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    if !r.candidates.is_empty() {
        let _ = writeln!(s, "## Candidates\n");
        let _ = writeln!(
            s,
            "Each alone, added to the logged features: its gain in held-out log-likelihood, its weight and the agreement.\n"
        );
        let _ = writeln!(
            s,
            "| Candidate | Bears on | Answered | Mean P(yes) | Gain alone (nats) | Weight | Agreement | Targets' gain | Kept |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|");
        for c in &r.candidates {
            let _ = writeln!(
                s,
                "| `{}` | {} | {} | {:.2} | {:+.1} | {:.2} | {:.1}% | {} | {} |",
                c.id,
                favors_name(&c.favors),
                c.answered,
                c.mean_yes,
                c.gain_alone,
                c.weight_alone,
                100.0 * c.agreement_alone,
                if targets {
                    format!("{:+.1}", c.target_gain_alone)
                } else {
                    String::new()
                },
                if c.kept { "yes" } else { "no" }
            );
        }
        let _ = writeln!(s, "\n## The search\n");
        let _ = writeln!(
            s,
            "| Round | Best candidate left | Log-likelihood | Gain | Targets' gain | Kept |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|");
        for (i, round) in r.rounds.iter().enumerate() {
            let _ = writeln!(
                s,
                "| {} | `{}` | {:.1} | {:+.1} | {} | {} |",
                i + 1,
                round.id,
                round.loglik,
                round.gain,
                if targets {
                    format!("{:+.1}", round.target_gain)
                } else {
                    String::new()
                },
                if round.kept { "yes" } else { "no" }
            );
        }
        let _ = writeln!(s);
    }
    let _ = writeln!(s, "## Sites\n");
    let _ = writeln!(
        s,
        "| Site | Decisions | Nats per decision | Refined | Agreement | Refined |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|");
    for site in &r.sites {
        let n = site.decisions.max(1) as f64;
        let _ = writeln!(
            s,
            "| `{}` | {} | {:.3} | {:.3} | {:.1}% | {:.1}% |",
            site.site,
            site.decisions,
            -site.base_loglik / n,
            -site.refined_loglik / n,
            100.0 * site.base_agreement,
            100.0 * site.refined_agreement
        );
    }
    s
}

fn favors_name(f: &Favors) -> String {
    match f {
        Favors::SameLookup => "the same lookup again".to_string(),
        Favors::AnyLookup => "any lookup".to_string(),
        Favors::HandBack => "handing back".to_string(),
        Favors::Lookup(tool) => format!("`{tool}`"),
    }
}

/// Examples for a proposer from the `sites` sites where the arbiter fitted
/// on the logged features is most surprised in total: at each, up to
/// `per_site` decisions where it put the least probability on the agent's
/// step, one per task, each with its state from `dump` (the requests
/// `phase0 --oracle-dump` wrote, by cache key).
pub fn examples(
    log: &[Logged],
    dump: &HashMap<String, Value>,
    folds: u64,
    sites: usize,
    per_site: usize,
) -> String {
    let fit = score(&cases(log, &[]), folds);
    let mut ranked: Vec<(&String, &(usize, f64, usize))> = fit.sites.iter().collect();
    ranked.sort_by(|a, b| a.1 .1.total_cmp(&b.1 .1));
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Where the arbiter is weakest\n\nThe {} sites where the arbiter, fitted on the logged \
         features by cross-validation over tasks, is most surprised by the agents' steps in \
         total. At each, the decisions where it put the least probability on the agent's step, \
         one per task. A predicate that separates these from the decisions it gets right is a \
         candidate.\n",
        sites.min(ranked.len())
    );
    for (site, &(n, ll, agreed)) in ranked.into_iter().take(sites) {
        let _ = writeln!(
            s,
            "## `{site}`: {n} decisions, {:.1}% agreed, {:.3} nats per decision\n",
            100.0 * agreed as f64 / n.max(1) as f64,
            -ll / n.max(1) as f64
        );
        let mut at: Vec<(usize, f64)> = log
            .iter()
            .enumerate()
            .filter(|(_, d)| d.site == *site)
            .filter_map(|(i, _)| fit.judged[i].0.map(|p| (i, p)))
            .collect();
        at.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut tasks = BTreeSet::new();
        for (i, p) in at {
            if tasks.len() >= per_site {
                break;
            }
            let d = &log[i];
            if !tasks.insert(d.task.clone()) {
                continue;
            }
            let top = &d.options[fit.judged[i].1];
            let _ = writeln!(
                s,
                "### Task {}, {}, step {}\n\nThe agent's step: `{}` (the arbiter gave it {:.2}; its top option: `{top}`). Predicates: {}.\n",
                d.task,
                d.model,
                d.step,
                d.actual,
                p,
                d.predicates
                    .iter()
                    .map(|(k, v)| format!("{k} {v:.2}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Some(state) = d.key.as_ref().and_then(|k| dump.get(k)) {
                let text = serde_json::to_string_pretty(state).unwrap_or_default();
                let clipped: String = text.chars().take(6000).collect();
                let _ = writeln!(s, "```json\n{clipped}\n```\n");
            }
        }
    }
    s
}

/// A `phase0 --oracle-dump` file's states, by cache key.
pub fn read_dump(text: &str) -> Result<HashMap<String, Value>> {
    let mut out = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: Value =
            serde_json::from_str(line).with_context(|| format!("dump line {}", n + 1))?;
        if let (Some(key), Some(state)) = (e["key"].as_str(), e["request"].get("state")) {
            out.insert(key.to_string(), state.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadow::RESPOND;

    fn predicate(id: &str, favors: Favors) -> Predicate {
        Predicate {
            id: id.to_string(),
            favors,
            question: format!("{id}?"),
            yes: "yes".to_string(),
            no: "no".to_string(),
        }
    }

    /// Decisions at one site with a lookup and handing back, where the
    /// agent looks up exactly when `signal` says yes, and `noise` says
    /// nothing. The logged features know only the option (a constant).
    fn synthetic(n: usize) -> Vec<Logged> {
        let mut state = 7u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        (0..n)
            .map(|i| {
                let looks = next() < 0.5;
                let signal = if looks { 0.8 } else { 0.2 };
                let noise = next();
                Logged {
                    episode: format!("e{i}"),
                    task: format!("{}", i % 40),
                    model: "m".to_string(),
                    step: 1,
                    target: false,
                    site: "get_order".to_string(),
                    options: vec!["get_order".to_string(), RESPOND.to_string()],
                    features: vec![vec![0.0, 0.0], vec![0.0, 1.0]],
                    pick: 0,
                    actual: if looks { "get_order" } else { RESPOND }.to_string(),
                    predicates: BTreeMap::from([
                        ("signal".to_string(), signal),
                        ("noise".to_string(), noise),
                    ]),
                    key: None,
                }
            })
            .collect()
    }

    #[test]
    fn a_candidate_that_explains_the_steps_is_kept_and_noise_is_not() {
        let log = synthetic(400);
        let candidates = vec![
            predicate("noise", Favors::AnyLookup),
            predicate("signal", Favors::SameLookup),
        ];
        let r = refine(&log, &candidates, 5, None);
        assert_eq!(r.kept, vec!["signal".to_string()]);
        let signal = r.candidates.iter().find(|c| c.id == "signal").unwrap();
        assert!(signal.gain_alone > 100.0, "{}", signal.gain_alone);
        assert!(signal.weight_alone > 0.5, "{}", signal.weight_alone);
        let noise = r.candidates.iter().find(|c| c.id == "noise").unwrap();
        assert!(noise.gain_alone < r.penalty, "{}", noise.gain_alone);
        assert!(r.refined.agreement() > 0.95, "{}", r.refined.agreement());
        assert!((r.base.agreement() - 0.5).abs() < 0.1);
        // The second round tried noise and stopped.
        assert_eq!(r.rounds.len(), 2);
        assert!(!r.rounds[1].kept);
    }

    #[test]
    fn targets_are_judged_but_never_fitted_on() {
        let mut log = synthetic(400);
        for (i, d) in log.iter_mut().enumerate() {
            d.target = i % 5 == 0;
        }
        let signal = predicate("signal", Favors::SameLookup);
        let r = refine(&log, std::slice::from_ref(&signal), 5, None);
        assert_eq!(r.base.decisions, 320);
        assert_eq!(r.base.target.2, 80);
        assert_eq!(r.kept, vec!["signal".to_string()]);
        // Fitted on the others, the candidate explains the targets too.
        assert!(r.candidates[0].target_gain_alone > 20.0);
        assert!(r.refined.target_agreement() > 0.9);
        // Flipping the targets' labels changes nothing the arbiter was
        // fitted on: the source decisions' fit is the same.
        let mut flipped = log.clone();
        for d in flipped.iter_mut().filter(|d| d.target) {
            d.actual = if d.actual == RESPOND {
                "get_order"
            } else {
                RESPOND
            }
            .to_string();
        }
        let f = refine(&flipped, &[signal], 5, None);
        assert!((f.refined.loglik - r.refined.loglik).abs() < 1e-9);
        assert!(f.refined.target_agreement() < 0.1);
    }

    #[test]
    fn candidate_features_follow_what_they_bear_on() {
        let mut d = synthetic(1).remove(0);
        d.site = "get_order (error)".to_string();
        d.predicates.insert("p".to_string(), 0.5);
        let logit = |v: f64| (v / (1.0 - v)).ln();
        let s = d.predicates["signal"];
        // The same lookup again: the site's tool, even after an error.
        let same = predicate("signal", Favors::SameLookup);
        assert!((candidate_feature(&d, &same, 0) - logit(s)).abs() < 1e-9);
        assert_eq!(candidate_feature(&d, &same, 1), 0.0);
        let back = predicate("signal", Favors::HandBack);
        assert_eq!(candidate_feature(&d, &back, 0), 0.0);
        assert!((candidate_feature(&d, &back, 1) - logit(s)).abs() < 1e-9);
        let any = predicate("signal", Favors::AnyLookup);
        assert!((candidate_feature(&d, &any, 0) - logit(s)).abs() < 1e-9);
        // A named lookup: only that option, wherever the site offers it.
        let named = predicate("signal", Favors::Lookup("get_order".to_string()));
        assert!((candidate_feature(&d, &named, 0) - logit(s)).abs() < 1e-9);
        assert_eq!(candidate_feature(&d, &named, 1), 0.0);
        let elsewhere = predicate("signal", Favors::Lookup("get_user".to_string()));
        assert_eq!(candidate_feature(&d, &elsewhere, 0), 0.0);
        // Unanswered: no feature.
        let missing = predicate("missing", Favors::AnyLookup);
        assert_eq!(candidate_feature(&d, &missing, 0), 0.0);
        assert_eq!(
            candidate_feature(&d, &predicate("p", Favors::AnyLookup), 0),
            0.0
        );
    }

    #[test]
    fn the_log_is_read_as_phase0_writes_it() {
        let line = serde_json::json!({
            "actual": "respond", "agent": "respond", "episode": "e1", "kind": "Next",
            "model": "m", "step": 2, "target": false, "task": "7", "key": "k1",
            "predicates": {"list_pending": 0.56, "cand": 0.9},
            "case": {"site": "get_user_details", "options": ["get_order", "respond"],
                     "features": [[-1.0, 0.0], [-0.5, 1.0]], "pick": 1},
        });
        let arg = serde_json::json!({"kind": {"Arg": "date"}, "case": null, "task": "7"});
        let text = format!("{line}\n{arg}\n");
        let log = read_log(&text).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].options, vec!["get_order", "respond"]);
        assert_eq!(log[0].predicates["cand"], 0.9);
        let cs = cases(&log, &[&predicate("cand", Favors::HandBack)]);
        assert_eq!(cs[0].actual, Some(1));
        assert_eq!(cs[0].features[1].len(), 3);
        assert_eq!(cs[0].group, task_group("7"));
    }
}
