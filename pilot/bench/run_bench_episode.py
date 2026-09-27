"""Run one live episode of another benchmark and score it with the benchmark's own check.

The agent runs in Claude Code with no built-in tools, on GLM through Z.ai's
coding endpoint (`../glm-claude.sh`) or on a Claude model (`../claude-agent.sh`),
as the τ²-bench pilots ran. Its only tools are the task's, served by
`bench_mcp.py` behind `stretto-proxy`, which records the session and, in a
flow arm, serves a flow: after each of the agent's calls, the flow's lookups
ride in the same result. The flow is the published one, learned from older
agents' runs of the benchmark's training tasks (`stretto learn --habit-only`),
and the episode is a test task, so the flow never saw it.

- `agentdojo`: the user task's prompt, once, with AgentDojo's default system
  message; the episode ends when the agent answers. Scored as AgentDojo scores
  a run: `utility_from_traces` on the agent's own calls where the task defines
  it, else `utility` on its answer and the environment before and after.
- `bfcl`: a BFCL multi-turn task's user turns, one after another, each sent
  when the agent has answered the last. Scored by BFCL's own checker
  (`multi_turn_checker`), which runs the agent's calls turn by turn on fresh
  instances and compares their state and results with the ground truth's.

`--agent scripted` runs no model: it connects to the same proxy and server as
an MCP client, makes the task's ground-truth calls (AgentDojo's
`ground_truth`, BFCL's possible answer) and answers with the task's
ground-truth output, so the server, the proxy, the flow and the scoring can be
checked for free.

    python run_bench_episode.py agentdojo --suite travel --task user_task_3 --arm reach \
        --flow agentdojo-travel.flow.json --model glm-5.3 --out runs/agentdojo
"""

import argparse
import asyncio
import json
import os
import pickle
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
PILOT = HERE.parent
sys.path.insert(0, str(PILOT.parent / "scripts"))
BIN = Path(os.environ.get("STRETTO_BIN", PILOT.parent / "target" / "release"))
WRAPPERS = {"glm": PILOT / "glm-claude.sh", "claude": PILOT / "claude-agent.sh"}
# Z.ai's GLM Coding Plan credits per token, as run_pilot.py counts them (halved off-peak).
RATES = {"input_tokens": 6.9, "cache_read_input_tokens": 1.7, "cache_creation_input_tokens": 6.9, "output_tokens": 24.0}
SERVER = "bench"


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


class AgentDojoTask:
    def __init__(self, args):
        from agentdojo.agent_pipeline.agent_pipeline import load_system_message
        from agentdojo.task_suite.load_suites import get_suite

        self.args = args
        self.suite = get_suite(args.version, args.suite)
        self.task = self.suite.get_user_task_by_id(args.task)
        self.system = load_system_message(None)
        self.turns = [self.task.PROMPT]
        self.domain = args.suite

    def server_args(self) -> list[str]:
        a = self.args
        return ["agentdojo", "--suite", a.suite, "--task", a.task, "--version", a.version]

    def pre(self):
        return self.task.init_environment(self.suite.load_and_inject_default_environment({}))

    def script(self) -> tuple[list[list[tuple[str, dict]]], list[str]]:
        """The ground truth: each turn's calls, and each turn's answer."""
        calls = [(c.function, dict(c.args)) for c in self.task.ground_truth(self.pre())]
        return [calls], [getattr(self.task, "GROUND_TRUTH_OUTPUT", "") or ""]

    def score(self, episode: Path, answers: list[str], calls: list[list[tuple[str, dict]]]) -> dict:
        from agentdojo.functions_runtime import FunctionCall

        pre = self.pre()
        post = pickle.loads((episode / "state.pkl").read_bytes())
        output = answers[-1] if answers else ""
        trace = [FunctionCall(function=n, args=a, id=f"call_{i}") for i, (n, a) in enumerate(c for t in calls for c in t)]
        by_traces = self.task.utility_from_traces(output, pre, post, trace)
        utility = by_traces if by_traces is not None else self.task.utility(output, pre, post)
        return {"reward": 1.0 if utility else 0.0, "scored_by": "traces" if by_traces is not None else "utility"}


