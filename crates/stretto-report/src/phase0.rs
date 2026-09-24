//! The Phase 0 pipeline.

use crate::arbitrate::{self, Case, Fitted};
use crate::flow::{Bindings, Flow};
use crate::shadow::{
    self, Agreement, Decision, Favors, Kind, Predicate, QuestionSet, Scored, ShadowConfig,
    ShadowEpisode, Sites, RESPOND,
};
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
    arg_needs, closed_sets, fit_prices, project, project_design, validated_contexts, ArgNeed,
    Design, Gate, OracleArgs, OracleStep, Projection, ProjectionInput, Scenario,
};
use stretto_model::provenance::{argument_provenance, call_sources, Source};
use stretto_model::world::{
    argmax, by_position, coverage_curve, decode, evaluate, PositionStats, Predictor, Symbol,
};
use stretto_model::{
    step_turns, steps, Action, BackoffModel, CoveragePoint, EncodedEpisode, EvalStats,
    GroupedModel, Outcome, Step, Vocab,
};
use stretto_oracle::Oracle;
use stretto_trace::tau2::{load_manifest, load_results, load_split, Split, Tau2Run};
use stretto_trace::{Episode, Event, ToolKind, ToolManifest, TurnUsage};

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
    /// Distinct training tasks those decisions must come from.
    pub validated_min_tasks: usize,
    /// Held-out top-1 agreement a context needs to be validated.
    pub validated_min_agreement: f64,
    /// Whether τ²-bench's own published baselines (in the checkout) are
    /// training sources.
    pub baselines: bool,
    /// Whether flows know the episode's goal (the writes it goes on to make,
    /// as the LLM would name them in a macro-tool call): the habit is
    /// conditioned on it and System-One questions name it. Off for flows
    /// nobody names, such as live flows that continue an agent's lookups.
    pub intent: bool,
    /// Also keep what a live flow needs (see [`compile_flow`]).
    pub flow: bool,
    /// Extra τ²-bench results files to train on, like the baselines. Files
    /// for other domains are skipped.
    pub sources: Vec<Target>,
    /// Extra τ²-bench results files for agent models the habit never trains
    /// on: measured only as transfer targets. Files for other domains are
    /// skipped.
    pub targets: Vec<Target>,
    /// Phase 0b: ask a System-One model at every held-out decision a flow
    /// would hand it.
    pub shadow: Option<ShadowConfig>,
}

/// A results file for a transfer target, optionally relabeled.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// Name to report the agent model under (default: the name the file
    /// records). Needed when two runs record the same model, or one model
    /// under different names.
    pub label: Option<String>,
    /// The τ²-bench results file.
    pub path: PathBuf,
}

impl Target {
    /// Parse `label=path` or a bare `path`.
    pub fn parse(arg: &str) -> Self {
        match arg.split_once('=') {
            Some((label, path)) if !label.is_empty() && !label.contains('/') => Target {
                label: Some(label.to_string()),
                path: path.into(),
            },
            _ => Target {
                label: None,
                path: arg.into(),
            },
        }
    }
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
            validated_min_tasks: 10,
            validated_min_agreement: 0.99,
            baselines: true,
            intent: true,
            flow: false,
            sources: Vec::new(),
            targets: Vec::new(),
            shadow: None,
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
    /// Distinct training tasks those decisions must come from.
    pub validated_min_tasks: usize,
    /// Held-out top-1 agreement a context needs to be validated.
    pub validated_min_agreement: f64,
    /// Whether τ²-bench's published baselines were training sources.
    pub baselines: bool,
    /// Labels (or paths) of extra training sources.
    pub sources: Vec<String>,
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
    /// Tokens of a typical lookup's output (characters over four), charged
    /// to every later prompt for each detour of a read-only flow.
    pub lookup_tokens: f64,
    /// Contexts where the habit's cross-validated agreement cleared the bar,
    /// most decisions first.
    pub validated: Vec<ValidatedContext>,
    /// Held-out episodes replayed through macro-tool flows driven by the
    /// code-and-intent habit, per scenario and gate.
    pub projection: Vec<Projection>,
    /// The headline configuration (validated contexts, then a perfect
    /// System-One model) for each agent model.
    pub by_model: Vec<ModelProjection>,
    /// Phase 0b: the System-One model's answers, scored.
    pub shadow: Option<ShadowReport>,
}

