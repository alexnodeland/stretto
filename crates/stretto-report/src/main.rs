use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use stretto_report::{phase0, render};

/// stretto: compile agent behavior into typed probabilistic flows.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Measure how compressible an agent's behavior is (no API keys needed).
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
        /// Write the Markdown report here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the full report as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
    },
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
    }
}

fn write(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
