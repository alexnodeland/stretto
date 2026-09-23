//! A hierarchical Dirichlet back-off model of the agent's next action.
//!
//! With `c_j` the last `j` steps,
//!
//! ```text
//! P_j(a | c_j) = (n(c_j, a) + α · P_{j-1}(a | c_{j-1})) / (n(c_j) + α),
//! ```
//!
//! and `P_{-1}` uniform over the vocabulary. Each level is a Dirichlet posterior
//! predictive whose prior mean is the level below: MacKay & Peto's hierarchical
//! Dirichlet language model, with one concentration `α` shared across levels
//! (see [`crate::alpha`] for its posterior). Contexts never seen in training fall
//! through to the level below, so predictions degrade gracefully with sparsity.

use crate::abstraction::{Step, Vocab};
use serde::Serialize;
use std::collections::HashMap;

/// A step encoded for use as context: `action id × 4 + outcome index`.
pub type Symbol = u32;

/// Padding before the first step.
const START: Symbol = u32::MAX;

/// An episode as the model sees it.
#[derive(Clone, Debug)]
pub struct EncodedEpisode {
    /// Action id at each step.
    pub actions: Vec<u32>,
    /// Context symbol of each step (action and outcome).
    pub symbols: Vec<Symbol>,
    /// Whether the episode solved its task.
    pub success: bool,
}

impl EncodedEpisode {
    /// Encode abstract steps with `vocab`.
    pub fn encode(steps: &[Step], vocab: &Vocab, success: bool) -> Self {
        let actions: Vec<u32> = steps.iter().map(|s| vocab.id(&s.action)).collect();
        let symbols = steps
            .iter()
            .zip(&actions)
            .map(|(s, &a)| a * 4 + s.outcome.index())
            .collect();
        Self {
            actions,
            symbols,
            success,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Counts {
    by_action: HashMap<u32, f64>,
    total: f64,
}

/// The back-off model.
#[derive(Clone, Debug)]
pub struct BackoffModel {
    order: usize,
    alpha: f64,
    vocab_size: usize,
    levels: Vec<HashMap<Vec<Symbol>, Counts>>,
}

impl BackoffModel {
    /// An empty model conditioning on up to `order` previous steps.
    pub fn new(order: usize, alpha: f64, vocab_size: usize) -> Self {
        assert!(alpha > 0.0, "concentration must be positive");
        assert!(vocab_size > 0, "vocabulary must be non-empty");
        Self {
            order,
            alpha,
            vocab_size,
            levels: vec![HashMap::new(); order + 1],
        }
    }

    /// A model trained on `episodes`.
    pub fn fit(order: usize, alpha: f64, vocab_size: usize, episodes: &[EncodedEpisode]) -> Self {
        let mut m = Self::new(order, alpha, vocab_size);
        for ep in episodes {
            for t in 0..ep.actions.len() {
                m.observe(&ep.symbols[..t], ep.actions[t]);
            }
        }
        m
    }

    /// Context length.
    pub fn order(&self) -> usize {
        self.order
    }

    /// Concentration.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    fn context(history: &[Symbol], j: usize) -> Vec<Symbol> {
        let mut c = vec![START; j.saturating_sub(history.len())];
        c.extend_from_slice(&history[history.len().saturating_sub(j)..]);
        c
    }

    /// Record that `action` followed `history`.
    pub fn observe(&mut self, history: &[Symbol], action: u32) {
        for j in 0..=self.order {
            let c = self.levels[j].entry(Self::context(history, j)).or_default();
            *c.by_action.entry(action).or_insert(0.0) += 1.0;
            c.total += 1.0;
        }
    }

    /// Posterior predictive over every action id, given `history`.
    pub fn predict(&self, history: &[Symbol]) -> Vec<f64> {
        let mut p = vec![1.0 / self.vocab_size as f64; self.vocab_size];
        for j in 0..=self.order {
            if let Some(c) = self.levels[j].get(&Self::context(history, j)) {
                let denom = c.total + self.alpha;
                for (a, pa) in p.iter_mut().enumerate() {
                    let n = c.by_action.get(&(a as u32)).copied().unwrap_or(0.0);
                    *pa = (n + self.alpha * *pa) / denom;
                }
            }
        }
        p
    }

    /// Posterior predictive probability of one action.
    pub fn prob(&self, history: &[Symbol], action: u32) -> f64 {
        let mut p = 1.0 / self.vocab_size as f64;
        for j in 0..=self.order {
            if let Some(c) = self.levels[j].get(&Self::context(history, j)) {
                let n = c.by_action.get(&action).copied().unwrap_or(0.0);
                p = (n + self.alpha * p) / (c.total + self.alpha);
            }
        }
        p
    }

    /// How many training observations share the full-length context of
    /// `history`: the evidence a top-level prediction rests on.
    pub fn evidence(&self, history: &[Symbol]) -> f64 {
        self.levels[self.order]
            .get(&Self::context(history, self.order))
            .map(|c| c.total)
            .unwrap_or(0.0)
    }
}

/// Held-out predictive quality.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct EvalStats {
    /// Decisions evaluated.
    pub steps: usize,
    /// Cross-entropy of the agent's actual actions, in bits per decision.
    pub bits_per_step: f64,
    /// Share of decisions where the model's top option is what the agent did.
    pub top1: f64,
}

/// Evaluate `model` on held-out episodes.
pub fn evaluate(model: &BackoffModel, episodes: &[EncodedEpisode]) -> EvalStats {
    let (mut n, mut bits, mut hits) = (0usize, 0.0, 0usize);
    for ep in episodes {
        for t in 0..ep.actions.len() {
            let p = model.predict(&ep.symbols[..t]);
            let a = ep.actions[t] as usize;
            bits -= p[a].log2();
            hits += (argmax(&p) == a) as usize;
            n += 1;
        }
    }
    if n == 0 {
        return EvalStats::default();
    }
    EvalStats {
        steps: n,
        bits_per_step: bits / n as f64,
        top1: hits as f64 / n as f64,
    }
}

/// One point on a coverage/accuracy curve.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct CoveragePoint {
    /// Confidence threshold on the habit's top option.
    pub threshold: f64,
    /// Share of decisions the habit would take at this threshold.
    pub coverage: f64,
    /// How often the habit's top option matched the agent on those decisions.
    pub accuracy: f64,
}

