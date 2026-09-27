"""Write a live run's episodes as τ²-bench results, to replay them from the record.

Each episode's agent events (`events.jsonl`, Claude Code's stream) become one
simulation: the task's request, then each LLM turn as an assistant message with
its calls, each call's result as a tool message, and the agent's answer. The
results are the ones the agent saw, so an AgentDojo episode from the no-flow arm
replays as the converters' published runs do (`pilot/check_flow.py --trace`),
and the savings the replay projects can be set against those the flow arm made
live on the same tasks.

    python live_to_tau2.py RUNS/glm --arm baseline --out WORK/live-glm
    # writes WORK/live-glm/<suite>.json, one results file per AgentDojo suite
"""

import argparse
import json
from collections import defaultdict
from pathlib import Path

PREFIX = "mcp__bench__"


def text_of(content) -> str:
    """A tool result's text, as the agent read it."""
    if isinstance(content, str):
        return content
    return "".join(b.get("text", "") for b in content or [] if isinstance(b, dict))


def messages(episode: Path, request: str | None) -> list[dict]:
    """The episode as τ²-bench messages: the request, then each LLM turn and its calls' results."""
    turns: dict[str, dict] = {}
    order: list[str] = []
    results: dict[str, dict] = {}
    answer = ""
    for line in (episode / "events.jsonl").read_text().splitlines():
        event = json.loads(line)
        if event.get("type") == "assistant":
            message = event.get("message", {})
            mid = message.get("id") or str(len(order))
            if mid not in turns:
                turns[mid] = {"text": [], "calls": []}
                order.append(mid)
            for b in message.get("content", []):
                if b.get("type") == "text" and b.get("text"):
                    turns[mid]["text"].append(b["text"])
                elif b.get("type") == "tool_use" and b.get("name", "").startswith(PREFIX):
                    turns[mid]["calls"].append({"id": b["id"], "name": b["name"][len(PREFIX):],
                                                "arguments": b.get("input") or {}, "requestor": "assistant"})
        elif event.get("type") == "user":
            for b in event.get("message", {}).get("content", []) or []:
                if isinstance(b, dict) and b.get("type") == "tool_result":
                    results[b["tool_use_id"]] = {"content": text_of(b.get("content")),
                                                 "error": bool(b.get("is_error"))}
        elif event.get("type") == "result":
            answer = event.get("result") or answer
    out = [{"role": "user", "content": request}] if request is not None else []
    for mid in order:
        turn = turns[mid]
        if turn["calls"]:
            out.append({"role": "assistant", "content": "\n".join(turn["text"]) or None, "tool_calls": turn["calls"]})
            for c in turn["calls"]:
                r = results.get(c["id"], {"content": "", "error": True})
                out.append({"role": "tool", "id": c["id"], "content": r["content"], "requestor": "assistant",
                            "error": r["error"]})
        elif turn["text"]:
            out.append({"role": "assistant", "content": "\n".join(turn["text"])})
    if answer and not (out[-1]["role"] == "assistant" and "tool_calls" not in out[-1]):
        out.append({"role": "assistant", "content": answer})
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("folder", type=Path, help="a model's folder of a live run (run_bench_paired.py --out)")
    parser.add_argument("--arm", default="baseline", help="the arm to write, by its label")
    parser.add_argument("--version", default="v1.2.1", help="AgentDojo's benchmark version, for the requests")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--no-request", action="store_true",
                        help="leave the request out, as the proxy saw the episodes (the bench harness passes it no "
                             "conversation), so the flow decides on what it decided on live")
    args = parser.parse_args()
    from agentdojo.task_suite.load_suites import get_suite

    by_suite = defaultdict(list)
    model = None
    for result in sorted(args.folder.glob(f"{args.arm}/*/result.json")):
        r = json.loads(result.read_text())
        if r.get("bench") != "agentdojo":
            continue
        model = r["model"]
        request = None if args.no_request else get_suite(args.version, r["domain"]).get_user_task_by_id(r["task"]).PROMPT
        by_suite[r["domain"]].append({
            "id": f"{r['task']}-0", "task_id": r["task"], "trial": 0,
            "messages": messages(result.parent, request),
            "reward_info": {"reward": r["reward"]}, "termination_reason": "agent_stop",
        })
    args.out.mkdir(parents=True, exist_ok=True)
    for suite, sims in by_suite.items():
        doc = {"info": {"environment_info": {"domain_name": suite}, "agent_info": {"llm": model},
                        "user_info": {"llm": None}, "num_trials": 1},
               "tasks": [{"id": s["task_id"]} for s in sims], "simulations": sims}
        (args.out / f"{suite}.json").write_text(json.dumps(doc, indent=1))
        print(f"{suite}: {len(sims)} episodes -> {args.out / f'{suite}.json'}")


if __name__ == "__main__":
    main()
