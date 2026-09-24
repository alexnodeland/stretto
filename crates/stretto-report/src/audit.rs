//! Auditing a flow against recorded episodes, with the flow as a fugue
//! program (RFC-001 §3.2 and §3.7).
//!
//! Wherever a flow would decide in an episode (right after a lookup
//! returns), its arbiter gives a distribution over the site's options: each
//! lookup offered there, handing back, and whatever mass is left for a step
//! it does not offer. [`model`] is those choices as one fugue program: a
//! categorical `sample` per decision, at address `decision#i`. The same
//! program serves two handlers:
//!
//! - fugue's `PriorHandler` runs the flow as a stochastic policy: what it
//!   would pick at each decision ([`simulate`]);
//! - `ScoreGivenTrace`, given a trace of the agent's own steps, scores them:
//!   the log-probability of what the agent did, which is how surprised the
//!   flow is by the episode ([`score`]).
//!
//! [`audit`] reports agreement (whether the flow's likeliest option was the
//! agent's step), calibration, and surprise per site and per episode. Sites
//! where the flow no longer fits the agent, and the episodes it fits least,
//! stand out. That is the check to run on new sessions before trusting a
//! flow compiled from older ones.

use crate::flow::Flow;
use crate::shadow::RESPOND;
use fugue::runtime::handler::run;
use fugue::{
    addr, sample, traverse_vec, Categorical, ChoiceValue, Model, PriorHandler, ScoreGivenTrace,
    Trace,
};
use rand::RngCore;
use serde::Serialize;
use std::collections::BTreeMap;
use stretto_oracle::Oracle;
use stretto_trace::{Episode, Event, ToolKind};

/// The option for an agent's step the flow did not offer.
pub const OTHER: &str = "other";

/// Probabilities below this are raised to it, so a step the flow thought
/// impossible costs a large but finite surprise.
const FLOOR: f64 = 1e-6;

/// One decision of the flow in a recorded episode.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Decision {
    /// The site: the lookup that had just returned.
    pub site: String,
    /// The options: the lookups offered, handing back, and [`OTHER`].
    pub options: Vec<String>,
    /// The flow's probability of each option (they sum to one).
    pub probs: Vec<f64>,
    /// The agent's own step, as an index into `options`.
    pub actual: usize,
}

impl Decision {
    /// The flow's likeliest option.
    pub fn top(&self) -> usize {
        self.probs.iter().enumerate().fold(
            0,
            |best, (i, &p)| if p > self.probs[best] { i } else { best },
        )
    }
}

/// Every decision `flow` would make in `episode`, with the agent's own step
/// at each, and how many decisions the oracle could not answer (a replay
/// cache without the question, say), which are left out.
pub fn decisions(flow: &Flow, episode: &Episode, oracle: &dyn Oracle) -> (Vec<Decision>, usize) {
    let mut out = Vec::new();
    let mut unanswered = 0;
    let events = &episode.events;
    for (i, e) in events.iter().enumerate() {
        // A decision point: a tool result, with every call of its turn
        // answered, and something after it.
        let (Event::ToolResult { .. }, Some(after)) = (e, events.get(i + 1)) else {
            continue;
        };
        if matches!(after, Event::ToolResult { .. }) {
            continue;
        }
        let prefix = Episode {
            events: events[..=i].to_vec(),
            ..episode.clone()
        };
        // Only the arbiter's probabilities are wanted, so no threshold is met
        // and no arguments are bound.
        let Ok(next) = flow.next(&prefix, oracle, f64::INFINITY) else {
            unanswered += 1;
            continue;
        };
        if next.probs.is_empty() {
            // The flow asked nothing here (no lookups followed this site in
            // training), or its question went unanswered.
            if next.key.is_some() {
                unanswered += 1;
            }
            continue;
        }
        let actual = match after {
            Event::Assistant { calls, .. } if !calls.is_empty() => {
                let tool = &calls[0].name;
                if next.probs.contains_key(tool) {
                    tool.clone()
                } else if flow.manifest().tools.get(tool) == Some(&ToolKind::Read) {
                    OTHER.to_string()
                } else {
                    // A write, or a tool that is neither: the flow hands back.
                    RESPOND.to_string()
                }
            }
            // A reply to the customer, or the customer speaking first.
            _ => RESPOND.to_string(),
        };
        let mut options: Vec<String> = next.probs.keys().cloned().collect();
        let mut probs: Vec<f64> = next.probs.values().copied().collect();
        let rest = 1.0 - probs.iter().sum::<f64>();
        options.push(OTHER.to_string());
        probs.push(rest.max(0.0));
        let probs = normalized(&probs);
        let actual = options
            .iter()
            .position(|o| *o == actual)
            .unwrap_or(options.len() - 1);
        out.push(Decision {
            site: next.site.unwrap_or_default(),
            options,
            probs,
            actual,
        });
    }
    (out, unanswered)
}

