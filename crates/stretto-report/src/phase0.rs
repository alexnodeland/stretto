//! The Phase 0 pipeline.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stretto_model::alpha::{alpha_posterior, AlphaPosterior};
use stretto_model::bursts::{runs, summarize, RunSummary};
use stretto_model::features::{
    discover, select, step_outputs, FeatureMap, Selected, StepOutput, TrainEpisode, FOLDS,
};
use stretto_model::policy::write_confirmations;
use stretto_model::projection::{
    arg_needs, closed_sets, fit_prices, project, validated_contexts, Gate, Projection,
    ProjectionInput, Scenario,
};
use stretto_model::provenance::{argument_provenance, call_sources, Source};
use stretto_model::world::{
    argmax, by_position, coverage_curve, decode, evaluate, PositionStats, Predictor, Symbol,
};
use stretto_model::{
    step_turns, steps, Action, BackoffModel, CoveragePoint, EncodedEpisode, EvalStats,
    GroupedModel, Outcome, Step, Vocab,
};
use stretto_trace::tau2::{load_manifest, load_results, load_split, Split, Tau2Run};
use stretto_trace::{Episode, ToolKind, ToolManifest, TurnUsage};

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
    /// Also learn code features from tool outputs and report the difference.
    pub features: bool,
    /// Most distinct values a candidate field may take.
    pub feature_max_values: usize,
    /// Minimum log-evidence gain (nats) for a field to be added.
    pub feature_min_gain: f64,
    /// Most fields to select.
    pub feature_max_fields: usize,
    /// Habit confidence thresholds for the macro-tool projection.
    pub projection_thresholds: Vec<f64>,
    /// Held-out decisions a context needs before it can be validated.
    pub validated_min_n: usize,
    /// Held-out top-1 agreement a context needs to be validated.
    pub validated_min_agreement: f64,
    /// Extra τ²-bench results files for agent models the habit never trains
    /// on: measured only as transfer targets. Files for other domains are
    /// skipped.
    pub targets: Vec<PathBuf>,
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
            features: true,
            feature_max_values: 8,
            feature_min_gain: 5.0,
            feature_max_fields: 8,
            projection_thresholds: vec![0.9, 0.95],
            validated_min_n: 20,
            validated_min_agreement: 0.99,
            targets: Vec::new(),
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
    /// Held-out decisions a context needs before it can be validated.
    pub validated_min_n: usize,
    /// Held-out top-1 agreement a context needs to be validated.
    pub validated_min_agreement: f64,
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
    /// The pooled habit again, with code features read from tool outputs.
    pub featured: Option<FeaturedReport>,
}

/// The pooled habit with code features from tool outputs, and again with the
/// episode's intent named up front.
#[derive(Clone, Debug, Serialize)]
pub struct FeaturedReport {
    /// Candidate fields discovered.
    pub candidates: usize,
    /// Fields chosen by grouped cross-validation, in order of selection.
    pub selected: Vec<Selected>,
    /// Distinct intents (sets of write tools an episode calls).
    pub intents: usize,
    /// Habit with the selected code features.
    pub code: VariantStats,
    /// Habit with the code features *and* the episode's intent: what a
    /// macro-tool call supplies, since the LLM names the intent when it calls
    /// the flow.
    pub code_and_intent: VariantStats,
    /// Dollars per million input and output tokens for each agent model,
    /// fitted from the costs the benchmark recorded.
    pub prices: Vec<ModelPrice>,
    /// Contexts where the habit's cross-validated agreement cleared the bar,
    /// most decisions first.
    pub validated: Vec<ValidatedContext>,
    /// Held-out episodes replayed through macro-tool flows driven by the
    /// code-and-intent habit, per scenario and gate.
    pub projection: Vec<Projection>,
    /// The headline configuration (validated contexts, then a perfect
    /// System-One model) for each agent model.
    pub by_model: Vec<ModelProjection>,
}

