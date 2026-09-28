use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use stretto_oracle::{Answer, MockOracle, NoulCriteria, Oracle, Question, ReplayCache, Request};
use stretto_report::confirm::Second;
use stretto_report::flow::{Arbiter, Decider};
use stretto_report::init::Host;
use stretto_report::shadow::{OracleKind, QuestionSet, ShadowConfig};
use stretto_report::surprise::{self, Override};
use stretto_report::{phase0, render};

/// Compile an agent's recorded behavior into flows, serve them, and measure
/// them against published τ²-bench trajectories.
#[derive(Parser)]
#[command(name = "stretto", version)]
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
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write the full report as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
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
        #[arg(
            help_heading = "Serving",
            value_name = "ADDR",
            long,
            default_value = "127.0.0.1:0"
        )]
        listen: String,
        /// Take the most likely lookup when its probability, times the
        /// chance that its arguments are the agent's, is at least this.
        #[arg(
            help_heading = "Serving",
            value_name = "P",
            long,
            default_value_t = 0.3
        )]
        threshold: f64,
        /// Where each option's probability comes from: `arbiter` (the
        /// habit, the System-One model and the predicates, combined: arm D0),
        /// `habit` (the habit alone, never asking the System-One model: a
        /// flow compiled from traces only, arm C at a high threshold), or
        /// `reach` (the habit's counts for whether the agent makes the lookup
        /// before its next write, now or later; a flow learned with them).
        #[arg(help_heading = "Serving", long, value_enum, default_value_t = DeciderArg::Arbiter)]
        decider: DeciderArg,
        /// Stop asking the System-One model after this many live questions.
        #[arg(
            help_heading = "Serving",
            value_name = "N",
            long,
            default_value_t = 300
        )]
        max_questions: usize,
        /// Hand back for the rest of a session once the agent's steps in a
        /// row surprise the flow by more than this many nats on average,
        /// over the flow's surprise gate's steps (5, if it has none); `off`
        /// serves the flow without the gate it stores (`learn --surprise`).
        #[arg(help_heading = "Serving", value_name = "off|NATS", long)]
        surprise: Option<Override>,
        /// Append every query's answer here (JSON lines).
        #[arg(help_heading = "Serving", value_name = "FILE", long)]
        log: Option<PathBuf>,
        /// Explore: with this probability, take a lookup other than the
        /// rule's choice, drawn by the decider's probabilities among those
        /// that bind. Each answer then carries its `policy`: every option
        /// and the chance that the flow took what it took, for `evaluate`.
        /// 0 explores nothing but still logs it.
        #[arg(help_heading = "Serving", value_name = "EPSILON", long)]
        explore: Option<f64>,
        /// Seed for the exploration draws.
        #[arg(
            help_heading = "Serving",
            value_name = "N",
            long,
            default_value_t = 0,
            requires = "explore"
        )]
        explore_seed: u64,
    },
    /// Compile a live read-only flow and write its IR (JSON) to a file: the
    /// same data and questions as `phase0 --questions v2`, goal free, from
    /// cached System-One answers only. `serve` and `stretto-proxy --flow`
    /// load the file.
    Compile {
        #[command(flatten)]
        data: Phase0Args,
        /// Where to write the flow.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: PathBuf,
    },
    /// Learn a live flow from sessions recorded by `stretto-proxy` (JSONL
    /// logs in a directory) and write its IR, as `compile` does from
    /// τ²-bench results. The tools come from the sessions' `tools/list`
    /// responses (their `readOnlyHint` annotations), or from `--manifest`.
    /// The logs one host session left with several servers are one session,
    /// each tool named after its server (`server::tool`).
    /// With `--results`, the sessions are τ²-bench episodes on a checkout's
    /// training tasks instead, as if a deployment had recorded them; with
    /// `--otel`, the traces of an agent framework's OpenTelemetry spans.
    Learn(LearnArgs),
    /// Serve a compiled flow (from `compile`), as `flow-serve` does.
    Serve {
        /// The flow IR to load.
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        flow: PathBuf,
        /// Who answers live questions: `jev` (needs TYPESAFE_API_KEY),
        /// `replay` (the cache only) or `mock`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Jev)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// Address to listen on (port 0: any free port; the ready line on
        /// stderr names it).
        #[arg(
            help_heading = "Serving",
            value_name = "ADDR",
            long,
            default_value = "127.0.0.1:0"
        )]
        listen: String,
        /// Take the most likely lookup when its probability, times the
        /// chance that its arguments are the agent's, is at least this.
        #[arg(
            help_heading = "Serving",
            value_name = "P",
            long,
            default_value_t = 0.3
        )]
        threshold: f64,
        /// Where each option's probability comes from: `arbiter` (the
        /// habit, the System-One model and the predicates, combined: arm D0),
        /// `habit` (the habit alone, never asking the System-One model: a
        /// flow compiled from traces only, arm C at a high threshold), or
        /// `reach` (the habit's counts for whether the agent makes the lookup
        /// before its next write, now or later; a flow learned with them).
        #[arg(help_heading = "Serving", long, value_enum, default_value_t = DeciderArg::Arbiter)]
        decider: DeciderArg,
        /// Stop asking the System-One model after this many live questions.
        #[arg(
            help_heading = "Serving",
            value_name = "N",
            long,
            default_value_t = 300
        )]
        max_questions: usize,
        /// Hand back for the rest of a session once the agent's steps in a
        /// row surprise the flow by more than this many nats on average,
        /// over the flow's surprise gate's steps (5, if it has none); `off`
        /// serves the flow without the gate it stores (`learn --surprise`).
        #[arg(help_heading = "Serving", value_name = "off|NATS", long)]
        surprise: Option<Override>,
        /// Append every query's answer here (JSON lines).
        #[arg(help_heading = "Serving", value_name = "FILE", long)]
        log: Option<PathBuf>,
        /// Explore: with this probability, take a lookup other than the
        /// rule's choice, drawn by the decider's probabilities among those
        /// that bind. Each answer then carries its `policy`: every option
        /// and the chance that the flow took what it took, for `evaluate`.
        /// 0 explores nothing but still logs it.
        #[arg(help_heading = "Serving", value_name = "EPSILON", long)]
        explore: Option<f64>,
        /// Seed for the exploration draws.
        #[arg(
            help_heading = "Serving",
            value_name = "N",
            long,
            default_value_t = 0,
            requires = "explore"
        )]
        explore_seed: u64,
    },
    /// Test the policy guards (typed checks a proxy runs before a write)
    /// against recorded τ²-bench trajectories: every write is checked
    /// against what came before it, as `stretto-proxy --guards` checks it.
    /// A rule that fails the writes of successful episodes is too strict,
    /// or wrong.
    Guards {
        /// Path to a τ²-bench checkout (its published baselines are read).
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: PathBuf,
        /// Domains to audit.
        #[arg(help_heading = "Inputs", value_name = "NAME", long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to audit: `path` or `label=path`
        /// (repeatable; files for other domains are skipped).
        #[arg(help_heading = "Inputs", value_name = "[LABEL=]PATH", long = "source")]
        sources: Vec<String>,
        /// Write the Markdown report here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write the audit as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Judge the customer's confirmation before each write with a
    /// System-One model, next to the guards' word list, on τ²-bench's
    /// published trajectories: one yes/no question per write, sorted by
    /// whether the tool accepted it and the episode passed.
    Confirm {
        /// Path to a τ²-bench checkout (its published baselines are read).
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: PathBuf,
        /// Domains to judge.
        #[arg(help_heading = "Inputs", value_name = "NAME", long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to judge: `path` or `label=path`
        /// (repeatable; files for other domains are skipped).
        #[arg(help_heading = "Inputs", value_name = "[LABEL=]PATH", long = "source")]
        sources: Vec<String>,
        /// Who judges: `jev` (needs TYPESAFE_API_KEY; pays once per distinct
        /// question), `replay` (the cache only) or `mock`.
        #[arg(help_heading = "The judge", long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The judge",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(
            help_heading = "The judge",
            value_name = "DOLLARS",
            long,
            default_value_t = 2.0
        )]
        oracle_budget: f64,
        /// The judge fails a write below this probability of an explicit yes.
        #[arg(
            help_heading = "The judge",
            value_name = "P",
            long,
            default_value_t = 0.5
        )]
        threshold: f64,
        /// Disagreements to show, of each kind, per domain.
        #[arg(help_heading = "Output", value_name = "N", long, default_value_t = 8)]
        examples: usize,
        /// Also ask a second question about each write: whether the agent had
        /// `proposed` this change before the customer's reply, or (the first
        /// wording, too literal) whether its message `described` it. The
        /// judge then fails a write unless both answers are yes.
        #[arg(help_heading = "The judge", long, value_enum)]
        second_question: Option<SecondArg>,
        /// Write every distinct question to this file (JSON lines; the
        /// domain is added to the file name).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        oracle_dump: Option<PathBuf>,
        /// Write the Markdown report here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write every judged write, and the audits, as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Match descriptions to records (RFC-001): at each write in τ²-bench's
    /// published trajectories that picks records out of earlier results
    /// (items of an order, a new variant, a payment method, a reservation),
    /// ask the System-One model which one the customer means, without the
    /// agent's pick, and score both against the task's expected actions.
    Match {
        /// Path to a τ²-bench checkout.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: PathBuf,
        /// Domains to judge.
        #[arg(help_heading = "Inputs", value_name = "NAME", long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
        domains: Vec<String>,
        /// Extra τ²-bench results to judge, besides the published baselines
        /// (repeatable).
        #[arg(help_heading = "Inputs", value_name = "[LABEL=]PATH", long = "source")]
        sources: Vec<String>,
        /// Who answers: `jev` (needs TYPESAFE_API_KEY), `replay` (the cache
        /// only) or `mock`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Jev)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DOLLARS",
            long,
            default_value_t = 2.0
        )]
        oracle_budget: f64,
        /// Disagreements to show, of each kind, per domain.
        #[arg(help_heading = "Output", value_name = "N", long, default_value_t = 8)]
        examples: usize,
        /// Write every distinct question to this file (JSON lines; the
        /// domain is added to the file name).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        oracle_dump: Option<PathBuf>,
        /// Write the Markdown report here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write every choice, and the audits, as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Audit a flow against recorded episodes: score the agent's own steps
    /// under the flow's decisions, run as a fugue program, for agreement,
    /// calibration and surprise per site and per episode. Run it on new
    /// sessions before trusting a flow compiled from older ones.
    Audit {
        /// The flow IR to audit.
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        flow: PathBuf,
        /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`).
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        sessions: Option<PathBuf>,
        /// OpenTelemetry GenAI spans in place of --sessions: an OTLP JSON
        /// export, each trace a session (`learn --otel`).
        #[arg(
            help_heading = "Inputs",
            value_name = "FILE",
            long,
            conflicts_with = "sessions"
        )]
        otel: Option<PathBuf>,
        /// τ²-bench results files (repeatable); files for other domains are
        /// skipped.
        #[arg(help_heading = "Inputs", value_name = "FILE", long = "results")]
        results: Vec<PathBuf>,
        /// With --results: keep only the test split of this τ²-bench
        /// checkout, the tasks a flow compiled from it never trained on.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: Option<PathBuf>,
        /// Who answers the flow's questions: `replay` (the cache only;
        /// decisions it cannot answer are left out), `jev` (needs
        /// TYPESAFE_API_KEY; about $0.0001 per decision) or `mock`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// How the flow decides: `arbiter` (weighing the System-One model's
        /// answers), `habit` (the habit alone, asking nothing) or `reach`
        /// (the chance of each lookup before the agent's next write, asking
        /// nothing). Default: the arbiter, or the habit for a flow without
        /// one (`learn --habit-only`).
        #[arg(help_heading = "The System-One model", long, value_enum)]
        decider: Option<DeciderArg>,
        /// Write the Markdown report here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write the audit as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Watch for the agent changing under a flow: score recorded sessions in
    /// the order they ran, as `audit` does, for surprise, disagreement and
    /// the share of steps at sites the flow does not know, and sound an
    /// alarm when Bayesian online change-point detection puts a change on a
    /// recent session, naming the sites that moved. Exits with 1 while the
    /// alarm sounds after the last session, 2 on an error.
    Drift {
        /// The flow IR the sessions were served with.
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        flow: PathBuf,
        /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`),
        /// taken in the order they started.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        sessions: Option<PathBuf>,
        /// OpenTelemetry GenAI spans in place of --sessions: an OTLP JSON
        /// export, each trace a session (taken in the order they started).
        #[arg(
            help_heading = "Inputs",
            value_name = "FILE",
            long,
            conflicts_with = "sessions"
        )]
        otel: Option<PathBuf>,
        /// τ²-bench results files, their episodes taken in the order listed
        /// and before any --sessions, as a flow compiled from benchmark runs
        /// serves a deployment later; files for other domains are skipped.
        #[arg(help_heading = "Inputs", value_name = "FILE", long = "results")]
        results: Vec<PathBuf>,
        /// With --results: keep only the test split of this τ²-bench
        /// checkout, the tasks a flow compiled from it never trained on.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: Option<PathBuf>,
        /// How the flow decides: `habit` (the default: it predicts the
        /// agent's next step and asks nothing), `arbiter` (answered by
        /// --oracle) or `reach`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = DeciderArg::Habit)]
        decider: DeciderArg,
        /// Who answers the arbiter's questions: `replay` (the cache only),
        /// `jev` (needs TYPESAFE_API_KEY) or `mock`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// The prior chance that the agent changes after any one session.
        #[arg(help_heading = "The alarm", value_name = "P", long, default_value_t = 0.01, value_parser = probability)]
        hazard: f64,
        /// How recent a change the alarm reports, in sessions (at least 3:
        /// a change needs three sessions of the new run to count).
        #[arg(help_heading = "The alarm", value_name = "SESSIONS", long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(3..))]
        window: u64,
        /// The probability of a recent change that sounds the alarm.
        #[arg(help_heading = "The alarm", value_name = "P", long, default_value_t = 0.5, value_parser = probability)]
        threshold: f64,
        /// Write the Markdown report here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write every session's scores and the alarms as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Show a flow as a reviewer reads it (Markdown): the tools it may call,
    /// the lookups it may make after each call and where their arguments
    /// come from, what it does there with the habit alone, and how its
    /// arbiter weighs the System-One model's answers.
    FlowShow {
        /// The flow IR.
        #[arg(value_name = "FILE")]
        flow: PathBuf,
        /// The threshold the flow will be served with (`stretto-proxy
        /// --flow-threshold`).
        #[arg(value_name = "P", long, default_value_t = 0.3)]
        threshold: f64,
        /// Write the Markdown here (default: stdout).
        #[arg(value_name = "FILE", long)]
        out: Option<PathBuf>,
    },
    /// What changed from one flow to another, as a change list for a pull
    /// request (Markdown). Exits with 1 when a change needs review (the flow
    /// may call a tool, make a lookup, bind an argument from a source, or
    /// ask a model or a question it did not before), 2 on an error, and 0
    /// otherwise.
    FlowDiff {
        /// The flow before.
        #[arg(value_name = "OLD")]
        old: PathBuf,
        /// The flow after.
        #[arg(value_name = "NEW")]
        new: PathBuf,
        /// Leave out shares, chances and weights that moved by less than
        /// this.
        #[arg(value_name = "X", long, default_value_t = 0.05)]
        tolerance: f64,
        /// The threshold the flow will be served with (`stretto-proxy
        /// --flow-threshold`).
        #[arg(value_name = "P", long, default_value_t = 0.3)]
        threshold: f64,
        /// Write the Markdown here (default: stdout).
        #[arg(value_name = "FILE", long)]
        out: Option<PathBuf>,
    },
    /// Learn a deployment's staged flow: the next version of the flow the
    /// proxy serves (`--flow`, the committed flow), from every session
    /// recorded so far, as `learn --habit-only` learns one, keeping the
    /// committed flow's arbiter, promotion and thresholds. Each session
    /// that is new since the last run is first scored by the committed
    /// flow and by the staged flow as it was, as each is served; the report
    /// compares the two on the last sessions, out of sample for both, and
    /// lists what committing the staged flow would change. Run it as
    /// sessions arrive. Nothing reaches the proxy until `flow-commit`.
    Stage(StageArgs),
    /// Commit the staged flow that `stage` learned: make it the flow the
    /// proxy serves (`--flow`, replaced whole, so a proxy that starts reads
    /// the old flow or the new one). Every version of the committed flow is
    /// kept in `NAME.history/`, with what changed and, for a commit, the
    /// comparison `stage` last reported, for `flow-log` and `flow-rollback`.
    /// A committed flow changed by other means is kept as a version of its
    /// own first.
    FlowCommit {
        /// The committed flow, `NAME.flow.json`.
        #[arg(value_name = "FILE", long)]
        flow: PathBuf,
        /// Why, kept with the version.
        #[arg(value_name = "TEXT", long)]
        note: Option<String>,
    },
    /// Roll the committed flow back to an earlier version: the one before
    /// the current version, or `--to N`. The rollback is a version of its
    /// own, so it can be rolled back too.
    FlowRollback {
        /// The committed flow, `NAME.flow.json`.
        #[arg(value_name = "FILE", long)]
        flow: PathBuf,
        /// The version to restore (see `flow-log`).
        #[arg(value_name = "N", long)]
        to: Option<u32>,
        /// Why, kept with the version.
        #[arg(value_name = "TEXT", long)]
        note: Option<String>,
    },
    /// List the committed flow's versions, the latest first (Markdown): how
    /// and when each became the committed flow, why, what changed, and for a
    /// commit, the comparison it rested on.
    FlowLog {
        /// The committed flow, `NAME.flow.json`.
        #[arg(value_name = "FILE", long)]
        flow: PathBuf,
    },
    /// Estimate what another rule would have done on a flow's logged
    /// decisions (RFC-001 §3.7). The decisions are JSON lines, each a flow
    /// answer logged with its `policy` (`serve --explore`, `stretto-proxy
    /// --flow-explore`) and a `labels` list: each option's outcome (`used`,
    /// `detour`, `turn`), as `pilot/check_flow.py --explore` writes them. For
    /// each target, per site and in total: the lookups, used lookups,
    /// detours and turns spared, estimated directly from every option's
    /// label, and by IPS, self-normalized IPS and doubly robust estimates
    /// from the taken option's label alone.
    Evaluate {
        /// Labelled decisions (JSON lines).
        #[arg(
            help_heading = "Inputs",
            value_name = "FILE",
            long = "decisions",
            required = true
        )]
        decisions: Vec<PathBuf>,
        /// A rule to evaluate: `NAME=DECIDER@THRESHOLD`, the decider
        /// `arbiter` (the logged decider's probabilities) or `habit`.
        #[arg(
            help_heading = "Targets",
            value_name = "RULE",
            long = "target",
            required = true
        )]
        targets: Vec<String>,
        /// Refuse weighted estimates, a site's or the total's, below this
        /// effective sample size.
        #[arg(
            help_heading = "Targets",
            value_name = "N",
            long,
            default_value_t = 10.0
        )]
        min_ess: f64,
        /// Write the Markdown here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write every estimate as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Refine the arbiter's predicates (RFC-001 §3.4). From one domain's
    /// decision log (`phase0 --questions v2 --oracle-log`, with the
    /// candidates asked by `--candidates`), fit the arbiter by
    /// cross-validation over tasks with each candidate added, and keep
    /// candidates greedily while one raises the held-out log-likelihood of
    /// the agents' steps by more than a penalty. With --examples, first
    /// write examples from the sites where the arbiter is weakest, for
    /// whoever proposes the candidates: a person or a model.
    Refine {
        /// One domain's decision log (`phase0 --oracle-log`).
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        log: PathBuf,
        /// The candidates to weigh (the `phase0 --candidates` file the log
        /// was written with).
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        candidates: Option<PathBuf>,
        /// Keep a candidate only when it raises the held-out
        /// log-likelihood by more than this many nats (default: half the
        /// log of the decisions scored, what BIC charges a parameter).
        #[arg(help_heading = "Search", value_name = "NATS", long)]
        penalty: Option<f64>,
        /// Write examples from the sites where the arbiter is weakest here
        /// (Markdown), for a proposer.
        #[arg(
            help_heading = "Examples",
            value_name = "FILE",
            long,
            requires = "dump"
        )]
        examples: Option<PathBuf>,
        /// The requests `phase0 --oracle-dump` wrote in the same run, for
        /// each example's state.
        #[arg(help_heading = "Examples", value_name = "FILE", long)]
        dump: Option<PathBuf>,
        /// Sites to take examples from.
        #[arg(help_heading = "Examples", value_name = "N", long, default_value_t = 4)]
        sites: usize,
        /// Examples per site, one per task.
        #[arg(help_heading = "Examples", value_name = "N", long, default_value_t = 6)]
        per_site: usize,
        /// The domain, for the report's title.
        #[arg(
            help_heading = "Output",
            value_name = "NAME",
            long,
            default_value = "airline"
        )]
        domain: String,
        /// Write the Markdown here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write the search as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
    },
    /// Search a flow's settings (RFC-001 §3.10): NSGA-II, from fugue-evo,
    /// over each site's threshold (0.10 to 0.95, or off) and the decider,
    /// to save the most turns for the fewest detours. Each setting is scored
    /// by the replay command after `--`, run with `--flow FILE
    /// --flow-decider NAME --out DIR` added; it must print a `CHECK {…}`
    /// line of totals, as `pilot/check_flow.py` does. A decision the replay
    /// cannot answer, such as a question missing from a replay cache, hands
    /// back and makes a setting look safer than it is. The report counts
    /// them (unanswered), so let the replay ask what its cache lacks. The
    /// hand-set flows start the search: every site at 0.3 with the arbiter
    /// (D0), and with the habit alone at 0.3 and at 0.9 (arm C). With
    /// --rescore, replay an earlier search's hand-set settings and front on
    /// the command's episodes instead.
    Search {
        /// The flow whose settings to search.
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        flow: PathBuf,
        /// Search this site only (repeatable; default: every site where
        /// the flow may look something up).
        #[arg(help_heading = "Inputs", value_name = "SITE", long = "site")]
        sites: Vec<String>,
        /// Settings in each generation.
        #[arg(help_heading = "Search", value_name = "N", long, default_value_t = 16)]
        population: usize,
        /// Generations to breed.
        #[arg(help_heading = "Search", value_name = "N", long, default_value_t = 10)]
        generations: usize,
        /// Seed for the search.
        #[arg(help_heading = "Search", value_name = "N", long, default_value_t = 25)]
        seed: u64,
        /// Replay the hand-set settings and front of this earlier search
        /// (its --json) instead of searching.
        #[arg(help_heading = "Search", value_name = "FILE", long)]
        rescore: Option<PathBuf>,
        /// Where each setting's flow and replay go.
        #[arg(help_heading = "Output", value_name = "DIR", long)]
        dir: PathBuf,
        /// Write the Markdown here (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: Option<PathBuf>,
        /// Also write every setting replayed, and the front, as JSON here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        json: Option<PathBuf>,
        /// The replay command and its arguments, after `--`.
        #[arg(value_name = "COMMAND", last = true, required = true)]
        replay: Vec<String>,
    },
    /// Promote a flow's sites (RFC-001 §3.7). Wherever the flow would decide
    /// in recorded sessions or τ²-bench results, score the lookup it would
    /// make: used if the agent made it in a later LLM turn, a detour if it
    /// never did. The promoted flow acts only after the calls whose record
    /// meets the bar, and hands back after the rest.
    /// Sessions recorded with `stretto-proxy --flow-shadow` have the flow's
    /// questions answered in the proxy's cache: pass it as --oracle-cache.
    Promote {
        /// The flow to promote.
        #[arg(help_heading = "Inputs", value_name = "FILE", long)]
        flow: PathBuf,
        /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`);
        /// each counts as its own task.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        sessions: Option<PathBuf>,
        /// OpenTelemetry GenAI spans in place of --sessions: an OTLP JSON
        /// export, each trace a session (each its own task).
        #[arg(
            help_heading = "Inputs",
            value_name = "FILE",
            long,
            conflicts_with = "sessions"
        )]
        otel: Option<PathBuf>,
        /// τ²-bench results files (repeatable); files for other domains are
        /// skipped.
        #[arg(help_heading = "Inputs", value_name = "FILE", long = "results")]
        results: Vec<PathBuf>,
        /// With --results: keep only the test split of this τ²-bench
        /// checkout, the tasks a flow compiled from it never trained on.
        #[arg(help_heading = "Inputs", value_name = "DIR", long)]
        tau2: Option<PathBuf>,
        /// With --results: keep only these tasks.
        #[arg(
            help_heading = "Inputs",
            value_name = "IDS",
            long,
            value_delimiter = ','
        )]
        task_ids: Vec<String>,
        /// Who answers the flow's questions: `replay` (the cache only;
        /// decisions it cannot answer are left out), `jev` (needs
        /// TYPESAFE_API_KEY) or `mock`.
        #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(
            help_heading = "The System-One model",
            value_name = "DIR",
            long,
            default_value = ".oracle-cache"
        )]
        oracle_cache: PathBuf,
        /// How the flow decides: `arbiter`, `habit` or `reach`, as it will be
        /// served (`stretto-proxy --flow-decider`). Default: as the proxy
        /// serves it by default, with its arbiter, else `reach`, else, for a
        /// flow learned before flows held reach's counts, `habit`.
        #[arg(help_heading = "The System-One model", long, value_enum)]
        decider: Option<DeciderArg>,
        /// The threshold the flow will be served with (`stretto-proxy
        /// --flow-threshold`).
        #[arg(
            help_heading = "The bar",
            value_name = "P",
            long,
            default_value_t = 0.3
        )]
        threshold: f64,
        /// The least share of the flow's lookups at a site that the agent
        /// made in a later LLM turn.
        #[arg(
            help_heading = "The bar",
            value_name = "X",
            long,
            default_value_t = 0.7
        )]
        min_used: f64,
        /// The least lower bound on that share (Wilson, 90% two-sided).
        #[arg(
            help_heading = "The bar",
            value_name = "X",
            long,
            default_value_t = 0.5
        )]
        min_lower: f64,
        /// The fewest distinct tasks (or sessions) the lookups came from.
        #[arg(help_heading = "The bar", value_name = "N", long, default_value_t = 3)]
        min_tasks: usize,
        /// Lookups the proxy will make after one call, at most, as it will
        /// be served (`stretto-proxy --flow-per-call`): the flow decides
        /// again after each lookup the agent made later.
        #[arg(
            help_heading = "The bar",
            value_name = "N",
            long,
            default_value_t = stretto_report::flow::PER_CALL
        )]
        per_call: usize,
        /// Write the promoted flow here.
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        out: PathBuf,
        /// Write each site's record here, as Markdown (default: stdout).
        #[arg(help_heading = "Output", value_name = "FILE", long)]
        report: Option<PathBuf>,
    },
    /// Pseudonymize recorded sessions. A value that fewer than
    /// --keep-shared sessions contain becomes a salted hash, the same
    /// wherever it appears; a value more sessions share is kept, unless its
    /// field is named with --hash-field. A flow learned from the redacted
    /// logs matches one learned from the originals (docs/privacy.md).
    Redact {
        /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`).
        #[arg(value_name = "DIR", long)]
        sessions: PathBuf,
        /// Write the redacted logs here, one per session.
        #[arg(value_name = "DIR", long)]
        out: PathBuf,
        /// The environment variable holding the salt. Keep the salt secret,
        /// and the same for logs whose hashes should match.
        #[arg(value_name = "VAR", long, default_value = "STRETTO_REDACT_SALT")]
        salt_env: String,
        /// Keep a value that at least this many sessions share.
        #[arg(value_name = "N", long, default_value_t = 3)]
        keep_shared: usize,
        /// Hash every value of this field (a JSON key, at any depth of the
        /// arguments and results), and each word of it, wherever it
        /// appears, however many sessions share it: for the ids and names
        /// that a returning customer shares among their own sessions.
        /// Repeatable, or comma-separated.
        #[arg(value_name = "NAME", long = "hash-field", value_delimiter = ',')]
        hash_fields: Vec<String>,
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
        #[arg(value_name = "FILE", long)]
        flow: PathBuf,
        /// Where to write it.
        #[arg(value_name = "FILE", long)]
        out: PathBuf,
    },
    /// Fit one arbiter, to ship, on the held-out decisions of one or more
    /// compiles: each `--log` is a decision log from `compile --oracle-log`
    /// asked with the same predicates. With logs of several domains, the
    /// arbiter is fitted on all of them at once.
    FitArbiter {
        /// A decision log (repeatable).
        #[arg(long = "log", value_name = "FILE", required = true)]
        logs: Vec<PathBuf>,
        /// The predicates the logs' questions weighed (see
        /// `data/predicates-v2.json`).
        #[arg(long, value_name = "FILE")]
        predicates: PathBuf,
        /// The name to give its domain, such as `retail+airline`.
        #[arg(long, value_name = "NAME")]
        domain: String,
        /// The System-One model id it asks.
        #[arg(long, value_name = "MODEL", default_value = "jev-latest")]
        model: String,
        /// Where to write it.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },
    /// Write every cached oracle answer to stdout as JSON lines
    /// (`{"key", "response"}`). Answers carry no benchmark text, so the
    /// bundle can be shared to replay Phase 0b without a key.
    ExportAnswers {
        /// Replay cache to read.
        #[arg(value_name = "DIR", long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
    },
    /// Ask a System-One model questions of your own: JSON lines of requests
    /// (bare, or `{"key", "request"}` as `--oracle-dump` writes them),
    /// answered through the replay cache and written as `{"key",
    /// "response"}` lines, as `export-answers` writes them. For experiments
    /// that change the questions, such as text injected into their state.
    Ask {
        /// The requests (JSON lines).
        #[arg(long, value_name = "FILE")]
        requests: PathBuf,
        /// Who answers: `replay` (the cache only), `jev` (needs
        /// TYPESAFE_API_KEY; pays once per distinct question) or `mock`.
        #[arg(long, value_enum, default_value_t = OracleArg::Replay)]
        oracle: OracleArg,
        /// Replay cache for oracle answers.
        #[arg(long, value_name = "DIR", default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
        /// Refuse to start if uncached questions could cost more than this
        /// many dollars.
        #[arg(long, value_name = "DOLLARS", default_value_t = 1.0)]
        oracle_budget: f64,
        /// Requests in flight at once.
        #[arg(long, value_name = "N", default_value_t = 8)]
        oracle_concurrency: usize,
        /// Where to write the answers (JSON lines; a request that failed
        /// gets `"error"` in place of `"response"`).
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },
    /// Read JSON lines from `export-answers` on stdin into a replay cache.
    ImportAnswers {
        /// Replay cache to fill.
        #[arg(value_name = "DIR", long, default_value = ".oracle-cache")]
        oracle_cache: PathBuf,
    },
    /// Print the configuration that runs an MCP server behind
    /// `stretto-proxy` in an MCP host, recording its sessions, and the steps
    /// from there to a served flow: for Claude Code a `claude mcp add`
    /// command, for the other hosts their JSON. The configuration goes to
    /// stdout and the steps to stderr. Nothing is written without --write.
    Init(InitArgs),
    /// Check the installation: the versions of `stretto-proxy`,
    /// `stretto-procedure` and `stretto-mcp-demo` on PATH, whether the data
    /// directory (~/.stretto) is writable, whether TYPESAFE_API_KEY is set
    /// (never its value), and the flows and recorded sessions in the data
    /// directory. Exits with 1 when something needs fixing.
    Doctor {
        /// Also ask Jev one question (uncached) if a key is set, as
        /// `jev-check` does. Without it, doctor makes no network request.
        #[arg(long)]
        network: bool,
        /// The data directory to check, instead of ~/.stretto, as
        /// `stretto-console --data` serves it.
        #[arg(long, value_name = "DIR")]
        data: Option<PathBuf>,
    },
    /// Print the completion script for `stretto` in `bash`, `zsh`, `fish`,
    /// `powershell` or `elvish` to stdout. docs/install.md says where each
    /// shell reads it.
    Completions {
        /// The shell.
        #[arg(value_enum, value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

/// What `init` sets up.
#[derive(clap::Args)]
struct InitArgs {
    /// The MCP host to configure.
    #[arg(long, value_enum)]
    host: HostArg,
    /// The server's name in the host, which is also the domain of its
    /// sessions and flows (default, with --flow: the flow's).
    #[arg(long, value_name = "NAME", required_unless_present = "flow")]
    domain: Option<String>,
    /// Run this flow (from `stretto learn` or `stretto promote`) after the
    /// agent's calls. A flow without an arbiter is served on the chance of
    /// each lookup before the agent's next write (`--flow-decider reach`),
    /// or on its habit alone (`habit`) if it was learned before flows held
    /// those counts.
    #[arg(long, value_name = "FILE")]
    flow: Option<String>,
    /// Run the flow in shadow: it decides and logs, but looks nothing up,
    /// for `stretto promote`.
    #[arg(long, requires = "flow")]
    shadow: bool,
    /// Where the proxy records sessions (default: ~/.stretto/logs/NAME, or
    /// ~/.stretto/shadow/NAME with --shadow).
    #[arg(long, value_name = "DIR")]
    record: Option<String>,
    /// Write the host's configuration file here instead of printing it (for
    /// Claude Code, a project's .mcp.json). An existing file is left as it
    /// is, unless --force is given.
    #[arg(long, value_name = "PATH")]
    write: Option<PathBuf>,
    /// With --write, replace an existing file, with any other servers in it.
    #[arg(long, requires = "write")]
    force: bool,
    /// A Streamable HTTP server, such as `https://example.com/mcp`, in place
    /// of a server command: the proxy connects to it (`stretto-proxy
    /// --upstream`).
    #[arg(long, value_name = "URL", conflicts_with = "server")]
    upstream: Option<String>,
    /// With --upstream: send header NAME with the value of environment
    /// variable VAR, which the host gives the proxy in its `env`
    /// (`stretto-proxy --upstream-header`). The value is not written.
    #[arg(
        long = "upstream-header",
        value_name = "NAME=VAR",
        requires = "upstream"
    )]
    upstream_headers: Vec<String>,
    /// The MCP server's command and its arguments, after `--`.
    #[arg(
        value_name = "SERVER_COMMAND",
        last = true,
        required_unless_present = "upstream"
    )]
    server: Vec<String>,
}

