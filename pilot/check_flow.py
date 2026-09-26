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
asks the real questions (one per flow step), and with `replay` it reads them
from the cache only. `--flow-decider habit` replays the habit alone, which
never asks the System-One model (arm C: at a high `--flow-threshold` it goes
on only where training shows no branch).

    python check_flow.py runs/pilot/baseline/task-90 --oracle-cache CACHE
    python check_flow.py --results glm-5_retail.json --oracle-cache CACHE \\
        --flow-oracle jev

`--explore EPSILON` serves the flow exploring (`stretto serve --explore`)
and writes every decision with each option's outcome to `decisions.jsonl`,
for `stretto evaluate`: `used` if the agent makes the lookup later (a
recorded call not yet made), `detour` if it never makes it, and `turn`, one
over the calls of the recorded turn the lookup spares a call of. `--explore
0` explores nothing and still writes them.

By default each episode's tools run in their own `tau2_mcp.py` process over
MCP, as the agent's would, and episodes replay one at a time. Most of that
time is spent starting Python and importing τ²-bench. `--in-process` calls
`tau2_mcp.Episode` directly instead, and `--jobs N` replays N episodes at
once in forked workers that inherit the imports. The rows are the same
either way.
"""

import argparse
import asyncio
import json
import os
import sys
from concurrent.futures import ProcessPoolExecutor
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


# tau2_mcp.py's own defaults for a server started as above.
MAX_CALLS, FLOW_MAX, FLOW_BUDGET = 60, 8, 40


async def replay(task_id: str, messages: list, episode: Path, address: str, args) -> dict:
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "tools-state.json"):
        (episode / stale).unlink(missing_ok=True)
    if getattr(args, "in_process", False):
        import tau2_mcp

        server = tau2_mcp.Episode(
            args.domain, task_id, episode, MAX_CALLS, address, FLOW_MAX, FLOW_BUDGET,
            record_answers=exploring(args),
        )
        server.save_state()

        async def call(name: str, arguments: dict) -> str:
            return server.call(name, arguments)[0]

        return await walk(messages, episode, call, task_id)
    params = StdioServerParameters(
        command=sys.executable,
        args=[
            str(HERE / "tau2_mcp.py"),
            "--domain", args.domain,
            "--task-id", task_id,
            "--episode-dir", str(episode),
            "--flow-address", address,
        ] + (["--record-answers"] if exploring(args) else []),
    )
    with open(episode / "server.stderr", "w") as errlog:
        async with stdio_client(params, errlog=errlog) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()

                async def call(name: str, arguments: dict) -> str:
                    out = await session.call_tool(name, arguments)
                    return out.content[0].text if out.content else ""

                return await walk(messages, episode, call, task_id)


def exploring(args) -> bool:
    return getattr(args, "explore", None) is not None


def label(answer: dict, made: set, recorded: set, turn_of: dict, name: str) -> dict:
    """A flow answer with each option's outcome, for `stretto evaluate`."""
    labels = []
    for o in answer["policy"]["options"]:
        if o.get("arguments") is None:
            labels.append({"used": False, "detour": False, "turn": 0.0})
            continue
        k = key(o["tool"], o["arguments"])
        used = k in recorded and k not in made
        labels.append({"used": used, "detour": k not in recorded, "turn": 1.0 / turn_of[k] if used else 0.0})
    return {**answer, "labels": labels, "episode": name}


