use clap::Parser;
use std::ffi::OsString;
use std::path::PathBuf;
use stretto_proxy::{expand_home, run, Config, FAILURE_EXIT_CODE};

/// Forward a stdio MCP server's traffic unchanged, and record it for stretto.
///
/// Put this in an MCP host's configuration in place of the server's command,
/// with the real command after `--`. stdout carries only the protocol; the
/// proxy's own messages go to stderr. The exit status is the server's, or 125
/// if the proxy itself fails.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Write a session log (JSONL) into this directory, created if missing.
    ///
    /// A leading `~` is expanded, since hosts start servers without a shell.
    #[arg(long, value_name = "DIR")]
    record: Option<PathBuf>,
    /// Domain for the log header, e.g. `retail`.
    #[arg(long, value_name = "NAME")]
    domain: Option<String>,
    /// Model that drives the agent, for the log header.
    #[arg(long, value_name = "MODEL")]
    agent_model: Option<String>,
    /// The MCP server to run, and its arguments.
    #[arg(last = true, required = true, value_name = "SERVER_COMMAND")]
    command: Vec<OsString>,
}

fn main() {
    let cli = Cli::parse();
    let config = Config {
        command: cli.command,
        record: cli.record.map(expand_home),
        domain: cli.domain,
        agent_model: cli.agent_model,
    };
    match run(&config) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("stretto-proxy: {e:#}");
            std::process::exit(FAILURE_EXIT_CODE);
        }
    }
}
