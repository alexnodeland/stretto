#!/usr/bin/env python3
"""Saved turns in seconds and dollars.

τ²-bench's recorded episodes report each LLM turn's generation time and cost.
A replay whose rows name the turns it saved (`saved_at`, from
`pilot/check_flow.py`) is priced here, per agent and domain: the seconds of
generation and the dollars those turns took, per episode and as a share of
the episodes' own. Detours are charged at the agent's input price, fitted on
its own turns (cost against prompt and completion tokens, least squares),
for each domain's counted detour tokens (Appendix B's δ).

    priced.py --replays DIR --results DIR [--tau2 DIR] [--json OUT]

`--replays` holds `c-<domain>-<name>-<decider>/check.json` for results
`<name>.json` in `--results`. Agents whose turns report no generation time or
no cost are left out.
"""

import argparse
import json
from collections import defaultdict
from pathlib import Path

DELTA = {"retail": 2470, "airline": 1020, "telecom": 1180}  # Appendix B: a detour's tokens, pooled


def prices(sims) -> tuple[float, float]:
    """The agent's price per input and per output token, least squares over its turns."""
    sxx = sxy = syy = sxc = syc = 0.0
    for s in sims:
        for m in s["messages"]:
            u, c = m.get("usage"), m.get("cost")
            if m["role"] != "assistant" or not u or not c:
                continue
            x, y = u.get("prompt_tokens") or 0, u.get("completion_tokens") or 0
            sxx += x * x
            sxy += x * y
            syy += y * y
            sxc += x * c
            syc += y * c
    det = sxx * syy - sxy * sxy
    if det <= 0:
        return 0.0, 0.0
    return (sxc * syy - syc * sxy) / det, (syc * sxx - sxc * sxy) / det


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--replays", type=Path, required=True)
    ap.add_argument("--results", type=Path, required=True)
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    rows = {}
    for d in sorted(args.replays.glob("c-*")):
        chk = d / "check.json"
        if not d.is_dir() or not chk.exists():
            continue
        dom = d.name.split("-")[1]
        dec = d.name.rsplit("-", 1)[1]
        if dom not in DELTA:
            continue  # solo telecom, whose detours are not counted
        name = d.name[len(f"c-{dom}-"): -len(f"-{dec}")]
        sims = json.loads((args.results / f"{name}.json").read_text())["simulations"]
        by = {f"task-{s['task_id']}-{s.get('trial', 0)}": s for s in sims}
        p_in, _ = prices(sims)
        t = dict(episodes=0, seconds=0.0, seconds_all=0.0, dollars=0.0, dollars_all=0.0, detours=0, saved=0)
        timed = True
        for e in json.loads(chk.read_text())["episodes"]:
            s = by.get(e["episode"])
            if s is None or "saved_at" not in e:
                continue
            msgs = s["messages"]
            agent = [m for m in msgs if m["role"] == "assistant" and m.get("usage")]
            if any(m.get("generation_time_seconds") is None for m in agent if m.get("tool_calls")):
                timed = False
            t["episodes"] += 1
            t["seconds_all"] += sum(m.get("generation_time_seconds") or 0 for m in agent)
            t["dollars_all"] += sum(m.get("cost") or 0 for m in agent)
            for i in e["saved_at"]:
                t["seconds"] += msgs[i].get("generation_time_seconds") or 0
                t["dollars"] += msgs[i].get("cost") or 0
            t["saved"] += len(e["saved_at"])
            t["detours"] += e["detours"]
        if not t["episodes"] or not t["dollars_all"]:
            continue
        t["detour_dollars"] = t["detours"] * DELTA[dom] * p_in
        t["price_in"] = p_in
        t["timed"] = timed
        rows.setdefault(f"{dom}/{name}", {})[dec] = t
    out = defaultdict(lambda: defaultdict(lambda: defaultdict(float)))
    for key, by_dec in rows.items():
        dom = key.split("/")[0]
        for dec, t in by_dec.items():
            for k in ("episodes", "seconds", "seconds_all", "dollars", "dollars_all", "detours", "detour_dollars", "saved"):
                out[dom][dec][k] += t[k]
    print("per domain, pooled over agents: per episode, and as a share of the episodes' own")
    for dom, by_dec in out.items():
        for dec, t in sorted(by_dec.items()):
            n = t["episodes"]
            print(f"  {dom:8s} {dec:6s} {n:5.0f} episodes | {t['seconds'] / n:6.1f} s saved ({t['seconds'] / t['seconds_all']:.1%} of generation) | "
                  f"${t['dollars'] / n:.4f} saved, ${t['detour_dollars'] / n:.4f} in detours, net ${(t['dollars'] - t['detour_dollars']) / n:.4f} "
                  f"({(t['dollars'] - t['detour_dollars']) / t['dollars_all']:.1%} of cost)")
    if args.json:
        args.json.write_text(json.dumps({"agents": rows, "domains": out}, indent=1) + "\n")


if __name__ == "__main__":
    main()
