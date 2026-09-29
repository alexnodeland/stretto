#!/usr/bin/env python3
"""The cold start on other agents, with more draws.

The cold start round rested on one agent (GLM-5) and five draws. This repeats
it on other agents' published τ²-bench results with many random draws. For
each agent, domain and size n, it draws `--draws` random sets of n training
tasks (seeded by agent, domain, size and draw) and learns three flows from each
set's trial-0 episodes, as that round did:

- `habit`: the habit alone (`learn --habit-only`);
- `arbiter`: the arbiter fitted on the draw's own held-out 30% (`learn`, asking
  Jev through the cache);
- `shipped`: the habit with a shipped arbiter (`learn --arbiter-from
  data/arbiters/<domain>.json`).

`--every` also learns the habit from every training task, the reference a
narrow draw is measured against. Each flow is replayed on the agent's test-task
episodes, all four trials, at 0.3 (`pilot/check_flow.py`: the habit with
`--flow-decider habit`, the others with `arbiter`), and each replay's totals and
per-episode rows go to OUT/rows.jsonl. A replay's episode folders are deleted
once read; its check.json stays. A row already written is not run again, so an
interrupted run resumes.

    cold_draws.py run RESULTS.json [RESULTS.json ...] --domain D --sizes 5 10 \\
        --draws 20 --every --out DIR --oracle-cache C
    cold_draws.py report DIR [--boot 2000] [--markdown]

The report gives, per agent, domain, size and flow, the share of turns saved
and the detours across draws (median and range); how many draws save less than
half of what the habit from every task saves; and the shipped arbiter's change
from the habit alone, paired by episode, as the mean over draws with a 95%
interval from resampling the test tasks.
"""

import argparse
import collections
import json
import random
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
FLOWS = ("habit", "arbiter", "shipped")


def agent_of(results: Path, domain: str) -> str:
    """The agent's name in a results file's name: what comes before `_<domain>`.

    >>> agent_of(Path("claude-opus-4-5_high_retail_gpt-5.2_4trials.json"), "retail")
    'claude-opus-4-5_high'
    >>> agent_of(Path("geminipro-retail.json"), "retail")
    'geminipro-retail'
    """
    name = results.name
    cut = name.find(f"_{domain}")
    return name[:cut] if cut > 0 else results.stem


def draw(train: list[str], agent: str, domain: str, size: int, index: int, seed: int) -> list[str]:
    """`size` of the training tasks, drawn at random for one agent, domain,
    size and draw, and the same on every machine.

    >>> train = [str(t) for t in range(20)]
    >>> draw(train, "qwen", "retail", 5, 1, 1)
    ['5', '11', '14', '15', '16']
    >>> draw(train, "qwen", "retail", 5, 2, 1)
    ['1', '12', '13', '14', '18']
    """
    rng = random.Random(f"{agent}|{domain}|{size}|{index}|{seed}")
    return sorted(rng.sample(train, size), key=lambda t: (len(t), t))


def learn(args, results: Path, tasks: list[str] | None, flow: str, out: Path) -> str | None:
    """Learn one flow; None if it was learned, else stretto's error."""
    cmd = [str(args.bin), "learn", "--results", str(results), "--tau2", str(args.tau2),
           "--domain", args.domain, "--trials", "0", "--out", str(out)]
    if tasks is not None:
        cmd += ["--train-tasks", ",".join(tasks)]
    if flow == "habit":
        cmd.append("--habit-only")
    elif flow == "shipped":
        cmd += ["--arbiter-from", str(ROOT / "data/arbiters" / f"{args.domain}.json")]
    else:
        cmd += ["--oracle", "jev", "--oracle-cache", str(args.oracle_cache),
                "--predicates", str(ROOT / "data/predicates-v2.json"),
                "--oracle-budget", str(args.budget)]
    done = subprocess.run(cmd, capture_output=True, text=True)
    if done.returncode == 0:
        return None
    # stretto's own message, not the backtrace after it.
    lines = done.stderr.strip().splitlines()
    return next((line for line in lines if line.startswith("Error:")), "\n".join(lines[-5:]))[:500]


