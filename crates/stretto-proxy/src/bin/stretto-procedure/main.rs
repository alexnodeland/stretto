//! Run a compiled procedure on a ticket against an MCP server, with no model.
//!
//! The procedure (the IR `scripts/telecom_workflow.py --export` writes, read
//! by [`stretto_report::procedure`]) calls the server's tools one at a time
//! until its tree says stop, then checks the outcome the ticket states with a
//! read of its own. The run, its calls and its verdict, goes to stdout (or
//! `--out`) as JSON. A `hand_back` verdict means the check failed: give the
//! ticket, with the run's calls and results, to a model.

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};
use stretto_report::procedure::{Procedure, Tools, Verdict};

/// Run a compiled procedure on a ticket against an MCP server (stdio), with
/// no model. The run goes to stdout as JSON: its calls, its check of the
/// outcome the ticket states, and its verdict (`resolved`, `transferred`, or
/// `hand_back` when the check failed and the ticket should go to a model).
#[derive(Parser)]
#[command(name = "stretto-procedure", version)]
struct Cli {
    /// The procedure (JSON), as `scripts/telecom_workflow.py --export` writes it.
    #[arg(long, value_name = "FILE")]
    procedure: PathBuf,
    /// The ticket, as text.
    #[arg(long, value_name = "TEXT", conflicts_with = "ticket_file")]
    ticket: Option<String>,
    /// The ticket, from a file.
    #[arg(long, value_name = "FILE")]
    ticket_file: Option<PathBuf>,
    /// Write the run here instead of to stdout.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// The MCP server to run against, and its arguments (after `--`).
    #[arg(last = true, required = true, value_name = "SERVER")]
    server: Vec<String>,
}

/// An MCP server over stdio: JSON-RPC requests, one line each.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    /// How long it may take to exit once its input is closed.
    patience: Duration,
}

impl Server {
    fn start(command: &[String]) -> Result<Self> {
        let (program, args) = command
            .split_first()
            .ok_or_else(|| anyhow!("no server command"))?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("starting {program}"))?;
        let stdin = child
            .stdin
            .take()
            .context("the server's stdin is not piped")?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .context("the server's stdout is not piped")?,
        );
        let mut server = Server {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 0,
            patience: Duration::from_secs(5),
        };
        server.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "stretto-procedure", "version": env!("CARGO_PKG_VERSION")},
            }),
        )?;
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(server)
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("the server's stdin is closed")?;
        writeln!(stdin, "{message}")?;
        stdin.flush()?;
        Ok(())
    }

    /// A request's result, skipping the server's notifications and its own
    /// requests (which this client does not serve).
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                bail!("the server closed its output before answering {method}");
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("id") != Some(&json!(id)) || message.get("method").is_some() {
                continue;
            }
            if let Some(error) = message.get("error") {
                bail!("{method}: {error}");
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

impl Tools for Server {
    fn call(&mut self, name: &str, arguments: &Map<String, Value>) -> Result<(String, bool)> {
        let result = self.request("tools/call", json!({"name": name, "arguments": arguments}))?;
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let failed = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok((text, failed))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Closing its stdin tells a stdio server to shut down; one that has
        // not within its patience is stopped.
        drop(self.stdin.take());
        let asked = Instant::now();
        while asked.elapsed() < self.patience {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn main() -> Result<()> {
    run(&Cli::parse(), &mut std::io::stdout())
}

/// Run the procedure `cli` names, and write the run to `--out`, or to
/// `stdout`.
fn run(cli: &Cli, stdout: &mut dyn Write) -> Result<()> {
    let procedure = Procedure::load(&cli.procedure)?;
    let ticket = match (&cli.ticket, &cli.ticket_file) {
        (Some(t), _) => t.clone(),
        (None, Some(path)) => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        (None, None) => bail!("give the ticket with --ticket or --ticket-file"),
    };
    let mut server = Server::start(&cli.server)?;
    let run = procedure.run(&ticket, &mut server)?;
    drop(server);
    let text = serde_json::to_string_pretty(&run)? + "\n";
    match &cli.out {
        Some(path) => {
            std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?
        }
        None => stdout.write_all(text.as_bytes())?,
    }
    eprintln!(
        "stretto-procedure: {} calls, {}",
        run.calls.len(),
        meaning(run.verdict)
    );
    Ok(())
}

/// What a run's verdict means, in words.
fn meaning(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Resolved => "resolved by its own check",
        Verdict::Transferred => "transferred to a person",
        Verdict::HandBack => "handed back: its check failed",
    }
}

#[cfg(test)]
mod tests;
