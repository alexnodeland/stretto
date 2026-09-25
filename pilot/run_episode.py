"""Run one τ²-bench episode live and score it.

The agent is GLM in Claude Code on Z.ai's coding endpoint (`glm-claude.sh`),
with no built-in tools: its only tools are the task's, served by
`tau2_mcp.py` behind `stretto-proxy`, which records the session. One
Claude Code process lives for the whole episode (stream-json in and out).
Each agent turn ends with a message to the customer; the customer, τ²-bench's
user simulator run through `customer.py`, answers, and the answer is the
agent's next user turn, as in τ²-bench itself.

The episode is scored with τ²-bench's own evaluator on the final database
(the environment check; natural-language assertions need an LLM judge and
are left out).

The guards arm (`--arm guards`) is the same episode with the proxy checking
each of the agent's calls against stretto's policy guards for the domain: a
call an enforced rule refuses never reaches the tools, and the agent gets an
error result that says why. The harness hands the proxy the conversation
(`context.jsonl`), which MCP does not carry.

The flows arm (`--arm flows`) is the same episode with a read-only flow
behind the tools: `stretto flow-serve` is compiled before the agent starts
(from cached System-One answers, goal free) and `tau2_mcp.py` asks it after
every call, making the lookups it names within the same tool response.

The habit arm (`--arm habit`) is the flows arm with the flow deciding on the
habit's prediction alone (`--decider habit`): it never asks the System-One
model.

    python run_episode.py --task-id 90 --out runs/pilot
    python run_episode.py --task-id 90 --out runs/pilot --arm flows \
        --oracle-cache ../.oracle-cache
"""

import argparse
import json
import os
import subprocess
import threading
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

import customer
from tau2.agent.llm_agent import AGENT_INSTRUCTION, SYSTEM_PROMPT
from tau2.data_model.message import AssistantMessage, ToolMessage, UserMessage
from tau2.data_model.simulation import SimulationRun, TerminationReason
from tau2.evaluator.evaluator import EvaluationType, evaluate_simulation
from tau2.registry import registry
from tau2.user.user_simulator import UserSimulator
from tau2.user.user_simulator_base import OUT_OF_SCOPE, STOP, TRANSFER

