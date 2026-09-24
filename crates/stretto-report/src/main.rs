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
        /// Take the most likely lookup when the arbiter gives it at least
        /// this probability.
        #[arg(long, default_value_t = 0.3)]
        threshold: f64,
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
    Learn {
        /// Directory of session logs (`*.jsonl`).
        #[arg(long)]
        sessions: PathBuf,
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
        #[arg(long, value_enum, default_value_t = OracleArg::Jev)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, default_value_t = 1.0)]
        oracle_budget: f64,
        /// Yes/no predicates to ask and weigh (see `data/predicates-v2.json`).
        #[arg(long)]
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
        /// Take the most likely lookup when its probability is at least
        /// this.
        #[arg(long, default_value_t = 0.3)]
        threshold: f64,
        /// Stop asking the System-One model after this many live questions.
        #[arg(long, default_value_t = 300)]
        max_questions: usize,
        /// Append every query's answer here (JSON lines).
        #[arg(long)]
        log: Option<PathBuf>,
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
        Command::Phase0 { data, out, json } => {
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
            max_questions,
            log,
        } => {
            let domains = data.domains.clone();
            let [domain] = domains.as_slice() else {
                anyhow::bail!("flow-serve serves one domain: pass --domain once");
            };
            let config = phase0_config(data)?;
            flow_serve(&config, domain, &listen, threshold, max_questions, log)
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
            domain,
            manifest,
            rewards,
            oracle,
            oracle_cache,
            oracle_budget,
            predicates,
            out,
        } => {
            let rewards: BTreeMap<String, f64> = match rewards {
                Some(path) => serde_json::from_str(
                    &std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?,
                )?,
                None => BTreeMap::new(),
            };
            let mut logs = Vec::new();
            for entry in std::fs::read_dir(&sessions)
                .with_context(|| format!("listing {}", sessions.display()))?
            {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "jsonl")
                    && !path.to_string_lossy().ends_with(".flow.jsonl")
                {
                    logs.push(stretto_trace::mcp::read_log(&path)?);
                }
            }
            logs.sort_by(|a, b| a.header.session.cmp(&b.header.session));
            let manifest = match manifest {
                Some(path) => serde_json::from_str(&std::fs::read_to_string(&path)?)?,
                None => {
                    let mut m = stretto_trace::ToolManifest {
                        domain: domain.clone(),
                        ..Default::default()
                    };
                    for log in &logs {
                        let listed = stretto_trace::mcp::manifest(log, &domain);
                        m.tools.extend(listed.tools);
                        m.docs.extend(listed.docs);
                    }
                    m
                }
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
            let mut config = phase0::Config::new(PathBuf::new());
            config.domains = vec![domain.clone()];
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            sc.budget = oracle_budget;
            sc.questions = QuestionSet::V2;
            if let Some(path) = predicates {
                sc.predicates =
                    serde_json::from_str::<PredicateFile>(&std::fs::read_to_string(&path)?)?
                        .predicates;
            }
            let oracle = sc.build()?;
            config.shadow = Some(sc);
            let flow =
                phase0::compile_flow_from_episodes(&config, &episodes, &manifest, oracle.as_ref())?;
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
            max_questions,
            log,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            serve(
                &flow,
                oracle.as_ref(),
                &listen,
                threshold,
                max_questions,
                log,
            )
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
        oracle_cache,
        oracle_concurrency,
        oracle_limit,
        oracle_budget,
        oracle_model,
        oracle_dump,
        oracle_log,
    } = data;
    let mut config = phase0::Config::new(tau2);
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

fn flow_serve(
    config: &phase0::Config,
    domain: &str,
    listen: &str,
    threshold: f64,
    max_questions: usize,
    log: Option<PathBuf>,
) -> Result<()> {
    let flow = compile(config, domain)?;
    let oracle = config
        .shadow
        .as_ref()
        .expect("compile checked it")
        .build()?;
    serve(
        &flow,
        oracle.as_ref(),
        listen,
        threshold,
        max_questions,
        log,
    )
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

/// Answer one flow query per connection on `listen`, asking `oracle`.
fn serve(
    flow: &stretto_report::flow::Flow,
    oracle: &(dyn Oracle + Sync),
    listen: &str,
    threshold: f64,
    max_questions: usize,
    log: Option<PathBuf>,
) -> Result<()> {
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
                .and_then(|episode| flow.next(&episode, oracle, threshold));
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
