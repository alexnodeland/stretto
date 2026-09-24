"""Run the paired live pilot: the same test-split tasks in both arms.

The tasks are a seeded sample of the domain's test split, so the habit
never trained on them. Each task runs once in the baseline arm and once in
the flows arm, one episode at a time, and an episode already recorded is
kept, so an interrupted pilot resumes. The summary (`pilot.json`,
`pilot.md`) pairs the arms task by task: LLM turns, the agent's input
tokens, Z.ai credits, τ²-bench's database check, and how often the agent
repeated a lookup the flow had already made (from the proxy's log).

Credits follow Z.ai's GLM Coding Plan: (input × 6.9 + cached input × 1.7 +
output × 24) / 10,000 for GLM-5.3, halved off-peak (outside Monday to
Friday 14:00–18:00 Singapore time, and all day from 2026-09-25 to
2026-10-07).

    python run_pilot.py --tasks 10 --seed 7 --out runs/pilot --oracle-cache CACHE

`--arms baseline guards` pairs the baseline with the guards arm instead
(`--arms baseline habit`: the flow on the habit alone), and
`--task-ids` names the tasks instead of sampling them (the guards pilot picks
the tasks where published trajectories show the guards firing, and says so).
`--trials` runs each task that many times per arm, under `trial-<k>/`.
"""

import argparse
import json
import random
import statistics
import subprocess
import sys
from datetime import date, datetime, timezone
from pathlib import Path

from check_flow import flow_calls, key

HERE = Path(__file__).resolve().parent
ARMS = ["baseline", "flows"]
RATES = {"input_tokens": 6.9, "cache_read_input_tokens": 1.7, "cache_creation_input_tokens": 6.9, "output_tokens": 24.0}
ALL_DAY_OFF_PEAK = (date(2026, 9, 25), date(2026, 10, 7))


def credits(usage: dict) -> float:
    return sum(usage.get(k, 0) * r for k, r in RATES.items()) / 10_000


def off_peak(start: str) -> bool:
    t = datetime.fromisoformat(start).astimezone(timezone.utc)
    if ALL_DAY_OFF_PEAK[0] <= t.date() <= ALL_DAY_OFF_PEAK[1]:
        return True
    # Peak: Monday to Friday, 14:00-18:00 at UTC+8, i.e. 06:00-10:00 UTC.
    return t.weekday() >= 5 or not 6 <= t.hour < 10


def sample(tau2: Path, domain: str, n: int, seed: int, exclude: set[str]) -> list[str]:
    split = json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())
    pool = sorted((t for t in split["test"] if t not in exclude), key=int)
    return sorted(random.Random(seed).sample(pool, n), key=int)


def repeats(directory: Path) -> int:
    """The agent's calls that repeat a lookup the flow made earlier."""
    by_flow: set[tuple[str, str]] = set()
    n = 0
    for log in sorted((directory / "log").glob("*.jsonl")):
        for line in log.read_text().splitlines()[1:]:
            entry = json.loads(line)
            m = entry.get("message", {})
            if entry.get("from") == "client" and m.get("method") == "tools/call":
                p = m.get("params", {})
                n += key(p.get("name", ""), p.get("arguments") or {}) in by_flow
            elif entry.get("from") == "server" and isinstance(m.get("result"), dict):
                for block in m["result"].get("content", []):
                    by_flow.update(flow_calls(block.get("text", "")))
    return n


def episode_summary(directory: Path) -> dict:
    r = json.loads((directory / "result.json").read_text())
    refusals = r.get("refusals", [])
    sim = json.loads((directory / "simulation.json").read_text())
    agent = [a["usage"] for a in r["agent_results"] if a.get("usage")]
    standard = sum(map(credits, agent)) + sum(map(credits, r["customer_usage"]))
    cheap = off_peak(sim["start_time"])
    return {
        "reward": r["reward"],
        "termination": r["termination"],
        "llm_turns": r["llm_turns"],
        "agent_calls": r["tool_calls"],
        "flow_lookups": r.get("flow_lookups", 0),
        "repeats": repeats(directory),
        "refusals": len(refusals),
        "refused": sorted({f["tool"] or "?" for f in refusals}),
        "agent_input_tokens": sum(
            u.get("input_tokens", 0) + u.get("cache_read_input_tokens", 0) + u.get("cache_creation_input_tokens", 0)
            for u in agent
        ),
        "agent_output_tokens": sum(u.get("output_tokens", 0) for u in agent),
        "customer_turns": r["customer_turns"],
        "credits_standard": round(standard, 2),
        "credits_billed": round(standard / 2 if cheap else standard, 2),
        "duration_s": r["duration_s"],
    }


def episode_dir(out: Path, arm: str, task: str, trial: int) -> Path:
    base = out if trial == 0 else out / f"trial-{trial}"
    return base / arm / f"task-{task}"


