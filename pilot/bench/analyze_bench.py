"""Pair a live run's arms task by task: LLM turns, passes, tokens, credits, and what the flow's lookups were.

For each model's folder (`run_bench_paired.py --out`), each task that ran in
both arms is a pair. It reports the LLM turns in each arm and the share fewer
with the flow, with a 95% interval from a bootstrap over tasks; the tasks that
passed the benchmark's check in each arm; the agent's input and output tokens;
GLM credits; and the flow's lookups, split into those the agent made itself in
the same task without the flow (its own) and the rest (detours), with how many
the agent then made again itself (repeats). Calls are the same when their tool
and arguments are. The pairs where the flow made no lookup ran alike in both
arms, so their difference is run-to-run variation; they are reported apart
from the pairs where the flow acted.

    python analyze_bench.py RUNS/glm RUNS/haiku [--set NAME=DOMAIN,...] [--json OUT] [--md OUT]
"""

import argparse
import json
import math
import random
from collections import defaultdict
from pathlib import Path

PREFIX = "mcp__bench__"


def key(tool: str, arguments: dict) -> str:
    return tool + " " + json.dumps(arguments, sort_keys=True)


def agent_calls(episode: Path) -> list[str]:
    out = []
    events = episode / "events.jsonl"
    for line in events.read_text().splitlines() if events.exists() else []:
        event = json.loads(line)
        if event.get("type") == "assistant":
            for b in event.get("message", {}).get("content", []):
                if b.get("type") == "tool_use" and b.get("name", "").startswith(PREFIX):
                    out.append(key(b["name"][len(PREFIX):], b.get("input") or {}))
    return out


def lookups(episode: Path) -> list[str]:
    flow = episode / "flow.jsonl"
    rows = [json.loads(l) for l in flow.read_text().splitlines()] if flow.exists() else []
    return [key(d.get("tool") or d.get("name") or "", d.get("arguments") or {}) for d in rows if d.get("action") == "lookup"]


def tokens(result: dict) -> tuple[int, int]:
    u = result.get("usage") or {}
    return (u.get("input_tokens", 0) + u.get("cache_read_input_tokens", 0) + u.get("cache_creation_input_tokens", 0),
            u.get("output_tokens", 0))


def ratio_ci(clusters: list[list[tuple[int, int]]], reps: int = 2000, seed: int = 0) -> tuple[float, float, float]:
    """1 - with/without over the pairs, and its bootstrap interval, resampling clusters: a task's pairs, one per model."""
    def share(cs):
        base = sum(p[0] for c in cs for p in c)
        return 1 - sum(p[1] for c in cs for p in c) / base if base else 0.0
    rng = random.Random(seed)
    draws = sorted(share([rng.choice(clusters) for _ in clusters]) for _ in range(reps))
    return share(clusters), draws[int(0.025 * reps)], draws[int(0.975 * reps) - 1]


def sign_test(fewer: int, more: int) -> float:
    """Two-sided exact sign test on the pairs that differ: how often a fair coin splits them this unevenly."""
    n, k = fewer + more, max(fewer, more)
    if n == 0:
        return 1.0
    tail = sum(math.comb(n, i) for i in range(k, n + 1)) / 2 ** n
    return min(1.0, 2 * tail)


def summarize(rs: list[dict]) -> dict:
    clusters = defaultdict(list)
    for row in rs:
        clusters[(row["domain"], row["task"])].append(row["turns"])
    share, lo, hi = ratio_ci(list(clusters.values()))
    fewer = sum(1 for r in rs if r["turns"][1] < r["turns"][0])
    more = sum(1 for r in rs if r["turns"][1] > r["turns"][0])
    return {
        "tasks": len(rs),
        "turns": [sum(r["turns"][0] for r in rs), sum(r["turns"][1] for r in rs)],
        "fewer": share, "fewer_ci": [lo, hi],
        "fewer_pairs": fewer, "more_pairs": more, "sign_p": sign_test(fewer, more),
        "passed": [sum(r["passed"][0] for r in rs), sum(r["passed"][1] for r in rs)],
        "tokens_in": [sum(r["tokens_in"][0] for r in rs), sum(r["tokens_in"][1] for r in rs)],
        "tokens_out": [sum(r["tokens_out"][0] for r in rs), sum(r["tokens_out"][1] for r in rs)],
        "credits": [round(sum(r["credits"][0] for r in rs), 1), round(sum(r["credits"][1] for r in rs), 1)],
        "lookups": sum(r["lookups"] for r in rs), "own": sum(r["own"] for r in rs),
        "detours": sum(r["lookups"] - r["own"] for r in rs), "repeats": sum(r["repeats"] for r in rs),
    }


SETS: dict[str, set[str]] = {}  # --set NAME=DOMAIN,...: more groups, each the pairs of some domains


