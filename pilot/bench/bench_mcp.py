"""One task of another benchmark, served over MCP (stdio), for live episodes.

The server holds the task's environment for a whole episode and runs each call
against it with the benchmark's own code. Each call and its result are appended
to the episode's `trajectory.jsonl` (τ²-bench's message shape), and the
environment's state after every call is saved to `state.pkl`, which
`run_bench_episode.py` scores once the episode ends. A call budget guards
against runaway episodes. `tools/list` marks each tool `readOnlyHint: true` or
`false` from the lists the converters use, so a flow behind `stretto-proxy`
looks up only reads.

Results are printed as the benchmark prints them to its own agents, then as the
converter (`scripts/*_to_tau2.py`) turned the published runs into JSON, so a
flow learned from those runs finds its arguments at the same paths live.

Backends:

- `agentdojo`: an AgentDojo suite (workspace, slack, banking, travel) at a
  benchmark version; the tools are the suite's, run by AgentDojo's own runtime
  against the user task's environment, and printed by its pipeline's formatter
  (YAML) before `agentdojo_to_tau2.as_json`.
- `bfcl`: a task of the Berkeley Function Calling Leaderboard's multi-turn
  categories; the tools are the involved classes' methods, described by BFCL's
  own function docs, run on instances loaded with the task's initial state, and
  printed as BFCL's executor prints them (a string as it is, a dict as JSON).
  BFCL is imported from `--bfcl-dir` (an unpacked `bfcl-eval` wheel or its
  site-packages).

Usage (normally started by the agent's MCP host, behind `stretto-proxy`):

    python bench_mcp.py agentdojo --suite travel --task user_task_3 --episode-dir DIR [--version v1.2.1]
    python bench_mcp.py bfcl --task multi_turn_base_12 --bfcl-dir DIR --episode-dir DIR
"""

import argparse
import asyncio
import json
import pickle
import sys
import uuid
from ast import literal_eval
from pathlib import Path

import mcp.types as types
from mcp.server import Server
from mcp.server.stdio import stdio_server

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))


class AgentDojo:
    """An AgentDojo user task's environment and tools."""

    def __init__(self, suite: str, task: str, version: str):
        from agentdojo.functions_runtime import FunctionsRuntime
        from agentdojo.task_suite.load_suites import get_suite

        import agentdojo_to_tau2

        self.suite = get_suite(version, suite)
        self.task = self.suite.get_user_task_by_id(task)
        self.env = self.task.init_environment(self.suite.load_and_inject_default_environment({}))
        self.runtime = FunctionsRuntime(self.suite.tools)
        self.writes = agentdojo_to_tau2.WRITES[suite]
        self.as_json = agentdojo_to_tau2.as_json

    def tools(self) -> list[types.Tool]:
        return [
            types.Tool(
                name=f.name,
                description=f.description,
                inputSchema=f.parameters.model_json_schema(),
                annotations=types.ToolAnnotations(readOnlyHint=f.name not in self.writes),
            )
            for f in self.suite.tools
        ]

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        from agentdojo.agent_pipeline.tool_execution import is_string_list, tool_result_to_str

        # As AgentDojo's tool executor: a list the model passed as a string is read as a list.
        arguments = {k: literal_eval(v) if isinstance(v, str) and is_string_list(v) else v for k, v in arguments.items()}
        result, error = self.runtime.run_function(self.env, name, arguments)
        if error:
            return error, True
        return self.as_json(tool_result_to_str(result)), False

    def state(self) -> bytes:
        # Pickled, not JSON: AgentDojo's checks read private fields a JSON dump leaves out.
        return pickle.dumps(self.env)


def bfcl_schema(node):
    """BFCL's parameter schema as JSON Schema: its `dict`, `float`, `tuple` and `any` types renamed."""
    if isinstance(node, list):
        return [bfcl_schema(n) for n in node]
    if not isinstance(node, dict):
        return node
    out = {k: bfcl_schema(v) for k, v in node.items()}
    kind = out.get("type")
    if kind in ("dict", "any"):
        out["type"] = "object" if kind == "dict" else None
    elif kind == "float":
        out["type"] = "number"
    elif kind == "tuple":
        out["type"] = "array"
    return {k: v for k, v in out.items() if v is not None}


def bfcl_entry(task: str, bfcl_dir: Path) -> tuple[dict, dict]:
    """A BFCL task's entry and its ground truth, from the package's data."""
    root = bfcl_dir / "bfcl_eval" / "data"
    category = task.rsplit("_", 1)[0]
    entry = next(json.loads(l) for l in (root / f"BFCL_v4_{category}.json").read_text().splitlines()
                 if l.strip() and json.loads(l)["id"] == task)
    answer = next(json.loads(l) for l in (root / "possible_answer" / f"BFCL_v4_{category}.json").read_text().splitlines()
                  if l.strip() and json.loads(l)["id"] == task)
    return entry, answer


