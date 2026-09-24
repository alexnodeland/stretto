"""One τ²-bench task's tools, served over MCP (stdio).

The server holds the task's environment for a whole episode and executes
the agent's tool calls against it. Each call and its result are appended to
the episode's `trajectory.jsonl` in τ²-bench's own message format, where
`run_episode.py` also appends the conversation, so the official evaluator
can score the episode afterwards. A call budget guards against runaway
episodes.

Usage (normally started by the agent, behind `stretto-proxy`):

    python tau2_mcp.py --domain retail --task-id 0 --episode-dir DIR
"""

import argparse
import asyncio
import json
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

    def __init__(self, domain: str, task_id: str, directory: Path, max_calls: int):
        self.dir = directory
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

    def append(self, *messages) -> None:
        with open(self.dir / "trajectory.jsonl", "a") as f:
            for m in messages:
                f.write(m.model_dump_json() + "\n")

    def save_state(self, over_budget: bool = False) -> None:
        (self.dir / "tools-state.json").write_text(
            json.dumps({"tool_calls": self.calls, "over_budget": over_budget})
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

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        self.calls += 1
        if self.calls > self.max_calls:
            self.save_state(over_budget=True)
            return "Tool call limit reached for this conversation.", True
        call = ToolCall(
            id=f"call_{uuid.uuid4().hex[:12]}",
            name=name,
            arguments=arguments,
            requestor="assistant",
        )
        result = self.env.get_response(call)
        self.append(AssistantMessage(role="assistant", content=None, tool_calls=[call]), result)
        self.save_state()
        return result.content or "", bool(result.error)


async def serve(episode: Episode) -> None:
    server = Server("tau2")

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return episode.list_tools()

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> types.CallToolResult:
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
    args = parser.parse_args()
    episode = Episode(args.domain, args.task_id, args.episode_dir, args.max_calls)
    episode.save_state()
    asyncio.run(serve(episode))


if __name__ == "__main__":
    sys.exit(main())
