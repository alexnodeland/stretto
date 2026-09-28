# CLAUDE.md

@AGENTS.md

AGENTS.md, imported above, is the project context every coding agent shares: what stretto is, the layout, the conventions, and the rules on secrets. This file adds what Claude Code needs on top of it. Keep project facts in AGENTS.md and Claude Code specifics here.

## Everyday commands

`make help` lists every target. The Rust targets run the same commands as CI.

| Command | What it runs |
|---|---|
| `make fmt` | `cargo fmt --all` |
| `make check` | `cargo fmt --all -- --check` |
| `make lint` | `cargo clippy --all-targets -- -D warnings` |
| `make test` | `cargo test --all-targets`, then `cargo test --doc` |
| `make doc` | `cargo doc --no-deps`, with rustdoc warnings as errors |
| `make ci` | check, lint, test and doc: CI's check job. Run it before every push |
| `make msrv` | `cargo check` on Rust 1.88, Cargo.toml's `rust-version`, with the lockfile |
| `make coverage` | cargo-llvm-cov: writes `lcov.info`, fails under `COVERAGE_MIN` in the Makefile |
| `make bless` | `STRETTO_BLESS=1 cargo test`: regenerates `docs/cli.md` and `docs/review.md`'s examples |
| `make quickstart`, `make walkthrough` | the quickstart and the walkthrough, as CI's walkthrough job runs them, on a debug build |
| `make site` | the documentation site, which fails on a dead link |

One test: `cargo test -p stretto-report --test formats`, or `cargo test -p stretto-model <name>`.

A hook runs rustfmt on every `.rs` file you edit or write (`.claude/hooks/format-rust.sh`), so you need not format by hand. `make check` still checks it.

The docs are tested against the code, so a change to the CLI or a file format fails the tests until the docs follow:

- `docs/cli.md` is generated from the clap definitions, and a test in each binary fails when its section is stale. After changing a command or an option, run `make bless` and commit the result.
- `crates/stretto-report/tests/formats.rs` fails when `docs/formats.md` does not name a field a flow or an arbiter file holds, and checks `docs/review.md`'s examples.
- CI runs `docs/walkthrough.md` (`scripts/walkthrough.py`) and `examples/quickstart/run.sh`, so neither can drift from the code.

## Running it

- **The quickstart:** `make quickstart`. It records six sessions on `stretto-mcp-demo`'s shop, then learns, reviews and serves a flow, with no key and no network.
- **The walkthrough:** `make walkthrough` runs `docs/walkthrough.md` on the official MCP filesystem server, which `npx` fetches, so it needs Node and the network.
- **The CLI:** `cargo run -p stretto-report -- <command>` runs `stretto`; `cargo run -p stretto-proxy --bin stretto-proxy -- <args>` runs the proxy. `stretto doctor` checks an install.

## Results, claims and numbers

- Each round of experiments has a dated page in `docs/results/` (`<topic>-YYYY-MM-DD.md`), with its rows (`.json`), answer bundles and episode archives beside it. `docs/results/README.md` indexes the pages by date.
- The claims ledger, `docs/results/claims.md`, lists each headline number with its kind of evidence (live, replay or record), its interval, and the script that recomputes it. The paper is `paper/stretto.md`. RFC-001 (`docs/rfc/001-habit-compiler.md`) records what each round changed, one amendment per round.
- **Every number you write comes from `docs/results/claims.md` or the paper, with its scope:** the model, the benchmark, live or replayed, and the interval. Copy it as the ledger writes it: 27.9% is not "almost 30%". Never estimate, round up or generalize past the scope. If no page has the number, say so rather than supply one.
- When a result changes, change the ledger and every page that quotes it in the same commit. The `results` skill has the list.

## Writing style

Calm and exact, as `brand/messaging.md` sets out: say what a thing does and how you know, in short sentences. No hype words, no unscoped claims, and no model names as endorsements. stretto is always lowercase. Use the project's terms: LLM turns, reads or lookups, a flow, a detour, the reach decider; live, replay, record. Commit messages follow `git log`: a subject that says what changed, in plain words with no type prefix, and a body in the same prose, with a list when there are several parts.

## Never

- **Commit, print or log a secret.** Keys come from the environment (`TYPESAFE_API_KEY`, `ZAI_API_KEY`). Check one only with `[ -n "$TYPESAFE_API_KEY" ]`; never echo it or write it to a file. `.claude/settings.json` denies Claude reads of `.env` files, key files under `~/.stretto`, the pilot's key files and credential stores.
- **Spend Z.ai or Jev credit casually.** The live pilots (`pilot/run_*.py`, `pilot/bench/run_bench_*.py`, `pilot/glm-claude.sh`, `pilot/claude-agent.sh`) and any command with `--oracle jev` or `jev-check` spend money or a plan's quota. Run them only when the owner asks, keep runs small, and prefer the replay cache and the published answer bundles. The Z.ai key is a coding-plan key, used only through Claude Code (`pilot/glm-claude.sh`). The settings ask before each of these.
- **Commit session logs, transcripts or caches.** Recorded sessions (`~/.stretto/logs`), `pilot/runs/`, Claude Code transcripts, `.oracle-cache/` and answer caches holding third-party data stay out of git. Episodes are published only in a results page's archive, and only from benchmark environments, whose customers are synthetic; a real user's sessions are shared only through `stretto redact` (`docs/privacy.md`).
- **Push to main, force-push, tag or publish** (`cargo publish`, `gh release`) unless the owner says to. Nothing is published to crates.io.

## Pull requests

1. Branch from `main`, one purpose per branch: `git switch -c <topic> origin/main`.
2. Run `make ci`. Add `make quickstart` and `make walkthrough` when the proxy or the CLI changed, and `make site` when `website/`, `docs/` or `paper/` changed: the Pages build runs only after merging.
3. Update what the change touches: `docs/cli.md` (`make bless`), `docs/formats.md`, the guide in `website/`, and `CHANGELOG.md` under *Unreleased*.
4. Open the PR with `.github/pull_request_template.md`: what changes and why, how it was checked, and the checklist.
5. CI must be green before merging: ci.yml (check, MSRV, walkthrough, the macOS and Windows builds) and Coverage, and container.yml when the image's inputs change. The `steward` skill has what each job checks and how to fix it.

## Skills and agents in this repository

- `check`: run what CI runs before a push, and read its failures.
- `results`: write up an experiment, updating the results page, the index, the ledger, the paper and RFC-001 together.
- `release`: cut a release, as `docs/releasing.md` describes.
- `steward`: take a pull request to green.
- The `reviewer` subagent reviews a diff against AGENTS.md, this file and the docs' drift tests.
- The `claims-checker` subagent checks every number in changed prose against the claims ledger, the results pages and the paper.
