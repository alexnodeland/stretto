#!/usr/bin/env python3
"""MCPMark's published runs as τ²-bench results, for trace-only replays.

MCPMark (eval-sys) gives agents tasks on real MCP servers: a filesystem,
PostgreSQL, GitHub, Notion and a browser. Its trajectory log
(huggingface.co/datasets/Jakumetsu/mcpmark-trajectory-log) keeps every run's
messages in the OpenAI Responses format, with a verdict in `meta.json`. This
writes each model's runs of one server as τ²-bench's results file, and a
checkout-shaped folder that stretto reads as τ²-bench's: the tools, marked a
read or a write, and a train/test split by task.

    mcpmark_to_tau2.py --runs DIR --service filesystem --out DIR

`--runs` holds `<model>__<service>/<task>/{messages,meta}.json`, one run
of each task. The request opens the run. The assistant's message and the
calls it makes before their results arrive are one turn. Results are paired
with calls by id, and MCP text blocks, which hold Python reprs, are unwrapped
to JSON as `dtap_to_tau2.py` does. Tools are marked as `dtap_to_tau2.py` marks
them, by the first verb in the name. A tool that runs whatever the agent
writes, such as `execute_sql`, is a write. MCPMark's tasks are hard, and
`learn` fits the habit on successful runs, so `--learn-from-all` gives every
run a reward of 1 and keeps the verdict as `verified`: a read-only flow needs
the agent's reads, which a failed run makes too.
"""

import argparse
import ast
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dtap_to_tau2 import is_read, result  # noqa: E402


def text_of(content) -> str:
    """An assistant message's text, whether a list of parts or a repr of one."""
    if isinstance(content, str):
        try:
            content = ast.literal_eval(content)
        except (ValueError, SyntaxError, MemoryError, RecursionError):
            return content
    if isinstance(content, list):
        return "\n".join(str(p.get("text") or "") for p in content if isinstance(p, dict)).strip()
    return str(content or "")


def convert(items: list) -> list:
    messages = []
    turn = None
    placed = {}

    def close():
        nonlocal turn
        if turn is not None:
            calls = turn["tool_calls"]
            if calls or turn["content"]:
                if not calls:
                    turn.pop("tool_calls")
                messages.append(turn)
                for c in calls:
                    placed[c["id"]] = len(messages)
                    messages.append({"role": "tool", "id": c["id"], "content": "", "requestor": "assistant", "error": True})
            turn = None

    for it in items:
        kind, role = it.get("type"), it.get("role")
        if role == "user" and kind in (None, "message"):
            close()
            messages.append({"role": "user", "content": text_of(it.get("content"))})
        elif kind == "message" and role == "assistant":
            if turn is not None and turn["tool_calls"]:
                close()
            if turn is None:
                turn = {"role": "assistant", "content": None, "tool_calls": []}
            text = text_of(it.get("content"))
            if text:
                turn["content"] = (turn["content"] + "\n" if turn["content"] else "") + text
        elif kind == "function_call":
            if turn is None:
                turn = {"role": "assistant", "content": None, "tool_calls": []}
            try:
                args = json.loads(it.get("arguments") or "{}")
            except ValueError:
                args = {"_unparsed": it.get("arguments")}
            turn["tool_calls"].append({"id": it.get("call_id"), "name": it.get("name") or "",
                                       "arguments": args if isinstance(args, dict) else {"_value": args},
                                       "requestor": "assistant"})
        elif kind == "function_call_output":
            close()
            k = placed.get(it.get("call_id"))
            if k is not None:
                out = result(it.get("output"))
                messages[k]["content"] = out
                messages[k]["error"] = out.lower().startswith("error")
    close()
    return messages


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=Path, required=True)
    ap.add_argument("--service", required=True, help="filesystem, postgres, github, notion or playwright")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--test-share", type=float, default=0.4, help="share of tasks held out, the same share of every ten in order")
    ap.add_argument("--learn-from-all", action="store_true",
                    help="give every run a reward of 1, keeping the verdict as `verified`, so that `learn` takes every "
                         "run's reads (it learns from successful runs, and few of MCPMark's pass)")
    args = ap.parse_args()
    domain = f"mcpmark_{args.service}"
    (args.out / domain).mkdir(parents=True, exist_ok=True)
    names, tasks = set(), set()
    for config in sorted(p for p in args.runs.iterdir() if p.is_dir() and p.name.endswith(f"__{args.service}")):
        model = config.name.split("__")[0]
        sims = []
        for task_dir in sorted(p for p in config.iterdir() if p.is_dir()):
            if not (task_dir / "messages.json").exists():
                continue
            messages = convert(json.loads((task_dir / "messages.json").read_text()))
            meta = json.loads((task_dir / "meta.json").read_text()) if (task_dir / "meta.json").exists() else {}
            ok = (meta.get("execution_result") or {}).get("success")
            names |= {c["name"] for m in messages for c in m.get("tool_calls") or []}
            tasks.add(task_dir.name)
            reward = {"reward": 1.0, "verified": bool(ok)} if args.learn_from_all else {"reward": 1.0 if ok else 0.0}
            sims.append({"id": f"{task_dir.name}-0", "task_id": task_dir.name, "trial": 0, "messages": messages,
                         "reward_info": reward, "termination_reason": "agent_stop"})
        results = {"info": {"environment_info": {"domain_name": domain}, "agent_info": {"llm": model},
                            "user_info": {"llm": None}, "num_trials": 1},
                   "tasks": [{"id": s["task_id"]} for s in sims], "simulations": sims}
        (args.out / domain / f"{model}.json").write_text(json.dumps(results))
        calls = sum(len(m.get("tool_calls") or []) for s in sims for m in s["messages"])
        print(f"mcpmark_to_tau2: {args.service} {model}: {len(sims)} runs, {calls} calls, "
              f"{sum(bool(s['reward_info'].get('verified', s['reward_info']['reward'])) for s in sims)} verified", file=sys.stderr)
    order = sorted(tasks)
    held = round(10 * args.test_share)
    split = {"train": [t for i, t in enumerate(order) if i % 10 >= held],
             "test": [t for i, t in enumerate(order) if i % 10 < held]}
    checkout = args.out / "checkout"
    (checkout / f"src/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
    (checkout / f"data/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
    lines = [f"# MCPMark's {args.service} tools, marked by their verbs (mcpmark_to_tau2.py).", ""]
    for name in sorted(n for n in names if n.isidentifier()):
        lines += [f"@is_tool(ToolType.{'READ' if is_read(name) else 'WRITE'})", f"def {name}(*args, **kwargs):", "    pass", ""]
    (checkout / f"src/tau2/domains/{domain}/tools.py").write_text("\n".join(lines))
    (checkout / f"data/tau2/domains/{domain}/split_tasks.json").write_text(json.dumps(split))


if __name__ == "__main__":
    main()
