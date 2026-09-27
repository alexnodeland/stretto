#!/usr/bin/env python3
"""The tables of docs/results/benchmarks-2026-09-27.md, from the replays and ceilings `replay.sh` writes.

    tables.py [--work DIR] [--json OUT]

Prints each set's read-only ceiling; τ²-bench's trace-only replays against the environment's; per benchmark and
threshold, the turns reach and habit saved pooled over agents, with reach minus habit and a paired bootstrap over
tasks (a task drawn with every agent's episodes of it), and the same per domain; and DTap-Bench by the flow's
source. With --json, writes the rows behind the tables: every replay's totals and episodes, the ceilings per agent,
the trace-versus-environment pairs and the pooled comparisons.
"""

import argparse
import json
import os
import random
import re
from collections import defaultdict
from pathlib import Path

BENCHMARKS = {  # replay set: (name, thresholds)
    "v1": ("τ-bench", ["0.3"]),
    "bfcl": ("BFCL", ["0.3", "0.1", "0.2", "0.5"]),
    "bfcl-ground-truth": ("BFCL ground truth", ["0.3", "0.1", "0.2", "0.5"]),
    "dojo": ("AgentDojo", ["0.3", "0.1"]),
    "dojo-nolists": ("AgentDojo travel, no lists", ["0.3"]),
    "wb": ("WorkBench", ["0.3", "0.1"]),
    "mcpm": ("MCPMark", ["0.3", "0.1"]),
    "dtap": ("DTap-Bench", ["0.3"]),
}
SOURCES = ["own", "same", "other", "all"]
EP = ["episode", "task_id", "turns", "turns_saved", "detours", "flow_lookups", "unrecorded", "used_next", "used_later"]
TOT = [
    "turns",
    "tool_turns",
    "turns_saved",
    "calls",
    "calls_skipped",
    "flow_lookups",
    "flow_queries",
    "detours",
    "unrecorded",
    "used_next",
    "used_later",
    "episodes",
]


def parse(name: str):
    """A replay folder's name, c-<domain>-<agent>[-<source>]-<decider>[-<θ>]: (stem, decider, θ or None)."""
    m = re.fullmatch(r"(?:c|sw)-(.+)-(reach|habit)(?:-([0-9.]+))?", name)
    return (m.group(1), m.group(2), m.group(3) or "0.3") if m else None  # the sets at 0.3 alone leave it out


def replays(folder: Path) -> dict:
    """{(stem, decider, θ): check.json} for the replays in a set's folder."""
    out = {}
    for d in sorted(folder.glob("c-*")) if folder.is_dir() else []:
        p = parse(d.name)
        if p and (d / "check.json").exists():
            out[p] = json.loads((d / "check.json").read_text())
    return out


def per_task(check: dict) -> dict:
    t = defaultdict(lambda: [0, 0, 0, 0])  # turns, saved, detours, episodes
    for e in check["episodes"]:
        r = t[e["task_id"]]
        r[0] += e["turns"]
        r[1] += e["turns_saved"]
        r[2] += e["detours"]
        r[3] += 1
    return t


def compare(pairs, reps=2000, seed=0):
    """Reach against habit over [(stem, reach check, habit check)], pooled, with a paired bootstrap over the tasks of
    each domain (the stem's first part)."""
    by = defaultdict(lambda: [0] * 6)  # turns, saved reach, saved habit, detours reach, detours habit, episodes
    for stem, r, h in pairs:
        tr, th = per_task(r), per_task(h)
        for task, x in tr.items():
            y = th.get(task, [0] * 4)
            a = by[(stem.split("-", 1)[0], task)]
            for i, v in enumerate((x[0], x[1], y[1], x[2], y[2], x[3])):
                a[i] += v
    keys = list(by)
    if not keys:
        return None
    tot = [sum(by[k][i] for k in keys) for i in range(6)]
    rng = random.Random(seed)
    ds, dd = [], []
    for _ in range(reps):
        s = [by[rng.choice(keys)] for _ in keys]
        ds.append(sum(x[1] - x[2] for x in s) / max(1, sum(x[0] for x in s)))
        dd.append(sum(x[3] - x[4] for x in s) / max(1, sum(x[5] for x in s)))
    ds.sort()
    dd.sort()
    lo, hi = int(0.025 * reps), int(0.975 * reps) - 1
    return {
        "agents": len(pairs),
        "episodes": tot[5],
        "turns": tot[0],
        "reach_saved": tot[1],
        "habit_saved": tot[2],
        "reach_detours": tot[3],
        "habit_detours": tot[4],
        "diff_saved_share": (tot[1] - tot[2]) / max(1, tot[0]),
        "diff_saved_ci": (ds[lo], ds[hi]),
        "diff_detours_per_ep": (tot[3] - tot[4]) / max(1, tot[5]),
        "diff_detours_ci": (dd[lo], dd[hi]),
    }