HERE = Path(__file__).resolve().parent
WRAPPER = HERE / "glm-claude.sh"
PROXY = HERE.parent / "target" / "release" / "stretto-proxy"
STRETTO = HERE.parent / "target" / "release" / "stretto"
TAU2 = Path(os.environ.get("TAU2_DIR", HERE.parent.parent / "sierra-research" / "tau2-bench"))
GREETING = "Hi! How can I help you today?"
STOPS = (STOP, TRANSFER, OUT_OF_SCOPE)
# The arms with a flow behind the tools, and who decides in each.
FLOW_ARMS = {"flows": "arbiter", "habit": "habit"}


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def start_flow(args, episode: Path) -> tuple[subprocess.Popen, str]:
    """Serve the flow (from `--flow FILE`, or compiled on the spot); once it
    listens, return it and its address."""
    serving = [
        "--oracle", args.flow_oracle,
        "--oracle-cache", str(args.oracle_cache),
        "--threshold", str(args.flow_threshold),
        "--decider", getattr(args, "flow_decider", "arbiter"),
        "--max-questions", str(getattr(args, "flow_max_questions", 300)),
        "--log", str(episode / "flow.jsonl"),
    ]
    if getattr(args, "flow", None):
        command = [str(STRETTO), "serve", "--flow", str(args.flow)] + serving
    else:
        command = [
            str(STRETTO), "flow-serve",
            "--tau2", str(args.tau2),
            "--domain", args.domain,
            "--questions", "v2",
            "--predicates", str(HERE.parent / "data" / "predicates-v2.json"),
            "--oracle-budget", "0.5",
        ] + serving
    serve = subprocess.Popen(
        command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True
    )
    log = open(episode / "flow-serve.stderr", "w")
    for line in serve.stderr:
        log.write(line)
        log.flush()
        if "flow ready on " in line:
            address = line.split("flow ready on ", 1)[1].strip()
            break
    else:
        raise RuntimeError(f"flow-serve exited: see {episode / 'flow-serve.stderr'}")

    def drain() -> None:
        for line in serve.stderr:
            log.write(line)
            log.flush()

    threading.Thread(target=drain, daemon=True).start()
    return serve, address


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--domain", default="retail")
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--arm", default="baseline", choices=["baseline", "flows", "habit", "guards"])
    parser.add_argument("--out", type=Path, default=Path("runs/pilot"))
    parser.add_argument("--model", default="glm-5.3")
    parser.add_argument("--max-calls", type=int, default=60)
    parser.add_argument("--max-turns", type=int, default=30)
    parser.add_argument("--tau2", type=Path, default=TAU2, help="τ²-bench checkout (flows arm)")
    parser.add_argument("--oracle-cache", type=Path, help="System-One replay cache (flows arm)")
    parser.add_argument(
        "--flow-oracle", default="jev", choices=["jev", "mock"],
        help="who answers live flow questions (mock: plumbing checks only)",
    )
    parser.add_argument("--flow-threshold", type=float, default=0.3)
    parser.add_argument("--flow", type=Path, help="a compiled flow (`stretto compile`), else compiled here")
    parser.add_argument(
        "--record-context", action="store_true",
        help="hand the proxy the conversation (as the guards arm does), so its session log can "
        "train a flow with `stretto learn`",
    )
    parser.add_argument(
        "--read-only-hints", action="store_true",
        help="mark τ²-bench's read tools readOnlyHint: true (writes false) in tools/list, as a real "
        "server would, so `stretto learn` needs no --manifest (off in the pilots)",
    )
    args = parser.parse_args()
    if args.arm in FLOW_ARMS and not args.oracle_cache:
        parser.error(f"the {args.arm} arm needs --oracle-cache")
    args.flow_decider = FLOW_ARMS.get(args.arm, "arbiter")

    task = next(
        t for t in registry.get_tasks_loader(args.domain)() if str(t.id) == args.task_id
    )
    env = registry.get_env_constructor(args.domain)()
    episode = (args.out / args.arm / f"task-{args.task_id}").resolve()
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "events.jsonl", "tools-state.json", "flow.jsonl", "context.jsonl"):
        (episode / stale).unlink(missing_ok=True)
    context = episode / "context.jsonl"

    def say(role: str, text: str) -> None:
        """Hand the proxy a message of the conversation (guards arm, or when recording it)."""
        if args.arm == "guards" or args.record_context:
            with open(context, "a") as f:
                f.write(json.dumps({"role": role, "content": text}) + "\n")
    serve, flow_address = start_flow(args, episode) if args.arm in FLOW_ARMS else (None, None)
    started, t0 = now(), time.time()

    trajectory = episode / "trajectory.jsonl"

    def record(*messages) -> None:
        with open(trajectory, "a") as f:
            for m in messages:
                f.write(m.model_dump_json() + "\n")

    sim_prompt = UserSimulator(
        llm="unused", instructions=str(task.user_scenario)
    ).system_prompt
    first, usage = customer.reply(sim_prompt, [("agent", GREETING)])
    dialogue = [("agent", GREETING), ("customer", first)]
    customer_usage = [usage]
    record(
        AssistantMessage(role="assistant", content=GREETING),
        UserMessage(role="user", content=first),
    )
    say("assistant", GREETING)
    say("user", first)

    python = Path(subprocess.check_output(["which", "python"], text=True).strip())
    config = {
        "mcpServers": {
            "tau2": {
                "command": str(PROXY),
                "args": [
                    "--record", str(episode / "log"),
                    "--domain", args.domain,
                    "--agent-model", args.model,
                ] + (["--guards"] if args.arm == "guards" else [])
                + (["--context", str(context)] if args.arm == "guards" or args.record_context else []) + [
                    "--",
                    str(python), str(HERE / "tau2_mcp.py"),
                    "--domain", args.domain,
                    "--task-id", args.task_id,
                    "--episode-dir", str(episode),
                    "--max-calls", str(args.max_calls),
                ] + (["--read-only-hints"] if args.read_only_hints else [])
                + (["--flow-address", flow_address] if serve else []),
            }
        }
    }
    (episode / "mcp.json").write_text(json.dumps(config, indent=1))
    system = SYSTEM_PROMPT.format(
        agent_instruction=AGENT_INSTRUCTION, domain_policy=env.get_policy()
    )
    agent = subprocess.Popen(
        [
            str(WRAPPER), "-p", "--bare", "--tools", "",
            "--strict-mcp-config", "--mcp-config", str(episode / "mcp.json"),
            "--allowedTools", "mcp__tau2", "--model", args.model,
            "--system-prompt", system,
            "--input-format", "stream-json", "--output-format", "stream-json",
            "--verbose",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=open(episode / "agent.stderr", "w"),
        text=True,
        bufsize=1,
    )
    events = open(episode / "events.jsonl", "w")

    def send(text: str) -> None:
        line = {"type": "user", "message": {"role": "user", "content": text}}
        agent.stdin.write(json.dumps(line) + "\n")
        agent.stdin.flush()

    def turn() -> str | None:
        """The agent's closing text for this turn, or None if it died."""
        for line in agent.stdout:
            events.write(line)
            event = json.loads(line)
            if event.get("type") == "result":
                return event.get("result") or ""
        return None

    def transferred() -> bool:
        """Whether the agent has handed the customer to a human agent. τ²-bench's
        customer is told to end the conversation then (###TRANSFER###); the
        simulated one here may not, and would talk on to the same agent."""
        return any(
            call.get("name") == "transfer_to_human_agents"
            for m in map(json.loads, trajectory.read_text().splitlines())
            for call in m.get("tool_calls") or []
        )

    ending = None
    send(first)
    for _ in range(args.max_turns):
        text = turn()
        if text is None:
            ending = "agent_error"
            break
        dialogue.append(("agent", text))
        record(AssistantMessage(role="assistant", content=text))
        say("assistant", text)
        state = json.loads((episode / "tools-state.json").read_text())
        if state.get("over_budget"):
            ending = "max_steps"
            break
        if transferred():
            ending = "transfer"
            break
        reply, usage = customer.reply(sim_prompt, dialogue)
        customer_usage.append(usage)
        dialogue.append(("customer", reply))
        record(UserMessage(role="user", content=reply))
        say("user", reply)
        if any(token in reply for token in STOPS):
            ending = "user_stop"
            break
        send(reply)
    else:
        ending = "max_steps"
    agent.stdin.close()
    try:
        agent.wait(timeout=120)
    except subprocess.TimeoutExpired:
        agent.kill()
    events.close()
    if serve:
        serve.terminate()
        serve.wait(timeout=30)

    kinds = {"assistant": AssistantMessage, "user": UserMessage, "tool": ToolMessage}
    messages = [
        kinds[m["role"]].model_validate(m)
        for m in map(json.loads, trajectory.read_text().splitlines())
    ]
    termination = {
        "user_stop": TerminationReason.USER_STOP,
        # As τ²-bench records its customer's ###TRANSFER###.
        "transfer": TerminationReason.USER_STOP,
        "max_steps": TerminationReason.MAX_STEPS,
        "agent_error": TerminationReason.AGENT_ERROR,
    }[ending]
    simulation = SimulationRun(
        id=str(uuid.uuid4()),
        task_id=str(task.id),
        timestamp=started,
        start_time=started,
        end_time=now(),
        duration=time.time() - t0,
        termination_reason=termination,
        messages=messages,
    )
    reward = evaluate_simulation(
        simulation, task, EvaluationType.ENV, solo_mode=False, domain=args.domain
    )
    simulation.reward_info = reward
    (episode / "simulation.json").write_text(simulation.model_dump_json(indent=1))

    # LLM turns and calls, from the agent's own event stream.
    responses: dict[str, int] = {}
    results = []
    for line in (episode / "events.jsonl").read_text().splitlines():
        event = json.loads(line)
        if event.get("type") == "assistant":
            message = event.get("message", {})
            calls = sum(1 for b in message.get("content", []) if b.get("type") == "tool_use")
            key = message.get("id") or str(len(responses))
            responses[key] = responses.get(key, 0) + calls
        elif event.get("type") == "result":
            results.append({k: event.get(k) for k in ("num_turns", "usage", "is_error")})
    tools = json.loads((episode / "tools-state.json").read_text())
    # The proxy's refusals, from its session log (guards arm).
    refusals = []
    for log in sorted((episode / "log").glob("*.jsonl")):
        if log.name.endswith(".flow.jsonl"):
            continue
        calls = {}
        for line in log.read_text().splitlines()[1:]:
            entry = json.loads(line)
            m = entry.get("message") or {}
            if not isinstance(m, dict):
                continue
            if entry.get("from") == "client" and m.get("method") == "tools/call":
                calls[json.dumps(m.get("id"))] = m.get("params", {}).get("name")
            elif entry.get("from") == "proxy" and isinstance(m.get("result"), dict):
                text = " ".join(c.get("text", "") for c in m["result"].get("content", []))
                if m["result"].get("isError") and text.startswith("Refused by the policy check"):
                    refusals.append({"tool": calls.get(json.dumps(m.get("id"))), "reason": text})
    flow_log = episode / "flow.jsonl"
    flow = [json.loads(l) for l in flow_log.read_text().splitlines()] if flow_log.exists() else []
    result = {
        "task_id": str(task.id),
        "arm": args.arm,
        "model": args.model,
        "reward": reward.reward,
        "termination": termination.value,
        "llm_turns": len(responses),
        "tool_turns": sum(1 for n in responses.values() if n > 0),
        "parallel_turns": sum(1 for n in responses.values() if n > 1),
        "tool_calls": tools.get("tool_calls"),
        "flow_lookups": tools.get("flow_lookups", 0),
        "flow_queries": tools.get("flow_queries", 0),
        "flow_answers": {
            action: sum(1 for a in flow if a.get("action") == action)
            for action in ("lookup", "hand_back")
        },
        "refusals": refusals,
        "customer_turns": len(customer_usage),
        "agent_results": results,
        "customer_usage": customer_usage,
        "duration_s": round(time.time() - t0, 1),
    }
    (episode / "result.json").write_text(json.dumps(result, indent=1))
    print(
        "RESULT",
        json.dumps(
            {k: result[k] for k in ("task_id", "arm", "reward", "termination", "llm_turns", "tool_calls", "flow_lookups", "customer_turns", "duration_s")}
            | {"refusals": len(refusals)}
        ),
    )


if __name__ == "__main__":
    main()
