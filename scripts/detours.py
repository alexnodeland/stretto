#!/usr/bin/env python3
"""Where a flow's detours come from, from replays that `pilot/check_flow_tagged.py`
wrote (a `tags.json` per episode) and, for `chains`, with `--explore 0` (a
`decisions.jsonl` of every decision with each option's outcome).

    detours.py handback RESULTS.json REPLAY_DIR
        A session hand-back: after a rule fires, the flow makes no more
        lookups in that session. Before it fires the replay is the tagged
        one, so each call's lookups are read from tags.json; after, the agent
        makes the calls the flow would have made. Rules: `oracle` (after k
        detours, known the moment they are made) and `miss` (after k calls
        of a tool the flow had already looked up, with other arguments).

    detours.py chains REPLAY_DIR...
        Each lookup by its place in its chain (the lookups after one of the
        agent's calls): the flow's probability times its binding's chance,
        the product of those along the chain so far (the "path" rule of
        speculative decoding's draft trees), and the share the agent used.

    detours.py described RESULTS.json REPLAY_DIR
        Airline: each reservation the flow read, by whether a reservation
        read before it was one the customer had described (its origin and
        destination named, by code or city, or its id).

    detours.py tools RESULTS.json REPLAY_DIR [RESULTS.json REPLAY_DIR ...]
        Each lookup tool's detours and uses, summed over the replays given,
        by the kind of task (the bracketed issue that starts a τ²-bench
        telecom task id, else "all"), and how many of the detours the agent
        made the same call of with other arguments in that episode (another
        record, or an optional argument such as a `limit`) rather than none.
"""

import argparse
import collections
import json
import re
from pathlib import Path


def key(tool, args):
    return tool, json.dumps(args, sort_keys=True)


def simulations(results):
    return {f"task-{s['task_id']}-{s.get('trial', 0)}": s for s in json.loads(Path(results).read_text())["simulations"]}


def episodes(replay):
    return sorted(p for p in Path(replay).iterdir() if (p / "tags.json").exists())


def replay_calls(messages, tags):
    """Walk the recorded calls as check_flow.py does, skipping those already
    made, and yield (call key, the lookups appended to it) per call made,
    with the customer's words so far."""
    made, customer, ti = set(), "", 0
    for m in messages:
        if m["role"] == "user":
            customer += (m.get("content") or "").lower() + "\n"
        if m["role"] != "assistant":
            continue
        for c in m.get("tool_calls") or []:
            k = key(c["name"], c["arguments"])
            if k in made:
                continue
            assert tags[ti][0] == "agent" and key(tags[ti][1], tags[ti][2]) == k, (tags[ti], k)
            ti += 1
            appended = []
            while ti < len(tags) and tags[ti][0] == "flow":
                appended.append(key(tags[ti][1], tags[ti][2]))
                ti += 1
            made.add(k)
            made.update(appended)
            yield k, appended, customer


def handback(messages, tags, rule, k):
    """(turns saved, detours, handed back) with the rule."""
    recorded = {key(c["name"], c["arguments"]) for m in messages for c in m.get("tool_calls") or []}
    appended_after = {}
    order = []
    for call, appended, _ in replay_calls(messages, list(tags)):
        appended_after.setdefault(call, appended)
        order.append(call)
    made, by_flow, flow_tools = set(), [], collections.Counter()
    misses = revealed = 0
    handed = False
    saved = 0
    for i, m in enumerate(messages):
        if m["role"] != "assistant" or not m.get("tool_calls"):
            continue
        left = 0
        for c in m["tool_calls"]:
            call = key(c["name"], c["arguments"])
            if call in made:
                continue
            left += 1
            made.add(call)
            if not handed:
                if rule == "miss" and flow_tools[c["name"]]:
                    misses += 1
                handed = (rule == "miss" and misses >= k) or (rule == "oracle" and revealed >= k)
            if handed:
                continue
            for f in appended_after.get(call, []):
                made.add(f)
                by_flow.append(f)
                flow_tools[f[0]] += 1
                revealed += f not in recorded
        saved += left == 0
    return saved, sum(f not in recorded for f in by_flow), handed


def cmd_handback(args):
    sims = simulations(args.results)
    eps = episodes(args.replay)
    for rule, ks in (("none", [0]), ("oracle", [1, 2, 3]), ("miss", [1, 2, 3])):
        for k in ks:
            saved = detours = handed = 0
            for e in eps:
                s, d, h = handback(sims[e.name]["messages"], json.loads((e / "tags.json").read_text()), rule, k)
                saved, detours, handed = saved + s, detours + d, handed + h
            print(f"{rule:6s} k={k}: turns saved {saved}, detours {detours}, sessions handed back {handed} of {len(eps)}")


def chain_rows(path):
    """Each lookup the flow made, with its place in its chain."""
    by_episode = collections.defaultdict(list)
    for line in open(path):
        d = json.loads(line)
        by_episode[d["episode"]].append(d)
    for ds in by_episode.values():
        prev, place, product = None, 0, 1.0
        for d in ds:
            events = d["policy"]["events"]
            # A lookup adds its call and its result: the next decision two
            # events on continues the chain.
            if not (prev is not None and prev["action"] == "lookup" and events == prev["policy"]["events"] + 2):
                place, product = 0, 1.0
            if d["action"] == "lookup":
                place += 1
                i = next(i for i, o in enumerate(d["policy"]["options"]) if o["tool"] == d["tool"])
                o = d["policy"]["options"][i]
                pc = o["p"] * o["binding"]
                product *= pc
                label = d["labels"][i]
                yield {"place": place, "pc": pc, "product": product, "used": label["used"], "detour": label["detour"]}
            prev = d


