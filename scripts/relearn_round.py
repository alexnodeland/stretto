"""A τ²-bench results file from replayed training episodes, for one round of
relearning a flow from the sessions it served (docs/results/served-sessions-2026-09-26.md).
The replays come from pilot/check_flow_tagged.py, run on the recorded episodes.

usage: relearn_round.py DOMAIN REPLAY_DIR RECORDED_RESULTS VARIANT OUT
VARIANT: unified (every call kept), used (the flow's detours dropped:
lookups the agent never made in the recorded episode), or marked (every call
kept, the flow's with stretto-proxy's ids for its own calls, stretto-N)."""
import copy, json, sys
from pathlib import Path

domain, replay_dir, recorded, variant, out = sys.argv[1:6]
rec = json.load(open(recorded))
by = {(str(s["task_id"]), s.get("trial", 0)): s for s in rec["simulations"]}

def key(name, args):
    return name + " " + json.dumps(args or {}, sort_keys=True)

sims, stats = [], {"episodes": 0, "flow": 0, "dropped": 0}
for ep in sorted(Path(replay_dir).glob("task-*")):
    task, trial = ep.name[len("task-"):].rsplit("-", 1)
    src = by[(task, int(trial))]
    recorded_keys = {key(c["name"], c["arguments"]) for m in src["messages"] for c in m.get("tool_calls") or []}
    msgs = [json.loads(l) for l in (ep / "trajectory.jsonl").read_text().splitlines()]
    tags = json.loads((ep / "tags.json").read_text())
    calls = [c for m in msgs for c in m.get("tool_calls") or []]
    assert len(calls) == len(tags), (ep, len(calls), len(tags))
    drop = set()
    rename = {}
    for c, (author, tool, args) in zip(calls, tags):
        if author == "flow" and variant == "marked":
            rename[c["id"]] = f"stretto-{len(rename) + 1}"
        assert key(c["name"], c["arguments"]) == key(tool, args), (ep, c["name"], tool)
        if author == "flow":
            stats["flow"] += 1
            if variant == "used" and key(tool, args) not in recorded_keys:
                drop.add(c["id"])
    stats["dropped"] += len(drop)
    for m in msgs:
        for c in m.get("tool_calls") or []:
            c["id"] = rename.get(c["id"], c["id"])
        if m["role"] == "tool" and m.get("id") in rename:
            m["id"] = rename[m["id"]]
    kept = []
    for m in msgs:
        if m["role"] == "assistant" and m.get("tool_calls"):
            tc = [c for c in m["tool_calls"] if c["id"] not in drop]
            if not tc:
                continue
            m["tool_calls"] = tc
        if m["role"] == "tool" and m.get("id") in drop:
            continue
        kept.append(m)
    sim = copy.deepcopy(src)
    sim["messages"] = kept
    sims.append(sim)
    stats["episodes"] += 1
json.dump({**{k: v for k, v in rec.items() if k != "simulations"}, "simulations": sims}, open(out, "w"))
print(json.dumps(stats))
