"""Paired live run on every test task (stretto #2), under a hard Z.ai budget.

Runs run_episode.py exactly as run_pilot.py does, two episodes at a time (the
two arms of one task together). With `--reuse`, a pilot's recorded pairs are
copied in as trial 0 of their tasks. Before starting an episode it checks a
ledger of every Z.ai credit billed: the episode's estimate, plus the
estimates of those in flight, must keep

- the week's total at or under `--week-cap` (the default is 90% of the Lite
  plan's weekly allowance, less a margin), and
- any rolling five hours at or under `--window-cap` (85% of the Lite plan's
  five-hour allowance),

or it waits for the window to free up, or stops for good. After an episode
it books the credits actually billed (run_pilot.episode_summary); a failed
episode is booked at its estimate. Three failures in a row stop the run.

    python run_paired.py retail --oracle-cache ../.oracle-cache --reuse runs/pilot
    python run_paired.py airline --trials 2 --oracle-cache ../.oracle-cache --reuse runs/pilot-airline

Run it in τ²-bench's Python environment. `analyze_paired.py` reports on the
output.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

from run_pilot import episode_dir, episode_summary

HERE = Path(__file__).resolve().parent
WINDOW_S = 5 * 3600
WORKERS = 2
# The mean credits of an episode, for a task no earlier episode ran.
MEAN = {"retail": 12.5, "airline": 18.0}

lock = threading.Lock()
in_flight: dict[str, float] = {}


def log(msg: str) -> None:
    print(f"{datetime.now(timezone.utc).strftime('%H:%M:%S')} {msg}", flush=True)


class Ledger:
    """Every credit billed, one JSON line per episode."""

    def __init__(self, path: Path):
        self.path = path

    def entries(self) -> list[dict]:
        if not self.path.exists():
            return []
        return [json.loads(line) for line in self.path.read_text().splitlines() if line.strip()]

    def book(self, what: str, credits: float) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a") as f:
            f.write(json.dumps({"unix": time.time(), "what": what, "credits": round(credits, 2)}) + "\n")

    def spent(self, since: float = 0.0) -> float:
        return sum(e["credits"] for e in self.entries() if e["unix"] >= since)


def test_tasks(tau2: Path, domain: str) -> list[str]:
    split = json.loads((tau2 / "data" / "tau2" / "domains" / domain / "split_tasks.json").read_text())
    return [str(t) for t in split["test"]]


def estimate(reuse: Path | None, domain: str, task: str) -> float:
    """1.3 times the costliest earlier episode of this task, else of the domain."""
    seen = []
    for arm in ("baseline", "flows"):
        d = reuse / arm / f"task-{task}" if reuse else None
        if d and (d / "result.json").exists():
            seen.append(episode_summary(d)["credits_billed"])
    return 1.3 * (max(seen) if seen else MEAN.get(domain, 18.0))


def may_start(args, ledger: Ledger, cost: float) -> str | None:
    """None if an episode of this estimated cost may start, else why not."""
    now = time.time()
    pending = sum(in_flight.values())
    week = ledger.spent(now - 7 * 86400) + pending + cost
    if week > args.week_cap:
        return f"stop: week total would reach {week:.0f} > {args.week_cap:.0f}"
    window = ledger.spent(now - WINDOW_S) + pending + cost
    if window > args.window_cap:
        return f"wait: five-hour window would reach {window:.0f} > {args.window_cap:.0f}"
    return None


def run_one(args, ledger: Ledger, out: Path, arm: str, task: str, trial: int, cost: float, failures: list) -> None:
    episode = episode_dir(out, arm, task, trial)
    command = [
        sys.executable, str(HERE / "run_episode.py"),
        "--domain", args.domain, "--task-id", task, "--out", str(episode.parent.parent),
        "--arm", arm, "--tau2", str(args.tau2), "--oracle-cache", str(args.oracle_cache),
    ]
    # run_episode.py hands the MCP server `which python`: the τ²-bench
    # environment must come first on PATH.
    env = dict(os.environ, PATH=f"{Path(sys.executable).parent}:{os.environ.get('PATH', '')}")
    done = subprocess.run(command, cwd=HERE, env=env, capture_output=True, text=True, check=False)
    (episode.parent.parent / f"{arm}-task-{task}.log").write_text(done.stdout + done.stderr)
    with lock:
        in_flight.pop(f"{arm}:{task}:{trial}", None)
        if (episode / "result.json").exists():
            s = episode_summary(episode)
            ledger.book(f"{args.domain} {arm} task {task} trial {trial}", s["credits_billed"])
            failures.clear()
            log(f"{args.domain} {arm:8s} task {task:>3} trial {trial}: reward {s['reward']}, turns {s['llm_turns']}, "
                f"credits {s['credits_billed']} (week {ledger.spent(time.time() - 7 * 86400):.0f}, "
                f"window {ledger.spent(time.time() - WINDOW_S):.0f})")
        else:
            ledger.book(f"{args.domain} {arm} task {task} trial {trial} FAILED (estimate)", cost)
            failures.append(task)
            log(f"FAILED {args.domain} {arm} task {task} trial {trial} (exit {done.returncode}); "
                f"booked the estimate {cost:.1f}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("domain", choices=sorted(MEAN))
    parser.add_argument("--trials", type=int, default=1, help="episodes per task and arm")
    parser.add_argument("--out", type=Path, help="default: runs/paired-DOMAIN")
    parser.add_argument("--tau2", type=Path, default=HERE.parent.parent / "sierra-research" / "tau2-bench")
    parser.add_argument("--oracle-cache", type=Path, required=True)
    parser.add_argument("--reuse", type=Path, help="a pilot's output directory: its pairs become trial 0")
    parser.add_argument("--ledger", type=Path, default=Path("runs/zai-ledger.jsonl"),
                        help="the credits billed; book earlier spending this week here first")
    parser.add_argument("--week-cap", type=float, default=8700.0, help="credits in any seven days, at most")
    parser.add_argument("--window-cap", type=float, default=1700.0, help="credits in any five hours, at most")
    parser.add_argument("--dry-run", action="store_true", help="list what would run and the budget, then exit")
    args = parser.parse_args()
    out = (args.out or Path("runs") / f"paired-{args.domain}").resolve()
    ledger = Ledger(args.ledger)
    tasks = test_tasks(args.tau2, args.domain)
    if args.reuse:
        for arm in ("baseline", "flows"):
            for t in tasks:
                src, dst = args.reuse / arm / f"task-{t}", episode_dir(out, arm, t, 0)
                if (src / "result.json").exists() and not dst.exists():
                    shutil.copytree(src, dst)
    todo = [(arm, t, trial) for trial in range(args.trials) for t in tasks for arm in ("baseline", "flows")
            if not (episode_dir(out, arm, t, trial) / "result.json").exists()]
    week = ledger.spent(time.time() - 7 * 86400)
    log(f"{args.domain}: {len(tasks)} tasks, {args.trials} trial(s); {len(todo)} episodes to run; "
        f"week so far {week:.0f} of {args.week_cap:.0f}")
    if args.dry_run:
        total = sum(estimate(args.reuse, args.domain, t) for _, t, _ in todo)
        log(f"dry run: estimated {total:.0f} credits for {len(todo)} episodes; "
            f"window so far {ledger.spent(time.time() - WINDOW_S):.0f} of {args.window_cap:.0f}")
        return
    failures: list = []
    threads: list[threading.Thread] = []
    while todo:
        if len(failures) >= 3:
            log("stop: three failures in a row")
            break
        arm, t, trial = todo[0]
        cost = estimate(args.reuse, args.domain, t)
        with lock:
            why = may_start(args, ledger, cost) if len(in_flight) < WORKERS else "busy"
            if why is None:
                in_flight[f"{arm}:{t}:{trial}"] = cost
        if why is None:
            todo.pop(0)
            th = threading.Thread(target=run_one, args=(args, ledger, out, arm, t, trial, cost, failures))
            th.start()
            threads.append(th)
            continue
        if why.startswith("stop"):
            log(why)
            break
        if why.startswith("wait"):
            log(why)
            time.sleep(300)
        else:
            time.sleep(5)
    for th in threads:
        th.join()
    log(f"done: {len(todo)} episodes not run; week total {ledger.spent(time.time() - 7 * 86400):.0f}")


if __name__ == "__main__":
    main()