/// A context where the habit may act.
#[derive(Clone, Debug, Serialize)]
pub struct ValidatedContext {
    /// The steps before the decision, oldest first (`start` before the first).
    pub context: Vec<String>,
    /// The agent's usual next step there.
    pub action: String,
    /// Decisions in this context under cross-validation on training tasks.
    pub cv_n: usize,
    /// Those where the habit matched the agent.
    pub cv_agreed: usize,
    /// Decisions in this context on held-out tasks.
    pub test_n: usize,
    /// Those where the habit matched the agent.
    pub test_agreed: usize,
}

/// The headline projection for one agent model.
#[derive(Clone, Debug, Serialize)]
pub struct ModelProjection {
    /// Agent model.
    pub model: String,
    /// Whether the model is a transfer target the habit never trained on.
    pub target: bool,
    /// Share of the model's tool-calling turns that made several calls.
    pub parallel_share: f64,
    /// Its held-out episodes, replayed.
    pub projection: Projection,
}

/// Fitted token prices for one agent model.
#[derive(Clone, Debug, Serialize)]
pub struct ModelPrice {
    /// Agent model.
    pub model: String,
    /// Dollars per million input tokens.
    pub input_per_mtok: f64,
    /// Dollars per million output tokens.
    pub output_per_mtok: f64,
}

/// Held-out results for one variant of the habit.
#[derive(Clone, Debug, Serialize)]
pub struct VariantStats {
    /// Posterior over α, if inferred.
    pub alpha: Option<AlphaPosterior>,
    /// α used.
    pub alpha_used: f64,
    /// Held-out predictability at each context length.
    pub by_order: Vec<(usize, EvalStats)>,
    /// Held-out coverage/accuracy at the configured context length.
    pub coverage: Vec<CoveragePoint>,
    /// Predictability and coverage by decision position.
    pub positions: Vec<PositionStats>,
}

/// One agent model in one domain.
#[derive(Clone, Debug, Serialize)]
pub struct ModelReport {
    /// Model name.
    pub model: String,
    /// Whether the model is a transfer target: never trained on, except for
    /// its own row of the transfer matrix and its own-habit numbers here.
    pub target: bool,
    /// Model that simulated the user, if recorded.
    pub user_model: Option<String>,
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
            validated_min_n: config.validated_min_n,
            validated_min_agreement: config.validated_min_agreement,
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
    target: bool,
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
        runs_by_model.push((run, false));
    }
    for path in &config.targets {
        let run = load_results(path)?;
        if run.domain == domain {
            runs_by_model.push((run, true));
        }
    }

    let all_steps: Vec<_> = runs_by_model
        .iter()
        .flat_map(|(r, _)| r.episodes.iter().map(steps))
        .collect();
    let vocab = Vocab::build(
        all_steps.iter().flatten(),
        manifest.tools.keys().map(String::as_str),
    );

    let (models, targets): (Vec<ModelData>, Vec<ModelData>) = runs_by_model
        .into_iter()
        .map(|(run, target)| {
            let (train, test) = encode_split(&run.episodes, &split, &vocab);
            ModelData {
                run,
                target,
                train,
                test,
            }
        })
        .partition(|m| !m.target);

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
        .chain(&targets)
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
        .chain(&targets)
        .flat_map(|a| {
            let habit =
                BackoffModel::fit(config.order, alpha_used, vocab.len(), &successful_train(a));
            models
                .iter()
                .chain(&targets)
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

    let target_episodes: Vec<&Episode> = targets.iter().flat_map(|m| &m.run.episodes).collect();
    let featured = config.features.then(|| {
        featured(
            config,
            &all_episodes,
            &target_episodes,
            &split,
            &vocab,
            &manifest,
            alpha_used,
        )
    });

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
        featured,
    })
}

struct Prepared {
    steps: Vec<Step>,
    outputs: Vec<StepOutput>,
    turns: Vec<usize>,
    sources: Vec<Vec<(String, Source, String)>>,
    usage: Vec<TurnUsage>,
    model: String,
    success: bool,
    train: bool,
    target: bool,
    test: bool,
    group: u64,
    intent: String,
}

