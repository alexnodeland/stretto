//! The math film's data from stretto's own code, on the console's test
//! fixtures: the six sessions `stretto learn` turned into `shop.flow.json`.
//!
//!   cargo run --release -- ../../../../crates/stretto-console/tests/fixtures/home/logs/shop > fugue.json
//!   python3 results.py ../../../../docs/results/reach-2026-09-26.json fugue.json > ../data.js
//!
//! It writes, as JSON:
//! - `alpha`: the chain `stretto learn` runs for α (stretto-model's
//!   `alpha_posterior`: a `Normal(0, 2)` prior on `log α`, the prequential
//!   log-likelihood as a `factor`, 600 adaptive Metropolis–Hastings draws after
//!   300 warm-up, seed 7), every kept draw in order; and the same posterior's
//!   density on a grid of `log α`, from the prior and the likelihood directly.
//! - `backoff`: the counts of the back-off model at α's median after one real
//!   history, level by level, for the next step and for use before the next
//!   write (`BackoffModel::fit` and `fit_reach`).
use anyhow::{Context, Result};
use fugue::{adaptive_mcmc_chain, addr, factor, sample, ModelExt, Normal};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use stretto_model::abstraction::{steps, Action, Vocab};
use stretto_model::alpha::prequential_loglik;
use stretto_model::world::{BackoffModel, EncodedEpisode};

const ORDER: usize = 2;
const SAMPLES: usize = 600;
const SEED: u64 = 7;

fn name(vocab: &Vocab, id: u32) -> String {
    match vocab.action(id) {
        Some(Action::Tool(t)) => t.clone(),
        Some(Action::Respond) => "reply".into(),
        None => "unseen".into(),
    }
}

fn main() -> Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).context("usage: math-film-data SESSIONS_DIR")?);
    let logs = stretto_trace::mcp::read_sessions(&dir)?;
    // As `stretto learn` reads sessions: a session with no reward recorded passed.
    let episodes: Vec<_> = logs
        .iter()
        .map(|l| {
            let mut e = stretto_trace::mcp::episode(l);
            e.reward = 1.0;
            e
        })
        .collect();
    let all: Vec<_> = episodes.iter().map(steps).collect();
    let vocab = Vocab::build(all.iter().flatten(), std::iter::empty());
    let encoded: Vec<EncodedEpisode> = episodes
        .iter()
        .zip(&all)
        .map(|(e, s)| EncodedEpisode::encode(s, &vocab, e.succeeded()))
        .filter(|e| e.success)
        .collect();
    let v = vocab.len();
    let names: Vec<String> = (0..v as u32).map(|i| name(&vocab, i)).collect();

    // alpha's chain, as stretto-model's alpha_posterior runs it.
    let eps = Arc::new(encoded.clone());
    let mut rng = StdRng::seed_from_u64(SEED);
    let chain = adaptive_mcmc_chain(
        &mut rng,
        {
            let eps = eps.clone();
            move || {
                let eps = eps.clone();
                sample(addr!("log_alpha"), Normal::new(0.0, 2.0).unwrap()).bind(move |log_alpha| {
                    let alpha = log_alpha.exp();
                    factor(prequential_loglik(&eps, ORDER, v, alpha)).map(move |_| alpha)
                })
            }
        },
        SAMPLES,
        SAMPLES / 2,
    );
    let draws: Vec<f64> = chain.into_iter().map(|(a, _)| a).collect();
    let mut sorted = draws.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |f: f64| sorted[((sorted.len() - 1) as f64 * f).round() as usize];
    let median = q(0.5);
    // The same posterior on a grid: log prior + log likelihood, normalized.
    let grid: Vec<f64> = (0..=200).map(|i| -5.0 + 10.0 * i as f64 / 200.0).collect();
    let logp: Vec<f64> = grid
        .iter()
        .map(|&la| -0.5 * (la / 2.0f64).powi(2) + prequential_loglik(&eps, ORDER, v, la.exp()))
        .collect();
    let max = logp.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let dens: Vec<f64> = logp.iter().map(|l| (l - max).exp()).collect();
    let loglik: Vec<f64> = grid.iter().map(|&la| prequential_loglik(&eps, ORDER, v, la.exp())).collect();

    // The back-off model at alpha's median, after a real history.
    let next = BackoffModel::fit(ORDER, median, v, &encoded);
    let write = |a: u32| matches!(vocab.action(a), Some(Action::Tool(t)) if t == "cancel_pending_order");
    let reach = BackoffModel::fit_reach(ORDER, median, v, &encoded, write);
    // Every distinct history of length 1..=2 seen in training, with both
    // models' predictions: the film picks one.
    let mut histories: Vec<Vec<u32>> = Vec::new();
    let mut hist_names: Vec<Vec<String>> = Vec::new();
    for e in &encoded {
        for t in 1..=e.actions.len() {
            let h = e.symbols[t.saturating_sub(ORDER)..t].to_vec();
            if !histories.contains(&h) {
                hist_names.push(e.actions[t.saturating_sub(ORDER)..t].iter().map(|&a| name(&vocab, a)).collect());
                histories.push(h);
            }
        }
    }
    let levels = |m: &BackoffModel, h: &[u32]| -> Value {
        // p_j for j = -1 (uniform), 0, 1, 2: a model of order j, on the same counts.
        let mut out = vec![json!({"j": -1, "p": vec![1.0 / v as f64; v]})];
        for j in 0..=ORDER {
            let mj = sub(m, j, v, &encoded, median, write, std::ptr::eq(m, &reach));
            out.push(json!({"j": j, "p": mj.predict(h), "evidence": mj.evidence(h)}));
        }
        Value::Array(out)
    };
    let examples: Vec<Value> = histories
        .iter()
        .zip(&hist_names)
        .map(|(h, n)| json!({"history": n, "next": levels(&next, h), "reach": levels(&reach, h)}))
        .collect();
    let seqs: Vec<Vec<String>> = encoded.iter().map(|e| e.actions.iter().map(|&a| name(&vocab, a)).collect()).collect();

    let out = json!({
        "about": "stretto-model on the console's fixtures (logs/shop: the quickstart's six sessions), by brand/video/math/data",
        "vocab": names,
        "episodes": seqs,
        "alpha": {"prior": "log α ~ Normal(0, 2)", "samples": SAMPLES, "warmup": SAMPLES / 2, "seed": SEED,
                  "draws": draws, "median": median, "lo": q(0.05), "hi": q(0.95),
                  "grid_log_alpha": grid, "density": dens, "loglik": loglik},
        "backoff": {"order": ORDER, "alpha": median, "write": "cancel_pending_order", "examples": examples},
    });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

/// The model of order `j` on the same episodes: its prediction is the
/// back-off chain cut at level `j`, the shorter contexts as its prior mean.
#[allow(clippy::too_many_arguments)]
fn sub(
    _m: &BackoffModel,
    j: usize,
    v: usize,
    eps: &[EncodedEpisode],
    alpha: f64,
    write: impl Fn(u32) -> bool,
    reach: bool,
) -> BackoffModel {
    if reach {
        BackoffModel::fit_reach(j, alpha, v, eps, write)
    } else {
        BackoffModel::fit(j, alpha, v, eps)
    }
}
