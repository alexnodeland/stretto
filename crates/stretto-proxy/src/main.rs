use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
use stretto_proxy::{
    expand_home, run, run_active, Active, Config, ConfirmConfig, FlowConfig, FAILURE_EXIT_CODE,
};
use stretto_report::confirm::Second;
use stretto_report::flow::{Decider, Flow};
use stretto_report::guards::Guards;
use stretto_report::shadow::{OracleKind, ShadowConfig};

/// Forward a stdio MCP server's traffic, record it for stretto, and
/// optionally run a flow, policy guards and a commit tool on it.
///
/// Put this in an MCP host's configuration in place of the server's command,
/// with the real command after `--`. stdout carries only the protocol; the
/// proxy's own messages go to stderr. The exit status is the server's, or 125
/// if the proxy itself fails.
///
/// Without --flow, --guards, --commit, --context or --confirm-judge, every line
/// is forwarded byte for byte and nothing is parsed.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Write a session log (JSONL) into this directory, created if missing.
    ///
    /// A leading `~` is expanded, since hosts start servers without a shell.
    #[arg(long, value_name = "DIR")]
    record: Option<PathBuf>,
    /// Domain for the log header, e.g. `retail`; --guards uses its rules.
    #[arg(long, value_name = "NAME")]
    domain: Option<String>,
    /// Model that drives the agent, for the log header.
    #[arg(long, value_name = "MODEL")]
    agent_model: Option<String>,
    /// Run this flow (from `stretto compile` or `stretto learn`) after each
    /// of the agent's calls, and append its lookups to the result.
    #[arg(long, value_name = "FILE")]
    flow: Option<PathBuf>,
    /// Who answers the flow's and the confirmation judge's questions: `jev`
    /// (needs TYPESAFE_API_KEY), `replay` (the cache only) or `mock`.
    #[arg(long, value_enum, default_value_t = OracleArg::Jev)]
    oracle: OracleArg,
    /// Replay cache for the System-One model's answers.
    #[arg(long, value_name = "DIR", default_value = "~/.stretto/oracle-cache")]
    oracle_cache: PathBuf,
    /// Take a lookup when the tool's probability times its arguments'
    /// agreement is at least this.
    #[arg(long, default_value_t = 0.3)]
    flow_threshold: f64,
    /// Where the tool's probability comes from: `arbiter` (the habit, the
    /// System-One model's answers and the predicates, combined) or `habit`
    /// (the habit alone, which never asks a System-One model and needs no
    /// key).
    #[arg(long, value_enum, default_value_t = DeciderArg::Arbiter)]
    flow_decider: DeciderArg,
    /// Lookups appended to one result, at most.
    #[arg(long, default_value_t = 8)]
    flow_per_call: usize,
    /// Lookups per session, at most.
    #[arg(long, default_value_t = 40)]
    flow_per_session: usize,
    /// Questions to the System-One model per session, at most.
    #[arg(long, default_value_t = 300)]
    flow_questions: usize,
    /// Append the flow's decisions here (default: next to the session log).
    #[arg(long, value_name = "FILE")]
    flow_log: Option<PathBuf>,
    /// Check each of the agent's calls against the policy guards of
    /// --domain (`retail` or `airline`), and refuse the ones they fail.
    #[arg(long)]
    guards: bool,
    /// Put each write the guards check for a confirmation to the System-One
    /// model too, with the questions `stretto confirm` asks: `log` records
    /// each judgment; `enforce` also refuses a write the judge fails. Needs
    /// --guards and --context. A judge that cannot answer refuses nothing.
    #[arg(long, value_enum, value_name = "MODE", requires_all = ["guards", "context"])]
    confirm_judge: Option<JudgeArg>,
    /// Also ask the second question (`proposed`: had the agent proposed the
    /// change?); a write then fails unless both answers are yes.
    #[arg(long, value_enum, value_name = "QUESTION", requires = "confirm_judge")]
    confirm_second: Option<SecondArg>,
    /// A write fails when an answer's probability of a yes is below this.
    #[arg(long, default_value_t = 0.5, requires = "confirm_judge")]
    confirm_threshold: f64,
    /// The judge's questions per session, at most.
    #[arg(long, default_value_t = 100, requires = "confirm_judge")]
    confirm_questions: usize,
    /// Append the judgments here (default: next to the session log).
    #[arg(long, value_name = "FILE", requires = "confirm_judge")]
    confirm_log: Option<PathBuf>,
    /// Add `stretto_commit`, which makes several calls in one, in order,
    /// each checked by the guards.
    #[arg(long)]
    commit: bool,
    /// Read the conversation from this file, which the host appends to as
    /// JSON lines: `{"role": "user" | "assistant", "content": text}`.
    #[arg(long, value_name = "FILE")]
    context: Option<PathBuf>,
    /// Task id, which picks the flow's fold (default: the session).
    #[arg(long, value_name = "ID")]
    task_id: Option<String>,
    /// The MCP server to run, and its arguments.
    #[arg(last = true, required = true, value_name = "SERVER_COMMAND")]
    command: Vec<OsString>,
}

