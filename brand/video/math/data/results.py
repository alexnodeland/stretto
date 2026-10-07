"""The math film's results, from the published rows of the reach round.

    python3 brand/video/math/data/results.py docs/results/reach-2026-09-26.json fugue.json > brand/video/math/data.js

Reads the round's rows (docs/results/reach-2026-09-26.json, the rows the
paper's Figures 6, 7 and 9 and Table 5 are drawn from) and the output of this
directory's Rust program (fugue.json), and writes the film's data as a script
its page loads (window.MATH). Nothing here is estimated: each value is a row,
or a sum or mean of rows as scripts/paper_figures.py computes it.
"""
import collections
import json
import sys

MIN_BIN = 100  # scripts/paper_figures.py: reliability bins of at least 100 lookups

rows = json.load(open(sys.argv[1]))
fugue = json.load(open(sys.argv[2]))

# Figure 6: reliability of the two scores, per domain.
cal = {}
for dom in ("retail", "airline", "telecom", "solo"):
    out = {}
    for key in ("habit", "reach"):
        c = rows["calibration"][key][dom]
        out[key] = {"ece": c["ece"], "lookups": c["lookups"],
                    "bins": [[b["score"], b["used"], b["lookups"]] for b in c["reliability"] if b["lookups"] >= MIN_BIN]}
    cal[dom] = out

# Figure 7: utility against the threshold, at the domain's pooled counted costs.
costs = {k.split("/")[0]: (v["beta"], v["delta"]) for k, v in rows["costs"]["counted"].items() if k.endswith("/pooled")}
sweep = collections.defaultdict(lambda: collections.defaultdict(dict))
for r in rows["sweep"]:
    b, d = costs[r["domain"]]
    key = f'{r["agent"]}'
    sweep[key][r["decider"]][r["threshold"]] = round((b * r["turns_saved"] - d * r["detours"]) / 1e6, 4)
sweep = {k: {"domain": next(r["domain"] for r in rows["sweep"] if r["agent"] == k),
             **{dec: sorted(v.items()) for dec, v in by.items()}} for k, by in sweep.items()}

# Table 5: live, GLM-5.3, each task's LLM turns, baseline (airline: the mean
# of its two trials) and with the speculator.
live = []
for r in rows["live"]:
    base = sum(t["llm_turns"] for t in r["baseline"]) / len(r["baseline"])
    live.append({"domain": r["domain"], "task": r["task_id"], "baseline": base, "reach": r["reach"]["llm_turns"]})

# Figure 9: turns saved against sessions learned from (mean over agents and orders).
turns = {(r["domain"], r["agent"]): r["reach"]["total"]["turns"] for r in rows["replays"] if "reach" in r}
lc = collections.defaultdict(lambda: collections.defaultdict(list))
for r in rows["learning_curves"]:
    t = r.get("turns") or turns.get((r["domain"], r["agent"])) or (1 if r["turns_saved"] == 0 else None)
    if t:
        lc[(r["domain"], r["protocol"])][r["n"]].append(100 * r["turns_saved"] / t)
learning = {f"{d}/{p}": sorted((n, round(sum(v) / len(v), 2)) for n, v in by.items()) for (d, p), by in lc.items()
            if p in ("own", "pool")}

data = {"calibration": cal, "costs": {k: list(v) for k, v in costs.items()}, "sweep": sweep, "live": live,
        "learning": learning, "fugue": fugue}
print("// Written by brand/video/math/data/results.py: the reach round's published rows and fugue.json.")
print(f"window.MATH = {json.dumps(data, separators=(',', ':'))};")
