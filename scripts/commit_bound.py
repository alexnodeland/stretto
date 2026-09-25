"""How many LLM turns `stretto_commit` could save (docs/results/commit-bound-2026-09-25.md; stretto #7).

    python3 scripts/commit_bound.py TAU2_CHECKOUT [EPISODE_DIR ...]

`stretto_commit` makes several writes in one call. Its saving is bounded by
the turns an agent spends on consecutive writes after the customer last
spoke: a run of k assistant turns that each make only writes (the agent's
own, not the customer's) could be one call, saving k − 1 turns. Turns that
already make several writes at once count once. For τ²-bench's published
retail and airline baselines, and for live episodes (directories holding
`task-*/simulation.json`, such as an unpacked pilot archive's `baseline/`).

Needs τ²-bench installed, for its tool types.
"""

import glob
import json
import sys
from pathlib import Path

from tau2.registry import registry


def kinds(domain: str) -> dict:
    env = registry.get_env_constructor(domain)()
    return {t.name: str(env.tools.tool_type(t.name)).split(".")[-1] for t in env.get_tools()}


def bound(simulations, tool_kinds: dict) -> tuple[int, int, int]:
    """(LLM turns, runs of two or more write turns, turns a commit call saves)."""
    turns = runs = saved = 0
    for s in simulations:
        run = 0

        def close():
            nonlocal runs, saved, run
            if run >= 2:
                runs += 1
                saved += run - 1
            run = 0

        for m in s["messages"]:
            if m["role"] == "assistant":
                calls = [c for c in (m.get("tool_calls") or []) if (c.get("requestor") or "assistant") == "assistant"]
                if not calls and not m.get("content"):
                    continue
                turns += 1
                if calls and all(tool_kinds.get(c["name"]) == "WRITE" for c in calls):
                    run += 1
                else:
                    close()
            elif m["role"] == "user":
                close()
        close()
    return turns, runs, saved


def main() -> None:
    tau2 = Path(sys.argv[1])
    rows = []
    for domain in ("retail", "airline"):
        k = kinds(domain)
        results = tau2 / "data/tau2/results/final"
        for path in sorted(glob.glob(str(results / f"*_{domain}_default_*.json")) + glob.glob(str(results / f"*_{domain}_base_*.json"))):
            agent = Path(path).name.split(f"_{domain}_")[0]
            rows.append((domain, agent, *bound(json.load(open(path))["simulations"], k)))
        for d in sys.argv[2:]:
            sims = [json.load(open(p)) for p in sorted(glob.glob(f"{d}/task-*/simulation.json"))]
            if sims and sims[0].get("task_id") is not None and _domain_of(d) == domain:
                rows.append((domain, f"live: {d}", *bound(sims, k)))
    print("| Domain | Agent | LLM turns | Runs of 2+ write turns | Turns a commit call saves |")
    print("|---|---|---|---|---|")
    for domain, agent, turns, runs, saved in rows:
        print(f"| {domain} | {agent} | {turns:,} | {runs} | {saved} ({saved / turns:.1%}) |")


def _domain_of(directory: str) -> str:
    return "airline" if "airline" in directory else "retail"


if __name__ == "__main__":
    main()
