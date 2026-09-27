"""A recorded episode's tools, answered from the record: replay without an environment.

`check_flow.py --trace` replays an agent's recorded episode with a flow
serving lookups, as it does against τ²-bench's environment, but answers every
call from the recorded trajectory instead. The agent's own calls return what
they returned. A flow lookup returns what the agent's own later call with the
same arguments returned, if the agent made one before its next write: with
no write in between a read returns the same, so that answer is exact, and it
is the only kind the replay rule counts. A lookup the agent never made before
its next write is a detour whatever it returned; the record cannot say what
that was, so it returns what another call of the same tool returned in the
episode (the nearest before it, else after it), a result of the right shape
for the flow's next decisions to read, or an empty one if the tool was never
called; `guessed` marks it, and check_flow.py never counts it as a saving,
even when the agent's own call after a write returned the same. So on any
benchmark whose trajectories keep each call's result, a replay needs only the
record and which tools write.

It stands in for `tau2_mcp.Episode` in check_flow.py's in-process replays:
the same trajectory file, the same state file, the same flow client.
"""

import json
import socket
import sys
from pathlib import Path

# What a lookup the record cannot answer returns.
UNRECORDED = "{}"


def key(name: str, arguments: dict) -> tuple[str, str]:
    return name, json.dumps(arguments, sort_keys=True)


def recorded_calls(messages: list, writes: set[str]) -> list[dict]:
    """The agent's calls in order, each with what it returned and whether it
    writes. Results name their call by id, or follow their calls in order.
    A customer's own calls (telecom's phone) are paired with their results
    but left out: whether one changed what the agent reads, the record
    cannot say, and a lookup is answered across it."""
    calls, by_id, waiting = [], {}, []
    for m in messages:
        if m["role"] == "assistant":
            for c in m.get("tool_calls") or []:
                call = {"key": key(c["name"], c.get("arguments") or {}), "write": c["name"] in writes,
                        "result": "", "error": False}
                calls.append(call)
                waiting.append(call)
                if c.get("id") is not None:
                    by_id[c["id"]] = call
        elif m["role"] == "user":
            for c in m.get("tool_calls") or []:
                call = {"customer": True}
                waiting.append(call)
                if c.get("id") is not None:
                    by_id[c["id"]] = call
        elif m["role"] == "tool" and waiting:
            call = by_id.get(m.get("id"))
            if call is None:
                call = waiting[0]
            if call in waiting:
                waiting.remove(call)
            if not call.get("customer"):
                call["result"] = m.get("content") or ""
                call["error"] = bool(m.get("error"))
    return calls


def declared(tools_py: Path) -> tuple[set[str], set[str]]:
    """The reads and writes a tools.py marks with τ²-bench's decorator, for a
    benchmark τ²-bench does not know, laid out as its checkout."""
    reads, writes, pending = set(), set(), None
    for line in tools_py.read_text().splitlines():
        t = line.strip()
        if t.startswith("@is_tool(ToolType."):
            pending = t.split(".", 1)[1].split(")")[0]
        elif t.startswith("def ") and pending is not None:
            (writes if pending == "WRITE" else reads).add(t[4:].split("(")[0].strip())
            pending = None
    return reads, writes


