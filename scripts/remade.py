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

An episode folder is laid out as in the published archives
(`docs/results/*-episodes.tar.gz`): `task-*/trajectory.jsonl` and
`task-*/flow.jsonl`.
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


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("episodes", type=Path, nargs="+", help="task-* folders of a live run")
    ap.add_argument("--domain", required=True, choices=["retail", "airline", "telecom"])
    args = ap.parse_args()
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