/// For each threshold `τ`: the share of held-out decisions where the habit's
/// top option has probability at least `τ` *and* rests on at least
/// `min_evidence` training observations, and how often that option matches
/// what the agent actually did.
pub fn coverage_curve(
    model: &BackoffModel,
    episodes: &[EncodedEpisode],
    thresholds: &[f64],
    min_evidence: f64,
) -> Vec<CoveragePoint> {
    let mut decisions: Vec<(f64, bool)> = Vec::new();
    for ep in episodes {
        for t in 0..ep.actions.len() {
            let h = &ep.symbols[..t];
            let p = model.predict(h);
            let top = argmax(&p);
            let conf = if model.evidence(h) >= min_evidence {
                p[top]
            } else {
                0.0
            };
            decisions.push((conf, top == ep.actions[t] as usize));
        }
    }
    let n = decisions.len().max(1) as f64;
    thresholds
        .iter()
        .map(|&tau| {
            let taken: Vec<bool> = decisions
                .iter()
                .filter(|(c, _)| *c >= tau)
                .map(|(_, ok)| *ok)
                .collect();
            let hits = taken.iter().filter(|&&ok| ok).count();
            CoveragePoint {
                threshold: tau,
                coverage: taken.len() as f64 / n,
                accuracy: if taken.is_empty() {
                    0.0
                } else {
                    hits as f64 / taken.len() as f64
                },
            }
        })
        .collect()
}

/// Where in the conversation a decision is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Position {
    /// Right after the user spoke (including the first decision): the agent
    /// is choosing how to respond to new intent.
    AfterUser,
    /// Right after a tool returned: the agent is continuing a run.
    AfterTool,
}

impl Position {
    /// The position of decision `t` in `ep`.
    pub fn of(ep: &EncodedEpisode, t: usize) -> Self {
        match t.checked_sub(1).map(|p| ep.symbols[p] % 4) {
            Some(0) | Some(1) => Position::AfterTool,
            _ => Position::AfterUser,
        }
    }
}