/// Phase 0b results for one domain.
#[derive(Clone, Debug, Serialize)]
pub struct ShadowReport {
    /// Which oracle answered (`jev`, `replay` or `mock`).
    pub oracle: String,
    /// Model versions that answered.
    pub versions: Vec<String>,
    /// Decisions asked about (including arguments answered without asking).
    pub decisions: usize,
    /// Distinct requests among them.
    pub distinct: usize,
    /// Requests that got an answer this run (from the service or the cache).
    pub answered: usize,
    /// Requests that failed.
    pub errors: usize,
    /// The first failure, if any.
    pub first_error: Option<String>,
    /// Input tokens the service reported for the answers used.
    pub input_tokens: u64,
    /// Agreement per agent model, then pooled over the source models.
    pub rows: Vec<ShadowRow>,
    /// Closed-set argument agreement per `(tool, argument)`, pooled over the
    /// source models.
    pub by_arg: Vec<ArgRow>,
    /// Probabilities at which the System-One pick was trusted in the
    /// projections.
    pub thresholds: Vec<f64>,
    /// Pooled projection over the source models, validated habit then the
    /// System-One model, one per threshold.
    pub projection: Vec<Projection>,
    /// Which questions were asked.
    pub questions: QuestionSet,
    /// v2: next-step decisions settled without asking, at sites where the
    /// agent never looked anything up in training (the flow hands back).
    pub structural: usize,
    /// v2: share of the source models' next-step decisions whose step (as a
    /// read-only flow takes it) was among the options.
    pub offered: f64,
    /// v2: the arbiter's weights, averaged over folds, by feature: the
    /// habit, the one-question answer, the split answer, handing back, each
    /// predicate, and the site's reliability.
    pub weights: Vec<(String, f64)>,
    /// v2: the predicates asked alongside the next-step questions.
    pub predicates: Vec<Predicate>,
}

/// Phase 0b agreement for one closed-set argument.
#[derive(Clone, Debug, Serialize)]
pub struct ArgRow {
    /// Tool.
    pub tool: String,
    /// Argument.
    pub arg: String,
    /// Agreement on it.
    pub agreement: Agreement,
}

/// Phase 0b agreement for one agent model.
#[derive(Clone, Debug, Serialize)]
pub struct ShadowRow {
    /// Agent model (or `all source models`).
    pub model: String,
    /// Whether it is a transfer target.
    pub target: bool,
    /// Next-step questions.
    pub next: Agreement,
    /// Closed-set argument questions.
    pub args: Agreement,
    /// v2: next-step answers from the stop and lookup questions.
    pub split: Option<Agreement>,
    /// v2: next-step answers combined with the habit.
    pub combined: Option<Agreement>,
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
    /// Distinct training tasks they came from.
    pub cv_tasks: usize,
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
    /// The same with read-only flows (plan/commit: writes go back to the
    /// LLM).
    pub read_only: Projection,
    /// The same with the real System-One model instead of a perfect one, one
    /// per Phase 0b threshold (empty without Phase 0b).
    pub with_oracle: Vec<Projection>,
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
    let oracle = config
        .shadow
        .as_ref()
        .map(ShadowConfig::build)
        .transpose()?;
    let domains = config
        .domains
        .iter()
        .map(|d| domain(config, d, oracle.as_deref()).map(|(report, _)| report))
        .collect::<Result<Vec<_>>>()?;
    Ok(Report {
        settings: Settings {
            orders: config.orders.clone(),
            order: config.order,
            thresholds: config.thresholds.clone(),
            min_evidence: config.min_evidence,
            position_threshold: config.position_threshold,
            validated_min_n: config.validated_min_n,
            validated_min_tasks: config.validated_min_tasks,
            validated_min_agreement: config.validated_min_agreement,
            baselines: config.baselines,
            sources: config
                .sources
                .iter()
                .map(|t| {
                    t.label
                        .clone()
                        .unwrap_or_else(|| t.path.display().to_string())
                })
                .collect(),
        },
        domains,
    })
}

/// Compile the live read-only flow for `domain` (RFC-001 §3.13): Phase 0b
/// v2 as `config` sets it up, goal free, keeping what a flow needs to act.
/// `oracle` answers the held-out questions the arbiter is fitted on.
pub fn compile_flow(
    config: &Config,
    domain_name: &str,
    oracle: &(dyn Oracle + Sync),
) -> Result<Flow> {
    let mut config = config.clone();
    config.intent = false;
    config.flow = true;
    config.features = true;
    let (_, flow) = domain(&config, domain_name, Some(oracle))?;
    flow.context("a live flow needs the v2 questions")
}

