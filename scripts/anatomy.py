#!/usr/bin/env python3
"""Decision anatomy of τ²-bench episodes: what decides each step an agent takes.

For every LLM turn, every decision after a tool returns, and every argument
of every tool call, this says where the step's information came from, which
bounds how much of the agent's work a workflow compiled once from traces
could do without a model.

Arguments are classed after TraceCompiler's binding classes
(arXiv 2608.02680), with its uniqueness rule for copies:

- copy, unique: the value appears in exactly one earlier tool result, at a
  path that holds one value there (a user id, a reservation's cabin);
- copy, select: it appears in one earlier result, among several values at the
  same path (one of a user's orders, one item of an order), so something must
  choose it;
- copy, several: it appears in more than one earlier result;
- customer: it appears only in what the customer wrote;
- constant: every call of the tool gives the argument one value;
- generated: none of these (a closed-set choice, a sum, a date, free text).

A copy the customer also wrote is counted as anchored: the customer's words
pick it.

Decisions after a tool returns (another call, or a reply) are predicted three
ways, fitted on half of the tasks and scored on the other half, and then the
other way round:

- sequence: the likeliest next step after that tool (and its success);
- structure: the best single feature of the tool results so far, such as a
  status field, whether a list is empty, or whether records listed earlier
  are still unread (decision mining's rule at a gateway, one feature deep);
- structure and goal: the same with the episode's goal as a feature, the set
  of writes it makes, known only in hindsight: what the customer asks for.

What none of them predicts takes more of the conversation than its goal.

usage: anatomy.py RESULTS.json... [--json OUT] [--successful]
       anatomy.py --domain airline EPISODES_DIR... (folders of simulation.json)
"""

import argparse
import collections
import json
import re
import sys
from pathlib import Path

READ = {
    "retail": {
        "find_user_id_by_email", "find_user_id_by_name_zip", "get_user_details",
        "get_order_details", "get_product_details", "list_all_product_types",
    },
    "airline": {
        "get_user_details", "get_reservation_details", "get_flight_status",
        "search_direct_flight", "search_onestop_flight", "list_all_airports",
    },
}
PURE = {"calculate", "think"}
HANDOFF = {"transfer_to_human_agents"}
CLASSES = ["copy, unique", "copy, select", "customer", "constant", "generated"]


def leaves(x, path="$"):
    """(path, value) of every scalar in a JSON value; list positions kept."""
    if isinstance(x, dict):
        for k, v in x.items():
            yield from leaves(v, f"{path}.{k}")
    elif isinstance(x, list):
        for i, v in enumerate(x):
            yield from leaves(v, f"{path}[{i}]")
    elif x is not None:
        yield path, x


def pattern(path):
    return re.sub(r"\[\d+\]", "[*]", path)


def norm(v):
    if isinstance(v, bool):
        return str(v).lower()
    if isinstance(v, float) and v.is_integer():
        v = int(v)
    return str(v).strip().lower()


def result_leaves(text):
    """A tool result's scalars: its JSON leaves, or its lines if not JSON."""
    try:
        value = json.loads(text)
    except (ValueError, TypeError):
        return [(f"$[{i}]", line.strip()) for i, line in enumerate(text.splitlines()) if line.strip()]
    if isinstance(value, (dict, list)):
        return list(leaves(value))
    return [("$", value)]


def in_text(value, text):
    """Whether the customer wrote this value (words and ids, not bare digits)."""
    v = norm(value)
    if isinstance(value, bool) or len(v) < 3:
        return False
    if re.fullmatch(r"[\d.]+", v):
        return re.search(rf"(?<![\d.]){re.escape(v)}(?![\d.])", text) is not None
    return v in text


def goal(messages, writes):
    return "+".join(sorted({c["name"] for m in messages for c in m.get("tool_calls") or [] if c["name"] in writes})) or "none"


