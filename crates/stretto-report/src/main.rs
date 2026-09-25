use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use stretto_oracle::{Answer, MockOracle, NoulCriteria, Oracle, Question, ReplayCache, Request};
use stretto_report::confirm::Second;
use stretto_report::flow::{Arbiter, Decider};
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_report::{phase0, render};

/// stretto: compile agent behavior into typed probabilistic flows.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// Parsed once, so the size of the largest variant does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Command {
    /// Measure how compressible an agent's behavior is (no API keys needed),
    /// and with --oracle, how well a System-One model takes the decisions
    /// flows would hand it (Phase 0b).
    Phase0 {
        #[command(flatten)]
        data: Phase0Args,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the full report as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Serve a live read-only flow (RFC-001 §3.13): compile it from the same
    /// data and questions as `phase0 --questions v2`, goal free, then answer
    /// one query per connection on a local TCP port. A query is a JSON line
    /// `{"task_id", "messages"}` (τ²-bench messages so far, ending with a tool
    /// result); the answer is a JSON line with `"action": "lookup"` (and
    /// `tool`, `arguments`) or `"action": "hand_back"` (and `reason`).
    FlowServe {
        #[command(flatten)]
        data: Phase0Args,
        /// Address to listen on (port 0: any free port; the ready line on
        /// stderr names it).
        #[arg(long, default_value = "127.0.0.1:0")]
        listen: String,
        /// Take the most likely lookup when its probability, times the
        /// chance that its arguments are the agent's, is at least this.
        #[arg(long, default_value_t = 0.3)]
        threshold: f64,
        /// Where each option's probability comes from: `arbiter` (the
        /// habit, the System-One model and the predicates, combined: arm D0)
        /// or `habit` (the habit alone, never asking the System-One model:
        /// a flow compiled from traces only, arm C at a high threshold).
        #[arg(long, value_enum, default_value_t = DeciderArg::Arbiter)]
        decider: DeciderArg,
        /// Stop asking the System-One model after this many live questions.
        #[arg(long, default_value_t = 300)]
        max_questions: usize,
        /// Append every query's answer here (JSON lines).
        #[arg(long)]
        log: Option<PathBuf>,
    },
    /// Compile a live read-only flow and write its IR (JSON) to a file: the
    /// same data and questions as `phase0 --questions v2`, goal free, from
    /// cached System-One answers only. `serve` and `stretto-proxy --flow`
    /// load the file.
    Compile {
        #[command(flatten)]
        data: Phase0Args,
        /// Where to write the flow.
        #[arg(long)]
        out: PathBuf,
    },
    /// Learn a live flow from sessions recorded by `stretto-proxy` (JSONL
    /// logs in a directory) and write its IR, as `compile` does from
    /// τ²-bench results. The tools come from the sessions' `tools/list`
    /// responses (their `readOnlyHint` annotations), or from `--manifest`.
    /// With `--results`, the sessions are τ²-bench episodes on a checkout's
    /// training tasks instead, as if a deployment had recorded them.
    Learn {
        /// Directory of session logs (`*.jsonl`).
        #[arg(long, required_unless_present = "results")]
        sessions: Option<PathBuf>,
        /// τ²-bench results to learn from in place of `--sessions`
        /// (repeatable): their episodes on the training tasks of the
        /// `--tau2` checkout's split, with their rewards. The tools come
        /// from the checkout.
        #[arg(
            long = "results",
            requires = "tau2",
            conflicts_with_all = ["sessions", "manifest", "rewards"]
        )]
        results: Vec<PathBuf>,
        /// The τ²-bench checkout that `--results` belong to.
        #[arg(long)]
        tau2: Option<PathBuf>,
        /// With `--results`, learn from this share of the training tasks:
        /// the sample `compile --train-fraction` takes.
        #[arg(long, default_value_t = 1.0)]
        train_fraction: f64,
        /// With `--results`, learn from these training tasks only
        /// (comma-separated), in place of `--train-fraction`.
        #[arg(long, value_delimiter = ',', conflicts_with = "train_fraction")]
        train_tasks: Vec<String>,
        /// With `--results`, only these trials of each task (default: all).
        #[arg(long, num_args = 1..)]
        trials: Vec<u32>,
        /// Ask no System-One model: every session trains the habit, and the
        /// flow has no arbiter (serve it with `--decider habit`).
        #[arg(long)]
        habit_only: bool,
        /// Once the arbiter is fitted on the held-out sessions, learn the
        /// habit, the sites and the bindings again from every session.
        #[arg(long, conflicts_with = "habit_only")]
        refit_habit: bool,
        /// Ask no System-One model while learning: every session trains the
        /// habit, and the flow serves this arbiter instead. It is an arbiter
        /// file (`export-arbiter`; `data/arbiters/` ships two), or a flow
        /// whose arbiter to take, such as one `compile` fitted on other
        /// agents' traces.
        #[arg(long, conflicts_with_all = ["habit_only", "refit_habit"])]
        arbiter_from: Option<PathBuf>,
        /// Offer every read-only tool at every site (see `compile`). Only
        /// the arbiter a flow fits here weighs such options.
        #[arg(long, conflicts_with_all = ["habit_only", "arbiter_from"])]
        manifest_options: bool,
        /// The domain to name the flow for.
        #[arg(long)]
        domain: String,
        /// A tool manifest (JSON, as stretto-trace writes it) instead of the
        /// sessions' own `tools/list`.
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Rewards by session id (JSON object). Sessions without one count as
        /// successful.
        #[arg(long)]
        rewards: Option<PathBuf>,
        /// Who answers the held-out questions the arbiter is fitted on.
        /// `--habit-only` and `--arbiter-from` ask nothing, so they take none
        /// of the oracle's options.
        #[arg(long, value_enum, default_value_t = OracleArg::Jev, conflicts_with_all = ["habit_only", "arbiter_from"])]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache", conflicts_with_all = ["habit_only", "arbiter_from"])]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, default_value_t = 1.0, conflicts_with_all = ["habit_only", "arbiter_from"])]
        oracle_budget: f64,
        /// Yes/no predicates to ask and weigh (see `data/predicates-v2.json`).
        /// An arbiter from `--arbiter-from` brings its own.
        #[arg(long, conflicts_with_all = ["habit_only", "arbiter_from"])]
        predicates: Option<PathBuf>,
        /// Where to write the flow.
        #[arg(long)]
        out: PathBuf,
    },
    /// Serve a compiled flow (from `compile`), as `flow-serve` does.
    Serve {
        /// The flow IR to load.
        #[arg(long)]
        flow: PathBuf,
        /// Who answers live questions: `jev` (needs TYPESAFE_API_KEY),
        /// `replay` (the cache only) or `mock`.
        #[arg(long, value_enum, default_value_t = OracleArg::Jev)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Address to listen on (port 0: any free port; the ready line on
        /// stderr names it).
        #[arg(long, default_value = "127.0.0.1:0")]
        listen: String,
        /// Take the most likely lookup when its probability, times the
        /// chance that its arguments are the agent's, is at least this.
        #[arg(long, default_value_t = 0.3)]
        threshold: f64,
        /// Where each option's probability comes from: `arbiter` (the
        /// habit, the System-One model and the predicates, combined: arm D0)
        /// or `habit` (the habit alone, never asking the System-One model:
        /// a flow compiled from traces only, arm C at a high threshold).
        #[arg(long, value_enum, default_value_t = DeciderArg::Arbiter)]
        decider: DeciderArg,
        /// Stop asking the System-One model after this many live questions.
        #[arg(long, default_value_t = 300)]
        max_questions: usize,
        /// Append every query's answer here (JSON lines).
        #[arg(long)]
        log: Option<PathBuf>,
    },
    /// Test the policy guards (typed checks a proxy runs before a write)
    /// against recorded τ²-bench trajectories: every write is checked
    /// against what came before it, as `stretto-proxy --guards` checks it.
    /// A rule that fails the writes of successful episodes is too strict,
    /// or wrong.
    Guards {
        /// Path to a τ²-bench checkout (its published baselines are read).
        #[arg(long)]
        tau2: PathBuf,
        /// Domains to audit.
        #[arg(long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to audit: `path` or `label=path`
        /// (repeatable; files for other domains are skipped).
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the audit as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Judge the customer's confirmation before each write with a
    /// System-One model, next to the guards' word list, on τ²-bench's
    /// published trajectories: one yes/no question per write, sorted by
    /// whether the tool accepted it and the episode passed.
    Confirm {
        /// Path to a τ²-bench checkout (its published baselines are read).
        #[arg(long)]
        tau2: PathBuf,
        /// Domains to judge.
        #[arg(long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to judge: `path` or `label=path`
        /// (repeatable; files for other domains are skipped).
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Who judges: `jev` (needs TYPESAFE_API_KEY; pays once per distinct
        /// question), `replay` (the cache only) or `mock`.
        #[arg(long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, default_value_t = 2.0)]
        oracle_budget: f64,
        /// The judge fails a write below this probability of an explicit yes.
        #[arg(long, default_value_t = 0.5)]
        threshold: f64,
        /// Disagreements to show, of each kind, per domain.
        #[arg(long, default_value_t = 8)]
        examples: usize,
        /// Also ask a second question about each write: whether the agent had
        /// `proposed` this change before the customer's reply, or (the first
        /// wording, too literal) whether its message `described` it. The
        /// judge then fails a write unless both answers are yes.
        #[arg(long, value_enum)]
        second_question: Option<SecondArg>,
        /// Write every distinct question to this file (JSON lines; the
        /// domain is added to the file name).
        #[arg(long)]
        oracle_dump: Option<PathBuf>,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write every judged write, and the audits, as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Match descriptions to records (RFC-001): at each write in τ²-bench's
    /// published trajectories that picks records out of earlier results
    /// (items of an order, a new variant, a payment method, a reservation),
    /// ask the System-One model which one the customer means, without the
    /// agent's pick, and score both against the task's expected actions.
    Match {
        /// Path to a τ²-bench checkout.
        #[arg(long)]
        tau2: PathBuf,
        /// Domains to judge.
        #[arg(long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to judge, besides the published baselines
        /// (repeatable).
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Who answers: `jev` (needs TYPESAFE_API_KEY), `replay` (the cache
        /// only) or `mock`.
        #[arg(long, value_enum, default_value_t = OracleArg::Jev)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, default_value_t = 2.0)]
        oracle_budget: f64,
        /// Disagreements to show, of each kind, per domain.
        #[arg(long, default_value_t = 8)]
        examples: usize,
        /// Write every distinct question to this file (JSON lines; the
        /// domain is added to the file name).
        #[arg(long)]
        oracle_dump: Option<PathBuf>,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write every choice, and the audits, as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Audit a flow against recorded episodes: score the agent's own steps
    /// under the flow's decisions, run as a fugue program, for agreement,
    /// calibration and surprise per site and per episode. Run it on new
    /// sessions before trusting a flow compiled from older ones.
    Audit {
        /// The flow IR to audit.
        #[arg(long)]
        flow: PathBuf,
        /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`).
        #[arg(long)]
        sessions: Option<PathBuf>,
        /// τ²-bench results files (repeatable); files for other domains are
        /// skipped.
        #[arg(long = "results")]
        results: Vec<PathBuf>,
        /// With --results: keep only the test split of this τ²-bench
        /// checkout, the tasks a flow compiled from it never trained on.
        #[arg(long)]
        tau2: Option<PathBuf>,
        /// Who answers the flow's questions: `replay` (the cache only;
        /// decisions it cannot answer are left out), `jev` (needs
        /// TYPESAFE_API_KEY; about $0.0001 per decision) or `mock`.
        #[arg(long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the audit as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Check that Jev is reachable with TYPESAFE_API_KEY: ask one small
    /// question (uncached) and print the answer, model version and latency.
    JevCheck,
    /// Write a flow's arbiter to its own file, to ship: the predicates it
    /// weighs, the model it asks, and one fit of its weights. `learn
    /// --arbiter-from` serves it with a habit learned from new sessions. The
    /// flow's folds must share one fit (`compile --pooled-arbiter`, or a
    /// flow from `learn`).
    ExportArbiter {
        /// The flow whose arbiter to write.
        #[arg(long)]
        flow: PathBuf,
        /// Where to write it.
        #[arg(long)]
        out: PathBuf,
    },
    /// Write every cached oracle answer to stdout as JSON lines
    /// (`{"key", "response"}`). Answers carry no benchmark text, so the
    /// bundle can be shared to replay Phase 0b without a key.
    ExportAnswers {
        /// Replay cache to read.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
    },
    /// Read JSON lines from `export-answers` on stdin into a replay cache.
    ImportAnswers {
        /// Replay cache to fill.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
    },
}

/// What Phase 0 reads and how it measures, shared by `phase0` and
/// `flow-serve`.
#[derive(clap::Args)]
struct Phase0Args {
    /// Path to a τ²-bench checkout.
    #[arg(long)]
    tau2: PathBuf,
    /// Domains to analyze.
    #[arg(long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
    domains: Vec<String>,
    /// Context length for coverage and transfer.
    #[arg(long, default_value_t = 2)]
    order: usize,
    /// MH draws for the posterior over α (0 to use --alpha).
    #[arg(long, default_value_t = 600)]
    alpha_samples: usize,
    /// α to use when --alpha-samples is 0.
    #[arg(long, default_value_t = 1.0)]
    alpha: f64,
    /// Training observations a context needs before the habit may act on it.
    #[arg(long, default_value_t = 5.0)]
    min_evidence: f64,
    /// Seed for the MH chain.
    #[arg(long, default_value_t = 7)]
    seed: u64,
    /// Skip learning code features from tool outputs.
    #[arg(long)]
    no_features: bool,
    /// Do not train on τ²-bench's published baselines in the checkout
    /// (then pass --source).
    #[arg(long)]
    no_baselines: bool,
    /// Do not let flows know the episode's goal: the habit is not
    /// conditioned on it and System-One questions do not name it (for
    /// live flows nobody names).
    #[arg(long)]
    no_intent: bool,
    /// Extra τ²-bench results to train on: `path` or `label=path`
    /// (repeatable; files for other domains are skipped).
    #[arg(long = "source")]
    sources: Vec<String>,
    /// τ²-bench results for an agent model the habit never trains on,
    /// measured as a transfer target: `path` or `label=path` (repeatable;
    /// files for other domains are skipped).
    #[arg(long = "target")]
    targets: Vec<String>,
    /// Phase 0b: who answers the System-One questions. `jev` needs
    /// TYPESAFE_API_KEY and pays once per distinct question; `replay`
    /// reads the cache only; `mock` checks the pipeline for free.
    #[arg(long, value_enum)]
    oracle: Option<OracleArg>,
    /// Which Phase 0b questions to ask: `v1` (one question over every
    /// tool, for flows that may call any tool) or `v2` (RFC-001 §3.5:
    /// read-only flows, a site's own lookups as options, a state slice,
    /// the stop decision asked on its own, and the answers combined with
    /// the habit).
    #[arg(long, value_enum, default_value_t = QuestionArg::V1, requires = "oracle")]
    questions: QuestionArg,
    /// v2: also describe each lookup by what its results supply,
    /// learned from argument dataflow in training.
    #[arg(long, requires = "oracle")]
    dataflow_hints: bool,
    /// v2: a JSON file of yes/no predicates about the state (see
    /// `data/predicates-v2.json`) to ask with every next-step question and
    /// weigh in the arbiter.
    #[arg(long, requires = "oracle")]
    predicates: Option<PathBuf>,
    /// v2: ask the predicates but leave them out of the arbiter, to
    /// measure what they add.
    #[arg(long, requires = "oracle")]
    no_predicate_features: bool,
    /// v2: offer every read-only tool at every site, not only the lookups
    /// seen there in training, for the System-One model to choose from. A
    /// lookup training never made is bound by argument name.
    #[arg(long, requires = "oracle")]
    manifest_options: bool,
    /// v2, `compile`: give the flow one arbiter, fitted on every held-out
    /// decision, in place of one per fold, so that `export-arbiter` can
    /// ship it. Replayed on the same test tasks it has seen other agents'
    /// decisions on them, so compare flows without it.
    #[arg(long, requires = "oracle")]
    pooled_arbiter: bool,
    /// Replay cache for oracle answers.
    #[arg(long, default_value = ".oracle-cache", requires = "oracle")]
    oracle_cache: PathBuf,
    /// Oracle requests in flight at once.
    #[arg(long, default_value_t = 8, requires = "oracle")]
    oracle_concurrency: usize,
    /// Ask at most this many distinct questions (a stable sample), for a
    /// pilot run.
    #[arg(long, requires = "oracle")]
    oracle_limit: Option<usize>,
    /// Refuse to start if uncached questions could cost more than this
    /// many dollars.
    #[arg(long, default_value_t = 5.0, requires = "oracle")]
    oracle_budget: f64,
    /// Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
    #[arg(long, requires = "oracle")]
    oracle_model: Option<String>,
    /// Write every distinct oracle request to this file (JSON lines; the
    /// domain is added to the file name).
    #[arg(long, requires = "oracle")]
    oracle_dump: Option<PathBuf>,
    /// Write every decision, with the agent's option and the oracle's
    /// pick, to this file (JSON lines; the domain is added to the file
    /// name).
    #[arg(long, requires = "oracle")]
    oracle_log: Option<PathBuf>,
    /// Train on this share of the training tasks: a fixed sample by task
    /// id, each smaller share part of every larger one. To see how flows
    /// do with fewer traces.
    #[arg(long, default_value_t = 1.0)]
    train_fraction: f64,
    /// Train on these training tasks only (comma-separated), in place of
    /// `--train-fraction`: a sample named exactly.
    #[arg(long, value_delimiter = ',', conflicts_with = "train_fraction")]
    train_tasks: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum OracleArg {
    Jev,
    Replay,
    Mock,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum QuestionArg {
    V1,
    V2,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DeciderArg {
    Arbiter,
    Habit,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SecondArg {
    Described,
    Proposed,
}

impl From<SecondArg> for Second {
    fn from(q: SecondArg) -> Self {
        match q {
            SecondArg::Described => Second::Described,
            SecondArg::Proposed => Second::Proposed,
        }
    }
}

impl From<DeciderArg> for Decider {
    fn from(d: DeciderArg) -> Self {
        match d {
            DeciderArg::Arbiter => Decider::Arbiter,
            DeciderArg::Habit => Decider::Habit,
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Phase0 { data, out, json } => {
            if data.pooled_arbiter {
                anyhow::bail!(
                    "--pooled-arbiter shapes a flow's arbiter: pass it to compile or flow-serve"
                );
            }
            let config = phase0_config(data)?;
            if config.shadow.is_some() && !config.features {
                anyhow::bail!("--oracle needs code features; drop --no-features");
            }
            let report = phase0::run(&config)?;
            let md = render::markdown(&report);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => print!("{md}"),
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&report)?)?;
            }
            Ok(())
        }
        Command::FlowServe {
            data,
            listen,
            threshold,
            decider,
            max_questions,
            log,
        } => {
            let domains = data.domains.clone();
            let [domain] = domains.as_slice() else {
                anyhow::bail!("flow-serve serves one domain: pass --domain once");
            };
            let config = phase0_config(data)?;
            let flow = compile(&config, domain)?;
            let oracle = config
                .shadow
                .as_ref()
                .expect("compile checked it")
                .build()?;
            let rule = Rule {
                threshold,
                decider: decider.into(),
                max_questions,
            };
            serve(&flow, oracle.as_ref(), &listen, rule, log)
        }
        Command::Compile { data, out } => {
            let domains = data.domains.clone();
            let [domain] = domains.as_slice() else {
                anyhow::bail!("compile builds one domain's flow: pass --domain once");
            };
            let config = phase0_config(data)?;
            let flow = compile(&config, domain)?;
            if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            flow.save(&out)?;
            eprintln!(
                "stretto: wrote the {} flow to {} ({} KB)",
                flow.domain(),
                out.display(),
                std::fs::metadata(&out)?.len() / 1024
            );
            Ok(())
        }
        Command::Learn {
            sessions,
            results,
            tau2,
            train_fraction,
            train_tasks,
            trials,
            habit_only,
            refit_habit,
            arbiter_from,
            manifest_options,
            domain,
            manifest,
            rewards,
            oracle,
            oracle_cache,
            oracle_budget,
            predicates,
            out,
        } => {
            if !(train_fraction > 0.0 && train_fraction <= 1.0) {
                anyhow::bail!("--train-fraction must be in (0, 1]");
            }
            let (episodes, manifest) = match sessions {
                Some(sessions) => {
                    if train_fraction < 1.0 || !train_tasks.is_empty() || !trials.is_empty() {
                        anyhow::bail!(
                            "--train-fraction, --train-tasks and --trials apply to --results"
                        );
                    }
                    let rewards: BTreeMap<String, f64> = match rewards {
                        Some(path) => serde_json::from_str(
                            &std::fs::read_to_string(&path)
                                .with_context(|| format!("reading {}", path.display()))?,
                        )?,
                        None => BTreeMap::new(),
                    };
                    let logs = stretto_trace::mcp::read_sessions(&sessions)?;
                    let manifest = match manifest {
                        Some(path) => serde_json::from_str(&std::fs::read_to_string(&path)?)?,
                        None => stretto_trace::mcp::manifest_of(&logs, &domain),
                    };
                    let episodes: Vec<stretto_trace::Episode> = logs
                        .iter()
                        .map(|log| {
                            let mut ep = stretto_trace::mcp::episode(log);
                            ep.domain = domain.clone();
                            ep.reward = rewards.get(&ep.id).copied().unwrap_or(1.0);
                            ep
                        })
                        .collect();
                    (episodes, manifest)
                }
                None => {
                    let tau2 = tau2.context("--results needs --tau2")?;
                    tau2_sessions(
                        &tau2,
                        &domain,
                        &results,
                        &train_tasks,
                        train_fraction,
                        &trials,
                    )?
                }
            };
            let mut config = phase0::Config::new(PathBuf::new());
            config.domains = vec![domain.clone()];
            config.refit_habit = refit_habit;
            let flow = if let Some(path) = arbiter_from {
                let habit =
                    phase0::compile_habit_flow_from_episodes(&config, &episodes, &manifest)?;
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let is_arbiter = serde_json::from_str::<serde_json::Value>(&text)
                    .with_context(|| format!("parsing {}", path.display()))?
                    .get("stretto_arbiter")
                    .is_some();
                if is_arbiter {
                    habit.with_arbiter(Arbiter::from_json(&text)?)
                } else {
                    habit.with_arbiter_of(stretto_report::flow::Flow::from_json(&text)?)?
                }
            } else if habit_only {
                phase0::compile_habit_flow_from_episodes(&config, &episodes, &manifest)?
            } else {
                let mut sc = ShadowConfig::new(oracle_kind(oracle));
                sc.cache_dir = oracle_cache;
                sc.budget = oracle_budget;
                sc.questions = QuestionSet::V2;
                sc.manifest_options = manifest_options;
                if let Some(path) = predicates {
                    sc.predicates =
                        serde_json::from_str::<PredicateFile>(&std::fs::read_to_string(&path)?)?
                            .predicates;
                }
                let oracle = sc.build()?;
                config.shadow = Some(sc);
                phase0::compile_flow_from_episodes(&config, &episodes, &manifest, oracle.as_ref())?
            };
            flow.save(&out)?;
            eprintln!(
                "stretto: learned the {domain} flow from {} sessions ({} tools) and wrote {}",
                episodes.len(),
                manifest.tools.len(),
                out.display()
            );
            Ok(())
        }
        Command::Serve {
            flow,
            oracle,
            oracle_cache,
            listen,
            threshold,
            decider,
            max_questions,
            log,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            if decider == DeciderArg::Arbiter && !flow.has_arbiter() {
                anyhow::bail!(
                    "this flow was learned without a System-One model: serve it with --decider habit"
                );
            }
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            let rule = Rule {
                threshold,
                decider: decider.into(),
                max_questions,
            };
            serve(&flow, oracle.as_ref(), &listen, rule, log)
        }
        Command::Guards {
            tau2,
            domains,
            sources,
            out,
            json,
        } => {
            let mut audits = Vec::new();
            for domain in &domains {
                let guards = stretto_report::guards::Guards::for_domain(domain)
                    .with_context(|| format!("no guards for {domain}"))?;
                let mut files = phase0::result_files(&tau2, domain)?;
                files.extend(sources.iter().map(|s| phase0::Target::parse(s).path));
                let mut episodes = Vec::new();
                for path in &files {
                    let run = stretto_trace::tau2::load_results(path)?;
                    if run.domain == *domain {
                        episodes.extend(run.episodes);
                    }
                }
                let refs: Vec<&stretto_trace::Episode> = episodes.iter().collect();
                audits.push(stretto_report::guards::audit(&guards, &refs));
            }
            let md = stretto_report::guards::markdown(&audits);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => print!("{md}"),
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&audits)?)?;
            }
            Ok(())
        }
        Command::Confirm {
            tau2,
            domains,
            sources,
            oracle,
            oracle_cache,
            oracle_budget,
            threshold,
            examples,
            second_question,
            oracle_dump,
            out,
            json,
        } => {
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            sc.budget = oracle_budget;
            sc.dump = oracle_dump;
            let model = sc.model.clone();
            let judge = sc.build()?;
            let (mut audits, mut judged) = (Vec::new(), Vec::new());
            for domain in &domains {
                let guards = stretto_report::guards::Guards::for_domain(domain)
                    .with_context(|| format!("no guards for {domain}"))?;
                let mut files = phase0::result_files(&tau2, domain)?;
                files.extend(sources.iter().map(|s| phase0::Target::parse(s).path));
                let mut episodes = Vec::new();
                for path in &files {
                    let run = stretto_trace::tau2::load_results(path)?;
                    if run.domain == *domain {
                        episodes.extend(run.episodes);
                    }
                }
                let refs: Vec<&stretto_trace::Episode> = episodes.iter().collect();
                let good = refs.iter().filter(|e| e.succeeded()).count();
                let mut items = stretto_report::confirm::writes(&guards, &refs, &model);
                let second = second_question.map(Second::from);
                stretto_report::confirm::judge(judge.as_ref(), &mut items, &sc, domain, second)?;
                audits.push(stretto_report::confirm::audit(
                    domain,
                    &items,
                    [good, refs.len() - good],
                    threshold,
                    examples,
                    second,
                ));
                judged.extend(items);
            }
            let md = stretto_report::confirm::markdown(&audits);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => print!("{md}"),
            }
            if let Some(path) = json {
                let all = serde_json::json!({"audits": audits, "writes": judged});
                write(&path, &serde_json::to_vec_pretty(&all)?)?;
            }
            Ok(())
        }
        Command::Match {
            tau2,
            domains,
            sources,
            oracle,
            oracle_cache,
            oracle_budget,
            examples,
            oracle_dump,
            out,
            json,
        } => {
            use stretto_report::matching;
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            sc.budget = oracle_budget;
            sc.dump = oracle_dump;
            let model = sc.model.clone();
            let judge = sc.build()?;
            let (mut audits, mut all) = (Vec::new(), Vec::new());
            for domain in &domains {
                let manifest = stretto_trace::tau2::load_manifest(
                    domain,
                    &tau2.join(format!("src/tau2/domains/{domain}/tools.py")),
                )?;
                let mut files = phase0::result_files(&tau2, domain)?;
                files.extend(sources.iter().map(|s| phase0::Target::parse(s).path));
                let mut items = Vec::new();
                for path in &files {
                    let run = stretto_trace::tau2::load_results(path)?;
                    if run.domain != *domain {
                        continue;
                    }
                    for ep in &run.episodes {
                        let gold = run
                            .gold_actions
                            .get(&ep.task_id)
                            .map_or(&[][..], Vec::as_slice);
                        items.extend(matching::choices(ep, gold, &manifest, &model));
                    }
                }
                matching::judge(judge.as_ref(), &mut items, &sc, domain)?;
                audits.push(matching::audit(domain, &items, examples));
                all.extend(items);
            }
            let md = matching::markdown(&audits);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => print!("{md}"),
            }
            if let Some(path) = json {
                let doc = serde_json::json!({"audits": audits, "choices": all});
                write(&path, &serde_json::to_vec_pretty(&doc)?)?;
            }
            Ok(())
        }
        Command::Audit {
            flow,
            sessions,
            results,
            tau2,
            oracle,
            oracle_cache,
            out,
            json,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            let domain = flow.domain().to_string();
            let mut episodes = Vec::new();
            if let Some(dir) = sessions {
                for log in stretto_trace::mcp::read_sessions(&dir)? {
                    let mut ep = stretto_trace::mcp::episode(&log);
                    ep.task_id = ep.id.clone();
                    episodes.push(ep);
                }
            }
            let test = match &tau2 {
                Some(root) => Some(
                    stretto_trace::tau2::load_split(
                        &root.join(format!("data/tau2/domains/{domain}/split_tasks.json")),
                    )?
                    .test,
                ),
                None => None,
            };
            for path in &results {
                let run = stretto_trace::tau2::load_results(path)?;
                if run.domain != domain {
                    continue;
                }
                episodes.extend(
                    run.episodes
                        .into_iter()
                        .filter(|ep| test.as_ref().is_none_or(|t| t.contains(&ep.task_id))),
                );
            }
            if episodes.is_empty() {
                anyhow::bail!("no episodes to audit: pass --sessions or --results for {domain}");
            }
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            let started = Instant::now();
            let audit = stretto_report::audit::audit(&flow, &episodes, oracle.as_ref());
            eprintln!(
                "stretto: audited {} decisions in {} episodes in {:.1} s",
                audit.decisions,
                audit.episodes,
                started.elapsed().as_secs_f64()
            );
            let md = stretto_report::audit::markdown(&audit);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => print!("{md}"),
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&audit)?)?;
            }
            Ok(())
        }
        Command::JevCheck => jev_check(),
        Command::ExportArbiter { flow, out } => {
            let arbiter = stretto_report::flow::Flow::load(&flow)?.arbiter()?;
            arbiter.save(&out)?;
            eprintln!(
                "stretto: wrote the {} arbiter, fitted on {} held-out decisions, to {}",
                arbiter.domain(),
                arbiter.provenance().arbiter_cases,
                out.display()
            );
            Ok(())
        }
        Command::ExportAnswers { oracle_cache } => {
            let cache: ReplayCache<MockOracle> = ReplayCache::new(oracle_cache, None);
            let mut out = std::io::stdout().lock();
            for (key, response) in cache.entries()? {
                serde_json::to_writer(
                    &mut out,
                    &serde_json::json!({"key": key, "response": response}),
                )?;
                writeln!(out)?;
            }
            Ok(())
        }
        Command::ImportAnswers { oracle_cache } => {
            let cache: ReplayCache<MockOracle> = ReplayCache::new(oracle_cache, None);
            let mut n = 0;
            for line in std::io::stdin().lock().lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry: AnswerLine = serde_json::from_str(&line)?;
                cache.insert(&entry.key, &entry.response)?;
                n += 1;
            }
            eprintln!("stretto: imported {n} answers");
            Ok(())
        }
    }
}