def groups(rows: list[dict]) -> dict[str, list[dict]]:
    by = defaultdict(list)
    for row in rows:
        by["all"].append(row)
        by[row["domain"]].append(row)
        for name, domains in SETS.items():
            if row["domain"] in domains:
                by[name].append(row)
        # Where the flow made no lookup the arms ran alike, so those pairs measure the run-to-run variation.
        by["all, flow acted" if row["lookups"] else "all, flow silent"].append(row)
    return by


def analyze(folder: Path) -> dict:
    episodes = {}
    for result in folder.glob("*/*/result.json"):
        r = json.loads(result.read_text())
        episodes[(r["arm"], r["domain"], r["task"])] = (result.parent, r)
    arms = sorted({a for a, _, _ in episodes} - {"baseline"})
    first = next(iter(episodes.values()))[1] if episodes else {}
    out = {"model": first.get("model"), "bench": first.get("bench"), "arms": {}}
    for arm in arms:
        rows = []
        for (a, domain, task), (ep, r) in sorted(episodes.items()):
            if a != arm or ("baseline", domain, task) not in episodes:
                continue
            bep, b = episodes[("baseline", domain, task)]
            own_calls = set(agent_calls(bep))
            flow = lookups(ep)
            later = agent_calls(ep)
            rows.append({
                "model": r["model"], "domain": domain, "task": task,
                "turns": (b["llm_turns"], r["llm_turns"]),
                "passed": (b["reward"], r["reward"]),
                "tokens_in": (tokens(b)[0], tokens(r)[0]), "tokens_out": (tokens(b)[1], tokens(r)[1]),
                "credits": (b.get("credits_billed", 0.0), r.get("credits_billed", 0.0)),
                "lookups": len(flow),
                "own": sum(1 for k in flow if k in own_calls),
                "repeats": sum(1 for k in flow if k in later),
                "ending": (b.get("ending"), r.get("ending")),
            })
        summary = {group: summarize(rs) for group, rs in groups(rows).items()}
        out["arms"][arm] = {"summary": summary, "pairs": rows}
    return out


def pool(results: list[dict]) -> list[dict]:
    """Each benchmark's pairs over every model that ran it; the interval resamples tasks, with each task's pairs."""
    pooled = []
    for bench in dict.fromkeys(r["bench"] for r in results):
        runs = [r for r in results if r["bench"] == bench]
        if len(runs) < 2:
            continue
        arms = {}
        for arm in dict.fromkeys(a for r in runs for a in r["arms"]):
            rows = [row for r in runs for row in r["arms"].get(arm, {}).get("pairs", [])]
            arms[arm] = {"summary": {g: summarize(rs) for g, rs in groups(rows).items()}, "pairs": rows}
        pooled.append({"model": " + ".join(r["model"] for r in runs), "bench": bench, "arms": arms})
    return pooled


def markdown(results: list[dict]) -> str:
    lines = ["| Benchmark | Model | Group | Tasks | LLM turns, no flow → flow | Fewer (95% interval) | "
             "Pairs fewer / more (sign test p) | Passed, no flow → flow | Lookups (own / detours / repeated) | "
             "Agent input tokens, no flow → flow |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for res in results:
        for arm, a in res["arms"].items():
            for group in ["all", "all, flow acted", "all, flow silent", *SETS] + sorted(
                    g for g in a["summary"] if not g.startswith("all") and g not in SETS):
                if group not in a["summary"]:
                    continue
                s = a["summary"][group]
                lines.append(
                    f"| {res['bench']} | {res['model']} | {group} | {s['tasks']} | {s['turns'][0]} → {s['turns'][1]} | "
                    f"{s['fewer']:.1%} ({s['fewer_ci'][0]:.1%} to {s['fewer_ci'][1]:.1%}) | "
                    f"{s['fewer_pairs']} / {s['more_pairs']} ({s['sign_p']:.2g}) | "
                    f"{s['passed'][0]:.0f} → {s['passed'][1]:.0f} | "
                    f"{s['lookups']} ({s['own']} / {s['detours']} / {s['repeats']}) | "
                    f"{s['tokens_in'][0]:,} → {s['tokens_in'][1]:,} |")
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("folders", nargs="+", type=Path)
    parser.add_argument("--json", type=Path)
    parser.add_argument("--md", type=Path)
    parser.add_argument("--set", action="append", default=[], metavar="NAME=DOMAIN,...",
                        help="a group of domains to report together, such as those a replay found reads to take in")
    args = parser.parse_args()
    for spec in args.set:
        name, _, domains = spec.partition("=")
        SETS[name] = set(domains.split(","))
    results = [analyze(f) for f in args.folders]
    results += pool(results)
    table = markdown(results)
    print(table)
    if args.json:
        args.json.write_text(json.dumps(results, indent=1))
    if args.md:
        args.md.write_text(table)


if __name__ == "__main__":
    main()
