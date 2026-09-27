"""Report on a run of paired trials (run_trials.py): each arm, and each pair of arms.

    python analyze_trials.py OUT [--json FILE] [--markdown]

OUT is run_trials.py's output: OUT/DOMAIN/trial-K/ARM/task-ID episodes (an
attempt kept as `task-ID.failed-N` is left out). For each arm, in each domain
and in both:

- pass^1, the share of episodes that passed τ²-bench's database check, and
  pass^k for every k up to the trials each task ran: the chance that k
  trials of a task drawn at random all pass (τ²-bench's estimator, averaged
  over tasks);
- per episode: LLM turns, the agent's input tokens (cached or not) and output
  tokens, its cost at list prices (Claude Code's `total_cost_usd`), the
  customer's, the episode's wall-clock time, and the agent's share of it
  (its turns' `duration_ms`, the flow's lookups included).

Each pair of arms is compared on the episodes of the same task and trial:
the change in total turns, input tokens, the agent's cost, wall-clock time
and the agent's time, with 95% intervals from a bootstrap over tasks (4,000 draws, a task's
trials drawn together, tasks drawn within each domain); an exact sign test on
each task's mean change in turns; and the change in pass^1, with its
interval and an exact McNemar test on the pairs that disagree. For an arm
with a flow against the same arm without one (reach against baseline,
batch-reach against batch), the flow's lookups are split into the agent's own
(the agent made the same call in the paired episode without the flow) and
detours, and a lookup the agent made again later in its episode counts as
repeated.
"""

import argparse
import json
import math
import random
import sys
from pathlib import Path

from analyze_paired import DRAWS, appended, mcnemar_p, session_calls
from run_pilot import episode_summary, repeats

# The comparisons reported, when both arms ran: (without, with).
PAIRS = [
    ("baseline", "reach"),
    ("baseline", "batch"),
    ("batch", "batch-reach"),
    ("reach", "batch-reach"),
    ("baseline", "batch-reach"),
]
# The arm each flow arm is paired with to tell its lookups apart.
WITHOUT_FLOW = {"reach": "baseline", "batch-reach": "batch"}


def episodes(out: Path) -> list[dict]:
    rows = []
    for result in sorted(out.glob("*/trial-*/*/task-*/result.json")):
        episode = result.parent
        if ".failed-" in episode.name:
            continue
        r = json.loads(result.read_text())
        s = episode_summary(episode)
        agent_cost = max((a.get("total_cost_usd") or 0 for a in r["agent_results"]), default=0)
        customer_cost = sum(u.get("total_cost_usd") or 0 for u in r["customer_usage"])
        rows.append({
            "domain": episode.parents[2].name,
            "trial": int(episode.parents[1].name.split("-", 1)[1]),
            "arm": episode.parent.name,
            "task": episode.name.split("-", 1)[1],
            "model": r["model"],
            "customer_model": r["customer_model"],
            "passed": r["reward"] >= 1 - 1e-9,
            "ending": r["ending"],
            "llm_turns": r["llm_turns"],
            "tool_turns": r["tool_turns"],
            "parallel_turns": r["parallel_turns"],
            "agent_calls": r["tool_calls"],
            "flow_lookups": r.get("flow_lookups", 0),
            "repeats": repeats(episode),
            "input_tokens": s["agent_input_tokens"],
            "output_tokens": s["agent_output_tokens"],
            "agent_cost_usd": agent_cost,
            "customer_cost_usd": customer_cost,
            "customer_turns": r["customer_turns"],
            "duration_s": r["duration_s"],
            # The agent's own time: each of its turns, from the message it
            # answers to its reply (Claude Code's per-turn `duration_ms`),
            # the flow's lookups included.
            "agent_time_s": sum(a.get("duration_ms") or 0 for a in r["agent_results"]) / 1000,
            "episode": str(episode),
        })
    return rows


def pass_hat(c: int, n: int, k: int) -> float:
    """τ²-bench's pass^k for a task that passed c of n trials."""
    return math.comb(c, k) / math.comb(n, k) if n >= k else float("nan")


def arm_summary(rows: list[dict]) -> dict:
    by_task: dict[tuple, list[dict]] = {}
    for r in rows:
        by_task.setdefault((r["domain"], r["task"]), []).append(r)
    n = min((len(v) for v in by_task.values()), default=0)
    mean = lambda k: sum(r[k] for r in rows) / len(rows)
    return {
        "episodes": len(rows),
        "tasks": len(by_task),
        "trials_per_task": sorted({len(v) for v in by_task.values()}),
        "passed": sum(r["passed"] for r in rows),
        "pass^k": {
            k: round(sum(pass_hat(sum(r["passed"] for r in v), len(v), k) for v in by_task.values()) / len(by_task), 4)
            for k in range(1, n + 1)
        },
        "llm_turns": sum(r["llm_turns"] for r in rows),
        "per_episode": {
            k: round(mean(k), 4)
            for k in ("llm_turns", "tool_turns", "parallel_turns", "agent_calls", "flow_lookups", "input_tokens",
                      "output_tokens", "agent_cost_usd", "customer_cost_usd", "duration_s", "agent_time_s")
        },
        "calls_per_tool_turn": round(sum(r["agent_calls"] for r in rows) / max(1, sum(r["tool_turns"] for r in rows)), 3),
        "agent_errors": sum(r["ending"] == "agent_error" for r in rows),
    }