/// What Phase 0 reads and how it measures, shared by `phase0` and
/// `flow-serve`.
/// `learn`'s arguments, apart from the other commands' so that parsing the
/// CLI builds them in a stack frame of their own.
#[derive(clap::Args)]
struct LearnArgs {
    /// Directory of session logs (`*.jsonl`); needed unless `--results`
    /// or `--otel` is given.
    #[arg(
        help_heading = "Inputs",
        value_name = "DIR",
        long,
        required_unless_present_any = ["results", "otel"]
    )]
    sessions: Option<PathBuf>,
    /// OpenTelemetry GenAI spans to learn from in place of `--sessions`:
    /// an OTLP JSON export, as the Collector's file exporter writes it,
    /// each trace one session. Spans do not say which tools only read,
    /// so `--manifest` gives the kinds. Without the tools' arguments and
    /// results, which the conventions capture only on request, the flow
    /// learns which lookups follow which calls but binds no arguments.
    #[arg(
        help_heading = "Inputs",
        value_name = "FILE",
        long,
        conflicts_with_all = ["sessions", "results"],
        requires = "manifest"
    )]
    otel: Option<PathBuf>,
    /// τ²-bench results to learn from in place of `--sessions`
    /// (repeatable): their episodes on the training tasks of the
    /// `--tau2` checkout's split, with their rewards. The tools come
    /// from the checkout.
    #[arg(help_heading = "Inputs", value_name = "FILE", 
        long = "results",
        requires = "tau2",
        conflicts_with_all = ["sessions", "manifest", "rewards"]
    )]
    results: Vec<PathBuf>,
    /// The τ²-bench checkout that `--results` belong to.
    #[arg(help_heading = "Inputs", value_name = "DIR", long)]
    tau2: Option<PathBuf>,
    /// With `--results`, learn from this share of the training tasks:
    /// the sample `compile --train-fraction` takes.
    #[arg(
        help_heading = "Inputs",
        value_name = "SHARE",
        long,
        default_value_t = 1.0
    )]
    train_fraction: f64,
    /// With `--results`, learn from these training tasks only
    /// (comma-separated), in place of `--train-fraction`.
    #[arg(
        help_heading = "Inputs",
        value_name = "IDS",
        long,
        value_delimiter = ',',
        conflicts_with = "train_fraction"
    )]
    train_tasks: Vec<String>,
    /// With `--results`, only these trials of each task (default: all).
    #[arg(help_heading = "Inputs", value_name = "N", long, num_args = 1..)]
    trials: Vec<u32>,
    /// Ask no System-One model: every session trains the habit, and the
    /// flow has no arbiter (serve it with `--flow-decider reach`, as
    /// `stretto init` does).
    #[arg(help_heading = "The arbiter", long)]
    habit_only: bool,
    /// Also pass, as the agent did, each argument it passed with one value
    /// in every call of a lookup, at least five, and in at least half of
    /// them, such as a page size, so that the flow's lookups are the
    /// agent's own calls.
    #[arg(help_heading = "The bindings", long)]
    constants: bool,
    /// Once the arbiter is fitted on the held-out sessions, learn the
    /// habit, the sites and the bindings again from every session.
    #[arg(help_heading = "The arbiter", long, conflicts_with = "habit_only")]
    refit_habit: bool,
    /// Forget old sessions: one this many sessions older than the newest
    /// counts half in the habit, one twice as old a quarter. Sessions are
    /// taken in the order they started (with `--results`, as listed).
    /// Relearn with it once `stretto drift` says the agent changed, to
    /// follow the change without discarding every earlier session.
    #[arg(help_heading = "The habit", value_name = "SESSIONS", long, value_parser = positive)]
    half_life: Option<f64>,
    /// Hand back for the rest of a session that surprises the flow. The
    /// threshold is this quantile of the most surprising run of the
    /// agent's steps (`--surprise-window` in a row) in each successful
    /// session, scored by a habit learned without the session's task:
    /// at 0.95, about one session like them in twenty trips it. `serve
    /// --surprise off` serves the flow without it.
    #[arg(help_heading = "Surprise", value_name = "Q", long, value_parser = probability)]
    surprise: Option<f64>,
    /// The agent's steps in a row whose surprise `--surprise` averages.
    #[arg(help_heading = "Surprise", value_name = "N", long, default_value_t = 5, requires = "surprise", value_parser = clap::value_parser!(u64).range(1..))]
    surprise_window: u64,
    /// Ask no System-One model while learning: every session trains the
    /// habit, and the flow serves this arbiter instead. It is an arbiter
    /// file (`export-arbiter`; `data/arbiters/` ships two), or a flow
    /// whose arbiter to take, such as one `compile` fitted on other
    /// agents' traces.
    #[arg(help_heading = "The arbiter", value_name = "FILE", long, conflicts_with_all = ["habit_only", "refit_habit"])]
    arbiter_from: Option<PathBuf>,
    /// Offer every read-only tool at every site (see `compile`). Only
    /// the arbiter a flow fits here weighs such options.
    #[arg(help_heading = "The arbiter", long, conflicts_with_all = ["habit_only", "arbiter_from"])]
    manifest_options: bool,
    /// The domain to name the flow for.
    #[arg(help_heading = "Inputs", value_name = "NAME", long)]
    domain: String,
    /// A tool manifest (JSON, as stretto-trace writes it) instead of the
    /// sessions' own `tools/list`.
    #[arg(help_heading = "Inputs", value_name = "FILE", long)]
    manifest: Option<PathBuf>,
    /// Rewards by session id (JSON object): a log's session, or, for the
    /// logs a host session left with several servers, the host session.
    /// Sessions without one count as successful.
    #[arg(help_heading = "Inputs", value_name = "FILE", long)]
    rewards: Option<PathBuf>,
    /// Who answers the held-out questions the arbiter is fitted on.
    /// `--habit-only` and `--arbiter-from` ask nothing, so they take none
    /// of the oracle's options.
    #[arg(help_heading = "The arbiter", long, value_enum, default_value_t = OracleArg::Jev, conflicts_with_all = ["habit_only", "arbiter_from"])]
    oracle: OracleArg,
    /// Replay cache for oracle answers.
    #[arg(help_heading = "The arbiter", value_name = "DIR", long, default_value = ".oracle-cache", conflicts_with_all = ["habit_only", "arbiter_from"])]
    oracle_cache: PathBuf,
    /// Refuse to start if uncached questions could cost more than this
    /// many dollars.
    #[arg(help_heading = "The arbiter", value_name = "DOLLARS", long, default_value_t = 1.0, conflicts_with_all = ["habit_only", "arbiter_from"])]
    oracle_budget: f64,
    /// Yes/no predicates to ask and weigh (see `data/predicates-v2.json`).
    /// An arbiter from `--arbiter-from` brings its own.
    #[arg(help_heading = "The arbiter", value_name = "FILE", long, conflicts_with_all = ["habit_only", "arbiter_from"])]
    predicates: Option<PathBuf>,
    /// Where to write the flow.
    #[arg(help_heading = "Output", value_name = "FILE", long)]
    out: PathBuf,
}

