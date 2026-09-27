"""Paired live trials on Claude models, within a Claude Code subscription's limits.

Runs run_episode.py for every task, trial and arm of a design, a few episodes
at a time, with the agent and the customer on Claude models through
`claude-agent.sh`. The arms:

- `baseline`: τ²-bench's system prompt and tools, no flow;
- `reach`: the same, with the flow learned from τ²-bench's 2025 runs served
  behind the tools (`--arm reach`, `--flow-oracle mock`: no model is asked);
- `batch`: the baseline with Anthropic's sample prompt for parallel tool calls
  added to the system prompt (`--batch-reads`);
- `batch-reach`: both.

Episodes run trial by trial. Within a trial the tasks come in a shuffled order
(`--seed`), and each task's arms in a shuffled order back to back, so the arms
of a task run minutes apart. Each episode lands in OUT/DOMAIN/trial-K/ARM/
task-ID; one with a result.json is not run again, so a stopped run resumes
where it left off. An episode whose agent failed on the API (an error result,
or its process died) is run once more at the end; the first attempt is kept
beside it as ARM/task-ID.failed-N, as is an attempt a stopped run left
unfinished.

The subscription reports its windows in the agent's stream
(`rate_limit_event`): the share of the five-hour and of the seven-day
allowance used. Before starting an episode the runner reads the latest
report, and

- stops for good once the seven-day share reaches `--week-cap`, or if the
  subscription reports anything but `allowed` or starts drawing on overage;
- waits for the five-hour window to reset once its share reaches
  `--window-cap`.

Three failed episodes in a row stop it too. Every finished episode is booked in
OUT/ledger.jsonl with its cost at list prices and the windows after it.

    python run_trials.py --model claude-sonnet-5 --out runs/frontier/sonnet
    python run_trials.py --model claude-haiku-4-5-20251001 --out runs/frontier/haiku \\
        --arms baseline reach batch batch-reach

Run it in τ²-bench's Python environment. `analyze_trials.py` reports on the
output.
"""

import argparse
import json
import random
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
RESULTS = HERE.parent / "docs" / "results"
# The live reach round's tasks (reach-2026-09-26.md): 20 retail and 8 airline
# test tasks, drawn at random (seed 26) from the paired run's.
TASKS = {
    "retail": [5, 12, 17, 27, 32, 33, 38, 40, 42, 60, 61, 62, 64, 68, 71, 74, 77, 79, 97, 108],
    "airline": [6, 16, 19, 22, 30, 31, 32, 48],
}
# The flows the reach round served, learned from τ²-bench's four 2025 runs.
FLOWS = {d: RESULTS / f"reach-2026-09-26-{d}.flow.json" for d in TASKS}
# Each arm's run_episode.py arguments, before the flow's.
ARMS = {
    "baseline": ["--arm", "baseline"],
    "reach": ["--arm", "reach"],
    "batch": ["--arm", "baseline", "--batch-reads"],
    "batch-reach": ["--arm", "reach", "--batch-reads"],
}
PROBE_MODEL = "claude-haiku-4-5-20251001"

lock = threading.Lock()


def log(msg: str) -> None:
    print(f"{datetime.now(timezone.utc).strftime('%H:%M:%S')} {msg}", flush=True)


def windows(info: dict | None) -> dict:
    """The five-hour and seven-day shares used, and when each resets."""
    w = (info or {}).get("unifiedWindows") or {}
    return {
        name: {"used": w.get(name, {}).get("utilization"), "resets": w.get(name, {}).get("resetsAt")}
        for name in ("five_hour", "seven_day")
    }


def probe() -> dict | None:
    """The subscription's windows now, from a one-word request to the
    smallest model (about a cent at list prices)."""
    done = subprocess.run(
        [str(HERE / "claude-agent.sh"), "-p", "--tools", "", "--strict-mcp-config", "--no-session-persistence",
         "--model", PROBE_MODEL, "--output-format", "stream-json", "--verbose", "Reply with the word ok."],
        stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=300, check=False,
    )
    info = None
    for line in done.stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "rate_limit_event":
            info = event.get("rate_limit_info")
    return info


