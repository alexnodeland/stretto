//! The Phase 0 pipeline.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stretto_model::alpha::{alpha_posterior, AlphaPosterior};
use stretto_model::bursts::{runs, summarize, RunSummary};
use stretto_model::policy::write_confirmations;
use stretto_model::provenance::{argument_provenance, Source};
use stretto_model::world::{by_position, coverage_curve, evaluate, PositionStats};
use stretto_model::{steps, BackoffModel, CoveragePoint, EncodedEpisode, EvalStats, Vocab};
use stretto_trace::tau2::{load_manifest, load_results, load_split, Split, Tau2Run};
use stretto_trace::{Episode, ToolKind, ToolManifest};

/// What to measure.
#[derive(Clone, Debug)]
pub struct Config {
    /// A τ²-bench checkout.
    pub tau2_dir: PathBuf,
    /// Domains to analyze, e.g. `retail`, `airline`.
    pub domains: Vec<String>,
    /// Context lengths to compare.
    pub orders: Vec<usize>,
    /// Context length used for coverage and transfer.
    pub order: usize,
    /// Confidence thresholds for the coverage curve.
    pub thresholds: Vec<f64>,
    /// Training observations a context needs before the habit may act on it.
    pub min_evidence: f64,
    /// MH draws for the posterior over α; 0 uses `fixed_alpha`.
    pub alpha_samples: usize,
    /// α used when `alpha_samples` is 0.
    pub fixed_alpha: f64,
    /// Seed for the MH chain.
    pub seed: u64,
    /// How many tool-run signatures to list.
    pub top_runs: usize,
    /// Threshold for the by-position coverage breakdown.
    pub position_threshold: f64,
}

impl Config {
    /// Defaults for a τ²-bench checkout at `tau2_dir`.
    pub fn new(tau2_dir: impl Into<PathBuf>) -> Self {
        Self {
            tau2_dir: tau2_dir.into(),
            domains: vec!["retail".into(), "airline".into()],
            orders: vec![0, 1, 2, 3],
            order: 2,
            thresholds: vec![0.5, 0.7, 0.8, 0.9, 0.95],
            min_evidence: 5.0,
            alpha_samples: 600,
            fixed_alpha: 1.0,
            seed: 7,
            top_runs: 12,
            position_threshold: 0.8,
        }
    }
}

/// The whole report.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// Settings that shaped the numbers.
    pub settings: Settings,
    /// One section per domain.
    pub domains: Vec<DomainReport>,
}

/// Settings echoed into the report.
#[derive(Clone, Debug, Serialize)]
pub struct Settings {
    /// Context lengths compared.
    pub orders: Vec<usize>,
    /// Context length for coverage and transfer.
    pub order: usize,
    /// Coverage thresholds.
    pub thresholds: Vec<f64>,
    /// Evidence required before the habit may act.
    pub min_evidence: f64,
    /// Threshold for the by-position breakdown.
    pub position_threshold: f64,
}

/// One domain.
#[derive(Clone, Debug, Serialize)]
pub struct DomainReport {
    /// Domain name.
    pub domain: String,
    /// Train and test task counts.
    pub train_tasks: usize,
    /// Held-out task count.
    pub test_tasks: usize,
    /// Tools and their kinds.
    pub tools: BTreeMap<String, ToolKind>,
    /// Posterior over α (pooled successful training episodes), if inferred.
    pub alpha: Option<AlphaPosterior>,
    /// α used for every model below.
    pub alpha_used: f64,
    /// One entry per agent model.
    pub models: Vec<ModelReport>,
    /// All models pooled: habit trained on every model's successful training
    /// episodes, tested on every model's held-out episodes.
    pub pooled: PooledReport,
    /// Train on one model, test on another.
    pub transfer: Vec<TransferCell>,
    /// Most common tool-run signatures in successful training episodes.
    pub top_runs: Vec<RunSignature>,
    /// Argument provenance by tool and argument.
    pub provenance: Vec<ProvenanceRow>,
}

/// One agent model in one domain.
#[derive(Clone, Debug, Serialize)]
pub struct ModelReport {
    /// Model name.
    pub model: String,
    /// Episodes (tasks × trials).
    pub episodes: usize,
    /// Share of episodes that solved their task.
    pub success_rate: f64,
    /// LLM calls per episode.
    pub assistant_turns_per_episode: f64,
    /// Tool calls per episode.
    pub tool_calls_per_episode: f64,
    /// Write calls per episode.
    pub writes_per_episode: f64,
    /// Held-out predictability at each context length.
    pub by_order: Vec<(usize, EvalStats)>,
    /// Held-out coverage/accuracy at the configured context length.
    pub coverage: Vec<CoveragePoint>,
    /// Tool runs over all episodes.
    pub runs: RunSummary,
    /// Write calls.
    pub writes: usize,
    /// Writes whose preceding user message contains "yes" (strict proxy).
    pub writes_after_yes: usize,
    /// Writes whose preceding user message contains "yes" or an assent
    /// phrase (lenient proxy).
    pub writes_after_assent: usize,
}