/// τ²-bench episodes from `results`, as a deployment's sessions for
/// `learn`: those on the training tasks of the checkout's split (the ones
/// named in `only`, or a `fraction` of them, as `compile` samples them), of
/// the named `trials` if any, with the checkout's tools for `domain`.
fn tau2_sessions(
    tau2: &Path,
    domain: &str,
    results: &[PathBuf],
    only: &[String],
    fraction: f64,
    trials: &[u32],
) -> Result<(Vec<stretto_trace::Episode>, stretto_trace::ToolManifest)> {
    use stretto_trace::tau2::{load_manifest, load_results, load_split};
    let manifest = load_manifest(
        domain,
        &tau2.join(format!("src/tau2/domains/{domain}/tools.py")),
    )?;
    let split = load_split(&tau2.join(format!("data/tau2/domains/{domain}/split_tasks.json")))?;
    let tasks: HashSet<String> = phase0::training_tasks(&split.train, only, fraction)?
        .into_iter()
        .collect();
    let mut episodes = Vec::new();
    for path in results {
        let run = load_results(path).with_context(|| format!("reading {}", path.display()))?;
        if run.domain != domain {
            anyhow::bail!("{} is for {}, not {domain}", path.display(), run.domain);
        }
        episodes.extend(run.episodes.into_iter().filter(|e| {
            tasks.contains(&e.task_id) && (trials.is_empty() || trials.contains(&e.trial))
        }));
    }
    if episodes.is_empty() {
        anyhow::bail!("the results have no episodes on the sampled training tasks");
    }
    Ok((episodes, manifest))
}

