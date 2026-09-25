//! A flow's run as a fugue program (RFC-001 §3.2 and §3.5).
//!
//! After one of the agent's calls returns, a flow may make a lookup, then
//! decide again, until it hands back. [`run`] is that loop as one fugue
//! program. Step `i` has a decision site, `decide#i`: a categorical over
//! handing back ([`HAND_BACK`]) and every lookup the flow could make, which
//! puts weight only on the lookups offered after the call that just
//! returned. Unless it hands back, an outcome site, `outcome#i`, says
//! whether the lookup succeeded. The next decision is made after that
//! lookup, and the program stops after `max_lookups` lookups.
//!
//! The sites' distributions are the flow's own statistics ([`RunModel`]):
//! what the agent did next after each call in training, and how often each
//! tool's calls succeeded. Each distribution carries its site as metadata
//! ([`DecideSite`], [`OutcomeSite`]), so a handler knows which decision, or
//! which call, it is at.
//!
//! One program has three interpreters:
//! - **Simulate.** fugue's `PriorHandler` runs the flow on its statistics
//!   alone: the lookups it would make if the agent did as in training.
//! - **Execute.** `stretto-proxy` runs it with `run_async` and a handler that
//!   makes each decision with the flow's arbiter ([`Flow::next_with`]) and
//!   each lookup against the server, scoring the server's answer at the
//!   outcome site.
//! - **Audit.** `ScoreGivenTrace` scores a recorded run: how surprised the
//!   flow's statistics are by it.

use crate::flow::Flow;
use crate::shadow::{Sites, RESPOND};
use fugue::{
    addr, beta_posterior, dirichlet_predictive, pure, sample, Bernoulli, Categorical, Model,
    ModelExt, WithMeta,
};
use std::collections::HashMap;
use std::sync::Arc;
use stretto_model::world::decode;
use stretto_model::{Action, Vocab};

/// Option 0 at every decision site: hand back.
pub const HAND_BACK: usize = 0;

/// A decision site's metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct DecideSite {
    /// The site: the tool that just returned, marked when it failed, as
    /// [`Sites::name`] writes it.
    pub site: String,
    /// What the decision chooses between: [`RESPOND`] (hand back), then
    /// every lookup the flow could make, by name. The same at every site;
    /// the site's distribution puts weight only on the lookups it offers.
    pub options: Vec<String>,
}

/// An outcome site's metadata: the lookup whose result it scores.
#[derive(Clone, Debug, PartialEq)]
pub struct OutcomeSite {
    /// The tool called.
    pub tool: String,
}

/// A flow's statistics, as its run draws on them.
#[derive(Clone, Debug)]
pub struct RunModel {
    sites: Sites,
    vocab: Vocab,
    /// What followed each step `(action id, failed)` in training: counts by
    /// action id, and their total.
    followed: HashMap<(u32, bool), (HashMap<u32, f64>, f64)>,
}

impl RunModel {
    /// The statistics of `flow`.
    pub fn new(flow: &Flow) -> Self {
        let mut followed: HashMap<(u32, bool), (HashMap<u32, f64>, f64)> = HashMap::new();
        let base = flow.habit.base();
        for id in 0..flow.vocab.len() as u32 {
            for failed in [false, true] {
                let outcome = u32::from(failed);
                let counts = base
                    .followed(|s| matches!(decode(s), Some((a, o, _)) if a == id && o == outcome));
                if counts.1 > 0.0 {
                    followed.insert((id, failed), counts);
                }
            }
        }
        Self {
            sites: flow.sites.clone(),
            vocab: flow.vocab.clone(),
            followed,
        }
    }

    fn counts(&self, tool: &str, failed: bool) -> Option<&(HashMap<u32, f64>, f64)> {
        let id = self.vocab.id(&Action::Tool(tool.to_string()));
        self.followed.get(&(id, failed))
    }

    /// Every decision's options: [`RESPOND`] (hand back), then every lookup
    /// the flow could make.
    pub fn options(&self) -> Vec<String> {
        std::iter::once(RESPOND.to_string())
            .chain(self.sites.reads().cloned())
            .collect()
    }

    /// The lookups the flow offers after a call to `tool` (failed or not),
    /// the options its arbiter weighs there.
    pub fn offered(&self, tool: &str, failed: bool) -> Vec<String> {
        self.sites.options(tool, failed)
    }

    /// The decision after a call to `tool` (failed or not). Handing back and
    /// each lookup offered there get the posterior predictive of a flat
    /// Dirichlet over them, given what the agent did next there in training;
    /// every step but the lookups offered, a write or a message included,
    /// counts as handing back. The lookups not offered there get 0.
    pub fn decide(&self, tool: &str, failed: bool) -> WithMeta<Categorical, DecideSite> {
        let options = self.options();
        let offered = self.offered(tool, failed);
        let (by_action, total) = self
            .counts(tool, failed)
            .map(|(c, t)| (Some(c), *t))
            .unwrap_or((None, 0.0));
        let seen = |lookup: &String| {
            let id = self.vocab.id(&Action::Tool(lookup.clone()));
            by_action.and_then(|c| c.get(&id)).copied().unwrap_or(0.0)
        };
        // Handing back, then each lookup offered: its count in training.
        let looked: f64 = offered.iter().map(seen).sum();
        let counts: Vec<u64> = std::iter::once((total - looked).max(0.0))
            .chain(offered.iter().map(seen))
            .map(|n| n.round() as u64)
            .collect();
        let predictive = dirichlet_predictive(&vec![1.0; counts.len()], &counts)
            .expect("a flat Dirichlet and counts make a predictive");
        let probs: Vec<f64> = options
            .iter()
            .enumerate()
            .map(|(i, option)| match i {
                HAND_BACK => predictive[0],
                _ => offered
                    .iter()
                    .position(|o| o == option)
                    .map_or(0.0, |j| predictive[j + 1]),
            })
            .collect();
        WithMeta::new(
            Categorical::new(probs).expect("predictive probabilities are a distribution"),
            DecideSite {
                site: Sites::name(tool, failed),
                options,
            },
        )
    }

