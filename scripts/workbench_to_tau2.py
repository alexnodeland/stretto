#!/usr/bin/env python3
"""WorkBench's published runs as τ²-bench results, for trace-only replays.

WorkBench (Styles et al. 2024) gives an agent one request in a simulated
workplace (email, calendar, web analytics, a CRM, project management and a
company directory) and no user after it; its repository publishes each
model's run of each of its 690 tasks (`data/results/<domain>/<model>_all_<time>.csv.gz`,
the agent given every tool), the calls with what they returned, and whether
the run left the right outcome (`item_level_results.csv.gz`). This writes each
model's runs of each domain as τ²-bench's results file, and a checkout-shaped
folder that stretto reads as τ²-bench's: the tools, marked a read or a write,
and a train/test split by task.

    workbench_to_tau2.py --results WorkBench/data/results --out DIR [--run-group revisited_2026]

A run's steps are its calls; the calls one step of the model made together
(the same input) are one turn, and the step the harness logs as a `Final
Answer` action is the reply. Tool names are written as the agent saw them
(`email_search_emails`); a name that is none of WorkBench's tools is kept as
the call the model made, and is neither a read nor a write.
"""

import argparse
import ast
import csv
import gzip
import json
import sys
from pathlib import Path

DOMAINS = ["email", "calendar", "analytics", "customer_relationship_manager", "project_management", "multi_domain"]
# WorkBench's tools; a call of any other name (models invent some) is an error, never a read.
TOOLS = {
    "analytics_create_plot", "analytics_engaged_users_count", "analytics_get_average_session_duration",
    "analytics_get_visitor_information_by_id", "analytics_total_visits_count", "analytics_traffic_source_count",
    "calendar_create_event", "calendar_delete_event", "calendar_get_event_information_by_id",
    "calendar_search_events", "calendar_update_event", "company_directory_find_email_address",
    "customer_relationship_manager_add_customer", "customer_relationship_manager_delete_customer",
    "customer_relationship_manager_search_customers", "customer_relationship_manager_update_customer",
    "email_delete_email", "email_forward_email", "email_get_email_information_by_id", "email_reply_email",
    "email_search_emails", "email_send_email", "project_management_create_task", "project_management_delete_task",
    "project_management_get_task_information_by_id", "project_management_search_tasks",
    "project_management_update_task",
}
# Every call that changes the state; the rest (searches, lookups by id, counts, the directory) only read.
WRITES = {
    "email_send_email", "email_delete_email", "email_forward_email", "email_reply_email",
    "calendar_create_event", "calendar_delete_event", "calendar_update_event",
    "analytics_create_plot",
    "customer_relationship_manager_add_customer", "customer_relationship_manager_delete_customer",
    "customer_relationship_manager_update_customer",
    "project_management_create_task", "project_management_delete_task", "project_management_update_task",
}


def value(node):
    """A literal of the repr, with the harness's own records (`AgentResult(...)`,
    `TraceStep(...)`) as dicts of their fields; nothing is evaluated."""
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
        return {k.arg: value(k.value) for k in node.keywords}
    if isinstance(node, (ast.List, ast.Tuple)):
        return [value(e) for e in node.elts]
    if isinstance(node, ast.Dict):
        return {value(k): value(v) for k, v in zip(node.keys, node.values)}
    if isinstance(node, ast.Name):
        return node.id  # a bare `nan` in a repr
    return ast.literal_eval(node)


def as_json(text: str) -> str:
    """A result as strict JSON: pandas writes a missing value as `NaN`,
    which JSON has no word for; it becomes null."""
    try:
        value = json.loads(text)
    except ValueError:
        return text
    if "NaN" not in text:
        return text

    def clean(v):
        if isinstance(v, float) and v != v:
            return None
        if isinstance(v, list):
            return [clean(x) for x in v]
        if isinstance(v, dict):
            return {k: clean(x) for k, x in v.items()}
        return v

    return json.dumps(clean(value))