class BFCLTask:
    SYSTEM = (
        "You are an assistant that completes the user's requests by calling the available tools. "
        "Make the calls each request needs without asking the user to confirm, and when a request "
        "is done, reply in one or two sentences."
    )

    def __init__(self, args):
        from bench_mcp import bfcl_entry

        self.args = args
        self.entry, self.answer = bfcl_entry(args.task, args.bfcl_dir)
        self.turns = ["\n".join(m["content"] for m in turn if m.get("role") == "user") for turn in self.entry["question"]]
        self.system = self.SYSTEM
        self.domain = "bfcl"

    def server_args(self) -> list[str]:
        return ["bfcl", "--task", self.args.task, "--bfcl-dir", str(self.args.bfcl_dir)]

    def script(self) -> tuple[list[list[tuple[str, dict]]], list[str]]:
        import inspect

        import bfcl_to_tau2
        from bench_mcp import BFCL

        methods = BFCL(self.args.task, self.args.bfcl_dir).methods
        turns = []
        for calls in self.answer["ground_truth"]:
            turn = []
            for text in calls:
                name, positional, keywords = bfcl_to_tau2.parse_call(text)
                params = list(inspect.signature(methods[name]).parameters)
                turn.append((name, {**dict(zip(params, positional)), **keywords}))
            turns.append(turn)
        return turns, ["Done."] * len(turns)

    def score(self, episode: Path, answers: list[str], calls: list[list[tuple[str, dict]]]) -> dict:
        import uuid

        sys.path.insert(0, str(self.args.bfcl_dir))
        from bfcl_eval.eval_checker.multi_turn_eval.multi_turn_checker import multi_turn_checker

        from bench_mcp import BFCL

        decoded = [[[BFCL.call_text(n, a)] for n, a in turn] for turn in calls]
        decoded += [[] for _ in range(len(self.answer["ground_truth"]) - len(decoded))]
        check = multi_turn_checker(decoded, self.answer["ground_truth"], self.entry, "multi_turn_base",
                                   model_name=f"live_{uuid.uuid4().hex[:8]}")
        return {"reward": 1.0 if check.get("valid") else 0.0, "scored_by": "bfcl",
                "error_type": check.get("error_type")}


BENCHES = {"agentdojo": AgentDojoTask, "bfcl": BFCLTask}


def mcp_config(args, bench, episode: Path) -> dict:
    proxy = [
        "--record", str(episode / "log"),
        "--domain", bench.domain,
        "--agent-model", args.model,
    ]
    if args.arm != "baseline":
        proxy += [
            "--flow", str(args.flow.resolve()),
            "--flow-decider", args.arm,
            "--flow-threshold", str(args.flow_threshold),
            "--flow-log", str(episode / "flow.jsonl"),
            "--oracle", "replay",
            "--oracle-cache", str(episode / "oracle-cache"),
        ]
    server = [sys.executable, str(HERE / "bench_mcp.py")] + bench.server_args() + [
        "--episode-dir", str(episode), "--max-calls", str(args.max_calls)]
    return {"mcpServers": {SERVER: {"command": str(BIN / "stretto-proxy"), "args": proxy + ["--"] + server}}}


def run_agent(args, bench, episode: Path) -> tuple[list[str], list[list[tuple[str, dict]]], str]:
    """The agent's episode in Claude Code: its answer to each user turn, its own calls per turn, and how it ended."""
    agent = subprocess.Popen(
        [
            str(WRAPPERS[args.agent_cli]), "-p", "--bare", "--tools", "",
            "--strict-mcp-config", "--mcp-config", str(episode / "mcp.json"),
            "--allowedTools", f"mcp__{SERVER}", "--model", args.model,
            "--system-prompt", bench.system,
            "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
        ],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(episode / "agent.stderr", "w"),
        text=True, bufsize=1,
    )
    events = open(episode / "events.jsonl", "w")
    deadline = time.time() + args.timeout
    answers, calls, ending = [], [], "agent_stop"
    prefix = f"mcp__{SERVER}__"
    try:
        for user in bench.turns:
            agent.stdin.write(json.dumps({"type": "user", "message": {"role": "user", "content": user}}) + "\n")
            agent.stdin.flush()
            turn_calls, answer = [], None
            for line in agent.stdout:
                events.write(line)
                event = json.loads(line)
                if event.get("type") == "system" and event.get("subtype") == "init":
                    failed = [m["name"] for m in event.get("mcp_servers", []) if m.get("status") == "failed"]
                    if failed:
                        raise RuntimeError(f"MCP server {failed} failed to start: see {episode / 'log'}")
                if event.get("type") == "assistant":
                    for b in event.get("message", {}).get("content", []):
                        if b.get("type") == "tool_use" and b.get("name", "").startswith(prefix):
                            turn_calls.append((b["name"][len(prefix):], b.get("input") or {}))
                if event.get("type") == "result":
                    answer = event.get("result") or ""
                    break
                if time.time() > deadline:
                    ending = "timeout"
                    break
            calls.append(turn_calls)
            if answer is None:
                ending = ending if ending == "timeout" else "agent_error"
                break
            answers.append(answer)
            if json.loads((episode / "tools-state.json").read_text()).get("over_budget"):
                ending = "max_calls"
                break
    finally:
        agent.stdin.close()
        try:
            agent.wait(timeout=60)
        except subprocess.TimeoutExpired:
            agent.kill()
        events.close()
    return answers, calls, ending


def run_scripted(args, bench, episode: Path) -> tuple[list[str], list[list[tuple[str, dict]]], str]:
    """No model: the ground truth's calls through the same proxy and server, as an MCP client."""
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    config = json.loads((episode / "mcp.json").read_text())["mcpServers"][SERVER]
    script, outputs = bench.script()

    async def go():
        params = StdioServerParameters(command=config["command"], args=config["args"], env=dict(os.environ))
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                results = []
                for turn in script:
                    for name, arguments in turn:
                        r = await session.call_tool(name, arguments)
                        results.append({"tool": name, "error": r.isError,
                                        "text": "".join(c.text for c in r.content if hasattr(c, "text"))})
                (episode / "scripted.jsonl").write_text("".join(json.dumps(r) + "\n" for r in results))

    asyncio.run(go())
    return outputs, script, "agent_stop"