/// `stage`'s arguments, apart from the other commands' so that parsing the
/// CLI builds them in a stack frame of their own.
#[derive(clap::Args)]
struct StageArgs {
    /// The committed flow, `NAME.flow.json`. The staged flow is
    /// `NAME.staged.flow.json` beside it, and the staged learner's state
    /// `NAME.stage.json`. It need not exist yet: then give --domain.
    #[arg(help_heading = "Inputs", value_name = "FILE", long)]
    flow: PathBuf,
    /// Sessions recorded by stretto-proxy (a directory of `*.jsonl`),
    /// taken in the order they started.
    #[arg(
        help_heading = "Inputs",
        value_name = "DIR",
        long,
        required_unless_present_any = ["otel", "results"]
    )]
    sessions: Option<PathBuf>,
    /// OpenTelemetry GenAI spans in place of --sessions: an OTLP JSON
    /// export, each trace a session (`learn --otel`).
    #[arg(
        help_heading = "Inputs",
        value_name = "FILE",
        long,
        conflicts_with = "sessions",
        requires = "manifest"
    )]
    otel: Option<PathBuf>,
    /// τ²-bench results in place of --sessions (repeatable): their
    /// episodes, in the order listed, with their rewards. The tools come
    /// from --tau2's checkout, or --manifest.
    #[arg(
        help_heading = "Inputs",
        value_name = "FILE",
        long = "results",
        conflicts_with_all = ["sessions", "otel", "rewards"]
    )]
    results: Vec<PathBuf>,
    /// The τ²-bench checkout whose tools `--results` call.
    #[arg(
        help_heading = "Inputs",
        value_name = "DIR",
        long,
        requires = "results"
    )]
    tau2: Option<PathBuf>,
    /// The domain to name the flow for; needed only before there is a
    /// committed flow.
    #[arg(help_heading = "Inputs", value_name = "NAME", long)]
    domain: Option<String>,
    /// A tool manifest (JSON, as stretto-trace writes it) instead of the
    /// sessions' own `tools/list`.
    #[arg(help_heading = "Inputs", value_name = "FILE", long)]
    manifest: Option<PathBuf>,
    /// Rewards by session id (JSON object), as for `learn`. Sessions
    /// without one count as successful.
    #[arg(help_heading = "Inputs", value_name = "FILE", long)]
    rewards: Option<PathBuf>,
    /// Learn constant arguments, as `learn --constants`.
    #[arg(help_heading = "The staged flow", long)]
    constants: bool,
    /// Forget old sessions, as `learn --half-life`: one this many
    /// sessions older than the newest counts half.
    #[arg(help_heading = "The staged flow", value_name = "SESSIONS", long, value_parser = positive)]
    half_life: Option<f64>,
    /// Compare the flows on the last this many sessions that both were
    /// scored on.
    #[arg(
        help_heading = "The comparison",
        value_name = "N",
        long,
        default_value_t = 50
    )]
    window: usize,
    /// How both flows decide as they are scored: `arbiter`, `habit` or
    /// `reach`. Default: each as the proxy serves it by default.
    #[arg(help_heading = "The comparison", long, value_enum)]
    decider: Option<DeciderArg>,
    /// The threshold the flows are served with (`stretto-proxy
    /// --flow-threshold`).
    #[arg(
        help_heading = "The comparison",
        value_name = "P",
        long,
        default_value_t = 0.3
    )]
    threshold: f64,
    /// Lookups the proxy makes after one call, at most
    /// (`stretto-proxy --flow-per-call`).
    #[arg(
        help_heading = "The comparison",
        value_name = "N",
        long,
        default_value_t = stretto_report::flow::PER_CALL
    )]
    per_call: usize,
    /// Who answers an arbiter's questions: `replay` (the cache only;
    /// decisions it cannot answer are left out), `jev` (needs
    /// TYPESAFE_API_KEY) or `mock`.
    #[arg(help_heading = "The comparison", long, value_enum, default_value_t = OracleArg::Replay)]
    oracle: OracleArg,
    /// Replay cache for oracle answers.
    #[arg(
        help_heading = "The comparison",
        value_name = "DIR",
        long,
        default_value = ".oracle-cache"
    )]
    oracle_cache: PathBuf,
    /// Write the report here (Markdown; default: stdout).
    #[arg(help_heading = "Output", value_name = "FILE", long)]
    out: Option<PathBuf>,
    /// Also write the comparison as JSON here.
    #[arg(help_heading = "Output", value_name = "FILE", long)]
    json: Option<PathBuf>,
}