def convert(task: str, response: str) -> list:
    """τ²-bench's messages for one run: the request, each turn's calls with
    what they returned, and the final reply."""
    result = value(ast.parse(response, mode="eval").body)
    messages = [{"role": "user", "content": task}]
    turn, last, n = [], None, 0

    def close():
        if turn:
            messages.append({"role": "assistant", "content": None, "tool_calls": [c for c, _ in turn]})
            for call, out in turn:
                messages.append({"role": "tool", "id": call["id"], "content": out, "requestor": "assistant",
                                 "error": out.lower().startswith("error") or out.startswith("Tool ")})
            turn.clear()

    final = None
    for step in result.get("trace") or []:
        if step.get("action") == "Final Answer":
            # The harness logs the reply as an action; it is the reply, not a call.
            close()
            final = final or step.get("action_input")
            continue
        if step.get("llm_input") != last:
            close()
        last = step.get("llm_input")
        raw = step.get("action_input") or "{}"
        try:
            arguments = ast.literal_eval(raw) if isinstance(raw, str) else raw
        except (ValueError, SyntaxError):
            arguments = {"_unparsed": raw}
        n += 1
        call = {"id": f"call_{n}", "name": str(step.get("action") or "").replace(".", "_"),
                "arguments": arguments if isinstance(arguments, dict) else {"_value": arguments},
                "requestor": "assistant"}
        turn.append((call, as_json(str(step.get("observation") if step.get("observation") is not None else ""))))
    close()
    reply = result.get("output") or final
    if reply:
        messages.append({"role": "assistant", "content": str(reply)})
    return messages


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--results", type=Path, required=True, help="WorkBench's data/results folder")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--run-group", default="revisited_2026", help="the runs to take, as item_level_results names them")
    ap.add_argument("--test-share", type=float, default=0.4, help="share of tasks held out, the same share of every ten in order")
    args = ap.parse_args()
    csv.field_size_limit(10**9)
    items = [r for r in csv.DictReader(gzip.open(args.results / "item_level_results.csv.gz", "rt"))
             if r["run_group"] == args.run_group and r["tool_selection"] == "all"]
    by_file: dict[str, list[dict]] = {}
    for r in items:
        by_file.setdefault(r["results_file"], []).append(r)
    names: dict[str, set[str]] = {d: set() for d in DOMAINS}
    for domain in DOMAINS:
        (args.out / domain).mkdir(parents=True, exist_ok=True)
        tasks: list[str] = []
        for results_file, rows in sorted(by_file.items()):
            if rows[0]["domain"] != domain:
                continue
            model = rows[0]["model"]
            runs = list(csv.DictReader(gzip.open(args.results.parent.parent / results_file, "rt")))
            if len(runs) != len(rows) or any(a["task"] != b["task"] for a, b in zip(runs, rows)):
                print(f"workbench_to_tau2: {results_file}: runs and scores disagree, skipped", file=sys.stderr)
                continue
            sims, bad = [], 0
            for run, row in zip(runs, rows):
                try:
                    messages = convert(run["task"], run["full_response"])
                except (ValueError, SyntaxError):
                    bad += 1  # the run left no response (it raised)
                    continue
                names[domain] |= {c["name"] for m in messages for c in m.get("tool_calls") or []}
                sims.append({"id": f"{row['task_id']}-0", "task_id": row["task_id"], "trial": 0, "messages": messages,
                             "reward_info": {"reward": 1.0 if row["correct"] == "True" else 0.0},
                             "termination_reason": "agent_stop"})
            tasks = [r["task_id"] for r in rows]
            results = {"info": {"environment_info": {"domain_name": domain}, "agent_info": {"llm": model},
                                "user_info": {"llm": None}, "num_trials": 1},
                       "tasks": [{"id": t} for t in tasks], "simulations": sims}
            (args.out / domain / f"{model}.json").write_text(json.dumps(results))
            print(f"workbench_to_tau2: {domain} {model}: {len(sims)} runs ({bad} without a response), "
                  f"{sum(s['reward_info']['reward'] for s in sims):.0f} correct", file=sys.stderr)
        held = round(10 * args.test_share)
        split = {"train": [t for i, t in enumerate(tasks) if i % 10 >= held],
                 "test": [t for i, t in enumerate(tasks) if i % 10 < held]}
        checkout = args.out / "checkout"
        (checkout / f"src/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
        (checkout / f"data/tau2/domains/{domain}").mkdir(parents=True, exist_ok=True)
        lines = [f"# WorkBench's tools in its {domain} tasks, marked as τ²-bench marks its own (workbench_to_tau2.py).", ""]
        for name in sorted(names[domain] & TOOLS):
            lines += [f"@is_tool(ToolType.{'WRITE' if name in WRITES else 'READ'})", f"def {name}(*args, **kwargs):", "    pass", ""]
        (checkout / f"src/tau2/domains/{domain}/tools.py").write_text("\n".join(lines))
        (checkout / f"data/tau2/domains/{domain}/split_tasks.json").write_text(json.dumps(split))


if __name__ == "__main__":
    main()
