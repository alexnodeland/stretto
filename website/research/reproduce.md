---
description: How to reproduce stretto's results - Phase 0 with no key, the published answer bundles, the benchmarks round, and the claims ledger's scripts.
---

# Reproduce the results

Every results page ends with the commands that reproduce it, and publishes the rows it read. This page is the map. Most of it needs no API key: the model answers the research paid for are published as bundles that replay from a cache.

You need a checkout of stretto and Rust. Build once:

```sh
git clone https://github.com/alexnodeland/stretto && cd stretto
cargo build --release
```

## Phase 0: no key

Phase 0 measures how compressible agents' behavior is, on τ²-bench's published trajectories: macro-tool headroom, how often a habit predicts the next call, where arguments come from, and transfer between models.

```sh
git clone --depth 1 https://github.com/sierra-research/tau2-bench ../tau2-bench
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench --out reports/phase0.md
```

It takes about 20 seconds. To also measure transfer to GLM-5, whose trajectories Sierra publishes with its τ²-bench leaderboard entry, fetch them and pass them as targets:

```sh
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench \
  $(scripts/fetch-leaderboard.sh | sed 's/^/--target /') --out reports/phase0.md
```

## The System-One model's answers, without a key

Phase 0b and the arbiter's results ask TypeSafe's Jev. Its answers are published as bundles in `docs/results/answers-*.jsonl.gz` and `docs/results/phase0b-*-answers.jsonl.gz`, one `{"key", "response"}` per line, keyed by the SHA-256 of each request. Import them into a replay cache, and the commands on the results pages read them with `--oracle replay`:

```sh
for b in docs/results/phase0b-v2-2026-09-23-answers docs/results/phase0b-v2-goal-free-2026-09-24-answers \
         docs/results/answers-2026-09-24-arms-sweep-confirm docs/results/answers-2026-09-24-cold-manifest-match \
         docs/results/answers-2026-09-24-arbiter-transfer docs/results/answers-2026-09-25-injection \
         docs/results/answers-2026-09-25-telecom; do
  gunzip -c $b.jsonl.gz | target/release/stretto import-answers --oracle-cache .oracle-cache
done
```

Each bundle's page says which results read it; [the telecom bundle's](../../docs/results/answers-2026-09-25-telecom.md) is the one above. The bundles carry TypeSafe's terms: the answers are provided only to reproduce and audit stretto's analysis.

To ask Jev yourself, set `TYPESAFE_API_KEY` and check it first with `stretto jev-check`. A full Phase 0b run is about 9,000 questions; `--oracle-budget` refuses to start above a dollar limit, and each distinct question is paid for once ([environment variables](/reference/environment)).

## The claims ledger

[Claims and evidence](./claims) lists, for each number in the paper's abstract and contributions, the script or page that recomputes it: `scripts/calibration.py`, `scripts/ceiling.py`, `scripts/learning_curve.py`, `scripts/telecom_workflow.py` and `stretto-procedure`, and the benchmarks round's scripts.

## The benchmarks round

Every number in [Seven benchmarks](./benchmarks) and in the paper's §4 is rebuilt from the benchmarks' published runs by [`scripts/bench/`](../../scripts/bench/README.md), with no model and no key:

1. build, and fetch τ²-bench's leaderboard runs (`scripts/fetch-leaderboard.sh -t all`);
2. convert each benchmark's runs to τ²-bench's format (the converters in `scripts/`);
3. learn and replay: `scripts/bench/replay.sh all`, resumable;
4. print the tables: `python3 scripts/bench/tables.py --json rows.json`.

The trace-only sets take minutes to an hour each; the sets that replay against τ²-bench's environment take hours. [The page's Reproduce section](./benchmarks#reproduce) gives each converter's command and the runs it reads.

## Live runs

The live pilots ran GLM-5.3 in Claude Code on a Z.ai coding plan, with τ²-bench's tools served over MCP behind `stretto-proxy`. They spend a model's credits, so each needs a budget before it starts. [`pilot/`](../../pilot/README.md) holds the harness, and the recorded episodes are published: `pilot/rescore.py` reproduces every recorded reward from them ([the episodes](../../docs/results/episodes-2026-09-24.md)).

## Paper figures

`scripts/paper_figures.py` draws the paper's figures from the round's published rows.