def cmd_chains(args):
    for replay in args.replays:
        rows = list(chain_rows(Path(replay) / "decisions.jsonl"))
        print(f"{Path(replay).name}: {len(rows)} lookups, {sum(r['used'] for r in rows)} used, {sum(r['detour'] for r in rows)} detours")
        for place in (1, 2, 3, 4):
            sel = [r for r in rows if min(r["place"], 4) == place]
            if sel:
                print(f"  place {place}{'+' if place == 4 else ' '}: {len(sel):4d} lookups, p x chance {sum(r['pc'] for r in sel) / len(sel):.2f},"
                      f" product {sum(r['product'] for r in sel) / len(sel):.2f}, used {sum(r['used'] for r in sel) / len(sel):.0%}")
        cut = [r for r in rows if r["product"] < args.threshold]
        print(f"  product below {args.threshold}: {len(cut)} lookups, {sum(r['used'] for r in cut)} used, {sum(r['detour'] for r in cut)} detours")


CITY = {
    "SFO": "San Francisco", "JFK": "New York", "LAX": "Los Angeles", "ORD": "Chicago", "DFW": "Dallas",
    "DEN": "Denver", "SEA": "Seattle", "ATL": "Atlanta", "MIA": "Miami", "BOS": "Boston", "PHX": "Phoenix",
    "IAH": "Houston", "LAS": "Las Vegas", "MCO": "Orlando", "EWR": "Newark", "CLT": "Charlotte",
    "MSP": "Minneapolis", "DTW": "Detroit", "PHL": "Philadelphia", "LGA": "LaGuardia",
}  # τ²-bench airline's list_all_airports


def cmd_described(args):
    db = json.loads((Path(args.tau2) / "data/tau2/domains/airline/db.json").read_text())["reservations"]

    def says(text, code):
        return re.search(rf"\b{code.lower()}\b", text) is not None or CITY.get(code, code).lower() in text

    def described(rid, text):
        r = db.get(rid)
        return r is not None and (rid.lower() in text or (says(text, r["origin"]) and says(text, r["destination"])))

    sims = simulations(args.results)
    tally = collections.Counter()
    for e in episodes(args.replay):
        messages = sims[e.name]["messages"]
        recorded = {key(c["name"], c["arguments"]) for m in messages for c in m.get("tool_calls") or []}
        read = []
        for call, appended, customer in replay_calls(messages, json.loads((e / "tags.json").read_text())):
            if call[0] == "get_reservation_details":
                read.append(json.loads(call[1]).get("reservation_id"))
            for tool, a in appended:
                if tool != "get_reservation_details":
                    continue
                rid = json.loads(a).get("reservation_id")
                before = any(described(r, customer) for r in read if r != rid)
                tally[(before, "detour" if (tool, a) not in recorded else "used")] += 1
                read.append(rid)
    for before in (False, True):
        print(f"after a reservation the customer described: {before!s:5}  used {tally[(before, 'used')]:3d}  detours {tally[(before, 'detour')]:3d}")


def kind(episode):
    """The bracketed issue that starts a telecom task id, else "all"."""
    m = re.match(r"task-\[([^\]]+)\]", episode)
    return m.group(1) if m else "all"


def cmd_tools(args):
    if len(args.pairs) % 2:
        raise SystemExit("detours.py tools: give RESULTS.json REPLAY_DIR pairs")
    tally = collections.Counter()
    for results, replay in zip(args.pairs[::2], args.pairs[1::2]):
        sims = simulations(results)
        for e in episodes(replay):
            if e.name not in sims:
                continue
            calls = [c for m in sims[e.name]["messages"] if m["role"] == "assistant" for c in m.get("tool_calls") or []]
            recorded = {key(c["name"], c["arguments"]) for c in calls}
            tools = {c["name"] for c in calls}
            for who, tool, a in json.loads((e / "tags.json").read_text()):
                if who != "flow":
                    continue
                if key(tool, a) in recorded:
                    tally[(tool, kind(e.name), "used")] += 1
                else:
                    tally[(tool, kind(e.name), "detour")] += 1
                    tally[(tool, kind(e.name), "other arguments")] += tool in tools
    rows = sorted({(t, k) for t, k, _ in tally}, key=lambda r: -tally[(*r, "detour")])
    print(f"{'lookup':26s} {'task':18s} {'detours':>8s} {'used':>6s}  detours the agent made with other arguments")
    for t, k in rows:
        print(f"{t:26s} {k:18s} {tally[(t, k, 'detour')]:8d} {tally[(t, k, 'used')]:6d}  {tally[(t, k, 'other arguments')]}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("handback")
    p.add_argument("results")
    p.add_argument("replay")
    p.set_defaults(run=cmd_handback)
    p = sub.add_parser("chains")
    p.add_argument("replays", nargs="+")
    p.add_argument("--threshold", type=float, default=0.3)
    p.set_defaults(run=cmd_chains)
    p = sub.add_parser("described")
    p.add_argument("results")
    p.add_argument("replay")
    p.add_argument("--tau2", default="../tau2-bench")
    p.set_defaults(run=cmd_described)
    p = sub.add_parser("tools")
    p.add_argument("pairs", nargs="+", metavar="RESULTS_OR_REPLAY")
    p.set_defaults(run=cmd_tools)
    args = ap.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