fn normalized(probs: &[f64]) -> Vec<f64> {
    let floored: Vec<f64> = probs.iter().map(|p| p.max(FLOOR)).collect();
    let total: f64 = floored.iter().sum();
    floored.iter().map(|p| p / total).collect()
}

/// The flow's decisions as one fugue program: a categorical choice per
/// decision, at `decision#i`, returning the options picked.
pub fn model(decisions: &[Decision]) -> Model<Vec<usize>> {
    let sites: Vec<(usize, Vec<f64>)> = decisions
        .iter()
        .enumerate()
        .map(|(i, d)| (i, d.probs.clone()))
        .collect();
    traverse_vec(sites, |(i, probs)| {
        sample(
            addr!("decision", i),
            Categorical::new(probs).expect("decision probabilities are normalized"),
        )
    })
}

/// The flow run as a stochastic policy: an option drawn at each decision.
pub fn simulate(decisions: &[Decision], rng: &mut impl RngCore) -> Vec<usize> {
    let (picks, _) = run(
        PriorHandler {
            rng,
            trace: Trace::default(),
        },
        model(decisions),
    );
    picks
}

/// The agent's own steps scored under the flow: a trace whose `log_prior` is
/// the log-probability (nats) of the agent's path, with each decision's
/// share at `decision#i`.
pub fn score(decisions: &[Decision]) -> Trace {
    let mut base = Trace::default();
    for (i, d) in decisions.iter().enumerate() {
        base.insert_choice(addr!("decision", i), ChoiceValue::Usize(d.actual), 0.0);
    }
    let (_, trace) = run(
        ScoreGivenTrace {
            base,
            trace: Trace::default(),
        },
        model(decisions),
    );
    trace
}

/// Agreement and surprise at one site.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SiteAudit {
    /// The site.
    pub site: String,
    /// Decisions made there.
    pub decisions: usize,
    /// Share where the flow's likeliest option was the agent's step.
    pub agreement: f64,
    /// Mean log-probability of the agent's step (nats).
    pub mean_log_prob: f64,
}

/// How surprised the flow was by one episode.
#[derive(Clone, Debug, Serialize)]
pub struct EpisodeSurprise {
    /// The episode.
    pub episode: String,
    /// Its task.
    pub task: String,
    /// Decisions scored.
    pub decisions: usize,
    /// Mean log-probability of the agent's steps (nats).
    pub mean_log_prob: f64,
}

/// One bin of the calibration table: decisions whose likeliest option had a
/// probability in the bin.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CalibrationBin {
    /// Lower edge of the bin.
    pub from: f64,
    /// Decisions in it.
    pub decisions: usize,
    /// Their mean top probability.
    pub mean_prob: f64,
    /// How often the top option was the agent's step.
    pub agreement: f64,
}

