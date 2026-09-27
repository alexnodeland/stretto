#!/usr/bin/env python3
"""A threshold per decision, evaluated over logged decisions.

Proposition 3 holds per read: make lookup c at state x when its probability
of use clears δ_c(x)/(β_c(x)+δ_c(x)). Both costs can be counted per
decision, in the agent's own tokens (`costs.py`): δ as the looked-up tool's
mean result tokens times the LLM turns left after the decision, and β as the
next turn's input tokens plus the call's tokens over the turns after it.

This evaluates such rules off-policy. It reads the decisions of a reach replay
at a low threshold (`pilot/check_flow.py --flow-threshold 0.1 --explore 0`,
whose decisions name the message they follow, `at`, and each used option the
message of the call it answers, `use_at`). A rule is run along each logged
chain of lookups. Where the rule makes the logged lookup, the chain goes on.
Where it makes another, that lookup is credited and the chain stops, since
what would follow was not logged. Where it hands back, the chain stops. A used
lookup is worth its share of the turn it spares, times that turn's input
tokens plus the call's tokens over the turns after it. A detour costs δ. At
flat thresholds, check the result against the sweep's actual replays
(`--sweep`).

    per_decision.py --replays DIR --results DIR [--sweep DIR] [--tau2 DIR] [--json OUT]

`--replays` holds `c-<domain>-<name>/decisions.jsonl` for results
`<name>.json` in `--results`. Prints, per domain and pooled over agents, the
counted utility of flat thresholds, of the domain's θ* (Appendix B's pooled
β and δ from `costs.py`), and of three per-decision rules: the threshold per
decision, and that threshold capped at θ* from above or from below (`lower
only`, `raise only`). With `--recalibrate WEIGHT`, also θ* and the threshold
per decision with each lookup scored by its own site's rate of use (site,
tool and binding's chance), counted on the other half of the tasks and shrunk
towards the model's score as a prior of WEIGHT lookups.
"""

import argparse
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import costs  # noqa: E402

THETA_STAR = {"retail": 0.298, "airline": 0.126, "telecom": 0.117}  # Appendix B's counts


def key(tool: str, args: dict) -> tuple[str, str]:
    return (tool, json.dumps(args, sort_keys=True))


def load(results: Path, domain: str, tau2: Path):
    """The test episodes by replay name, the agent's characters per token, and each tool's mean result tokens."""
    sims = json.loads(results.read_text())["simulations"]
    cpt = costs.chars_per_token(sims)
    test = set(json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())["test"])
    eps, size = {}, defaultdict(list)
    for s in sims:
        msgs = s["messages"]
        waiting = []
        for m in msgs:
            if m["role"] in ("assistant", "user"):
                waiting.extend((c["name"], m["role"]) for c in m.get("tool_calls") or [])
            elif m["role"] == "tool" and waiting:
                name, who = waiting.pop(0)
                if who == "assistant":
                    size[name].append(len(str(m.get("content") or "")))
        if str(s["task_id"]) in test and s.get("trial", 0) in (0, 1, 2, 3):
            eps[f"task-{s['task_id']}-{s.get('trial', 0)}"] = msgs
    return eps, cpt, {t: statistics.mean(v) / cpt for t, v in size.items()}


def positions(msgs):
    """For each message index: the LLM turns after it, and the next turn's input tokens."""
    idx = [i for i, m in enumerate(msgs) if m["role"] == "assistant" and m.get("usage")]
    after, nxt = {}, {}
    for i in range(len(msgs)):
        later = [j for j in idx if j > i]
        after[i] = len(later)
        nxt[i] = msgs[later[0]]["usage"]["prompt_tokens"] if later else 0
    return after, nxt


def calibration(decisions, weight: float) -> dict:
    """Each site's own rate of use, counted: for (site, tool, binding's chance) the options weighed there and how
    many the agent used, to shrink the model's score towards, Beta-binomial with the score as prior mean."""
    counts = defaultdict(lambda: [0, 0])
    for d in decisions:
        for o, lab in zip(d["policy"]["options"], d["labels"]):
            if o.get("arguments") is None or o.get("binding") is None or not (lab["used"] or lab["detour"]):
                continue
            k = (d.get("site"), o["tool"], round(o["binding"], 2))
            counts[k][0] += 1
            counts[k][1] += lab["used"]
    return {"counts": counts, "weight": weight}


def half(d) -> int:
    """A decision's half of the tasks, for cross-fitting."""
    t = str(d["policy"].get("task_id") or d.get("episode", ""))
    return int(t) % 2 if t.isdigit() else sum(map(ord, t)) % 2


def score(o, d, cal) -> tuple[float, float]:
    """The rule's two tests: the tool's chance and the lookup's (times its binding's), or with `cal`, the
    lookup's chance recalibrated at its site for both."""
    v = o["p"] * o["binding"]
    if cal is None:
        return o["p"], v
    n, used = cal["counts"].get((d.get("site"), o["tool"], round(o["binding"], 2)), (0, 0))
    q = (used + cal["weight"] * v) / (n + cal["weight"])
    return q, q


