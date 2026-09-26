#!/usr/bin/env python3
"""Check a write against what the agent proposed, with no model.

A write should do what the customer agreed to. Before each write, this takes
what the agent said last before the customer's last message (the proposal)
and that message, and for every value the write passes, finds the record it
came from in an earlier tool result, among the records listed with it: an
order among the user's orders, an item among the order's items, a card among
the payment methods, a flight among the search results. A record is named
when the text states its id, or a field no other record in its list shares
(an item's name or options, a card's brand or last four digits).

The write is flagged when the text names another record of that list and not
the one the write passes: a refund to a gift card after "my Mastercard ending
in 2732", another order's laptop, flights other than the ones proposed. A
value whose list the text names nothing of is not flagged: agents confirm
"the original payment method" or "the water bottle" without an id, and a
check on what was left unsaid flags half of all writes (--omissions).

Arguments that take few values across all calls (a reason, a cabin class) are
closed choices and are not checked.

usage: proposal_check.py RESULTS.json... [--labels LABELS.json] [--omissions]
"""

import argparse
import collections
import json
import re
import sys

READ = {
    "find_user_id_by_email", "find_user_id_by_name_zip", "get_user_details",
    "get_order_details", "get_product_details", "list_all_product_types",
    "get_reservation_details", "get_flight_status", "search_direct_flight",
    "search_onestop_flight", "list_all_airports",
}
NOT_WRITES = READ | {"calculate", "think", "transfer_to_human_agents"}
VERBS = {
    "cancel": ["cancel"],
    "modify": ["modify", "change", "update", "switch", "replace", "swap", "exchange"],
    "return": ["return", "refund"],
    "exchange": ["exchange", "swap", "replace", "change"],
    "update": ["update", "change", "modify", "upgrade", "downgrade", "add", "switch"],
    "book": ["book", "reserv", "purchase"],
    "send": ["send", "issue", "compensat", "certificate"],
}


def leaves(x):
    if isinstance(x, dict):
        for v in x.values():
            yield from leaves(v)
    elif isinstance(x, list):
        for v in x:
            yield from leaves(v)
    elif x is not None and not isinstance(x, bool):
        yield x


def norm(v):
    if isinstance(v, float) and v.is_integer():
        v = int(v)
    return str(v).strip().lower()


def said(value, text):
    """Whether `text` states the value: its words, an id without its '#', or a number."""
    v = norm(value)
    if not v:
        return True
    if re.fullmatch(r"-?[\d.,]+", v):
        num = v.rstrip("0").rstrip(".") if "." in v else v
        return re.search(rf"(?<![\d.]){re.escape(num)}(?![\d])", text.replace(",", "")) is not None
    # An id such as credit_card_4196779 is named by its number too.
    number = re.fullmatch(r"[a-z_]+_(\d{4,})", v)
    if number and re.search(rf"(?<!\d){number.group(1)}(?!\d)", text):
        return True
    return v in text or v.lstrip("#") in text


def records(value):
    """Every JSON object in a tool result, with the scalars under it."""
    out = []

    def walk(x):
        if isinstance(x, dict):
            scalars = [norm(v) for v in leaves(x)]
            out.append(scalars)
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)

    walk(value)
    return out


def groups(value):
    """The lists of records in a tool result: each object's or string's
    siblings under one parent (a user's orders, an order's items, the payment
    methods), each record as its key -> scalar map and all its scalars."""
    out = []

    def record(x):
        if isinstance(x, dict):
            keys = {k: norm(v) for k, v in x.items() if not isinstance(v, (dict, list, bool)) and v is not None}
            return keys, [norm(v) for v in leaves(x)]
        return {"": norm(x)}, [norm(x)]

    def walk(x):
        members = None
        if isinstance(x, list):
            members = x
        elif isinstance(x, dict) and len(x) > 1 and all(isinstance(v, dict) for v in x.values()):
            members = list(x.values())  # records keyed by id
        if members and len(members) > 1:
            out.append([record(m) for m in members if isinstance(m, (dict, str, int, float))])
        if isinstance(x, dict):
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)

    walk(value)
    return out


def writes(sim):
    """(call, proposal, customer, records, groups so far, success) per write."""
    proposal, customer, agent_last = "", "", ""
    recs, grps = [], []
    success = (sim.get("reward_info") or {}).get("reward") == 1
    for m in sim["messages"]:
        role = m["role"]
        if role == "user":
            proposal, customer = agent_last, (m.get("content") or "").lower()
        elif role == "assistant":
            if (m.get("content") or "").strip():
                agent_last = m["content"].lower()
            for c in m.get("tool_calls") or []:
                if c["name"] not in NOT_WRITES:
                    yield c, proposal, customer, list(recs), list(grps), success
        elif role == "tool":
            try:
                value = json.loads(m.get("content") or "")
            except (ValueError, TypeError):
                continue
            recs.extend(records(value))
            grps.extend(groups(value))


