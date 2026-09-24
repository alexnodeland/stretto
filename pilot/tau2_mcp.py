"""One τ²-bench task's tools, served over MCP (stdio).

The server holds the task's environment for a whole episode and executes
the agent's tool calls against it. Each call and its result are appended to
the episode's `trajectory.jsonl` in τ²-bench's own message format, where
`run_episode.py` also appends the conversation, so the official evaluator
can score the episode afterwards. A call budget guards against runaway
episodes.

With `--flow-address`, a read-only flow runs after each of the agent's calls
(the flows arm): the server asks `stretto flow-serve` what to do next, makes
the lookup it names, records it like any other call, and asks again, until
the flow hands back. The agent gets its own result followed by the flow's
lookups and their results, in the same tool response, so the lookups cost
it no turns.

Usage (normally started by the agent, behind `stretto-proxy`):

    python tau2_mcp.py --domain retail --task-id 0 --episode-dir DIR \
        [--flow-address HOST:PORT]
"""

import argparse
import asyncio
import json
import socket
import sys
import uuid
from pathlib import Path

import mcp.types as types
from mcp.server import Server
from mcp.server.stdio import stdio_server

from tau2.data_model.message import AssistantMessage, ToolCall
from tau2.registry import registry


def load_task(domain: str, task_id: str):
    for t in registry.get_tasks_loader(domain)():
        if str(t.id) == str(task_id):
            return t
    raise SystemExit(f"no task {task_id} in {domain}")


class Episode:
    """The environment and the record of one episode."""

    def __init__(
        self,
        domain: str,
        task_id: str,
        directory: Path,
        max_calls: int,
        flow_address: str | None = None,
        flow_max: int = 8,
        flow_budget: int = 40,
    ):
        self.dir = directory
        self.task_id = str(task_id)
        self.task = load_task(domain, task_id)
        self.env = registry.get_env_constructor(domain)()
        if self.task.initial_state is not None:
            init = self.task.initial_state
            self.env.set_state(
                initialization_data=init.initialization_data,
                initialization_actions=init.initialization_actions,
                message_history=init.message_history or [],
            )
        self.tools = {t.name: t for t in self.env.get_tools()}
        self.calls = 0
        self.max_calls = max_calls
        self.flow_address = flow_address
        self.flow_max = flow_max
        self.flow_budget = flow_budget
        self.flow_queries = 0
        self.flow_lookups = 0
        # One call (and the flow after it) at a time.
        self.lock = asyncio.Lock()

    def append(self, *messages) -> None:
        with open(self.dir / "trajectory.jsonl", "a") as f:
            for m in messages:
                f.write(m.model_dump_json() + "\n")

    def save_state(self, over_budget: bool = False) -> None:
        (self.dir / "tools-state.json").write_text(
            json.dumps(
                {
                    "tool_calls": self.calls,
                    "over_budget": over_budget,
                    "flow_queries": self.flow_queries,
                    "flow_lookups": self.flow_lookups,
                }
            )
        )

    def list_tools(self) -> list[types.Tool]:
        out = []
        for name, tool in self.tools.items():
            fn = tool.openai_schema["function"]
            out.append(
                types.Tool(
                    name=name,
                    description=fn.get("description", ""),
                    inputSchema=fn.get("parameters", {"type": "object"}),
                )
            )
        return out

    def execute(self, name: str, arguments: dict) -> tuple[str, bool]:
        """Run one call against the environment and record it."""
        call = ToolCall(
            id=f"call_{uuid.uuid4().hex[:12]}",
            name=name,
            arguments=arguments,
            requestor="assistant",
        )
        result = self.env.get_response(call)
        self.append(AssistantMessage(role="assistant", content=None, tool_calls=[call]), result)
        return result.content or "", bool(result.error)

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        """The agent's call, then (in the flows arm) the flow's lookups."""
        self.calls += 1
        if self.calls > self.max_calls:
            self.save_state(over_budget=True)
            return "Tool call limit reached for this conversation.", True
        text, error = self.execute(name, arguments)
        extra = self.flow() if self.flow_address else []
        self.save_state()
        if extra:
            text += (
                "\n\n--- Also looked up automatically (current results; "
                "no need to repeat these calls) ---"
            )
            for tool, args, result, failed in extra:
                status = " (error)" if failed else ""
                text += f"\n\n{tool} {json.dumps(args)}{status}:\n{result}"
        return text, error

    def ask_flow(self) -> dict:
        """What the flow does next, given the trajectory so far."""
        messages = [
            json.loads(line)
            for line in (self.dir / "trajectory.jsonl").read_text().splitlines()
        ]
        query = {"task_id": self.task_id, "messages": messages}
        host, port = self.flow_address.rsplit(":", 1)
        with socket.create_connection((host, int(port)), timeout=120) as s:
            s.sendall((json.dumps(query) + "\n").encode())
            data = b""
            while not data.endswith(b"\n"):
                chunk = s.recv(1 << 16)
                if not chunk:
                    break
                data += chunk
        return json.loads(data)

    def flow(self) -> list[tuple[str, dict, str, bool]]:
        """The flow's lookups after a call, until it hands back."""
        done = []
        while len(done) < self.flow_max and self.flow_lookups < self.flow_budget:
            self.flow_queries += 1
            try:
                answer = self.ask_flow()
            except (OSError, ValueError) as e:
                print(f"tau2_mcp: flow unavailable: {e}", file=sys.stderr)
                break
            if answer.get("action") != "lookup" or answer.get("tool") not in self.tools:
                break
            tool, args = answer["tool"], answer.get("arguments") or {}
            self.flow_lookups += 1
            result, failed = self.execute(tool, args)
            done.append((tool, args, result, failed))
        return done


async def serve(episode: Episode) -> None:
    server = Server("tau2")

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return episode.list_tools()

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> types.CallToolResult:
        async with episode.lock:
            text, error = await asyncio.to_thread(episode.call, name, arguments or {})
        return types.CallToolResult(
            content=[types.TextContent(type="text", text=text)], isError=error
        )

    async with stdio_server() as (read, write):
        await server.run(read, write, server.create_initialization_options())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--domain", default="retail")
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--episode-dir", required=True, type=Path)
    parser.add_argument("--max-calls", type=int, default=60)
    parser.add_argument("--flow-address", help="stretto flow-serve's HOST:PORT (flows arm)")
    parser.add_argument("--flow-max", type=int, default=8, help="flow lookups per agent call")
    parser.add_argument("--flow-budget", type=int, default=40, help="flow lookups per episode")
    args = parser.parse_args()
    episode = Episode(
        args.domain,
        args.task_id,
        args.episode_dir,
        args.max_calls,
        args.flow_address,
        args.flow_max,
        args.flow_budget,
    )
    episode.save_state()
    asyncio.run(serve(episode))


if __name__ == "__main__":
    sys.exit(main())
