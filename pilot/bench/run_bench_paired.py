"""Run a benchmark's held-out tasks live, once in each arm, for one agent model.

Each task runs in every arm (`--arms`, by default no flow and the reach flow),
in an order drawn per task from `--seed`, so neither arm always goes first. The
plan runs every task's first arm, then every task's second, so a task's arms
run apart: two arms share their prompt's prefix, the system prompt, the tools
and the request, and an arm that ran within the prompt cache's lifetime of the
other would read what that one wrote, which confounds their cost. An episode
already recorded (its `result.json`) is kept, so an interrupted run resumes,
and several runners can share one plan (each episode is taken by the runner
that first makes its lock folder under `--out/locks`). For GLM, every
episode's credits are booked in `credits.jsonl` under `--out`, and the run
stops before an episode would start past `--credit-cap`.

The held-out tasks are those the flows never trained on: the converters hold
out the first four of every ten tasks in order (`--test-share 0.4`).

    python run_bench_paired.py agentdojo --suites travel slack --agent-cli glm --model glm-5.3 \
        --flows DIR --out runs/agentdojo --credit-cap 300
    python run_bench_paired.py bfcl --bfcl-dir DIR --sample 20 --agent-cli claude \
        --model claude-haiku-4-5-20251001 --flows DIR --out runs/bfcl
"""

import argparse
import json
import random
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def held_out(i: int, split: str) -> bool:
    """Whether the i-th task, in order, is in the split: the first four of every ten are the test tasks."""
    return (i % 10 < 4) == (split == "test")


def agentdojo_tasks(suites: list[str], version: str, split: str = "test") -> list[tuple[str, str]]:
    from agentdojo.task_suite.load_suites import get_suite

    out = []
    for suite in suites:
        ids = sorted(get_suite(version, suite).user_tasks, key=lambda t: int(t.rsplit("_", 1)[1]))
        out += [(suite, t) for i, t in enumerate(ids) if held_out(i, split)]
    return out


def bfcl_tasks(bfcl_dir: Path, category: str, sample: int | None, seed: int,
               split: str = "test") -> list[tuple[str, str]]:
    """The split's tasks, in the data file's order, and a seeded sample of them."""
    path = bfcl_dir / "bfcl_eval" / "data" / f"BFCL_v4_{category}.json"
    ids = [json.loads(l)["id"] for l in path.read_text().splitlines() if l.strip()]
    held = [t for i, t in enumerate(ids) if held_out(i, split)]
    if sample:
        held = sorted(random.Random(seed).sample(held, sample), key=lambda t: int(t.rsplit("_", 1)[1]))
    return [("bfcl", t) for t in held]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("bench", choices=["agentdojo", "bfcl"])
    parser.add_argument("--suites", nargs="+", default=["travel", "slack", "banking", "workspace"])
    parser.add_argument("--version", default="v1.2.1")
    parser.add_argument("--arms", nargs="+", default=["baseline", "reach"])
    parser.add_argument("--flows", type=Path, required=True, help="folder with agentdojo-<suite>.flow.json or bfcl.flow.json")
    parser.add_argument("--bfcl-dir", type=Path, help="the folder holding the bfcl_eval package")
    parser.add_argument("--category", default="multi_turn_base", help="BFCL's category")
    parser.add_argument("--sample", type=int, help="BFCL: this many held-out tasks, drawn with --seed")
    parser.add_argument("--agent-cli", default="glm", choices=["glm", "claude"])
    parser.add_argument("--model", default="glm-5.3")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--credit-cap", type=float, default=0.0, help="GLM credits for this run, at most (0: no cap)")
    parser.add_argument("--limit", type=int, help="the first N tasks only")
    parser.add_argument("--split", default="test", choices=["test", "train"],
                        help="the held-out tasks (default), or the training tasks the flows learned from, such as "
                             "to record an agent's own sessions for `stretto promote`")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    ledger = args.out / "credits.jsonl"
    spent = sum(json.loads(l)["credits"] for l in ledger.read_text().splitlines()) if ledger.exists() else 0.0
    if args.bench == "agentdojo":
        tasks = agentdojo_tasks(args.suites, args.version, args.split)[: args.limit]
    else:
        tasks = bfcl_tasks(args.bfcl_dir, args.category, args.sample, args.seed, args.split)[: args.limit]
    rng = random.Random(args.seed)
    orders = []
    for suite, task in tasks:
        arms = list(args.arms)
        rng.shuffle(arms)
        orders.append((suite, task, arms))
    # One pass per position: a task's arms run a pass apart, not back to back.
    plan = [(suite, task, arms[k]) for k in range(len(args.arms)) for suite, task, arms in orders]
    print(f"{len(tasks)} tasks, {len(plan)} episodes; {spent:.1f} credits booked", file=sys.stderr, flush=True)
    for suite, task, arm in plan:
        result = args.out / arm / f"{suite}-{task}" / "result.json"
        if result.exists():
            continue
        if args.credit_cap:
            # Every recorded episode's credits, whichever runner ran it.
            spent = sum(json.loads(r.read_text()).get("credits_billed", 0.0) for r in args.out.glob("*/*/result.json"))
            if spent >= args.credit_cap:
                print(f"credit cap reached: {spent:.1f} of {args.credit_cap}", file=sys.stderr, flush=True)
                break
        # Several runners may share one plan: each episode is taken by whoever makes its lock first.
        lock = args.out / "locks" / arm / f"{suite}-{task}"
        lock.parent.mkdir(parents=True, exist_ok=True)
        try:
            lock.mkdir()
        except FileExistsError:
            continue
        command = [sys.executable, str(HERE / "run_bench_episode.py"), args.bench, "--task", task,
                   "--arm", arm, "--agent-cli", args.agent_cli, "--model", args.model, "--out", str(args.out)]
        if args.bench == "agentdojo":
            command += ["--suite", suite, "--version", args.version]
        else:
            command += ["--bfcl-dir", str(args.bfcl_dir)]
        if arm != "baseline":
            flow = f"agentdojo-{suite}.flow.json" if args.bench == "agentdojo" else "bfcl.flow.json"
            command += ["--flow", str(args.flows / flow)]
        t0 = time.time()
        done = subprocess.run(command, capture_output=True, text=True)
        line = next((l for l in done.stdout.splitlines() if l.startswith("RESULT")), None)
        if result.exists():
            credits = json.loads(result.read_text()).get("credits_billed", 0.0)
        else:
            credits = 0.0
            (args.out / "failed.log").open("a").write(f"{suite} {task} {arm}: exit {done.returncode}\n{done.stderr[-2000:]}\n")
        spent += credits
        lock.rmdir()
        with ledger.open("a") as f:
            f.write(json.dumps({"unix": time.time(), "what": f"{suite} {task} {arm}", "credits": credits}) + "\n")
        print(f"{suite} {task} {arm}: {line or 'failed'} ({time.time() - t0:.0f} s; {spent:.1f} credits)",
              file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
