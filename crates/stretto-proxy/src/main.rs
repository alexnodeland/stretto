use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
use stretto_proxy::{
    expand_home, prune, run, run_active, Active, Config, ConfirmConfig, FlowConfig, Upstream,
    FAILURE_EXIT_CODE,
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
    #[arg(help_heading = "Recording", long, value_name = "DIR")]
    record: Option<PathBuf>,
    /// Domain for the log header, e.g. `retail`; --guards uses its rules.
    #[arg(help_heading = "Recording", long, value_name = "NAME")]
    domain: Option<String>,
    /// Model that drives the agent, for the log header.
    #[arg(help_heading = "Recording", long, value_name = "MODEL")]
    agent_model: Option<String>,
    /// On start, delete what is older than this many days: the session
    /// logs in --record with the flow and confirmation logs beside them,
    /// and the answers in --oracle-cache.
    #[arg(help_heading = "Recording", long, value_name = "DAYS")]
    retain_days: Option<u64>,
    /// Run this flow (from `stretto compile` or `stretto learn`) after each
    /// of the agent's calls, and append its lookups to the result.
    #[arg(help_heading = "Flows", long, value_name = "FILE")]
    flow: Option<PathBuf>,
    /// Who answers the flow's and the confirmation judge's questions: `jev`
    /// (needs TYPESAFE_API_KEY), `replay` (the cache only) or `mock`.
    #[arg(help_heading = "The System-One model", long, value_enum, default_value_t = OracleArg::Jev)]
    oracle: OracleArg,
    /// Replay cache for the System-One model's answers.
    #[arg(
        help_heading = "The System-One model",
        long,
        value_name = "DIR",
        default_value = "~/.stretto/oracle-cache"
    )]
    oracle_cache: PathBuf,
    /// Take a lookup when the tool's probability times its arguments'
    /// agreement is at least this.
    #[arg(help_heading = "Flows", value_name = "P", long, default_value_t = 0.3)]
    flow_threshold: f64,
    /// Where the tool's probability comes from: `arbiter` (the habit, the
    /// System-One model's answers and the predicates, combined) or `habit`
    /// (the habit alone, which never asks a System-One model and needs no
    /// key).
    #[arg(help_heading = "Flows", long, value_enum, default_value_t = DeciderArg::Arbiter)]
    flow_decider: DeciderArg,
    /// Lookups appended to one result, at most.
    #[arg(help_heading = "Flows", value_name = "N", long, default_value_t = 8)]
    flow_per_call: usize,
    /// Lookups per session, at most.
    #[arg(help_heading = "Flows", value_name = "N", long, default_value_t = 40)]
    flow_per_session: usize,
    /// Questions to the System-One model per session, at most.
    #[arg(help_heading = "Flows", value_name = "N", long, default_value_t = 300)]
    flow_questions: usize,
    /// Append the flow's decisions here (default: next to the session log).
    #[arg(help_heading = "Flows", long, value_name = "FILE")]
    flow_log: Option<PathBuf>,
    /// Shadow mode: the flow decides after each call and logs what it
    /// would look up (`"shadow": true`), but makes no lookups, so the agent
    /// gets the server's results unchanged. `stretto promote --sessions`
    /// then makes the same decisions again from the answers cached in
    /// --oracle-cache, and scores them against what the agent did.
    #[arg(help_heading = "Flows", long, requires = "flow")]
    flow_shadow: bool,
    /// Check each of the agent's calls against the policy guards of
    /// --domain (`retail` or `airline`), and refuse the ones they fail.
    #[arg(help_heading = "Writes", long)]
    guards: bool,
    /// Put each write the guards check for a confirmation to the System-One
    /// model too, with the questions `stretto confirm` asks: `log` records
    /// each judgment; `enforce` also refuses a write the judge fails. Needs
    /// --guards and --context. A judge that cannot answer refuses nothing.
    #[arg(help_heading = "Writes", long, value_enum, value_name = "MODE", requires_all = ["guards", "context"])]
    confirm_judge: Option<JudgeArg>,
    /// Also ask the second question (`proposed`: had the agent proposed the
    /// change?); a write then fails unless both answers are yes.
    #[arg(
        help_heading = "Writes",
        long,
        value_enum,
        value_name = "QUESTION",
        requires = "confirm_judge"
    )]
    confirm_second: Option<SecondArg>,
    /// Ask the second question but only log its answer (shadow mode): a
    /// write then fails on the first answer alone.
    #[arg(help_heading = "Writes", long, requires = "confirm_second")]
    confirm_second_shadow: bool,
    /// A write fails when an answer's probability of a yes is below this.
    #[arg(
        help_heading = "Writes",
        value_name = "P",
        long,
        default_value_t = 0.5,
        requires = "confirm_judge"
    )]
    confirm_threshold: f64,
    /// The judge's questions per session, at most.
    #[arg(
        help_heading = "Writes",
        value_name = "N",
        long,
        default_value_t = 100,
        requires = "confirm_judge"
    )]
    confirm_questions: usize,
    /// Append the judgments here (default: next to the session log).
    #[arg(
        help_heading = "Writes",
        long,
        value_name = "FILE",
        requires = "confirm_judge"
    )]
    confirm_log: Option<PathBuf>,
    /// Add `stretto_commit`, which makes several calls in one, in order,
    /// each checked by the guards.
    #[arg(help_heading = "Writes", long)]
    commit: bool,
    /// Read the conversation from this file, which the host appends to as
    /// JSON lines: `{"role": "user" | "assistant", "content": text}`.
    #[arg(help_heading = "The conversation", long, value_name = "FILE")]
    context: Option<PathBuf>,
    /// Task id, which picks the flow's fold (default: the session).
    #[arg(help_heading = "Flows", long, value_name = "ID")]
    task_id: Option<String>,
    /// A Streamable HTTP server to proxy for, such as
    /// `https://example.com/mcp`, in place of a server command. The host
    /// still runs the proxy as a stdio server.
    #[arg(help_heading = "The server", long, value_name = "URL")]
    upstream: Option<String>,
    /// With --upstream: send header NAME with the value of environment
    /// variable VAR, such as `Authorization=GITHUB_AUTH` for a variable that
    /// holds `Bearer …`. Values are never logged.
    #[arg(
        help_heading = "The server",
        long = "upstream-header",
        value_name = "NAME=VAR",
        requires = "upstream"
    )]
    upstream_headers: Vec<String>,
    /// The MCP server to run, and its arguments.
    #[arg(
        last = true,
        required_unless_present = "upstream",
        conflicts_with = "upstream",
        value_name = "SERVER_COMMAND"
    )]
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
    if let Some(days) = cli.retain_days {
        retain(&cli, days);
    }
    let result = active(&cli).and_then(|active| {
        let config = Config {
            command: cli.command.clone(),
            upstream: upstream(&cli)?,
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

/// Delete what is older than `days` in the log directory and the answer
/// cache, before the session starts.
fn retain(cli: &Cli, days: u64) {
    let dirs = [
        cli.record.clone().map(expand_home),
        Some(expand_home(cli.oracle_cache.clone())),
    ];
    for dir in dirs.into_iter().flatten() {
        match prune(&dir, days) {
            Ok(0) => {}
            Ok(n) => eprintln!(
                "stretto-proxy: deleted {n} files older than {days} days from {}",
                dir.display()
            ),
            Err(e) => eprintln!("stretto-proxy: pruning {}: {e:#}", dir.display()),
        }
    }
}

/// The Streamable HTTP server to proxy for, with its headers' values read
/// from the environment.
fn upstream(cli: &Cli) -> Result<Option<Upstream>> {
    let Some(url) = &cli.upstream else {
        return Ok(None);
    };
    let mut headers = Vec::new();
    for spec in &cli.upstream_headers {
        let Some((name, var)) = spec.split_once('=') else {
            bail!("--upstream-header takes NAME=VAR, the header and the environment variable holding its value");
        };
        let value = std::env::var(var).with_context(|| {
            format!("the environment variable {var}, for the header {name}, is not set")
        })?;
        headers.push((name.to_string(), value));
    }
    Ok(Some(Upstream {
        url: url.clone(),
        headers,
    }))
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
                shadow: cli.flow_shadow,
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
                second_shadow: cli.confirm_second_shadow,
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;
    use stretto_report::cli_doc;

    /// `docs/cli.md` documents this CLI as it is.
    #[test]
    fn the_cli_reference_is_current() {
        let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
        let section = cli_doc::markdown(&Cli::command());
        if let Err(e) = cli_doc::check_page(&page, "stretto-proxy", &section) {
            panic!("{e}");
        }
    }
}
