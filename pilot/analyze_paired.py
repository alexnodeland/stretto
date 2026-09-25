"""Report on a paired live run (run_paired.py): passes, turns, tokens, detours.

A pair is one task's episode without the flow (`baseline`) and with it
(`flows`), at the same trial. Intervals are 95%, from a bootstrap over tasks
(4,000 draws; a task's trials are drawn together). The pass-rate difference
also gets an exact McNemar test on the pairs that disagree.

    python analyze_paired.py pairs OUT DOMAIN TRIALS [--basis FILE]
    python analyze_paired.py detours OUT DOMAIN TRIALS
    python analyze_paired.py pool OUT...
    python analyze_paired.py arms --tasks ID... --arm NAME=DIR... [--json FILE]

- `pairs` writes OUT/analysis.json: every pair, and the domain's summary.
  `--basis` takes `rescore.py --scoring basis --json` rows and adds the
  pass rate under τ²-bench's full reward basis.
- `detours` writes OUT/detours.json. A lookup the flow made (a tool and its
  arguments) is the agent's own when the agent made the same call in the
  pair's episode without the flow; any other lookup is a detour. A detour
  costs its result's tokens once, as the result enters the agent's context,
  and carried: that many tokens times the LLM turns from then to the
  episode's end, since each turn reads the whole context again. Tokens are
  counted with tiktoken's cl100k_base, a stand-in for GLM's tokenizer. The
  turns come from Claude Code's event stream (`events.jsonl`).
- `pool` pools domains' `pairs` output, drawing tasks within each domain.
- `arms` compares arms that ran the same tasks once each, each with the
  first, which is usually the arm without a flow: turns, tokens, passes and
  the flow's lookups, the agent's own and detours as for `detours`. Each
  DIR holds `task-<id>` episodes.
"""

import argparse
import json
import math
import random
import re
import statistics
import sys
from pathlib import Path

from loguru import logger

# τ²-bench logs its registry at DEBUG as it loads.
logger.remove()
logger.add(sys.stderr, level="WARNING")

import run_pilot  # noqa: E402
from run_pilot import episode_dir  # noqa: E402

HERE = Path(__file__).resolve().parent
DRAWS = 4000
MARKER = "--- Also looked up automatically (current results; no need to repeat these calls) ---"
HEADER = re.compile(r"^([A-Za-z_][\w-]*) (\{.*\}):$")


def mcnemar_p(b: int, c: int) -> float:
    """Two-sided exact McNemar p-value for b and c discordant pairs."""
    n = b + c
    if n == 0:
        return 1.0
    return min(1.0, 2 * sum(math.comb(n, i) for i in range(min(b, c) + 1)) / 2 ** n)


def clustered(tasks: dict, stat, seed: int = 7) -> list[float]:
    """stat over every pair, and its 95% interval over tasks resampled."""
    rng = random.Random(seed)
    names = sorted(tasks)
    point = stat([p for t in names for p in tasks[t]])
    draws = sorted(stat([p for t in (rng.choice(names) for _ in names) for p in tasks[t]]) for _ in range(DRAWS))
    return [round(x, 4) for x in (point, draws[int(0.025 * DRAWS)], draws[int(0.975 * DRAWS) - 1])]


def test_tasks(tau2: Path, domain: str) -> list[str]:
    split = json.loads((tau2 / "data" / "tau2" / "domains" / domain / "split_tasks.json").read_text())
    return [str(t) for t in split["test"]]


