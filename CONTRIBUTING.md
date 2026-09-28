# Contributing to stretto

Thanks for helping. stretto is a Rust workspace (the product: `stretto`, `stretto-proxy`, `stretto-procedure`) with Python scripts for the research behind it (converters, replays, live pilots). Most changes touch one of the two; you rarely need both toolchains.

## Development setup

```bash
git clone https://github.com/alexnodeland/stretto && cd stretto
make help   # the everyday commands
make ci     # what CI's check job runs
```

You need Rust (`rust-toolchain.toml` pins stable, with rustfmt and clippy; the workspace builds with Rust 1.88 or later), Python 3 for `scripts/` and `pilot/`, and Node 22 for the documentation site and the walkthrough. `make install-dev-tools` adds llvm-tools and cargo-llvm-cov, which `make coverage` needs.

The [`Makefile`](Makefile)'s Rust targets run the commands CI runs ([`.github/workflows/ci.yml`](.github/workflows/ci.yml) and [`coverage.yml`](.github/workflows/coverage.yml)):

| Command | What it does |
|---|---|
| `make fmt`, `make check` | Formats the code; checks the formatting, as CI does |
| `make lint` | Clippy on every target, with warnings as errors, and on the console's `ts` feature |
| `make test` | Unit, integration and doc tests |
| `make types` | Rewrites the API's TypeScript for the UI (`console/src/api/generated/`), and fails if it was out of date |
| `make doc` | The API docs, with rustdoc warnings as errors |
| `make ci` | `check`, `lint`, `test`, `types` and `doc`: CI's check job |
| `make msrv` | The workspace on Rust 1.88, Cargo.toml's `rust-version`, with the lockfile, as CI's MSRV job |
| `make coverage` | Line coverage with cargo-llvm-cov, written to `lcov.info`; fails under the threshold in the Makefile |
| `make bless` | Regenerates `docs/cli.md` and the examples in `docs/review.md` |
| `make quickstart`, `make walkthrough` | The quickstart and the walkthrough end to end on a debug build, as CI's walkthrough job runs them |
| `make site` | Builds the documentation site, which fails on a dead link |
| `make ui`, `make ui-check` | Builds the console's UI; runs its formatting check, lint, type check, unit tests and build, as CI's ui job |
| `make e2e` | The UI's end-to-end tests in Chromium, on the mock API and on the console over the test fixtures |
| `make console` | Builds the UI and runs the console on `~/.stretto` (`ARGS="--read-only --open"`) |
| `make docker`, `make docker-console` | Build the CLI's container image, and the console's |

`docs/cli.md` is generated from the CLI's own help, and each binary's tests fail when its section is stale. After changing a command or an option, regenerate it with `make bless` (`STRETTO_BLESS=1 cargo test`) and commit the result. A test also checks that `docs/formats.md` names every field a flow or an arbiter file holds.

The documentation site lives in [`website/`](website/) (VitePress). `make site` builds it; `cd website && npm run dev` serves it with hot reload while you edit.

### Dev container and editor

[`.devcontainer/`](.devcontainer/) sets up Rust, Python 3, Node 22, shellcheck and Claude Code, with the editor extensions, and forwards port 7878 (the console) and 5173 (the site's dev server). Open the repository in VS Code with the Dev Containers extension, or in GitHub Codespaces; the first start runs `make install-dev-tools` and fetches the dependencies. [`.vscode/`](.vscode/) formats Rust on save, runs clippy as you edit, and recommends the extensions.

### Claude Code

The repository carries a [Claude Code](https://code.claude.com/docs) setup. [`CLAUDE.md`](CLAUDE.md) imports [`AGENTS.md`](AGENTS.md), the context every coding agent shares, and adds the commands, the rules for numbers and secrets, and the pull request workflow. In [`.claude/`](.claude/):

- `settings.json` allows the routine commands (cargo, the make targets, npm in `website/`, read-only git). It asks before live pilots, `--oracle jev`, pushes to `main`, force-pushes and tags. It denies reading `.env` files and key files, and `cargo publish`. It also runs two hooks: rustfmt on each `.rs` file Claude edits, and `hooks/session-start.sh`, which prepares a Claude Code on the web container (Rust components, crates, node modules) and does nothing locally.
- `skills/` holds `check` (run what CI runs), `results` (write up an experiment across the results page, the claims ledger, the paper and RFC-001), `release` and `steward` (take a pull request to green).
- `agents/` holds `reviewer`, which reviews a diff against the conventions and the docs' drift tests, and `claims-checker`, which checks every number in changed prose against the claims ledger.

Personal overrides go in `.claude/settings.local.json`, which git ignores.

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
| `brand/` | The brand kit: logo, tokens, messaging, the explainer and the videos, with the scripts that rebuild them ([brand/README.md](brand/README.md)) |
| `examples/quickstart/` | The no-key quickstart that CI runs |
| `packaging/` | The Homebrew formula and the installers' test; `install.sh`, `install.ps1` and the `Dockerfile` are at the root |

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