#[derive(clap::Args)]
struct Phase0Args {
    /// Path to a τ²-bench checkout.
    #[arg(help_heading = "Inputs", value_name = "DIR", long)]
    tau2: PathBuf,
    /// Domains to analyze.
    #[arg(help_heading = "Inputs", value_name = "NAME", long = "domain", default_values_t = ["retail".to_string(), "airline".to_string()])]
    domains: Vec<String>,
    /// Context length for coverage and transfer.
    #[arg(
        help_heading = "The habit",
        value_name = "N",
        long,
        default_value_t = 2
    )]
    order: usize,
    /// MH draws for the posterior over α (0 to use --alpha).
    #[arg(
        help_heading = "The habit",
        value_name = "N",
        long,
        default_value_t = 600
    )]
    alpha_samples: usize,
    /// α to use when --alpha-samples is 0.
    #[arg(help_heading = "The habit", long, default_value_t = 1.0)]
    alpha: f64,
    /// Training observations a context needs before the habit may act on it.
    #[arg(
        help_heading = "The habit",
        value_name = "N",
        long,
        default_value_t = 5.0
    )]
    min_evidence: f64,
    /// Seed for the MH chain.
    #[arg(
        help_heading = "The habit",
        value_name = "N",
        long,
        default_value_t = 7
    )]
    seed: u64,
    /// Skip learning code features from tool outputs.
    #[arg(help_heading = "The habit", long)]
    no_features: bool,
    /// Do not train on τ²-bench's published baselines in the checkout
    /// (then pass --source).
    #[arg(help_heading = "Inputs", long)]
    no_baselines: bool,
    /// Do not let flows know the episode's goal: the habit is not
    /// conditioned on it and System-One questions do not name it (for
    /// live flows nobody names).
    #[arg(help_heading = "The habit", long)]
    no_intent: bool,
    /// Extra τ²-bench results to train on: `path` or `label=path`
    /// (repeatable; files for other domains are skipped).
    #[arg(help_heading = "Inputs", value_name = "[LABEL=]PATH", long = "source")]
    sources: Vec<String>,
    /// τ²-bench results for an agent model the habit never trains on,
    /// measured as a transfer target: `path` or `label=path` (repeatable;
    /// files for other domains are skipped).
    #[arg(help_heading = "Inputs", value_name = "[LABEL=]PATH", long = "target")]
    targets: Vec<String>,
    /// Phase 0b: who answers the System-One questions. `jev` needs
    /// TYPESAFE_API_KEY and pays once per distinct question; `replay`
    /// reads the cache only; `mock` checks the pipeline for free. The other
    /// options under this heading need it.
    #[arg(help_heading = "Phase 0b: the System-One model", long, value_enum)]
    oracle: Option<OracleArg>,
    /// Which Phase 0b questions to ask: `v1` (one question over every
    /// tool, for flows that may call any tool) or `v2` (RFC-001 §3.5:
    /// read-only flows, a site's own lookups as options, a state slice,
    /// the stop decision asked on its own, and the answers combined with
    /// the habit).
    #[arg(help_heading = "Phase 0b: the System-One model", long, value_enum, default_value_t = QuestionArg::V1, requires = "oracle")]
    questions: QuestionArg,
    /// v2: also describe each lookup by what its results supply,
    /// learned from argument dataflow in training.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        long,
        requires = "oracle"
    )]
    dataflow_hints: bool,
    /// v2: a JSON file of yes/no predicates about the state (see
    /// `data/predicates-v2.json`) to ask with every next-step question and
    /// weigh in the arbiter.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "FILE",
        long,
        requires = "oracle"
    )]
    predicates: Option<PathBuf>,
    /// v2: ask the predicates but leave them out of the arbiter, to
    /// measure what they add.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        long,
        requires = "oracle"
    )]
    no_predicate_features: bool,
    /// v2, `phase0`: candidate predicates (same format as --predicates),
    /// each asked alone at every asked next-step decision with the same
    /// state, so the other answers keep their cache keys. Their answers
    /// go to --oracle-log, for `stretto refine`.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "FILE",
        long,
        requires = "oracle"
    )]
    candidates: Option<PathBuf>,
    /// v2, `phase0`: also weigh this candidate in the arbiter (repeatable).
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "ID",
        long = "weigh",
        requires = "candidates"
    )]
    weigh: Vec<String>,
    /// v2: offer every read-only tool at every site, not only the lookups
    /// seen there in training, for the System-One model to choose from. A
    /// lookup training never made is bound by argument name.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        long,
        requires = "oracle"
    )]
    manifest_options: bool,
    /// v2, `compile`: give the flow one arbiter, fitted on every held-out
    /// decision, in place of one per fold, so that `export-arbiter` can
    /// ship it. Replayed on the same test tasks it has seen other agents'
    /// decisions on them, so compare flows without it.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        long,
        requires = "oracle"
    )]
    pooled_arbiter: bool,
    /// Replay cache for oracle answers.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "DIR",
        long,
        default_value = ".oracle-cache",
        requires = "oracle"
    )]
    oracle_cache: PathBuf,
    /// Oracle requests in flight at once.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "N",
        long,
        default_value_t = 8,
        requires = "oracle"
    )]
    oracle_concurrency: usize,
    /// Ask at most this many distinct questions (a stable sample), for a
    /// pilot run.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "N",
        long,
        requires = "oracle"
    )]
    oracle_limit: Option<usize>,
    /// Refuse to start if uncached questions could cost more than this
    /// many dollars.
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "DOLLARS",
        long,
        default_value_t = 5.0,
        requires = "oracle"
    )]
    oracle_budget: f64,
    /// Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "MODEL",
        long,
        requires = "oracle"
    )]
    oracle_model: Option<String>,
    /// Write every distinct oracle request to this file (JSON lines; the
    /// domain is added to the file name).
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "FILE",
        long,
        requires = "oracle"
    )]
    oracle_dump: Option<PathBuf>,
    /// Write every decision, with the agent's option and the oracle's
    /// pick (with v2, also the features the arbiter weighs), to this file
    /// (JSON lines; the domain is added to the file name).
    #[arg(
        help_heading = "Phase 0b: the System-One model",
        value_name = "FILE",
        long,
        requires = "oracle"
    )]
    oracle_log: Option<PathBuf>,
    /// Train on this share of the training tasks: a fixed sample by task
    /// id, each smaller share part of every larger one. To see how flows
    /// do with fewer traces.
    #[arg(
        help_heading = "Inputs",
        value_name = "SHARE",
        long,
        default_value_t = 1.0
    )]
    train_fraction: f64,
    /// Train on these training tasks only (comma-separated), in place of
    /// `--train-fraction`: a sample named exactly.
    #[arg(
        help_heading = "Inputs",
        value_name = "IDS",
        long,
        value_delimiter = ',',
        conflicts_with = "train_fraction"
    )]
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
    Reach,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SecondArg {
    Described,
    Proposed,
}

#[derive(Clone, Copy, ValueEnum)]
enum HostArg {
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
    Vscode,
}

impl From<HostArg> for Host {
    fn from(h: HostArg) -> Self {
        match h {
            HostArg::ClaudeCode => Host::ClaudeCode,
            HostArg::ClaudeDesktop => Host::ClaudeDesktop,
            HostArg::Cursor => Host::Cursor,
            HostArg::Vscode => Host::VsCode,
        }
    }
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
            DeciderArg::Reach => Decider::Reach,
        }
    }
}

/// What a command reads and writes besides its arguments and files: the
/// process's own, or in tests a test's, so that every command runs in
/// process.
struct Env<'a> {
    /// What `import-answers` reads.
    stdin: &'a mut dyn BufRead,
    /// Where reports go without `--out`.
    stdout: &'a mut dyn Write,
    /// HOME, or USERPROFILE on Windows.
    home: Option<PathBuf>,
    /// PATH, where `init` and `doctor` look for the other binaries.
    path: OsString,
    /// This binary, which they may be next to.
    exe: Option<PathBuf>,
    /// Whether TYPESAFE_API_KEY and TYPESAFE_API_KEY_FILE are set, never
    /// what they hold.
    key: [bool; 2],
    /// Ask Jev one question: `jev-check`, and `doctor --network`.
    jev: fn() -> Result<String>,
}

impl<'a> Env<'a> {
    /// The process's own.
    fn process(stdin: &'a mut dyn BufRead, stdout: &'a mut dyn Write) -> Self {
        let set = |var: &str| std::env::var_os(var).is_some_and(|v| !v.is_empty());
        Env {
            stdin,
            stdout,
            home: home_dir(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            exe: std::env::current_exe().ok(),
            key: [set("TYPESAFE_API_KEY"), set("TYPESAFE_API_KEY_FILE")],
            jev: jev_check,
        }
    }
}

/// The CLI runs on a thread with an 8 MB stack, Linux's default for the main
/// thread. Windows gives its main thread 1 MB, which parsing this CLI's many
/// subcommands overflows in a debug build. A panic there panics here too,
/// as the scope ends.
fn main() -> Result<()> {
    let mut code = Ok(0);
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("stretto".to_string())
            .stack_size(8 << 20)
            .spawn_scoped(scope, || {
                let (mut stdin, mut stdout) = (std::io::stdin().lock(), std::io::stdout());
                code = run(Cli::parse(), &mut Env::process(&mut stdin, &mut stdout));
            })
            .map(drop)
    });
    std::process::exit(spawned.map_err(anyhow::Error::from).and(code)?)
}