/// All models pooled.
#[derive(Clone, Debug, Serialize)]
pub struct PooledReport {
    /// Held-out predictability at each context length.
    pub by_order: Vec<(usize, EvalStats)>,
    /// Held-out coverage/accuracy at the configured context length.
    pub coverage: Vec<CoveragePoint>,
    /// Tool runs over all episodes.
    pub runs: RunSummary,
    /// Predictability and coverage split by where decisions are made.
    pub positions: Vec<PositionStats>,
}

/// Train on `train`'s successful training episodes, test on `test`'s
/// held-out episodes.
#[derive(Clone, Debug, Serialize)]
pub struct TransferCell {
    /// Model the habit was learned from.
    pub train: String,
    /// Model whose held-out behavior was predicted.
    pub test: String,
    /// Result.
    pub eval: EvalStats,
}

/// A tool-run signature and how often it occurred.
#[derive(Clone, Debug, Serialize)]
pub struct RunSignature {
    /// Tools in order.
    pub tools: Vec<String>,
    /// Occurrences.
    pub count: usize,
}

/// Provenance counts for one tool argument.
#[derive(Clone, Debug, Serialize)]
pub struct ProvenanceRow {
    /// Tool name.
    pub tool: String,
    /// Tool kind, if the manifest lists the tool.
    pub kind: Option<ToolKind>,
    /// Argument name.
    pub arg: String,
    /// Values by source.
    pub counts: BTreeMap<Source, usize>,
    /// All values.
    pub total: usize,
}

/// Run Phase 0 over every configured domain.
pub fn run(config: &Config) -> Result<Report> {
    let domains = config
        .domains
        .iter()
        .map(|d| domain(config, d))
        .collect::<Result<Vec<_>>>()?;
    Ok(Report {
        settings: Settings {
            orders: config.orders.clone(),
            order: config.order,
            thresholds: config.thresholds.clone(),
            min_evidence: config.min_evidence,
            position_threshold: config.position_threshold,
        },
        domains,
    })
}

/// Results files for `domain` under the checkout's published baselines.
pub fn result_files(tau2_dir: &Path, domain: &str) -> Result<Vec<PathBuf>> {
    let dir = tau2_dir.join("data/tau2/results/final");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(".json")
                && (name.contains(&format!("_{domain}_default_"))
                    || name.contains(&format!("_{domain}_base_")))
        })
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("no results files for {domain} in {}", dir.display());
    }
    Ok(files)
}

struct ModelData {
    run: Tau2Run,
    train: Vec<EncodedEpisode>,
    test: Vec<EncodedEpisode>,
}

