#!/usr/bin/env python3
"""A detour's cost and a saved turn's value, counted from recorded episodes.

A read-only flow should make a lookup when its chance of being used before the
next write clears θ* = δ/(β+δ) (the working paper's Proposition 3), where β is
what saving an LLM turn saves and δ what a lookup nobody uses costs. Both are
countable in τ²-bench's recorded episodes, which report each LLM turn's input
tokens, in the agent's own tokens:

- β: over the turns in the read-only ceiling (`ceiling.py`), the turn's input
  tokens, plus its calls' tokens times the LLM turns after it, since the calls
  leave every later context too;
- δ: over the detours a flow made replaying these episodes (`--replays`, from
  `pilot/check_flow.py --explore 0`), the looked-up tool's mean result tokens
  times the mean number of LLM turns left after a result of the tool that
  prompted the lookup (its site), since the result stays in every later context.

Characters become tokens at the agent's own rate, fitted on how much its input
grew between consecutive turns against the characters added in between.

    costs.py RESULTS.json... --replays DIR [--solo] [--tau2 DIR] [--json OUT]

A results file's replay is `DIR/c-<domain>-<name>` (`c-solo-<name>` with
`--solo`), as `pilot/check_flow.py --out` was given. Prints β, δ and θ* per
agent and pooled per domain (β weighted by ceiling turns, δ by detours).
"""

import argparse
import collections
import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anatomy  # noqa: E402
import ceiling  # noqa: E402


def chars_per_token(sims) -> float | None:
    chars = grow = 0
    for s in sims:
        msgs = s["messages"]
        idx = [i for i, m in enumerate(msgs) if m["role"] == "assistant" and m.get("usage")]
        for a, b in zip(idx, idx[1:]):
            g = msgs[b]["usage"]["prompt_tokens"] - msgs[a]["usage"]["prompt_tokens"]
            if g > 0:
                grow += g
                chars += len(msgs[a].get("content") or "") + len(json.dumps(msgs[a].get("tool_calls") or []))
                chars += sum(len(str(m.get("content") or "")) for m in msgs[a + 1:b])
    return chars / grow if grow else None


def count(path: Path, replays: Path, solo: bool, tau2: Path) -> dict | None:
    sims = json.loads(path.read_text())["simulations"]
    domain = ceiling.domain_of(sims)
    test = set(json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())["test"])
    sims = [s for s in sims if str(s["task_id"]) in test and s.get("trial", 0) in (0, 1, 2, 3)]
    writes = {c["name"] for s in sims for m in s["messages"] for c in m.get("tool_calls") or []
              if c["name"] not in anatomy.READ[domain] | anatomy.PURE | anatomy.HANDOFF}
    cpt = chars_per_token(sims)
    if not cpt:
        return None  # the episodes report no tokens
    beta_rows, result_chars, left_after = [], collections.defaultdict(list), collections.defaultdict(list)
    for s in sims:
        msgs = s["messages"]
        # LLM turns: the opening greeting is canned and reports no usage.
        idx = [i for i, m in enumerate(msgs) if m["role"] == "assistant" and m.get("usage")]
        rows = ceiling.classify(s, domain, writes)
        if not idx or len(rows) != len(idx):
            continue
        for k, i in enumerate(idx):
            left = len(idx) - k - 1
            calls = msgs[i].get("tool_calls") or []
            j = i + 1
            for c in calls:
                if j < len(msgs) and msgs[j]["role"] == "tool":
                    result_chars[c["name"]].append(len(str(msgs[j].get("content") or "")))
                    left_after[c["name"]].append(left)
                    j += 1
            if rows[k][2]:  # in the ceiling
                beta_rows.append(msgs[i]["usage"]["prompt_tokens"] + len(json.dumps(calls)) / cpt * left)
    name = path.name.removesuffix(".json")
    label = "solo" if solo else domain
    decisions = replays / f"c-{label}-{name}" / "decisions.jsonl"
    if not decisions.exists() and solo:
        decisions = replays / f"c-solo-{name.split('_telecom')[0]}" / "decisions.jsonl"
    detours = []
    for line in decisions.read_text().splitlines() if decisions.exists() else []:
        d = json.loads(line)
        if d["action"] != "lookup":
            continue
        opts = d["policy"]["options"]
        k = next((i for i, o in enumerate(opts) if o["tool"] == d["tool"] and o.get("arguments") == d.get("arguments")), None)
        if k is None or not d["labels"][k]["detour"]:
            continue
        rc, la = result_chars.get(d["tool"]), left_after.get(d["site"])
        if rc and la:
            detours.append(statistics.mean(rc) / cpt * statistics.mean(la))
    if not beta_rows or not detours:
        return None
    beta, delta = statistics.mean(beta_rows), statistics.mean(detours)
    return {"domain": label, "agent": name, "chars_per_token": round(cpt, 2), "beta": round(beta), "delta": round(delta),
            "theta": round(delta / (beta + delta), 3), "ceiling_turns": len(beta_rows), "detours": len(detours)}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", type=Path, nargs="+")
    ap.add_argument("--replays", type=Path, required=True, help="replays of these results with --explore 0")
    ap.add_argument("--solo", action="store_true", help="τ²-bench's no-user mode (telecom)")
    ap.add_argument("--tau2", type=Path, default=Path("../tau2-bench"))
    ap.add_argument("--json", type=Path, help="write the rows here")
    args = ap.parse_args()
    out = {}
    for path in args.results:
        row = count(path, args.replays, args.solo, args.tau2)
        if row is None:
            print(f"{path.name}: no tokens reported, or no detours replayed", file=sys.stderr)
            continue
        out[f"{row['domain']}/{row['agent']}"] = row
        print(f"{row['domain']:8s} {row['agent'][:40]:40s} {row['chars_per_token']:4.2f} chars/token  "
              f"β {row['beta']:6d}  δ {row['delta']:6d}  θ* {row['theta']:.3f}  ({row['detours']} detours)")
    for dom in sorted({r["domain"] for r in out.values()}):
        v = [r for r in out.values() if r["domain"] == dom]
        b = sum(r["beta"] * r["ceiling_turns"] for r in v) / sum(r["ceiling_turns"] for r in v)
        d = sum(r["delta"] * r["detours"] for r in v) / sum(r["detours"] for r in v)
        out[f"{dom}/pooled"] = {"domain": dom, "agents": len(v), "beta": round(b), "delta": round(d), "theta": round(d / (b + d), 3)}
        print(f"== {dom}: {len(v)} agents, β {b:.0f}, δ {d:.0f}, θ* {d / (b + d):.3f}")
    if args.json:
        args.json.write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()