def features(state):
    """Structured features of the tool results so far, for decision mining."""
    f = {}
    last = state["last"]
    if last is not None:
        tool, error, value = last
        f["error"] = error
        if isinstance(value, dict):
            for k, v in value.items():
                if isinstance(v, (str, bool, int, float)) and not isinstance(v, float):
                    if isinstance(v, str) and (len(v) > 24 or re.search(r"\d{3}", v)):
                        continue  # ids and free text say nothing across episodes
                    f[f"{k}={norm(v)}"] = True
                elif isinstance(v, (list, dict)):
                    n = len(v)
                    f[f"len({k})"] = "0" if n == 0 else "1" if n == 1 else "2+"
                    if isinstance(v, dict):
                        for item in v.values():
                            if isinstance(item, dict) and isinstance(item.get("status"), str):
                                f[f"{k}.*.status={norm(item['status'])}"] = True
        elif isinstance(value, list):
            f["len($)"] = "0" if not value else "1" if len(value) == 1 else "2+"
    # Records listed earlier and not yet read: what iterating over them needs.
    used = state["used"]
    for (tool, pat), values in state["lists"].items():
        if len(values) > 1:
            f[f"unread {tool}:{pat}"] = any(v not in used for v in values)
    return f


def walk(sim, domain, writes):
    """Turns, calls (with argument classes) and decisions of one episode."""
    messages = sim["messages"]
    customer = ""
    produced = collections.defaultdict(list)  # value -> [(call index, path pattern)]
    width = collections.Counter()  # (call index, path pattern) -> values there
    members = collections.defaultdict(set)  # (call index, path pattern) -> the values
    state = {"last": None, "used": set(), "lists": collections.defaultdict(set)}
    calls, turns, decisions = [], [], []
    pending = {}
    ncall = 0
    g = goal(messages, writes)
    for i, m in enumerate(messages):
        role = m["role"]
        if role == "user":
            customer += "\n" + (m.get("content") or "").lower()
            continue
        if role == "tool":
            cid = m.get("id")
            if cid not in pending:
                continue
            idx, tool = pending.pop(cid)
            text = m.get("content") or ""
            error = bool(m.get("error")) or text.startswith("Error")
            items = [] if error else result_leaves(text)
            for path, v in items:
                produced[norm(v)].append((idx, pattern(path)))
                width[(idx, pattern(path))] += 1
                members[(idx, pattern(path))].add(norm(v))
            for path, v in items:
                p = pattern(path)
                if p.endswith("[*]") and width[(idx, p)] > 1 and isinstance(v, str):
                    state["lists"][(tool, p)].add(norm(v))
            try:
                value = json.loads(text) if not error else None
            except (ValueError, TypeError):
                value = text
            state["last"] = (tool, error, value)
            # The agent's next move after the last result of a turn is a decision.
            nxt = messages[i + 1] if i + 1 < len(messages) else None
            if nxt is not None and nxt["role"] == "assistant":
                action = (nxt.get("tool_calls") or [{"name": "reply"}])[0]["name"]
                decisions.append({
                    "site": tool + ("!" if error else ""),
                    "action": action,
                    "features": features(state),
                    "goal": g,
                })
            continue
        if role != "assistant":
            continue
        tcs = m.get("tool_calls") or []
        greeting = i == 0 and not tcs
        if greeting:
            continue
        prev = next((messages[j]["role"] for j in range(i - 1, -1, -1)), None)
        trigger = "tool" if prev == "tool" else "customer"
        if not tcs:
            turns.append({"kind": "reply", "trigger": trigger})
            continue
        turn_calls = []
        for c in tcs:
            name = c["name"]
            kind = "read" if name in READ[domain] else "pure" if name in PURE else "handoff" if name in HANDOFF else "write" if name in writes else "other"
            args = []
            for path, v in leaves(c.get("arguments") or {}):
                occ = produced.get(norm(v), [])
                anchored = in_text(v, customer)
                source = None
                if occ:
                    if any(width[o] == 1 for o in occ):
                        cls = "copy, unique"
                    else:
                        cls = "copy, select"
                        source = max(occ)
                elif anchored:
                    cls = "customer"
                else:
                    cls = "generated"
                args.append({
                    "arg": pattern(path), "class": cls, "value": norm(v), "source": source,
                    "anchored": anchored and bool(occ), "several": len({q for q, _ in occ}) > 1,
                })
                state["used"].add(norm(v))
            call = {"tool": name, "kind": kind, "trigger": trigger, "args": args}
            calls.append(call)
            turn_calls.append(call)
            pending[c.get("id")] = (ncall, name)
            ncall += 1
        turns.append({"kind": "tools", "trigger": trigger, "calls": turn_calls})
    # A selected value is part of an iteration when, by the episode's end, the
    # agent had used every value of the list it came from; otherwise it picked.
    used = {a["value"] for c in calls for a in c["args"]}
    for c in calls:
        for a in c["args"]:
            if a["source"] is not None:
                a["pick"] = not members[a["source"]] <= used
    return turns, calls, decisions