def pairs(args) -> None:
    basis = None
    if args.basis:
        basis = {Path(r["episode"]).resolve(): r["reward"] for r in json.loads(args.basis.read_text())}
    run_pilot.ARMS[:] = ["baseline", "flows"]
    run_pilot.summarize(args.out, test_tasks(args.tau2, args.domain), args.trials)
    report = json.loads((args.out / "pilot.json").read_text())
    by_task: dict[str, list] = {}
    for r in report["pairs"]:
        if not (r["baseline"] and r["flows"]):
            continue
        a, b = r["baseline"], r["flows"]
        pair = {"task": r["task_id"], "trial": r["trial"],
                "pass_a": a["reward"] >= 1 - 1e-9, "pass_b": b["reward"] >= 1 - 1e-9,
                "turns_a": a["llm_turns"], "turns_b": b["llm_turns"],
                "tok_a": a["agent_input_tokens"], "tok_b": b["agent_input_tokens"],
                "cr_a": a["credits_billed"], "cr_b": b["credits_billed"],
                "calls_a": a["agent_calls"], "calls_b": b["agent_calls"],
                "lookups": b["flow_lookups"], "repeats": b["repeats"]}
        if basis is not None:
            da = episode_dir(args.out, "baseline", r["task_id"], r["trial"]).resolve()
            db = episode_dir(args.out, "flows", r["task_id"], r["trial"]).resolve()
            pair["basis_a"] = basis.get(da, 0.0) >= 1 - 1e-9
            pair["basis_b"] = basis.get(db, 0.0) >= 1 - 1e-9
        by_task.setdefault(r["task_id"], []).append(pair)
    ps = [p for group in by_task.values() for p in group]
    total = lambda k: sum(p[k] for p in ps)
    diff = lambda a, b: lambda group: (sum(p[b] for p in group) - sum(p[a] for p in group)) / len(group)
    saved = lambda a, b: lambda group: 1 - sum(p[b] for p in group) / sum(p[a] for p in group)
    b_only = sum(p["pass_b"] and not p["pass_a"] for p in ps)
    a_only = sum(p["pass_a"] and not p["pass_b"] for p in ps)
    res = {
        "domain": args.domain, "trials": args.trials, "pairs": len(ps), "tasks": len(by_task),
        "pass_baseline": total("pass_a"), "pass_flows": total("pass_b"),
        "pass_difference": clustered(by_task, diff("pass_a", "pass_b")),
        "discordant": {"flows_only": b_only, "baseline_only": a_only},
        "mcnemar_p": round(mcnemar_p(b_only, a_only), 4),
        "turns": {"baseline": total("turns_a"), "flows": total("turns_b")},
        "turn_difference_per_episode": clustered(
            by_task, lambda group: sum(p["turns_b"] - p["turns_a"] for p in group) / len(group)),
        "turns_saved_share": clustered(by_task, saved("turns_a", "turns_b")),
        "input_tokens": {"baseline": total("tok_a"), "flows": total("tok_b")},
        "input_tokens_saved_share": clustered(by_task, saved("tok_a", "tok_b")),
        "credits": {"baseline": round(total("cr_a"), 1), "flows": round(total("cr_b"), 1)},
        "agent_calls": {"baseline": total("calls_a"), "flows": total("calls_b")},
        "pairs_fewer_turns": sum(p["turns_b"] < p["turns_a"] for p in ps),
        "pairs_more_turns": sum(p["turns_b"] > p["turns_a"] for p in ps),
        "flow_lookups": total("lookups"), "repeats": total("repeats"),
        "episodes_with_lookups": sum(p["lookups"] > 0 for p in ps),
    }
    if basis is not None:
        res["basis_pass"] = {"baseline": total("basis_a"), "flows": total("basis_b")}
        res["basis_pass_difference"] = clustered(by_task, diff("basis_a", "basis_b"))
    (args.out / "analysis.json").write_text(json.dumps({"result": res, "pairs": ps}, indent=1) + "\n")
    print(json.dumps(res, indent=1))


def session_calls(episode: Path) -> list[dict]:
    """The agent's tools/call requests in the proxy's session log, in order, with their results."""
    logs = [p for p in sorted((episode / "log").glob("*.jsonl"))
            if not p.name.endswith((".flow.jsonl", ".confirm.jsonl"))]
    out, by_id = [], {}
    for line in (logs[0].read_text().splitlines()[1:] if logs else []):
        e = json.loads(line)
        m = e.get("message")
        if not isinstance(m, dict):
            continue
        if e.get("from") == "client" and m.get("method") == "tools/call":
            p = m.get("params", {})
            call = {"name": p.get("name"), "arguments": p.get("arguments", {}),
                    "use_id": (p.get("_meta") or {}).get("claudecode/toolUseId"), "text": None}
            out.append(call)
            by_id[json.dumps(m.get("id"))] = call
        elif e.get("from") == "server" and "id" in m and isinstance(m.get("result"), dict):
            call = by_id.get(json.dumps(m["id"]))
            if call is not None:
                call["text"] = "\n".join(c.get("text", "") for c in m["result"].get("content", []))
    return out


def appended(text: str | None) -> list[dict]:
    """The flow's lookups appended to one result: tool, arguments, result text."""
    if not text or MARKER not in text:
        return []
    lookups, current = [], None
    for line in text.split(MARKER, 1)[1].split("\n"):
        header = HEADER.match(line)
        if header:
            try:
                arguments = json.loads(header.group(2))
            except json.JSONDecodeError:
                arguments = None
            if arguments is not None:
                current = {"name": header.group(1), "arguments": arguments, "lines": []}
                lookups.append(current)
                continue
        if current is not None:
            current["lines"].append(line)
    for lookup in lookups:
        lookup["text"] = "\n".join(lookup.pop("lines")).strip("\n")
    return lookups


