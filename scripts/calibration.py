#!/usr/bin/env python3
"""Is a flow's probability of use calibrated?

For each lookup a flow weighed (the options of every decision a replay with
`pilot/check_flow.py --explore 0` logged), its score is the decider's chance
of the tool (the habit's next-step probability, or with `--flow-decider
reach` its probability of coming before the next write) times the binding's
chance of the arguments, $q = p\\,\\rho$, and its label is whether the agent
made that call later, before its next write (`used`), as the replay rule has
it. Per domain: the lookups binned by score with the share used in each; the
expected calibration error (bins weighted by their lookups); the Brier
score; and the AUC, the chance that a used lookup scores above an unused one.
Each decision's options count, so a lookup weighed at several decisions
counts at each. What a threshold buys is measured by replaying at it.

    calibration.py REPLAY_DIR... [--json OUT]

A folder's domain is read from its name (`c-<domain>-...`, or `solo`).
"""

import argparse
import collections
import json
from pathlib import Path

BINS = [0.0, 0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0001]


def domain_of(name: str) -> str:
    for d in ("retail", "airline", "telecom", "solo"):
        if name.startswith(f"c-{d}-") or f"-{d}" in name:
            return d
    return "other"


def lookups(replay: Path) -> tuple[list[tuple[float, bool, float]], int]:
    """(score, used, turn share) per bound option of every logged decision,
    and the replay's episodes."""
    out = []
    for line in (replay / "decisions.jsonl").read_text().splitlines():
        d = json.loads(line)
        for o, lab in zip(d["policy"]["options"], d["labels"]):
            if o.get("arguments") is None:
                continue
            out.append((o["p"] * (o.get("binding") or 0.0), lab["used"], lab["turn"]))
    episodes = json.loads((replay / "check.json").read_text())["episodes"]
    return out, len(episodes)


def reliability(rows):
    bins = []
    for lo, hi in zip(BINS, BINS[1:]):
        inside = [(s, u) for s, u, _ in rows if lo <= s < hi]
        if inside:
            bins.append({"bin": [lo, round(min(hi, 1.0), 2)], "lookups": len(inside),
                         "score": round(sum(s for s, _ in inside) / len(inside), 3),
                         "used": round(sum(u for _, u in inside) / len(inside), 3)})
    n = len(rows)
    ece = sum(b["lookups"] / n * abs(b["score"] - b["used"]) for b in bins) if n else 0.0
    brier = sum((s - u) ** 2 for s, u, _ in rows) / n if n else 0.0
    return bins, round(ece, 4), round(brier, 4)


def auc(rows):
    """The chance a used lookup outscores an unused one (ties count half)."""
    ranked = sorted(rows, key=lambda r: r[0])
    pos = sum(1 for _, u, _ in ranked if u)
    neg = len(ranked) - pos
    if not pos or not neg:
        return None
    # Mid-ranks over ties, then the Mann-Whitney U of the used lookups.
    rank_sum, i = 0.0, 0
    while i < len(ranked):
        j = i
        while j < len(ranked) and ranked[j][0] == ranked[i][0]:
            j += 1
        mid = (i + j + 1) / 2
        rank_sum += mid * sum(1 for r in ranked[i:j] if r[1])
        i = j
    return round((rank_sum - pos * (pos + 1) / 2) / (pos * neg), 4)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("replays", nargs="+", type=Path)
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    by_domain = collections.defaultdict(lambda: ([], 0))
    for replay in args.replays:
        if not (replay / "decisions.jsonl").exists():
            print(f"{replay.name}: no decisions.jsonl (replay with --explore 0), skipped")
            continue
        rows, episodes = lookups(replay)
        d = domain_of(replay.name)
        prev_rows, prev_eps = by_domain[d]
        by_domain[d] = (prev_rows + rows, prev_eps + episodes)
    report = {}
    for d, (rows, episodes) in sorted(by_domain.items()):
        bins, ece, brier = reliability(rows)
        report[d] = {"lookups": len(rows), "episodes": episodes, "ece": ece, "brier": brier, "auc": auc(rows),
                     "reliability": bins}
        print(f"{d}: {len(rows)} weighed lookups over {episodes} episodes; ECE {ece:.3f}, Brier {brier:.3f}, "
              f"AUC {report[d]['auc']}")
        for b in bins:
            print(f"  score {b['bin'][0]:.2f}-{b['bin'][1]:.2f}: {b['lookups']:6d} lookups, mean score {b['score']:.3f}, used {b['used']:.3f}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=1) + "\n")


if __name__ == "__main__":
    main()