class Limits:
    """The latest report of the subscription's windows, and what it allows."""

    def __init__(self, week_cap: float, window_cap: float):
        self.week_cap, self.window_cap = week_cap, window_cap
        self.info: dict | None = None
        self.stopped: str | None = None

    def update(self, info: dict | None) -> None:
        if info:
            with lock:
                self.info = info

    def refresh(self) -> None:
        self.update(probe())

    def verdict(self) -> tuple[str, float]:
        """`go`, `stop` (with the reason in self.stopped) or `wait` (and how long)."""
        with lock:
            if self.stopped:
                return "stop", 0
            info = self.info
            if info is None:
                self.stopped = "the subscription reported no windows"
                return "stop", 0
            if info.get("status") not in ("allowed", "allowed_warning") or info.get("isUsingOverage"):
                self.stopped = f"the subscription reports {info.get('status')}, overage {info.get('isUsingOverage')}"
                return "stop", 0
            w = windows(info)
            week, window = w["seven_day"]["used"], w["five_hour"]["used"]
            if week is None or window is None:
                self.stopped = "the subscription's report lacks a window's share"
                return "stop", 0
            if week >= self.week_cap:
                self.stopped = f"the seven-day window is at {week:.2f}, the cap {self.week_cap:.2f}"
                return "stop", 0
            if window >= self.window_cap:
                return "wait", max(60.0, (w["five_hour"]["resets"] or 0) - time.time() + 60)
            return "go", 0


def episode_path(out: Path, domain: str, trial: int, arm: str, task: int) -> Path:
    return out / domain / f"trial-{trial}" / arm / f"task-{task}"


def design(domains: list[str], arms: list[str], trials: int, seed: int) -> list[tuple]:
    """Every episode, in the order it runs: trial by trial, tasks shuffled,
    each task's arms shuffled and back to back."""
    rng = random.Random(seed)
    order = []
    for trial in range(1, trials + 1):
        tasks = [(d, t) for d in domains for t in TASKS[d]]
        rng.shuffle(tasks)
        for domain, task in tasks:
            shuffled = list(arms)
            rng.shuffle(shuffled)
            order += [(domain, task, trial, arm) for arm in shuffled]
    return order


def cost(result: dict) -> tuple[float, float]:
    """The agent's and the customer's cost at list prices, in dollars."""
    agent = max((a.get("total_cost_usd") or 0 for a in result.get("agent_results", [])), default=0)
    customer = sum(u.get("total_cost_usd") or 0 for u in result.get("customer_usage", []))
    return agent, customer


def failed_on_api(result: dict) -> bool:
    """Whether the agent's episode broke on the harness or the API, not on
    anything the agent chose: its process died, or a turn ended in an error."""
    return result.get("ending") == "agent_error" or any(a.get("is_error") for a in result.get("agent_results", []))


def run_one(args, limits: Limits, key: tuple, attempt: int) -> str:
    """Run one episode; `ok`, `retry` (the API failed it) or `failed`."""
    domain, task, trial, arm = key
    path = episode_path(args.out, domain, trial, arm, task)
    command = [
        sys.executable, str(HERE / "run_episode.py"),
        "--domain", domain, "--task-id", str(task),
        "--out", str(args.out / domain / f"trial-{trial}"), "--label", arm,
        "--agent-cli", "claude", "--model", args.model,
        "--customer-cli", "claude", "--customer-model", args.customer_model,
    ] + ARMS[arm]
    if "reach" in arm:
        command += ["--flow", str(FLOWS[domain]), "--flow-oracle", "mock", "--flow-threshold", str(args.threshold)]
    if path.exists():
        # An attempt a stopped run left unfinished: kept beside, never read.
        set_aside(path)
    path.mkdir(parents=True, exist_ok=True)
    with open(path / "run.log", "w") as out:
        done = subprocess.run(command, stdout=out, stderr=subprocess.STDOUT, cwd=HERE, check=False)
    result_file = path / "result.json"
    result = json.loads(result_file.read_text()) if done.returncode == 0 and result_file.exists() else None
    status = "ok"
    if result is None:
        status = "failed"
    elif failed_on_api(result):
        status = "retry"
    if result:
        limits.update(result.get("rate_limit"))
    agent_cost, customer_cost = cost(result or {})
    w = windows(limits.info)
    entry = {
        "unix": round(time.time(), 1), "model": args.model, "domain": domain, "task": task, "trial": trial,
        "arm": arm, "attempt": attempt, "status": status,
        "reward": (result or {}).get("reward"), "llm_turns": (result or {}).get("llm_turns"),
        "agent_cost_usd": round(agent_cost, 5), "customer_cost_usd": round(customer_cost, 5),
        "duration_s": (result or {}).get("duration_s"),
        "five_hour": w["five_hour"]["used"], "seven_day": w["seven_day"]["used"],
    }
    with lock:
        with open(args.out / "ledger.jsonl", "a") as f:
            f.write(json.dumps(entry) + "\n")
    log(
        f"{domain} {task} trial {trial} {arm}: {status}, reward {entry['reward']}, {entry['llm_turns']} turns, "
        f"${agent_cost + customer_cost:.3f}, {entry['duration_s']} s; windows {w['five_hour']['used']} / {w['seven_day']['used']}"
    )
    if status != "ok":
        # Keep the attempt beside the episode, and clear the way for the next.
        set_aside(path)
    return status