/// An episode's intent: the sorted set of write tools it calls.
fn intent(ep: &Episode, manifest: &ToolManifest) -> String {
    let mut writes: Vec<&str> = ep
        .tool_calls()
        .filter(|c| manifest.is_write(&c.name))
        .map(|c| c.name.as_str())
        .collect();
    writes.sort();
    writes.dedup();
    if writes.is_empty() {
        "(no write)".to_string()
    } else {
        writes.join("+")
    }
}

fn variant(
    config: &Config,
    vocab: &Vocab,
    train: Vec<EncodedEpisode>,
    test: &[EncodedEpisode],
    alpha_default: f64,
) -> VariantStats {
    let alpha = (config.alpha_samples > 0).then(|| {
        alpha_posterior(
            Arc::new(train.clone()),
            config.order,
            vocab.len(),
            config.alpha_samples,
            config.seed,
        )
    });
    let a = alpha.as_ref().map_or(alpha_default, |p| p.median);
    let habit = BackoffModel::fit(config.order, a, vocab.len(), &train);
    VariantStats {
        alpha,
        alpha_used: a,
        by_order: config
            .orders
            .iter()
            .map(|&k| {
                let m = BackoffModel::fit(k, a, vocab.len(), &train);
                (k, evaluate(&m, test))
            })
            .collect(),
        coverage: coverage_curve(&habit, test, &config.thresholds, config.min_evidence),
        positions: by_position(&habit, test, config.position_threshold, config.min_evidence),
    }
}