/// A flow audited against recorded episodes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Audit {
    /// The flow's domain.
    pub domain: String,
    /// Episodes read.
    pub episodes: usize,
    /// Decisions scored.
    pub decisions: usize,
    /// Decisions left out because the oracle had no answer.
    pub unanswered: usize,
    /// Log-probability of every agent step scored (nats), from fugue's
    /// `ScoreGivenTrace`.
    pub log_prob: f64,
    /// Per decision.
    pub mean_log_prob: f64,
    /// Share of decisions where the flow's likeliest option was the agent's.
    pub agreement: f64,
    /// The flow's mean probability of the agent's step: its agreement if it
    /// picked at random from its own probabilities.
    pub expected_agreement: f64,
    /// Expected calibration error of the top option.
    pub ece: f64,
    /// By top probability, in tenths.
    pub calibration: Vec<CalibrationBin>,
    /// By site, most decisions first.
    pub sites: Vec<SiteAudit>,
    /// The episodes the flow fit least (lowest mean log-probability), up to
    /// ten.
    pub surprising: Vec<EpisodeSurprise>,
}

/// Audit `flow` against `episodes`, asking `oracle` its questions.
pub fn audit(flow: &Flow, episodes: &[Episode], oracle: &dyn Oracle) -> Audit {
    let mut out = Audit {
        domain: flow.domain().to_string(),
        episodes: episodes.len(),
        ..Default::default()
    };
    let mut sites: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
    let mut bins = vec![(0usize, 0.0f64, 0usize); 10];
    let mut surprise = Vec::new();
    let mut agreed = 0;
    let mut expected = 0.0;
    for ep in episodes {
        let (ds, unanswered) = decisions(flow, ep, oracle);
        out.unanswered += unanswered;
        if ds.is_empty() {
            continue;
        }
        let trace = score(&ds);
        out.log_prob += trace.log_prior;
        out.decisions += ds.len();
        for (i, d) in ds.iter().enumerate() {
            let lp = trace
                .choices
                .get(&addr!("decision", i))
                .map_or(f64::NEG_INFINITY, |c| c.logp);
            let hit = d.top() == d.actual;
            agreed += usize::from(hit);
            expected += d.probs[d.actual];
            let s = sites.entry(d.site.clone()).or_default();
            s.0 += 1;
            s.1 += usize::from(hit);
            s.2 += lp;
            let top = d.probs[d.top()];
            let b = &mut bins[((top * 10.0) as usize).min(9)];
            b.0 += 1;
            b.1 += top;
            b.2 += usize::from(hit);
        }
        surprise.push(EpisodeSurprise {
            episode: ep.id.clone(),
            task: ep.task_id.clone(),
            decisions: ds.len(),
            mean_log_prob: trace.log_prior / ds.len() as f64,
        });
    }
    if out.decisions == 0 {
        return out;
    }
    let n = out.decisions as f64;
    out.mean_log_prob = out.log_prob / n;
    out.agreement = agreed as f64 / n;
    out.expected_agreement = expected / n;
    out.calibration = bins
        .iter()
        .enumerate()
        .filter(|(_, b)| b.0 > 0)
        .map(|(k, &(count, p, hits))| CalibrationBin {
            from: k as f64 / 10.0,
            decisions: count,
            mean_prob: p / count as f64,
            agreement: hits as f64 / count as f64,
        })
        .collect();
    out.ece = out
        .calibration
        .iter()
        .map(|b| b.decisions as f64 / n * (b.mean_prob - b.agreement).abs())
        .sum();
    out.sites = sites
        .into_iter()
        .map(|(site, (count, hits, lp))| SiteAudit {
            site,
            decisions: count,
            agreement: hits as f64 / count as f64,
            mean_log_prob: lp / count as f64,
        })
        .collect();
    out.sites
        .sort_by(|a, b| b.decisions.cmp(&a.decisions).then(a.site.cmp(&b.site)));
    surprise.sort_by(|a, b| a.mean_log_prob.total_cmp(&b.mean_log_prob));
    surprise.truncate(10);
    out.surprising = surprise;
    out
}