def set_aside(path: Path) -> None:
    """Move an episode's attempt to ARM/task-ID.failed-N."""
    with lock:
        n = 1
        while (path.parent / f"{path.name}.failed-{n}").exists():
            n += 1
        shutil.move(str(path), str(path.parent / f"{path.name}.failed-{n}"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model", required=True, help="the agent's Claude model")
    parser.add_argument("--customer-model", default="claude-haiku-4-5-20251001", help="the customer's")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--domains", nargs="+", default=list(TASKS), choices=list(TASKS))
    parser.add_argument("--arms", nargs="+", default=["baseline", "reach"], choices=list(ARMS))
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--seed", type=int, default=27)
    parser.add_argument("--threshold", type=float, default=0.3, help="the reach arms' threshold")
    parser.add_argument("--jobs", type=int, default=3, help="episodes at a time")
    parser.add_argument("--week-cap", type=float, default=0.85, help="stop at this share of the seven-day window")
    parser.add_argument("--window-cap", type=float, default=0.80, help="wait at this share of the five-hour window")
    parser.add_argument("--max-episodes", type=int, help="stop after this many episodes (a pilot)")
    parser.add_argument("--dry-run", action="store_true", help="print the order, run nothing")
    args = parser.parse_args()
    if not args.model.startswith("claude-") or not args.customer_model.startswith("claude-"):
        parser.error("the agent and the customer run on Claude models here")
    args.out = args.out.resolve()

    order = design(args.domains, args.arms, args.trials, args.seed)
    todo = [k for k in order if not (episode_path(args.out, k[0], k[2], k[3], k[1]) / "result.json").exists()]
    log(f"{len(order)} episodes in the design, {len(order) - len(todo)} done, {len(todo)} to run")
    if args.max_episodes is not None and args.max_episodes < len(todo):
        todo = todo[: args.max_episodes]
        log(f"running the first {len(todo)} of them")
    if args.dry_run:
        for k in todo:
            print(*k)
        return
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "design.json").write_text(json.dumps({
        "model": args.model, "customer_model": args.customer_model, "domains": args.domains, "arms": args.arms,
        "trials": args.trials, "seed": args.seed, "threshold": args.threshold,
        "tasks": {d: TASKS[d] for d in args.domains}, "flows": {d: str(FLOWS[d].relative_to(HERE.parent)) for d in args.domains},
    }, indent=1) + "\n")

    limits = Limits(args.week_cap, args.window_cap)
    limits.refresh()
    w = windows(limits.info)
    log(f"windows now: five hours {w['five_hour']['used']}, seven days {w['seven_day']['used']}")

    queue = [(k, 1) for k in todo]
    retries: list[tuple] = []
    failures = {"row": 0}

    def worker() -> None:
        while True:
            with lock:
                if not queue:
                    return
            while True:
                verdict, wait = limits.verdict()
                if verdict != "wait":
                    break
                log(f"the five-hour window is at its cap: waiting {wait / 60:.0f} min")
                time.sleep(wait)
                limits.refresh()
            if verdict == "stop":
                return
            with lock:
                if not queue:
                    return
                key, attempt = queue.pop(0)
            status = run_one(args, limits, key, attempt)
            with lock:
                if status == "ok":
                    failures["row"] = 0
                    continue
                failures["row"] += 1
                if status == "retry" and attempt == 1:
                    retries.append((key, 2))
                if failures["row"] >= 3:
                    limits.stopped = limits.stopped or "three failed episodes in a row"
                if not queue and retries:
                    queue.extend(retries)
                    retries.clear()

    threads = [threading.Thread(target=worker) for _ in range(args.jobs)]
    for t in threads:
        t.start()
        time.sleep(5)
    for t in threads:
        t.join()
    if retries and not limits.stopped:
        queue.extend(retries)
        retries.clear()
        worker()
    left = [k for k in order if not (episode_path(args.out, k[0], k[2], k[3], k[1]) / "result.json").exists()]
    log(f"done: {len(order) - len(left)} of {len(order)} episodes" + (f"; stopped: {limits.stopped}" if limits.stopped else ""))


if __name__ == "__main__":
    main()
