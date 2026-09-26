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


def label(name, args):
    """A call as an action: its tool and arguments, numbers as integers where
    they are whole. A hand-off's summary is free text and is left out."""
    if name == "transfer_to_human_agents":
        return name
    args = {k: int(v) if isinstance(v, float) and v.is_integer() else v for k, v in (args or {}).items()}
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
                out.append({"site": state.site, "state": state.whole(), "action": label(c["name"], c.get("arguments"))})
                state.called.add(c["name"])
                if c["name"] not in anatomy.READ["telecom"]:
                    state.fixes.add(label(c["name"], c.get("arguments")))
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


def run(task, predict, vocab, sure, guard, tau2, rng=None, record=None):
    """Run the workflow on one task in τ²-bench's environment; its reward,
    its calls, and whether it handed back. With `rng`, each call is drawn
    from the leaf's calls by their counts; `record` collects the decisions."""
    from tau2.data_model.message import AssistantMessage, ToolCall
    from tau2.data_model.tasks import RewardType
    from tau2.domains.telecom.environment import get_environment
    from tau2.evaluator.evaluator_action import ActionEvaluator
    from tau2.evaluator.evaluator_env import EnvironmentEvaluator

    constructor = partial(get_environment, policy_type="workflow")
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
    for i in range(MAX_CALLS):
        _, _, support, tally = predict({"site": state.site, "state": state.whole()})
        # The leaf's likeliest action that is not a repeat: a read made since
        # the last write returns what it returned, and a fix made once is made.
        allowed = [(a, k) for a, k in tally.most_common() if not (guard and repeats(a, calls, since_write))]
        action, k = allowed[0] if allowed else (STOP, 0)
        if rng is not None and allowed:
            action, k = rng.choices(allowed, weights=[k for _, k in allowed])[0]
        share = k / support if support else 0.0
        if sure and not (share >= sure and support >= 10):
            handed = True
            break
        if record is not None:
            record.append({"site": state.site, "state": state.whole(), "action": action})
        if action == STOP:
            break
        name, args = unlabel(action)
        call = ToolCall(id=f"call_{i}", name=name, arguments=args, requestor="assistant")
        messages.append(AssistantMessage(role="assistant", content=None, tool_calls=[call]))
        response = env.get_response(call)
        messages.append(response)
        calls.append(action)
        since_write = since_write | {action} if name in anatomy.READ["telecom"] else set()
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
    ap.add_argument("--bag", type=int, default=0, metavar="N",
                    help="fit N trees, each on a resample of the episodes, and take the call their leaves' shares favour")
    ap.add_argument("--keep", choices=["shortest", "first"], default="shortest",
                    help="per task, keep the shortest run kept in any round, or this round's first (as it stands, else the first draw)")
    ap.add_argument("--verifier", choices=["own", "evaluator"], default="own",
                    help="keep a run on its own check (the default), or on τ²-bench's evaluator, an oracle a deployment lacks")
    args = ap.parse_args()
    from tau2.domains.telecom.environment import get_tasks

    tasks = {t.id: t for t in get_tasks("base")}
    split = json.loads((Path(args.tau2) / "data/tau2/domains/telecom/split_tasks.json").read_text())
    train, test = set(split["train"]), set(split["test"])
    tickets = sorted(train)  # every training task's ticket, for --self-train
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
            reward, calls, handed, resolved = run(tasks[tid], predict, vocab, args.sure, not args.no_guard, args.tau2)
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
