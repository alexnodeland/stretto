"""Replay recorded episodes' tool calls with a flow behind the tools.

No LLM runs. Each recorded conversation is written to a fresh episode as it
happened, and each of the agent's recorded calls goes through `tau2_mcp.py`
(over MCP, as the agent's would) with `stretto flow-serve` behind it. A
recorded call the flow has already made (same tool and arguments) is
skipped, as an agent that reads the flow's results would skip it. A recorded
turn whose calls were all skipped is an LLM turn the flow saves, had the
agent otherwise done the same (the offline projection's assumption, with
the live flow's own argument bindings); a flow lookup the agent never made
is a detour.

With `--flow-oracle mock` this checks the plumbing for free; with `jev` it
asks the real questions (one per flow step).

    python check_flow.py runs/pilot/baseline/task-90 --oracle-cache CACHE
    python check_flow.py --results glm-5_retail.json --oracle-cache CACHE \\
        --flow-oracle jev
"""

import argparse
import asyncio
import json
import sys
from pathlib import Path
from types import SimpleNamespace

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

import run_episode

HERE = Path(__file__).resolve().parent
MARK = "--- Also looked up automatically"


def key(tool: str, arguments: dict) -> tuple[str, str]:
    return tool, json.dumps(arguments, sort_keys=True)


def flow_calls(text: str) -> list[tuple[str, str]]:
    """The lookups the flow appended to a tool response."""
    if MARK not in text:
        return []
    out = []
    for line in text.split(MARK, 1)[1].splitlines():
        head = line.split(" {", 1)
        if len(head) == 2 and line.endswith(":"):
            body = "{" + head[1].rsplit(":", 1)[0].removesuffix(" (error)")
            out.append(key(head[0], json.loads(body)))
    return out


async def replay(task_id: str, messages: list, episode: Path, address: str, args) -> dict:
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "tools-state.json"):
        (episode / stale).unlink(missing_ok=True)
    trajectory = episode / "trajectory.jsonl"
    server = StdioServerParameters(
        command=sys.executable,
        args=[
            str(HERE / "tau2_mcp.py"),
            "--domain", args.domain,
            "--task-id", task_id,
            "--episode-dir", str(episode),
            "--flow-address", address,
        ],
    )
    recorded = {key(c["name"], c["arguments"]) for m in messages for c in m.get("tool_calls") or []}
    made: set[tuple[str, str]] = set()
    by_flow: list[tuple[str, str]] = []
    turns = tool_turns = saved = calls = skipped = 0
    with open(episode / "server.stderr", "w") as errlog:
        async with stdio_client(server, errlog=errlog) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                for i, m in enumerate(messages):
                    if m["role"] == "tool":
                        continue  # the server records its own results
                    # τ²-bench's scripted greeting is not an LLM turn.
                    greeting = i == 0 and not m.get("tool_calls")
                    if m["role"] == "assistant" and not greeting:
                        turns += 1
                    if not m.get("tool_calls"):
                        with open(trajectory, "a") as f:
                            f.write(json.dumps(m) + "\n")
                        continue
                    tool_turns += 1
                    left = 0
                    for c in m["tool_calls"]:
                        calls += 1
                        k = key(c["name"], c["arguments"])
                        if k in made:
                            skipped += 1
                            continue
                        left += 1
                        out = await session.call_tool(c["name"], c["arguments"])
                        made.add(k)
                        for f in flow_calls(out.content[0].text if out.content else ""):
                            made.add(f)
                            by_flow.append(f)
                    saved += left == 0
    state = json.loads((episode / "tools-state.json").read_text())
    return {
        "task_id": task_id,
        "turns": turns,
        "tool_turns": tool_turns,
        "turns_saved": saved,
        "calls": calls,
        "calls_skipped": skipped,
        "flow_lookups": state.get("flow_lookups"),
        "flow_queries": state.get("flow_queries"),
        "detours": sum(1 for f in by_flow if f not in recorded),
    }


def recorded_episodes(args) -> list[tuple[str, str, list]]:
    """(name, task id, messages) of each episode to replay."""
    if args.results:
        sims = json.loads(args.results.read_text())["simulations"]
        split = args.tau2 / f"data/tau2/domains/{args.domain}/split_tasks.json"
        test = set(json.loads(split.read_text())["test"])
        wanted = set(args.task_ids) if args.task_ids else test
        return [
            (f"task-{s['task_id']}-{s.get('trial', 0)}", str(s["task_id"]), s["messages"])
            for s in sims
            if str(s["task_id"]) in wanted and s.get("trial", 0) in args.trials
        ]
    out = []
    for d in args.recorded:
        messages = [json.loads(l) for l in (d / "trajectory.jsonl").read_text().splitlines()]
        task_id = json.loads((d / "result.json").read_text())["task_id"]
        out.append((d.name, task_id, messages))
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("recorded", type=Path, nargs="*", help="recorded episode directories")
    parser.add_argument("--results", type=Path, help="a τ²-bench results file instead")
    parser.add_argument("--task-ids", nargs="*", help="with --results (default: the test split)")
    parser.add_argument("--trials", type=int, nargs="*", default=[0], help="with --results")
    parser.add_argument("--domain", default="retail")
    parser.add_argument("--out", type=Path, default=Path("runs/check"))
    parser.add_argument("--tau2", type=Path, default=run_episode.TAU2)
    parser.add_argument("--oracle-cache", type=Path, required=True)
    parser.add_argument("--flow-oracle", default="mock", choices=["jev", "mock"])
    parser.add_argument("--flow-threshold", type=float, default=0.3)
    args = parser.parse_args()
    episodes = recorded_episodes(args)
    if not episodes:
        parser.error("nothing to replay")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / "flow.jsonl").unlink(missing_ok=True)
    serve, address = run_episode.start_flow(
        SimpleNamespace(
            tau2=args.tau2,
            domain=args.domain,
            flow_oracle=args.flow_oracle,
            oracle_cache=args.oracle_cache,
            flow_threshold=args.flow_threshold,
            flow_max_questions=50 * len(episodes),
        ),
        out,
    )
    rows = []
    try:
        for name, task_id, messages in episodes:
            row = asyncio.run(replay(task_id, messages, out / name, address, args))
            rows.append({"episode": name, **row})
            print(json.dumps(rows[-1]), flush=True)
    finally:
        serve.terminate()
        serve.wait(timeout=30)
    total = {k: sum(r[k] for r in rows) for k in rows[0] if k not in ("episode", "task_id")}
    total["episodes"] = len(rows)
    total["turns_saved_share"] = round(total["turns_saved"] / max(total["turns"], 1), 4)
    total["episodes_with_detour"] = sum(1 for r in rows if r["detours"])
    (out / "check.json").write_text(json.dumps({"total": total, "episodes": rows}, indent=1))
    print("CHECK", json.dumps(total))


if __name__ == "__main__":
    main()
