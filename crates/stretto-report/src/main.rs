use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;
use stretto_oracle::{Answer, MockOracle, NoulCriteria, Oracle, Question, ReplayCache, Request};
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
        #[arg(long, value_enum, default_value_t = QuestionArg::V1)]
        questions: QuestionArg,
        /// v2: also describe each lookup by what its results supply,
        /// learned from argument dataflow in training.
        #[arg(long)]
        dataflow_hints: bool,
        /// v2: a JSON file of yes/no predicates about the state (see
        /// `data/predicates-v2.json`) to ask with every next-step question and
        /// weigh in the arbiter.
        #[arg(long)]
        predicates: Option<PathBuf>,
        /// v2: ask the predicates but leave them out of the arbiter, to
        /// measure what they add.
        #[arg(long)]
        no_predicate_features: bool,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Oracle requests in flight at once.
        #[arg(long, default_value_t = 8)]
        oracle_concurrency: usize,
        /// Ask at most this many distinct questions (a stable sample), for a
        /// pilot run.
        #[arg(long)]
        oracle_limit: Option<usize>,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, default_value_t = 5.0)]
        oracle_budget: f64,
        /// Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
        #[arg(long)]
        oracle_model: Option<String>,
        /// Write every distinct oracle request to this file (JSON lines; the
        /// domain is added to the file name).
        #[arg(long)]
        oracle_dump: Option<PathBuf>,
        /// Write every decision, with the agent's option and the oracle's
        /// pick, to this file (JSON lines; the domain is added to the file
        /// name).
        #[arg(long)]
        oracle_log: Option<PathBuf>,
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the full report as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Check that Jev is reachable with TYPESAFE_API_KEY: ask one small
    /// question (uncached) and print the answer, model version and latency.
    JevCheck,
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

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Phase0 {
            tau2,
            domains,
            order,
            alpha_samples,
            alpha,
            min_evidence,
            seed,
            no_features,
            no_baselines,
            sources,
            targets,
            oracle,
            questions,
            dataflow_hints,
            predicates,
            no_predicate_features,
            oracle_cache,
            oracle_concurrency,
            oracle_limit,
            oracle_budget,
            oracle_model,
            oracle_dump,
            oracle_log,
            out,
            json,
        } => {
            let mut config = phase0::Config::new(tau2);
            config.domains = domains;
            config.order = order;
            config.alpha_samples = alpha_samples;
            config.fixed_alpha = alpha;
            config.min_evidence = min_evidence;
            config.seed = seed;
            config.features = !no_features;
            config.baselines = !no_baselines;
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
                let mut sc = ShadowConfig::new(match kind {
                    OracleArg::Jev => OracleKind::Jev,
                    OracleArg::Replay => OracleKind::Replay,
                    OracleArg::Mock => OracleKind::Mock,
                });
                sc.cache_dir = oracle_cache;
                sc.hints = dataflow_hints;
                sc.predicates = predicates.clone();
                sc.predicate_features = !no_predicate_features;
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
        Command::JevCheck => jev_check(),
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
