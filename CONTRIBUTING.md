# Contributing to stretto

Thanks for helping. stretto is a Rust workspace (the product: `stretto`, `stretto-proxy`, `stretto-procedure`) with Python scripts for the research behind it (converters, replays, live pilots). Most changes touch one of the two; you rarely need both toolchains.

## Development setup

```bash
git clone https://github.com/alexnodeland/stretto && cd stretto
cargo build --release          # the toolchain is pinned in rust-toolchain.toml
cargo test
```

The checks CI runs ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`docs/cli.md` is generated from the CLI's own help, and each binary's tests fail when its section is stale. After changing a command or an option, regenerate it with `STRETTO_BLESS=1 cargo test` and commit the result. A test also checks that `docs/formats.md` names every field a flow or an arbiter file holds.

The documentation site lives in [`website/`](website/) (VitePress):

```bash
cd website && npm ci && npm run dev
```

## Project layout

| Path | What it holds |
|---|---|
| `crates/stretto-trace` | Agent episodes and trace ingest (recorded sessions, τ²-bench's results) |
| `crates/stretto-model` | Action abstraction and the Bayesian models of what an agent does next |
| `crates/stretto-oracle` | The System-One model interface (Jev) and its replay cache |
| `crates/stretto-report` | The `stretto` CLI: learn, compile, serve, show, diff, audit, promote, redact, and the Phase 0 reports |
| `crates/stretto-proxy` | `stretto-proxy` (the MCP proxy that records sessions and serves flows), `stretto-procedure`, and a small demo MCP server |
| `pilot/` | Live episodes: τ²-bench (`run_episode.py`) and other benchmarks (`bench/`) |
| `scripts/` | Converters from other benchmarks' published runs, and the analyses behind the results pages |
| `docs/` | Design, formats, privacy, review, the CLI reference, RFC-001, and the results pages |
| `paper/` | The working paper |
| `website/` | The documentation site |

## Making a change

1. Open an issue first for anything larger than a fix, so the approach can be agreed before you write it.
2. Keep a change to one purpose, with tests for what it adds or fixes.
3. Run the checks above.
4. Update the docs the change touches: `docs/cli.md` (regenerated), `docs/formats.md` for a file format, the guide pages in `website/`, and `CHANGELOG.md` under *Unreleased*.

### Results and claims

A number in the README, the paper or the docs must come from a results page in `docs/results/`, and the page must say how to recompute it. Headline numbers are listed in [the claims ledger](docs/results/claims.md), with the kind of evidence behind each (live, replay, or from the published record) and its interval. When a result changes, change the ledger and every page that quotes it in the same commit.

### Writing style

Plain and exact: say what a thing does and how you know, with the scope of every number (which model, which benchmark, live or replayed). Short sentences, no hype.

## Reporting bugs and security issues

Use the issue templates for bugs and feature requests. Report a vulnerability privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.

## License

By contributing you agree that your contribution is licensed under the [MIT License](LICENSE).
