"""Compare ways of deciding a flow's lookups at one domain's held-out
decisions (docs/results/telecom-2026-09-25.md; stretto #10).

    python3 scripts/compare_arbiters.py LOG FLOW [ARBITER ...] [--json OUT]

LOG is a decision log from `stretto compile --oracle-log`, and FLOW the flow
that compile wrote: its five fold arbiters judge each decision as the flow
would (a fold never saw the decision's task). Each ARBITER file (from
`export-arbiter` or `fit-arbiter`) judges every decision instead, as a
shipped arbiter would, and "habit" takes the habit's own probabilities.
At a site an arbiter never saw, it weighs Jev's pick by its record over all
the sites it was fitted on.

Each decider takes the likeliest lookup when its probability reaches the
threshold, as a flow does, and hands back otherwise. The binding's chance
is left out, which can only add lookups. Against the agent's own next step,
a lookup is right when it is the lookup the agent made, and a detour when it
is not.
"""

import json
import math
import sys
from pathlib import Path

RESPOND = "respond"
SHRINK = 10.0
FOLDS = 5
THRESHOLDS = (0.3, 0.5)


def logit(p: float) -> float:
    p = min(max(p, 1e-4), 1 - 1e-4)
    return math.log(p / (1 - p))


def task_group(task_id: str) -> int:
    h = 1469598103934665603
    for b in task_id.encode():
        h = ((h ^ b) * 1099511628211) % (1 << 64)
    return h


class Arbiter:
    """A fitted arbiter, as crates/stretto-report/src/arbitrate.rs judges."""

    def __init__(self, fitted: dict):
        self.w = fitted["weights"]
        self.sites = {s: tuple(v) for s, v in fitted["rates"]["sites"]}
        self.agree, self.cover = fitted["rates"]["agree"], fitted["rates"]["cover"]

    def probs(self, case: dict) -> list:
        n, agreed, covered = self.sites.get(case["site"], (0.0, 0.0, 0.0))
        r = logit((agreed + SHRINK * self.agree) / (n + SHRINK))
        cover = (covered + SHRINK * self.cover) / (n + SHRINK)
        scores = [sum(a * b for a, b in zip(x + [r if i == case["pick"] else 0.0], self.w))
                  for i, x in enumerate(case["features"])]
        m = max(scores)
        z = m + math.log(sum(math.exp(s - m) for s in scores))
        return [math.exp(s - z) * cover for s in scores]


def habit_probs(case: dict) -> list:
    return [math.exp(x[0]) for x in case["features"]]


def lookup(options: list, probs: list, threshold: float):
    best = None
    for o, p in zip(options, probs):
        if o != RESPOND and (best is None or p > best[1]):
            best = (o, p)
    return best[0] if best and best[1] >= threshold else None


def score(decisions: list, judge) -> dict:
    out = {}
    for threshold in THRESHOLDS:
        made = right = agree = agent_lookups = 0
        for d in decisions:
            case = d["case"]
            pick = lookup(case["options"], judge(d), threshold)
            actual = d["actual"]
            agent_lookups += actual != RESPOND
            made += pick is not None
            right += pick is not None and pick == actual
            agree += (pick or RESPOND) == actual
        out[str(threshold)] = {
            "decisions": len(decisions), "agent_lookups": agent_lookups, "lookups": made,
            "right": right, "detours": made - right, "agreement": round(agree / len(decisions), 4),
            "share_of_agent_lookups": round(right / agent_lookups, 4) if agent_lookups else 0.0,
        }
    return out


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    out_path = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    if out_path:
        args.remove(out_path)
    log, flow_path, arbiter_paths = Path(args[0]), Path(args[1]), [Path(a) for a in args[2:]]
    decisions = [d for d in map(json.loads, log.read_text().splitlines())
                 if d.get("case") and not d.get("target")]
    folds = [Arbiter(f) for f in json.loads(flow_path.read_text())["folds"]]
    results = {
        "habit alone": score(decisions, lambda d: habit_probs(d["case"])),
        "own arbiter (cross-fitted)": score(
            decisions, lambda d: folds[task_group(d["task"]) % FOLDS].probs(d["case"])),
    }
    for path in arbiter_paths:
        a = json.loads(path.read_text())
        arbiter = Arbiter(a["fitted"])
        results[f"{a['domain']} arbiter"] = score(decisions, lambda d, arbiter=arbiter: arbiter.probs(d["case"]))
    for name, r in results.items():
        cells = "; ".join(f"at {t}: {v['lookups']} lookups, {v['right']} right, {v['detours']} detours, "
                          f"agreement {v['agreement']:.1%}" for t, v in r.items())
        print(f"{name}: {cells}")
    if out_path:
        Path(out_path).write_text(json.dumps(results, indent=1) + "\n")


if __name__ == "__main__":
    main()