fn featured(
    config: &Config,
    episodes: &[&Episode],
    targets: &[&Episode],
    split: &Split,
    vocab: &Vocab,
    manifest: &ToolManifest,
    alpha_used: f64,
) -> FeaturedReport {
    let train_ids: HashSet<&str> = split.train.iter().map(String::as_str).collect();
    let test_ids: HashSet<&str> = split.test.iter().map(String::as_str).collect();
    let prepared: Vec<Prepared> = episodes
        .iter()
        .map(|ep| (ep, false))
        .chain(targets.iter().map(|ep| (ep, true)))
        .map(|(ep, target)| Prepared {
            steps: steps(ep),
            outputs: step_outputs(ep),
            turns: step_turns(ep),
            sources: call_sources(ep),
            usage: ep
                .turn_usage()
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect(),
            model: ep.agent_model.clone(),
            success: ep.succeeded(),
            train: !target && train_ids.contains(ep.task_id.as_str()),
            target,
            test: test_ids.contains(ep.task_id.as_str()),
            group: task_group(&ep.task_id),
            intent: intent(ep, manifest),
        })
        .collect();
    let habit_set: Vec<&Prepared> = prepared.iter().filter(|p| p.train && p.success).collect();
    let test_set: Vec<&Prepared> = prepared.iter().filter(|p| p.test && !p.target).collect();
    let target_set: Vec<&Prepared> = prepared.iter().filter(|p| p.test && p.target).collect();

    let train_outputs: Vec<&[StepOutput]> =
        habit_set.iter().map(|p| p.outputs.as_slice()).collect();
    let candidates = discover(&train_outputs, config.feature_max_values);
    let train_eps: Vec<TrainEpisode> = habit_set
        .iter()
        .map(|p| TrainEpisode {
            steps: &p.steps,
            outputs: &p.outputs,
            group: p.group,
        })
        .collect();
    let selected = select(
        &train_eps,
        &candidates,
        vocab,
        config.order,
        alpha_used,
        config.feature_min_gain,
        config.feature_max_fields,
    );
    let fields: Vec<_> = selected
        .iter()
        .map(|s| (s.tool.clone(), s.field.clone()))
        .collect();
    let map = FeatureMap::fit(&fields, &train_outputs);

    let mut intent_ids: BTreeMap<&str, u32> = BTreeMap::new();
    for p in &prepared {
        let next = intent_ids.len() as u32 + 1;
        intent_ids.entry(p.intent.as_str()).or_insert(next);
    }
    let encode = |p: &&Prepared| {
        let f = map.features(&p.outputs);
        EncodedEpisode::encode_with_features(&p.steps, Some(&f), vocab, p.success)
            .with_group(intent_ids[p.intent.as_str()])
    };
    let train: Vec<EncodedEpisode> = habit_set.iter().map(encode).collect();
    let test: Vec<EncodedEpisode> = test_set.iter().map(encode).collect();
    let target_test: Vec<EncodedEpisode> = target_set.iter().map(encode).collect();

    let code = variant(config, vocab, train.clone(), &test, alpha_used);
    let a = code.alpha_used;
    let intent_habit = GroupedModel::fit(config.order, a, a, vocab.len(), &train);
    let code_and_intent = VariantStats {
        alpha: None,
        alpha_used: a,
        by_order: config
            .orders
            .iter()
            .map(|&k| {
                let m = GroupedModel::fit(k, a, a, vocab.len(), &train);
                (k, evaluate(&m, &test))
            })
            .collect(),
        coverage: coverage_curve(
            &intent_habit,
            &test,
            &config.thresholds,
            config.min_evidence,
        ),
        positions: by_position(
            &intent_habit,
            &test,
            config.position_threshold,
            config.min_evidence,
        ),
    };
    // Replay held-out episodes through macro-tool flows driven by this habit.
    let closed = closed_sets(
        habit_set.iter().flat_map(|p| {
            p.steps
                .iter()
                .filter_map(|s| match &s.action {
                    Action::Tool(t) => Some(t.clone()),
                    Action::Respond => None,
                })
                .zip(&p.sources)
                .flat_map(|(tool, leaves)| {
                    leaves
                        .iter()
                        .map(move |(arg, _, value)| (tool.clone(), arg.clone(), value.clone()))
                })
                .collect::<Vec<_>>()
        }),
        8,
        10,
    );
    let mut by_model: BTreeMap<&str, Vec<TurnUsage>> = BTreeMap::new();
    for p in &prepared {
        by_model
            .entry(p.model.as_str())
            .or_default()
            .extend(p.usage.iter().copied());
    }
    let prices: BTreeMap<&str, (f64, f64)> = by_model
        .into_iter()
        .map(|(m, turns)| (m, fit_prices(turns)))
        .collect();
    let inputs: Vec<ProjectionInput> = test_set
        .iter()
        .zip(&test)
        .map(|(p, enc)| projection_input(p, enc, &closed, &prices))
        .collect();
    let grouped: Vec<(u64, EncodedEpisode)> = habit_set
        .iter()
        .map(|p| p.group)
        .zip(train.iter().cloned())
        .collect();
    let validated = validated_contexts(
        &grouped,
        config.order,
        a,
        vocab.len(),
        FOLDS,
        config.validated_min_n,
        config.validated_min_agreement,
    );
    let gates: Vec<Gate> = config
        .projection_thresholds
        .iter()
        .map(|&t| Gate::Threshold(t))
        .chain([Gate::Validated(&validated)])
        .collect();
    let projection = [Scenario::HabitOnly, Scenario::HabitThenPerfectOracle]
        .into_iter()
        .flat_map(|scenario| gates.iter().map(move |&gate| (scenario, gate)))
        .map(|(scenario, gate)| {
            project(&intent_habit, &inputs, scenario, gate, config.min_evidence)
        })
        .collect();
    let by_model = prices
        .keys()
        .map(|&m| {
            let inputs: Vec<ProjectionInput> = test_set
                .iter()
                .zip(&test)
                .chain(target_set.iter().zip(&target_test))
                .filter(|(p, _)| p.model == m)
                .map(|(p, enc)| projection_input(p, enc, &closed, &prices))
                .collect();
            ModelProjection {
                model: m.to_string(),
                target: prepared.iter().any(|p| p.model == m && p.target),
                parallel_share: parallel_share(prepared.iter().filter(|p| p.model == m)),
                projection: project(
                    &intent_habit,
                    &inputs,
                    Scenario::HabitThenPerfectOracle,
                    Gate::Validated(&validated),
                    config.min_evidence,
                ),
            }
        })
        .collect();

    // How the validated contexts fared on held-out tasks.
    let mut held_out: HashMap<Vec<Symbol>, (usize, usize)> = HashMap::new();
    for enc in &test {
        for t in 0..enc.actions.len() {
            if let Some(c) = validated.context_at(enc, t) {
                let top = argmax(&intent_habit.predict_at(enc, t));
                let e = held_out.entry(c).or_default();
                e.0 += 1;
                e.1 += (top == enc.actions[t] as usize) as usize;
            }
        }
    }
    let describe = |symbol: Symbol| match decode(symbol) {
        None => "start".to_string(),
        Some((a, o, f)) => {
            let action = vocab.action(a);
            let outcome = match Outcome::from_index(o) {
                Some(Outcome::Ok) => "ok",
                Some(Outcome::Err) => "error",
                Some(Outcome::Reply) => "user replied",
                Some(Outcome::End) => "ended",
                None => "?",
            };
            let feature = match action {
                Some(Action::Tool(t)) if f > 0 => map
                    .describe(t, f)
                    .map(|d| format!("; {d}"))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let name = action.map_or_else(|| "?".to_string(), Action::to_string);
            format!("{name} ({outcome}{feature})")
        }
    };
    let validated_list = validated
        .records()
        .into_iter()
        .map(|(context, r)| {
            let (test_n, test_agreed) = held_out.get(context).copied().unwrap_or_default();
            ValidatedContext {
                context: context.iter().map(|&s| describe(s)).collect(),
                action: vocab
                    .action(r.action)
                    .map_or_else(|| "?".to_string(), Action::to_string),
                cv_n: r.n,
                cv_agreed: r.agreed,
                test_n,
                test_agreed,
            }
        })
        .collect();

    FeaturedReport {
        candidates: candidates.len(),
        selected,
        intents: prepared
            .iter()
            .filter(|p| !p.target)
            .map(|p| p.intent.as_str())
            .collect::<HashSet<_>>()
            .len(),
        code,
        code_and_intent,
        prices: prices
            .iter()
            .map(|(m, (i, o))| ModelPrice {
                model: m.to_string(),
                input_per_mtok: i * 1e6,
                output_per_mtok: o * 1e6,
            })
            .collect(),
        validated: validated_list,
        projection,
        by_model,
    }
}

fn projection_input<'a>(
    p: &Prepared,
    encoded: &'a EncodedEpisode,
    closed: &HashSet<(String, String)>,
    prices: &BTreeMap<&str, (f64, f64)>,
) -> ProjectionInput<'a> {
    ProjectionInput {
        encoded,
        turns: p.turns.clone(),
        args: arg_needs(&p.steps, &p.sources, closed),
        usage: p.usage.clone(),
        input_price: prices[p.model.as_str()].0,
    }
}

/// Share of tool-calling turns that made several calls at once.
fn parallel_share<'a>(episodes: impl Iterator<Item = &'a Prepared>) -> f64 {
    let (mut tool_turns, mut parallel) = (0, 0);
    for p in episodes {
        let mut calls: BTreeMap<usize, usize> = BTreeMap::new();
        for (step, &turn) in p.steps.iter().zip(&p.turns) {
            if matches!(step.action, Action::Tool(_)) {
                *calls.entry(turn).or_insert(0) += 1;
            }
        }
        tool_turns += calls.len();
        parallel += calls.values().filter(|&&n| n > 1).count();
    }
    parallel as f64 / tool_turns.max(1) as f64
}

/// A stable group id for a task, so all of its episodes share a fold.
fn task_group(task_id: &str) -> u64 {
    task_id.bytes().fold(1469598103934665603u64, |h, b| {
        (h ^ b as u64).wrapping_mul(1099511628211)
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
        target: m.target,
        user_model: m.run.user_model.clone(),
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
