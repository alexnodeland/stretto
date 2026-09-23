//! Posterior over the back-off concentration `α`, with fugue.
//!
//! The sequential predictive of [`BackoffModel`] defines a proper joint
//! distribution over an action sequence, so its product over the training data
//! (the prequential log-likelihood) is the marginal likelihood of the data
//! given `α`. A prior on `log α` plus that likelihood as a fugue `factor` gives
//! the posterior by adaptive Metropolis–Hastings.

use crate::world::{BackoffModel, EncodedEpisode};
use fugue::{adaptive_mcmc_chain, addr, factor, sample, ModelExt, Normal};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::Serialize;
use std::sync::Arc;

/// Prequential log-likelihood (nats) of `episodes` under a back-off model
/// with concentration `alpha`, updated as it reads.
pub fn prequential_loglik(
    episodes: &[EncodedEpisode],
    order: usize,
    vocab_size: usize,
    alpha: f64,
) -> f64 {
    let mut m = BackoffModel::new(order, alpha, vocab_size);
    let mut ll = 0.0;
    for ep in episodes {
        for t in 0..ep.actions.len() {
            let h = &ep.symbols[..t];
            ll += m.prob(h, ep.actions[t]).ln();
            m.observe(h, ep.actions[t]);
        }
    }
    ll
}

/// Summary of the posterior over `α`.
#[derive(Clone, Debug, Serialize)]
pub struct AlphaPosterior {
    /// Posterior median.
    pub median: f64,
    /// 5th percentile.
    pub lo: f64,
    /// 95th percentile.
    pub hi: f64,
    /// Number of draws kept.
    pub draws: usize,
}

/// Posterior over `α` given `episodes`, with a `Normal(0, 2)` prior on
/// `log α`, from `n_samples` adaptive-MH draws after `n_samples / 2` warm-up.
pub fn alpha_posterior(
    episodes: Arc<Vec<EncodedEpisode>>,
    order: usize,
    vocab_size: usize,
    n_samples: usize,
    seed: u64,
) -> AlphaPosterior {
    let mut rng = StdRng::seed_from_u64(seed);
    let draws = adaptive_mcmc_chain(
        &mut rng,
        move || {
            let episodes = episodes.clone();
            sample(addr!("log_alpha"), Normal::new(0.0, 2.0).unwrap()).bind(move |log_alpha| {
                let alpha = log_alpha.exp();
                factor(prequential_loglik(&episodes, order, vocab_size, alpha)).map(move |_| alpha)
            })
        },
        n_samples,
        n_samples / 2,
    );
    let mut alphas: Vec<f64> = draws.into_iter().map(|(a, _)| a).collect();
    alphas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |f: f64| alphas[((alphas.len() - 1) as f64 * f).round() as usize];
    AlphaPosterior {
        median: q(0.5),
        lo: q(0.05),
        hi: q(0.95),
        draws: alphas.len(),
    }
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
    fn a_regular_process_prefers_a_small_concentration() {
        // Perfectly regular data: trusting the counts (small α) explains it
        // better than smoothing toward uniform (large α).
        let data = vec![ep(&[1, 2, 3, 1, 2, 3, 1, 2, 3]); 10];
        let small = prequential_loglik(&data, 2, 6, 0.1);
        let large = prequential_loglik(&data, 2, 6, 50.0);
        assert!(small > large, "{small} vs {large}");

        let post = alpha_posterior(Arc::new(data), 2, 6, 300, 11);
        assert!(post.lo <= post.median && post.median <= post.hi);
        assert!(post.median < 1.0, "median α {}", post.median);
    }
}
