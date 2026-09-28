"""How many LLM turns `stretto_commit` could save (docs/results/commit-bound-2026-09-25.md; stretto #7),
and, on WorkBench's write-heavy tasks, what bundling writes could save at all
(docs/results/write-bound-2026-09-28.md; stretto #37).

    python3 scripts/commit_bound.py TAU2_CHECKOUT [EPISODE_DIR ...]
    python3 scripts/commit_bound.py --workbench CONVERTED_DIR

`stretto_commit` makes several writes in one call. Its saving is bounded by
the turns an agent spends on consecutive writes after the customer last
spoke: a run of k assistant turns that each make only writes (the agent's
own, not the customer's) could be one call, saving k − 1 turns. Turns that
already make several writes at once count once. For τ²-bench's published
retail and airline baselines, and for live episodes (directories holding
`task-*/simulation.json`, such as an unpacked pilot archive's `baseline/`).
Those need τ²-bench installed, for its tool types. With `--workbench`, the runs
are WorkBench's, as `scripts/workbench_to_tau2.py` converts them, and a write
is a call that changes the state (`WRITES` there).
"""

import glob
import json
import sys
from pathlib import Path



def kinds(domain: str) -> dict:
    from tau2.registry import registry

    env = registry.get_env_constructor(domain)()
    return {t.name: str(env.tools.tool_type(t.name)).split(".")[-1] for t in env.get_tools()}


def bound(simulations, is_write) -> tuple[int, int, int, int]:
    """(LLM turns, turns that make only writes, runs of two or more of those, turns a commit call saves),
    where `is_write` says whether a tool, by name, writes."""
    turns = writes = runs = saved = 0
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
                if calls and all(is_write(c["name"]) for c in calls):
                    run += 1
                    writes += 1
                else:
                    close()
            elif m["role"] == "user":
                close()
        close()
    return turns, writes, runs, saved


def main() -> None:
    if sys.argv[1] == "--workbench":
        return workbench(Path(sys.argv[2]))
    tau2 = Path(sys.argv[1])
    rows = []
    for domain in ("retail", "airline"):
        k = kinds(domain)
        writes = lambda name: k.get(name) == "WRITE"  # noqa: E731
        results = tau2 / "data/tau2/results/final"
        for path in sorted(glob.glob(str(results / f"*_{domain}_default_*.json")) + glob.glob(str(results / f"*_{domain}_base_*.json"))):
            agent = Path(path).name.split(f"_{domain}_")[0]
            rows.append((domain, agent, *bound(json.load(open(path))["simulations"], writes)))
        for d in sys.argv[2:]:
            sims = [json.load(open(p)) for p in sorted(glob.glob(f"{d}/task-*/simulation.json"))]
            if sims and sims[0].get("task_id") is not None and _domain_of(d) == domain:
                rows.append((domain, f"live: {d}", *bound(sims, writes)))
    print("| Domain | Agent | LLM turns | Runs of 2+ write turns | Turns a commit call saves |")
    print("|---|---|---|---|---|")
    for domain, agent, turns, _, runs, saved in rows:
        print(f"| {domain} | {agent} | {turns:,} | {runs} | {saved} ({saved / turns:.1%}) |")


def workbench(converted: Path) -> None:
    """The bound on every WorkBench run in `converted` (DOMAIN/MODEL.json), by domain and by model."""
    from workbench_to_tau2 import DOMAINS, WRITES

    by_domain: dict = {}
    by_model: dict = {}
    for domain in DOMAINS:
        for path in sorted(glob.glob(str(converted / domain / "*.json"))):
            counts = bound(json.load(open(path))["simulations"], WRITES.__contains__)
            for table, key in ((by_domain, domain), (by_model, Path(path).stem)):
                table[key] = [a + b for a, b in zip(table.get(key, [0, 0, 0, 0]), counts)]
    by_domain["all"] = [sum(c) for c in zip(*by_domain.values())]
    for title, table in (("Domain", by_domain), ("Model", sorted(by_model.items(), key=lambda kv: -kv[1][3] / kv[1][0]))):
        print(f"| {title} | LLM turns | Turns that only write | Runs of 2+ | Turns saved |")
        print("|---|---|---|---|---|")
        for name, (turns, writes, runs, saved) in table.items() if isinstance(table, dict) else table:
            print(f"| {name} | {turns:,} | {writes:,} ({writes / turns:.1%}) | {runs} | {saved} ({saved / turns:.1%}) |")
        print()


def _domain_of(directory: str) -> str:
    return "airline" if "airline" in directory else "retail"


if __name__ == "__main__":
    main()
