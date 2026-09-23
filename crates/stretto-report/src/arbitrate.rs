//! Combining the System-One model with the habit (RFC-001 §3.6).
//!
//! Jev's answer at a decision is treated as an observation from a sensor whose
//! reliability depends on the site, not as a verdict. Each option `a` of a
//! decision gets the score
//!
//! ```text
//! s(a) = Σᵢ wᵢ · xᵢ(a) + w_r · [a = Jev's pick] · logit r(site),
//! ```
//!
//! where the `xᵢ` are the caller's features (the log of the habit's posterior
//! predictive, the log of each question design's probability, an indicator
//! for handing back) and `r(site)` is Jev's agreement with the agent at the
//! site on other tasks, shrunk toward its overall agreement: a one-coin
//! Dawid–Skene sensor, with the labels known. `P(a) ∝ exp s(a)` is a
//! conditional logit. Its weights are fitted by maximum likelihood on the
//! other folds of a split by task, so each decision is judged by a model
//! that never saw its task. The flow acts on the top option when `P(top)`,
//! times the site's coverage (how often the agent's step was among the
//! options there), clears the threshold.

use std::collections::HashMap;

/// Pseudo-decisions that pull a site's rates toward the overall ones.
const SHRINK: f64 = 10.0;
/// L2 penalty on the weights.
const RIDGE: f64 = 1.0;

/// One decision, as the arbiter sees it.
#[derive(Clone, Debug)]
pub struct Case {
    /// The task group, for the folds.
    pub group: u64,
    /// The site, for Jev's per-site reliability and coverage.
    pub site: String,
    /// The caller's features for each option.
    pub features: Vec<Vec<f64>>,
    /// The option Jev picked.
    pub pick: usize,
    /// The agent's option, if it was among the options.
    pub actual: Option<usize>,
    /// Whether the case may be fitted on (transfer targets are only
    /// arbitrated).
    pub fit: bool,
}

/// The arbiter's answer to one decision.
#[derive(Clone, Debug, PartialEq)]
pub struct Arbitrated {
    /// The option it would take.
    pub top: usize,
    /// The probability that this is the agent's option.
    pub prob: f64,
    /// The probability of each option being the agent's; the rest is the
    /// chance that the agent's step is not among them.
    pub probs: Vec<f64>,
}

/// Arbitrate every case with a model fitted on the fittable cases of the
/// other `folds - 1` folds (`group % folds`). Also returns the weights,
/// averaged over folds, with the reliability weight last.
pub fn cross_fit(cases: &[Case], folds: u64) -> (Vec<Arbitrated>, Vec<f64>) {
    let dims = cases.first().map_or(0, |c| c.features[0].len()) + 1;
    let mut out = vec![
        Arbitrated {
            top: 0,
            prob: 0.0,
            probs: Vec::new(),
        };
        cases.len()
    ];
    let mut mean = vec![0.0; dims];
    let folds = folds.max(1);
    for fold in 0..folds {
        let train: Vec<&Case> = cases
            .iter()
            .filter(|c| c.fit && c.group % folds != fold)
            .collect();
        let rates = Rates::of(&train);
        let data: Vec<(Vec<Vec<f64>>, usize)> = train
            .iter()
            .filter_map(|c| c.actual.map(|y| (rates.design(c), y)))
            .collect();
        let w = fit(&data, dims);
        for (m, x) in mean.iter_mut().zip(&w) {
            *m += x / folds as f64;
        }
        for (i, c) in cases.iter().enumerate() {
            if c.group % folds != fold {
                continue;
            }
            let cover = rates.coverage(&c.site);
            let probs: Vec<f64> = softmax(&scores(&rates.design(c), &w))
                .into_iter()
                .map(|p| p * cover)
                .collect();
            let top = argmax(&probs);
            out[i] = Arbitrated {
                top,
                prob: probs[top],
                probs,
            };
        }
    }
    (out, mean)
}

/// Jev's agreement and the options' coverage, per site and overall.
struct Rates<'a> {
    sites: HashMap<&'a str, (f64, f64, f64)>,
    agree: f64,
    cover: f64,
}