def show(label: str, c: dict, ceiling=None):
    lo, hi = c["diff_saved_ci"]
    dlo, dhi = c["diff_detours_ci"]
    of = (lambda s: f", {s / ceiling:.0%} of the ceiling") if ceiling else (lambda s: "")
    print(
        f"  {label:34s} {c['agents']:3d} agents {c['episodes']:5d} episodes {c['turns']:6d} turns | "
        f"reach {c['reach_saved']:5d} ({c['reach_saved'] / c['turns']:.1%}{of(c['reach_saved'])}) "
        f"habit {c['habit_saved']:5d} ({c['habit_saved'] / c['turns']:.1%}{of(c['habit_saved'])}) "
        f"difference {c['diff_saved_share']:+.2%} [{lo:+.2%}, {hi:+.2%}] | detours an episode "
        f"reach {c['reach_detours'] / c['episodes']:.3f} habit {c['habit_detours'] / c['episodes']:.3f} "
        f"difference {c['diff_detours_per_ep']:+.3f} [{dlo:+.3f}, {dhi:+.3f}]"
    )


def pairs_at(runs: dict, th, keep=lambda stem: True):
    return [
        (s, runs[(s, "reach", t)], runs[(s, "habit", t)])
        for (s, dec, t) in sorted(runs)
        if dec == "reach" and t == th and (s, "habit", t) in runs and keep(s)
    ]


def ceilings(work: Path) -> dict:
    """{set-domain file stem: {agent: ceiling.py's row}} for the ceilings `replay.sh ceilings` wrote."""
    return {p.stem: json.loads(p.read_text()) for p in sorted((work / "ceilings").glob("*.json"))}


def ceiling_of(cz: dict, key: str, stem: str) -> int:
    """The read-only ceiling, in turns, of a replayed agent's test runs, from its set's ceilings."""
    domain, _, agent = stem.partition("-")
    rows = cz.get(f"{key}-{domain}") or cz.get(key) or {}
    agent = re.sub(r"-(own|same|other|all)$", "", agent)
    row = rows.get(agent) if agent else (next(iter(rows.values())) if len(rows) == 1 else None)
    return (row or {}).get("ceiling, tool state", 0)


