#!/usr/bin/env python3
"""DTap-Bench's published benign runs as τ²-bench results, for trace-only replays.

DTap-Bench (AI-Secure) runs agents built on four harnesses, the Claude Agent
SDK, the OpenAI Agents SDK, Google's ADK and OpenClaw, against MCP servers in
fourteen domains. It publishes every run's trajectory
(`<harness>/<model>/<domain>/benign/<task>/<time>.json`) with a judge's
verdict (`judge_result.json`). This writes each harness and model's runs of
one domain as τ²-bench's results file, and a checkout-shaped folder that
stretto reads as τ²-bench's: the tools, marked a read or a write, and a
train/test split by task.

    dtap_to_tau2.py --runs DIR --domain customer-service --out DIR [--name NAME]

`--runs` holds `<domain>/<harness>__<model>/<task>/{traj,judge}.json`, the
latest trajectory of each task and its verdict. A run's steps are the agent's
actions and the tools' results. The agent's consecutive calls are one turn,
together with any message to the user it sent with them. Results answer the
calls in the order they were made. The Claude Agent SDK's logs name some
results' tools wrongly, so names are not used to pair them. Calls that only
list or load tools (`List MCP Tools`, the Claude Agent SDK's `ToolSearch`)
belong to the harness, not to the domain. They are left out with their
results, and so is a turn that made nothing else. OpenClaw's logs record no
calls, so its runs are skipped. The Claude Agent SDK's own tools (`Bash`,
`Read`, `TodoWrite`, ...) stay as calls but are neither reads nor writes. A
tool is a read when the first verb in its name reads (`get`, `list`,
`search`, `find`, `lookup`, ..., in `getJiraIssue` or `meetings_get` alike);
a name with a writing verb, or with none known, is a write, which a flow
never calls.
"""

import argparse
import ast
import json
import re
import sys
from pathlib import Path

# The harness's calls that list or load tools: left out, with their results.
META = {"List MCP Tools", "ToolSearch", "ListMcpResourcesTool", "ReadMcpResourceTool"}
# The Claude Agent SDK's own tools, which some agents call: kept as calls, but neither a read nor a write.
BUILTIN = {"Bash", "Read", "Write", "Edit", "MultiEdit", "Glob", "Grep", "LS", "TodoWrite", "AskUserQuestion",
           "WebFetch", "WebSearch", "Task", "Agent", "Skill", "NotebookEdit", "ExitPlanMode"}
READ_WORDS = {"get", "list", "search", "find", "lookup", "view", "read", "fetch", "query", "count", "describe",
              "retrieve", "show", "history", "inbox", "info", "health", "meta", "tree", "exists", "browse"}
WRITE_WORDS = {"create", "update", "add", "delete", "remove", "send", "reply", "forward", "grant", "cancel", "refund",
               "set", "post", "assign", "convert", "merge", "link", "unlink", "invite", "login", "logout", "join",
               "leave", "edit", "transition", "pause", "resume", "modify", "transfer", "apply", "book", "pay",
               "schedule", "submit", "approve", "reject", "close", "mark", "move", "upload", "share", "archive", "request",
               "restore", "reset", "enable", "disable", "change", "suspend", "register", "issue", "process"}


def words(name: str) -> list[str]:
    """A tool's name as words: `getJiraIssue` and `meetings_get` alike."""
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).lower().replace("-", "_").split("_")


def is_read(name: str) -> bool:
    """A read when the first verb in its name reads (`get`, `list`, `search`, ...); a name with no
    verb it knows is a write, which a flow never calls. An HTTP method's `post` is a write unless a
    read verb follows it, as in Notion's `API-post-search` and `API-post-database-query`."""
    ws = words(name)
    for k, w in enumerate(ws):
        if w == "post" and any(x in READ_WORDS for x in ws[k + 1:]):
            continue
        if w in WRITE_WORDS:
            return False
        if w in READ_WORDS:
            return True
    return False


def result(state) -> str:
    """A tool's result as JSON. A harness that wraps it as {"result": "<json>"} is unwrapped, and so is an
    MCP text block, {"type": "text", "text": "<json>"}, which the OpenAI Agents SDK logs in some domains as
    a Python repr."""
    if isinstance(state, dict) and set(state) == {"result"} and isinstance(state["result"], str):
        state = state["result"]
    if isinstance(state, str):
        try:
            state = json.loads(state)
        except ValueError:
            try:
                state = ast.literal_eval(state)
            except (ValueError, SyntaxError, MemoryError, RecursionError):
                return state
            if not isinstance(state, (dict, list)):
                return str(state)
    if isinstance(state, dict) and state.get("type") == "text" and isinstance(state.get("text"), str):
        return result(state["text"])
    if isinstance(state, list) and len(state) == 1 and isinstance(state[0], dict) and state[0].get("type") == "text":
        return result(state[0].get("text") or "")
    return json.dumps(state, ensure_ascii=False)