/// Run a command, and return the exit code: 0, or 1 when `flow-diff`,
/// `drift` or `doctor` have something to report, or 2 when `flow-diff` or
/// `drift` fail. Other failures are the error.
fn run(cli: Cli, env: &mut Env) -> Result<i32> {
    let mut code = 0;
    let done = match cli.command {
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
                None => write!(env.stdout, "{md}")?,
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
            surprise,
            log,
            explore,
            explore_seed,
        } => {
            let domains = data.domains.clone();
            let [domain] = domains.as_slice() else {
                anyhow::bail!("flow-serve serves one domain: pass --domain once");
            };
            let config = phase0_config(data)?;
            let flow = compile(&config, domain)?.with_surprise_override(surprise);
            let oracle = config
                .shadow
                .as_ref()
                .expect("compile checked it")
                .build()?;
            let rule = Rule {
                threshold,
                decider: decider.into(),
                max_questions,
                explore: explore_arg(explore, explore_seed)?,
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
            make_parent(&out)?;
            flow.save(&out)?;
            eprintln!(
                "stretto: wrote the {} flow to {} ({} KB)",
                flow.domain(),
                out.display(),
                std::fs::metadata(&out)?.len() / 1024
            );
            Ok(())
        }
        Command::Learn(args) => learn(args),
        Command::Serve {
            flow,
            oracle,
            oracle_cache,
            listen,
            threshold,
            decider,
            max_questions,
            surprise,
            log,
            explore,
            explore_seed,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?.with_surprise_override(surprise);
            if decider == DeciderArg::Arbiter && !flow.has_arbiter() {
                anyhow::bail!(
                    "this flow was learned without a System-One model: serve it with --decider {}",
                    flow.served_decider().name()
                );
            }
            if decider == DeciderArg::Reach && !flow.has_reach() {
                anyhow::bail!(
                    "this flow was learned before flows counted what comes before the next write: \
                     learn it again for --decider reach"
                );
            }
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            let rule = Rule {
                threshold,
                decider: decider.into(),
                max_questions,
                explore: explore_arg(explore, explore_seed)?,
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
                None => write!(env.stdout, "{md}")?,
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
                None => write!(env.stdout, "{md}")?,
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
                None => write!(env.stdout, "{md}")?,
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
            otel,
            results,
            tau2,
            oracle,
            oracle_cache,
            decider,
            out,
            json,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            let domain = flow.domain().to_string();
            let episodes: Vec<stretto_trace::Episode> = recorded(
                &domain,
                (sessions.as_deref(), otel.as_deref()),
                &results,
                tau2.as_deref(),
                &[],
            )?
            .into_iter()
            .map(|r| r.episode)
            .collect();
            if episodes.is_empty() {
                anyhow::bail!(
                    "no episodes to audit: pass --sessions, --otel or --results for {domain}"
                );
            }
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            let started = Instant::now();
            let decider = decider.map_or(flow.default_decider(), Decider::from);
            if decider == Decider::Arbiter && !flow.has_arbiter() {
                anyhow::bail!("the flow has no arbiter: audit it with --decider habit");
            }
            if decider == Decider::Reach && !flow.has_reach() {
                anyhow::bail!("the flow has no counts for --decider reach: learn it again");
            }
            let audit =
                stretto_report::audit::audit_with(&flow, &episodes, oracle.as_ref(), decider);
            eprintln!(
                "stretto: audited {} decisions in {} episodes in {:.1} s",
                audit.decisions,
                audit.episodes,
                started.elapsed().as_secs_f64()
            );
            let md = stretto_report::audit::markdown(&audit);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => write!(env.stdout, "{md}")?,
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&audit)?)?;
            }
            Ok(())
        }
        Command::Drift {
            flow,
            sessions,
            otel,
            results,
            tau2,
            decider,
            oracle,
            oracle_cache,
            hazard,
            window,
            threshold,
            out,
            json,
        } => {
            let run = || -> Result<bool> {
                let flow = stretto_report::flow::Flow::load(&flow)?;
                let domain = flow.domain().to_string();
                // Benchmark runs first, as a flow is compiled from them before
                // it serves a deployment's sessions.
                let episodes: Vec<stretto_trace::Episode> =
                    recorded(&domain, (None, None), &results, tau2.as_deref(), &[])?
                        .into_iter()
                        .chain(recorded(
                            &domain,
                            (sessions.as_deref(), otel.as_deref()),
                            &[],
                            None,
                            &[],
                        )?)
                        .map(|r| r.episode)
                        .collect();
                let decider = Decider::from(decider);
                if decider == Decider::Arbiter && !flow.has_arbiter() {
                    anyhow::bail!("the flow has no arbiter: watch it with --decider habit");
                }
                if decider == Decider::Reach && !flow.has_reach() {
                    anyhow::bail!("the flow has no counts for --decider reach: learn it again");
                }
                let mut sc = ShadowConfig::new(oracle_kind(oracle));
                sc.cache_dir = oracle_cache;
                let oracle = sc.build()?;
                let settings = stretto_report::drift::Settings {
                    hazard,
                    window: window as usize,
                    threshold,
                };
                let d = stretto_report::drift::drift(
                    &flow,
                    &episodes,
                    oracle.as_ref(),
                    decider,
                    settings,
                );
                let md = stretto_report::drift::markdown(&d);
                match out {
                    Some(path) => write(&path, md.as_bytes())?,
                    None => write!(env.stdout, "{md}")?,
                }
                if let Some(path) = json {
                    write(&path, &serde_json::to_vec_pretty(&d)?)?;
                }
                if d.sounding {
                    eprintln!("stretto: the agent changed; the flow may no longer fit");
                }
                Ok(d.sounding)
            };
            code = match run() {
                Ok(found) => i32::from(found),
                Err(e) => {
                    eprintln!("stretto: {e:#}");
                    2
                }
            };
            Ok(())
        }
        Command::FlowShow {
            flow,
            threshold,
            out,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            let md = stretto_report::review::show(&flow, threshold);
            match out {
                Some(path) => write(&path, md.as_bytes()),
                None => Ok(write!(env.stdout, "{md}")?),
            }
        }
        Command::FlowDiff {
            old,
            new,
            tolerance,
            threshold,
            out,
        } => {
            // Like diff(1): 1 means "look at this", 2 means trouble.
            let run = || -> Result<bool> {
                let (a, b) = (
                    stretto_report::flow::Flow::load(&old)?,
                    stretto_report::flow::Flow::load(&new)?,
                );
                let d = stretto_report::review::diff(&a, &b, tolerance, threshold);
                match out {
                    Some(path) => write(&path, d.markdown.as_bytes())?,
                    None => write!(env.stdout, "{}", d.markdown)?,
                }
                if !d.needs_review.is_empty() {
                    eprintln!("stretto: {} change(s) need review", d.needs_review.len());
                }
                Ok(!d.needs_review.is_empty())
            };
            code = match run() {
                Ok(found) => i32::from(found),
                Err(e) => {
                    eprintln!("stretto: {e:#}");
                    2
                }
            };
            Ok(())
        }
        Command::Stage(args) => stage_flow(args, env),
        Command::FlowCommit { flow, note } => {
            use stretto_report::stage;
            let paths = stage::Paths::of(&flow)?;
            let evidence = stage::State::load(&paths.state)?.and_then(|s| s.last);
            let record = stage::commit(&paths, note, evidence, now_ms())?;
            let md =
                stage::log_markdown(&flow.display().to_string(), std::slice::from_ref(&record));
            write!(env.stdout, "{md}")?;
            eprintln!(
                "stretto: committed the staged flow as version {} of {}",
                record.version,
                flow.display()
            );
            Ok(())
        }
        Command::FlowRollback { flow, to, note } => {
            use stretto_report::stage;
            let paths = stage::Paths::of(&flow)?;
            let record = stage::rollback(&paths, to, note, now_ms())?;
            let md =
                stage::log_markdown(&flow.display().to_string(), std::slice::from_ref(&record));
            write!(env.stdout, "{md}")?;
            eprintln!(
                "stretto: rolled {} back to version {}, as version {}",
                flow.display(),
                record.restored.unwrap_or_default(),
                record.version
            );
            Ok(())
        }
        Command::FlowLog { flow } => {
            use stretto_report::stage;
            let paths = stage::Paths::of(&flow)?;
            let versions = stage::history(&paths)?;
            let mut md = stage::log_markdown(&flow.display().to_string(), &versions);
            if stage::unrecorded(&paths, &versions)? {
                md.push_str(
                    "The committed flow is not the latest version: it has changed since, by other means than a commit. The next commit or rollback keeps it as a version first.\n",
                );
            }
            write!(env.stdout, "{md}")?;
            Ok(())
        }
        Command::Search {
            flow,
            sites,
            population,
            generations,
            seed,
            rescore,
            dir,
            out,
            json,
            replay,
        } => {
            use stretto_report::search::{self, CommandReplay, Decider, Searched, Setting};
            let flow = stretto_report::flow::Flow::from_json(
                &std::fs::read_to_string(&flow)
                    .with_context(|| format!("reading {}", flow.display()))?,
            )?;
            let known = flow.sites();
            if let Some(s) = sites.iter().find(|s| !known.contains(s)) {
                anyhow::bail!("the flow makes no lookup after {s}; its sites are {known:?}");
            }
            let sites = if sites.is_empty() { known } else { sites };
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            let replay = CommandReplay::new(replay, dir);
            let (md, value) = match rescore {
                Some(path) => {
                    let earlier: Searched = serde_json::from_str(
                        &std::fs::read_to_string(&path)
                            .with_context(|| format!("reading {}", path.display()))?,
                    )
                    .with_context(|| format!("{}: not a search's --json", path.display()))?;
                    let settings: Vec<Setting> = earlier
                        .seeds
                        .iter()
                        .chain(&earlier.front)
                        .map(|s| s.setting.clone())
                        .collect();
                    let names: Vec<String> = earlier
                        .seeds
                        .iter()
                        .map(|s| s.setting.label())
                        .chain((1..=earlier.front.len()).map(|i| format!("F{i}")))
                        .collect();
                    let scored = search::rescore(&flow, &settings, &replay)?;
                    let md = format!(
                        "# Flow search, replayed again\n\nThe hand-set settings and the front of {}, \
                         replayed on this command's episodes.\n\n{}",
                        path.display(),
                        search::table(&scored, &earlier.sites, &names)
                    );
                    let value = serde_json::json!({"names": names, "scored": scored});
                    (md, value)
                }
                None => {
                    let seeds = vec![
                        Setting::uniform(&sites, 0.3, Decider::Arbiter),
                        Setting::uniform(&sites, 0.3, Decider::Habit),
                        Setting::uniform(&sites, 0.9, Decider::Habit),
                    ];
                    let r = search::search(
                        &flow,
                        &sites,
                        &seeds,
                        &replay,
                        population,
                        generations,
                        seed,
                    )?;
                    (search::markdown(&r), serde_json::to_value(&r)?)
                }
            };
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => write!(env.stdout, "{md}")?,
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&value)?)?;
            }
            Ok(())
        }
        Command::Refine {
            log,
            candidates,
            penalty,
            examples,
            dump,
            sites,
            per_site,
            domain,
            out,
            json,
        } => {
            use stretto_report::refine;
            let logged = refine::read_log(
                &std::fs::read_to_string(&log)
                    .with_context(|| format!("reading {}", log.display()))?,
            )?;
            if logged.is_empty() {
                anyhow::bail!(
                    "{}: no next-step decisions the arbiter judged (phase0 --questions v2 --oracle-log)",
                    log.display()
                );
            }
            let folds = stretto_model::features::FOLDS;
            if let Some(path) = examples {
                let dump = dump.expect("clap requires --dump");
                let states = refine::read_dump(
                    &std::fs::read_to_string(&dump)
                        .with_context(|| format!("reading {}", dump.display()))?,
                )?;
                std::fs::write(
                    &path,
                    refine::examples(&logged, &states, folds, sites, per_site),
                )
                .with_context(|| format!("writing {}", path.display()))?;
                eprintln!("stretto: wrote examples to {}", path.display());
            }
            let candidates = match candidates {
                Some(path) => {
                    let text = std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?;
                    serde_json::from_str::<PredicateFile>(&text)
                        .with_context(|| format!("parsing {}", path.display()))?
                        .predicates
                }
                None => Vec::new(),
            };
            let unanswered: Vec<&str> = candidates
                .iter()
                .filter(|c| !logged.iter().any(|d| d.predicates.contains_key(&c.id)))
                .map(|c| c.id.as_str())
                .collect();
            if !unanswered.is_empty() {
                anyhow::bail!(
                    "no answers in {} for {unanswered:?}: run phase0 with --candidates first",
                    log.display()
                );
            }
            let refined = refine::refine(&logged, &candidates, folds, penalty);
            let md = refine::markdown(&refined, &domain);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => write!(env.stdout, "{md}")?,
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&refined)?)?;
            }
            Ok(())
        }
        Command::Evaluate {
            decisions,
            targets,
            min_ess,
            out,
            json,
        } => {
            use stretto_report::evaluate::{evaluate, markdown, Record, Target};
            let targets = targets
                .iter()
                .map(|t| Target::parse(t).map_err(anyhow::Error::msg))
                .collect::<Result<Vec<_>>>()?;
            let mut records = Vec::new();
            for path in &decisions {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?;
                for (i, line) in text
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| !l.trim().is_empty())
                {
                    let record: Record = serde_json::from_str(line).with_context(|| {
                        format!("{}:{}: not a labelled decision", path.display(), i + 1)
                    })?;
                    if record.labels.len() != record.policy.options.len() {
                        anyhow::bail!(
                            "{}:{}: {} labels for {} options",
                            path.display(),
                            i + 1,
                            record.labels.len(),
                            record.policy.options.len()
                        );
                    }
                    records.push(record);
                }
            }
            if let Some(t) = targets.iter().find(|t| !t.habit) {
                if let Some(r) = records.iter().find(|r| r.policy.decider != "arbiter") {
                    anyhow::bail!(
                        "{} needs the arbiter's probabilities, but a decision was logged with the {}",
                        t.name,
                        r.policy.decider
                    );
                }
            }
            eprintln!("stretto: {} decisions", records.len());
            let evaluations: Vec<_> = targets
                .iter()
                .map(|t| evaluate(&records, t, min_ess))
                .collect();
            let md = markdown(&evaluations, min_ess);
            match out {
                Some(path) => write(&path, md.as_bytes())?,
                None => write!(env.stdout, "{md}")?,
            }
            if let Some(path) = json {
                write(&path, &serde_json::to_vec_pretty(&evaluations)?)?;
            }
            Ok(())
        }
        Command::Promote {
            flow,
            sessions,
            otel,
            results,
            tau2,
            task_ids,
            oracle,
            oracle_cache,
            decider,
            threshold,
            min_used,
            min_lower,
            min_tasks,
            per_call,
            out,
            report,
        } => {
            let flow = stretto_report::flow::Flow::load(&flow)?;
            let domain = flow.domain().to_string();
            let episodes = recorded(
                &domain,
                (sessions.as_deref(), otel.as_deref()),
                &results,
                tau2.as_deref(),
                &task_ids,
            )?;
            if episodes.is_empty() {
                anyhow::bail!(
                    "no episodes to score: pass --sessions, --otel or --results for {domain}"
                );
            }
            let mut sc = ShadowConfig::new(oracle_kind(oracle));
            sc.cache_dir = oracle_cache;
            let oracle = sc.build()?;
            let decider = decider.map_or(flow.served_decider(), Decider::from);
            if decider == Decider::Arbiter && !flow.has_arbiter() {
                anyhow::bail!(
                    "the flow has no arbiter: promote it with --decider {}",
                    flow.served_decider().name()
                );
            }
            if decider == Decider::Reach && !flow.has_reach() {
                anyhow::bail!("the flow has no counts for --decider reach: learn it again");
            }
            let bar = stretto_report::flow::Bar {
                threshold,
                min_used,
                min_lower,
                min_tasks,
            };
            let serving = stretto_report::promote::Serving {
                decider,
                threshold,
                per_call,
            };
            let (scored, promotion) = stretto_report::promote::promote_served(
                &flow,
                &episodes,
                oracle.as_ref(),
                serving,
                bar,
            );
            let promoted = promotion.sites.values().filter(|r| r.promoted).count();
            eprintln!(
                "stretto: scored {} episodes ({} decisions left out, unanswered); {promoted} of {} sites promoted",
                scored.episodes,
                scored.unanswered,
                promotion.sites.len()
            );
            if promoted == 0 {
                eprintln!(
                    "stretto: no site met the bar, so the promoted flow hands back after every call"
                );
            }
            let md = stretto_report::promote::markdown(&promotion);
            flow.with_promotion(Some(promotion)).save(&out)?;
            match report {
                Some(path) => write(&path, md.as_bytes()),
                None => Ok(write!(env.stdout, "{md}")?),
            }
        }
        Command::Redact {
            sessions,
            out,
            salt_env,
            keep_shared,
            hash_fields,
        } => {
            let salt = std::env::var(&salt_env).unwrap_or_default();
            if salt.is_empty() {
                anyhow::bail!(
                    "set {salt_env} to a secret salt: without one, anyone could hash guesses and match them"
                );
            }
            if out.canonicalize().ok() == Some(sessions.canonicalize()?) {
                anyhow::bail!(
                    "--out must be another directory: redact leaves the originals as they are"
                );
            }
            let logs = stretto_trace::mcp::read_sessions(&sessions)?;
            if logs.is_empty() {
                anyhow::bail!("no session logs in {}", sessions.display());
            }
            let redacted = stretto_trace::redact::redact(&logs, &salt, keep_shared, &hash_fields);
            std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
            let mut names = std::collections::HashSet::new();
            for log in &redacted {
                // The session id names the file, so it must not name a path.
                let name: String = log
                    .header
                    .session
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || "_-".contains(c) {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                if !names.insert(name.clone()) {
                    anyhow::bail!(
                        "two logs in {} share the session id {name}",
                        sessions.display()
                    );
                }
                write(
                    &out.join(format!("{name}.jsonl")),
                    log.to_jsonl().as_bytes(),
                )?;
            }
            eprintln!(
                "stretto: redacted {} sessions into {}; values fewer than {keep_shared} of them share are hashed",
                redacted.len(),
                out.display()
            );
            Ok(())
        }
        Command::JevCheck => Ok(writeln!(env.stdout, "{}", (env.jev)()?)?),
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
        Command::FitArbiter {
            logs,
            predicates,
            domain,
            model,
            out,
        } => {
            let text = std::fs::read_to_string(&predicates)
                .with_context(|| format!("reading {}", predicates.display()))?;
            let weighed = serde_json::from_str::<PredicateFile>(&text)
                .with_context(|| format!("parsing {}", predicates.display()))?
                .predicates;
            let mut cases = Vec::new();
            let mut sources = std::collections::BTreeSet::new();
            for path in &logs {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?;
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    let d: serde_json::Value = serde_json::from_str(line)
                        .with_context(|| format!("{}: not a decision log", path.display()))?;
                    let Some(case) = d.get("case").filter(|c| !c.is_null()) else {
                        continue;
                    };
                    let options: Vec<String> = serde_json::from_value(case["options"].clone())?;
                    let features: Vec<Vec<f64>> = serde_json::from_value(case["features"].clone())?;
                    if features.first().map_or(0, Vec::len) != 4 + weighed.len() {
                        anyhow::bail!(
                            "{}: the cases weigh {} predicates, not the {} in {}",
                            path.display(),
                            features.first().map_or(0, Vec::len).saturating_sub(4),
                            weighed.len(),
                            predicates.display()
                        );
                    }
                    let target = d["target"].as_bool().unwrap_or(false);
                    if let (false, Some(m)) = (target, d["model"].as_str()) {
                        sources.insert(m.to_string());
                    }
                    let actual = d["actual"].as_str().unwrap_or_default();
                    cases.push(stretto_report::arbitrate::Case {
                        group: 0,
                        site: case["site"].as_str().unwrap_or_default().to_string(),
                        features,
                        pick: case["pick"].as_u64().unwrap_or(0) as usize,
                        actual: options.iter().position(|o| o == actual),
                        fit: !target,
                    });
                }
            }
            if cases.is_empty() {
                anyhow::bail!("no v2 decisions in the logs: compile with --oracle-log");
            }
            let arbiter = Arbiter::fit(
                &domain,
                sources.into_iter().collect(),
                &cases,
                weighed.clone(),
                weighed,
                model,
            );
            arbiter.save(&out)?;
            eprintln!(
                "stretto: wrote the {domain} arbiter, fitted on {} held-out decisions, to {}",
                arbiter.provenance().arbiter_cases,
                out.display()
            );
            Ok(())
        }
        Command::ExportAnswers { oracle_cache } => {
            let cache: ReplayCache<MockOracle> = ReplayCache::new(oracle_cache, None);
            let mut lines = String::new();
            for (key, response) in cache.entries()? {
                lines += &serde_json::json!({"key": key, "response": response}).to_string();
                lines.push('\n');
            }
            Ok(env.stdout.write_all(lines.as_bytes())?)
        }
        Command::Ask {
            requests,
            oracle,
            oracle_cache,
            oracle_budget,
            oracle_concurrency,
            out,
        } => ask(
            &requests,
            oracle,
            oracle_cache,
            oracle_budget,
            oracle_concurrency,
            &out,
        ),
        Command::ImportAnswers { oracle_cache } => {
            let cache: ReplayCache<MockOracle> = ReplayCache::new(oracle_cache, None);
            let mut n = 0;
            for line in (&mut *env.stdin).lines() {
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
        Command::Init(args) => init(args, env),
        Command::Doctor { network, data } => doctor(network, data, env).map(|problems| {
            code = i32::from(problems);
        }),
        Command::Completions { shell } => {
            // Generated whole first: clap_complete panics on a failed write.
            let mut script = Vec::new();
            clap_complete::generate(shell, &mut Cli::command(), "stretto", &mut script);
            env.stdout.write_all(&script)?;
            Ok(())
        }
    };
    done.map(|()| code)
}