def anatomy(rows) -> dict:
    a = defaultdict(float)
    for row in rows:
        for k, v in row.items():
            if isinstance(v, (int, float)):
                a[k] += v
    t = a["turns"] or 1
    share = lambda k: (a.get(f"{k}, after tool", 0) + a.get(f"{k}, after customer", 0)) / t  # noqa: E731
    return {
        "episodes": int(a["episodes"]),
        "turns": int(a["turns"]),
        "replies": share("reply"),
        "writes": share("writes"),
        "reads": share("reads"),
        "ceiling": a["ceiling, tool state"] / t,
        "ceiling_turns": int(a["ceiling, tool state"]),
        "with_words": a["ceiling, with words"] / t,
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--work", type=Path, default=Path(os.environ.get("WORK", "work")))
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    work, out = args.work, {"episode_columns": EP}

    print("== Read-only ceilings: episodes, turns, and the share of turns that are replies, writes and reads")
    cz = ceilings(work)
    groups = {f"{k}": list(v.values()) for k, v in cz.items()}
    for prefix in ("dojo-", "wb-", "mcpm-", "dtap-"):
        groups[f"{prefix}all"] = [r for k, v in cz.items() if k.startswith(prefix) for r in v.values()]
    for harness in ("claudesdk", "openaisdk", "googleadk"):
        groups[f"dtap-{harness}"] = [
            r for k, v in cz.items() if k.startswith("dtap-") for a, r in v.items() if a.startswith(harness)
        ]
    out["anatomy"] = {}
    for k, rows in groups.items():
        if not rows:
            continue
        r = out["anatomy"][k] = anatomy(rows)
        print(
            f"  {k:36s} {r['episodes']:5d} episodes {r['turns']:6d} turns | replies {r['replies']:.1%} writes "
            f"{r['writes']:.1%} reads {r['reads']:.1%} | ceiling {r['ceiling']:.1%} ({r['ceiling_turns']} turns), "
            f"with words {r['with_words']:.1%}"
        )
    out["ceilings"] = [
        {"set": k, "agent": a, **{f: v for f, v in row.items() if f != "per_episode"}}
        for k, v in cz.items()
        for a, row in v.items()
    ]

    print("== τ²-bench: trace-only replays against the environment's, at 0.3")
    trace, env = replays(work / "replays/tau2"), replays(work / "replays/tau2-env")
    out["validation"], agg = [], defaultdict(lambda: [0] * 8)
    for (stem, dec, th), t in sorted(trace.items()):
        e = env.get((stem, dec, th))
        if e is None:
            continue
        te = {x["episode"]: x["turns_saved"] for x in t["episodes"]}
        ee = {x["episode"]: x["turns_saved"] for x in e["episodes"]}
        same = sum(1 for k in te if ee.get(k) == te[k])
        out["validation"].append(
            {
                "decider": dec,
                "domain": stem.split("-", 1)[0],
                "agent": stem.split("-", 1)[1],
                "trace": {k: t["total"].get(k) for k in TOT},
                "environment": {k: e["total"].get(k) for k in TOT},
                "episodes_same_saved": same,
                "episodes": len(te),
            }
        )
        for key in ((dec, stem.split("-", 1)[0]), (dec, "all")):
            a = agg[key]
            for i, v in enumerate(
                (
                    t["total"]["turns_saved"],
                    e["total"]["turns_saved"],
                    t["total"]["detours"],
                    e["total"]["detours"],
                    same,
                    len(te),
                    t["total"]["flow_lookups"],
                    e["total"]["flow_lookups"],
                )
            ):
                a[i] += v
    for (dec, d), a in sorted(agg.items(), key=lambda x: (x[0][0], x[0][1] == "all", x[0][1])):
        print(
            f"  {dec:5s} {d:8s} saved {a[0]:5d} trace, {a[1]:5d} environment ({a[0] / max(1, a[1]) - 1:+.1%}) | detours "
            f"{a[2]:4d}, {a[3]:4d} | lookups {a[6]}, {a[7]} | episodes saving the same {a[4]}/{a[5]} ({a[4] / max(1, a[5]):.1%})"
        )

    print("== Reach against habit, pooled over agents (paired bootstrap over tasks, 95%)")
    out["replays"], out["pooled"], out["per_domain"] = [], {}, {}
    for key, (name, ths) in BENCHMARKS.items():
        runs = replays(work / "replays" / key)
        for (stem, dec, th), chk in sorted(runs.items()):
            domain, agent = stem.split("-", 1) if "-" in stem else (stem, "ground-truth")
            out["replays"].append(
                {
                    "set": key,
                    "domain": domain,
                    "agent": agent,
                    "decider": dec,
                    "threshold": float(th),
                    "total": {k: chk["total"].get(k) for k in TOT},
                    "episodes": [[e.get(k) for k in EP] for e in chk["episodes"]],
                }
            )
        for th in ths:
            ps = pairs_at(runs, th, keep=lambda s: key != "dtap" or s.endswith("-other"))
            c = compare(ps)
            if not c:
                continue
            ceil = sum(ceiling_of(cz, key, s) for s, _, _ in ps) or None
            show(f"{name} at {th}" + (" (flows from other harnesses)" if key == "dtap" else ""), c, ceil)
            out["pooled"][f"{key}|{th}"] = c
            domains = sorted({s.split("-", 1)[0] for s, _, _ in ps})
            if th != "0.3" or len(domains) < 2:
                continue
            for d in domains:
                dp = [p for p in ps if p[0].split("-", 1)[0] == d]
                c = compare(dp)
                show(f"   {d}", c, sum(ceiling_of(cz, key, s) for s, _, _ in dp) or None)
                out["per_domain"][f"{key}|{d}|{th}"] = c

    print(
        "== DTap-Bench at 0.3 by the flow's source: the agent's own runs, its harness's other agents, the other "
        "harnesses', all"
    )
    runs = replays(work / "replays/dtap")
    out["dtap_by_source"] = {}
    domains = sorted({s.split("-", 1)[0] for s, _, _ in runs})
    for label, doms in [("all four", domains)] + [(d, [d]) for d in domains]:
        for src in SOURCES:
            ps = pairs_at(runs, "0.3", keep=lambda s: s.split("-", 1)[0] in doms and s.endswith(f"-{src}"))
            c = compare(ps)
            if c:
                out["dtap_by_source"][f"{label}|{src}"] = c
                show(f"{label}, {src}", c)
        own = {
            s[: -len("-own")]: r
            for s, r, _ in pairs_at(runs, "0.3", keep=lambda s: s.split("-", 1)[0] in doms and s.endswith("-own"))
        }
        for src in SOURCES[1:]:
            both = [
                (r, own[s[: -len(f"-{src}")]])
                for s, r, _ in pairs_at(runs, "0.3", keep=lambda s: s.endswith(f"-{src}"))
                if s[: -len(f"-{src}")] in own
            ]
            if both:
                kept = sum(r["total"]["turns_saved"] for r, _ in both) / max(
                    1, sum(o["total"]["turns_saved"] for _, o in both)
                )
                det = sum(r["total"]["detours"] for r, _ in both) / max(1, sum(o["total"]["detours"] for _, o in both))
                print(
                    f"    reach with the {src} flows, against the agent's own over the {len(both)} agents with both: "
                    f"{kept:.0%} of the turns saved, {det:.0%} of the detours"
                )
    if args.json:
        args.json.write_text(json.dumps(out, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