class BFCL:
    """A BFCL multi-turn task's classes, loaded with its initial state."""

    def __init__(self, task: str, bfcl_dir: Path):
        import copy
        import importlib
        import inspect

        sys.path.insert(0, str(bfcl_dir))
        import bfcl_to_tau2

        self.entry, _ = bfcl_entry(task, bfcl_dir)
        self.writes = bfcl_to_tau2.WRITES
        docs_dir = bfcl_dir / "bfcl_eval" / "data" / "multi_turn_func_doc"
        self.methods, self.docs, self.instances = {}, {}, {}
        prefix = "bfcl_eval.eval_checker.multi_turn_eval.func_source_code"
        for cls in self.entry["involved_classes"]:
            module = bfcl_to_tau2.MODULES[cls]
            inst = getattr(importlib.import_module(f"{prefix}.{module}"), cls)()
            if cls not in bfcl_to_tau2.STATELESS:
                inst._load_scenario(copy.deepcopy(self.entry["initial_config"].get(cls, {})),
                                    long_context="long_context" in self.entry["id"])
            self.instances[cls] = inst
            for name, method in inspect.getmembers(inst, predicate=inspect.ismethod):
                if not name.startswith("_"):
                    self.methods[name] = method
            for line in (docs_dir / f"{module}.json").read_text().splitlines():
                if line.strip():
                    doc = json.loads(line)
                    self.docs[doc["name"]] = doc
        # The miss-function categories hold some functions back until a later turn.
        self.missing = {f for fs in (self.entry.get("missed_function") or {}).values() for f in fs}

    def tools(self) -> list[types.Tool]:
        return [
            types.Tool(
                name=name,
                description=doc.get("description", ""),
                inputSchema=bfcl_schema(doc.get("parameters") or {"type": "dict", "properties": {}}),
                annotations=types.ToolAnnotations(readOnlyHint=name not in self.writes),
            )
            for name, doc in sorted(self.docs.items())
            if name in self.methods and name not in self.missing
        ]

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        method = self.methods.get(name)
        if method is None or name in self.missing:
            return f"Error during execution: no such function {name}", True
        try:
            out = method(**arguments)
        except Exception as e:  # noqa: BLE001 (the backend's own error is its result, as in BFCL's executor)
            return f"Error during execution: {e}", True
        if isinstance(out, dict):
            try:
                out = json.dumps(out)
            except (TypeError, ValueError):
                out = str(out)
        elif not isinstance(out, str):
            out = str(out)
        return out, out.startswith("{") and '"error"' in out

    def state(self) -> bytes:
        return pickle.dumps({cls: vars(inst) for cls, inst in self.instances.items()})

    @staticmethod
    def call_text(name: str, arguments: dict) -> str:
        """The call as BFCL writes it, `f(a=1, b='x')`."""
        return f"{name}({', '.join(f'{k}={v!r}' for k, v in arguments.items())})"


class Episode:
    """The backend, the record of the episode, and the call budget."""

    def __init__(self, backend, directory: Path, max_calls: int):
        self.backend = backend
        self.dir = directory
        self.max_calls = max_calls
        self.calls = 0
        self.lock = asyncio.Lock()
        self.save()

    def save(self, over_budget: bool = False) -> None:
        try:
            (self.dir / "state.pkl").write_bytes(self.backend.state())
        except Exception as e:  # a state that cannot be pickled is not recorded; scoring does not need it
            (self.dir / "state.pkl").write_bytes(b"")
            print(f"bench_mcp: state not saved: {e}", file=sys.stderr)
        (self.dir / "tools-state.json").write_text(json.dumps({"tool_calls": self.calls, "over_budget": over_budget}))

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        self.calls += 1
        if self.calls > self.max_calls:
            self.save(over_budget=True)
            return "Tool call limit reached for this conversation.", True
        try:
            text, error = self.backend.call(name, arguments)
        except Exception as e:  # the benchmark's own code failed on these arguments
            text, error = f"{type(e).__name__}: {e}", True
        cid = f"call_{uuid.uuid4().hex[:12]}"
        with open(self.dir / "trajectory.jsonl", "a") as f:
            f.write(json.dumps({"role": "assistant", "content": None, "tool_calls": [
                {"id": cid, "name": name, "arguments": arguments, "requestor": "assistant"}]}) + "\n")
            f.write(json.dumps({"role": "tool", "id": cid, "content": text, "requestor": "assistant",
                                "error": error}) + "\n")
        self.save()
        return text, error


async def serve(episode: Episode) -> None:
    server = Server("bench")

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return episode.backend.tools()

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> types.CallToolResult:
        async with episode.lock:
            text, error = await asyncio.to_thread(episode.call, name, arguments or {})
        return types.CallToolResult(content=[types.TextContent(type="text", text=text)], isError=error)

    async with stdio_server() as (read, write):
        await server.run(read, write, server.create_initialization_options())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("bench", choices=["agentdojo", "bfcl"])
    parser.add_argument("--suite", help="AgentDojo's suite")
    parser.add_argument("--task", required=True)
    parser.add_argument("--version", default="v1.2.1", help="AgentDojo's benchmark version")
    parser.add_argument("--bfcl-dir", type=Path, help="the folder holding the bfcl_eval package")
    parser.add_argument("--episode-dir", type=Path, required=True)
    parser.add_argument("--max-calls", type=int, default=40)
    args = parser.parse_args()
    backend = AgentDojo(args.suite, args.task, args.version) if args.bench == "agentdojo" else BFCL(args.task, args.bfcl_dir)
    asyncio.run(serve(Episode(backend, args.episode_dir, args.max_calls)))


if __name__ == "__main__":
    main()