def turns_after(episode: Path, use_id: str) -> int:
    """LLM turns from the agent reading tool result `use_id` to the episode's end."""
    after, ids = False, set()
    for line in (episode / "events.jsonl").read_text().splitlines():
        e = json.loads(line)
        if e.get("type") == "user" and not after:
            content = e.get("message", {}).get("content")
            after = isinstance(content, list) and any(c.get("tool_use_id") == use_id for c in content)
        elif e.get("type") == "assistant" and after:
            ids.add(e["message"].get("id"))
    return len(ids)


def detours(args) -> None:
    import tiktoken

    enc = tiktoken.get_encoding("cl100k_base")
    key = lambda name, arguments: (name, json.dumps(arguments, sort_keys=True))
    rows = []
    for trial in range(args.trials):
        for task in test_tasks(args.tau2, args.domain):
            flows = episode_dir(args.out, "flows", task, trial)
            baseline = episode_dir(args.out, "baseline", task, trial)
            own = {key(c["name"], c["arguments"]) for c in session_calls(baseline)}
            agent = session_calls(flows)
            for i, call in enumerate(agent):
                later = {key(c["name"], c["arguments"]) for c in agent[i + 1:]}
                for lookup in appended(call["text"]):
                    k = key(lookup["name"], lookup["arguments"])
                    tokens = len(enc.encode(lookup["text"]))
                    turns = turns_after(flows, call["use_id"]) if call["use_id"] else 0
                    rows.append({"task": task, "trial": trial, "tool": lookup["name"],
                                 "arguments": lookup["arguments"], "own": k in own, "repeated": k in later,
                                 "tokens": tokens, "turns_after": turns, "carried": tokens * turns})
    detour = [r for r in rows if not r["own"]]
    own = [r for r in rows if r["own"]]
    by_tool: dict[str, list[int]] = {}
    for r in rows:
        by_tool.setdefault(r["tool"], [0, 0])[0 if r["own"] else 1] += 1
    summary = {
        "domain": args.domain, "lookups": len(rows), "own": len(own), "detours": len(detour),
        "repeated": sum(r["repeated"] for r in rows), "by_tool_own_detour": by_tool,
        "detour_tokens": sum(r["tokens"] for r in detour),
        "detour_tokens_median": statistics.median([r["tokens"] for r in detour]) if detour else 0,
        "detour_tokens_max": max((r["tokens"] for r in detour), default=0),
        "detour_carried": sum(r["carried"] for r in detour),
        "own_tokens": sum(r["tokens"] for r in own), "own_carried": sum(r["carried"] for r in own),
        "pairs_with_detours": len({(r["task"], r["trial"]) for r in detour}),
    }
    (args.out / "detours.json").write_text(json.dumps({"summary": summary, "lookups": rows}, indent=1) + "\n")
    print(json.dumps(summary, indent=1))


def pool(args) -> None:
    domains = []
    for out in args.outs:
        by_task: dict[str, list] = {}
        for p in json.loads((out / "analysis.json").read_text())["pairs"]:
            by_task.setdefault(p["task"], []).append(p)
        domains.append(by_task)

    def stats(sample: list[list[list[dict]]]) -> dict:
        ps = [p for d in sample for group in d for p in group]
        return {
            "pass_difference": (sum(p["pass_b"] for p in ps) - sum(p["pass_a"] for p in ps)) / len(ps),
            "turns_saved_share": 1 - sum(p["turns_b"] for p in ps) / sum(p["turns_a"] for p in ps),
            "input_tokens_saved_share": 1 - sum(p["tok_b"] for p in ps) / sum(p["tok_a"] for p in ps),
        }

    point = stats([list(d.values()) for d in domains])
    rng = random.Random(7)
    draws: dict[str, list[float]] = {k: [] for k in point}
    for _ in range(DRAWS):
        sample = []
        for d in domains:
            names = sorted(d)
            sample.append([d[rng.choice(names)] for _ in names])
        for k, v in stats(sample).items():
            draws[k].append(v)
    ps = [p for d in domains for group in d.values() for p in group]
    b_only = sum(p["pass_b"] and not p["pass_a"] for p in ps)
    a_only = sum(p["pass_a"] and not p["pass_b"] for p in ps)
    res = {"pairs": len(ps), "pass_baseline": sum(p["pass_a"] for p in ps),
           "pass_flows": sum(p["pass_b"] for p in ps),
           "discordant": {"flows_only": b_only, "baseline_only": a_only},
           "mcnemar_p": round(mcnemar_p(b_only, a_only), 4),
           "turns": {"baseline": sum(p["turns_a"] for p in ps), "flows": sum(p["turns_b"] for p in ps)},
           "input_tokens": {"baseline": sum(p["tok_a"] for p in ps), "flows": sum(p["tok_b"] for p in ps)}}
    for k, v in point.items():
        d = sorted(draws[k])
        res[k] = [round(v, 4), round(d[int(0.025 * DRAWS)], 4), round(d[int(0.975 * DRAWS) - 1], 4)]
    print(json.dumps(res, indent=1))