fn domain(config: &Config, domain: &str) -> Result<DomainReport> {
    let root = &config.tau2_dir;
    let manifest = load_manifest(
        domain,
        &root.join(format!("src/tau2/domains/{domain}/tools.py")),
    )?;
    let split = load_split(&root.join(format!("data/tau2/domains/{domain}/split_tasks.json")))?;

    let mut runs_by_model = Vec::new();
    for path in result_files(root, domain)? {
        let run = load_results(&path)?;
        if run.domain != domain {
            bail!("{} is for {}, not {domain}", path.display(), run.domain);
        }
        runs_by_model.push(run);
    }

    let all_steps: Vec<_> = runs_by_model
        .iter()
        .flat_map(|r| r.episodes.iter().map(steps))
        .collect();
    let vocab = Vocab::build(
        all_steps.iter().flatten(),
        manifest.tools.keys().map(String::as_str),
    );

    let models: Vec<ModelData> = runs_by_model
        .into_iter()
        .map(|run| {
            let (train, test) = encode_split(&run.episodes, &split, &vocab);
            ModelData { run, train, test }
        })
        .collect();

    let successful_train = |m: &ModelData| -> Vec<EncodedEpisode> {
        m.train.iter().filter(|e| e.success).cloned().collect()
    };
    let pooled_train: Vec<EncodedEpisode> = models.iter().flat_map(successful_train).collect();
    let pooled_test: Vec<EncodedEpisode> =
        models.iter().flat_map(|m| m.test.iter().cloned()).collect();

    let alpha = (config.alpha_samples > 0).then(|| {
        alpha_posterior(
            Arc::new(pooled_train.clone()),
            config.order,
            vocab.len(),
            config.alpha_samples,
            config.seed,
        )
    });
    let alpha_used = alpha.as_ref().map_or(config.fixed_alpha, |a| a.median);

    let by_order = |train: &[EncodedEpisode], test: &[EncodedEpisode]| {
        config
            .orders
            .iter()
            .map(|&k| {
                let m = BackoffModel::fit(k, alpha_used, vocab.len(), train);
                (k, evaluate(&m, test))
            })
            .collect::<Vec<_>>()
    };
    let coverage = |train: &[EncodedEpisode], test: &[EncodedEpisode]| {
        let m = BackoffModel::fit(config.order, alpha_used, vocab.len(), train);
        coverage_curve(&m, test, &config.thresholds, config.min_evidence)
    };
    let pooled_habit = BackoffModel::fit(config.order, alpha_used, vocab.len(), &pooled_train);

    let model_reports = models
        .iter()
        .map(|m| {
            let train = successful_train(m);
            model_report(
                m,
                &manifest,
                by_order(&train, &m.test),
                coverage(&train, &m.test),
            )
        })
        .collect();

    let transfer = models
        .iter()
        .flat_map(|a| {
            let habit =
                BackoffModel::fit(config.order, alpha_used, vocab.len(), &successful_train(a));
            models
                .iter()
                .map(move |b| TransferCell {
                    train: a.run.agent_model.clone(),
                    test: b.run.agent_model.clone(),
                    eval: evaluate(&habit, &b.test),
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let all_episodes: Vec<&Episode> = models.iter().flat_map(|m| &m.run.episodes).collect();
    let train_ids: HashSet<&str> = split.train.iter().map(String::as_str).collect();

    let mut signatures: HashMap<Vec<String>, usize> = HashMap::new();
    for ep in all_episodes
        .iter()
        .filter(|e| e.succeeded() && train_ids.contains(e.task_id.as_str()))
    {
        for r in runs(ep) {
            *signatures.entry(r.tools).or_insert(0) += 1;
        }
    }
    let mut top_runs: Vec<RunSignature> = signatures
        .into_iter()
        .map(|(tools, count)| RunSignature { tools, count })
        .collect();
    top_runs.sort_by(|a, b| b.count.cmp(&a.count).then(a.tools.cmp(&b.tools)));
    top_runs.truncate(config.top_runs);

    Ok(DomainReport {
        domain: domain.to_string(),
        train_tasks: split.train.len(),
        test_tasks: split.test.len(),
        tools: manifest.tools.clone(),
        alpha,
        alpha_used,
        models: model_reports,
        pooled: PooledReport {
            by_order: by_order(&pooled_train, &pooled_test),
            coverage: coverage(&pooled_train, &pooled_test),
            runs: summarize(all_episodes.iter().copied()),
            positions: by_position(
                &pooled_habit,
                &pooled_test,
                config.position_threshold,
                config.min_evidence,
            ),
        },
        transfer,
        top_runs,
        provenance: provenance(&all_episodes, &manifest),
    })
}

fn encode_split(
    episodes: &[Episode],
    split: &Split,
    vocab: &Vocab,
) -> (Vec<EncodedEpisode>, Vec<EncodedEpisode>) {
    let train_ids: HashSet<&str> = split.train.iter().map(String::as_str).collect();
    let test_ids: HashSet<&str> = split.test.iter().map(String::as_str).collect();
    let mut train = Vec::new();
    let mut test = Vec::new();
    for ep in episodes {
        let enc = EncodedEpisode::encode(&steps(ep), vocab, ep.succeeded());
        if train_ids.contains(ep.task_id.as_str()) {
            train.push(enc);
        } else if test_ids.contains(ep.task_id.as_str()) {
            test.push(enc);
        }
    }
    (train, test)
}

fn model_report(
    m: &ModelData,
    manifest: &ToolManifest,
    by_order: Vec<(usize, EvalStats)>,
    coverage: Vec<CoveragePoint>,
) -> ModelReport {
    let eps = &m.run.episodes;
    let n = eps.len().max(1) as f64;
    let checks: Vec<_> = eps
        .iter()
        .flat_map(|e| write_confirmations(e, manifest))
        .collect();
    ModelReport {
        model: m.run.agent_model.clone(),
        episodes: eps.len(),
        success_rate: eps.iter().filter(|e| e.succeeded()).count() as f64 / n,
        assistant_turns_per_episode: eps.iter().map(|e| e.assistant_turns()).sum::<usize>() as f64
            / n,
        tool_calls_per_episode: eps.iter().map(|e| e.tool_calls().count()).sum::<usize>() as f64
            / n,
        writes_per_episode: checks.len() as f64 / n,
        by_order,
        coverage,
        runs: summarize(eps),
        writes: checks.len(),
        writes_after_yes: checks.iter().filter(|c| c.yes).count(),
        writes_after_assent: checks.iter().filter(|c| c.assent).count(),
    }
}

fn provenance(episodes: &[&Episode], manifest: &ToolManifest) -> Vec<ProvenanceRow> {
    let mut rows: BTreeMap<(String, String), BTreeMap<Source, usize>> = BTreeMap::new();
    for ep in episodes {
        for u in argument_provenance(ep) {
            *rows
                .entry((u.tool, u.arg))
                .or_default()
                .entry(u.source)
                .or_insert(0) += 1;
        }
    }
    rows.into_iter()
        .map(|((tool, arg), counts)| ProvenanceRow {
            kind: manifest.kind(&tool),
            total: counts.values().sum(),
            tool,
            arg,
            counts,
        })
        .collect()
}