def summary(episode: Path, args) -> dict:
    """LLM turns, calls, tokens and credits from the agent's events; lookups from the flow's log."""
    responses: dict[str, int] = {}
    usage: dict[str, int] = {}
    for line in (episode / "events.jsonl").read_text().splitlines() if (episode / "events.jsonl").exists() else []:
        event = json.loads(line)
        if event.get("type") == "assistant":
            message = event.get("message", {})
            n = sum(1 for b in message.get("content", []) if b.get("type") == "tool_use")
            key = message.get("id") or str(len(responses))
            responses[key] = responses.get(key, 0) + n
        elif event.get("type") == "result":
            for k, v in (event.get("usage") or {}).items():
                if isinstance(v, int):
                    usage[k] = usage.get(k, 0) + v
    flow = [json.loads(l) for l in (episode / "flow.jsonl").read_text().splitlines()] if (episode / "flow.jsonl").exists() else []
    lookups = [d for d in flow if d.get("lookup") or d.get("action") == "lookup"]
    credits = sum(usage.get(k, 0) * r for k, r in RATES.items()) / 10_000 / 2  # off-peak until 2026-10-07
    return {
        "llm_turns": len(responses),
        "tool_turns": sum(1 for n in responses.values() if n > 0),
        "parallel_turns": sum(1 for n in responses.values() if n > 1),
        "usage": usage,
        "credits_billed": round(credits, 2) if args.agent_cli == "glm" else 0.0,
        "flow_decisions": len(flow),
        "flow_lookups": len(lookups),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("bench", choices=sorted(BENCHES))
    parser.add_argument("--suite", help="AgentDojo's suite")
    parser.add_argument("--task", required=True)
    parser.add_argument("--version", default="v1.2.1", help="AgentDojo's benchmark version")
    parser.add_argument("--bfcl-dir", type=Path, default=Path(os.environ.get("BFCL_DIR", "bfcl")),
                        help="the folder holding the bfcl_eval package (BFCL_DIR)")
    parser.add_argument("--arm", default="baseline", choices=["baseline", "reach", "habit"])
    parser.add_argument("--flow", type=Path, help="the flow a reach or habit arm serves")
    parser.add_argument("--flow-threshold", type=float, default=0.3)
    parser.add_argument("--agent", default="claude-code", choices=["claude-code", "scripted"])
    parser.add_argument("--agent-cli", default="glm", choices=sorted(WRAPPERS))
    parser.add_argument("--model", default="glm-5.3")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--label", help="the arm's folder under --out (default: the arm)")
    parser.add_argument("--max-calls", type=int, help="calls the server runs, the flow's included (default: 40 for "
                        "AgentDojo; 150 for BFCL, whose own harness allows 20 steps in each of up to five turns)")
    parser.add_argument("--timeout", type=float, help="seconds for the whole episode (default: 900; 1500 for BFCL)")
    args = parser.parse_args()
    if args.arm != "baseline" and not args.flow:
        parser.error(f"--arm {args.arm} needs --flow")
    args.max_calls = args.max_calls or (150 if args.bench == "bfcl" else 40)
    args.timeout = args.timeout or (1500 if args.bench == "bfcl" else 900)
    if args.agent == "claude-code" and (args.agent_cli == "claude") == args.model.startswith("glm"):
        parser.error(f"--model {args.model} does not run on --agent-cli {args.agent_cli}")
    bench = BENCHES[args.bench](args)
    episode = (args.out / (args.label or args.arm) / f"{bench.domain}-{args.task}").resolve()
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "events.jsonl", "flow.jsonl", "state.pkl", "tools-state.json", "scripted.jsonl"):
        (episode / stale).unlink(missing_ok=True)
    (episode / "mcp.json").write_text(json.dumps(mcp_config(args, bench, episode), indent=1))
    started, t0 = now(), time.time()
    run = run_scripted if args.agent == "scripted" else run_agent
    answers, calls, ending = run(args, bench, episode)
    score = bench.score(episode, answers, calls)
    tools = json.loads((episode / "tools-state.json").read_text())
    result = {
        "bench": args.bench, "domain": bench.domain, "task": args.task, "arm": args.arm,
        "label": args.label or args.arm, "agent": args.agent, "agent_cli": args.agent_cli, "model": args.model,
        "flow": str(args.flow) if args.flow else None, "flow_threshold": args.flow_threshold,
        **score, "ending": ending, "started": started, "duration_s": round(time.time() - t0, 1),
        "agent_calls": sum(len(t) for t in calls), "server_calls": tools.get("tool_calls"),
        "answers": answers,
        **summary(episode, args),
    }
    (episode / "result.json").write_text(json.dumps(result, indent=1))
    print("RESULT", json.dumps({k: result[k] for k in (
        "domain", "task", "arm", "model", "reward", "ending", "llm_turns", "agent_calls", "server_calls",
        "flow_lookups", "credits_billed", "duration_s")}))


if __name__ == "__main__":
    main()