    /// The outcome of a call to `tool`: whether it succeeds, with the
    /// posterior predictive of a flat Beta given how often its calls
    /// succeeded and failed in training.
    pub fn outcome(&self, tool: &str) -> WithMeta<Bernoulli, OutcomeSite> {
        let n = |failed| {
            self.counts(tool, failed)
                .map_or(0, |(_, total)| total.round() as u64)
        };
        let (a, b) = beta_posterior(1.0, 1.0, n(false), n(true))
            .expect("a flat Beta and counts make a posterior");
        WithMeta::new(
            Bernoulli::new(a / (a + b)).expect("a predictive probability is in (0, 1)"),
            OutcomeSite {
                tool: tool.to_string(),
            },
        )
    }
}

/// A flow's run after a call to `tool` (failed or not): the lookups it makes,
/// each with whether it succeeded. See the [module docs](self).
pub fn run(
    model: Arc<RunModel>,
    tool: String,
    failed: bool,
    max_lookups: usize,
) -> Model<Vec<(String, bool)>> {
    step(model, 0, tool, failed, max_lookups)
}

fn step(
    model: Arc<RunModel>,
    i: usize,
    tool: String,
    failed: bool,
    max_lookups: usize,
) -> Model<Vec<(String, bool)>> {
    if i >= max_lookups {
        return pure(Vec::new());
    }
    let decide = model.decide(&tool, failed);
    let options = decide.meta().options.clone();
    sample(addr!("decide", i), decide).bind(move |choice| {
        if choice == HAND_BACK {
            return pure(Vec::new());
        }
        let lookup = options[choice].clone();
        let outcome = model.outcome(&lookup);
        sample(addr!("outcome", i), outcome).bind(move |ok| {
            step(model, i + 1, lookup.clone(), !ok, max_lookups).map(move |rest| {
                let mut steps = vec![(lookup, ok)];
                steps.extend(rest);
                steps
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fugue::runtime::handler::run as interpret;
    use fugue::{PriorHandler, ScoreGivenTrace, Trace};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn flow() -> Flow {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/examples/retail-10-sessions.flow.json");
        Flow::load(&path).unwrap()
    }

    #[test]
    fn a_decision_offers_handing_back_and_the_sites_lookups() {
        let model = RunModel::new(&flow());
        let d = model.decide("get_user_details", false);
        assert_eq!(d.meta().site, "get_user_details");
        assert_eq!(d.meta().options, model.options());
        assert_eq!(d.meta().options[HAND_BACK], RESPOND);
        let probs = d.dist().probs();
        assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Only the lookups offered there, and handing back, carry weight.
        let offered = model.offered("get_user_details", false);
        assert!(offered.iter().any(|o| o == "get_order_details"));
        for (option, p) in d.meta().options.iter().zip(probs).skip(1) {
            assert_eq!(*p > 0.0, offered.contains(option), "{option}");
        }
        // In training the agent read an order after the user's details.
        let order = d
            .meta()
            .options
            .iter()
            .position(|o| o == "get_order_details")
            .unwrap();
        assert!(probs[order] > probs[HAND_BACK], "{:?}", d.meta());
        // A site never seen offers only handing back.
        let unseen = model.decide("no_such_tool", true);
        assert_eq!(unseen.dist().probs()[HAND_BACK], 1.0);
        assert_eq!(unseen.dist().probs().iter().sum::<f64>(), 1.0);
    }

    #[test]
    fn simulating_makes_offered_lookups_at_addressed_sites() {
        let model = Arc::new(RunModel::new(&flow()));
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (steps, trace) = interpret(
                PriorHandler {
                    rng: &mut rng,
                    trace: Trace::default(),
                },
                run(model.clone(), "find_user_id_by_name_zip".into(), false, 4),
            );
            assert!(steps.len() <= 4);
            let (mut tool, mut failed) = ("find_user_id_by_name_zip".to_string(), false);
            for (i, (lookup, ok)) in steps.iter().enumerate() {
                assert!(model.offered(&tool, failed).contains(lookup));
                assert!(trace.choices.contains_key(&addr!("decide", i)));
                assert_eq!(trace.get_bool(&addr!("outcome", i)), Some(*ok));
                (tool, failed) = (lookup.clone(), !ok);
            }
            // The run ends by handing back, or after its last lookup.
            let last = trace.get_usize(&addr!("decide", steps.len()));
            assert!(steps.len() == 4 || last == Some(HAND_BACK));
            // Replayed under the same program, the run scores the same.
            let (_, scored) = interpret(
                ScoreGivenTrace {
                    base: trace.clone(),
                    trace: Trace::default(),
                },
                run(model.clone(), "find_user_id_by_name_zip".into(), false, 4),
            );
            assert!((scored.total_log_weight() - trace.total_log_weight()).abs() < 1e-12);
        }
    }
}