def bootstrap(by_task: dict[tuple, list], stat, seed: int = 7) -> list[float]:
    """stat over every pair, and its 95% interval over tasks resampled within
    each domain (a task's pairs together)."""
    rng = random.Random(seed)
    domains: dict[str, list[tuple]] = {}
    for key in sorted(by_task):
        domains.setdefault(key[0], []).append(key)
    point = stat([p for key in sorted(by_task) for p in by_task[key]])
    draws = []
    for _ in range(DRAWS):
        sample = [p for keys in domains.values() for key in (rng.choice(keys) for _ in keys) for p in by_task[key]]
        draws.append(stat(sample))
    draws.sort()
    return [round(x, 4) for x in (point, draws[int(0.025 * DRAWS)], draws[int(0.975 * DRAWS) - 1])]


def sign_test(diffs: list[float]) -> dict:
    """Two-sided exact sign test on per-task changes, ties dropped."""
    fewer, more = sum(d < 0 for d in diffs), sum(d > 0 for d in diffs)
    n = fewer + more
    p = 1.0 if n == 0 else min(1.0, 2 * sum(math.comb(n, i) for i in range(min(fewer, more) + 1)) / 2 ** n)
    return {"tasks_fewer": fewer, "tasks_more": more, "tasks_same": len(diffs) - n, "p": round(p, 5)}


def lookups(without: dict, with_: dict) -> dict:
    """The flow's lookups in `with_`, the agent's own or detours against `without`."""
    key = lambda name, arguments: (name, json.dumps(arguments, sort_keys=True))
    own = {key(c["name"], c["arguments"]) for c in session_calls(Path(without["episode"]))}
    counts = {"lookups": 0, "own": 0, "detours": 0}
    for call in session_calls(Path(with_["episode"])):
        for lookup in appended(call["text"]):
            counts["lookups"] += 1
            counts["own" if key(lookup["name"], lookup["arguments"]) in own else "detours"] += 1
    return counts


def compare(rows: list[dict], a: str, b: str) -> dict | None:
    index = {(r["domain"], r["task"], r["trial"], r["arm"]): r for r in rows}
    by_task: dict[tuple, list[tuple]] = {}
    for (domain, task, trial, arm), r in sorted(index.items()):
        if arm == a and (domain, task, trial, b) in index:
            by_task.setdefault((domain, task), []).append((r, index[(domain, task, trial, b)]))
    if not by_task:
        return None
    pairs = [p for v in by_task.values() for p in v]
    change = lambda k: lambda ps: sum(y[k] for _, y in ps) / sum(x[k] for x, _ in ps) - 1
    pass_diff = lambda ps: (sum(y["passed"] for _, y in ps) - sum(x["passed"] for x, _ in ps)) / len(ps)
    b_only = sum(y["passed"] and not x["passed"] for x, y in pairs)
    a_only = sum(x["passed"] and not y["passed"] for x, y in pairs)
    task_diffs = [sum(y["llm_turns"] - x["llm_turns"] for x, y in v) / len(v) for v in by_task.values()]
    res = {
        "arms": [a, b],
        "pairs": len(pairs),
        "tasks": len(by_task),
        "llm_turns": [sum(x["llm_turns"] for x, _ in pairs), sum(y["llm_turns"] for _, y in pairs)],
        "turns_change": bootstrap(by_task, change("llm_turns")),
        "turns_sign_test": sign_test(task_diffs),
        "pairs_fewer_turns": sum(y["llm_turns"] < x["llm_turns"] for x, y in pairs),
        "pairs_more_turns": sum(y["llm_turns"] > x["llm_turns"] for x, y in pairs),
        "input_tokens_change": bootstrap(by_task, change("input_tokens")),
        "agent_cost_change": bootstrap(by_task, change("agent_cost_usd")),
        "cost_change": bootstrap(
            by_task,
            lambda ps: sum(y["agent_cost_usd"] + y["customer_cost_usd"] for _, y in ps)
            / sum(x["agent_cost_usd"] + x["customer_cost_usd"] for x, _ in ps) - 1,
        ),
        "duration_change": bootstrap(by_task, change("duration_s")),
        "agent_time_change": bootstrap(by_task, change("agent_time_s")),
        "passed": [sum(x["passed"] for x, _ in pairs), sum(y["passed"] for _, y in pairs)],
        "pass_difference": bootstrap(by_task, pass_diff),
        "discordant": {f"{b}_only": b_only, f"{a}_only": a_only},
        "mcnemar_p": round(mcnemar_p(b_only, a_only), 4),
    }
    if WITHOUT_FLOW.get(b) == a:
        counts = {"lookups": 0, "own": 0, "detours": 0}
        for x, y in pairs:
            for k, v in lookups(x, y).items():
                counts[k] += v
        counts["repeated"] = sum(y["repeats"] for _, y in pairs)
        res["flow"] = counts
    return res


