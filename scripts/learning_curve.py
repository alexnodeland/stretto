#!/usr/bin/env python3
"""How a flow's savings grow with the sessions it learns from.

A deployment learns from the sessions it serves, one after another. This
simulates that: for an agent's τ²-bench results, it orders the agent's
training-task episodes at random (a seed per order), learns a habit-only flow
from the first n of them (`stretto learn --results`), and replays the flow on
the agent's own test-task episodes (`pilot/check_flow.py`), for each n.
n = 0 is the agent alone: a flow learned from nothing makes no lookup.
Episodes are drawn whole, whether or not they succeeded, as sessions arrive;
`learn` keeps the successful ones.

With --pool, the flow learns from other agents' sessions instead (the
episodes of the files given to --pool), and is replayed on the agent's.

    learning_curve.py RESULTS.json --domain D --n 30 60 100 200 --seeds 1 2 3 \\
        --out DIR [--pool OTHER.json ...] [--bin target/release/stretto]

Each point is a row of DIR/curve.jsonl: n, seed, turns, turns saved, detours,
lookups, and the replay folder, whose check.json has a row per episode.
"""

import argparse
import json
import random
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def train_episodes(files: list[Path], tau2: Path, domain: str) -> tuple[dict, list[dict]]:
    split = json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())
    train = set(split["train"])
    first = json.loads(files[0].read_text())
    sims = []
    for f in files:
        data = first if f == files[0] else json.loads(f.read_text())
        sims += [s for s in data["simulations"] if str(s["task_id"]) in train]
    return first, sims


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", type=Path, help="the agent's τ²-bench results: its test episodes are replayed")
    ap.add_argument("--domain", required=True)
    ap.add_argument("--n", type=int, nargs="+", required=True, help="numbers of sessions to learn from (0 = none)")
    ap.add_argument("--seeds", type=int, nargs="+", default=[1])
    ap.add_argument("--pool", type=Path, nargs="*", help="learn from these results' training episodes instead")
    ap.add_argument("--tau2", type=Path, default=ROOT.parent / "tau2-bench")
    ap.add_argument("--bin", type=Path, default=ROOT / "target/release/stretto")
    ap.add_argument("--python", default=sys.executable, help="the Python with τ²-bench, for the replays")
    ap.add_argument("--oracle-cache", type=Path, required=True)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--solo", action="store_true", help="τ²-bench's no-user mode (telecom)")
    ap.add_argument("--decider", default="habit", choices=["habit", "reach"], help="the flow's decider when replayed")
    ap.add_argument("--threshold", type=float, default=0.3, help="the flow's threshold when replayed")
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    template, pool = train_episodes(args.pool or [args.results], args.tau2, args.domain)
    rows = args.out / "curve.jsonl"
    done = set()
    if rows.exists():
        done = {(r["seed"], r["n"]) for r in map(json.loads, rows.read_text().splitlines())}
    for seed in args.seeds:
        order = list(pool)
        random.Random(seed).shuffle(order)
        for n in args.n:
            if (seed, n) in done:
                continue
            n = min(n, len(order))
            point = args.out / f"s{seed}-n{n}"
            point.mkdir(exist_ok=True)
            if n == 0:
                row = {"seed": seed, "n": 0, "turns_saved": 0, "detours": 0, "flow_lookups": 0}
            else:
                sample = dict(template, simulations=order[:n])
                (point / "sessions.json").write_text(json.dumps(sample))
                learn = [str(args.bin), "learn", "--results", str(point / "sessions.json"), "--tau2", str(args.tau2),
                         "--domain", args.domain, "--habit-only", "--out", str(point / "flow.json")]
                learned = subprocess.run(learn, capture_output=True, text=True)
                if learned.returncode != 0:
                    # Too few sessions to learn from (none succeeded, say):
                    # the deployment serves no flow yet.
                    row = {"seed": seed, "n": n, "turns_saved": 0, "detours": 0, "flow_lookups": 0,
                           "learn_failed": learned.stderr.strip().splitlines()[-1:]}
                    with open(rows, "a") as f:
                        f.write(json.dumps(row) + "\n")
                    print(json.dumps(row), flush=True)
                    continue
                replay = [args.python, str(ROOT / "pilot/check_flow.py"), "--domain", args.domain, "--trials", "0", "1", "2", "3",
                          "--results", str(args.results), "--oracle-cache", str(args.oracle_cache), "--flow-oracle", "replay",
                          "--flow-decider", args.decider, "--flow", str(point / "flow.json"),
                          "--flow-threshold", str(args.threshold),
                          "--in-process", "--jobs", str(args.jobs), "--tau2", str(args.tau2), "--out", str(point / "replay")]
                if args.solo:
                    replay.append("--solo")
                out = subprocess.run(replay, capture_output=True, text=True)
                check = next((line for line in out.stdout.splitlines() if line.startswith("CHECK ")), None)
                if check is None:
                    print(out.stdout[-2000:], out.stderr[-2000:], file=sys.stderr)
                    raise SystemExit(f"replay failed at seed {seed}, n {n}")
                total = json.loads(check[6:])
                row = {"seed": seed, "n": n, **{k: total[k] for k in ("turns", "turns_saved", "detours", "flow_lookups", "episodes")},
                       "replay": str(point / "replay")}
                # The replay's episodes, kept for intervals; its per-episode folders are not.
                for d in (point / "replay").glob("task-*"):
                    subprocess.run(["rm", "-rf", str(d)], check=True)
            with open(rows, "a") as f:
                f.write(json.dumps(row) + "\n")
            print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
