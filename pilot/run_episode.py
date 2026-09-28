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
model. The reach arm (`--arm reach`) decides on the probability that the agent
makes a lookup before its next write (`--decider reach`), with no model either;
it needs a flow learned with those counts (`--flow`).

`--confirm-judge log|enforce` adds the confirmation judge to the guards arm:
the proxy puts each write the guards check for a confirmation to Jev as
well, and in `enforce` refuses the writes it fails. The proxy runs under the
agent's process, so the harness hands it Jev's key in a file only it opens
(`TYPESAFE_API_KEY_FILE`, mode 0600, outside the episode directory, deleted
when the agent exits); the agent's process gets the file's path, never the
key. `--label` names the arm's directory, so two judge settings can share
`--out`.

`--solo` runs τ²-bench's no-user mode (telecom): no customer, the ticket in
τ²-bench's solo system prompt, the phone's tools and `done` served to the
agent, and the episode scored as τ²-bench scores solo runs (its environment
assertions, and the required actions where the task asks). As in τ²-bench,
the episode ends when the agent calls `done`, and a turn that ends with
neither `done` nor a transfer to a human is an agent error. With `--handback
RUNS.json` (`scripts/telecom_workflow.py --json`, or one run of
`stretto-procedure`) the agent takes over from the compiled workflow: the
workflow's calls on the task are made first, with their effects in place,
and the agent is told what they returned and that the workflow's own check
of the ticket's outcome failed (the cascade).

`--agent-cli claude` runs the agent on a Claude model instead
(`claude-agent.sh`, with `--model`), and `--customer-cli claude
--customer-model M` the customer.

`--batch-reads` adds Anthropic's sample prompt for parallel tool calls to the
agent's system prompt (BATCH_READS): the baseline a flow must beat when the
agent is simply asked to make its independent calls at once. It combines with
any arm.

    python run_episode.py --task-id 90 --out runs/pilot
    python run_episode.py --task-id 90 --out runs/pilot --arm flows \
        --oracle-cache ../.oracle-cache
