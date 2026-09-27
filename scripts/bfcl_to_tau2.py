#!/usr/bin/env python3
"""BFCL's multi-turn tasks as τ²-bench episodes, so stretto's tools read them.

The Berkeley Function Calling Leaderboard's multi-turn categories give, for
each task, the user's turns, the APIs in play (a file system, messaging, a
social network, tickets, trading, travel, a car, math) with their starting
state, and the calls that answer each turn. This runs those calls against
BFCL's own Python backends, turn by turn, and writes each task as one episode
in τ²-bench's results format: the user's message, one assistant turn per call
with what it returned, and a short reply that closes the turn. It also writes
a checkout-shaped folder that stretto reads as τ²-bench's: the tools, each
marked a read or a write, and a train/test split by task.

    bfcl_to_tau2.py --bfcl DIR --out DIR [--categories base long_context] [--test-share 0.4]

`--bfcl` is the folder that holds the `bfcl_eval` package (an unpacked
`bfcl-eval` wheel, or its site-packages). The episodes are ground truth, one
agent that makes exactly the calls each turn needs; the domain is `bfcl`.

With `--runs`, a snapshot of BFCL-Result (github.com/HuanzhiMao/BFCL-Result,
the leaderboard's own outputs: `<date>/result/<model>/multi_turn/...` and
`<date>/score/...`), each of `--models` is written too, as `runs/<model>.json`:
its logged run of every task, each step's calls with what they returned,
scored by the leaderboard's checker (a task it lists as failed has reward 0).
"""

import argparse
import ast
import copy
import importlib
import inspect
import json
import sys
from pathlib import Path

MODULES = {
    "GorillaFileSystem": "gorilla_file_system",
    "MathAPI": "math_api",
    "MessageAPI": "message_api",
    "TwitterAPI": "posting_api",
    "TicketAPI": "ticket_api",
    "TradingBot": "trading_bot",
    "TravelAPI": "travel_booking",
    "VehicleControlAPI": "vehicle_control",
}
STATELESS = {"MathAPI"}

# Which calls change the state: every other call only reads it. `cd` moves the
# file system's working directory, so it is a write: what `ls` returns after it
# differs. Logins change what later calls may do.
WRITES = {
    # file system
    "cd", "cp", "echo", "mkdir", "mv", "rm", "rmdir", "touch",
    # messaging
    "add_contact", "delete_message", "message_login", "send_message",
    # social network
    "authenticate_twitter", "comment", "follow_user", "mention", "post_tweet", "retweet", "unfollow_user",
    # tickets
    "close_ticket", "create_ticket", "edit_ticket", "logout", "resolve_ticket", "ticket_login",
    # trading
    "add_to_watchlist", "cancel_order", "fund_account", "place_order", "remove_stock_from_watchlist",
    "trading_login", "trading_logout", "withdraw_funds",
    # travel
    "authenticate_travel", "book_flight", "cancel_booking", "contact_customer_support", "purchase_insurance",
    "register_credit_card", "set_budget_limit",
    # car
    "activateParkingBrake", "adjustClimateControl", "fillFuelTank", "lockDoors", "pressBrakePedal",
    "releaseBrakePedal", "setCruiseControl", "setHeadlights", "set_navigation", "startEngine",
}


def parse_call(text: str) -> tuple[str, list, dict]:
    """A call as BFCL writes it, `f(a=1, b='x')`, as its name, positional and
    keyword arguments, read as literals (never evaluated)."""
    node = ast.parse(text.strip(), mode="eval").body
    if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name):
        raise ValueError(f"not a call: {text}")
    args = [ast.literal_eval(a) for a in node.args]
    kwargs = {k.arg: ast.literal_eval(k.value) for k in node.keywords}
    return node.func.id, args, kwargs


def as_result(value) -> str:
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value)
    except (TypeError, ValueError):
        return str(value)


