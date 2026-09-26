#!/usr/bin/env python3
"""A workflow compiled once from agents' traces, run with no model on
τ²-bench telecom in solo mode, where the agent operates the phone itself.

The workflow is what `anatomy.py` fits to predict decisions: per site (the
tool that just returned, or the start), a decision tree over the whole state
(every tool's last result, which tools were called) and the ticket's words.
Here its actions are whole calls, a tool with its arguments, since telecom's
arguments come from a closed set (the base tasks share one customer), or
stopping. It is fitted on the successful episodes of the training tasks, and
run on each test task in τ²-bench's own environment: from the task's initial
state, call the tree's call, append its result to the state, and repeat
until the tree says stop (or 50 calls). τ²-bench's evaluator scores the run
(its environment assertions, and the required actions where the task asks).

With --sure, the workflow runs only while its leaf is sure (one action in at
least 95% of at least 10 training cases, or the share given) and hands back otherwise: the share
of test episodes it finishes alone, and how many of those pass.

With --self-train ROUNDS, the workflow then learns from its own runs, as
expert iteration and rejection-sampling fine-tuning do, with no model: each
round it runs on every training task's ticket, once as it stands and
--rollouts times drawing each call from its leaf's calls by their counts, and
keeps, per task, the shortest run its own check of the ticket's criterion
says resolved (a transfer is not kept). It is refitted on the demonstrations
and those runs, and scored on the test tasks after each round.

With --symbolic, an action's identifiers (arguments of at least four
characters with a digit: ids, phone numbers) are learned as where they came
from, the path of an earlier result or the ticket, and bound again when the
workflow runs; with --rename, the test tasks run for a customer none of the
traces saw: John Smith's name, ids and phone numbers renamed throughout
τ²-bench's database, the phone's and each task. With --as-customer ID they
run for another customer of the database, whose first line takes John's
line's state, keeping the tasks whose own gold actions still solve them.

usage: telecom_workflow.py RESULTS.json... [--tau2 DIR] [--sure] [--json OUT]
"""

import argparse
import collections
import hashlib
import json
import random
import re
import sys
from functools import partial
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anatomy  # noqa: E402

STOP = "stop"
MAX_CALLS = 50  # the successful training episodes made at most 38
SYMBOLIC = False  # --symbolic: identifiers as where they came from
RENAME = None  # --rename: a function renaming John Smith's identifiers


def keyish(v):
    return isinstance(v, str) and len(v) >= 4 and re.search(r"\d", v) is not None


def paths(value, target, path="$"):
    """The paths at which `target` is a string of `value`, lists as [*]."""
    if isinstance(value, str):
        return [path] if value == target else []
    if isinstance(value, dict):
        return [p for k, v in value.items() for p in paths(v, target, f"{path}.{k}")]
    if isinstance(value, list):
        return [p for v in value for p in paths(v, target, f"{path}[*]")]
    return []


def at(value, path):
    """The strings of `value` at `path`, in document order."""
    if path in ("", "$"):
        return [value] if isinstance(value, str) else []
    rest = path[1:] if path.startswith("$") else path
    if rest.startswith("[*]"):
        return [x for v in value for x in at(v, "$" + rest[3:])] if isinstance(value, list) else []
    if rest.startswith("."):
        key = re.match(r"\.([^.\[]+)", rest).group(1)
        return at(value.get(key), "$" + rest[1 + len(key):]) if isinstance(value, dict) else []
    return []


def select(value, path):
    """Everything in `value` at `path`, lists as [*]."""
    if path in ("", "$"):
        return [value]
    rest = path[1:] if path.startswith("$") else path
    if rest.startswith("[*]"):
        return [x for v in value for x in select(v, "$" + rest[3:])] if isinstance(value, list) else []
    if rest.startswith("."):
        key = re.match(r"\.([^.\[]+)", rest).group(1)
        return select(value.get(key), "$" + rest[1 + len(key):]) if isinstance(value, dict) else []
    return []


def members(value, path):
    """For a path through a list of records, each value at the path with its
    record: `(value, record)`, in document order."""
    head, _, tail = path.rpartition("[*]")
    return [(v, e) for e in select(value, head + "[*]") for v in at(e, "$" + tail) if isinstance(e, dict)]


