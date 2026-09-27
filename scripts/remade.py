#!/usr/bin/env python3
"""Does the agent skip what a flow already looked up? A live check of the
replay's assumption.

`pilot/check_flow.py` counts a recorded call as answered when a flow lookup
returned the same result earlier, assuming the agent, seeing that result,
would not make the call itself. In a live episode the flow's lookups are in
`flow.jsonl` and every call, the agent's and the flow's, in τ²-bench's
`trajectory.jsonl`. This matches each lookup to its call in the trajectory,
in order, and counts the lookups the agent made again before the next write.
For comparison it also counts the agent's repeats of its own reads.

    remade.py --domain retail EPISODE_DIR...
    remade.py --results RESULTS.json... [--tau2 DIR]

An episode folder is laid out as in the published archives
(`docs/results/*-episodes.tar.gz`): `task-*/trajectory.jsonl` and
`task-*/flow.jsonl`. With `--results`, recorded runs with no flow (any
benchmark laid out as τ²-bench's, `scripts/*_to_tau2.py`), it counts only
the agent's repeats of its own reads, with no write between: an agent that
makes a read again is one that may make a lookup's again.
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anatomy  # noqa: E402


def calls(trajectory: Path) -> list[tuple[str, str]]:
    out = []
    for line in trajectory.read_text().splitlines():
        m = json.loads(line)
        if m["role"] == "assistant" and m.get("tool_calls"):
            out += [(c["name"], json.dumps(c["arguments"], sort_keys=True)) for c in m["tool_calls"]]
    return out


def repeats(results: Path, tau2: Path) -> dict:
    """An agent's reads in recorded runs, and those it had made already with no write since."""
    data = json.loads(results.read_text())
    domain = data["info"]["environment_info"]["domain_name"]
    if domain in anatomy.READ:
        reads, writes = anatomy.READ[domain], None
    else:
        reads, writes = anatomy.stub_tools(tau2, domain)
    out = dict(runs=0, reads=0, repeats=0)
    for sim in data["simulations"]:
        out["runs"] += 1
        seen = set()
        for m in sim["messages"]:
            if m["role"] != "assistant":
                continue
            for c in m.get("tool_calls") or []:
                if c["name"] in reads:
                    k = (c["name"], json.dumps(c["arguments"], sort_keys=True))
                    out["reads"] += 1
                    out["repeats"] += k in seen
                    seen.add(k)
                elif writes is None or c["name"] in writes:
                    seen = set()  # a write: what was read may have changed
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("episodes", type=Path, nargs="*", help="task-* folders of a live run")
    ap.add_argument("--domain", choices=["retail", "airline", "telecom"])
    ap.add_argument("--results", type=Path, nargs="+", help="recorded runs, as τ²-bench results")
    ap.add_argument("--tau2", type=Path, default=Path("../tau2-bench"), help="the checkout those results belong to")
    args = ap.parse_args()
    if args.results:
        for path in args.results:
            r = repeats(path, args.tau2)
            print(json.dumps({"results": path.name, **r, "share": round(r["repeats"] / max(1, r["reads"]), 4)}))
        return
    if not args.domain:
        ap.error("--domain is required with live episodes")
    reads = anatomy.READ[args.domain]
    tally = dict(episodes=0, lookups=0, matched=0, remade=0, agent_reads=0, agent_repeats=0)
    for ep in args.episodes:
        if not (ep / "flow.jsonl").exists():
            continue
        tally["episodes"] += 1
        seq = calls(ep / "trajectory.jsonl")
        lookups = [(d["tool"], json.dumps(d["arguments"], sort_keys=True))
                   for d in map(json.loads, (ep / "flow.jsonl").read_text().splitlines()) if d.get("action") == "lookup"]
        flow_at, start = set(), 0
        for lookup in lookups:
            tally["lookups"] += 1
            at = next((k for k in range(start, len(seq)) if seq[k] == lookup and k not in flow_at), None)
            if at is None:
                continue
            flow_at.add(at)
            tally["matched"] += 1
            start = at + 1
            for k in range(at + 1, len(seq)):
                if seq[k][0] not in reads:
                    break  # a write: the lookup may no longer hold
                if seq[k] == lookup:
                    tally["remade"] += 1
                    print(f"{ep.name}: made {lookup[0]} {lookup[1]} again")
                    break
        seen = set()
        for k, c in enumerate(seq):
            if k in flow_at:
                seen.add(c)
            elif c[0] not in reads:
                seen = set()
            else:
                tally["agent_reads"] += 1
                tally["agent_repeats"] += c in seen
                seen.add(c)
    print(json.dumps(tally))


if __name__ == "__main__":
    main()