/// The Phase 0 configuration the arguments ask for.
fn phase0_config(data: Phase0Args) -> Result<phase0::Config> {
    let Phase0Args {
        tau2,
        domains,
        order,
        alpha_samples,
        alpha,
        min_evidence,
        seed,
        no_features,
        no_baselines,
        no_intent,
        sources,
        targets,
        oracle,
        questions,
        dataflow_hints,
        predicates,
        no_predicate_features,
        manifest_options,
        pooled_arbiter,
        oracle_cache,
        oracle_concurrency,
        oracle_limit,
        oracle_budget,
        oracle_model,
        oracle_dump,
        oracle_log,
        train_fraction,
        train_tasks,
    } = data;
    if !(train_fraction > 0.0 && train_fraction <= 1.0) {
        anyhow::bail!("--train-fraction must be in (0, 1]");
    }
    let mut config = phase0::Config::new(tau2);
    config.train_fraction = train_fraction;
    config.train_tasks = train_tasks;
    config.pooled_arbiter = pooled_arbiter;
    config.domains = domains;
    config.order = order;
    config.alpha_samples = alpha_samples;
    config.fixed_alpha = alpha;
    config.min_evidence = min_evidence;
    config.seed = seed;
    config.features = !no_features;
    config.baselines = !no_baselines;
    config.intent = !no_intent;
    config.sources = sources.iter().map(|t| phase0::Target::parse(t)).collect();
    config.targets = targets.iter().map(|t| phase0::Target::parse(t)).collect();
    let predicates = match predicates {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str::<PredicateFile>(&text)
                .with_context(|| format!("parsing {}", path.display()))?
                .predicates
        }
        None => Vec::new(),
    };
    config.shadow = oracle.map(|kind| {
        let mut sc = ShadowConfig::new(oracle_kind(kind));
        sc.cache_dir = oracle_cache;
        sc.hints = dataflow_hints;
        sc.predicates = predicates.clone();
        sc.predicate_features = !no_predicate_features;
        sc.manifest_options = manifest_options;
        sc.questions = match questions {
            QuestionArg::V1 => QuestionSet::V1,
            QuestionArg::V2 => QuestionSet::V2,
        };
        sc.concurrency = oracle_concurrency;
        sc.limit = oracle_limit;
        sc.budget = oracle_budget;
        if let Some(m) = oracle_model {
            sc.model = m;
        }
        sc.dump = oracle_dump;
        sc.log = oracle_log;
        sc
    });
    Ok(config)
}

