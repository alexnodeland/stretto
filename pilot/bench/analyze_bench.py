"""Pair a live run's arms task by task: LLM turns, passes, tokens, credits, and what the flow's lookups were.

For each model's folder (`run_bench_paired.py --out`), each task that ran in
both arms is a pair. It reports the LLM turns in each arm and the share fewer
with the flow, with a 95% interval from a bootstrap over tasks; the tasks that
passed the benchmark's check in each arm; the agent's input and output tokens;
GLM credits; and the flow's lookups, split into those the agent made itself in
the same task without the flow (its own) and the rest (detours), with how many
the agent then made again itself (repeats). Calls are the same when their tool
and arguments are.

    python analyze_bench.py RUNS/glm RUNS/haiku [--json OUT] [--md OUT]
"""

import argparse
import json
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


def ratio_ci(pairs: list[tuple[int, int]], reps: int = 2000, seed: int = 0) -> tuple[float, float, float]:
    """1 - with/without over the pairs, and its bootstrap interval."""
    def share(ps):
        base = sum(p[0] for p in ps)
        return 1 - sum(p[1] for p in ps) / base if base else 0.0
    rng = random.Random(seed)
    draws = sorted(share([rng.choice(pairs) for _ in pairs]) for _ in range(reps))
    return share(pairs), draws[int(0.025 * reps)], draws[int(0.975 * reps) - 1]


def analyze(folder: Path) -> dict:
    episodes = {}
    for result in folder.glob("*/*/result.json"):
        r = json.loads(result.read_text())
        episodes[(r["arm"], r["domain"], r["task"])] = (result.parent, r)
    arms = sorted({a for a, _, _ in episodes} - {"baseline"})
    out = {"model": next(iter(episodes.values()))[1]["model"] if episodes else None, "arms": {}}
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
                "domain": domain, "task": task,
                "turns": (b["llm_turns"], r["llm_turns"]),
                "passed": (b["reward"], r["reward"]),
                "tokens_in": (tokens(b)[0], tokens(r)[0]), "tokens_out": (tokens(b)[1], tokens(r)[1]),
                "credits": (b.get("credits_billed", 0.0), r.get("credits_billed", 0.0)),
                "lookups": len(flow),
                "own": sum(1 for k in flow if k in own_calls),
                "repeats": sum(1 for k in flow if k in later),
                "ending": (b.get("ending"), r.get("ending")),
            })
        by = defaultdict(list)
        for row in rows:
            by["all"].append(row)
            by[row["domain"]].append(row)
        summary = {}
        for group, rs in by.items():
            share, lo, hi = ratio_ci([row["turns"] for row in rs])
            summary[group] = {
                "tasks": len(rs),
                "turns": [sum(r["turns"][0] for r in rs), sum(r["turns"][1] for r in rs)],
                "fewer": share, "fewer_ci": [lo, hi],
                "fewer_pairs": sum(1 for r in rs if r["turns"][1] < r["turns"][0]),
                "more_pairs": sum(1 for r in rs if r["turns"][1] > r["turns"][0]),
                "passed": [sum(r["passed"][0] for r in rs), sum(r["passed"][1] for r in rs)],
                "tokens_in": [sum(r["tokens_in"][0] for r in rs), sum(r["tokens_in"][1] for r in rs)],
                "tokens_out": [sum(r["tokens_out"][0] for r in rs), sum(r["tokens_out"][1] for r in rs)],
                "credits": [round(sum(r["credits"][0] for r in rs), 1), round(sum(r["credits"][1] for r in rs), 1)],
                "lookups": sum(r["lookups"] for r in rs), "own": sum(r["own"] for r in rs),
                "detours": sum(r["lookups"] - r["own"] for r in rs), "repeats": sum(r["repeats"] for r in rs),
            }
        out["arms"][arm] = {"summary": summary, "pairs": rows}
    return out


def markdown(results: list[dict]) -> str:
    lines = ["| Model | Group | Tasks | LLM turns, no flow → flow | Fewer (95% interval) | Pairs fewer / more | "
             "Passed, no flow → flow | Lookups (own / detours / repeated) | Agent input tokens, no flow → flow |",
             "|---|---|---|---|---|---|---|---|---|"]
    for res in results:
        for arm, a in res["arms"].items():
            for group in ["all"] + sorted(g for g in a["summary"] if g != "all"):
                s = a["summary"][group]
                lines.append(
                    f"| {res['model']} | {group} | {s['tasks']} | {s['turns'][0]} → {s['turns'][1]} | "
                    f"{s['fewer']:.1%} ({s['fewer_ci'][0]:.1%} to {s['fewer_ci'][1]:.1%}) | "
                    f"{s['fewer_pairs']} / {s['more_pairs']} | {s['passed'][0]:.0f} → {s['passed'][1]:.0f} | "
                    f"{s['lookups']} ({s['own']} / {s['detours']} / {s['repeats']}) | "
                    f"{s['tokens_in'][0]:,} → {s['tokens_in'][1]:,} |")
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("folders", nargs="+", type=Path)
    parser.add_argument("--json", type=Path)
    parser.add_argument("--md", type=Path)
    args = parser.parse_args()
    results = [analyze(f) for f in args.folders]
    table = markdown(results)
    print(table)
    if args.json:
        args.json.write_text(json.dumps(results, indent=1))
    if args.md:
        args.md.write_text(table)


if __name__ == "__main__":
    main()
