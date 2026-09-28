"""The surprise gate replayed on τ²-bench agents (issue #17).

    PATH=~/.venvs/tau2/bin:$PATH python scripts/surprise_study.py --tau2 DIR --glm5 DIR
        [--stretto BIN] [--quantiles 0.99 0.95 0.9 0.8] [--windows 3 5] [--jobs 4]
        [--out FILE.json]

For each agent and domain, a flow of the habit alone is learned from the
agent's episodes on the training tasks (`stretto learn --habit-only
--results`): without a surprise gate, and with one learned at each quantile
over each window (`--surprise Q --surprise-window N`). Each flow serves the agent's test
episodes, every trial, as `stretto init` serves a learned flow (`reach` at
0.3), in `pilot/check_flow.py`'s exact replay: the recorded conversation,
each of the agent's calls made through τ²-bench's tools and the flow's
lookups after it. A recorded LLM turn is saved when the flow's lookups
answered all of its calls, and a lookup that answered none is a detour. A
session trips the gate when the flow handed back in it because the session
surprised it.

The agents are GLM-5 (`--glm5`, its results files) and Claude 3.7 Sonnet
(τ²-bench's own results), in retail and airline. Prints a Markdown table
and writes every run, with each session's turns saved and detours, to
`--out`.
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def results_of(a, agent: str, domain: str) -> Path:
    if agent == "GLM-5":
        return next(a.glm5.glob(f"glm-5_*_{domain}_*.json"))
    return (a.tau2 / "data/tau2/results/final"
            / f"claude-3-7-sonnet-20250219_{domain}_default_gpt-4.1-2025-04-14_4trials.json")


def learn(a, results: Path, domain: str, out: Path, q: float | None, window: int) -> dict | None:
    """Learn the flow, with a gate at quantile `q` over `window` steps; the
    gate it stores."""
    args = [a.stretto, "learn", "--domain", domain, "--results", str(results), "--tau2", str(a.tau2),
            "--habit-only", "--out", str(out)]
    if q is not None:
        args += ["--surprise", str(q), "--surprise-window", str(window)]
    done = subprocess.run(args, capture_output=True, text=True)
    if done.returncode:
        raise SystemExit(f"{' '.join(args)}: {done.stderr}")
    return json.loads(out.read_text()).get("surprise")


def replay(a, results: Path, domain: str, flow: Path, out: Path) -> list[dict]:
    """Each test session as the flow served it."""
    cmd = [sys.executable, str(HERE.parent / "pilot" / "check_flow.py"), "--results", str(results),
           "--domain", domain, "--tau2", str(a.tau2), "--trials", *map(str, range(a.trials)),
           "--oracle-cache", str(out / "cache"), "--flow", str(flow), "--flow-decider", "reach",
           "--flow-threshold", "0.3", "--in-process", "--jobs", str(a.jobs), "--explore", "0",
           "--out", str(out)]
    done = subprocess.run(cmd, capture_output=True, text=True)
    if done.returncode:
        raise SystemExit(f"{' '.join(cmd)}: {done.stderr[-3000:]}")
    check = json.loads((out / "check.json").read_text())
    sessions = []
    for ep in check["episodes"]:
        answers = out / ep["episode"] / "flow-answers.jsonl"
        lines = answers.read_text().splitlines() if answers.exists() else []
        reasons = [json.loads(line)["answer"].get("reason", "") for line in lines]
        sessions.append({
            "episode": ep["episode"],
            "turns": ep["turns"],
            "saved": ep["turns_saved"],
            "used": ep["calls_skipped"],
            "detours": ep["detours"],
            "tripped": any(r.startswith("the session surprised the flow") for r in reasons),
        })
    return sessions


def total(sessions: list[dict], key: str) -> int:
    return sum(s[key] for s in sessions)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--tau2", type=Path, required=True)
    ap.add_argument("--glm5", type=Path, required=True, help="the directory of GLM-5's results files")
    ap.add_argument("--stretto", default="target/release/stretto")
    ap.add_argument("--quantiles", type=float, nargs="+", default=[0.99, 0.95, 0.9, 0.8])
    ap.add_argument("--windows", type=int, nargs="+", default=[3, 5])
    ap.add_argument("--trials", type=int, default=4, help="trials of each test task to replay")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--keep", type=Path, help="a directory to keep the flows in")
    a = ap.parse_args()
    tmp = Path(tempfile.mkdtemp(prefix="surprise-study-"))
    runs = []
    for agent in ("GLM-5", "Claude 3.7 Sonnet"):
        for domain in ("retail", "airline"):
            results = results_of(a, agent, domain)
            name = f"{agent.split()[0].lower()}-{domain}"
            settings = [(None, None)] + [(w, q) for w in a.windows for q in a.quantiles]
            for window, q in settings:
                tag = f"{name}-{window}-{q}" if q else f"{name}-off"
                flow = tmp / f"{tag}.flow.json"
                gate = learn(a, results, domain, flow, q, window)
                sessions = replay(a, results, domain, flow, tmp / tag)
                runs.append({"agent": agent, "domain": domain, "window": window, "quantile": q,
                             "gate": gate, "sessions": sessions})
                print(f"{tag}: {total(sessions, 'saved')} saved, {total(sessions, 'detours')} detours, "
                      f"{sum(s['tripped'] for s in sessions)} tripped", file=sys.stderr, flush=True)
                if a.keep:
                    shutil.copy(flow, a.keep / flow.name)
    shutil.rmtree(tmp)

    print("| Agent | Domain | Gate | Sessions tripped | Turns saved | Detours | Turns lost | Detours avoided |")
    print("|---|---|---|---|---|---|---|---|")
    for r in runs:
        off = next(o for o in runs if o["agent"] == r["agent"] and o["domain"] == r["domain"]
                   and o["quantile"] is None)
        n = len(r["sessions"])
        tripped = sum(s["tripped"] for s in r["sessions"])
        gate = ("off" if r["gate"] is None
                else f"{r['window']} steps at {r['quantile']}: {r['gate']['threshold']:.2f} nats")
        lost = total(off["sessions"], "saved") - total(r["sessions"], "saved")
        avoided = total(off["sessions"], "detours") - total(r["sessions"], "detours")
        print(f"| {r['agent']} | {r['domain']} | {gate} | {tripped} of {n} | "
              f"{total(r['sessions'], 'saved')} | {total(r['sessions'], 'detours')} | {lost} | {avoided} |")
    if a.out:
        a.out.write_text(json.dumps(runs, indent=1) + "\n")


if __name__ == "__main__":
    main()