def evaluate(decisions, eps, cpt, mean_tok, rule, cal=None) -> dict:
    saved = util = 0.0
    detours = lookups = 0
    by_ep = defaultdict(list)
    for d in decisions:
        by_ep[d["episode"]].append(d)
    for ep, ds in by_ep.items():
        msgs = eps.get(ep)
        if msgs is None:
            continue
        after, nxt = positions(msgs)
        chains = defaultdict(list)
        for d in ds:
            chains[d["at"]].append(d)
        for at, chain in chains.items():
            for d in chain:
                cand = []
                for o, lab in zip(d["policy"]["options"], d["labels"]):
                    if o.get("arguments") is None or o.get("binding") is None:
                        continue
                    call_tok = len(json.dumps({"name": o["tool"], "arguments": o["arguments"]})) / cpt
                    delta = mean_tok.get(o["tool"], 0.0) * after[at]
                    beta = nxt[at] + call_tok * max(0, after[at] - 1)
                    th = rule(delta / (beta + delta) if beta + delta > 0 else 1.0)
                    q, v = score(o, d, cal)
                    if q >= th and v >= th:
                        cand.append((v, o, lab, delta, call_tok))
                if not cand:
                    break
                v, o, lab, delta, call_tok = max(cand, key=lambda x: x[0])
                lookups += 1
                if lab["used"]:
                    j = lab.get("use_at")
                    worth = msgs[j]["usage"]["prompt_tokens"] + call_tok * after[j] if j is not None and msgs[j].get("usage") else 0.0
                    saved += lab["turn"]
                    util += lab["turn"] * worth
                elif lab["detour"]:
                    detours += 1
                    util -= delta
                if not (d["action"] == "lookup" and key(d["tool"], d.get("arguments") or {}) == key(o["tool"], o["arguments"])):
                    break
    return {"saved": round(saved, 1), "detours": detours, "lookups": lookups, "util": round(util)}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--replays", type=Path, required=True, help="reach replays at a low threshold, with --explore 0")
    ap.add_argument("--results", type=Path, required=True, help="the folder of the replayed results files")
    ap.add_argument("--sweep", type=Path, help="actual replays sw-<domain>-<name>-reach-<θ>, to check flat thresholds against")
    ap.add_argument("--tau2", type=Path, default=Path("../tau2-bench"))
    ap.add_argument("--thetas", default="0.1,0.15,0.2,0.3,0.4,0.5")
    ap.add_argument("--recalibrate", type=float, metavar="WEIGHT",
                    help="also score each lookup by its site's own rate of use, counted on the other half of the tasks "
                         "(cross-fitted), with the model's score as a prior of this many lookups")
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    thetas = [float(x) for x in args.thetas.split(",")]
    rows, validation = {}, []
    for d in sorted(args.replays.glob("c-*")):
        if not d.is_dir() or not (d / "check.json").exists():
            continue
        dom = d.name.split("-")[1]
        name = d.name[len(f"c-{dom}-"):]
        eps, cpt, mean_tok = load(args.results / f"{name}.json", dom, args.tau2)
        if not cpt:
            continue  # the episodes report no tokens
        decisions = [json.loads(line) for line in (d / "decisions.jsonl").read_text().splitlines()]
        star = THETA_STAR[dom]
        row = {f"flat {t}": evaluate(decisions, eps, cpt, mean_tok, lambda x, t=t: t) for t in thetas}
        row["theta star"] = evaluate(decisions, eps, cpt, mean_tok, lambda x: star)
        row["per decision"] = evaluate(decisions, eps, cpt, mean_tok, lambda x: x)
        row["lower only"] = evaluate(decisions, eps, cpt, mean_tok, lambda x: min(star, x))
        row["raise only"] = evaluate(decisions, eps, cpt, mean_tok, lambda x: max(star, x))
        if args.recalibrate:
            for k, rule in [("theta star, recalibrated", lambda x: star), ("per decision, recalibrated", lambda x: x)]:
                parts = []
                for f in (0, 1):
                    cal = calibration([x for x in decisions if half(x) != f], args.recalibrate)
                    parts.append(evaluate([x for x in decisions if half(x) == f], eps, cpt, mean_tok, rule, cal))
                row[k] = {m: parts[0][m] + parts[1][m] for m in parts[0]}
        rows[d.name] = row
        for t in thetas if args.sweep else []:
            chk = args.sweep / f"sw-{dom}-{name}-reach-{t:g}" / "check.json"
            if chk.exists():
                tot = json.loads(chk.read_text())["total"]
                validation.append({"domain": dom, "agent": name, "theta": t, "saved": tot["turns_saved"], "saved_est": row[f"flat {t}"]["saved"],
                                   "detours": tot["detours"], "detours_est": row[f"flat {t}"]["detours"]})
    if validation:
        err = [(v["saved_est"] - v["saved"]) / max(1, v["saved"]) for v in validation]
        print(f"flat thresholds against the sweep ({len(validation)} replays): turns saved {statistics.mean(err):+.1%} on average, "
              f"{statistics.mean(abs(e) for e in err):.1%} in absolute value")
    summary = {}
    for dom in THETA_STAR:
        rs = [r for n, r in rows.items() if n.split("-")[1] == dom]
        if not rs:
            continue
        base = sum(r["theta star"]["util"] for r in rs)
        summary[dom] = {}
        print(f"{dom} ({len(rs)} agents, θ* = {THETA_STAR[dom]})")
        for k in ["theta star", "per decision", "lower only", "raise only", "theta star, recalibrated", "per decision, recalibrated"]:
            if k not in rs[0]:
                continue
            tot = {m: sum(r[k][m] for r in rs) for m in ("util", "saved", "detours", "lookups")}
            change = [r[k]["util"] / r["theta star"]["util"] - 1 for r in rs if r["theta star"]["util"] > 0]
            summary[dom][k] = {**tot, "change": tot["util"] / base - 1, "per_agent_change": [min(change), max(change)]}
            print(f"  {k:26s} utility {tot['util'] / 1e6:6.2f}M tokens ({tot['util'] / base - 1:+.1%}; per agent {min(change):+.0%} to {max(change):+.0%})"
                  f"  saved {tot['saved']:7.1f}  detours {tot['detours']:5d}")
    if args.json:
        args.json.write_text(json.dumps({"validation": validation, "summary": summary, "rows": rows}, indent=1) + "\n")


if __name__ == "__main__":
    main()