/// `stretto init`: the host's configuration, printed or written, then the
/// next steps.
fn init(args: InitArgs, env: &mut Env) -> Result<()> {
    use stretto_report::init::{self, Served, Setup};
    let flow = match &args.flow {
        Some(path) => {
            let path = host_path(path)?;
            let loaded =
                stretto_report::flow::Flow::load(&init::expand_home(&path, env.home.as_deref()))?;
            Some((path, loaded))
        }
        None => None,
    };
    // clap requires one or the other.
    let domain = match args.domain {
        Some(domain) => domain,
        None => flow
            .as_ref()
            .map(|(_, loaded)| loaded.domain().to_string())
            .context("pass --domain or --flow")?,
    };
    init::check_domain(&domain)?;
    let record = match &args.record {
        Some(dir) => host_path(dir)?,
        None => Setup::default_record(&domain, args.shadow),
    };
    // A host starts the server in a directory of its own choosing, so a
    // program named by a relative path gets its absolute one.
    let mut server = args.server;
    if let Some(program) = server.first_mut() {
        let path = Path::new(program.as_str());
        if path.is_relative() && path.components().count() > 1 {
            *program = std::path::absolute(path)?.display().to_string();
        }
    }
    let upstream = match args.upstream {
        Some(url) => {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                anyhow::bail!(
                    "--upstream {url:?}: the server's URL starts with http:// or https://"
                );
            }
            let mut headers = Vec::new();
            for spec in &args.upstream_headers {
                match spec.split_once('=') {
                    Some((name, var)) if !name.is_empty() && !var.is_empty() => {
                        headers.push((name.to_string(), var.to_string()))
                    }
                    _ => anyhow::bail!(
                        "--upstream-header takes NAME=VAR, the header and the environment \
                         variable holding its value"
                    ),
                }
            }
            Some(init::Upstream { url, headers })
        }
        None => None,
    };
    let host = Host::from(args.host);
    let setup = Setup {
        flow: flow.map(|(path, loaded)| Served {
            path,
            arbiter: loaded.has_arbiter(),
            reach: loaded.has_reach(),
            shadow: args.shadow,
            decide_with: None,
            threshold: None,
        }),
        domain,
        record,
        proxy: proxy_command(host, env),
        server,
        upstream,
    };
    match &args.write {
        Some(path) => {
            init::write_new(path, &init::config_text(host, &setup), args.force)?;
            eprintln!("stretto: wrote {}", path.display());
        }
        None => {
            write!(env.stdout, "{}", init::snippet(host, &setup))?;
            env.stdout.flush()?;
            eprintln!("\n{}", host.placement());
        }
    }
    let again = format!(
        "stretto init --host {} --domain {}",
        host.name(),
        setup.domain
    );
    eprintln!("\n{}", init::next_steps(&setup, &again));
    Ok(())
}

/// How the host should start `stretto-proxy`: by name when it is on PATH,
/// except for Claude Desktop, which starts servers with a minimal PATH and
/// so gets its full path, as does every host when the proxy is only next
/// to this binary.
fn proxy_command(host: Host, env: &Env) -> String {
    let name = "stretto-proxy";
    if let Some(found) = stretto_report::doctor::which(name, &env.path) {
        if host == Host::ClaudeDesktop {
            return found.display().to_string();
        }
        return name.to_string();
    }
    let beside = env.exe.as_ref().and_then(|exe| {
        let beside = exe
            .parent()?
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        beside.is_file().then_some(beside)
    });
    match beside {
        Some(found) => {
            eprintln!(
                "stretto: {name} is not on PATH, so the configuration names it by its full path"
            );
            found.display().to_string()
        }
        None => {
            eprintln!(
                "stretto: {name} is not on PATH; install it before the host starts the server \
                 (docs/install.md)"
            );
            name.to_string()
        }
    }
}

/// A path as a host should get it: a leading `~` is kept, for the proxy to
/// expand, and anything else is made absolute, since hosts start servers in
/// a directory of their own choosing.
fn host_path(path: &str) -> Result<String> {
    if path.starts_with('~') || Path::new(path).is_absolute() {
        return Ok(path.to_string());
    }
    Ok(std::path::absolute(path)
        .with_context(|| format!("resolving {path}"))?
        .display()
        .to_string())
}

/// `stretto learn`.
fn learn(args: LearnArgs) -> Result<()> {
    let LearnArgs {
        sessions,
        otel,
        results,
        tau2,
        train_fraction,
        train_tasks,
        trials,
        habit_only,
        constants,
        refit_habit,
        half_life,
        surprise,
        surprise_window,
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
    } = args;
    if !(train_fraction > 0.0 && train_fraction <= 1.0) {
        anyhow::bail!("--train-fraction must be in (0, 1]");
    }
    let conventions = otel
        .is_some()
        .then(|| stretto_trace::otel::CONVENTIONS.to_string());
    // Sessions or spans, which clap allows only without --results.
    let recorded = results.is_empty();
    if recorded && (train_fraction < 1.0 || !train_tasks.is_empty() || !trials.is_empty()) {
        anyhow::bail!("--train-fraction, --train-tasks and --trials apply to --results");
    }
    let rewards = read_rewards(rewards)?;
    let manifest = read_manifest(manifest)?;
    let (mut episodes, manifest, contracts) = match (sessions, otel) {
        (Some(sessions), _) => {
            let logs = sessions_in(&sessions)?;
            let manifest =
                manifest.unwrap_or_else(|| stretto_trace::mcp::manifest_of(&logs, &domain));
            let episodes = logs.iter().map(stretto_trace::mcp::episode).collect();
            (episodes, manifest, stretto_trace::mcp::contracts_of(&logs))
        }
        (None, Some(otel)) => {
            let manifest = manifest.context("--otel needs --manifest")?;
            (spans_in(&otel)?, manifest, BTreeMap::new())
        }
        (None, None) => {
            let tau2 = tau2.context("--results needs --tau2")?;
            let (episodes, manifest) = tau2_sessions(
                &tau2,
                &domain,
                &results,
                &train_tasks,
                train_fraction,
                &trials,
            )?;
            (episodes, manifest, BTreeMap::new())
        }
    };
    if recorded {
        for ep in &mut episodes {
            ep.domain = domain.clone();
            ep.reward = rewards.get(&ep.id).copied().unwrap_or(1.0);
        }
    }
    let mut config = phase0::Config::new(PathBuf::new());
    config.domains = vec![domain.clone()];
    config.refit_habit = refit_habit;
    config.constants = constants;
    config.half_life = half_life;
    let flow = if let Some(path) = arbiter_from {
        let habit = phase0::compile_habit_flow_from_episodes(&config, &episodes, &manifest)?;
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
                serde_json::from_str::<PredicateFile>(&std::fs::read_to_string(&path)?)?.predicates;
        }
        let oracle = sc.build()?;
        config.shadow = Some(sc);
        phase0::compile_flow_from_episodes(&config, &episodes, &manifest, oracle.as_ref())?
    };
    let gate = match surprise {
        Some(q) => Some(stretto_report::surprise::learn(
            &config,
            &episodes,
            &manifest,
            q,
            surprise_window as usize,
        )?),
        None => None,
    };
    if let Some(gate) = &gate {
        eprintln!("stretto: the flow {}", gate.describe());
    }
    let flow = flow
        .with_contracts(contracts)
        .with_conventions(conventions)
        .with_surprise(gate);
    flow.save(&out)?;
    eprintln!(
        "stretto: learned the {domain} flow from {} sessions ({} tools) and wrote {}",
        episodes.len(),
        manifest.tools.len(),
        out.display()
    );
    Ok(())
}

