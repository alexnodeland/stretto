"""How well stretto infers an agent's LLM turns from an MCP proxy's log,
against the host's own record of them (docs/results/turns-2026-09-28.md;
stretto #40).

    python3 scripts/turn_inference.py EPISODES [--json rows.json] [--check-against TSV]

EPISODES is a folder of task folders laid out as the live archives lay them
out (docs/results/live-benchmarks-2026-09-27-episodes.tar.gz). Each has the
proxy's session log (`log/*.jsonl`) and Claude Code's event stream
(`events.jsonl`), whose assistant messages hold each LLM turn's tool calls:
the truth. A task folder without both, or whose log's calls are not the
event stream's MCP calls in order, is left out.

The rule is `stretto_trace::mcp::episode`'s, over the agent's calls (the
proxy's own lookups left out, as the console counts turns). A call joins the
current turn while an earlier call of it awaits its response. Otherwise it
joins when it comes within GAP ms of the turn's last response, with nothing
said in between, and, with the dependency check, passes no value (of two
characters or more) that first appeared in what the turn returned. Any other
call starts a turn. `infer` below mirrors the Rust call for call;
`--check-against` compares it with a file of the Rust's turns, one line per
log: its path, a tab, and each call's turn, comma-separated.

Each rule is scored on the LLM turns of several calls that it splits, and
on each pair of consecutive calls: a pair of one LLM turn that it splits,
and a pair of two turns that it merges.
"""

import argparse
import glob
import json
import os
from collections import Counter

# stretto_trace::mcp::SAME_TURN_MS
SAME_TURN_MS = 500
RULES = [("overlap only", None, False)] + [
    (f"within {gap} ms{', unless it needs what the turn returned' if dep else ''}", gap, dep)
    for gap in (100, 250, 500, 1000, 2000)
    for dep in (False, True)
]


def compact(value) -> str:
    """serde_json's `to_string`: compact, keys sorted, non-ASCII as is."""
    return json.dumps(value, separators=(",", ":"), sort_keys=True, ensure_ascii=False)


def scalars(value, out: list) -> None:
    if isinstance(value, dict):
        for v in value.values():
            scalars(v, out)
    elif isinstance(value, list):
        for v in value:
            scalars(v, out)
    elif isinstance(value, bool) or value is None:
        return
    else:
        out.append(value if isinstance(value, str) else compact(value))


def uses_new_values(arguments, returned: str, known: str) -> bool:
    values: list = []
    scalars(arguments, values)
    return any(len(v) >= 2 and v in returned and v not in known for v in values)


def outcome(response: dict) -> str:
    """A response's content as text, as `stretto_trace::mcp` reads it."""
    error = response.get("error")
    if error is not None:
        message = error.get("message") if isinstance(error, dict) else None
        return message if isinstance(message, str) else compact(error)
    result = response.get("result")
    items = result.get("content") if isinstance(result, dict) else None
    texts = [i["text"] for i in items or [] if isinstance(i, dict) and i.get("type") == "text"
             and isinstance(i.get("text"), str)] if isinstance(items, list) else []
    if texts:
        return "\n".join(texts)
    if isinstance(result, dict) and "structuredContent" in result:
        return compact(result["structuredContent"])
    return compact(result)


def messages(log: str):
    """The log's messages as (t_ms, sender, message), batches flattened."""
    with open(log) as f:
        next(f)  # the header
        for line in f:
            entry = json.loads(line)
            m = entry.get("message")
            for item in m if isinstance(m, list) else [m] if m is not None else []:
                if isinstance(item, dict):
                    yield entry["t_ms"], entry["from"], item


def infer(log: str, gap: int | None = SAME_TURN_MS, dependency: bool = True) -> tuple[list, list]:
    """Each of the agent's calls in the order sent, as (sent at, name,
    arguments, answered at), and its turn: the index of the turn's event, as
    `stretto_trace::mcp::episode` places it. A call is answered by its first
    response, or never (None)."""
    events: list[str] = []
    turn, open_calls, answered_ms = 0, 0, None
    pending: dict[str, bool] = {}  # id as JSON: whether it holds its turn open
    index: dict[str, int] = {}  # id as JSON: the call's place in `calls`
    returned = known = ""
    calls, turns = [], []
    for t, sender, m in messages(log):
        method = m.get("method") if isinstance(m.get("method"), str) else None
        if sender == "context":
            text = m.get("content") if isinstance(m.get("content"), str) else ""
            known += text
            if m.get("role") == "user" or (m.get("role") == "assistant" and text):
                events.append("said")
        elif sender == "proxy" and method is not None:
            continue  # the flow's lookups: not the agent's calls
        elif sender == "client" and method == "tools/call":
            params = m.get("params") if isinstance(m.get("params"), dict) else {}
            name, id_ = params.get("name"), m.get("id")
            if id_ is None or not isinstance(name, str):
                continue
            if pending.pop(compact(id_), False):
                open_calls -= 1
            arguments = params["arguments"] if "arguments" in params else {}
            continues = (
                gap is not None
                and answered_ms is not None
                and t - answered_ms <= gap
                and all(e == "result" for e in events[turn + 1:])
                and not (dependency and uses_new_values(arguments, returned, known))
            )
            if open_calls == 0 and not continues:
                known += returned
                returned, answered_ms = "", None
                events.append("turn")
                turn = len(events) - 1
            known += compact(arguments)
            index[compact(id_)] = len(calls)
            calls.append([t, name, arguments, None])
            turns.append(turn)
            open_calls += 1
            pending[compact(id_)] = True
        elif sender == "client" and method == "notifications/cancelled":
            params = m.get("params") if isinstance(m.get("params"), dict) else {}
            key = compact(params.get("requestId"))
            if pending.get(key):
                pending[key] = False
                open_calls -= 1
        elif sender in ("server", "proxy") and "method" not in m:
            id_ = m.get("id")
            answers = "result" in m or m.get("error") is not None
            if id_ is None or not answers or compact(id_) not in pending:
                continue
            calls[index[compact(id_)]][3] = t
            if pending.pop(compact(id_)):
                open_calls -= 1
                if open_calls == 0:
                    answered_ms = t
            returned += outcome(m)
            events.append("result")
    return calls, turns