fn oracle_kind(kind: OracleArg) -> OracleKind {
    match kind {
        OracleArg::Jev => OracleKind::Jev,
        OracleArg::Replay => OracleKind::Replay,
        OracleArg::Mock => OracleKind::Mock,
    }
}

/// A query to `flow-serve`.
#[derive(serde::Deserialize)]
struct FlowQuery {
    task_id: String,
    messages: serde_json::Value,
    #[serde(default)]
    agent_model: String,
}

/// Compile `domain`'s live flow from cached answers alone (fill the cache
/// with `phase0 --oracle jev --questions v2 --no-intent` first).
fn compile(config: &phase0::Config, domain: &str) -> Result<stretto_report::flow::Flow> {
    let sc = config
        .shadow
        .as_ref()
        .context("compiling a flow needs --oracle (jev, or replay)")?;
    if sc.questions != QuestionSet::V2 {
        anyhow::bail!("flows ask the v2 questions: pass --questions v2");
    }
    let mut offline = config.clone();
    if let Some(sc) = offline.shadow.as_mut() {
        sc.oracle = OracleKind::Replay;
        sc.log = None;
        sc.dump = None;
    }
    let cached = offline.shadow.as_ref().expect("checked above").build()?;
    let start = Instant::now();
    let flow = phase0::compile_flow(&offline, domain, cached.as_ref())?;
    eprintln!(
        "stretto: compiled the {} flow in {:.1} s",
        flow.domain(),
        start.elapsed().as_secs_f64()
    );
    for (tool, [not, named]) in flow.binding_agreement() {
        eprintln!(
            "stretto: binding {tool}: agreed {}/{} unmentioned, {}/{} mentioned",
            not.0, not.1, named.0, named.1
        );
    }
    Ok(flow)
}