/// `stretto stage`: learn the staged flow beside the committed one, and
/// report how the two compare.
fn stage_flow(args: StageArgs, env: &mut Env) -> Result<()> {
    use stretto_report::flow::Flow;
    use stretto_report::promote::{Recorded, Serving};
    use stretto_report::stage;
    let StageArgs {
        flow,
        sessions,
        otel,
        results,
        tau2,
        domain,
        manifest,
        rewards,
        constants,
        half_life,
        window,
        decider,
        threshold,
        per_call,
        oracle,
        oracle_cache,
        out,
        json,
    } = args;
    let paths = stage::Paths::of(&flow)?;
    let load = |path: &Path| -> Result<Option<Flow>> {
        match path.exists() {
            true => Ok(Some(Flow::load(path)?)),
            false => Ok(None),
        }
    };
    let committed = load(&paths.committed)?;
    let staged_before = load(&paths.staged)?;
    let domain = match (&committed, domain) {
        (Some(f), Some(d)) if f.domain() != d => {
            anyhow::bail!("the committed flow is for {}, not {d}", f.domain())
        }
        (Some(f), _) => f.domain().to_string(),
        (None, Some(d)) => d,
        (None, None) => anyhow::bail!(
            "there is no committed flow at {} yet: name the domain with --domain",
            paths.committed.display()
        ),
    };
    let mut state = match stage::State::load(&paths.state)? {
        Some(s) if s.domain != domain => anyhow::bail!(
            "{} is the staged {} flow's state, not {domain}'s",
            paths.state.display(),
            s.domain
        ),
        Some(s) => s,
        None => stage::State::new(&domain),
    };
    let rewards = read_rewards(rewards)?;
    let manifest = read_manifest(manifest)?;
    let conventions = otel
        .is_some()
        .then(|| stretto_trace::otel::CONVENTIONS.to_string());
    // The stream of sessions, in order.
    let (mut stream, manifest, contracts) = match (sessions, otel) {
        (Some(dir), _) => {
            let logs = sessions_in(&dir)?;
            let manifest =
                manifest.unwrap_or_else(|| stretto_trace::mcp::manifest_of(&logs, &domain));
            let stream: Vec<Recorded> = logs
                .iter()
                .map(|log| {
                    let (episode, sent_after) = stretto_trace::mcp::episode_sent(log);
                    Recorded {
                        episode,
                        sent_after,
                    }
                })
                .collect();
            (stream, manifest, stretto_trace::mcp::contracts_of(&logs))
        }
        (None, Some(path)) => {
            let manifest = manifest.context("--otel needs --manifest")?;
            let stream = spans_in(&path)?.into_iter().map(Recorded::from).collect();
            (stream, manifest, BTreeMap::new())
        }
        (None, None) => {
            let manifest = match (manifest, &tau2) {
                (Some(m), _) => m,
                (None, Some(root)) => stretto_trace::tau2::load_manifest(
                    &domain,
                    &root.join(format!("src/tau2/domains/{domain}/tools.py")),
                )?,
                (None, None) => {
                    anyhow::bail!("--results needs --tau2, for the tools, or --manifest")
                }
            };
            let mut stream = Vec::new();
            for path in &results {
                let run = stretto_trace::tau2::load_results(path)
                    .with_context(|| format!("reading {}", path.display()))?;
                if run.domain == domain {
                    stream.extend(run.episodes.into_iter().map(Recorded::from));
                }
            }
            (stream, manifest, BTreeMap::new())
        }
    };
    if stream.is_empty() {
        anyhow::bail!("there are no {domain} sessions to learn from");
    }
    for r in &mut stream {
        r.episode.domain = domain.clone();
        if results.is_empty() {
            r.episode.reward = rewards.get(&r.episode.id).copied().unwrap_or(1.0);
        }
    }
    // Each new session, scored before the staged flow learns from it.
    let mut sc = ShadowConfig::new(oracle_kind(oracle));
    sc.cache_dir = oracle_cache;
    let oracle = sc.build()?;
    let serving = |f: &Flow| Serving {
        decider: decider.map_or(f.served_decider(), Decider::from),
        threshold,
        per_call,
    };
    let mut known: HashSet<String> = state.sessions.iter().cloned().collect();
    let mut new = 0;
    for r in &stream {
        if !known.insert(r.episode.id.clone()) {
            continue;
        }
        let flows = [committed.as_ref(), staged_before.as_ref()];
        state.record(stage::score(flows, r, oracle.as_ref(), serving));
        state.sessions.push(r.episode.id.clone());
        new += 1;
    }
    // The staged flow, learned again from every session so far.
    let staged = match staged_before {
        Some(flow) if new == 0 => flow,
        _ => {
            let episodes: Vec<stretto_trace::Episode> =
                stream.iter().map(|r| r.episode.clone()).collect();
            let mut config = phase0::Config::new(PathBuf::new());
            config.domains = vec![domain.clone()];
            config.constants = constants;
            config.half_life = half_life;
            let mut flow = phase0::compile_habit_flow_from_episodes(&config, &episodes, &manifest)?
                .with_contracts(contracts)
                .with_conventions(conventions);
            if let Some(c) = &committed {
                if c.has_arbiter() {
                    flow = flow.with_arbiter_of(c.clone())?;
                }
                flow = flow
                    .with_promotion(c.promotion().cloned())
                    .with_thresholds(c.thresholds().clone());
                let gate = surprise::again(c.surprise(), &config, &episodes, &manifest)?;
                flow = flow.with_surprise(gate);
            }
            stage::write_atomic(&paths.staged, &serde_json::to_vec(&flow)?)?;
            flow
        }
    };
    // What the staged flow holds that it did not learn: the committed
    // flow's, when it was learned.
    let mut carried: Vec<String> = Vec::new();
    if staged.has_arbiter() {
        carried.push("its arbiter".to_string());
    }
    if let Some(p) = staged.promotion() {
        let promoted = p.sites.values().filter(|r| r.promoted).count();
        carried.push(format!(
            "its promotion (it acts at {promoted} of {} sites)",
            p.sites.len()
        ));
    }
    if !staged.thresholds().is_empty() {
        carried.push(format!(
            "its thresholds at {} sites",
            staged.thresholds().len()
        ));
    }
    if staged.surprise().is_some_and(|g| g.quantile.is_none()) {
        carried.push("its surprise gate, set by hand".to_string());
    }
    let mut cmp = stage::compare(&state, window, committed.is_some());
    cmp.new = new;
    cmp.learned_unix_ms = staged.provenance().compiled_unix_ms;
    cmp.carried = carried;
    if let Some(c) = &committed {
        let d = stretto_report::review::diff(c, &staged, 0.05, threshold);
        cmp.changes = d.sections;
        cmp.needs_review = d.needs_review;
    }
    state.last = Some(cmp.clone());
    state.save(&paths.state)?;
    let md = cmp.markdown(&flow.display().to_string());
    match out {
        Some(path) => write(&path, md.as_bytes())?,
        None => write!(env.stdout, "{md}")?,
    }
    if let Some(path) = json {
        write(&path, &serde_json::to_vec_pretty(&cmp)?)?;
    }
    eprintln!(
        "stretto: the staged {domain} flow has learned from {} sessions, {new} of them new, and is at {}",
        state.sessions.len(),
        paths.staged.display()
    );
    Ok(())
}

/// Rewards by session id, from a JSON object in `path`; none without one.
fn read_rewards(path: Option<PathBuf>) -> Result<BTreeMap<String, f64>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// A tool manifest from `path`, if one was given.
fn read_manifest(path: Option<PathBuf>) -> Result<Option<stretto_trace::ToolManifest>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The home directory: HOME, or USERPROFILE on Windows.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// `stretto doctor`: the report on `data` (default: ~/.stretto), then, with
/// `network`, Jev's answer. Whether it found a problem.
fn doctor(network: bool, data: Option<PathBuf>, env: &mut Env) -> Result<bool> {
    use stretto_report::doctor::{self, Report};
    let version = env!("CARGO_PKG_VERSION");
    let exe = env.exe.as_ref();
    let at = exe.map(|exe| format!(" ({})", exe.display()));
    writeln!(env.stdout, "stretto {version}{}\n", at.unwrap_or_default())?;
    let mut report = Report::default();
    doctor::check_binaries(
        &mut report,
        version,
        &env.path,
        exe.and_then(|exe| exe.parent()),
    );
    let dir = data.or_else(|| env.home.as_ref().map(|h| h.join(".stretto")));
    match &dir {
        Some(dir) => doctor::check_dir(&mut report, dir),
        None => report
            .problem("neither HOME nor USERPROFILE is set, so there is no ~/.stretto: pass --data"),
    }
    let [key, key_file] = env.key;
    doctor::check_key(&mut report, key, key_file);
    if let Some(dir) = &dir {
        doctor::check_files(&mut report, dir, env.home.as_deref());
    }
    if network {
        if !(key || key_file) {
            report.note("--network: no key is set, so Jev was not asked");
        } else {
            match (env.jev)() {
                Ok(answer) => report.ok(answer),
                Err(e) => report.problem(format!("Jev did not answer: {e:#}")),
            }
        }
    }
    write!(env.stdout, "{}", report.render())?;
    if report.problems() > 0 {
        eprintln!("stretto: {} problem(s) to fix", report.problems());
    }
    Ok(report.problems() > 0)
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
    let mut manifest = load_manifest(
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
    // In a solo run (τ²-bench's `no-user` mode) the agent itself calls the
    // customer's tools, telecom's phone: they are the agent's tools then.
    // An agent that calls one when the customer holds the phone is told the
    // tool does not exist, so a solo run is one where such a call returned.
    let user_tools = tau2.join(format!("src/tau2/domains/{domain}/user_tools.py"));
    if user_tools.exists() {
        let user = load_manifest(domain, &user_tools)?;
        let solo = episodes.iter().any(|ep| {
            let mut asked: HashSet<&str> = HashSet::new();
            ep.events.iter().any(|e| match e {
                stretto_trace::Event::Assistant { calls, .. } => {
                    asked.extend(
                        calls
                            .iter()
                            .filter(|c| user.tools.contains_key(&c.name))
                            .map(|c| c.id.as_str()),
                    );
                    false
                }
                stretto_trace::Event::ToolResult {
                    call_id,
                    error: false,
                    ..
                } => asked.contains(call_id.as_str()),
                _ => false,
            })
        });
        if solo {
            manifest.tools.extend(user.tools);
            manifest.docs.extend(user.docs);
        }
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
        candidates,
        weigh,
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
    let candidates = match candidates {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str::<PredicateFile>(&text)
                .with_context(|| format!("parsing {}", path.display()))?
                .predicates
        }
        None => Vec::new(),
    };
    if let Some(c) = candidates
        .iter()
        .find(|c| predicates.iter().any(|p| p.id == c.id))
    {
        anyhow::bail!("candidate `{}` is already a predicate", c.id);
    }
    if let Some(w) = weigh
        .iter()
        .find(|w| !candidates.iter().any(|c| c.id == **w))
    {
        anyhow::bail!("--weigh {w}: no such candidate");
    }
    if !candidates.is_empty() && questions != QuestionArg::V2 {
        anyhow::bail!("--candidates needs --questions v2");
    }
    config.shadow = oracle.map(|kind| {
        let mut sc = ShadowConfig::new(oracle_kind(kind));
        sc.cache_dir = oracle_cache;
        sc.hints = dataflow_hints;
        sc.predicates = predicates.clone();
        sc.predicate_features = !no_predicate_features;
        sc.candidates = candidates.clone();
        sc.weigh = weigh.clone();
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

/// `stretto ask`: answer every request in `requests`, `concurrency` at a
/// time, and write the answers to `out` in the order asked.
fn ask(
    requests: &Path,
    oracle: OracleArg,
    oracle_cache: PathBuf,
    budget: f64,
    concurrency: usize,
    out: &Path,
) -> Result<()> {
    let text = std::fs::read_to_string(requests)
        .with_context(|| format!("reading {}", requests.display()))?;
    let mut todo: Vec<(String, Request)> = Vec::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let value: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("{} line {}", requests.display(), n + 1))?;
        let request: Request = match value.get("request") {
            Some(r) => serde_json::from_value(r.clone()),
            None => serde_json::from_value(value),
        }
        .with_context(|| format!("{} line {}: not a request", requests.display(), n + 1))?;
        todo.push((stretto_oracle::request_key(&request), request));
    }
    let tokens: u64 = todo
        .iter()
        .map(|(_, r)| stretto_report::shadow::estimate_tokens(r))
        .sum();
    let dollars = tokens as f64 / 1e6 * stretto_report::shadow::PRICE_PER_MTOK;
    eprintln!(
        "stretto: {} questions, about {:.2}M input tokens, at most ${dollars:.2} at Jev's price \
         before cache hits",
        todo.len(),
        tokens as f64 / 1e6
    );
    if matches!(oracle, OracleArg::Jev) && dollars > budget {
        anyhow::bail!(
            "estimated cost ${dollars:.2} exceeds the budget of ${budget:.2}; raise --oracle-budget"
        );
    }
    let mut sc = ShadowConfig::new(oracle_kind(oracle));
    sc.cache_dir = oracle_cache;
    let oracle = sc.build()?;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Result<stretto_oracle::Response>>>> =
        todo.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..concurrency.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((_, request)) = todo.get(i) else {
                    break;
                };
                let answer = oracle.ask(request);
                *slots[i].lock().expect("no thread panics holding a slot") = Some(answer);
            });
        }
    });
    let (mut lines, mut failed) = (String::new(), 0);
    for ((key, _), slot) in todo.iter().zip(slots) {
        let answer = slot.into_inner().expect("no thread panics holding a slot");
        let line = match answer.expect("every request is asked") {
            Ok(response) => serde_json::json!({"key": key, "response": response}),
            Err(e) => {
                failed += 1;
                serde_json::json!({"key": key, "error": format!("{e:#}")})
            }
        };
        lines.push_str(&line.to_string());
        lines.push('\n');
    }
    write(&out.to_path_buf(), lines.as_bytes())?;
    eprintln!(
        "stretto: answered {} of {} questions",
        todo.len() - failed,
        todo.len()
    );
    Ok(())
}