def pct(x: float) -> str:
    return f"{x * 100:+.1f}%"


def markdown(report: dict) -> str:
    lines = []
    for scope, s in report["scopes"].items():
        lines += [f"### {scope}", "", "| Arm | Episodes | pass^1 | pass^k (k = trials) | Turns/episode | Calls/tool turn | Input tokens | Agent $ | Customer $ | Wall-clock s |",
                  "|---|---|---|---|---|---|---|---|---|---|"]
        for arm, a in s["arms"].items():
            e = a["per_episode"]
            k = max(a["pass^k"], key=int) if a["pass^k"] else None
            lines.append(
                f"| {arm} | {a['episodes']} | {a['pass^k'].get(1, float('nan')):.3f} | "
                + (f"{a['pass^k'][k]:.3f} (k = {k})" if k else "") + f" | {e['llm_turns']:.2f} | {a['calls_per_tool_turn']:.2f} | "
                f"{e['input_tokens']:,.0f} | {e['agent_cost_usd']:.4f} | {e['customer_cost_usd']:.4f} | {e['duration_s']:.0f} |"
            )
        lines += ["", "| Without → with | Pairs | Turns | Change [95%] | Sign test (fewer/more/same, p) | Input tokens | Agent $ | Wall-clock | Agent time | Passed | Pass change [95%] | McNemar p | Lookups (own/detours, repeated) |",
                  "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
        for c in s["comparisons"]:
            t, st = c["turns_change"], c["turns_sign_test"]
            f = c.get("flow")
            lines.append(
                f"| {c['arms'][0]} → {c['arms'][1]} | {c['pairs']} | {c['llm_turns'][0]} → {c['llm_turns'][1]} | "
                f"{pct(t[0])} [{pct(t[1])}, {pct(t[2])}] | {st['tasks_fewer']}/{st['tasks_more']}/{st['tasks_same']}, {st['p']:.3g} | "
                f"{pct(c['input_tokens_change'][0])} | {pct(c['agent_cost_change'][0])} | {pct(c['duration_change'][0])} | {pct(c['agent_time_change'][0])} | "
                f"{c['passed'][0]} → {c['passed'][1]} | {pct(c['pass_difference'][0])} [{pct(c['pass_difference'][1])}, {pct(c['pass_difference'][2])}] | "
                f"{c['mcnemar_p']:.3g} | " + (f"{f['lookups']} ({f['own']}/{f['detours']}, {f['repeated']})" if f else "") + " |"
            )
        lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("out", type=Path)
    parser.add_argument("--json", type=Path, help="write the report here (default OUT/analysis.json)")
    parser.add_argument("--markdown", action="store_true", help="print the tables as Markdown")
    args = parser.parse_args()
    rows = episodes(args.out)
    if not rows:
        sys.exit(f"no episodes under {args.out}")
    arms = [a for a in ("baseline", "reach", "batch", "batch-reach") if any(r["arm"] == a for r in rows)]
    scopes = {d: [r for r in rows if r["domain"] == d] for d in sorted({r["domain"] for r in rows})}
    scopes["both"] = rows
    report = {
        "models": sorted({r["model"] for r in rows}),
        "customer_models": sorted({r["customer_model"] for r in rows}),
        "scopes": {
            scope: {
                "arms": {a: arm_summary([r for r in rs if r["arm"] == a]) for a in arms if any(r["arm"] == a for r in rs)},
                "comparisons": [c for a, b in PAIRS if a in arms and b in arms for c in [compare(rs, a, b)] if c],
            }
            for scope, rs in scopes.items()
        },
        "episodes": [{k: v for k, v in r.items() if k != "episode"} for r in rows],
    }
    (args.json or args.out / "analysis.json").write_text(json.dumps(report, indent=1) + "\n")
    if args.markdown:
        print(markdown(report))
    else:
        print(json.dumps({s: v["comparisons"] for s, v in report["scopes"].items()}, indent=1))


if __name__ == "__main__":
    main()