/// Compile a live flow from `episodes` of one domain rather than from
/// τ²-bench results: for example sessions recorded by `stretto-proxy` (see
/// [`stretto_trace::mcp`]). The pipeline is [`compile_flow`]'s, with each
/// episode its own task and a split by episode id standing in for
/// τ²-bench's: 70% train the habit, the sites and the bindings (the
/// successful ones, reward 1), and `oracle` is asked at every decision of
/// the other 30%, where the arbiter is fitted. An episode's task id is kept
/// when it has one, so episodes of one task share a side of the split.
pub fn compile_flow_from_episodes(
    config: &Config,
    episodes: &[Episode],
    manifest: &ToolManifest,
    oracle: &(dyn Oracle + Sync),
) -> Result<Flow> {
    let mut config = config.clone();
    config.intent = false;
    config.flow = true;
    config.features = true;
    let episodes: Vec<Episode> = episodes
        .iter()
        .cloned()
        .map(|mut e| {
            if e.task_id.is_empty() {
                e.task_id = e.id.clone();
            }
            e
        })
        .collect();
    let mut split = Split {
        train: Vec::new(),
        test: Vec::new(),
    };
    for ep in &episodes {
        let side = if task_group(&ep.task_id) % 10 < 7 {
            &mut split.train
        } else {
            &mut split.test
        };
        if !side.contains(&ep.task_id) {
            side.push(ep.task_id.clone());
        }
    }
    if split.train.is_empty() || split.test.is_empty() {
        bail!(
            "{} episodes are too few to split into training and held-out tasks",
            episodes.len()
        );
    }
    let all_steps: Vec<Vec<Step>> = episodes.iter().map(steps).collect();
    let vocab = Vocab::build(
        all_steps.iter().flatten(),
        manifest.tools.keys().map(String::as_str),
    );
    let (train, _) = encode_split(&episodes, &split, &vocab);
    let successful: Vec<EncodedEpisode> = train.into_iter().filter(|e| e.success).collect();
    if successful.is_empty() {
        bail!("no successful training episodes to learn a habit from (set rewards to 1)");
    }
    let alpha_used = if config.alpha_samples > 0 {
        alpha_posterior(
            Arc::new(successful),
            config.order,
            vocab.len(),
            config.alpha_samples,
            config.seed,
        )
        .median
    } else {
        config.fixed_alpha
    };
    let refs: Vec<&Episode> = episodes.iter().collect();
    let (_, flow) = featured(
        &config,
        &refs,
        &[],
        &split,
        &vocab,
        manifest,
        alpha_used,
        Some(oracle),
    )?;
    flow.context("a live flow needs the v2 questions")
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

fn domain(
    config: &Config,
    domain: &str,
    oracle: Option<&(dyn Oracle + Sync)>,
) -> Result<(DomainReport, Option<Flow>)> {
    let root = &config.tau2_dir;
    let manifest = load_manifest(
        domain,
        &root.join(format!("src/tau2/domains/{domain}/tools.py")),
    )?;
    let split = load_split(&root.join(format!("data/tau2/domains/{domain}/split_tasks.json")))?;

    let mut runs_by_model = Vec::new();
    if config.baselines {
        for path in result_files(root, domain)? {
            let run = load_results(&path)?;
            if run.domain != domain {
                bail!("{} is for {}, not {domain}", path.display(), run.domain);
            }
            runs_by_model.push((run, false));
        }
    }
    let extra = config
        .sources
        .iter()
        .map(|t| (t, false))
        .chain(config.targets.iter().map(|t| (t, true)));
    for (file, target) in extra {
        let mut run = load_results(&file.path)?;
        if run.domain != domain {
            continue;
        }
        if let Some(label) = &file.label {
            run.agent_model = label.clone();
            for ep in &mut run.episodes {
                ep.agent_model = label.clone();
            }
        }
        runs_by_model.push((run, target));
    }
    if !runs_by_model.iter().any(|(_, target)| !target) {
        bail!("no training sources for {domain}: keep the baselines or pass --source");
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
    let (featured, flow) = if config.features {
        let (report, flow) = featured(
            config,
            &all_episodes,
            &target_episodes,
            &split,
            &vocab,
            &manifest,
            alpha_used,
            oracle,
        )?;
        (Some(report), flow)
    } else {
        (None, None)
    };

    let report = DomainReport {
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
    };
    Ok((report, flow))
}

struct Prepared<'a> {
    ep: &'a Episode,
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

#[allow(clippy::too_many_arguments)]
fn featured(
    config: &Config,
    episodes: &[&Episode],
    targets: &[&Episode],
    split: &Split,
    vocab: &Vocab,
    manifest: &ToolManifest,
    alpha_used: f64,
    oracle: Option<&(dyn Oracle + Sync)>,
) -> Result<(FeaturedReport, Option<Flow>)> {
    let train_ids: HashSet<&str> = split.train.iter().map(String::as_str).collect();
    let test_ids: HashSet<&str> = split.test.iter().map(String::as_str).collect();
    let prepared: Vec<Prepared> = episodes
        .iter()
        .map(|ep| (ep, false))
        .chain(targets.iter().map(|ep| (ep, true)))
        .map(|(ep, target)| Prepared {
            ep,
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
            intent: if config.intent {
                intent(ep, manifest)
            } else {
                String::new()
            },
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
        config.validated_min_tasks,
        config.validated_min_agreement,
    );

    // Action ids a read-only flow hands back: writes, and tools not known to
    // be read-only (the unseen-tool id included).
    let high: HashSet<u32> = (0..vocab.len() as u32)
        .filter(|&id| match vocab.action(id) {
            Some(Action::Tool(t)) => manifest.tools.get(t) != Some(&ToolKind::Read),
            Some(Action::Respond) => false,
            None => true,
        })
        .collect();

    let (lookups, chars) = prepared
        .iter()
        .flat_map(|p| p.ep.events.iter())
        .filter_map(|e| match e {
            Event::ToolResult { name, content, .. }
                if manifest.tools.get(name) == Some(&ToolKind::Read) =>
            {
                Some(content.len())
            }
            _ => None,
        })
        .fold((0usize, 0usize), |(n, c), len| (n + 1, c + len));
    let lookup_tokens = if lookups > 0 {
        chars as f64 / lookups as f64 / 4.0
    } else {
        0.0
    };
    let read_only = Design::ReadOnly {
        high: &high,
        detour_tokens: lookup_tokens,
    };

    // Every held-out episode, source models first, as the projection and
    // Phase 0b replay them.
    let replayed: Vec<(&Prepared, &EncodedEpisode)> = test_set
        .iter()
        .copied()
        .zip(&test)
        .chain(target_set.iter().copied().zip(&target_test))
        .collect();
    let needs: Vec<Vec<ArgNeed>> = replayed
        .iter()
        .map(|(p, _)| arg_needs(&p.steps, &p.sources, &closed))
        .collect();
    let shadow = match (&config.shadow, oracle) {
        (Some(sc), Some(oracle)) => Some(shadow_run(
            sc,
            oracle,
            &replayed,
            &needs,
            manifest,
            vocab,
            &habit_set,
            &intent_habit,
        )?),
        _ => None,
    };
    // A live flow, from the same pieces.
    let flow = match (&shadow, &config.shadow) {
        (Some(run), Some(sc)) if config.flow && sc.questions == QuestionSet::V2 => {
            let Some(&group) = intent_ids.get("") else {
                bail!("a live flow is goal free: compile it without intents");
            };
            let mut sources: Vec<String> = prepared
                .iter()
                .filter(|p| !p.target)
                .map(|p| p.model.clone())
                .collect();
            sources.sort();
            sources.dedup();
            Some(Flow {
                stretto_flow: crate::flow::FLOW_VERSION,
                provenance: crate::flow::Provenance {
                    stretto: env!("CARGO_PKG_VERSION").to_string(),
                    sources,
                    habit_episodes: habit_set.len(),
                    arbiter_cases: run.cases,
                    compiled_unix_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as u64),
                },
                vocab: vocab.clone(),
                manifest: manifest.clone(),
                map: map.clone(),
                group,
                habit: intent_habit.clone(),
                sites: run.sites.clone(),
                predicates: sc.predicates.clone(),
                weighed: if sc.predicate_features {
                    sc.predicates.clone()
                } else {
                    Vec::new()
                },
                folds: run.folds.clone(),
                bindings: Bindings::learn(habit_set.iter().map(|p| p.ep), manifest),
                model: sc.model.clone(),
            })
        }
        _ => None,
    };
    let input = |i: usize| {
        let (p, enc) = replayed[i];
        let answers = shadow.as_ref().map(|run| &run.raw[i]);
        projection_input(p, enc, &needs[i], &prices, answers)
    };
    let combined_input = |i: usize| {
        let (p, enc) = replayed[i];
        let answers = shadow
            .as_ref()
            .and_then(|run| run.combined.as_ref())
            .map(|c| &c[i]);
        projection_input(p, enc, &needs[i], &prices, answers)
    };
    let lookup_first_input = |i: usize| {
        let (p, enc) = replayed[i];
        let answers = shadow
            .as_ref()
            .and_then(|run| run.lookup_first.as_ref())
            .map(|c| &c[i]);
        projection_input(p, enc, &needs[i], &prices, answers)
    };
    let inputs: Vec<ProjectionInput> = (0..test_set.len()).map(input).collect();
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
    let thresholds: Vec<f64> = config
        .shadow
        .as_ref()
        .filter(|_| shadow.is_some())
        .map(|sc| sc.thresholds.clone())
        .unwrap_or_default();
    // v2 asks for read-only flows (plan/commit); v1 let flows call any tool.
    let design = match config.shadow.as_ref().map(|sc| sc.questions) {
        Some(QuestionSet::V2) => read_only,
        _ => Design::AnyTool,
    };
    let with_oracle = |idx: &[usize]| -> Vec<Projection> {
        let raw: Vec<ProjectionInput> = idx.iter().map(|&i| input(i)).collect();
        let replay = |inputs: &[ProjectionInput], scenario| {
            project_design(
                &intent_habit,
                inputs,
                scenario,
                Gate::Validated(&validated),
                config.min_evidence,
                design,
            )
        };
        let mut out: Vec<Projection> = thresholds
            .iter()
            .map(|&t| Scenario::HabitThenOracle(t))
            .chain(thresholds.iter().map(|&t| Scenario::TwoKeys(t)))
            .map(|scenario| replay(&raw, scenario))
            .collect();
        if shadow.as_ref().is_some_and(|run| run.combined.is_some()) {
            let combined: Vec<ProjectionInput> = idx.iter().map(|&i| combined_input(i)).collect();
            out.extend(
                thresholds
                    .iter()
                    .map(|&t| replay(&combined, Scenario::Arbitrated(t))),
            );
            // Below one half, only a lookup-first answer can continue.
            let first: Vec<ProjectionInput> = idx.iter().map(|&i| lookup_first_input(i)).collect();
            out.extend(
                LOOKUP_FIRST
                    .iter()
                    .map(|&t| replay(&first, Scenario::LookupFirst(t))),
            );
        }
        out
    };
    let mut models: Vec<(bool, &str)> = prices
        .keys()
        .map(|&m| (prepared.iter().any(|p| p.model == m && p.target), m))
        .collect();
    models.sort();
    let by_model = models
        .into_iter()
        .map(|(target, m)| {
            let idx: Vec<usize> = (0..replayed.len())
                .filter(|&i| replayed[i].0.model == m)
                .collect();
            let inputs: Vec<ProjectionInput> = idx.iter().map(|&i| input(i)).collect();
            ModelProjection {
                model: m.to_string(),
                target,
                parallel_share: parallel_share(prepared.iter().filter(|p| p.model == m)),
                projection: project(
                    &intent_habit,
                    &inputs,
                    Scenario::HabitThenPerfectOracle,
                    Gate::Validated(&validated),
                    config.min_evidence,
                ),
                read_only: project_design(
                    &intent_habit,
                    &inputs,
                    Scenario::HabitThenPerfectOracle,
                    Gate::Validated(&validated),
                    config.min_evidence,
                    read_only,
                ),
                with_oracle: with_oracle(&idx),
            }
        })
        .collect();
    let sources: Vec<usize> = (0..test_set.len()).collect();
    let pooled = with_oracle(&sources);
    let shadow = shadow.map(|run| {
        let mut report = run.report;
        report.projection = pooled;
        report
    });

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
                cv_tasks: r.tasks,
                cv_agreed: r.agreed,
                test_n,
                test_agreed,
            }
        })
        .collect();

    let report = FeaturedReport {
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
        lookup_tokens,
        projection,
        by_model,
        shadow,
    };
    Ok((report, flow))
}

