"""Compare arms recorded at different times on the same tasks, one episode per task and arm.

    python analyze_recorded.py DOMAIN:ARM=DIR[,DIR...] ... [--json FILE]

Each DIR holds `task-<id>` episodes. An arm given several DIRs (two trials of
a baseline) counts each task's mean turns and tokens over them, and its pass
from the first, as the reach round compared its live arm with the paired
run's recorded baseline. Each pair of arms, in the order named, is compared on
the tasks both ran in each domain: the change in total LLM turns and input
tokens (uncached and cache reads, as the reach round counted them), with 95%
intervals from a bootstrap over tasks drawn within each domain (4,000 draws),
the exact sign test on each task's change, and passes with the exact McNemar
test. Unlike analyze_trials.py, the arms here need not have run at the same
time, and the results should say which did not.

    python analyze_recorded.py \\
        retail:baseline=BASE/retail/baseline retail:reach=REACH/retail retail:batch=GLM/retail/batch \\
        airline:baseline=BASE/airline/baseline,BASE/airline/trial-1/baseline airline:reach=REACH/airline ...
"""

import argparse
import json
import random
import statistics
import sys
from pathlib import Path

from analyze_paired import DRAWS, mcnemar_p
from analyze_trials import sign_test


def episode(path: Path) -> dict:
    r = json.loads((path / "result.json").read_text())
    usage = [a["usage"] for a in r["agent_results"] if a.get("usage")]
    return {
        "turns": r["llm_turns"],
        "input": sum(u.get("input_tokens", 0) + u.get("cache_read_input_tokens", 0) for u in usage),
        "passed": r["reward"] >= 1 - 1e-9,
        "lookups": r.get("flow_lookups", 0),
        "calls": r["tool_calls"],
        "tool_turns": r["tool_turns"],
        "parallel_turns": r["parallel_turns"],
    }


def arm(dirs: list[Path]) -> dict[str, dict]:
    """Each task's episode, its turns and tokens averaged over the DIRs that hold it."""
    tasks = {}
    found = {p.name for d in dirs for p in d.glob("task-*") if (p / "result.json").exists()}
    for task in sorted(found, key=lambda name: int(name.split("-", 1)[1])):
        runs = [episode(d / task) for d in dirs if (d / task / "result.json").exists()]
        tasks[task] = {
            "turns": statistics.mean(e["turns"] for e in runs),
            "input": statistics.mean(e["input"] for e in runs),
            "passed": runs[0]["passed"],
            "lookups": statistics.mean(e["lookups"] for e in runs),
            "calls": sum(e["calls"] for e in runs),
            "tool_turns": sum(e["tool_turns"] for e in runs),
            "parallel_turns": sum(e["parallel_turns"] for e in runs),
            "trials": len(runs),
        }
    return tasks


def compare(arms: dict, a: str, b: str) -> dict:
    """b against a, on the tasks both ran, pooled over domains."""
    pairs = {
        d: [(arms[d][a][t], arms[d][b][t]) for t in arms[d][a] if t in arms[d][b]]
        for d in arms if a in arms[d] and b in arms[d]
    }
    pairs = {d: p for d, p in pairs.items() if p}

    def change(k: str, sample: list) -> float:
        return sum(y[k] for _, y in sample) / sum(x[k] for x, _ in sample) - 1

    def interval(k: str) -> list[float]:
        rng = random.Random(7)
        draws = sorted(
            change(k, [p for ps in pairs.values() for p in (rng.choice(ps) for _ in ps)]) for _ in range(DRAWS)
        )
        return [draws[int(0.025 * DRAWS)], draws[int(0.975 * DRAWS)]]

    everything = [p for ps in pairs.values() for p in ps]
    b_only = sum(y["passed"] and not x["passed"] for x, y in everything)
    a_only = sum(x["passed"] and not y["passed"] for x, y in everything)
    return {
        "arms": [a, b],
        "tasks": len(everything),
        "llm_turns": [sum(x["turns"] for x, _ in everything), sum(y["turns"] for _, y in everything)],
        "turns_change": [round(v, 4) for v in [change("turns", everything)] + interval("turns")],
        "turns_sign_test": sign_test([y["turns"] - x["turns"] for x, y in everything]),
        "input_change": [round(v, 4) for v in [change("input", everything)] + interval("input")],
        "passed": [sum(x["passed"] for x, _ in everything), sum(y["passed"] for _, y in everything)],
        "discordant": {f"{b}_only": b_only, f"{a}_only": a_only},
        "mcnemar_p": round(mcnemar_p(b_only, a_only), 4),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("arms", nargs="+", metavar="DOMAIN:ARM=DIR[,DIR...]")
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    arms: dict[str, dict] = {}
    order: list[str] = []
    for spec in args.arms:
        try:
            where, dirs = spec.split("=", 1)
            domain, name = where.split(":", 1)
        except ValueError:
            sys.exit(f"analyze_recorded.py: {spec!r} is not DOMAIN:ARM=DIR[,DIR...]")
        arms.setdefault(domain, {})[name] = arm([Path(d) for d in dirs.split(",")])
        if name not in order:
            order.append(name)
    report = {
        "arms": {
            d: {
                n: {
                    "tasks": len(ts),
                    "llm_turns": sum(t["turns"] for t in ts.values()),
                    "passed": sum(t["passed"] for t in ts.values()),
                    "lookups": sum(t["lookups"] for t in ts.values()),
                    "calls_per_tool_turn": round(
                        sum(t["calls"] for t in ts.values()) / max(1, sum(t["tool_turns"] for t in ts.values())), 3),
                }
                for n, ts in by_arm.items()
            }
            for d, by_arm in arms.items()
        },
        "comparisons": {
            scope: [compare(subset, a, b) for i, a in enumerate(order) for b in order[i + 1:]]
            for scope, subset in [(d, {d: arms[d]}) for d in arms] + [("both", arms)]
        },
    }
    if args.json:
        args.json.write_text(json.dumps(report, indent=1) + "\n")
    for scope, comparisons in report["comparisons"].items():
        for c in comparisons:
            t, i, st = c["turns_change"], c["input_change"], c["turns_sign_test"]
            print(
                f"{scope:8s} {c['arms'][0]:>9s} -> {c['arms'][1]:<9s} {c['tasks']:3d} tasks  turns {c['llm_turns'][0]:g} -> "
                f"{c['llm_turns'][1]:g} {t[0]:+.1%} [{t[1]:+.1%}, {t[2]:+.1%}]  fewer/more/same {st['tasks_fewer']}/"
                f"{st['tasks_more']}/{st['tasks_same']} p={st['p']:.3g}  input {i[0]:+.1%} [{i[1]:+.1%}, {i[2]:+.1%}]  "
                f"passed {c['passed'][0]} -> {c['passed'][1]} (McNemar p={c['mcnemar_p']:.2f})"
            )


if __name__ == "__main__":
    main()