def fit(train, keyfn):
    """Majority action per key, with the site's majority for unseen keys."""
    by_key = collections.defaultdict(collections.Counter)
    by_site = collections.defaultdict(collections.Counter)
    for d in train:
        by_key[(d["site"], keyfn(d))][d["action"]] += 1
        by_site[d["site"]][d["action"]] += 1
    overall = collections.Counter(d["action"] for d in train)

    def predict(d):
        """The likeliest action, its share of the training cases and their number."""
        c = by_key.get((d["site"], keyfn(d))) or by_site.get(d["site"]) or overall
        action, k = c.most_common(1)[0]
        n = sum(c.values())
        return action, k / n, n

    return predict


def halves(ds):
    tasks = sorted({d["task"] for d in ds}, key=lambda x: (len(x), x))
    fold = {t: i % 2 for i, t in enumerate(tasks)}
    return [d for d in ds if fold[d["task"]] == 0], [d for d in ds if fold[d["task"]] == 1]


def one_rule(train, with_goal):
    """Per site, the one feature that best predicts the action, chosen by
    two-fold cross-validation over the training tasks, fitted on all of them."""
    rules = {}
    sites = collections.defaultdict(list)
    for d in train:
        sites[d["site"]].append(d)
    for site, tr in sites.items():
        names = sorted({k for d in tr for k in d["features"]})
        candidates = [lambda d: None]
        candidates += [lambda d, k=k: d["features"].get(k) for k in names]
        if with_goal:
            candidates.append(lambda d: d["goal"])
            candidates += [lambda d, k=k: (d["goal"], d["features"].get(k)) for k in names]
        a, b = halves(tr)
        best, best_score = candidates[0], -1
        if a and b:
            for f in candidates:
                fa, fb = fit(a, f), fit(b, f)
                score = sum(fa(d)[0] == d["action"] for d in b) + sum(fb(d)[0] == d["action"] for d in a)
                if score > best_score:
                    best, best_score = f, score
        rules[site] = fit(tr, best)
    fallback = fit(train, lambda d: None)
    return lambda d: rules.get(d["site"], fallback)(d)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", nargs="+", help="τ²-bench results files, or folders of episodes' simulation.json")
    ap.add_argument("--domain", help="the domain of episode folders (results files name their own)")
    ap.add_argument("--json", help="write the numbers here")
    ap.add_argument("--successful", action="store_true", help="only episodes with reward 1")
    args = ap.parse_args()
    report = {}
    inputs = []
    folders = [Path(p) for p in args.results if Path(p).is_dir()]
    if folders:
        if not args.domain:
            ap.error("episode folders need --domain")
        sims = [json.loads(f.read_text()) for d in folders for f in sorted(d.rglob("simulation.json"))]
        inputs.append(("+".join(str(d) for d in folders), args.domain, sims))
    for path in args.results:
        if Path(path).is_dir():
            continue
        data = json.load(open(path))
        inputs.append((path, data["info"]["environment_info"]["domain_name"], data["simulations"]))
    for path, domain, all_sims in inputs:
        if domain not in READ:
            print(f"anatomy.py: skipping {path}: no read-only tool list for {domain}", file=sys.stderr)
            continue
        writes = set()
        # Every tool the agents called that is not a read, pure or a hand-off is a write.
        sims = [s for s in all_sims if not args.successful or (s.get("reward_info") or {}).get("reward") == 1]
        for s in sims:
            for m in s["messages"]:
                for c in m.get("tool_calls") or []:
                    if c["name"] not in READ[domain] | PURE | HANDOFF:
                        writes.add(c["name"])
        turns, calls, decisions = [], [], []
        for s in sims:
            t, c, d = walk(s, domain, writes)
            for x in d:
                x["task"] = str(s["task_id"])
            turns += t
            calls += c
            decisions += d
        # Constants: an argument that takes one value in every call of its tool.
        values = collections.defaultdict(set)
        counts = collections.Counter()
        for c in calls:
            for a in c["args"]:
                values[(c["tool"], a["arg"])].add(a["value"])
                counts[(c["tool"], a["arg"])] += 1
        for c in calls:
            for a in c["args"]:
                if a["class"] == "generated" and len(values[(c["tool"], a["arg"])]) == 1 and counts[(c["tool"], a["arg"])] >= 5:
                    a["class"] = "constant"
        # Arguments by class, for reads and writes.
        args_by = {}
        for kind in ("read", "write"):
            these = [a for c in calls if c["kind"] == kind for a in c["args"]]
            cnt = collections.Counter(a["class"] for a in these)
            args_by[kind] = {
                "n": len(these), **{k: cnt[k] for k in CLASSES},
                "select, picked": sum(1 for a in these if a.get("pick")),
                "anchored": sum(a["anchored"] for a in these),
                "several results": sum(a["several"] for a in these),
            }
        # Turns: replies; tool turns after the customer; tool turns after a tool,
        # by what their calls need.
        def need(call):
            classes = {a["class"] for a in call["args"]}
            if classes <= {"copy, unique", "constant"}:
                return "bound"
            if classes <= {"copy, unique", "constant", "copy, select"}:
                return "pick" if any(a.get("pick") for a in call["args"]) else "iterate"
            return "customer or generated"
        tb = collections.Counter()
        for t in turns:
            if t["kind"] == "reply":
                tb["reply"] += 1
                continue
            kinds = {c["kind"] for c in t["calls"]}
            if "write" in kinds or "handoff" in kinds:
                tb[f"write, after the {t['trigger']}"] += 1
                continue
            if t["trigger"] == "customer":
                tb["read, after the customer"] += 1
                continue
            worst = max((need(c) for c in t["calls"]), key=["bound", "iterate", "pick", "customer or generated"].index)
            tb[f"read, after a tool: {worst}"] += 1
        # Decisions after a tool returns.
        first, second = halves(decisions)
        for train, test in ((first, second), (second, first)):
            models = {
                "sequence": fit(train, lambda d: None),
                "structure": one_rule(train, with_goal=False),
                "structure and goal": one_rule(train, with_goal=True),
            }
            for d in test:
                d["right"], d["sure"] = {}, {}
                for k, m in models.items():
                    action, share, support = m(d)
                    d["right"][k] = action == d["action"]
                    # A rule a compiled workflow could run on its own: one action
                    # in at least 95% of at least 10 training cases.
                    d["sure"][k] = share >= 0.95 and support >= 10
        n = len(decisions)
        acc = {k: sum(d["right"][k] for d in decisions) for k in ("sequence", "structure", "structure and goal")}
        sure = {}
        for k in acc:
            covered = [d for d in decisions if d["sure"][k]]
            sure[k] = {
                "coverage": round(len(covered) / n, 4) if n else 0,
                "accuracy": round(sum(d["right"][k] for d in covered) / len(covered), 4) if covered else None,
            }
        per_site = {}
        for site, n_site in collections.Counter(d["site"] for d in decisions).most_common(8):
            ds = [d for d in decisions if d["site"] == site]
            per_site[site] = {"n": n_site, **{k: round(sum(d["right"][k] for d in ds) / n_site, 3) for k in acc}}
        report[f"{domain}:{path.rsplit('/', 1)[-1]}"] = {
            "domain": domain,
            "episodes": len(sims),
            "turns": dict(tb),
            "turns_total": sum(tb.values()),
            "arguments": args_by,
            "decisions_after_tool": n,
            "decision_accuracy": {k: round(v / n, 4) for k, v in acc.items()},
            "decision_accuracy_by_site": per_site,
            "decisions_sure": sure,
            "actions_after_tool": dict(collections.Counter(d["action"] for d in decisions).most_common(8)),
        }
    out = json.dumps(report, indent=1)
    if args.json:
        open(args.json, "w").write(out + "\n")
    print(out)


if __name__ == "__main__":
    sys.exit(main())
