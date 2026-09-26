#!/usr/bin/env python3
"""Replays' savings with intervals, and their share of the read-only ceiling.

For each replay folder that `pilot/check_flow.py` wrote (its `check.json`
holds a row per episode), the share of LLM turns saved, the detours per
episode, and the share of the ceiling (`scripts/ceiling.py --json`) the
flow reached, each with a 95% interval from a bootstrap over tasks: a task's
four trials are resampled together, since they share the task's ticket.

    replay_stats.py --ceiling CEILING.json REPLAY_DIR... [--json OUT]

A folder is matched to its agent's ceiling by name: `c-<domain>-<results file
name>`, as the replays in the paper name them, or the results file name alone.
"""

import argparse
import json
import random
from pathlib import Path


def rows(replay: Path) -> list[dict]:
    return json.loads((replay / "check.json").read_text())["episodes"]


def interval(values: list[float]) -> tuple[float, float]:
    v = sorted(values)
    return v[int(0.025 * len(v))], v[int(0.975 * len(v)) - 1]


def stats(eps: list[dict], ceiling: dict, draws: int, seed: int) -> dict:
    by_task: dict[str, list[dict]] = {}
    for e in eps:
        by_task.setdefault(str(e["task_id"]), []).append(e)
    tasks = sorted(by_task)

    def measure(sample: list[str]) -> tuple[float, float, float]:
        chosen = [e for t in sample for e in by_task[t]]
        turns = sum(e["turns"] for e in chosen)
        saved = sum(e["turns_saved"] for e in chosen)
        top = sum(ceiling[e["episode"]]["ceiling"] for e in chosen)
        return saved / max(turns, 1), sum(e["detours"] for e in chosen) / max(len(chosen), 1), saved / max(top, 1)

    point = measure(tasks)
    rng = random.Random(seed)
    boot = [measure([rng.choice(tasks) for _ in tasks]) for _ in range(draws)]
    out = {
        "episodes": len(eps),
        "turns": sum(e["turns"] for e in eps),
        "saved": sum(e["turns_saved"] for e in eps),
        "detours": sum(e["detours"] for e in eps),
        "ceiling": sum(ceiling[e["episode"]]["ceiling"] for e in eps),
    }
    for i, name in enumerate(("saved_share", "detours_per_episode", "share_of_ceiling")):
        lo, hi = interval([b[i] for b in boot])
        out[name] = [round(point[i], 4), round(lo, 4), round(hi, 4)]
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("replays", nargs="+", type=Path)
    ap.add_argument("--ceiling", type=Path, required=True, help="scripts/ceiling.py --json")
    ap.add_argument("--draws", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    ceilings = json.loads(args.ceiling.read_text())
    report = {}
    for replay in args.replays:
        name = replay.name
        for d in ("retail", "airline", "telecom"):
            name = name.removeprefix(f"c-{d}-")
        if name not in ceilings:
            print(f"{replay.name}: no ceiling for {name}, skipped")
            continue
        s = stats(rows(replay), ceilings[name]["per_episode"], args.draws, args.seed)
        report[replay.name] = {"domain": ceilings[name]["domain"], **s}
        f = lambda x, pct=True: f"{100 * x[0]:.1f}% [{100 * x[1]:.1f}, {100 * x[2]:.1f}]" if pct else f"{x[0]:.2f} [{x[1]:.2f}, {x[2]:.2f}]"
        print(f"{ceilings[name]['domain']:8s} {name[:40]:40s} saved {f(s['saved_share'])}  detours/ep {f(s['detours_per_episode'], False)}"
              f"  of ceiling {f(s['share_of_ceiling'])}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=1) + "\n")


if __name__ == "__main__":
    main()