def contradicted(call, proposal, customer, grps, choices):
    """The values of a write whose list the confirmation chose another record
    of. The customer's reply chooses the records it names; failing that, the
    proposal chooses a record when it names only that one of its list, since
    a proposal that lists several offers a choice rather than making one."""
    out = []

    def named(rec, group, text):
        for v in set(rec[1]):
            if len(v) < 4:
                continue
            if sum(v in other[1] for other in group) == 1 and said(v, text):
                return True
        return False

    # A record the write passes too is not another: an exchange passes the
    # old item and the new one, and the proposal names the new one.
    passed = {norm(v) for v in leaves(call.get("arguments") or {})}
    for arg, value in (call.get("arguments") or {}).items():
        if (call["name"], arg) in choices:
            continue
        for v in leaves(value):
            nv = norm(v)
            for group in reversed(grps):
                mine = [r for r in group if nv in r[0].values()]
                if not mine:
                    continue
                chosen = [r for r in group if named(r, group, customer)]
                if not chosen:
                    offered = [r for r in group if named(r, group, proposal)]
                    chosen = offered if len(offered) == 1 else []
                if chosen and not any(r in chosen for r in mine) and not any(
                    passed & set(r[0].values()) for r in chosen
                ):
                    out.append((arg, nv))
                break
    return out


def check(call, proposal, customer, recs, choices, use_customer, verb):
    """The values of a write nobody stated, and whether its verb was missing."""
    text = proposal + ("\n" + customer if use_customer else "")
    # A sibling names a record only if no other record holds it.
    holders = collections.Counter(s for r in recs for s in set(r))
    missing = []
    for arg, value in (call.get("arguments") or {}).items():
        if (call["name"], arg) in choices:
            continue
        for v in leaves(value):
            if said(v, text):
                continue
            nv = norm(v)
            siblings = {
                s for r in recs if nv in r for s in r
                if s != nv and len(s) >= 4 and holders[s] == 1 and not re.fullmatch(r"[\d.]+", s)
            }
            if any(s in text for s in siblings):
                continue
            missing.append((arg, nv))
    no_verb = False
    if verb:
        head = call["name"].split("_")[0]
        words = VERBS.get(head, [head])
        no_verb = not any(w in proposal for w in words)
    return missing, no_verb


def closed_choices(sims, most=6):
    """(tool, argument) pairs whose values across every call are few short strings."""
    seen = collections.defaultdict(set)
    for s in sims:
        for m in s["messages"]:
            for c in m.get("tool_calls") or []:
                if c["name"] in NOT_WRITES:
                    continue
                for arg, v in (c.get("arguments") or {}).items():
                    if isinstance(v, str):
                        seen[(c["name"], arg)].add(v)
                    else:
                        seen[(c["name"], arg)].add(None)
    return {k for k, vs in seen.items() if None not in vs and len(vs) <= most}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", nargs="+")
    ap.add_argument("--labels", help="blind labels (docs/results/confirm-second-2026-09-24-labels.json)")
    ap.add_argument("--omissions", action="store_true", help="flag any value the proposal left unsaid instead")
    ap.add_argument("--examples", type=int, default=0, help="print this many flagged writes")
    ap.add_argument("--json", help="write the tallies here")
    args = ap.parse_args()
    sims_by_domain = collections.defaultdict(list)
    for path in args.results:
        data = json.load(open(path))
        domain = data["info"]["environment_info"]["domain_name"]
        sims_by_domain[domain] += data["simulations"]
    report = {}
    flagged_examples = []
    index = {}
    for domain, sims in sims_by_domain.items():
        choices = closed_choices(sims)
        tally = collections.Counter()
        for s in sims:
            for call, proposal, customer, recs, grps, success in writes(s):
                if args.omissions:
                    missing, _ = check(call, proposal, customer, recs, choices, True, False)
                else:
                    missing = contradicted(call, proposal, customer, grps, choices)
                flag = bool(missing)
                key = "successful" if success else "failed"
                tally[(key, "writes")] += 1
                tally[(key, "flagged")] += flag
                index[(domain, str(s["task_id"]), call["name"], json.dumps(call.get("arguments"), sort_keys=True), proposal[:200])] = flag
                if flag and len(flagged_examples) < args.examples:
                    flagged_examples.append((domain, s["task_id"], success, call["name"], missing))
        report[domain] = {
            k: {"writes": tally[(k, "writes")], "flagged": tally[(k, "flagged")]}
            for k in ("successful", "failed")
        }
    for domain, r in report.items():
        print(domain, "  ".join(f"{k}: {v['flagged']} of {v['writes']} ({100 * v['flagged'] / max(v['writes'], 1):.1f}%)" for k, v in r.items()))
    for e in flagged_examples:
        print("flagged:", e)
    if args.labels:
        labels = json.load(open(args.labels))
        rows = collections.Counter()
        unmatched = 0
        for set_name in ("described_flips", "proposed_flips"):
            for row in labels[set_name]:
                shown = row["shown"]
                agent = shown.split("AGENT SAID LAST:\n", 1)[1].split("\n\nCUSTOMER REPLIED:\n", 1)[0].lower()
                call = json.loads(shown.split("\n\nCALL: ", 1)[1])
                k = (row["domain"], str(row["task"]), call["tool"], json.dumps(call["arguments"], sort_keys=True), agent[:200])
                if k not in index:
                    unmatched += 1
                    continue
                kind = (row.get("kind") or "?").split(":")[0] if row["label"] == "N" else "confirmed"
                rows[(row["label"], kind, index[k])] += 1
        report["labels"] = {f"{l} {k} flagged={f}": n for (l, k, f), n in sorted(rows.items())}
        print("labels matched:", sum(rows.values()), "unmatched:", unmatched)
        for (label, kind, flag), n in sorted(rows.items()):
            print(f"  {label} {kind:28s} flagged={flag}: {n}")
    if args.json:
        json.dump(report, open(args.json, "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
