#!/usr/bin/env python3
"""τ-bench's historical trajectories as τ²-bench results, for trace-only replays.

The original τ-bench (sierra-research/tau-bench) publishes some agents'
trajectories (`historical_trajectories/`: GPT-4o and Claude 3.5 Sonnet, in
retail and airline), recorded with its own harness in OpenAI's message
format. This rewrites each as τ²-bench's results file, whose tools share
their names, so `pilot/check_flow.py --trace` can replay them with a flow
learned from τ²-bench's runs:

    taubench_v1_to_tau2.py tau-bench/historical_trajectories/gpt-4o-retail.json \\
        --domain retail --agent gpt-4o --out gpt-4o_retail_taubench-v1.json
"""

import argparse
import json
from pathlib import Path


def convert(traj: list) -> list:
    out = []
    for m in traj:
        role = m.get("role")
        if role == "user":
            out.append({"role": "user", "content": m.get("content") or ""})
        elif role == "assistant":
            calls = []
            for c in m.get("tool_calls") or []:
                fn = c["function"]
                args = fn.get("arguments") or "{}"
                calls.append({"id": c.get("id"), "name": fn["name"],
                              "arguments": json.loads(args) if isinstance(args, str) else args,
                              "requestor": "assistant"})
            text = m.get("content")
            out.append({"role": "assistant", "content": None if text in (None, "None") else text,
                        "tool_calls": calls or None})
        elif role == "tool":
            content = m.get("content") or ""
            out.append({"role": "tool", "id": m.get("tool_call_id"), "content": content, "requestor": "assistant",
                        "error": content.startswith("Error")})
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trajectories", type=Path)
    ap.add_argument("--domain", required=True, choices=["retail", "airline"])
    ap.add_argument("--agent", required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    episodes = json.loads(args.trajectories.read_text())
    sims = [{"id": f"{e['task_id']}-{e.get('trial', 0)}", "task_id": str(e["task_id"]), "trial": e.get("trial", 0),
             "messages": convert(e["traj"]), "reward_info": {"reward": e.get("reward", 0.0)}} for e in episodes]
    tasks = [{"id": t} for t in sorted({s["task_id"] for s in sims}, key=int)]
    results = {"info": {"environment_info": {"domain_name": args.domain},
                        "agent_info": {"llm": f"{args.agent} (τ-bench)"}, "user_info": {"llm": None}},
               "tasks": tasks, "simulations": sims}
    args.out.write_text(json.dumps(results))
    print(f"taubench_v1_to_tau2: {len(sims)} episodes of {len(tasks)} tasks")


if __name__ == "__main__":
    main()