def summarize(out: Path, tasks: list[str], trials: int = 1) -> dict:
    rows = []
    for trial in range(trials):
        for t in tasks:
            row = {"task_id": t, "trial": trial}
            for arm in ARMS:
                d = episode_dir(out, arm, t, trial)
                row[arm] = episode_summary(d) if (d / "result.json").exists() else None
            rows.append(row)
    a, b = ARMS
    done = [r for r in rows if r[a] and r[b]]
    total = lambda arm, k: sum(r[arm][k] for r in done)
    diffs = [r[b]["llm_turns"] - r[a]["llm_turns"] for r in done]
    summary = {
        "arms": ARMS,
        "pairs": len(done),
        "llm_turns": {arm: total(arm, "llm_turns") for arm in ARMS},
        "agent_input_tokens": {arm: total(arm, "agent_input_tokens") for arm in ARMS},
        "passed": {arm: sum(r[arm]["reward"] >= 1 - 1e-9 for r in done) for arm in ARMS},
        "credits_billed": round(sum(r[x]["credits_billed"] for r in rows for x in ARMS if r[x]), 1),
        "flow_lookups": total(b, "flow_lookups"),
        "repeats": total(b, "repeats"),
        "refusals": total(b, "refusals"),
        "episodes_with_refusals": sum(r[b]["refusals"] > 0 for r in done),
    }
    if done:
        base = summary["llm_turns"][a]
        summary["turns_saved"] = round(1 - summary["llm_turns"][b] / base, 4) if base else None
        tb = summary["agent_input_tokens"][a]
        summary["input_tokens_saved"] = round(1 - summary["agent_input_tokens"][b] / tb, 4) if tb else None
        summary["turn_difference_mean"] = round(statistics.mean(diffs), 2)
        summary["turn_difference_sd"] = round(statistics.stdev(diffs), 2) if len(diffs) > 1 else None
        summary["pairs_fewer_turns"] = sum(d < 0 for d in diffs)
        summary["pairs_more_turns"] = sum(d > 0 for d in diffs)
    report = {"tasks": tasks, "trials": trials, "summary": summary, "pairs": rows}
    (out / "pilot.json").write_text(json.dumps(report, indent=1))
    A, B = a.capitalize(), b.capitalize()
    lines = [
        f"| Task | Trial | {A} turns | {B} turns | Flow lookups | Refusals | {A} input tokens | {B} input tokens | {A} reward | {B} reward |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        x, y = r[a], r[b]
        cell = lambda e, k: "—" if e is None else f"{e[k]:,}" if isinstance(e[k], int) else f"{e[k]}"
        lines.append(
            f"| {r['task_id']} | {r['trial']} | {cell(x, 'llm_turns')} | {cell(y, 'llm_turns')} | {cell(y, 'flow_lookups')} "
            f"| {cell(y, 'refusals')} | {cell(x, 'agent_input_tokens')} | {cell(y, 'agent_input_tokens')} | {cell(x, 'reward')} | {cell(y, 'reward')} |"
        )
    (out / "pilot.md").write_text("\n".join(lines) + "\n\n```json\n" + json.dumps(summary, indent=1) + "\n```\n")
    return summary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--domain", default="retail")
    parser.add_argument("--tasks", type=int, default=10)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--exclude", nargs="*", default=["90"], help="tasks already smoke-tested")
    parser.add_argument("--out", type=Path, default=Path("runs/pilot"))
    parser.add_argument("--tau2", type=Path, default=HERE.parent.parent / "sierra-research" / "tau2-bench")
    parser.add_argument("--oracle-cache", type=Path, required=True)
    parser.add_argument("--flow", type=Path, help="a compiled flow (`stretto compile`), else compiled per episode")
    parser.add_argument("--summarize-only", action="store_true")
    parser.add_argument("--arms", nargs=2, default=ARMS, help="the two arms to pair, baseline first")
    parser.add_argument("--task-ids", nargs="*", help="these tasks instead of a sample")
    parser.add_argument("--trials", type=int, default=1, help="episodes per task and arm")
    parser.add_argument(
        "--only", nargs="*", default=[],
        help="ARM:TASK pairs to run; other episodes must already be recorded (reused)",
    )
    args = parser.parse_args()
    ARMS[:] = args.arms
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    tasks = args.task_ids or sample(args.tau2, args.domain, args.tasks, args.seed, set(args.exclude))
    print("tasks:", " ".join(tasks), "arms:", " ".join(ARMS), "trials:", args.trials, flush=True)
    if not args.summarize_only:
        for trial in range(args.trials):
            for t in tasks:
                for arm in ARMS:
                    episode = episode_dir(out, arm, t, trial)
                    if (episode / "result.json").exists():
                        continue
                    if args.only and f"{arm}:{t}" not in args.only:
                        continue
                    command = [
                        sys.executable, str(HERE / "run_episode.py"),
                        "--domain", args.domain, "--task-id", t, "--out", str(episode.parent.parent),
                        "--arm", arm, "--tau2", str(args.tau2), "--oracle-cache", str(args.oracle_cache),
                    ] + (["--flow", str(args.flow)] if args.flow else [])
                    done = subprocess.run(command, capture_output=True, text=True, check=False)
                    (episode.parent.parent / f"{arm}-task-{t}.log").write_text(done.stdout + done.stderr)
                    result = [l for l in done.stdout.splitlines() if l.startswith("RESULT")]
                    print(result[-1] if result else f"FAILED {arm} task {t} (exit {done.returncode})", flush=True)
    print("SUMMARY", json.dumps(summarize(out, tasks, args.trials)), flush=True)


if __name__ == "__main__":
    main()