def replay(args, results: Path, flow_file: Path, decider: str, out: Path) -> dict:
    if out.exists():
        shutil.rmtree(out)
    cmd = [args.python, "check_flow.py", "--results", str(results), "--trials", "0", "1", "2", "3",
           "--domain", args.domain, "--flow", str(flow_file), "--flow-decider", decider,
           "--flow-oracle", "jev", "--flow-threshold", "0.3", "--oracle-cache", str(args.oracle_cache),
           "--in-process", "--jobs", str(args.jobs), "--out", str(out), "--tau2", str(args.tau2)]
    subprocess.run(cmd, cwd=ROOT / "pilot", capture_output=True, text=True, check=True)
    check = json.loads((out / "check.json").read_text())
    for episode in out.iterdir():
        if episode.is_dir():
            shutil.rmtree(episode)
    keep = ("task_id", "turns", "turns_saved", "detours", "flow_lookups")
    return {
        "total": check["total"],
        "episodes": [{k: e.get(k) for k in keep} | {"episode": e["episode"]} for e in check["episodes"]],
    }


def cmd_run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "flows").mkdir(exist_ok=True)
    rows_path = args.out / "rows.jsonl"
    done = set()
    if rows_path.exists():
        for line in rows_path.read_text().splitlines():
            r = json.loads(line)
            done.add((r["agent"], r["domain"], r["size"], r["draw"], r["flow"]))
    split = json.loads((args.tau2 / f"data/tau2/domains/{args.domain}/split_tasks.json").read_text())
    train = [str(t) for t in split["train"]]
    agents = [(agent_of(r, args.domain), r) for r in args.results]
    # Every task first, then draw by draw, so that a run cut short has every
    # agent at the same number of draws.
    plan = []
    if args.every:
        plan += [(agent, results, "every", 0, "habit") for agent, results in agents]
    for index in range(1, args.draws + 1):
        for size in args.sizes:
            for agent, results in agents:
                plan += [(agent, results, size, index, flow) for flow in args.flows]
    with rows_path.open("a") as rows:
        for agent, results, size, index, flow in plan:
            key = (agent, args.domain, size, index, flow)
            if key in done:
                continue
            tasks = None if size == "every" else draw(train, agent, args.domain, size, index, args.seed)
            name = f"{agent}-{args.domain}-{size}-{index}-{flow}"
            flow_file = args.out / "flows" / f"{name}.flow.json"
            row = {"agent": agent, "domain": args.domain, "size": size, "draw": index,
                   "flow": flow, "tasks": tasks}
            error = learn(args, results, tasks, flow, flow_file)
            if error is None:
                decider = "habit" if flow == "habit" else "arbiter"
                try:
                    row |= replay(args, results, flow_file, decider, args.out / "replays" / name)
                except subprocess.CalledProcessError as e:
                    error = (e.stderr or "").strip()[-500:] or f"check_flow.py exited {e.returncode}"
            if error is not None:
                row["error"] = error
            rows.write(json.dumps(row) + "\n")
            rows.flush()
            total = row.get("total", {})
            print(f"{name}: saved {total.get('turns_saved')} of {total.get('turns')}, "
                  f"detours {total.get('detours')}{' ERROR' if error else ''}", flush=True)


def interval(values: list[float]) -> tuple[float, float]:
    """The 95% interval of bootstrap values.

    >>> interval([float(x) for x in range(101)])
    (2.0, 97.0)
    """
    ordered = sorted(values)
    return ordered[int(0.025 * (len(ordered) - 1))], ordered[int(0.975 * (len(ordered) - 1))]