/// Phase 0b's answers for one held-out episode, laid out per step for the
/// projection.
type Answers = (Vec<Option<OracleStep>>, Vec<Option<OracleArgs>>);

/// What a Phase 0b run hands back to the projection.
struct ShadowRun {
    report: ShadowReport,
    /// The System-One model's own answers, per held-out episode.
    raw: Vec<Answers>,
    /// v2: the answers combined with the habit.
    combined: Option<Vec<Answers>>,
    /// v2: the most likely lookup of each combined answer.
    lookup_first: Option<Vec<Answers>>,
    /// The sites the questions were asked at.
    sites: Sites,
    /// v2: the arbiter fitted for each fold (empty for v1).
    folds: Vec<Fitted>,
    /// v2: the held-out decisions the arbiter was fitted on.
    cases: usize,
}

/// Probabilities at which a read-only flow takes its most likely lookup.
const LOOKUP_FIRST: [f64; 3] = [0.2, 0.3, 0.4];

/// Ask the System-One model at every decision of `replayed`, score the
/// answers (and, for v2, combine them with the habit), and lay them out per
/// step for the projection.
#[allow(clippy::too_many_arguments)]
fn shadow_run(
    sc: &ShadowConfig,
    oracle: &(dyn Oracle + Sync),
    replayed: &[(&Prepared, &EncodedEpisode)],
    needs: &[Vec<ArgNeed>],
    manifest: &ToolManifest,
    vocab: &Vocab,
    habit_set: &[&Prepared],
    habit: &GroupedModel,
) -> Result<ShadowRun> {
    let closed = shadow::closed_values(habit_set.iter().map(|p| p.ep));
    let episodes: Vec<ShadowEpisode> = replayed
        .iter()
        .zip(needs)
        .map(|((p, _), needs)| ShadowEpisode {
            episode: p.ep,
            steps: &p.steps,
            sources: &p.sources,
            needs,
            goal: &p.intent,
        })
        .collect();
    let v2 = sc.questions == QuestionSet::V2;
    let mut sites = Sites::learn(habit_set.iter().map(|p| p.steps.as_slice()), manifest);
    if sc.hints {
        sites.learn_feeds(habit_set.iter().map(|p| p.ep), manifest);
    }
    let decisions = if v2 {
        shadow::decisions_v2(
            &episodes,
            manifest,
            &closed,
            &sites,
            &sc.predicates,
            &sc.model,
        )
    } else {
        shadow::decisions(&episodes, manifest, &closed, &sc.model)
    };
    let asked = shadow::ask(oracle, &decisions, sc, &manifest.domain)?;
    let scored: Vec<Option<Scored>> = decisions.iter().map(|d| shadow::score(d, &asked)).collect();
    let split: Vec<Option<Scored>> = if v2 {
        decisions
            .iter()
            .map(|d| shadow::score_split(d, &asked))
            .collect()
    } else {
        vec![None; decisions.len()]
    };
    let predicates: Vec<BTreeMap<String, f64>> = decisions
        .iter()
        .map(|d| shadow::predicate_answers(d, &asked))
        .collect();
    let (combined, weights, folds, cases) = if v2 {
        let (c, w, f, n) = combine(
            &decisions,
            &scored,
            &split,
            &predicates,
            if sc.predicate_features {
                &sc.predicates
            } else {
                &[]
            },
            replayed,
            habit,
            vocab,
        );
        (Some(c), w, f, n)
    } else {
        (None, Vec::new(), Vec::new(), 0)
    };
    // The most likely lookup, wherever there is one.
    let lookup_first: Option<Vec<Option<Scored>>> = combined.as_ref().map(|c| {
        c.iter()
            .zip(&decisions)
            .map(|(s, d)| {
                let s = s.as_ref()?;
                if d.kind != Kind::Next || d.request.is_none() {
                    return Some(s.clone());
                }
                let best = s.probs.iter().filter(|(o, _)| o.as_str() != RESPOND).fold(
                    None::<(&String, f64)>,
                    |best, (o, &p)| match best {
                        Some((_, q)) if q >= p => best,
                        _ => Some((o, p)),
                    },
                );
                Some(match best {
                    Some((o, _)) => Scored::of(s.probs.clone(), o.clone(), &d.actual),
                    None => s.clone(),
                })
            })
            .collect()
    });
    if let Some(path) = &sc.log {
        let path = shadow::per_domain(path, &manifest.domain);
        let mut lines = String::new();
        for (i, d) in decisions.iter().enumerate() {
            let p = replayed[d.episode].0;
            let pick = |s: &Option<Scored>| s.as_ref().map(|s| serde_json::json!([s.pick, s.prob]));
            let line = serde_json::json!({
                "episode": p.ep.id,
                "task": p.ep.task_id,
                "model": p.model,
                "target": p.target,
                "step": d.step,
                "kind": d.kind,
                "tool": d.tool,
                "agent": d.agent,
                "actual": d.actual,
                "fixed": d.fixed,
                "key": d.request.as_ref().map(stretto_oracle::request_key),
                "options": scored[i].as_ref().map(|s| s.probs.keys().collect::<Vec<_>>()),
                "pick": pick(&scored[i]),
                "split": pick(&split[i]),
                "combined": combined.as_ref().and_then(|c| pick(&c[i])),
                "predicates": predicates[i],
            });
            lines.push_str(&line.to_string());
            lines.push('\n');
        }
        std::fs::write(&path, lines).with_context(|| format!("writing {}", path.display()))?;
    }

    let layout = |answers: &[Option<Scored>]| -> Vec<Answers> {
        let mut by_episode: Vec<Vec<usize>> = vec![Vec::new(); replayed.len()];
        for (i, d) in decisions.iter().enumerate() {
            by_episode[d.episode].push(i);
        }
        by_episode
            .iter()
            .enumerate()
            .map(|(e, idx)| {
                let ds: Vec<&Decision> = idx.iter().map(|&i| &decisions[i]).collect();
                let ss: Vec<Option<Scored>> = idx.iter().map(|&i| answers[i].clone()).collect();
                shadow::projection_answers(&ds, &ss, replayed[e].0.steps.len(), vocab)
            })
            .collect()
    };
    let raw = layout(&scored);
    let combined_answers = combined.as_deref().map(layout);
    let lookup_first_answers = lookup_first.as_deref().map(layout);

    let agreement = |answers: &[Option<Scored>], keep: &dyn Fn(&Prepared) -> bool, next: bool| {
        Agreement::of(
            decisions
                .iter()
                .zip(answers)
                .filter(|(d, _)| keep(replayed[d.episode].0) && (d.kind == Kind::Next) == next)
                .filter_map(|(d, s)| s.as_ref().map(|s| (s, d.actual.as_str()))),
            &sc.thresholds,
        )
    };
    let row = |model: String, target: bool, keep: &dyn Fn(&Prepared) -> bool| ShadowRow {
        model,
        target,
        next: agreement(&scored, keep, true),
        args: agreement(&scored, keep, false),
        split: v2.then(|| agreement(&split, keep, true)),
        combined: combined.as_deref().map(|c| agreement(c, keep, true)),
    };
    let mut models: Vec<(&str, bool)> = replayed
        .iter()
        .map(|(p, _)| (p.model.as_str(), p.target))
        .collect();
    models.sort();
    models.dedup();
    models.sort_by_key(|&(_, target)| target);
    let mut rows: Vec<ShadowRow> = models
        .iter()
        .map(|&(m, target)| row(m.to_string(), target, &|p: &Prepared| p.model == m))
        .collect();
    rows.push(row(
        "all source models".to_string(),
        false,
        &|p: &Prepared| !p.target,
    ));
    let mut args: BTreeMap<(String, String), Vec<(&Scored, &str)>> = BTreeMap::new();
    for (d, s) in decisions.iter().zip(&scored) {
        if let (Kind::Arg(arg), Some(tool), Some(s)) = (&d.kind, &d.tool, s) {
            if !replayed[d.episode].0.target {
                args.entry((tool.clone(), arg.clone()))
                    .or_default()
                    .push((s, d.actual.as_str()));
            }
        }
    }
    let by_arg = args
        .into_iter()
        .map(|((tool, arg), answers)| ArgRow {
            tool,
            arg,
            agreement: Agreement::of(answers, &sc.thresholds),
        })
        .collect();
    let source_next: Vec<(&Decision, &Option<Scored>)> = decisions
        .iter()
        .zip(&scored)
        .filter(|(d, _)| d.kind == Kind::Next && !replayed[d.episode].0.target)
        .collect();
    let offered = source_next
        .iter()
        .filter(|(d, s)| match (&d.fixed, s) {
            (Some(_), _) => d.actual == RESPOND,
            (None, Some(s)) => s.probs.contains_key(&d.actual),
            (None, None) => false,
        })
        .count() as f64
        / source_next.len().max(1) as f64;
    let mut versions: Vec<String> = asked.responses.values().map(|r| r.model.clone()).collect();
    versions.sort();
    versions.dedup();
    let report = ShadowReport {
        oracle: match sc.oracle {
            shadow::OracleKind::Mock => "mock",
            shadow::OracleKind::Jev => "jev",
            shadow::OracleKind::Replay => "replay",
        }
        .to_string(),
        versions,
        decisions: decisions.len(),
        distinct: asked.distinct,
        answered: asked.responses.len(),
        errors: asked.errors,
        first_error: asked.first_error.clone(),
        input_tokens: asked.responses.values().map(|r| r.usage.input_tokens).sum(),
        rows,
        by_arg,
        thresholds: sc.thresholds.clone(),
        projection: Vec::new(),
        questions: sc.questions,
        structural: decisions
            .iter()
            .filter(|d| d.kind == Kind::Next && d.request.is_none())
            .count(),
        offered: if v2 { offered } else { 1.0 },
        weights,
        predicates: sc.predicates.clone(),
    };
    Ok(ShadowRun {
        report,
        raw,
        combined: combined_answers,
        lookup_first: lookup_first_answers,
        sites,
        folds,
        cases,
    })
}