/// The sessions recorded in `dir`: its session logs, with the logs of each
/// host session that spans several servers merged into one
/// ([`stretto_trace::mcp::merge`]), as it says on stderr.
fn sessions_in(dir: &Path) -> Result<Vec<stretto_trace::mcp::McpLog>> {
    let sessions = stretto_trace::mcp::merge(stretto_trace::mcp::read_sessions(dir)?);
    if sessions.merged_sessions > 0 {
        eprintln!(
            "stretto: merged {} logs of several servers into {} sessions, each tool named \
             after its server (server::tool)",
            sessions.merged_logs, sessions.merged_sessions
        );
    }
    for host in &sessions.apart {
        eprintln!(
            "stretto: kept the logs of host session {host} apart, as two servers of the same \
             name ran in it at once; name each server with stretto-proxy --server-name, or give \
             each session its own STRETTO_SESSION"
        );
    }
    Ok(sessions.logs)
}

/// The traces of an OTLP JSON export of OpenTelemetry GenAI spans
/// ([`stretto_trace::otel`]), as episodes, saying on stderr when the spans
/// lack the tools' arguments or results, which bind a lookup's arguments.
fn spans_in(path: &Path) -> Result<Vec<stretto_trace::Episode>> {
    let run = stretto_trace::otel::read_otel(path)?;
    let [calls, arguments, results] = run.content;
    if arguments < calls || results < calls {
        eprintln!(
            "stretto: of the spans' {calls} tool calls, {arguments} carry their arguments and \
             {results} their results, which OpenTelemetry's conventions capture only on request; \
             the flow binds no argument it did not see"
        );
    }
    Ok(run.episodes)
}

/// Recorded episodes for a flow of `domain`: the proxy's sessions in
/// `sessions`, or the traces of the OpenTelemetry spans in `otel` (each its
/// own task), and the episodes of `results` for the
/// domain, only the test split of `tau2` if given, and only `task_ids` if
/// any are.
fn recorded(
    domain: &str,
    (sessions, otel): (Option<&Path>, Option<&Path>),
    results: &[PathBuf],
    tau2: Option<&Path>,
    task_ids: &[String],
) -> Result<Vec<stretto_report::promote::Recorded>> {
    let mut episodes = Vec::new();
    if let Some(dir) = sessions {
        for log in sessions_in(dir)? {
            let (mut episode, sent_after) = stretto_trace::mcp::episode_sent(&log);
            episode.task_id = episode.id.clone();
            episodes.push(stretto_report::promote::Recorded {
                episode,
                sent_after,
            });
        }
    }
    if let Some(path) = otel {
        for mut episode in spans_in(path)? {
            episode.task_id = episode.id.clone();
            episodes.push(stretto_report::promote::Recorded::from(episode));
        }
    }
    let test = match tau2 {
        Some(root) => Some(
            stretto_trace::tau2::load_split(
                &root.join(format!("data/tau2/domains/{domain}/split_tasks.json")),
            )?
            .test,
        ),
        None => None,
    };
    for path in results {
        let run = stretto_trace::tau2::load_results(path)?;
        if run.domain != domain {
            continue;
        }
        episodes.extend(
            run.episodes
                .into_iter()
                .filter(|ep| {
                    test.as_ref().is_none_or(|t| t.contains(&ep.task_id))
                        && (task_ids.is_empty() || task_ids.contains(&ep.task_id))
                })
                .map(stretto_report::promote::Recorded::from),
        );
    }
    Ok(episodes)
}

/// A number above zero, as an option takes it.
fn positive(arg: &str) -> std::result::Result<f64, String> {
    match arg.parse::<f64>() {
        Ok(x) if x > 0.0 && x.is_finite() => Ok(x),
        _ => Err(format!("{arg} is not a number above zero")),
    }
}

/// A probability above zero and at most one, as an option takes it.
fn probability(arg: &str) -> std::result::Result<f64, String> {
    match arg.parse::<f64>() {
        Ok(p) if p > 0.0 && p <= 1.0 => Ok(p),
        _ => Err(format!("{arg} is not a probability above 0 and at most 1")),
    }
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
    if !sc.candidates.is_empty() {
        anyhow::bail!(
            "--candidates is for phase0 and refine: a flow asks its predicates with each \
             next-step question, so add a kept candidate to --predicates"
        );
    }
    let mut offline = config.clone();
    if let Some(sc) = offline.shadow.as_mut() {
        sc.oracle = OracleKind::Replay;
    }
    let cached = offline.shadow.as_ref().expect("checked above").build()?;
    let start = Instant::now();
    let flow = phase0::compile_flow(&offline, domain, cached.as_ref())?;
    eprintln!(
        "stretto: compiled the {} flow in {:.1} s",
        flow.domain(),
        start.elapsed().as_secs_f64()
    );
    eprint!("{}", agreement(&flow));
    Ok(flow)
}

/// How often each lookup's binding picked the agent's own arguments in
/// training, a line per lookup.
fn agreement(flow: &stretto_report::flow::Flow) -> String {
    let mut lines = String::new();
    for (tool, [not, named]) in flow.binding_agreement() {
        lines += &format!(
            "stretto: binding {tool}: agreed {}/{} unmentioned, {}/{} mentioned\n",
            not.0, not.1, named.0, named.1
        );
    }
    lines
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
    /// Exploration, if any.
    explore: Option<stretto_report::flow::Explore>,
}

/// `--explore` and `--explore-seed` as an [`Explore`](stretto_report::flow::Explore).
fn explore_arg(epsilon: Option<f64>, seed: u64) -> Result<Option<stretto_report::flow::Explore>> {
    match epsilon {
        Some(e) if !(0.0..=1.0).contains(&e) => anyhow::bail!("--explore must be in [0, 1]"),
        Some(epsilon) => Ok(Some(stretto_report::flow::Explore { epsilon, seed })),
        None => Ok(None),
    }
}

/// Answer one flow query per connection on `listen`, asking `oracle`.
fn serve(
    flow: &stretto_report::flow::Flow,
    oracle: &(dyn Oracle + Sync),
    listen: &str,
    rule: Rule,
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
    loop {
        let (mut stream, _) = listener.accept()?;
        let mut line = String::new();
        std::io::BufReader::new(&stream).read_line(&mut line)?;
        let started = Instant::now();
        let answer = answer_query(flow, oracle, &line, &rule, &mut asked)?;
        let mut reply = answer.to_string();
        reply.push('\n');
        stream.write_all(reply.as_bytes())?;
        if let Some(f) = log.as_mut() {
            let mut entry = answer;
            entry["ms"] = serde_json::json!(started.elapsed().as_millis() as u64);
            writeln!(f, "{entry}")?;
        }
    }
}

/// The answer to one query to `flow-serve`, `line`: the flow's next step
/// after the conversation it holds, or a hand-back that says why not.
/// `asked` counts the questions asked of the System-One model so far.
fn answer_query(
    flow: &stretto_report::flow::Flow,
    oracle: &(dyn Oracle + Sync),
    line: &str,
    rule: &Rule,
    asked: &mut usize,
) -> Result<serde_json::Value> {
    let query = match serde_json::from_str::<FlowQuery>(line) {
        Err(e) => {
            return Ok(
                serde_json::json!({"action": "hand_back", "reason": format!("bad query: {e}")}),
            )
        }
        Ok(_) if *asked >= rule.max_questions => {
            return Ok(serde_json::json!({
                "action": "hand_back",
                "reason": format!("the flow's {} questions are spent", rule.max_questions),
            }))
        }
        Ok(query) => query,
    };
    let sim = serde_json::json!({
        "id": "live",
        "task_id": query.task_id,
        "messages": query.messages,
    });
    let next =
        stretto_trace::tau2::parse_simulation(&sim.to_string(), flow.domain(), &query.agent_model)
            .and_then(|episode| {
                flow.next_explored(
                    &episode,
                    &[],
                    oracle,
                    rule.threshold,
                    rule.decider,
                    rule.explore,
                )
            });
    Ok(match next {
        Ok(next) => {
            *asked += next.key.is_some() as usize;
            serde_json::to_value(&next)?
        }
        Err(e) => serde_json::json!({
            "action": "hand_back",
            "reason": format!("error: {e:#}"),
        }),
    })
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

/// Ask Jev one small question, uncached, and say how it answered.
fn jev_check() -> Result<String> {
    ask_jev(&stretto_oracle::jev::JevClient::from_env()?)
}

/// Ask `jev` [`jev_check`]'s question, and say how it answered.
fn ask_jev(jev: &dyn Oracle) -> Result<String> {
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
    let response = jev.ask(&request)?;
    let elapsed = start.elapsed();
    let answer = match response.answers.get("cancel") {
        Some(Answer::Noul { noul }) => format!("P(yes) = {noul:.3}"),
        other => format!("{other:?}"),
    };
    Ok(format!(
        "Jev OK: model {}, {answer}, {} input tokens, {} ms",
        response.model,
        response.usage.input_tokens,
        elapsed.as_millis()
    ))
}

fn write(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    make_parent(path)?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// Create the directory `path` is in, unless it is the current one.
fn make_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod cli_tests;

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{error::ErrorKind, CommandFactory, Parser};
    use stretto_report::cli_doc;

    /// `docs/cli.md` documents this CLI as it is.
    #[test]
    fn the_cli_reference_is_current() {
        let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
        let section = cli_doc::markdown(&Cli::command());
        let checked = cli_doc::check_page(&page, "stretto", &section);
        assert!(checked.is_ok(), "{}", checked.unwrap_err());
    }

    fn parse(args: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("stretto").chain(args.split_whitespace()))
    }

    fn kind(args: &str) -> Option<ErrorKind> {
        parse(args).err().map(|e| e.kind())
    }

    /// A flow of the docs' examples, from five retail sessions.
    fn example_flow() -> stretto_report::flow::Flow {
        stretto_report::flow::Flow::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/examples/retail-5-sessions.flow.json"),
        )
        .unwrap()
    }

    const ORACLE: stretto_oracle::MockOracle = stretto_oracle::MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };

    /// The habit's rule, which asks no questions. After finding the user,
    /// the example flow reads their details at 0.25, and the binding's
    /// chance is 0.75.
    fn habit_rule(max_questions: usize) -> super::Rule {
        super::Rule {
            threshold: 0.15,
            decider: stretto_report::flow::Decider::Habit,
            max_questions,
            explore: None,
        }
    }

    /// `flow-serve` answers a query with the flow's next step after the
    /// conversation, and hands back one it cannot read, one that is not a
    /// conversation, and any past its budget of questions.
    #[test]
    fn flow_serve_answers_a_query_or_says_why_not() {
        let flow = example_flow();
        let query = serde_json::json!({
            "task_id": "1",
            "messages": [
                {"role": "user", "content": "Please cancel my order. I am c1@example.com."},
                {"role": "assistant", "tool_calls": [{"id": "a", "name": "find_user_id_by_email",
                    "arguments": {"email": "c1@example.com"}, "requestor": "assistant"}]},
                {"role": "tool", "id": "a", "content": "user_1", "error": false}
            ]
        })
        .to_string();
        let mut asked = 0;
        let mut answer = |line: &str, max_questions| {
            super::answer_query(&flow, &ORACLE, line, &habit_rule(max_questions), &mut asked)
                .unwrap()
        };
        let next = answer(&query, 5);
        assert_eq!(next["site"], "find_user_id_by_email", "{next}");
        assert_eq!(next["tool"], "get_user_details", "{next}");
        assert_eq!(next["arguments"], serde_json::json!({"user_id": "user_1"}));
        let reason = |v: serde_json::Value| v["reason"].as_str().unwrap().to_string();
        assert!(reason(answer("not json", 5)).starts_with("bad query: "));
        let not_a_conversation = r#"{"task_id": "1", "messages": "none"}"#;
        assert!(reason(answer(not_a_conversation, 5)).starts_with("error: "));
        assert_eq!(
            reason(answer(&query, 0)),
            "the flow's 0 questions are spent"
        );
    }

    /// `flow-serve` answers a line on a connection with a line, and logs
    /// the answer with the time it took.
    #[test]
    fn flow_serve_answers_on_its_port_and_logs_each_answer() {
        use std::io::{BufRead, Write};
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let listen = format!("127.0.0.1:{port}");
        let log =
            std::env::temp_dir().join(format!("stretto-flow-serve-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&log);
        // It serves until the process ends.
        let (at, to) = (listen.clone(), log.clone());
        std::thread::spawn(move || {
            super::serve(&example_flow(), &ORACLE, &at, habit_rule(5), Some(to))
        });
        let ask = |line: &str| {
            // A pause before each try, the first too, as the server may not
            // listen yet: whether a try fails is the scheduler's to say.
            let mut stream = (0..250)
                .find_map(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    std::net::TcpStream::connect(&listen).ok()
                })
                .expect("flow-serve listens");
            stream.write_all(line.as_bytes()).unwrap();
            let mut reply = String::new();
            std::io::BufReader::new(&stream)
                .read_line(&mut reply)
                .unwrap();
            serde_json::from_str::<serde_json::Value>(&reply).unwrap()
        };
        let first = ask("not json\n");
        assert_eq!(first["action"], "hand_back", "{first}");
        // It logs an answer before it takes the next connection.
        assert_eq!(ask("{}\n")["action"], "hand_back");
        let logged = std::fs::read_to_string(&log).unwrap();
        let entry: serde_json::Value =
            serde_json::from_str(logged.lines().next().unwrap()).unwrap();
        assert_eq!(entry["reason"], first["reason"]);
        assert!(entry["ms"].is_u64(), "{entry}");
        std::fs::remove_file(&log).ok();
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

    #[test]
    fn init_needs_a_domain_or_a_flow_and_the_server_after_two_dashes() {
        for ok in [
            "init --host cursor --domain notes -- npx -y server /notes",
            "init --host vscode --flow f.json --shadow -- npx -y server",
            "init --host claude-code --domain notes --write .mcp.json --force -- server",
        ] {
            assert!(parse(ok).is_ok(), "{ok}");
        }
        for missing in [
            "init --host cursor -- server",
            "init --host cursor --domain notes",
            "init --host cursor --domain notes --shadow -- server",
            "init --host cursor --domain notes --force -- server",
        ] {
            assert_eq!(
                kind(missing),
                Some(ErrorKind::MissingRequiredArgument),
                "{missing}"
            );
        }
        assert_eq!(
            kind("init --host zed --domain notes -- server"),
            Some(ErrorKind::InvalidValue)
        );
    }
}