/// How a served flow acts on its probabilities.
struct Rule {
    /// Take the most likely lookup when its probability, times the chance
    /// that its arguments are the agent's, is at least this.
    threshold: f64,
    /// Where the probabilities come from.
    decider: Decider,
    /// Stop asking the System-One model after this many live questions.
    max_questions: usize,
}

/// Answer one flow query per connection on `listen`, asking `oracle`.
fn serve(
    flow: &stretto_report::flow::Flow,
    oracle: &(dyn Oracle + Sync),
    listen: &str,
    rule: Rule,
    log: Option<PathBuf>,
) -> Result<()> {
    let Rule {
        threshold,
        decider,
        max_questions,
    } = rule;
    let listener =
        std::net::TcpListener::bind(listen).with_context(|| format!("listening on {listen}"))?;
    // The harness waits for this line.
    eprintln!("stretto: flow ready on {}", listener.local_addr()?);
    let mut log = match log {
        Some(path) => Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("opening {}", path.display()))?,
        ),
        None => None,
    };
    let mut asked = 0;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut line = String::new();
        std::io::BufReader::new(&stream).read_line(&mut line)?;
        let started = Instant::now();
        let answer = match serde_json::from_str::<FlowQuery>(&line) {
            Err(e) => {
                serde_json::json!({"action": "hand_back", "reason": format!("bad query: {e}")})
            }
            Ok(_) if asked >= max_questions => serde_json::json!({
                "action": "hand_back",
                "reason": format!("the flow's {max_questions} questions are spent"),
            }),
            Ok(query) => {
                let sim = serde_json::json!({
                    "id": "live",
                    "task_id": query.task_id,
                    "messages": query.messages,
                });
                let answer = stretto_trace::tau2::parse_simulation(
                    &sim.to_string(),
                    flow.domain(),
                    &query.agent_model,
                )
                .and_then(|episode| flow.next_with(&episode, oracle, threshold, decider));
                match answer {
                    Ok(next) => {
                        asked += next.key.is_some() as usize;
                        serde_json::to_value(&next)?
                    }
                    Err(e) => serde_json::json!({
                        "action": "hand_back",
                        "reason": format!("error: {e:#}"),
                    }),
                }
            }
        };
        let mut reply = answer.to_string();
        reply.push('\n');
        stream.write_all(reply.as_bytes())?;
        if let Some(f) = log.as_mut() {
            let mut entry = answer;
            entry["ms"] = serde_json::json!(started.elapsed().as_millis() as u64);
            writeln!(f, "{entry}")?;
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct PredicateFile {
    predicates: Vec<stretto_report::shadow::Predicate>,
}

#[derive(serde::Deserialize)]
struct AnswerLine {
    key: String,
    response: stretto_oracle::Response,
}

fn jev_check() -> Result<()> {
    let client = stretto_oracle::jev::JevClient::from_env()?;
    let request = Request {
        model: stretto_oracle::jev::JevClient::default_model(),
        state: serde_json::json!(
            "Customer: I'd like to cancel order #W1, I ordered it by mistake."
        ),
        questions: BTreeMap::from([(
            "cancel".to_string(),
            Question::Noul {
                instructions: "Does the customer want to cancel an order?".to_string(),
                criteria: Some(NoulCriteria {
                    yes: "The customer asks to cancel an order".to_string(),
                    no: "The customer wants something else".to_string(),
                }),
            },
        )]),
    };
    let start = Instant::now();
    let response = client.ask(&request)?;
    let elapsed = start.elapsed();
    let answer = match response.answers.get("cancel") {
        Some(Answer::Noul { noul }) => format!("P(yes) = {noul:.3}"),
        other => format!("{other:?}"),
    };
    println!(
        "Jev OK: model {}, {answer}, {} input tokens, {} ms",
        response.model,
        response.usage.input_tokens,
        elapsed.as_millis()
    );
    Ok(())
}

fn write(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{error::ErrorKind, Parser};

    fn parse(args: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("stretto").chain(args.split_whitespace()))
    }

    fn kind(args: &str) -> Option<ErrorKind> {
        parse(args).err().map(|e| e.kind())
    }

    const LEARN: &str = "learn --sessions logs --domain retail --out f.json";

    #[test]
    fn learn_refuses_options_its_habit_only_and_arbiter_from_branches_ignore() {
        for extra in [
            "--manifest-options",
            "--predicates p.json",
            "--oracle mock",
            "--oracle-cache cache",
            "--oracle-budget 2",
        ] {
            for branch in ["--habit-only", "--arbiter-from a.json"] {
                assert_eq!(
                    kind(&format!("{LEARN} {branch} {extra}")),
                    Some(ErrorKind::ArgumentConflict),
                    "{branch} {extra}"
                );
            }
            assert!(parse(&format!("{LEARN} {extra}")).is_ok(), "{extra} alone");
        }
        assert!(parse(&format!("{LEARN} --habit-only")).is_ok());
        assert!(parse(&format!("{LEARN} --arbiter-from a.json")).is_ok());
    }

    const PHASE0: &str = "phase0 --tau2 t";

    #[test]
    fn phase0_options_that_only_an_oracle_reads_need_an_oracle() {
        for extra in [
            "--questions v2",
            "--dataflow-hints",
            "--predicates p.json",
            "--no-predicate-features",
            "--manifest-options",
            "--pooled-arbiter",
            "--oracle-cache cache",
            "--oracle-concurrency 2",
            "--oracle-limit 10",
            "--oracle-budget 2",
            "--oracle-model m",
            "--oracle-dump d.jsonl",
            "--oracle-log l.jsonl",
        ] {
            assert_eq!(
                kind(&format!("{PHASE0} {extra}")),
                Some(ErrorKind::MissingRequiredArgument),
                "{extra}"
            );
            assert!(
                parse(&format!("{PHASE0} --oracle mock {extra}")).is_ok(),
                "{extra} with --oracle"
            );
        }
        assert!(parse(PHASE0).is_ok());
    }

    #[test]
    fn compile_takes_the_oracle_options_with_an_oracle() {
        assert!(parse(
            "compile --tau2 t --domain retail --out f.json --oracle replay --questions v2 \
             --predicates p.json --pooled-arbiter"
        )
        .is_ok());
        assert_eq!(
            kind("compile --tau2 t --domain retail --out f.json --questions v2"),
            Some(ErrorKind::MissingRequiredArgument)
        );
    }
}