def which(record, others):
    """The fields that set a record apart from the others of its list (a
    bill's status): short values, no identifiers."""
    return {k: x for k, x in record.items()
            if isinstance(x, (str, bool)) and not keyish(x) and any(o.get(k) != x for o in others)}


def symbol(v, outputs, ticket):
    r"""Where an identifier came from: the path of the most recent result that
    holds it (with, in a list of records, the fields that set its record
    apart), else the ticket (by its shape), else itself.

    >>> outs = [("get_customer_by_phone", {"customer_id": "C1", "line_ids": ["L1", "L2"]}),
    ...         ("get_bills_for_customer", [{"bill_id": "B1001", "status": "Paid"},
    ...                                     {"bill_id": "B1234321", "status": "Overdue"}])]
    >>> symbol("L2", outs, "")
    '@get_customer_by_phone$.line_ids[*]'
    >>> symbol("B1234321", outs, "")
    '@get_bills_for_customer$[*].bill_id?{"status": "Overdue"}'
    >>> symbol("555-123-2002", outs, "phone number: 555-123-2002")
    '@ticket\\d{3}\\-\\d{3}\\-\\d{4}'
    """
    for tool, value in reversed(outputs):
        found = paths(value, v)
        if found:
            path = found[0]
            if "[*]" in path:
                listed = members(value, path)
                chosen = [e for x, e in listed if x == v]
                if chosen:
                    apart = which(chosen[0], [e for x, e in listed if x != v])
                    if apart:
                        return f"@{tool}{path}?" + json.dumps(apart, sort_keys=True)
            return f"@{tool}{path}"
    if v in ticket:
        return "@ticket" + re.sub(r"(?:\\d)+", lambda m: "\\d{%d}" % (len(m.group()) // 2),
                                  re.sub(r"\d", r"\\d", re.escape(v)))
    return v


def renaming():
    """A function renaming, in any text, the base tasks' customer (John
    Smith): name, email, customer, line, bill and device ids, and phone
    numbers, to ones τ²-bench's database does not hold."""
    from tau2.domains.telecom.data_model import TelecomDB
    from tau2.domains.telecom.environment import TELECOM_DB_PATH

    db = json.loads(TelecomDB.load(TELECOM_DB_PATH).model_dump_json())
    customer = next(c for c in db["customers"] if c["full_name"] == "John Smith")
    lines = [line for line in db["lines"] if line["line_id"] in customer["line_ids"]]
    ids = [customer["customer_id"], *customer["line_ids"], *customer.get("bill_ids", []),
           *[line["device_id"] for line in lines if line.get("device_id")]]
    phones = {customer["phone_number"], *[line["phone_number"] for line in lines]}
    mapping = {i: re.sub(r"\d+", lambda m: str(int(m.group()) + 6000), i) for i in ids}
    mapping.update({ph: re.sub(r"^(\d{3})-\d{3}-\d{2}", r"\1-987-65", ph) for ph in phones})
    mapping.update({"John Smith": "Maria Lopez", "john.smith@example.com": "maria.lopez@example.com"})
    text = json.dumps(db)
    assert not any(new in text for new in mapping.values()), mapping
    pattern = re.compile(r"(?<![\w.-])(" + "|".join(map(re.escape, sorted(mapping, key=len, reverse=True))) + r")(?![\w-])")
    return lambda t: pattern.sub(lambda m: mapping[m.group()], t)


def renamed_environment(rename):
    """τ²-bench's telecom environment with the customer renamed."""
    from tau2.domains.telecom.data_model import TelecomDB
    from tau2.domains.telecom.environment import TELECOM_DB_PATH, TELECOM_USER_DB_PATH, get_environment
    from tau2.domains.telecom.user_data_model import TelecomUserDB

    db = rename(TelecomDB.load(TELECOM_DB_PATH).model_dump_json())
    user = rename(TelecomUserDB.load(TELECOM_USER_DB_PATH).model_dump_json())

    def make(**kw):
        return get_environment(db=TelecomDB.model_validate_json(db), user_db=TelecomUserDB.model_validate_json(user),
                               policy_type="workflow", **kw)

    return make


def as_customer(customer_id):
    """John Smith's tasks moved to another customer of τ²-bench's database:
    his customer and line ids, device, number, name and email in each task
    become theirs, and their first line takes his line's state (active,
    roaming, data used), so that the account differs in its lines, bills,
    device and payment method, not in the faults the task sets. Returns the
    renaming and the environment."""
    from tau2.domains.telecom.data_model import TelecomDB
    from tau2.domains.telecom.environment import TELECOM_DB_PATH, TELECOM_USER_DB_PATH, get_environment
    from tau2.domains.telecom.user_data_model import TelecomUserDB

    db = json.loads(TelecomDB.load(TELECOM_DB_PATH).model_dump_json())
    john = next(c for c in db["customers"] if c["full_name"] == "John Smith")
    other = next(c for c in db["customers"] if c["customer_id"] == customer_id)
    lines = {line["line_id"]: line for line in db["lines"]}
    target = next(lines[i] for i in john["line_ids"] if lines[i]["phone_number"] == john["phone_number"])
    line = lines[other["line_ids"][0]]
    for k in ("status", "roaming_enabled", "data_used_gb", "data_refueling_gb", "suspension_start_date"):
        line[k] = target[k]
    other["account_status"] = john["account_status"]
    mapping = {john["customer_id"]: other["customer_id"], target["line_id"]: line["line_id"],
               target["device_id"]: line["device_id"], target["phone_number"]: line["phone_number"],
               john["full_name"]: other["full_name"], john["email"]: other["email"]}
    pattern = re.compile(r"(?<![\w.-])(" + "|".join(map(re.escape, sorted(mapping, key=len, reverse=True))) + r")(?![\w-])")

    def rename(text):
        return pattern.sub(lambda m: mapping[m.group()], text)

    db_text = json.dumps(db)
    user = rename(TelecomUserDB.load(TELECOM_USER_DB_PATH).model_dump_json())

    def make(**kw):
        return get_environment(db=TelecomDB.model_validate_json(db_text), user_db=TelecomUserDB.model_validate_json(user),
                               policy_type="workflow", **kw)

    return rename, make


def gold_passes(task, constructor):
    """Whether a task's own gold actions, made as the agent's calls, pass
    τ²-bench's evaluator: whether a moved task can still be solved."""
    from tau2.data_model.message import AssistantMessage, ToolCall
    from tau2.data_model.tasks import RewardType
    from tau2.evaluator.evaluator_action import ActionEvaluator
    from tau2.evaluator.evaluator_env import EnvironmentEvaluator

    env = constructor(solo_mode=True)
    init = task.initial_state
    try:
        env.set_state(initialization_data=init.initialization_data if init else None,
                      initialization_actions=init.initialization_actions if init else None, message_history=[])
    except ValueError:
        return False  # the task's setup does not fit the account (a second overdue bill)
    messages = []
    for i, a in enumerate(task.evaluation_criteria.actions or []):
        call = ToolCall(id=f"gold_{i}", name=a.name, arguments=a.arguments, requestor="assistant")
        messages.append(AssistantMessage(role="assistant", content=None, tool_calls=[call]))
        messages.append(env.get_response(call))
    reward = EnvironmentEvaluator.calculate_reward(environment_constructor=constructor, task=task, full_trajectory=messages,
                                                   solo_mode=True, strict_replay=False).reward
    if RewardType.ACTION in task.evaluation_criteria.reward_basis:
        reward *= ActionEvaluator.calculate_reward(task=task, full_trajectory=messages).reward
    return reward == 1


def resolve(sym, outputs, ticket, passed):
    r"""Bind a symbol again: the ticket's first match of its shape, or the most
    recent result of its tool with a value at its path, the first in a list
    not yet passed as this argument.

    >>> outs = [("get_customer_by_phone", {"customer_id": "C7", "line_ids": ["L7", "L8"]}),
    ...         ("get_details_by_id", {"line_id": "L7", "phone_number": "555-0107"}),
    ...         ("get_details_by_id", {"line_id": "L8", "phone_number": "555-0108"}),
    ...         ("get_bills_for_customer", [{"bill_id": "B1", "status": "Paid"},
    ...                                     {"bill_id": "B2", "status": "Overdue"}])]
    >>> resolve("@get_customer_by_phone$.line_ids[*]", outs, "", {"L7"})
    'L8'
    >>> resolve("@get_details_by_id$.line_id", outs, "", set())  # the most recent
    'L8'
    >>> resolve("@get_details_by_id$.line_id", outs, "my number is 555-0107", set())
    'L7'
    >>> resolve('@get_bills_for_customer$[*].bill_id?{"status": "Overdue"}', outs, "", set())
    'B2'
    >>> resolve("@ticket\\d{3}-\\d{4}", outs, "call me at 555-0199", set())
    '555-0199'
    """
    if not (isinstance(sym, str) and sym.startswith("@")):
        return sym
    if sym.startswith("@ticket"):
        m = re.search(sym[len("@ticket"):], ticket)
        return m.group() if m else None
    tool, path = re.match(r"@([^$]+)(\$.*)", sym).groups()
    path, _, apart = path.partition("?")
    apart = json.loads(apart) if apart else {}
    found = []
    for name, value in reversed(outputs):
        if name != tool:
            continue
        values = [v for v in at(value, path) if not ("[*]" in path and v in passed)]
        if apart:
            # The record like the one the demonstrator chose, if one is.
            like = [x for x, e in members(value, path)
                    if x not in passed and all(e.get(k) == want for k, want in apart.items())]
            values = like + [v for v in values if v not in like]
        if values:
            found.append((value, values[0]))
    if not found:
        return None
    # A field of one record: the record that holds what the ticket gives (the
    # line with the customer's number) before the most recent.
    if "[*]" not in path:
        given = {g for g in re.findall(r"[\w.@-]*\d[\w.@-]*", ticket) if keyish(g)}
        for value, v in found:
            if given & set(str(x) for x in anatomy_leaves(value)):
                return v
    return found[0][1]


def anatomy_leaves(value):
    """Every string and number under a result."""
    if isinstance(value, dict):
        return [x for v in value.values() for x in anatomy_leaves(v)]
    if isinstance(value, list):
        return [x for v in value for x in anatomy_leaves(v)]
    return [value] if isinstance(value, (str, int, float)) and not isinstance(value, bool) else []


def label(name, args, outputs=None, ticket=""):
    """A call as an action: its tool and arguments, numbers as integers where
    they are whole. A hand-off's summary is free text and is left out. With
    --symbolic and the results so far, identifiers as where they came from."""
    if name == "transfer_to_human_agents":
        return name
    args = {k: int(v) if isinstance(v, float) and v.is_integer() else v for k, v in (args or {}).items()}
    if SYMBOLIC and outputs is not None:
        args = {k: symbol(v, outputs, ticket) if keyish(v) else v for k, v in args.items()}
    return name + json.dumps(args, sort_keys=True) if args else name


def unlabel(action):
    i = action.find("{")
    if i < 0:
        args = {"summary": "The customer's issue needs a human agent."} if action == "transfer_to_human_agents" else {}
        return action, args
    return action[:i], json.loads(action[i:])


def words(ticket):
    """The ticket's words and word pairs, digits left out."""
    w = re.findall(r"[a-z]+", ticket.lower())
    return set(w) | {f"{a} {b}" for a, b in zip(w, w[1:])}


class State:
    def __init__(self, ticket, vocab):
        self.latest = {}
        self.called = set()
        self.fixes = set()  # the writes made, with their arguments
        self.site = "start"
        self.ticket = {f"ticket: {g}" for g in words(ticket) & vocab}
        self.text = ticket
        self.outputs = []  # (tool, parsed result), for --symbolic

    def result(self, tool, content, error):
        try:
            value = json.loads(content) if not error else None
        except (ValueError, TypeError):
            value = content
        if isinstance(value, str):
            try:
                value = json.loads(value)
            except (ValueError, TypeError):
                pass
        self.latest[tool] = anatomy.features({"last": (tool, error, value), "used": set(), "lists": {}})
        self.site = tool + ("!" if error else "")
        if not error:
            self.outputs.append((tool, value))

    def whole(self):
        out = set(self.ticket) | {f"called {t}" for t in self.called} | {f"made {a}" for a in self.fixes}
        for tool, fs in self.latest.items():
            for k, v in fs.items():
                out.add(f"{tool}:{k}" if v is True else f"{tool}:{k}={v}")
        return frozenset(out)


def decisions(sim, ticket, vocab):
    """Each decision of a recorded solo episode: the state, and the call made
    (or stop, at the agent's last message)."""
    state = State(ticket, vocab)
    pending = {}
    order = collections.deque()
    out = []
    for m in sim["messages"]:
        if m["role"] == "assistant":
            calls = m.get("tool_calls") or []
            if not calls:
                out.append({"site": state.site, "state": state.whole(), "action": STOP})
                continue
            for c in calls:
                action = label(c["name"], c.get("arguments"), state.outputs, state.text)
                out.append({"site": state.site, "state": state.whole(), "action": action})
                state.called.add(c["name"])
                if c["name"] not in anatomy.READ["telecom"]:
                    state.fixes.add(action)
                pending[c.get("id")] = c["name"]
                order.append(c.get("id"))
        elif m["role"] == "tool":
            cid = m.get("id")
            if cid is None and order:
                cid = order.popleft()
            name = pending.pop(cid, None)
            if name is not None:
                state.result(name, m.get("content") or "", bool(m.get("error")))
    return out


# The tickets' own criteria ("They will consider the issue resolved when ..."),
# each as a check the workflow can run itself: the phrase, the probe, and what
# its result says when the issue is resolved.
RESOLVED = [
    ("mms message can be successfully sent", "can_send_mms", lambda r: "can send mms" in r.lower()),
    ("speed test returns excellent", "run_speed_test", lambda r: "(excellent)" in r.lower()),
    ("status bar shows that they have signal", "check_status_bar",
     lambda r: "📶" in r and "no signal" not in r.lower() and "airplane mode" not in r.lower()),
]


def own_check(env, ticket):
    """Whether the ticket's stated criterion holds, by the probe the workflow
    can call itself (None if the ticket states none of these)."""
    from tau2.data_model.message import ToolCall

    for phrase, probe, holds in RESOLVED:
        if phrase in ticket.lower():
            result = env.get_response(ToolCall(id="check", name=probe, arguments={}, requestor="assistant"))
            return holds(result.content or "")
    return None


def repeats(action, calls, since_write):
    """Whether an action repeats a read made since the last write, or a write."""
    if action == STOP:
        return False
    name = action.split("{", 1)[0]
    if name in anatomy.READ["telecom"]:
        return action in since_write
    return action in calls


def run(task, predict, vocab, sure, guard, tau2, rng=None, record=None, constructor=None):
    """Run the workflow on one task in τ²-bench's environment; its reward,
    its calls, and whether it handed back. With `rng`, each call is drawn
    from the leaf's calls by their counts; `record` collects the decisions."""
    from tau2.data_model.message import AssistantMessage, ToolCall
    from tau2.data_model.tasks import RewardType
    from tau2.domains.telecom.environment import get_environment
    from tau2.evaluator.evaluator_action import ActionEvaluator
    from tau2.evaluator.evaluator_env import EnvironmentEvaluator

    constructor = constructor or partial(get_environment, policy_type="workflow")
    env = constructor(solo_mode=True)
    init = task.initial_state
    env.set_state(
        initialization_data=init.initialization_data if init else None,
        initialization_actions=init.initialization_actions if init else None,
        message_history=[],
    )
    state = State(task.ticket or "", vocab)
    messages, calls, handed = [], [], False
    since_write = set()  # reads made since the last write
    passed = collections.defaultdict(set)  # (tool, argument) -> values passed

    def concrete(action):
        """The call an action makes here: with --symbolic, its identifiers
        bound again from this run's results and ticket (None if one cannot be)."""
        if action == STOP or not SYMBOLIC:
            return action
        name, args = unlabel(action)
        bound = {}
        for k, v in args.items():
            bound[k] = resolve(v, state.outputs, state.text, passed[(name, k)])
            if bound[k] is None:
                return None
        return label(name, bound) if name != "transfer_to_human_agents" else action

    for i in range(MAX_CALLS):
        _, _, support, tally = predict({"site": state.site, "state": state.whole()})
        # The leaf's likeliest action that is not a repeat: a read made since
        # the last write returns what it returned, and a fix made once is made.
        options = [(a, k, concrete(a)) for a, k in tally.most_common()]
        allowed = [(a, k, c) for a, k, c in options
                   if c is not None and not (guard and repeats(c, calls, since_write))]
        action, k, made = allowed[0] if allowed else (STOP, 0, STOP)
        if rng is not None and allowed:
            action, k, made = rng.choices(allowed, weights=[k for _, k, _ in allowed])[0]
        share = k / support if support else 0.0
        if sure and not (share >= sure and support >= 10):
            handed = True
            break
        if record is not None:
            record.append({"site": state.site, "state": state.whole(), "action": action})
        if action == STOP:
            break
        name, args = unlabel(made)
        call = ToolCall(id=f"call_{i}", name=name, arguments=args, requestor="assistant")
        messages.append(AssistantMessage(role="assistant", content=None, tool_calls=[call]))
        response = env.get_response(call)
        messages.append(response)
        calls.append(made)
        for key, value in args.items():
            if isinstance(value, (str, int, float)):
                passed[(name, key)].add(value)
        since_write = since_write | {made} if name in anatomy.READ["telecom"] else set()
        state.called.add(name)
        if name not in anatomy.READ["telecom"]:
            state.fixes.add(action)
        state.result(name, response.content or "", response.error)
        if name == "transfer_to_human_agents":
            break
    reward = EnvironmentEvaluator.calculate_reward(
        environment_constructor=constructor, task=task, full_trajectory=messages, solo_mode=True, strict_replay=False
    ).reward
    if RewardType.ACTION in task.evaluation_criteria.reward_basis:
        reward *= ActionEvaluator.calculate_reward(task=task, full_trajectory=messages).reward
    # After scoring (the probe is a read, and the score was taken from the
    # calls alone): would the workflow's own check of the ticket's criterion
    # have told it whether it had succeeded?
    # A transfer to a human is the policy's own ending for what the agent may
    # not fix (a locked SIM), not a failure to hand to a model.
    if calls and calls[-1] == "transfer_to_human_agents":
        check = "transferred"
    else:
        check = "resolved" if own_check(env, task.ticket or "") else "not resolved"
    return reward, calls, handed, check


def bagged(predicts):
    """Trees fitted on resamples, as one: each action's share of its leaf,
    summed over the trees (Breiman's bagging, by average share)."""

    def predict(d):
        total = collections.Counter()
        for p in predicts:
            _, _, _, tally = p(d)
            n = sum(tally.values())
            for a, k in tally.items():
                total[a] += k / n
        action, k = total.most_common(1)[0]
        return action, k / len(predicts), len(predicts), total

    return predict


def show(predict, sites):
    """The compiled workflow as rules a person can read: per site (the call
    that just returned, busiest first), the tree's questions down to its
    chosen depth, each leaf the call it makes, with its share of the
    training cases there."""
    lines = []

    def leaf(tally):
        action, k = tally.most_common(1)[0]
        n = sum(tally.values())
        return f"**{action}** ({k} of {n})"

    def walk(node, depth, indent):
        tally, split = node
        if depth == 0 or split is None:
            lines.append(f"{indent}- then {leaf(tally)}")
            return
        f, yes, no = split
        lines.append(f"{indent}- if `{f}`:")
        walk(yes, depth - 1, indent + "  ")
        lines.append(f"{indent}- else:")
        walk(no, depth - 1, indent + "  ")

    for site, n in sites.most_common():
        node, depth = predict.roots[site]
        lines.append(f"\n### After `{site}` ({n} decisions, {depth} deep)\n")
        walk(node, depth, "")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", nargs="+", help="τ²-bench telecom results in solo mode (no-user)")
    ap.add_argument("--tau2", default="../tau2-bench", help="τ²-bench checkout (its tasks and split)")
    ap.add_argument("--sure", type=float, nargs="?", const=0.95, default=None, metavar="SHARE",
                    help="hand back where the leaf's call held in less than this share (0.95) of at least 10 training cases")
    ap.add_argument("--no-guard", action="store_true", help="let the workflow repeat a read before any write, or a write")
    ap.add_argument("--train-share", type=float, default=1.0, help="learn from this share of the training tasks")
    ap.add_argument("--json", help="write each test task's run here")
    ap.add_argument("--show", help="write the compiled workflow here, as rules (Markdown)")
    ap.add_argument("--bootstrap", type=int, metavar="SEED", help="learn from a resample of the episodes, with replacement")
    ap.add_argument("--self-train", type=int, default=0, metavar="ROUNDS",
                    help="then learn from its own runs on every training task's ticket that its own check says resolved")
    ap.add_argument("--rollouts", type=int, default=8, help="runs per training task and round drawn from the leaves' counts (8)")
    ap.add_argument("--seed", type=int, default=0, help="seed for --self-train's draws and --bag's resamples")
    ap.add_argument("--symbolic", action="store_true",
                    help="learn identifiers as where they came from (a result's path, the ticket) and bind them when run")
    ap.add_argument("--rename", action="store_true",
                    help="run the test tasks for a customer no trace saw: John Smith's name, ids and numbers renamed")
    ap.add_argument("--as-customer", metavar="ID",
                    help="run the test tasks for another customer of the database (C1003), keeping those its gold actions still solve")
    ap.add_argument("--bag", type=int, default=0, metavar="N",
                    help="fit N trees, each on a resample of the episodes, and take the call their leaves' shares favour")
    ap.add_argument("--keep", choices=["shortest", "first"], default="shortest",
                    help="per task, keep the shortest run kept in any round, or this round's first (as it stands, else the first draw)")
    ap.add_argument("--verifier", choices=["own", "evaluator"], default="own",
                    help="keep a run on its own check (the default), or on τ²-bench's evaluator, an oracle a deployment lacks")
    args = ap.parse_args()
    global SYMBOLIC, RENAME
    SYMBOLIC = args.symbolic
    from tau2.data_model.tasks import Task
    from tau2.domains.telecom.environment import get_tasks

    tasks = {t.id: t for t in get_tasks("base")}
    split = json.loads((Path(args.tau2) / "data/tau2/domains/telecom/split_tasks.json").read_text())
    train, test = set(split["train"]), set(split["test"])
    tickets = sorted(train)  # every training task's ticket, for --self-train
    test_environment = None
    if args.rename:
        RENAME = renaming()
        test_environment = renamed_environment(RENAME)
        for tid in test:
            tasks[tid] = Task.model_validate_json(RENAME(tasks[tid].model_dump_json()))
    if args.as_customer:
        RENAME, test_environment = as_customer(args.as_customer)
        for tid in test:
            tasks[tid] = Task.model_validate_json(RENAME(tasks[tid].model_dump_json()))
        solvable = {tid for tid in test if gold_passes(tasks[tid], test_environment)}
        print(f"as {args.as_customer}: the gold actions solve {len(solvable)} of {len(test)} test tasks", file=sys.stderr)
        test = solvable
    if args.train_share < 1:
        # A fixed sample of the training tasks, each smaller share inside every larger one.
        ranked = sorted(train, key=lambda t: hashlib.sha256(t.encode()).hexdigest())
        train = set(ranked[: max(1, round(args.train_share * len(ranked)))])
    sims = [s for path in args.results for s in json.load(open(path))["simulations"]]
    good = [s for s in sims if s["task_id"] in train and (s.get("reward_info") or {}).get("reward") == 1]
    if args.bootstrap is not None:
        good = random.Random(args.bootstrap).choices(good, k=len(good))
    # Ticket words in between 5% and 95% of the training tickets.
    df = collections.Counter(g for t in train for g in words(tasks[t].ticket or ""))
    vocab = {g for g, n in df.items() if 0.05 * len(train) <= n <= 0.95 * len(train)}
    episodes = []
    for s in good:
        ep = decisions(s, tasks[s["task_id"]].ticket or "", vocab)
        for d in ep:
            d["task"], d["goal"] = s["task_id"], "none"
        episodes.append(ep)
    ds = [d for ep in episodes for d in ep]

    def fit(episodes):
        if not args.bag:
            return anatomy.tree([d for ep in episodes for d in ep], with_goal=False, counts=True)
        rng = random.Random(f"bag/{args.seed}/{len(episodes)}")
        return bagged([anatomy.tree([d for ep in rng.choices(episodes, k=len(episodes)) for d in ep],
                                    with_goal=False, counts=True) for _ in range(args.bag)])

    predict = fit(episodes)
    print(f"fitted on {len(good)} successful episodes of {len(train)} training tasks: {len(ds)} decisions, "
          f"{len({d['action'] for d in ds})} distinct actions", file=sys.stderr)

    def on_test(predict):
        rows = []
        for tid in sorted(test):
            reward, calls, handed, resolved = run(tasks[tid], predict, vocab, args.sure, not args.no_guard, args.tau2,
                                                  constructor=test_environment)
            rows.append({"task_id": tid, "reward": reward, "handed_back": handed, "own_check": resolved, "calls": calls})
        return rows

    rows = on_test(predict)
    rounds = [{"round": 0, "kept_runs": 0, "kept_that_pass": 0, "test_passed": sum(r["reward"] == 1 for r in rows)}]
    best = {}  # per training task, the shortest run kept: (calls, decisions, reward)
    for r in range(args.self_train):
        for tid in tickets:
            for k in range(args.rollouts + 1):
                record = []
                rng = random.Random(f"{args.seed}/{r}/{tid}/{k}" if args.seed else f"{r}/{tid}/{k}") if k else None
                reward, calls, _, check = run(tasks[tid], predict, vocab, None, not args.no_guard, args.tau2, rng, record)
                ok = check == "resolved" if args.verifier == "own" else reward == 1
                if ok and args.keep == "first":
                    best[tid] = (calls, record, reward)
                    break
                if ok and (tid not in best or len(calls) < len(best[tid][0])):
                    best[tid] = (calls, record, reward)
        own = [[dict(d, task=tid, goal="none") for d in record] for tid, (_, record, _) in sorted(best.items())]
        predict = fit(episodes + own)
        rows = on_test(predict)
        rounds.append({"round": r + 1, "kept_runs": len(best), "kept_that_pass": sum(b[2] == 1 for b in best.values()),
                       "test_passed": sum(x["reward"] == 1 for x in rows)})
        print(f"round {r + 1}: kept runs on {len(best)} of {len(tickets)} training tickets "
              f"({rounds[-1]['kept_that_pass']} pass the evaluator); test passed {rounds[-1]['test_passed']}", file=sys.stderr)
    if args.show and not args.bag:
        own = [d for _, record, _ in best.values() for d in record]
        Path(args.show).write_text(show(predict, collections.Counter(d["site"] for d in ds + own)))
    passed = sum(r["reward"] == 1 for r in rows)
    alone = [r for r in rows if not r["handed_back"]]
    agents = collections.defaultdict(list)
    trials = collections.defaultdict(lambda: collections.defaultdict(list))
    for path in args.results:
        data = json.load(open(path))
        for s in data["simulations"]:
            if s["task_id"] in test:
                ok = (s.get("reward_info") or {}).get("reward") == 1
                agents[Path(path).name].append(ok)
                trials[Path(path).name][s["task_id"]].append(ok)
    per_task = {k: {t: sum(v) / len(v) for t, v in ts.items()} for k, ts in trials.items()}
    report = {
        "test_tasks": len(rows),
        "passed": passed,
        "finished_alone": len(alone),
        "passed_alone": sum(r["reward"] == 1 for r in alone),
        "calls_per_episode": round(sum(len(r["calls"]) for r in rows) / max(len(rows), 1), 2),
        "passed_by_issue": {
            issue: f"{sum(r['reward'] == 1 for r in rows if r['task_id'].startswith(issue))} of {sum(r['task_id'].startswith(issue) for r in rows)}"
            for issue in sorted({r["task_id"].split("]")[0] + "]" for r in rows})
        },
        "training_tasks": len(train),
        "agents_on_test_tasks": {k: round(sum(v) / len(v), 3) for k, v in agents.items()},
        # The workflow's own check against the evaluator: (check says resolved, passed).
        "own_check": {
            f"{c}, {'passed' if p else 'failed'}": sum(1 for r in rows if r["own_check"] == c and (r["reward"] == 1) == p)
            for c in ("resolved", "transferred", "not resolved") for p in (True, False)
        },
        # Hand the episodes the check says are unresolved to each agent, which
        # passes them as often as its four trials of that task did.
        "workflow_then_agent": {
            k: round((sum(r["reward"] == 1 for r in rows if r["own_check"] != "not resolved")
                      + sum(per_task[k][r["task_id"]] for r in rows if r["own_check"] == "not resolved")) / len(rows), 3)
            for k in per_task
        },
        "handed_to_agent": sum(1 for r in rows if r["own_check"] == "not resolved"),
        **({"self_training": rounds} if args.self_train else {}),
        "runs": rows,
    }
    print(json.dumps({k: v for k, v in report.items() if k != "runs"}, indent=1))
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=1) + "\n")


if __name__ == "__main__":
    main()