"""

import argparse
import atexit
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

import customer
from tau2.agent.llm_agent import AGENT_INSTRUCTION, AGENT_SOLO_INSTRUCTION, SYSTEM_PROMPT, SYSTEM_PROMPT_SOLO
from tau2.data_model.message import AssistantMessage, ToolMessage, UserMessage
from tau2.data_model.simulation import SimulationRun, TerminationReason
from tau2.evaluator.evaluator import EvaluationType, evaluate_simulation
from tau2.registry import registry
from tau2.user.user_simulator import UserSimulator
from tau2.user.user_simulator_base import OUT_OF_SCOPE, STOP, TRANSFER

HERE = Path(__file__).resolve().parent
# The wrapper each agent CLI runs through: GLM on Z.ai, or a Claude model.
WRAPPERS = customer.WRAPPERS
PROXY = HERE.parent / "target" / "release" / "stretto-proxy"
STRETTO = HERE.parent / "target" / "release" / "stretto"
TAU2 = Path(os.environ.get("TAU2_DIR", HERE.parent.parent / "sierra-research" / "tau2-bench"))
GREETING = "Hi! How can I help you today?"
STOPS = (STOP, TRANSFER, OUT_OF_SCOPE)
# The arms with a flow behind the tools, and who decides in each.
FLOW_ARMS = {"flows": "arbiter", "habit": "habit", "reach": "reach"}
# τ²-bench's solo agent never speaks; its first turn has no message to answer.
SOLO_START = "Start on the ticket."
# `--batch-reads`: the sample prompt for maximum parallel efficiency in
# Anthropic's prompting guide ("Optimize parallel tool calling",
# platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices),
# word for word, as one paragraph.
BATCH_READS = (
    "<use_parallel_tool_calls>\n"
    "If you intend to call multiple tools and there are no dependencies between the tool calls, "
    "make all of the independent tool calls in parallel. Prioritize calling tools simultaneously "
    "whenever the actions can be done in parallel rather than sequentially. For example, when "
    "reading 3 files, run 3 tool calls in parallel to read all 3 files into context at the same "
    "time. Maximize use of parallel tool calls where possible to increase speed and efficiency. "
    "However, if some tool calls depend on previous calls to inform dependent values like the "
    "parameters, do NOT call these tools in parallel and instead call them sequentially. Never "
    "use placeholders or guess missing parameters in tool calls.\n"
    "</use_parallel_tool_calls>"
)


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def start_flow(args, episode: Path) -> tuple[subprocess.Popen, str]:
    """Serve the flow (from `--flow FILE`, or compiled on the spot); once it
    listens, return it and its address."""
    serving = [
        "--oracle", args.flow_oracle,
    ] + (["--oracle-cache", str(args.oracle_cache)] if args.oracle_cache else []) + [
        "--threshold", str(args.flow_threshold),
        "--decider", getattr(args, "flow_decider", "arbiter"),
        "--max-questions", str(getattr(args, "flow_max_questions", 300)),
        "--log", str(episode / "flow.jsonl"),
    ]
    if getattr(args, "explore", None) is not None:
        serving += ["--explore", str(args.explore), "--explore-seed", str(getattr(args, "explore_seed", 0))]
    if getattr(args, "flow_surprise", None) is not None:
        serving += ["--surprise", args.flow_surprise]
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


def handback(runs: Path, task, env) -> tuple[list[dict], str]:
    """The compiled workflow's calls on a task, and the note the agent gets
    when the workflow hands the task over: each call with what it returned
    (made here on the task's initial state, as the tools will make them), and
    the workflow's own check of the ticket's outcome, made last."""
    sys.path.insert(0, str(HERE.parent / "scripts"))
    import telecom_workflow
    from tau2.data_model.message import ToolCall

    data = json.loads(runs.read_text())
    if "verdict" in data:
        # One run of `stretto-procedure`: its calls, then its check's read.
        calls = [{"name": c["tool"], "arguments": c["arguments"]} for c in data["calls"]]
        probe = data["check"][0] if data.get("check") else None
    else:
        row = next((r for r in data["runs"] if r["task_id"] == str(task.id)), None)
        if row is None:
            raise SystemExit(f"no workflow run for task {task.id} in {runs}")
        calls = [dict(zip(("name", "arguments"), telecom_workflow.unlabel(c))) for c in row["calls"]]
        probe = next((p for phrase, p, *_ in telecom_workflow.RESOLVED if phrase in (task.ticket or "").lower()), None)
    if probe:
        calls.append({"name": probe, "arguments": {}})
    init = task.initial_state
    env.set_state(
        initialization_data=init.initialization_data if init else None,
        initialization_actions=init.initialization_actions if init else None,
        message_history=[],
    )
    lines = []
    for i, c in enumerate(calls):
        result = env.get_response(ToolCall(id=f"prefix_{i}", name=c["name"], arguments=c["arguments"], requestor="assistant"))
        lines.append(f"{i + 1}. {c['name']} {json.dumps(c['arguments'])}\n{result.content or ''}")
    note = (
        "An automated procedure worked on this ticket before you. It made these tool calls, in order; "
        "their effects are in place. The last is its check of the outcome the ticket states, which "
        "found the issue not yet resolved.\n\n" + "\n\n".join(lines)
        + "\n\nContinue from here: solve the ticket, then call `done`."
    )
    return calls, note


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--domain", default="retail")
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--arm", default="baseline", choices=["baseline", "flows", "habit", "reach", "guards"])
    parser.add_argument("--out", type=Path, default=Path("runs/pilot"))
    parser.add_argument("--model", default="glm-5.3", help="the agent's model")
    parser.add_argument(
        "--agent-cli", default="glm", choices=sorted(WRAPPERS),
        help="the agent's Claude Code: `glm` (glm-claude.sh, Z.ai) or `claude` (claude-agent.sh)",
    )
    parser.add_argument("--customer-cli", default="glm", choices=sorted(WRAPPERS), help="the customer's, likewise")
    parser.add_argument("--customer-model", help="the customer's model (default: PILOT_CUSTOMER_MODEL, else glm-5.3)")
    parser.add_argument("--label", help="the arm's directory under --out (default: the arm's name)")
    parser.add_argument("--max-calls", type=int, default=60)
    parser.add_argument("--max-turns", type=int, default=30)
    parser.add_argument("--tau2", type=Path, default=TAU2, help="τ²-bench checkout (flows arm)")
    parser.add_argument("--oracle-cache", type=Path, help="System-One replay cache (flows arm)")
    parser.add_argument(
        "--flow-oracle", default="jev", choices=["jev", "mock"],
        help="who answers live flow questions (mock: plumbing checks only)",
    )
    parser.add_argument("--flow-threshold", type=float, default=0.3)
    parser.add_argument(
        "--flow-surprise", metavar="off|NATS",
        help="serve the flow with this surprise threshold, or without its gate (`stretto serve --surprise`)",
    )
    parser.add_argument("--flow", type=Path, help="a compiled flow (`stretto compile`), else compiled here")
    parser.add_argument(
        "--record-context", action="store_true",
        help="hand the proxy the conversation (as the guards arm does), so its session log can "
        "train a flow with `stretto learn`",
    )
    parser.add_argument(
        "--confirm-judge", choices=["log", "enforce"],
        help="guards arm: put each write the guards check for a confirmation to the System-One model "
        "too (stretto-proxy --confirm-judge); needs --oracle-cache",
    )
    parser.add_argument("--confirm-second", choices=["proposed", "described"], help="and ask the second question")
    parser.add_argument(
        "--confirm-second-shadow", action="store_true",
        help="only log the second question's answer (stretto-proxy --confirm-second-shadow)",
    )
    parser.add_argument(
        "--judge-oracle", default="jev", choices=["jev", "replay", "mock"],
        help="who answers the judge's questions (mock: plumbing checks only)",
    )
    parser.add_argument("--solo", action="store_true", help="τ²-bench's no-user mode (telecom): no customer")
    parser.add_argument(
        "--handback", type=Path, metavar="RUNS.json",
        help="solo: take over from the compiled workflow's run on the task (scripts/telecom_workflow.py --json, "
        "or a stretto-procedure run)",
    )
    parser.add_argument(
        "--batch-reads", action="store_true",
        help="add Anthropic's sample prompt for parallel tool calls to the agent's system prompt "
        "(BATCH_READS): the prompting baseline",
    )
    parser.add_argument(
        "--read-only-hints", action="store_true",
        help="mark τ²-bench's read tools readOnlyHint: true (writes false) in tools/list, as a real "
        "server would, so `stretto learn` needs no --manifest (off in the pilots)",
    )
    args = parser.parse_args()
    # The mock oracle never reads a cache; nor is it asked by the habit or
    # reach decider, so a served flow file needs no cache with it.
    if args.arm in FLOW_ARMS and not args.oracle_cache and args.flow_oracle != "mock":
        parser.error(f"the {args.arm} arm needs --oracle-cache, or --flow-oracle mock")
    if args.arm in FLOW_ARMS and not args.oracle_cache and not args.flow:
        parser.error(f"the {args.arm} arm compiles a flow here: give it --oracle-cache, or a --flow")
    if args.confirm_judge and (args.arm != "guards" or not args.oracle_cache):
        parser.error("--confirm-judge needs --arm guards and --oracle-cache")
    if args.confirm_second and not args.confirm_judge:
        parser.error("--confirm-second needs --confirm-judge")
    if args.confirm_second_shadow and not args.confirm_second:
        parser.error("--confirm-second-shadow needs --confirm-second")
    if (args.agent_cli == "claude") == args.model.startswith("glm"):
        parser.error(f"--model {args.model} does not run on --agent-cli {args.agent_cli}")
    if args.customer_cli == "claude" and not args.customer_model:
        parser.error("--customer-cli claude needs --customer-model")
    if args.handback and not args.solo:
        parser.error("--handback needs --solo")
    if args.solo and (args.arm == "guards" or args.record_context):
        parser.error("--solo has no conversation for the guards arm or --record-context")
    if args.solo and args.batch_reads:
        parser.error("--batch-reads adds to the conversational system prompt, not --solo's")
    args.flow_decider = FLOW_ARMS.get(args.arm, "arbiter")

    task = next(
        t for t in registry.get_tasks_loader(args.domain)() if str(t.id) == args.task_id
    )
    env = registry.get_env_constructor(args.domain)(solo_mode=args.solo)
    episode = (args.out / (args.label or args.arm) / f"task-{args.task_id}").resolve()
    episode.mkdir(parents=True, exist_ok=True)
    for stale in ("trajectory.jsonl", "events.jsonl", "tools-state.json", "flow.jsonl", "context.jsonl", "prefix.json"):
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
    def customer_reply(dialogue: list[tuple[str, str]]) -> tuple[str, dict]:
        return customer.reply(sim_prompt, dialogue, cli=args.customer_cli, model=args.customer_model)

    prefix = None
    if args.solo:
        # No customer: the agent's first message answers none.
        first, dialogue, customer_usage = SOLO_START, [], []
        if args.handback:
            calls, first = handback(args.handback, task, registry.get_env_constructor(args.domain)(solo_mode=True))
            prefix = episode / "prefix.json"
            prefix.write_text(json.dumps(calls, indent=1))
    else:
        first, usage = customer_reply([("agent", GREETING)])
        dialogue = [("agent", GREETING), ("customer", first)]
        customer_usage = [usage]
        record(
            AssistantMessage(role="assistant", content=GREETING),
            UserMessage(role="user", content=first),
        )
        say("assistant", GREETING)
        say("user", first)

    # The tools run in this interpreter's environment, which has τ²-bench.
    python = Path(sys.executable)
    judge, server_env = [], {}
    if args.confirm_judge:
        judge = [
            "--confirm-judge", args.confirm_judge,
            "--oracle", args.judge_oracle, "--oracle-cache", str(args.oracle_cache.resolve()),
        ] + (["--confirm-second", args.confirm_second] if args.confirm_second else []) + (
            ["--confirm-second-shadow"] if args.confirm_second_shadow else []
        )
    key_file = None
    if args.confirm_judge and args.judge_oracle == "jev":
        key = os.environ.get("TYPESAFE_API_KEY")
        if not key:
            raise SystemExit("--confirm-judge with --judge-oracle jev needs TYPESAFE_API_KEY")
        # mkstemp makes the file 0600; it lives outside the episode directory.
        fd, key_file = tempfile.mkstemp(prefix="stretto-jev-")
        with os.fdopen(fd, "w") as f:
            f.write(key)
        atexit.register(lambda: Path(key_file).unlink(missing_ok=True))
        server_env["TYPESAFE_API_KEY_FILE"] = key_file
    config = {
        "mcpServers": {
            "tau2": {
                "command": str(PROXY),
                "args": [
                    "--record", str(episode / "log"),
                    "--domain", args.domain,
                    "--agent-model", args.model,
                ] + (["--guards"] if args.arm == "guards" else [])
                + (["--context", str(context)] if args.arm == "guards" or args.record_context else [])
                + judge + [
                    "--",
                    str(python), str(HERE / "tau2_mcp.py"),
                    "--domain", args.domain,
                    "--task-id", args.task_id,
                    "--episode-dir", str(episode),
                    "--max-calls", str(args.max_calls),
                ] + (["--read-only-hints"] if args.read_only_hints else [])
                + (["--flow-address", flow_address] if serve else [])
                + (["--solo"] if args.solo else []) + (["--prefix", str(prefix)] if prefix else []),
            } | ({"env": server_env} if server_env else {})
        }
    }
    (episode / "mcp.json").write_text(json.dumps(config, indent=1))
    if args.solo:
        system = SYSTEM_PROMPT_SOLO.format(
            agent_instruction=AGENT_SOLO_INSTRUCTION.format(stop_function_name="done", stop_token="###STOP###"),
            domain_policy=env.get_policy(),
            ticket=task.ticket,
        )
    else:
        system = SYSTEM_PROMPT.format(
            agent_instruction=AGENT_INSTRUCTION, domain_policy=env.get_policy()
        )
        if args.batch_reads:
            system += "\n" + BATCH_READS
    agent = subprocess.Popen(
        [
            str(WRAPPERS[args.agent_cli]), "-p", "--bare", "--tools", "",
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
            if event.get("type") == "system" and event.get("subtype") == "init":
                # Without its tools the agent would run on, spending turns.
                failed = [m["name"] for m in event.get("mcp_servers", []) if m.get("status") == "failed"]
                if failed:
                    agent.kill()
                    raise SystemExit(f"MCP server {failed} failed to start: see {episode / 'log'}")
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
        if args.solo:
            state = json.loads((episode / "tools-state.json").read_text())
            if state.get("over_budget"):
                ending = "max_steps"
            elif state.get("stopped") or transferred():
                # `done`, or a transfer: the policy's own ending for what the
                # agent may not fix, as the compiled workflow ends there too.
                ending = "agent_stop"
            else:
                # τ²-bench's solo agent may only call tools: a message with
                # neither a call nor the stop ends the episode as an error.
                record(AssistantMessage(role="assistant", content=text))
                ending = "solo_error"
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
        reply, usage = customer_reply(dialogue)
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
    if key_file:
        Path(key_file).unlink(missing_ok=True)
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
        "agent_stop": TerminationReason.AGENT_STOP,
        "solo_error": TerminationReason.AGENT_ERROR,
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
    # Solo runs are scored as τ²-bench scores them: the environment
    # assertions, and the required actions where the task's basis has them.
    reward = evaluate_simulation(
        simulation, task, EvaluationType.ALL if args.solo else EvaluationType.ENV, solo_mode=args.solo, domain=args.domain
    )
    simulation.reward_info = reward
    (episode / "simulation.json").write_text(simulation.model_dump_json(indent=1))

    # LLM turns and calls, from the agent's own event stream.
    responses: dict[str, int] = {}
    results = []
    # A Claude Code subscription's last reported windows (a Claude agent only).
    rate_limit = None
    for line in (episode / "events.jsonl").read_text().splitlines():
        event = json.loads(line)
        if event.get("type") == "rate_limit_event":
            rate_limit = event.get("rate_limit_info")
        if event.get("type") == "assistant":
            message = event.get("message", {})
            calls = sum(1 for b in message.get("content", []) if b.get("type") == "tool_use")
            key = message.get("id") or str(len(responses))
            responses[key] = responses.get(key, 0) + calls
        elif event.get("type") == "result":
            # `usage` is the turn's; `modelUsage` and `total_cost_usd` (at list
            # prices) are the session's so far.
            results.append({
                k: event.get(k)
                for k in ("num_turns", "usage", "is_error", "modelUsage", "total_cost_usd", "duration_ms", "duration_api_ms")
            })
    tools = json.loads((episode / "tools-state.json").read_text())
    # The proxy's refusals, from its session log (guards arm).
    refusals = []
    judgments = []
    for log in sorted((episode / "log").glob("*.jsonl")):
        if log.name.endswith(".confirm.jsonl"):
            judgments += [json.loads(line) for line in log.read_text().splitlines()]
            continue
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
        "label": args.label or args.arm,
        "model": args.model,
        "agent_cli": args.agent_cli,
        "customer_cli": args.customer_cli,
        "customer_model": args.customer_model or customer.MODEL,
        "judge": {
            "mode": args.confirm_judge, "second": args.confirm_second,
            "second_shadow": args.confirm_second_shadow, "oracle": args.judge_oracle,
        } if args.confirm_judge else None,
        "reward": reward.reward,
        "termination": termination.value,
        "ending": ending,
        "solo": args.solo,
        "handback": str(args.handback) if args.handback else None,
        "workflow_calls": tools.get("prefix_calls", 0),
        "llm_turns": len(responses),
        "tool_turns": sum(1 for n in responses.values() if n > 0),
        "parallel_turns": sum(1 for n in responses.values() if n > 1),
        "tool_calls": tools.get("tool_calls"),
        "stopped": tools.get("stopped", False),
        "flow_lookups": tools.get("flow_lookups", 0),
        "flow_queries": tools.get("flow_queries", 0),
        "flow_answers": {
            action: sum(1 for a in flow if a.get("action") == action)
            for action in ("lookup", "hand_back")
        },
        "refusals": refusals,
        "judgments": [
            {k: j.get(k) for k in ("tool", "p_yes", "p_second", "fails", "unknown", "enforced", "error") if k in j}
            for j in judgments
        ],
        "customer_turns": len(customer_usage),
        "agent_results": results,
        "customer_usage": customer_usage,
        "batch_reads": args.batch_reads,
        "rate_limit": rate_limit,
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