def arms(args) -> None:
    named = [a.split("=", 1) for a in args.arm]
    base_name, base_dir = named[0]
    base_calls = {t: {(c["name"], json.dumps(c["arguments"], sort_keys=True))
                      for c in session_calls(Path(base_dir) / f"task-{t}")} for t in args.tasks}
    out = {"tasks": args.tasks, "base": base_name, "arms": {}}
    base = {t: run_pilot.episode_summary(Path(base_dir) / f"task-{t}") for t in args.tasks}
    for name, directory in named:
        eps = {t: run_pilot.episode_summary(Path(directory) / f"task-{t}") for t in args.tasks}
        pairs = {t: [{"turns_a": base[t]["llm_turns"], "turns_b": eps[t]["llm_turns"],
                      "tok_a": base[t]["agent_input_tokens"], "tok_b": eps[t]["agent_input_tokens"],
                      "pass_a": base[t]["reward"] >= 1 - 1e-9, "pass_b": eps[t]["reward"] >= 1 - 1e-9}]
                 for t in args.tasks}
        own = detour = 0
        for t in args.tasks:
            for call in session_calls(Path(directory) / f"task-{t}"):
                for lookup in appended(call["text"]):
                    key = (lookup["name"], json.dumps(lookup["arguments"], sort_keys=True))
                    own += key in base_calls[t]
                    detour += key not in base_calls[t]
        b_only = sum(ps[0]["pass_a"] and not ps[0]["pass_b"] for ps in pairs.values())
        a_only = sum(ps[0]["pass_b"] and not ps[0]["pass_a"] for ps in pairs.values())
        out["arms"][name] = {
            "passed": sum(e["reward"] >= 1 - 1e-9 for e in eps.values()),
            "failed_tasks": [t for t in args.tasks if eps[t]["reward"] < 1 - 1e-9],
            "turns": sum(e["llm_turns"] for e in eps.values()),
            "input_tokens": sum(e["agent_input_tokens"] for e in eps.values()),
            "credits": round(sum(e["credits_billed"] for e in eps.values()), 1),
            "agent_calls": sum(e["agent_calls"] for e in eps.values()),
            "flow_lookups": sum(e["flow_lookups"] for e in eps.values()),
            "repeats": sum(e["repeats"] for e in eps.values()),
            "own": own, "detours": detour,
            "turns_saved_share": clustered(pairs, lambda ps: 1 - sum(p["turns_b"] for p in ps) / sum(p["turns_a"] for p in ps)),
            "turn_difference_per_episode": clustered(
                pairs, lambda ps: sum(p["turns_b"] - p["turns_a"] for p in ps) / len(ps)),
            "input_tokens_saved_share": clustered(pairs, lambda ps: 1 - sum(p["tok_b"] for p in ps) / sum(p["tok_a"] for p in ps)),
            "fewer_turns": sum(eps[t]["llm_turns"] < base[t]["llm_turns"] for t in args.tasks),
            "more_turns": sum(eps[t]["llm_turns"] > base[t]["llm_turns"] for t in args.tasks),
            "passed_only_here": a_only, "passed_only_in_base": b_only,
            "per_task": {t: {k: eps[t][k] for k in ("reward", "llm_turns", "agent_input_tokens", "flow_lookups",
                                                    "repeats", "credits_billed")} for t in args.tasks},
        }
    if args.json:
        args.json.write_text(json.dumps(out, indent=1) + "\n")
    print(json.dumps({k: {m: v for m, v in a.items() if m != "per_task"} for k, a in out["arms"].items()}, indent=1))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    tau2 = HERE.parent.parent / "sierra-research" / "tau2-bench"
    for name in ("pairs", "detours"):
        p = sub.add_parser(name)
        p.add_argument("out", type=Path)
        p.add_argument("domain")
        p.add_argument("trials", type=int)
        p.add_argument("--tau2", type=Path, default=tau2)
        if name == "pairs":
            p.add_argument("--basis", type=Path, help="rescore.py --scoring basis --json rows")
    p = sub.add_parser("pool")
    p.add_argument("outs", type=Path, nargs="+")
    p = sub.add_parser("arms")
    p.add_argument("--tasks", nargs="+", required=True)
    p.add_argument("--arm", action="append", required=True, help="NAME=DIR; the first is the base")
    p.add_argument("--json", type=Path, help="also write every arm's numbers here")
    args = parser.parse_args()
    {"pairs": pairs, "detours": detours, "pool": pool, "arms": arms}[args.command](args)


if __name__ == "__main__":
    main()