/// The combined answers, the arbiter's mean weights (named), the arbiter of
/// each fold, and the number of cases it was fitted on.
type Combined = (Vec<Option<Scored>>, Vec<(String, f64)>, Vec<Fitted>, usize);

/// v2: combine each asked next-step answer with the habit (see
/// [`arbitrate`]); argument answers and decisions settled without asking
/// pass through. Returns the answers, the arbiter's mean weights, and the
/// arbiter fitted for each fold.
#[allow(clippy::too_many_arguments)]
fn combine(
    decisions: &[Decision],
    scored: &[Option<Scored>],
    split: &[Option<Scored>],
    predicates: &[BTreeMap<String, f64>],
    weighed: &[Predicate],
    replayed: &[(&Prepared, &EncodedEpisode)],
    habit: &GroupedModel,
    vocab: &Vocab,
) -> Combined {
    let mut cases: Vec<Case> = Vec::new();
    let mut at: Vec<usize> = Vec::new();
    for (i, d) in decisions.iter().enumerate() {
        if d.kind != Kind::Next || d.request.is_none() {
            continue;
        }
        let (Some(one), Some(two)) = (&scored[i], &split[i]) else {
            continue;
        };
        let (p, enc) = replayed[d.episode];
        let Step {
            action: Action::Tool(prev),
            outcome,
        } = &p.steps[d.step - 1]
        else {
            continue;
        };
        let predicted = habit.predict_at(enc, d.step);
        let Some((mut case, options)) = case_of(
            one,
            two,
            &predicates[i],
            weighed,
            &predicted,
            prev,
            *outcome == Outcome::Err,
            p.group,
            vocab,
        ) else {
            continue;
        };
        case.actual = options.iter().position(|o| *o == d.actual);
        case.fit = !p.target;
        cases.push(case);
        at.push(i);
    }
    let (arbitrated, weights, folds) = arbitrate::cross_fit_folds(&cases, FOLDS);
    let names = ["habit", "one question", "split", "handing back"]
        .into_iter()
        .map(String::from)
        .chain(weighed.iter().map(|q| format!("predicate {}", q.id)))
        .chain(["the model's record at the site".to_string()]);
    let weights: Vec<(String, f64)> = if cases.is_empty() {
        Vec::new()
    } else {
        names.zip(weights).collect()
    };
    let mut out = scored.to_vec();
    for (&i, a) in at.iter().zip(arbitrated) {
        let d = &decisions[i];
        let options: Vec<String> = scored[i]
            .as_ref()
            .map(|s| s.probs.keys().cloned().collect())
            .unwrap_or_default();
        let probs: BTreeMap<String, f64> = options.iter().cloned().zip(a.probs).collect();
        out[i] = Some(Scored::of(probs, options[a.top].clone(), &d.actual));
    }
    let fitted_on = cases.iter().filter(|c| c.fit).count();
    (out, weights, folds, fitted_on)
}