impl<'a> Rates<'a> {
    fn of(train: &[&'a Case]) -> Self {
        let mut sites: HashMap<&str, (f64, f64, f64)> = HashMap::new();
        let (mut n, mut agreed, mut covered) = (0.0, 0.0, 0.0);
        for c in train {
            let e = sites.entry(c.site.as_str()).or_default();
            let right = (c.actual == Some(c.pick)) as u8 as f64;
            let offered = c.actual.is_some() as u8 as f64;
            e.0 += 1.0;
            e.1 += right;
            e.2 += offered;
            n += 1.0;
            agreed += right;
            covered += offered;
        }
        let rate = |k: f64| if n > 0.0 { k / n } else { 0.5 };
        Rates {
            sites,
            agree: rate(agreed),
            cover: if n > 0.0 { covered / n } else { 1.0 },
        }
    }

    fn reliability(&self, site: &str) -> f64 {
        let (n, agreed, _) = self.sites.get(site).copied().unwrap_or_default();
        (agreed + SHRINK * self.agree) / (n + SHRINK)
    }

    fn coverage(&self, site: &str) -> f64 {
        let (n, _, covered) = self.sites.get(site).copied().unwrap_or_default();
        (covered + SHRINK * self.cover) / (n + SHRINK)
    }

    /// The case's features with the reliability feature appended.
    fn design(&self, c: &Case) -> Vec<Vec<f64>> {
        let r = logit(self.reliability(&c.site));
        c.features
            .iter()
            .enumerate()
            .map(|(a, x)| {
                let mut x = x.clone();
                x.push(if a == c.pick { r } else { 0.0 });
                x
            })
            .collect()
    }
}

/// Maximum penalized likelihood weights of a conditional logit, by Newton's
/// method with step halving. `data` holds each case's option features and
/// the index of the chosen option.
pub fn fit(data: &[(Vec<Vec<f64>>, usize)], dims: usize) -> Vec<f64> {
    let objective = |w: &[f64]| -> f64 {
        let ll: f64 = data
            .iter()
            .map(|(x, y)| {
                let s = scores(x, w);
                s[*y] - log_sum_exp(&s)
            })
            .sum();
        ll - 0.5 * RIDGE * w.iter().map(|v| v * v).sum::<f64>()
    };
    let mut w = vec![0.0; dims];
    let mut current = objective(&w);
    for _ in 0..100 {
        let mut g = vec![0.0; dims];
        let mut h = vec![vec![0.0; dims]; dims];
        for (x, y) in data {
            let p = softmax(&scores(x, &w));
            let mu: Vec<f64> = (0..dims)
                .map(|j| x.iter().zip(&p).map(|(xa, pa)| pa * xa[j]).sum())
                .collect();
            for j in 0..dims {
                g[j] += x[*y][j] - mu[j];
            }
            for (xa, pa) in x.iter().zip(&p) {
                for j in 0..dims {
                    for k in 0..dims {
                        h[j][k] += pa * xa[j] * xa[k];
                    }
                }
            }
            for j in 0..dims {
                for k in 0..dims {
                    h[j][k] -= mu[j] * mu[k];
                }
            }
        }
        for j in 0..dims {
            g[j] -= RIDGE * w[j];
            h[j][j] += RIDGE;
        }
        let Some(d) = solve(h, g) else {
            break;
        };
        let mut step = 1.0;
        let gained = loop {
            let candidate: Vec<f64> = w.iter().zip(&d).map(|(a, b)| a + step * b).collect();
            let value = objective(&candidate);
            if value >= current {
                w = candidate;
                let gained = value - current;
                current = value;
                break gained;
            }
            step /= 2.0;
            if step < 1e-6 {
                break 0.0;
            }
        };
        if gained < 1e-9 {
            break;
        }
    }
    w
}

fn scores(x: &[Vec<f64>], w: &[f64]) -> Vec<f64> {
    x.iter()
        .map(|xa| xa.iter().zip(w).map(|(a, b)| a * b).sum())
        .collect()
}

fn log_sum_exp(s: &[f64]) -> f64 {
    let m = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    m + s.iter().map(|v| (v - m).exp()).sum::<f64>().ln()
}

fn softmax(s: &[f64]) -> Vec<f64> {
    let z = log_sum_exp(s);
    s.iter().map(|v| (v - z).exp()).collect()
}

fn argmax(p: &[f64]) -> usize {
    p.iter()
        .enumerate()
        .fold((0, f64::NEG_INFINITY), |best, (i, &v)| {
            if v > best.1 {
                (i, v)
            } else {
                best
            }
        })
        .0
}

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln()
}

/// Solve `a · x = b` by Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            let (upper, lower) = a.split_at_mut(row);
            for (x, p) in lower[0][col..].iter_mut().zip(&upper[col][col..]) {
                *x -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let rest: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - rest) / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newton_recovers_a_conditional_logit() {
        // Two options; the first is chosen with probability σ(2·x).
        let mut data = Vec::new();
        for i in 0..2000 {
            let x = (i % 21) as f64 / 10.0 - 1.0;
            let p = 1.0 / (1.0 + (-2.0 * x).exp());
            // A deterministic stand-in for sampling: the share chosen at each
            // x matches p.
            let chosen = if ((i / 21) as f64 + 0.5) / (2000.0 / 21.0) < p {
                0
            } else {
                1
            };
            data.push((vec![vec![x], vec![0.0]], chosen));
        }
        let w = fit(&data, 1);
        assert!((w[0] - 2.0).abs() < 0.15, "{w:?}");
    }

    #[test]
    fn a_reliable_site_earns_trust_and_an_unreliable_one_does_not() {
        // Options: respond (0) or a lookup (1). One feature: the log of Jev's
        // probability. At site "good" Jev is always right; at "bad" it picks
        // the lookup but the agent always hands back.
        let case = |group: u64, site: &str, actual: usize| Case {
            group,
            site: site.to_string(),
            features: vec![vec![0.2f64.ln()], vec![0.8f64.ln()]],
            pick: 1,
            actual: Some(actual),
            fit: true,
        };
        let cases: Vec<Case> = (0..200u64)
            .flat_map(|g| [case(g, "good", 1), case(g, "bad", 0)])
            .collect();
        let (out, weights) = cross_fit(&cases, 5);
        assert_eq!(weights.len(), 2);
        let good = &out[0];
        let bad = &out[1];
        assert_eq!(good.top, 1);
        assert!(good.prob > 0.95, "{good:?}");
        assert_eq!(bad.top, 0);
        assert!(bad.prob > 0.95, "{bad:?}");
    }

    #[test]
    fn coverage_discounts_sites_where_the_agent_leaves_the_options() {
        // Jev is right whenever the agent's step is offered, but half the
        // time it is not.
        let cases: Vec<Case> = (0..400u64)
            .map(|g| Case {
                group: g,
                site: "s".to_string(),
                features: vec![vec![0.0], vec![1.0]],
                pick: 1,
                actual: (g % 2 == 0).then_some(1),
                fit: true,
            })
            .collect();
        let (out, _) = cross_fit(&cases, 5);
        assert_eq!(out[0].top, 1);
        assert!(out[0].prob < 0.55 && out[0].prob > 0.4, "{:?}", out[0]);
    }
}