def run_task(entry: dict, answer: dict, prefix: str) -> list[dict]:
    """The task's episode: τ²-bench's messages for the ground-truth calls."""
    instances, methods = {}, {}
    for cls in entry["involved_classes"]:
        module = importlib.import_module(f"{prefix}.{MODULES[cls]}")
        inst = getattr(module, cls)()
        if cls not in STATELESS:
            inst._load_scenario(copy.deepcopy(entry["initial_config"].get(cls, {})), long_context="long_context" in entry["id"])
        instances[cls] = inst
        for name, method in inspect.getmembers(inst, predicate=inspect.ismethod):
            if not name.startswith("_"):
                methods[name] = method
    messages, n = [], 0
    for turn, calls in zip(entry["question"], answer["ground_truth"]):
        for m in turn:
            if m["role"] == "user":
                messages.append({"role": "user", "content": m["content"]})
        for text in calls:
            name, args, kwargs = parse_call(text)
            method = methods[name]
            # Positional arguments by the method's own parameter names.
            params = [p for p in inspect.signature(method).parameters]
            kwargs = {**dict(zip(params, args)), **kwargs}
            try:
                out, error = as_result(method(**kwargs)), False
            except Exception as e:  # noqa: BLE001 (a backend's own error is its result)
                out, error = f"Error during execution: {e}", True
            if isinstance(out, str) and out.startswith("{") and '"error"' in out:
                error = True
            n += 1
            cid = f"call_{n}"
            messages.append({"role": "assistant", "content": None,
                             "tool_calls": [{"id": cid, "name": name, "arguments": kwargs, "requestor": "assistant"}]})
            messages.append({"role": "tool", "id": cid, "content": out, "requestor": "assistant", "error": error})
        messages.append({"role": "assistant", "content": "Done."})
    return messages


def logged(record: dict) -> list[dict]:
    """τ²-bench's messages for a model's logged run of a task (BFCL-Result's
    `inference_log`): each turn's user message, then each step's calls with
    what they returned, in order, or the step's reply."""
    messages, n = [], 0
    for turn in record["inference_log"]:
        if not isinstance(turn, dict):
            continue  # the state at the turn's start
        for m in turn.get("begin_of_turn_query") or []:
            if m.get("role") == "user":
                content = m.get("content") or ""
                if isinstance(content, list):  # some handlers log content blocks
                    content = "".join(b.get("text") or "" if isinstance(b, dict) else str(b) for b in content)
                messages.append({"role": "user", "content": content})
        for step in sorted((k for k in turn if k.startswith("step_")), key=lambda k: int(k[5:])):
            results = [m for m in turn[step] if m.get("role") == "tool"]
            for m in turn[step]:
                if m.get("role") != "assistant":
                    continue
                content = m.get("content")
                if not (isinstance(content, list) and content and all(isinstance(c, dict) for c in content)):
                    messages.append({"role": "assistant", "content": content if isinstance(content, str) else json.dumps(content)})
                    continue
                calls = []
                for c in content:
                    for name, args in c.items():
                        n += 1
                        try:
                            arguments = json.loads(args) if isinstance(args, str) else args or {}
                        except json.JSONDecodeError:
                            arguments = {"_unparsed": args}
                        calls.append({"id": f"call_{n}", "name": name, "arguments": arguments, "requestor": "assistant"})
                messages.append({"role": "assistant", "content": None, "tool_calls": calls})
                for call, r in zip(calls, results):
                    out = as_result(r.get("content"))
                    messages.append({"role": "tool", "id": call["id"], "content": out, "requestor": "assistant",
                                     "error": out.startswith("Error") or (out.startswith("{") and '"error"' in out)})
    return messages


def convert_runs(snapshot: Path, models: list[str], categories: list[str], out: Path) -> None:
    """Each model's logged runs, as τ²-bench results, scored by the leaderboard."""
    (out / "runs").mkdir(parents=True, exist_ok=True)
    for model in models:
        sims, skipped = [], 0
        for cat in categories:
            name = f"BFCL_v4_multi_turn_{cat}"
            results = (snapshot / "result" / model / "multi_turn" / f"{name}_result.json").read_text().split("\n")
            scores = (snapshot / "score" / model / "multi_turn" / f"{name}_score.json").read_text().split("\n")
            failed = {json.loads(line)["id"] for line in scores[1:] if line.strip()}
            # One JSON object a line; split on newlines only (a string may hold U+2028).
            for line in filter(str.strip, results):
                record = json.loads(line)
                if "inference_log" not in record:
                    skipped += 1  # the run raised before it began
                    continue
                sims.append({"id": f"{record['id']}-0", "task_id": record["id"], "trial": 0, "messages": logged(record),
                             "reward_info": {"reward": 0.0 if record["id"] in failed else 1.0},
                             "termination_reason": "user_stop"})
        results = {"info": {"environment_info": {"domain_name": "bfcl"}, "agent_info": {"llm": model},
                            "user_info": {"llm": None}, "num_trials": 1},
                   "tasks": [{"id": s["task_id"]} for s in sims], "simulations": sims}
        (out / "runs" / f"{model}.json").write_text(json.dumps(results))
        print(f"bfcl_to_tau2: {model}: {len(sims)} runs ({skipped} without a log), "
              f"{sum(s['reward_info']['reward'] for s in sims):.0f} passed")