def truth(folder: str, calls: list) -> list | None:
    """Each call's LLM turn, by Claude Code's message id; None when the event
    stream's MCP calls are not the log's."""
    record = []
    with open(os.path.join(folder, "events.jsonl")) as f:
        for line in f:
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            if e.get("type") != "assistant":
                continue
            message = e.get("message") or {}
            for c in message.get("content") or []:
                if c.get("type") == "tool_use" and c.get("name", "").startswith("mcp__"):
                    record.append((message.get("id"), c["name"].split("__", 2)[2]))
    if [n for _, n in record] != [c[1] for c in calls]:
        return None
    return [turn for turn, _ in record]


def gaps(log: str, turns: list) -> list[tuple[int, bool]]:
    """For each call sent when no call of its predecessor's turn (by overlap
    alone) awaited a response: how long after that turn's last response it
    came, and whether it was in fact the same LLM turn."""
    calls, overlap = infer(log, None, False)
    never = float("inf")
    out = []
    for i in range(1, len(calls)):
        members = [c for c, turn in zip(calls[:i], overlap) if turn == overlap[i - 1]]
        t = calls[i][0]
        if any((c[3] if c[3] is not None else never) > t for c in members):
            continue
        out.append((t - max(c[3] for c in members), turns[i] == turns[i - 1]))
    return out


def quantile(xs: list, p: float):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("episodes", help="a folder of task folders, as the live archives hold them")
    ap.add_argument("--json", help="write each rule's counts and the gaps here")
    ap.add_argument("--check-against", help="the Rust's turns, one log per line: path, a tab, each call's turn")
    args = ap.parse_args()

    sessions, left_out = [], 0
    for log_dir in sorted(glob.glob(os.path.join(args.episodes, "**", "log"), recursive=True)):
        folder = os.path.dirname(log_dir)
        logs = glob.glob(os.path.join(log_dir, "*.jsonl"))
        if len(logs) != 1 or not os.path.exists(os.path.join(folder, "events.jsonl")):
            left_out += 1
            continue
        calls, _ = infer(logs[0], None, False)
        record = truth(folder, calls)
        if record is None:
            left_out += 1
            continue
        sessions.append((folder, logs[0], record))
    several = sum(sum(1 for r in set(record) if record.count(r) > 1) for *_, record in sessions)
    print(f"{len(sessions)} sessions, {sum(len(r) for *_, r in sessions)} calls in "
          f"{sum(len(set(r)) for *_, r in sessions)} LLM turns, {several} of them with several calls; "
          f"{left_out} task folder(s) left out")

    rows = {}
    print(f"\n{'rule':<62} {'turns':>6} {'turns split':>12} {'pairs split':>12} {'merged':>7}")
    for name, gap, dep in RULES:
        c = Counter(turns=0, turns_split=0, split=0, merged=0)
        for _, log, record in sessions:
            _, inferred = infer(log, gap, dep)
            c["turns"] += len(set(inferred))
            c["turns_split"] += sum(1 for r in set(record)
                                    if len({inferred[i] for i, x in enumerate(record) if x == r}) > 1)
            for i in range(1, len(record)):
                if record[i] == record[i - 1] and inferred[i] != inferred[i - 1]:
                    c["split"] += 1
                elif record[i] != record[i - 1] and inferred[i] == inferred[i - 1]:
                    c["merged"] += 1
        rows[name] = dict(c)
        print(f"{name:<62} {c['turns']:>6} {c['turns_split']:>12} {c['split']:>12} {c['merged']:>7}")

    same, new = [], []
    for _, log, record in sessions:
        for g, is_same in gaps(log, record):
            (same if is_same else new).append(g)
    print(f"\nafter the turn's last response, with nothing awaiting one:")
    print(f"  the same LLM turn: {len(same)} calls, median {quantile(same, .5)} ms, "
          f"90% within {quantile(same, .9)} ms, the longest {max(same)} ms")
    print(f"  a new LLM turn:    {len(new)} calls, the soonest {min(new)} ms, "
          f"1% within {quantile(new, .01)} ms, median {quantile(new, .5)} ms")
    for g in (250, 500, 750, 1000):
        print(f"  within {g} ms: {sum(x <= g for x in same)} of the same turn, {sum(x <= g for x in new)} of a new one")

    if args.check_against:
        rust = {}
        with open(args.check_against) as f:
            for line in f:
                path, _, ids = line.rstrip("\n").partition("\t")
                rust[os.path.realpath(path)] = [int(x) for x in ids.split(",")] if ids else []
        differ = [folder for folder, log, _ in sessions if rust.get(os.path.realpath(log)) != infer(log)[1]]
        print(f"\nagainst the Rust: {len(sessions) - len(differ)} of {len(sessions)} sessions the same, call for call")
        for folder in differ:
            print(f"  differs: {os.path.relpath(folder, args.episodes)}")

    if args.json:
        with open(args.json, "w") as f:
            json.dump({"sessions": len(sessions), "calls": sum(len(r) for *_, r in sessions),
                       "turns": sum(len(set(r)) for *_, r in sessions), "turns_with_several_calls": several,
                       "rules": rows,
                       "gaps": {"same_turn": sorted(same), "new_turn": sorted(new)}}, f, indent=1)
            f.write("\n")


if __name__ == "__main__":
    main()