def convert(run: dict) -> tuple[list, int]:
    """τ²-bench's messages for one run, and the number of harness calls left out."""
    messages = [{"role": "user", "content": run["task_info"]["original_instruction"]}]
    turn: dict | None = None  # the assistant message being gathered
    pending: list[dict | None] = []  # calls awaiting results, in order (None: a harness call)
    n = dropped = 0
    first_user = True

    def close():
        nonlocal turn
        if turn is not None:
            if turn["tool_calls"] or turn["content"]:
                if not turn["tool_calls"]:
                    turn.pop("tool_calls")
                messages.append(turn)
            turn = None

    for step in run["trajectory"]:
        role, meta = step.get("role"), step.get("metadata") or {}
        if role == "user":
            close()
            if first_user:
                first_user = False  # the request, written above
                continue
            messages.append({"role": "user", "content": str(step.get("state") or "")})
        elif role == "agent":
            if turn is None:
                turn = {"role": "assistant", "content": None, "tool_calls": []}
            if step.get("action") == "send_message_to_user":
                text = str(meta.get("message") or "")
                turn["content"] = (turn["content"] + "\n" if turn["content"] else "") + text
                continue
            name = meta.get("tool_name") or ""
            if name in META:
                pending.append(None)
                dropped += 1
                continue
            n += 1
            call = {"id": f"call_{n}", "name": name, "arguments": meta.get("tool_params") or {}, "requestor": "assistant"}
            turn["tool_calls"].append(call)
            pending.append(call)
        elif role == "tool":
            if turn is not None and turn["tool_calls"]:
                calls = turn["tool_calls"]
                close()
                messages.extend({"role": "tool", "id": c["id"], "content": None, "requestor": "assistant"} for c in calls)
            elif turn is not None:
                close()
            if not pending:
                continue
            call = pending.pop(0)
            if call is None:
                continue
            out = result(step.get("state"))
            for m in messages:
                if m["role"] == "tool" and m["id"] == call["id"] and m["content"] is None:
                    m["content"] = out
                    m["error"] = out.startswith('{"ok": false') or out.lower().startswith("error")
                    break
    close()
    # A call whose result the log lost keeps an empty result.
    for m in messages:
        if m["role"] == "tool" and m["content"] is None:
            m["content"] = ""
            m["error"] = True
    return messages, dropped


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=Path, required=True)
    ap.add_argument("--domain", default="customer-service")
    ap.add_argument("--name", help="the domain's name in τ²-bench's layout (default: --domain with underscores); "
                    "one that τ²-bench or another benchmark also has, such as telecom, takes a prefix")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--test-share", type=float, default=0.4, help="share of tasks held out, the same share of every ten in order")
    args = ap.parse_args()
    domain = args.name or args.domain.replace("-", "_")
    root = args.runs / args.domain
    names: set[str] = set()
    tasks: set[str] = set()
    (args.out / domain).mkdir(parents=True, exist_ok=True)
    for config in sorted(p for p in root.iterdir() if p.is_dir()):
        harness, model = config.name.split("__", 1)
        if harness == "openclaw":
            continue  # its logs record the reply only
        sims, dropped, calls = [], 0, 0
        for task_dir in sorted(p for p in config.iterdir() if p.is_dir()):
            traj = task_dir / "traj.json"
            if not traj.exists():
                continue
            run = json.loads(traj.read_text())
            messages, d = convert(run)
            dropped += d
            calls += sum(len(m.get("tool_calls") or []) for m in messages)
            names |= {c["name"] for m in messages for c in m.get("tool_calls") or []}
            judge = task_dir / "judge.json"
            ok = json.loads(judge.read_text()).get("task_success") if judge.exists() else None
            tasks.add(task_dir.name)
            sims.append({"id": f"{task_dir.name}-0", "task_id": task_dir.name, "trial": 0, "messages": messages,
                         "reward_info": {"reward": 1.0 if ok else 0.0}, "termination_reason": "agent_stop"})
        agent = f"{harness}-{model}"
        results = {"info": {"environment_info": {"domain_name": domain}, "agent_info": {"llm": agent, "harness": harness},
                            "user_info": {"llm": None}, "num_trials": 1},
                   "tasks": [{"id": s["task_id"]} for s in sims], "simulations": sims}
        (args.out / domain / f"{agent}.json").write_text(json.dumps(results))
        print(f"dtap_to_tau2: {agent}: {len(sims)} runs, {calls} calls ({dropped} harness calls left out), "
              f"{sum(s['reward_info']['reward'] for s in sims):.0f} judged successful", file=sys.stderr)
    order = sorted(tasks)
    held = round(10 * args.test_share)
    split = {"train": [t for i, t in enumerate(order) if i % 10 >= held],
             "test": [t for i, t in enumerate(order) if i % 10 < held]}
    checkout = args.out / "checkout"
    (checkout / f"src/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
    (checkout / f"data/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
    lines = [f"# DTap-Bench's tools in its {args.domain} tasks, marked by their verbs (dtap_to_tau2.py).", ""]
    for name in sorted(n for n in names if n.isidentifier() and n not in BUILTIN):
        lines += [f"@is_tool(ToolType.{'READ' if is_read(name) else 'WRITE'})", f"def {name}(*args, **kwargs):", "    pass", ""]
    (checkout / f"src/tau2/domains/{domain}/tools.py").write_text("\n".join(lines))
    (checkout / f"data/tau2/domains/{domain}/split_tasks.json").write_text(json.dumps(split))


if __name__ == "__main__":
    main()