def tool_stubs(prefix: str) -> str:
    """tools.py in τ²-bench's form: each tool a stub marked a read or a write."""
    lines = ["# BFCL's multi-turn tools, marked as τ²-bench marks its own (written by bfcl_to_tau2.py).", ""]
    seen = set()
    for cls, mod in MODULES.items():
        module = importlib.import_module(f"{prefix}.{mod}")
        inst = getattr(module, cls)()
        for name, method in inspect.getmembers(inst, predicate=inspect.ismethod):
            if name.startswith("_") or name in seen:
                continue
            seen.add(name)
            kind = "WRITE" if name in WRITES else "READ"
            doc = (inspect.getdoc(method) or "").split("\n")[0].replace('"""', "'")
            lines += [f"@is_tool(ToolType.{kind})", f"def {name}(*args, **kwargs):", f'    """{doc}"""', ""]
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--bfcl", type=Path, required=True, help="the folder holding the bfcl_eval package")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--categories", nargs="+", default=["base"], help="multi-turn categories: base long_context miss_func miss_param")
    ap.add_argument("--test-share", type=float, default=0.4, help="share of tasks held out, the same share of every ten in order")
    ap.add_argument("--runs", type=Path, help="a BFCL-Result snapshot folder (its result/ and score/): also convert --models' runs")
    ap.add_argument("--models", nargs="+", default=[], help="with --runs: the models to convert")
    args = ap.parse_args()
    sys.path.insert(0, str(args.bfcl))
    prefix = "bfcl_eval.eval_checker.multi_turn_eval.func_source_code"
    data = args.bfcl / "bfcl_eval/data"
    sims, tasks, failed = [], [], 0
    for cat in args.categories:
        name = f"BFCL_v4_multi_turn_{cat}.json"
        entries = [json.loads(line) for line in (data / name).read_text().split("\n") if line.strip()]
        answers = {a["id"]: a for a in map(json.loads, filter(str.strip, (data / "possible_answer" / name).read_text().split("\n")))}
        for entry in entries:
            try:
                messages = run_task(entry, answers[entry["id"]], prefix)
            except Exception as e:  # noqa: BLE001
                failed += 1
                print(f"bfcl_to_tau2: {entry['id']}: {e}", file=sys.stderr)
                continue
            tasks.append({"id": entry["id"]})
            sims.append({"id": f"{entry['id']}-0", "task_id": entry["id"], "trial": 0, "messages": messages,
                         "reward_info": {"reward": 1.0}, "termination_reason": "user_stop"})
    held = round(10 * args.test_share)
    test = [t["id"] for i, t in enumerate(tasks) if i % 10 < held]
    train = [t["id"] for i, t in enumerate(tasks) if i % 10 >= held]
    results = {"info": {"environment_info": {"domain_name": "bfcl"}, "agent_info": {"llm": "bfcl-ground-truth"},
                        "user_info": {"llm": None}, "num_trials": 1},
               "tasks": tasks, "simulations": sims}
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "bfcl_ground_truth.json").write_text(json.dumps(results))
    checkout = args.out / "checkout"
    (checkout / "src/tau2/domains/bfcl").mkdir(parents=True, exist_ok=True)
    (checkout / "data/tau2/domains/bfcl").mkdir(parents=True, exist_ok=True)
    (checkout / "src/tau2/domains/bfcl/tools.py").write_text(tool_stubs(prefix))
    (checkout / "data/tau2/domains/bfcl/split_tasks.json").write_text(json.dumps({"train": train, "test": test}))
    print(f"bfcl_to_tau2: {len(sims)} episodes ({failed} failed), {len(train)} train / {len(test)} test tasks, "
          f"{sum(1 for s in sims for m in s['messages'] if m.get('tool_calls'))} calls")
    if args.runs:
        convert_runs(args.runs, args.models, args.categories, args.out)


if __name__ == "__main__":
    main()
