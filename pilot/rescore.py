"""Score recorded pilot episodes again with τ²-bench's own evaluator.

Each episode directory holds the `simulation.json` that `run_episode.py`
wrote, and a `result.json` with the reward it recorded. This rescores the
simulation and prints both, one row per episode.

- `--scoring env` is what the pilots used: the database check (and the
  environment assertions). It should reproduce every recorded reward.
- `--scoring communicate` is the communication check alone: whether the
  agent told the customer what the task says it must. It needs no judge.
- `--scoring basis` scores each task as τ²-bench's `ALL` does: the product
  of the checks in its reward basis. Natural-language assertions need an
  LLM judge, so a task whose basis includes them is refused unless
  `--allow-judge` is given (τ²-bench then calls its configured judge model).

    python rescore.py runs/pilot/baseline/task-90
    python rescore.py --scoring basis --json out.json EPISODE_DIR...

An argument may also be a directory of episode directories (`task-*`), such
as one arm of a published pilot archive.
"""

import argparse
import json
import sys
from pathlib import Path

from loguru import logger
from tau2.data_model.simulation import SimulationRun
from tau2.evaluator.evaluator import EvaluationType, evaluate_simulation
from tau2.registry import registry

# τ²-bench logs every replayed tool response at DEBUG.
logger.remove()
logger.add(sys.stderr, level="WARNING")

SCORING = {
    "env": EvaluationType.ENV,
    "communicate": EvaluationType.COMMUNICATE,
    "basis": EvaluationType.ALL,
}


def load_task(domain: str, task_id: str):
    for t in registry.get_tasks_loader(domain)():
        if str(t.id) == str(task_id):
            return t
    raise SystemExit(f"no task {task_id} in {domain}")


def episodes(paths: list[Path]) -> list[Path]:
    out = []
    for p in paths:
        if (p / "simulation.json").exists():
            out.append(p)
        else:
            out.extend(sorted(d for d in p.glob("task-*") if (d / "simulation.json").exists()))
    return out


def domain_of(episode: Path, result: dict) -> str:
    """The domain, from the episode's proxy log header (or `--domain`)."""
    for log in sorted((episode / "log").glob("*.jsonl")):
        if log.name.endswith(".flow.jsonl"):
            continue
        header = json.loads(log.read_text().splitlines()[0])
        if header.get("domain"):
            return header["domain"]
    return result.get("domain", "")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("episodes", type=Path, nargs="+", help="episode directories, or directories of them")
    parser.add_argument("--scoring", choices=sorted(SCORING), default="env")
    parser.add_argument("--domain", help="override the domain read from each episode's log")
    parser.add_argument("--allow-judge", action="store_true", help="let tasks with natural-language assertions call τ²-bench's LLM judge")
    parser.add_argument("--json", type=Path, help="also write the rows here")
    args = parser.parse_args()

    rows, mismatches = [], 0
    for episode in episodes(args.episodes):
        result = json.loads((episode / "result.json").read_text())
        domain = args.domain or domain_of(episode, result)
        task = load_task(domain, result["task_id"])
        basis = [str(b.value if hasattr(b, "value") else b) for b in (task.evaluation_criteria.reward_basis or [])]
        if args.scoring == "basis" and "NL_ASSERTION" in basis and not args.allow_judge:
            print(f"{episode}: task {result['task_id']} counts natural-language assertions; "
                  "skipped (pass --allow-judge to call τ²-bench's judge)", file=sys.stderr)
            continue
        simulation = SimulationRun.model_validate_json((episode / "simulation.json").read_text())
        info = evaluate_simulation(simulation, task, SCORING[args.scoring], solo_mode=False, domain=domain)
        row = {
            "episode": str(episode),
            "domain": domain,
            "task_id": result["task_id"],
            "arm": result.get("arm"),
            "recorded": result.get("reward"),
            "scoring": args.scoring,
            "basis": basis,
            "reward": info.reward,
        }
        if args.scoring == "env" and row["recorded"] is not None and abs(row["reward"] - row["recorded"]) > 1e-9:
            mismatches += 1
            row["mismatch"] = True
        rows.append(row)
        print(f"{domain:8} task {row['task_id']:>4} {str(row['arm']):9} recorded {row['recorded']}  "
              f"{args.scoring} {row['reward']}{'  MISMATCH' if row.get('mismatch') else ''}")
    if args.json:
        args.json.write_text(json.dumps(rows, indent=1) + "\n")
    if args.scoring == "env":
        print(f"{len(rows)} episodes rescored; {mismatches} differ from the recorded reward", file=sys.stderr)
    sys.exit(1 if mismatches else 0)


if __name__ == "__main__":
    main()