def cmd_report(args):
    rows = [json.loads(line) for line in (args.out / "rows.jsonl").read_text().splitlines()]
    rows = [r for r in rows if "error" not in r]
    every = {(r["agent"], r["domain"]): r["total"] for r in rows if r["size"] == "every"}
    by = collections.defaultdict(list)
    for r in rows:
        if r["size"] != "every":
            by[(r["agent"], r["domain"], r["size"], r["flow"])].append(r)
    lines = []
    head = ("| Agent | Domain | Sessions | Flow | Draws | Turns saved, median (range) | "
            "Detours, median (range) | Below half of every task |")
    lines += [head, "|" + "---|" * 8]
    for (agent, domain, size, flow), rs in sorted(by.items(), key=lambda kv: (kv[0][0], kv[0][1], kv[0][2], FLOWS.index(kv[0][3]))):
        share = [100 * r["total"]["turns_saved"] / r["total"]["turns"] for r in rs]
        detours = [r["total"]["detours"] for r in rs]
        ref = every.get((agent, domain))
        below = "–"
        if ref:
            half = ref["turns_saved"] / 2
            below = f"{sum(r['total']['turns_saved'] < half for r in rs)} of {len(rs)}"
        lines.append(
            f"| {agent} | {domain} | {size} | {flow} | {len(rs)} | "
            f"{statistics.median(share):.1f}% ({min(share):.1f}–{max(share):.1f}) | "
            f"{statistics.median(detours):.0f} ({min(detours)}–{max(detours)}) | {below} |"
        )
    lines += ["", "| Agent | Domain | Sessions | Every task: turns saved · detours | "
              "Shipped − habit: turns saved (95%) | Detours (95%) |", "|" + "---|" * 6]
    for agent, domain, size in sorted({k[:3] for k in by}):
        # A stream per row, so that each interval is the same whatever else the rows hold.
        rng = random.Random(f"{args.seed}|{agent}|{domain}|{size}")
        habit = {r["draw"]: r for r in by.get((agent, domain, size, "habit"), [])}
        shipped = {r["draw"]: r for r in by.get((agent, domain, size, "shipped"), [])}
        draws = sorted(set(habit) & set(shipped))
        if not draws:
            continue
        # Per task, per draw: the difference, summed over the task's episodes.
        diff = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0]))
        for d in draws:
            for side, sign in ((shipped[d], 1), (habit[d], -1)):
                for e in side["episodes"]:
                    diff[e["task_id"]][d][0] += sign * e["turns_saved"]
                    diff[e["task_id"]][d][1] += sign * e["detours"]
        tasks = sorted(diff)

        def mean_over_draws(sample: list[str]) -> tuple[float, float]:
            saved = sum(diff[t][d][0] for t in sample for d in draws) / len(draws)
            det = sum(diff[t][d][1] for t in sample for d in draws) / len(draws)
            return saved, det

        point = mean_over_draws(tasks)
        boots = [mean_over_draws([rng.choice(tasks) for _ in tasks]) for _ in range(args.boot)]
        lo_s, hi_s = interval([b[0] for b in boots])
        lo_d, hi_d = interval([b[1] for b in boots])
        ref = every.get((agent, domain))
        ref_text = f"{ref['turns_saved']} · {ref['detours']}" if ref else "–"
        lines.append(
            f"| {agent} | {domain} | {size} | {ref_text} | {point[0]:+.1f} ({lo_s:+.1f} to {hi_s:+.1f}) | "
            f"{point[1]:+.1f} ({lo_d:+.1f} to {hi_d:+.1f}) |"
        )
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run")
    run.add_argument("results", type=Path, nargs="+", help="each agent's τ²-bench results")
    run.add_argument("--domain", required=True)
    run.add_argument("--sizes", type=int, nargs="+", default=[5, 10])
    run.add_argument("--draws", type=int, default=20)
    run.add_argument("--flows", nargs="+", default=list(FLOWS), choices=FLOWS)
    run.add_argument("--every", action="store_true", help="also the habit from every training task")
    run.add_argument("--seed", type=int, default=1)
    run.add_argument("--out", type=Path, required=True)
    run.add_argument("--oracle-cache", type=Path, required=True)
    run.add_argument("--budget", type=float, default=1.0, help="each learn's --oracle-budget, in dollars")
    run.add_argument("--tau2", type=Path, default=ROOT.parent / "tau2-bench")
    run.add_argument("--bin", type=Path, default=ROOT / "target/release/stretto")
    run.add_argument("--python", default=sys.executable, help="the Python with τ²-bench, for the replays")
    run.add_argument("--jobs", type=int, default=4)
    run.set_defaults(run=cmd_run)
    rep = sub.add_parser("report")
    rep.add_argument("out", type=Path)
    rep.add_argument("--boot", type=int, default=2000)
    rep.add_argument("--seed", type=int, default=1)
    rep.set_defaults(run=cmd_report)
    args = ap.parse_args()
    for path in ("tau2", "oracle_cache", "out", "bin"):
        if hasattr(args, path):
            setattr(args, path, getattr(args, path).resolve())
    args.run(args)


if __name__ == "__main__":
    main()
