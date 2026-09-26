"""Run a compiled procedure with `stretto-procedure` on τ²-bench telecom's
held-out solo tasks, over MCP, and score each run with τ²-bench's evaluator.

Each task's tools are served by `tau2_mcp.py --solo`, which records the run
in τ²-bench's message format; the run is scored as
`scripts/telecom_workflow.py` scores its own, on the environment's final
state and the required actions where the task asks for them. The
procedure's check of the ticket's outcome is a read made through the same
server, after its last call.

    python run_procedure.py PROCEDURE.json --out DIR [--compare RUNS.json]

With `--compare` (`scripts/telecom_workflow.py --json`), each run is checked
against the reference implementation's: the same calls, the same reward and
the same verdict. The summary goes to DIR/summary.json.
"""

import argparse
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from functools import partial
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "scripts"))

import telecom_workflow  # noqa: E402
from tau2.data_model.message import AssistantMessage, ToolMessage  # noqa: E402
from tau2.data_model.tasks import RewardType  # noqa: E402
from tau2.domains.telecom.environment import get_environment  # noqa: E402
from tau2.evaluator.evaluator_action import ActionEvaluator  # noqa: E402
from tau2.evaluator.evaluator_env import EnvironmentEvaluator  # noqa: E402
from tau2.registry import registry  # noqa: E402

DOMAIN = "telecom-workflow"
VERDICTS = {"resolved": "resolved", "transferred": "transferred", "not resolved": "hand_back"}


def score(task, trajectory: Path) -> float:
    """τ²-bench's reward for a solo run's recorded calls."""
    kinds = {"assistant": AssistantMessage, "tool": ToolMessage}
    messages = [kinds[m["role"]].model_validate(m) for m in map(json.loads, trajectory.read_text().splitlines())
                if m["role"] in kinds]
    reward = EnvironmentEvaluator.calculate_reward(
        environment_constructor=partial(get_environment, policy_type="workflow"), task=task,
        full_trajectory=messages, solo_mode=True, strict_replay=False,
    ).reward
    if RewardType.ACTION in task.evaluation_criteria.reward_basis:
        reward *= ActionEvaluator.calculate_reward(task=task, full_trajectory=messages).reward
    return reward


def run_one(task, args) -> dict:
    episode = (args.out / f"task-{task.id}").resolve()
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "tools-state.json", "run.json"):
        (episode / stale).unlink(missing_ok=True)
    command = [
        str(args.bin), "--procedure", str(args.procedure), "--ticket", task.ticket or "", "--out", str(episode / "run.json"),
        "--", args.python, str(HERE / "tau2_mcp.py"), "--domain", DOMAIN, "--task-id", str(task.id),
        "--episode-dir", str(episode), "--solo", "--max-calls", "100",
    ]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        return {"task_id": task.id, "error": done.stderr.strip().splitlines()[-1:]}
    run = json.loads((episode / "run.json").read_text())
    return {
        "task_id": task.id,
        "reward": score(task, episode / "trajectory.jsonl"),
        "verdict": run["verdict"],
        "calls": [[c["tool"], c["arguments"]] for c in run["calls"]],
    }


def reference_calls(row: dict) -> list:
    """A reference run's calls, as tool and arguments."""
    return [list(telecom_workflow.unlabel(c)) for c in row["calls"]]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("procedure", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--compare", type=Path, help="scripts/telecom_workflow.py --json, for parity")
    ap.add_argument("--bin", type=Path, default=HERE.parent / "target/release/stretto-procedure")
    ap.add_argument("--python", default=sys.executable, help="the Python with τ²-bench, for the tools")
    ap.add_argument("--tau2", type=Path, default=HERE.parent.parent / "sierra-research" / "tau2-bench")
    ap.add_argument("--jobs", type=int, default=4)
    args = ap.parse_args()
    args.procedure = args.procedure.resolve()
    split = json.loads((args.tau2 / "data/tau2/domains/telecom/split_tasks.json").read_text())
    test = set(split["test"])
    tasks = [t for t in registry.get_tasks_loader(DOMAIN)() if str(t.id) in test]
    with ThreadPoolExecutor(args.jobs) as pool:
        rows = list(pool.map(lambda t: run_one(t, args), tasks))
    summary = {
        "tasks": len(rows),
        "errors": [r for r in rows if "error" in r],
        "passed": sum(r.get("reward") == 1 for r in rows),
        "resolved": sum(r.get("verdict") == "resolved" for r in rows),
        "transferred": sum(r.get("verdict") == "transferred" for r in rows),
        "handed_back": sum(r.get("verdict") == "hand_back" for r in rows),
        "calls_per_episode": round(sum(len(r.get("calls", [])) for r in rows) / max(len(rows), 1), 2),
    }
    if args.compare:
        reference = {r["task_id"]: r for r in json.loads(args.compare.read_text())["runs"]}
        same = [r for r in rows if "error" not in r and r["task_id"] in reference
                and r["calls"] == reference_calls(reference[r["task_id"]])
                and r["reward"] == reference[r["task_id"]]["reward"]
                and r["verdict"] == VERDICTS[reference[r["task_id"]]["own_check"]]]
        summary["same_as_reference"] = len(same)
        summary["differ"] = [r["task_id"] for r in rows if r not in same]
    (args.out / "summary.json").write_text(json.dumps({"summary": summary, "runs": rows}, indent=1) + "\n")
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
