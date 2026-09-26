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

    calibration.py REPLAY_DIR... [--versus REPLAY_DIR...] [--json OUT]

With `--versus`, the second set replays the same episodes under another
decider, and each domain's differences (second minus first) get 95% intervals
from a bootstrap over tasks, one resample shared by both sets and all agents.

A folder's domain is read from its name (`c-<domain>-...`, or `solo`).
"""

import argparse
import collections
import json
import random
from pathlib import Path

BINS = [0.0, 0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0001]


def domain_of(name: str) -> str:
    for d in ("retail", "airline", "telecom", "solo"):
        if name.startswith(f"c-{d}-") or f"-{d}" in name:
            return d
    return "other"


def lookups(replay: Path) -> tuple[list[tuple[float, bool, float, str]], int]:
    """(score, used, turn share, task) per bound option of every logged
    decision, and the replay's episodes."""
    out = []
    for line in (replay / "decisions.jsonl").read_text().splitlines():
        d = json.loads(line)
        task = d.get("episode", "").removeprefix("task-").rsplit("-", 1)[0]
        for o, lab in zip(d["policy"]["options"], d["labels"]):
            if o.get("arguments") is None:
                continue
            out.append((o["p"] * (o.get("binding") or 0.0), lab["used"], lab["turn"], task))
    episodes = json.loads((replay / "check.json").read_text())["episodes"]
    return out, len(episodes)


def reliability(rows):
    bins = []
    for lo, hi in zip(BINS, BINS[1:]):
        inside = [(r[0], r[1]) for r in rows if lo <= r[0] < hi]
        if inside:
            bins.append({"bin": [lo, round(min(hi, 1.0), 2)], "lookups": len(inside),
                         "score": round(sum(s for s, _ in inside) / len(inside), 3),
                         "used": round(sum(u for _, u in inside) / len(inside), 3)})
    n = len(rows)
    ece = sum(b["lookups"] / n * abs(b["score"] - b["used"]) for b in bins) if n else 0.0
    brier = sum((r[0] - r[1]) ** 2 for r in rows) / n if n else 0.0
    return bins, round(ece, 4), round(brier, 4)


def auc(rows):
    """The chance a used lookup outscores an unused one (ties count half)."""
    ranked = sorted(rows, key=lambda r: r[0])
    pos = sum(1 for r in ranked if r[1])
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


def load(replays: list[Path]) -> dict:
    by_domain = collections.defaultdict(lambda: ([], 0))
    for replay in replays:
        if not (replay / "decisions.jsonl").exists():
            print(f"{replay.name}: no decisions.jsonl (replay with --explore 0), skipped")
            continue
        rows, episodes = lookups(replay)
        d = domain_of(replay.name)
        prev_rows, prev_eps = by_domain[d]
        by_domain[d] = (prev_rows + rows, prev_eps + episodes)
    return by_domain


def measures(rows) -> tuple[float, float, float | None]:
    _, ece, brier = reliability(rows)
    return ece, brier, auc(rows)


def bootstrap(a_rows, b_rows, draws: int, seed: int = 0) -> dict:
    """95% intervals for b's ECE, Brier and AUC minus a's, over tasks."""
    by_a, by_b = collections.defaultdict(list), collections.defaultdict(list)
    for r in a_rows:
        by_a[r[3]].append(r)
    for r in b_rows:
        by_b[r[3]].append(r)
    tasks = sorted(set(by_a) | set(by_b))
    rng = random.Random(seed)
    diffs = []
    for _ in range(draws):
        sample = [rng.choice(tasks) for _ in tasks]
        ma = measures([r for t in sample for r in by_a.get(t, [])])
        mb = measures([r for t in sample for r in by_b.get(t, [])])
        diffs.append([None if x is None or y is None else y - x for x, y in zip(ma, mb)])
    out = {}
    for i, name in enumerate(("ece", "brier", "auc")):
        v = sorted(d[i] for d in diffs if d[i] is not None)
        out[name] = [round(v[int(0.025 * len(v))], 4), round(v[int(0.975 * len(v)) - 1], 4)] if v else None
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("replays", nargs="+", type=Path)
    ap.add_argument("--versus", nargs="+", type=Path, help="the same episodes replayed under another decider")
    ap.add_argument("--bootstrap", type=int, default=1000, help="draws for --versus (1000)")
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    report = {}
    sets = [("", load(args.replays))] + ([("versus", load(args.versus))] if args.versus else [])
    for label, by_domain in sets:
        for d, (rows, episodes) in sorted(by_domain.items()):
            bins, ece, brier = reliability(rows)
            entry = {"lookups": len(rows), "episodes": episodes, "ece": ece, "brier": brier, "auc": auc(rows), "reliability": bins}
            (report.setdefault(label, {}) if label else report)[d] = entry
            print(f"{label + ' ' if label else ''}{d}: {len(rows)} weighed lookups over {episodes} episodes; ECE {ece:.3f}, "
                  f"Brier {brier:.3f}, AUC {entry['auc']}")
            for b in bins:
                print(f"  score {b['bin'][0]:.2f}-{b['bin'][1]:.2f}: {b['lookups']:6d} lookups, mean score {b['score']:.3f}, used {b['used']:.3f}")
    if args.versus:
        first, second = sets[0][1], sets[1][1]
        for d in sorted(set(first) & set(second)):
            ci = bootstrap(first[d][0], second[d][0], args.bootstrap)
            a, b = report[d], report["versus"][d]
            report.setdefault("difference", {})[d] = {"ece": round(b["ece"] - a["ece"], 4), "brier": round(b["brier"] - a["brier"], 4),
                                                      "auc": round((b["auc"] or 0) - (a["auc"] or 0), 4), "intervals": ci}
            print(f"{d}: versus minus first: ECE {b['ece'] - a['ece']:+.3f} {ci['ece']}, Brier {b['brier'] - a['brier']:+.3f} {ci['brier']}, "
                  f"AUC {(b['auc'] or 0) - (a['auc'] or 0):+.3f} {ci['auc']}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=1) + "\n")


if __name__ == "__main__":
    main()