/// Predictability and coverage at one [`Position`].
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PositionStats {
    /// Where the decisions were made.
    pub position: Position,
    /// Share of all held-out decisions made here.
    pub share: f64,
    /// Held-out predictability here.
    pub eval: EvalStats,
    /// Coverage/accuracy here at the requested threshold.
    pub coverage: CoveragePoint,
}

/// [`evaluate`] and [`coverage_curve`] split by [`Position`], at one threshold.
pub fn by_position(
    model: &BackoffModel,
    episodes: &[EncodedEpisode],
    threshold: f64,
    min_evidence: f64,
) -> Vec<PositionStats> {
    let positions = [Position::AfterUser, Position::AfterTool];
    let total: usize = episodes.iter().map(|e| e.actions.len()).sum();
    positions
        .iter()
        .map(|&pos| {
            // Per-position counters: steps, bits, top-1 hits, taken, taken-and-right.
            let (mut n, mut bits, mut hits, mut taken, mut right) = (0usize, 0.0, 0usize, 0, 0);
            for ep in episodes {
                for t in (0..ep.actions.len()).filter(|&t| Position::of(ep, t) == pos) {
                    let h = &ep.symbols[..t];
                    let p = model.predict(h);
                    let a = ep.actions[t] as usize;
                    let top = argmax(&p);
                    n += 1;
                    bits -= p[a].log2();
                    hits += (top == a) as usize;
                    if model.evidence(h) >= min_evidence && p[top] >= threshold {
                        taken += 1;
                        right += (top == a) as usize;
                    }
                }
            }
            let nf = n.max(1) as f64;
            PositionStats {
                position: pos,
                share: n as f64 / total.max(1) as f64,
                eval: EvalStats {
                    steps: n,
                    bits_per_step: bits / nf,
                    top1: hits as f64 / nf,
                },
                coverage: CoveragePoint {
                    threshold,
                    coverage: taken as f64 / nf,
                    accuracy: if taken == 0 {
                        0.0
                    } else {
                        right as f64 / taken as f64
                    },
                },
            }
        })
        .collect()
}

/// Index of the largest element (the first, on ties).
pub fn argmax(p: &[f64]) -> usize {
    (0..p.len()).fold(0, |best, i| if p[i] > p[best] { i } else { best })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(actions: &[u32]) -> EncodedEpisode {
        EncodedEpisode {
            actions: actions.to_vec(),
            symbols: actions.iter().map(|a| a * 4).collect(),
            success: true,
        }
    }

    #[test]
    fn predictive_is_normalized_and_backs_off() {
        let data = vec![ep(&[1, 2, 1, 2, 1, 2]); 5];
        let m = BackoffModel::fit(2, 0.5, 4, &data);
        for t in 0..6 {
            let p = m.predict(&data[0].symbols[..t]);
            let z: f64 = p.iter().sum();
            assert!((z - 1.0).abs() < 1e-12, "not normalized at {t}: {z}");
            for a in 0..4u32 {
                let single = m.prob(&data[0].symbols[..t], a);
                assert!((single - p[a as usize]).abs() < 1e-12);
            }
        }
        // After "1", "2" is near-certain; the unseen context backs off.
        let after_one = m.predict(&[4]);
        assert!(after_one[2] > 0.9);
        let unseen = m.predict(&[12, 12]);
        assert!((unseen.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_deterministic_process_is_learned() {
        let train = vec![ep(&[1, 2, 3, 1, 2, 3]); 20];
        let m = BackoffModel::fit(1, 0.1, 5, &train);
        let s = evaluate(&m, &train[..1]);
        assert_eq!(s.steps, 6);
        assert!(s.top1 > 0.8, "top1 {}", s.top1);
        assert!(s.bits_per_step < 1.0, "bits {}", s.bits_per_step);
        let curve = coverage_curve(&m, &train[..1], &[0.5, 0.99], 1.0);
        assert!(curve[0].coverage >= curve[1].coverage);
        assert!(curve[0].accuracy > 0.8);
    }
}