#[derive(Clone, Copy, ValueEnum)]
enum OracleArg {
    Jev,
    Replay,
    Mock,
}

#[derive(Clone, Copy, ValueEnum)]
enum DeciderArg {
    Arbiter,
    Habit,
}

#[derive(Clone, Copy, ValueEnum)]
enum JudgeArg {
    Log,
    Enforce,
}

#[derive(Clone, Copy, ValueEnum)]
enum SecondArg {
    Proposed,
    Described,
}

fn main() {
    let cli = Cli::parse();
    let result = active(&cli).and_then(|active| {
        let config = Config {
            command: cli.command.clone(),
            record: cli.record.clone().map(expand_home),
            domain: cli.domain.clone(),
            agent_model: cli.agent_model.clone(),
        };
        if active.is_active() {
            run_active(&config, &active)
        } else {
            run(&config)
        }
    });
    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("stretto-proxy: {e:#}");
            std::process::exit(FAILURE_EXIT_CODE);
        }
    }
}

/// What the flags ask the proxy to do besides forwarding.
fn active(cli: &Cli) -> Result<Active> {
    let flow = match &cli.flow {
        Some(path) => {
            let path = expand_home(path.clone());
            let flow = Flow::load(&path).with_context(|| format!("loading {}", path.display()))?;
            let decider = match cli.flow_decider {
                DeciderArg::Arbiter => Decider::Arbiter,
                DeciderArg::Habit => Decider::Habit,
            };
            if decider == Decider::Arbiter && !flow.has_arbiter() {
                bail!(
                    "{} was learned without a System-One model: serve it with --flow-decider habit",
                    path.display()
                );
            }
            // The habit alone asks no one, so it needs no key.
            let mut sc = ShadowConfig::new(match (decider, cli.oracle) {
                (Decider::Habit, _) | (_, OracleArg::Mock) => OracleKind::Mock,
                (_, OracleArg::Jev) => OracleKind::Jev,
                (_, OracleArg::Replay) => OracleKind::Replay,
            });
            sc.cache_dir = expand_home(cli.oracle_cache.clone());
            eprintln!(
                "stretto-proxy: serving the {} flow from {}",
                flow.domain(),
                path.display()
            );
            Some(FlowConfig {
                flow,
                oracle: sc.build()?,
                threshold: cli.flow_threshold,
                decider,
                per_call: cli.flow_per_call,
                per_session: cli.flow_per_session,
                max_questions: cli.flow_questions,
                log: cli.flow_log.clone().map(expand_home),
            })
        }
        None => None,
    };
    let guards = if cli.guards {
        let domain = cli
            .domain
            .as_deref()
            .context("--guards needs --domain (retail or airline)")?;
        Some(Guards::for_domain(domain).with_context(|| format!("no guards for {domain}"))?)
    } else {
        None
    };
    let confirm = match cli.confirm_judge {
        Some(mode) => {
            let mut sc = ShadowConfig::new(match cli.oracle {
                OracleArg::Jev => OracleKind::Jev,
                OracleArg::Replay => OracleKind::Replay,
                OracleArg::Mock => OracleKind::Mock,
            });
            sc.cache_dir = expand_home(cli.oracle_cache.clone());
            Some(ConfirmConfig {
                oracle: sc.build()?,
                model: sc.model.clone(),
                second: cli.confirm_second.map(|q| match q {
                    SecondArg::Proposed => Second::Proposed,
                    SecondArg::Described => Second::Described,
                }),
                threshold: cli.confirm_threshold,
                enforce: matches!(mode, JudgeArg::Enforce),
                max_questions: cli.confirm_questions,
                log: cli.confirm_log.clone().map(expand_home),
            })
        }
        None => None,
    };
    Ok(Active {
        flow,
        guards,
        confirm,
        commit: cli.commit,
        context: cli.context.clone().map(expand_home),
        task_id: cli.task_id.clone(),
    })
}
