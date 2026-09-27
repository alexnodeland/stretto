#!/usr/bin/env python3
"""AgentDojo's published runs as τ²-bench results, for trace-only replays.

AgentDojo (Debenedetti et al. 2024) gives an agent one instruction in a
simulated workspace (email, calendar, files), Slack, a bank account or a
travel agency, and no user after it; its repository publishes each model's
run of every task, the benign ones under `runs/<pipeline>/<suite>/user_task_<n>/none/none.json`,
each message with its tool calls and what they returned. This writes each
pipeline's benign runs of each suite as τ²-bench's results file, and a
checkout-shaped folder that stretto reads as τ²-bench's: each suite's tools,
marked a read or a write, and a train/test split by task.

    agentdojo_to_tau2.py --runs agentdojo/runs --out DIR [--test-share 0.4]

The suite is the domain. Pipelines that add a defense to a model (a suffix
such as `-tool_filter`) are left out unless `--all` is given. Results are
written as JSON, the form an MCP server returns: AgentDojo prints them as YAML,
Python literals, or a label and one name a line, and the values in them are
the same (`as_json`). Needs PyYAML.
"""

import argparse
import ast
import json
import sys
from pathlib import Path

import yaml

# Which calls change the state; every other call only reads it. Reading unread
# emails marks them read, so a second call returns other emails: a write.
WRITES = {
    "workspace": {
        "add_calendar_event_participants", "append_to_file", "cancel_calendar_event", "create_calendar_event",
        "create_file", "delete_email", "delete_file", "get_unread_emails", "reschedule_calendar_event",
        "send_email", "share_file",
    },
    "slack": {
        "add_user_to_channel", "invite_user_to_slack", "post_webpage", "remove_user_from_slack",
        "send_channel_message", "send_direct_message",
    },
    "banking": {"schedule_transaction", "send_money", "update_password", "update_scheduled_transaction",
                "update_user_info"},
    "travel": {"cancel_calendar_event", "create_calendar_event", "reserve_car_rental", "reserve_hotel",
               "reserve_restaurant", "send_email"},
}
DEFENSES = ("-repeat_user_prompt", "-spotlighting_with_delimiting", "-tool_filter", "-transformers_pi_detector")


def text_of(content) -> str | None:
    """A message's text: a string, or (in later runs) a list of text blocks."""
    if content is None or isinstance(content, str):
        return content
    return "".join(b.get("content") or b.get("text") or "" for b in content if isinstance(b, dict))


def as_json(text: str) -> str:
    """A result as JSON: AgentDojo's Python literals (`{'City Hub': ...}`),
    its YAML records, and its listings (`Restaurant in Paris: A\nB\nC`, a
    label and one name a line, as `{"label": ..., "items": [...]}`, so that
    where a name sits does not depend on the city), with the same values;
    anything else as it is."""
    t = text.strip()
    if not t:
        return text
    try:
        json.loads(t)
        return text
    except ValueError:
        pass
    try:
        v = ast.literal_eval(t)
        if isinstance(v, (dict, list, tuple)):
            return json.dumps(v, default=str)
    except (ValueError, SyntaxError, MemoryError, RecursionError):
        pass
    lines = [line.strip() for line in t.split("\n") if line.strip()]
    if len(lines) > 1 and ": " in lines[0] and not any(": " in x or x.startswith("- ") for x in lines[1:]):
        label, first = lines[0].split(": ", 1)
        return json.dumps({"label": label, "items": [first] + lines[1:]})
    try:
        v = yaml.safe_load(t)
    except yaml.YAMLError:
        return text
    return json.dumps(v, default=str) if isinstance(v, (dict, list)) else text