async def walk(messages: list, episode: Path, call, task_id: str) -> dict:
    """Replay the recorded calls through `call`, skipping those the flow made."""
    trajectory = episode / "trajectory.jsonl"
    agent = [m for m in messages if m["role"] == "assistant"]
    recorded = {key(c["name"], c["arguments"]) for m in agent for c in m.get("tool_calls") or []}
    # The calls in the first turn of each recorded call: a lookup that
    # spares one of them spares that share of the turn.
    turn_of: dict[tuple[str, str], int] = {}
    for m in agent:
        for c in m.get("tool_calls") or []:
            turn_of.setdefault(key(c["name"], c["arguments"]), len(m["tool_calls"]))
    answers = episode / "flow-answers.jsonl"
    answers.unlink(missing_ok=True)
    seen_answers = 0
    decisions: list[dict] = []
    made: set[tuple[str, str]] = set()
    by_flow: list[tuple[str, str]] = []
    turns = tool_turns = saved = calls = skipped = 0
    for i, m in enumerate(messages):
        if m["role"] == "tool":
            continue  # the server records its own results
        # τ²-bench's scripted greeting is not an LLM turn.
        greeting = i == 0 and not m.get("tool_calls")
        if m["role"] == "assistant" and not greeting:
            turns += 1
        # In telecom the customer calls tools on their own phone; those are
        # the customer's turn, recorded as they happened, not replayed.
        if not m.get("tool_calls") or m["role"] != "assistant":
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
            text = await call(c["name"], c["arguments"])
            made.add(k)
            # The flow's decisions after this call, each with what was made
            # by then: the call, then each lookup before it.
            if answers.exists():
                new = answers.read_text().splitlines()[seen_answers:]
                seen_answers += len(new)
                chain = set(made)
                for line in new:
                    answer = json.loads(line)["answer"]
                    if "policy" in answer:
                        decisions.append(label(answer, chain, recorded, turn_of, episode.name))
                    if answer.get("action") == "lookup":
                        chain.add(key(answer["tool"], answer.get("arguments") or {}))
            for f in flow_calls(text):
                made.add(f)
                by_flow.append(f)
        saved += left == 0
    state = json.loads((episode / "tools-state.json").read_text())
    return {
        "decisions": decisions,
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


def replay_one(job: tuple) -> dict:
    """One episode's row (a worker's job with `--jobs`)."""
    name, task_id, messages, out, address, args = job
    return {"episode": name, **asyncio.run(replay(task_id, messages, out / name, address, args))}


def keep(row: dict, out: Path) -> dict:
    """Print a replayed episode's row, and write its labelled decisions."""
    decisions = row.pop("decisions", [])
    if decisions:
        with open(out / "decisions.jsonl", "a") as f:
            for d in decisions:
                f.write(json.dumps(d) + "\n")
    print(json.dumps(row), flush=True)
    return row


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
    for d, name in zip(args.recorded, episode_names(args.recorded)):
        messages = [json.loads(l) for l in (d / "trajectory.jsonl").read_text().splitlines()]
        task_id = json.loads((d / "result.json").read_text())["task_id"]
        out.append((name, task_id, messages))
    return out


def episode_names(dirs: list[Path]) -> list[str]:
    """A distinct name for each recorded episode directory, to replay it under.

    Each keeps its own folder name unless two share one, as a task's trials do
    when each trial has its own folder. Then every episode is named by its path
    below the folders' common parent, joined with dashes, since episodes that
    shared a folder would share one trajectory, which the flow reads.

    >>> episode_names([Path("runs/a/task-2"), Path("runs/a/task-3")])
    ['task-2', 'task-3']
    >>> episode_names([Path("runs/a/task-2"), Path("runs/a/trial-1/task-2")])
    ['task-2', 'trial-1-task-2']
    """
    names = [d.name for d in dirs]
    if len(set(names)) == len(names):
        return names
    resolved = [d.resolve() for d in dirs]
    if len(set(resolved)) < len(resolved):
        raise SystemExit("check_flow.py: an episode directory is given twice")
    common = Path(os.path.commonpath(resolved))
    return ["-".join(d.relative_to(common).parts) for d in resolved]


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
    parser.add_argument("--flow-oracle", default="mock", choices=["jev", "replay", "mock"])
    parser.add_argument("--flow-threshold", type=float, default=0.3)
    parser.add_argument(
        "--flow-decider", default="arbiter", choices=["arbiter", "habit"],
        help="`habit`: the habit alone, never asking the System-One model (arm C)",
    )
    parser.add_argument("--flow", type=Path, help="a compiled flow (`stretto compile`), else compiled here")
    parser.add_argument(
        "--explore", type=float, metavar="EPSILON",
        help="serve the flow exploring, and write each decision with its options' outcomes to decisions.jsonl",
    )
    parser.add_argument("--explore-seed", type=int, default=0, help="seed for the exploration draws")
    parser.add_argument("--in-process", action="store_true", help="call tau2_mcp.Episode directly instead of over MCP")
    parser.add_argument("--jobs", type=int, default=1, help="episodes to replay at once")
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
            flow_decider=args.flow_decider,
            flow_max_questions=50 * len(episodes),
            flow=args.flow,
            explore=args.explore,
            explore_seed=args.explore_seed,
        ),
        out,
    )
    (out / "decisions.jsonl").unlink(missing_ok=True)
    rows = []
    jobs = [(name, task_id, messages, out, address, args) for name, task_id, messages in episodes]
    try:
        if args.jobs > 1:
            if args.in_process:
                import tau2_mcp  # noqa: F401  (imported once, inherited by the forked workers)
            with ProcessPoolExecutor(args.jobs) as pool:
                for row in pool.map(replay_one, jobs):
                    rows.append(keep(row, out))
        else:
            for job in jobs:
                rows.append(keep(replay_one(job), out))
    finally:
        serve.terminate()
        serve.wait(timeout=30)
    total = {k: sum(r[k] for r in rows) for k in rows[0] if k not in ("episode", "task_id")}
    total["episodes"] = len(rows)
    total["turns_saved_share"] = round(total["turns_saved"] / max(total["turns"], 1), 4)
    total["episodes_with_detour"] = sum(1 for r in rows if r["detours"])
    flow = {
        "decider": args.flow_decider,
        "threshold": args.flow_threshold,
        "oracle": args.flow_oracle,
        "explore": args.explore,
    }
    (out / "check.json").write_text(
        json.dumps({"flow": flow, "total": total, "episodes": rows}, indent=1)
    )
    print("CHECK", json.dumps(total))


if __name__ == "__main__":
    main()