/// The arbiter's view of one v2 next-step decision (see [`arbitrate`]): the
/// options the System-One model was offered (their order is the case's) and
/// each option's features, from its answers (`one`, `two`, the predicates'
/// answers, of which `weighed` are features) and the habit's prediction
/// `predicted` at the decision, which follows a call to `prev` that failed
/// or not. The case's `actual` and `fit` are left for the caller. `None` if
/// handing back is not an option.
#[allow(clippy::too_many_arguments)]
pub(crate) fn case_of(
    one: &Scored,
    two: &Scored,
    predicates: &BTreeMap<String, f64>,
    weighed: &[Predicate],
    predicted: &[f64],
    prev: &str,
    failed: bool,
    group: u64,
    vocab: &Vocab,
) -> Option<(Case, Vec<String>)> {
    let ln = |v: f64| v.max(1e-6).ln();
    let logit = |v: f64| {
        let v = v.clamp(1e-4, 1.0 - 1e-4);
        (v / (1.0 - v)).ln()
    };
    let options: Vec<String> = one.probs.keys().cloned().collect();
    let respond = options.iter().position(|o| o == RESPOND)?;
    // The habit's prediction as a read-only flow would act on it: all mass
    // on steps it cannot take (replies, writes, lookups not offered) goes to
    // handing back.
    let mut prior: Vec<f64> = options
        .iter()
        .map(|o| {
            if o == RESPOND {
                0.0
            } else {
                predicted[vocab.id(&Action::Tool(o.clone())) as usize]
            }
        })
        .collect();
    prior[respond] = (1.0 - prior.iter().sum::<f64>()).max(0.0);
    let features = options
        .iter()
        .enumerate()
        .map(|(a, o)| {
            let mut x = vec![
                ln(prior[a]),
                ln(one.probs[o]),
                ln(two.probs.get(o).copied().unwrap_or(0.0)),
                (a == respond) as u8 as f64,
            ];
            // Each predicate's answer, on the options it bears on (zero
            // where it was not asked).
            for q in weighed {
                let on = match q.favors {
                    Favors::SameLookup => o == prev,
                    Favors::AnyLookup => a != respond,
                    Favors::HandBack => a == respond,
                };
                let answer = predicates.get(&q.id).copied().map_or(0.0, logit);
                x.push(if on { answer } else { 0.0 });
            }
            x
        })
        .collect();
    let case = Case {
        group,
        site: Sites::name(prev, failed),
        features,
        pick: options
            .iter()
            .position(|o| *o == one.pick)
            .unwrap_or(respond),
        actual: None,
        fit: false,
    };
    Some((case, options))
}