def convert(messages: list) -> list:
    """τ²-bench's messages for an AgentDojo run: the user's instruction, the
    agent's turns with their calls, and each call's result. Some pipelines
    leave calls without ids; theirs are numbered, and results follow their
    calls in order."""
    out, waiting, n = [], [], 0
    for m in messages:
        role = m.get("role")
        if role == "user":
            out.append({"role": "user", "content": text_of(m.get("content")) or ""})
        elif role == "assistant":
            calls = []
            for c in m.get("tool_calls") or []:
                n += 1
                cid = c.get("id") or f"call_{n}"
                waiting.append(cid)
                calls.append({"id": cid, "name": c["function"], "arguments": c.get("args") or {},
                              "requestor": "assistant"})
            out.append({"role": "assistant", "content": text_of(m.get("content")), "tool_calls": calls or None})
        elif role == "tool":
            cid = m.get("tool_call_id")
            if cid not in waiting:
                cid = waiting[0] if waiting else f"call_{n}"
            if cid in waiting:
                waiting.remove(cid)
            out.append({"role": "tool", "id": cid, "content": as_json(text_of(m.get("content")) or ""),
                        "requestor": "assistant", "error": m.get("error") is not None})
    return out


def tool_stubs(suite: str, names: set[str]) -> str:
    lines = [f"# AgentDojo's {suite} tools, marked as τ²-bench marks its own (written by agentdojo_to_tau2.py).", ""]
    for name in sorted(names):
        kind = "WRITE" if name in WRITES[suite] else "READ"
        lines += [f"@is_tool(ToolType.{kind})", f"def {name}(*args, **kwargs):", "    pass", ""]
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=Path, required=True, help="AgentDojo's runs folder")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--test-share", type=float, default=0.4, help="share of tasks held out, the same share of every ten in order")
    ap.add_argument("--all", action="store_true", help="also the pipelines that add a defense")
    args = ap.parse_args()
    pipelines = sorted(p.name for p in args.runs.iterdir() if p.is_dir()
                       and (args.all or not p.name.endswith(DEFENSES)))
    for suite in WRITES:
        tasks = sorted({p.parent.parent.name for p in args.runs.glob(f"*/{suite}/user_task_*/none/none.json")},
                       key=lambda t: int(t.rsplit("_", 1)[1]))
        held = round(10 * args.test_share)
        split = {"train": [t for i, t in enumerate(tasks) if i % 10 >= held],
                 "test": [t for i, t in enumerate(tasks) if i % 10 < held]}
        names: set[str] = set()
        (args.out / suite).mkdir(parents=True, exist_ok=True)
        written = 0
        for pipeline in pipelines:
            sims = []
            for task in tasks:
                path = args.runs / pipeline / suite / task / "none/none.json"
                if not path.exists():
                    continue
                run = json.loads(path.read_text())
                messages = convert(run["messages"])
                names |= {c["name"] for m in messages for c in m.get("tool_calls") or []}
                sims.append({"id": f"{task}-0", "task_id": task, "trial": 0, "messages": messages,
                             "reward_info": {"reward": 1.0 if run.get("utility") else 0.0},
                             "termination_reason": "agent_stop" if run.get("error") is None else "error"})
            if not sims:
                continue
            results = {"info": {"environment_info": {"domain_name": suite}, "agent_info": {"llm": pipeline},
                                "user_info": {"llm": None}, "num_trials": 1},
                       "tasks": [{"id": t} for t in tasks], "simulations": sims}
            (args.out / suite / f"{pipeline}.json").write_text(json.dumps(results))
            written += 1
        unknown = names - WRITES[suite]
        checkout = args.out / "checkout"
        (checkout / f"src/tau2/domains/{suite}").mkdir(parents=True, exist_ok=True)
        (checkout / f"data/tau2/domains/{suite}").mkdir(parents=True, exist_ok=True)
        (checkout / f"src/tau2/domains/{suite}/tools.py").write_text(tool_stubs(suite, names))
        (checkout / f"data/tau2/domains/{suite}/split_tasks.json").write_text(json.dumps(split))
        print(f"agentdojo_to_tau2: {suite}: {written} pipelines, {len(split['train'])} train / {len(split['test'])} "
              f"test tasks, {len(names)} tools ({len(names) - len(unknown)} write)", file=sys.stderr)


if __name__ == "__main__":
    main()
