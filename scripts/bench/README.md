# Rebuilding the benchmarks round

These scripts recompute every number in [docs/results/benchmarks-2026-09-27.md](../../docs/results/benchmarks-2026-09-27.md) and in the paper's §4 from the benchmarks' published runs. No step calls a model or needs an API key. Every replay is trace-only, answered from the recorded trajectory, except the τ²-bench environment sets, which run τ²-bench's own tools in-process.

## 1. Build and fetch

```bash
cargo build --release                      # target/release/stretto, or set S
scripts/fetch-leaderboard.sh -t all        # τ²-bench's leaderboard runs, in .data/tau2-targets/
export WORK=work TAU2=../tau2-bench PY=python3   # PY: a Python with τ²-bench installed
```

`$TAU2` is a checkout of [τ²-bench](https://github.com/sierra-research/tau2-bench). It holds the domains' tasks and splits, its own solo runs (`data/tau2/results/final/`), and the environment the `tau2-env`, `sweep` and `perdec` sets replay against.

## 2. Convert the other benchmarks' runs

The results page's Reproduce section gives each converter's command and the runs it reads. Write each converter's output here:

| `$WORK/` | Converter | Holds |
|---|---|---|
| `v1/` | `taubench_v1_to_tau2.py` | `gpt-4o-retail.json`, `gpt-4o-airline.json`, `sonnet-35-new-retail.json`, `sonnet-35-new-airline.json` |
| `bfcl/` | `bfcl_to_tau2.py --runs` | `runs/<model>.json`, `bfcl_ground_truth.json`, `checkout/` |
| `dojo/` | `agentdojo_to_tau2.py` | `<suite>/<model>.json`, `checkout/` |
| `wb/` | `workbench_to_tau2.py` | `<domain>/<model>.json`, `checkout/` |
| `mcpm/` | `mcpmark_to_tau2.py --learn-from-all` | `mcpmark_<service>/<model>.json`, `checkout/` |
| `dtap/` | `dtap_to_tau2.py` | `customer_service/`, `dtap_crm/`, `dtap_telecom/`, `dtap_travel/` with `<harness>-<model>.json`, and `checkout/` |

## 3. Learn and replay

```bash
scripts/bench/replay.sh all        # or any of: tau2 tau2-env sweep perdec v1 bfcl dojo wb mcpm dtap ceilings
```

`replay.sh` learns each flow into `$WORK/flows/`. It writes each replay's totals and episodes to `$WORK/replays/<set>/<replay>/check.json`, and each set's read-only ceiling to `$WORK/ceilings/`. The script's header lists what each set replays. A replay that already has its `check.json` is skipped, so an interrupted batch resumes. The trace-only sets take minutes to an hour each with `JOBS=3`. The environment sets take hours.

## 4. Tables

```bash
python3 scripts/bench/tables.py --json rows.json
```

This prints:

- each set's anatomy and ceiling;
- τ²-bench's trace-only replays against the environment's;
- reach against the next-step decider per benchmark, threshold and domain, pooled over agents, with a paired bootstrap over tasks;
- DTap-Bench by the flow's source, with each source's share of what the agent's own flow saves, over the agents that have both.

The other tables come from scripts that read the same replays:

```bash
# Calibration: each lookup's score against whether it was used before the agent's next write
python3 scripts/calibration.py $WORK/replays/dojo/c-travel-*-habit-0.3 --versus $WORK/replays/dojo/c-travel-*-reach-0.3
# A threshold per decision, checked against the sweep's actual replays at flat thresholds
python3 scripts/per_decision.py --replays $WORK/replays/perdec --results .data/tau2-targets --sweep $WORK/replays/sweep \
    --tau2 $TAU2 --recalibrate 40
# Saved turns in seconds and dollars, from the environment replays that name the turns they saved
python3 scripts/priced.py --replays $WORK/replays/tau2-env --results .data/tau2-targets
# How often each recorded agent makes a read again with no write between
python3 scripts/remade.py --results $WORK/dtap/dtap_telecom/*.json --tau2 $WORK/dtap/checkout
```

The results JSON also keeps WorkBench's first replays, from before bare calls were counted. Those came from an earlier binary and are not rebuilt here.