/// The audit as Markdown.
pub fn markdown(a: &Audit) -> String {
    let pct = |x: f64| format!("{:.1}%", 100.0 * x);
    let mut md = format!(
        "# Flow audit: {}\n\n\
         {} episodes, {} decisions scored ({} left out: no answer from the oracle).\n\n\
         - **Agreement:** the flow's likeliest option was the agent's step at {} of decisions \
         ({} expected if it picked at random from its own probabilities).\n\
         - **Surprise:** {:.3} nats per decision (the agent's path has log-probability {:.1} under \
         the flow, scored by fugue's `ScoreGivenTrace`).\n\
         - **Calibration:** expected calibration error {:.3}.\n",
        a.domain,
        a.episodes,
        a.decisions,
        a.unanswered,
        pct(a.agreement),
        pct(a.expected_agreement),
        -a.mean_log_prob,
        a.log_prob,
        a.ece,
    );
    md.push_str(
        "\n## Sites\n\n| Site | Decisions | Agreement | Nats per decision |\n|---|---|---|---|\n",
    );
    for s in &a.sites {
        md.push_str(&format!(
            "| `{}` | {} | {} | {:.3} |\n",
            s.site,
            s.decisions,
            pct(s.agreement),
            -s.mean_log_prob
        ));
    }
    md.push_str("\n## Calibration\n\n| Top probability | Decisions | Mean | Agreement |\n|---|---|---|---|\n");
    for b in &a.calibration {
        md.push_str(&format!(
            "| {:.1}–{:.1} | {} | {:.3} | {} |\n",
            b.from,
            b.from + 0.1,
            b.decisions,
            b.mean_prob,
            pct(b.agreement)
        ));
    }
    md.push_str("\n## Episodes the flow fit least\n\n| Episode | Task | Decisions | Nats per decision |\n|---|---|---|---|\n");
    for e in &a.surprising {
        md.push_str(&format!(
            "| {} | {} | {} | {:.3} |\n",
            e.episode, e.task, e.decisions, -e.mean_log_prob
        ));
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn decision(probs: &[f64], actual: usize) -> Decision {
        Decision {
            site: "s".to_string(),
            options: (0..probs.len()).map(|i| format!("o{i}")).collect(),
            probs: probs.to_vec(),
            actual,
        }
    }

    #[test]
    fn scores_the_agents_path_with_score_given_trace() {
        let ds = [
            decision(&[0.7, 0.2, 0.1], 0),
            decision(&[0.25, 0.25, 0.5], 1),
        ];
        let trace = score(&ds);
        let expected = 0.7f64.ln() + 0.25f64.ln();
        assert!((trace.log_prior - expected).abs() < 1e-12);
        let first = trace.choices.get(&addr!("decision", 0)).unwrap();
        assert!((first.logp - 0.7f64.ln()).abs() < 1e-12);
        assert_eq!(ds[1].top(), 2);
    }

    #[test]
    fn simulates_the_flow_as_a_policy() {
        // A sure decision always comes out the same; the other follows its
        // probabilities.
        let ds = [
            decision(&[1.0 - 2e-6, 1e-6, 1e-6], 0),
            decision(&[0.5, 0.5], 0),
        ];
        let mut rng = StdRng::seed_from_u64(7);
        let draws: Vec<Vec<usize>> = (0..400).map(|_| simulate(&ds, &mut rng)).collect();
        assert!(draws.iter().all(|d| d[0] == 0));
        let ones = draws.iter().filter(|d| d[1] == 1).count();
        assert!((150..250).contains(&ones), "{ones}");
    }

    #[test]
    fn floors_impossible_steps() {
        let p = normalized(&[0.6, 0.4, 0.0]);
        assert!(p[2] > 0.0 && (p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}