#[allow(clippy::type_complexity)]
fn projection_input<'a>(
    p: &Prepared,
    encoded: &'a EncodedEpisode,
    needs: &[ArgNeed],
    prices: &BTreeMap<&str, (f64, f64)>,
    answers: Option<&(Vec<Option<OracleStep>>, Vec<Option<OracleArgs>>)>,
) -> ProjectionInput<'a> {
    ProjectionInput {
        encoded,
        turns: p.turns.clone(),
        args: needs.to_vec(),
        usage: p.usage.clone(),
        input_price: prices[p.model.as_str()].0,
        oracle_steps: answers.map(|a| a.0.clone()).unwrap_or_default(),
        oracle_args: answers.map(|a| a.1.clone()).unwrap_or_default(),
    }
}

/// Share of tool-calling turns that made several calls at once.
fn parallel_share<'a, 'b: 'a>(episodes: impl Iterator<Item = &'a Prepared<'b>>) -> f64 {
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
pub(crate) fn task_group(task_id: &str) -> u64 {
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

#[cfg(test)]
mod tests {
    use super::Target;

    #[test]
    fn targets_parse_with_or_without_a_label() {
        assert_eq!(
            Target::parse("gpt-5.2-none=runs/a.json"),
            Target {
                label: Some("gpt-5.2-none".into()),
                path: "runs/a.json".into()
            }
        );
        assert_eq!(Target::parse("runs/a.json").label, None);
        // An '=' inside a path is not a label.
        assert_eq!(Target::parse("dir/x=y.json").label, None);
    }
}