class TraceEpisode:
    """One recorded episode's tools, and the flow's lookups, from the record."""

    def __init__(
        self,
        messages: list,
        writes: set[str],
        task_id: str,
        directory: Path,
        max_calls: int,
        flow_address: str | None = None,
        flow_max: int = 8,
        flow_budget: int = 40,
        record_answers: bool = False,
    ):
        self.dir = directory
        self.task_id = str(task_id)
        self.recorded = recorded_calls(messages, writes)
        self.tools = {c["key"][0] for c in self.recorded}
        self.at = 0  # the next recorded call the agent has not made yet
        self.calls = 0
        self.max_calls = max_calls
        self.flow_address = flow_address
        self.flow_max = flow_max
        self.flow_budget = flow_budget
        self.flow_queries = 0
        self.flow_lookups = 0
        self.unrecorded = 0
        self.guessed: list[bool] = []  # per lookup, in order: answered by a stand-in
        self.record_answers = record_answers
        self.ids = 0

    def append(self, *messages: dict) -> None:
        with open(self.dir / "trajectory.jsonl", "a") as f:
            for m in messages:
                f.write(json.dumps(m) + "\n")

    def save_state(self, over_budget: bool = False) -> None:
        (self.dir / "tools-state.json").write_text(json.dumps({
            "tool_calls": self.calls,
            "over_budget": over_budget,
            "flow_queries": self.flow_queries,
            "flow_lookups": self.flow_lookups,
            "unrecorded": self.unrecorded,
        }))

    def record(self, name: str, arguments: dict, result: str, error: bool) -> None:
        self.ids += 1
        cid = f"call_{self.ids}"
        self.append(
            {"role": "assistant", "content": None,
             "tool_calls": [{"id": cid, "name": name, "arguments": arguments, "requestor": "assistant"}]},
            {"id": cid, "role": "tool", "content": result, "requestor": "assistant", "error": error},
        )

    def own(self, name: str, arguments: dict) -> tuple[str, bool]:
        """The agent's call: what the record says it returned."""
        k = key(name, arguments)
        for i in range(self.at, len(self.recorded)):
            if self.recorded[i]["key"] == k:
                self.at = i + 1
                return self.recorded[i]["result"], self.recorded[i]["error"]
        raise ValueError(f"trace_env: {name} {k[1]} is not in the record after call {self.at}")

    def ahead(self, name: str, arguments: dict) -> tuple[str, bool] | None:
        """A lookup: what the agent's own later call returned, if it made the
        same call before its next write."""
        k = key(name, arguments)
        for c in self.recorded[self.at:]:
            if c["key"] == k:
                return c["result"], c["error"]
            if c["write"]:
                return None
        return None

    def stand_in(self, name: str) -> tuple[str, bool]:
        """For a detour: what another call of the same tool returned, the
        nearest before the agent's position, else the nearest after it."""
        before = [c for c in self.recorded[:self.at] if c["key"][0] == name and not c["error"]]
        after = [c for c in self.recorded[self.at:] if c["key"][0] == name and not c["error"]]
        pick = before[-1] if before else after[0] if after else None
        return (pick["result"], False) if pick else (UNRECORDED, False)

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        """The agent's call, then (with a flow) the flow's lookups."""
        self.calls += 1
        if self.calls > self.max_calls:
            self.save_state(over_budget=True)
            return "Tool call limit reached for this conversation.", True
        text, error = self.own(name, arguments)
        self.record(name, arguments, text, error)
        extra = self.flow() if self.flow_address else []
        self.save_state()
        if extra:
            text += ("\n\n--- Also looked up automatically (current results; "
                     "no need to repeat these calls) ---")
            for tool, args, result, failed in extra:
                status = " (error)" if failed else ""
                text += f"\n\n{tool} {json.dumps(args)}{status}:\n{result}"
        return text, error

    def ask_flow(self) -> dict:
        messages = [json.loads(line) for line in (self.dir / "trajectory.jsonl").read_text().splitlines()]
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
                print(f"trace_env: flow unavailable: {e}", file=sys.stderr)
                break
            if self.record_answers:
                with open(self.dir / "flow-answers.jsonl", "a") as f:
                    f.write(json.dumps({"call": self.calls, "answer": answer}) + "\n")
            if answer.get("action") != "lookup" or answer.get("tool") not in self.tools:
                break
            tool, args = answer["tool"], answer.get("arguments") or {}
            self.flow_lookups += 1
            found = self.ahead(tool, args)
            self.guessed.append(found is None)
            if found is None:
                self.unrecorded += 1
                found = self.stand_in(tool)
            result, failed = found
            self.record(tool, args, result, failed)
            done.append((tool, args, result, failed))
        return done
