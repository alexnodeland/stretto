# Live pilot harness

Runs τ²-bench episodes live, with the agent in an MCP host, so stretto can
be measured on real traffic rather than replays.

- **Agent:** GLM in Claude Code, on Z.ai's GLM Coding Plan endpoint. That is the setup Z.ai documents for its coding plan, which may only be used in supported tools. [`glm-claude.sh`](glm-claude.sh) runs Claude Code with a clean environment, so a Claude Code session that starts it is not affected. The agent has no built-in tools; it gets τ²-bench's system prompt (instructions and domain policy).
- **Tools:** [`tau2_mcp.py`](tau2_mcp.py) serves one task's tools over MCP, behind [`stretto-proxy`](../crates/stretto-proxy), which records the session. It appends every call and result to the episode's `trajectory.jsonl` in τ²-bench's message format.
- **Customer:** τ²-bench's user-simulator prompt, answered by GLM through the same route ([`customer.py`](customer.py)), one short request per reply.
- **Episode:** [`run_episode.py`](run_episode.py) keeps one Claude Code process alive for the whole episode (stream-json in and out). Each agent turn ends with a message to the customer, and the customer's reply is the next turn, as in τ²-bench.
- **Scoring:** τ²-bench's own evaluator checks the final database. Natural-language assertions need an LLM judge and are left out, so the reward here is the database check alone.
- **Flows arm:** the same episode with a read-only flow behind the tools (RFC-001 §3.13). `stretto flow-serve` compiles the flow before the agent starts, from cached System-One answers, goal free (no one names the episode's goal live). After each of the agent's calls, `tau2_mcp.py` asks it what comes next, makes the lookup it names, records it like any other call and asks again, until the flow hands back. The agent gets its own result followed by the flow's lookups in the same tool response, so those lookups cost it no turns. The flow asks Jev one question per step (`TYPESAFE_API_KEY`, read by `flow-serve` only, never by the agent's process).

## Setup

```bash
uv venv --python 3.12 ~/.venvs/tau2
VIRTUAL_ENV=~/.venvs/tau2 uv pip install -e ../../tau2-bench websockets "mcp>=1.10,<2"
cargo build --release -p stretto-proxy
```

`ZAI_API_KEY` must be set; `GLM_CLAUDE_CONFIG_DIR` optionally keeps the nested Claude Code's state apart.

## Run one episode

```bash
PATH=~/.venvs/tau2/bin:$PATH python run_episode.py --task-id 90 --out runs/pilot
PATH=~/.venvs/tau2/bin:$PATH python run_episode.py --task-id 90 --out runs/pilot \
  --arm flows --oracle-cache ../.oracle-cache
```

The flows arm needs `cargo build --release -p stretto-report` and a replay cache holding the goal-free v2 answers. `--arm habit` runs the same flow on the habit alone; it still compiles from the cache, but asks Jev nothing live. Fill it with `stretto phase0 --oracle jev --questions v2 --predicates data/predicates-v2.json --no-intent`, or import the published bundles (the v2 bundle and its goal-free supplement, in `docs/results/`). Pilot tasks come from the test split, so the habit never trained on them. Each task is judged by the arbiter of its own fold, which never saw it.

The episode directory (`runs/pilot/baseline/task-90/`) holds everything:

- the conversation and tool calls (`trajectory.jsonl`);
- the agent's event stream (`events.jsonl`);
- the proxy's session log (`log/`);
- the scored τ²-bench simulation (`simulation.json`);
- `result.json`, which records the reward, LLM turns, tool calls, parallel-call turns and token usage (and, in the flows arm, the flow's lookups and queries);
- in the flows arm, every flow answer (`flow.jsonl`) and `flow-serve`'s log.

## Results so far

**Replay check, no LLM.** Before any flows episode, `check_flow.py` replayed GLM-5's recorded retail test episodes (trial 0, 40 tasks) through the live flow. It used real Jev answers:

| Flow | LLM turns saved | Flow lookups that were the agent's own | Episodes with a detour |
|---|---|---|---|
| Acts on the tool's probability (p ≥ 0.3) | 77 of 347 (22.2%) | 151 of 193 (78%) | 12 of 40 |
| Acts on the tool's probability times the binding's agreement (p ≥ 0.3) | 77 of 347 (22.2%) | 151 of 169 (89%) | 8 of 40 |

Over all four recorded trials (160 episodes, 1,384 LLM turns), the second flow saves 294 turns (21.2%), and 91% of its lookups are the agent's own; 27 episodes have a detour. The offline projection for the same episodes (goal free, lookup first at p ≥ 0.3) saves 281 turns (20.3%), with a detour in 45 episodes. The first flow's detours were mostly product walks: it looked up every product in an order, where agents look up only the products the customer asks about. The second flow multiplies by how often its argument binding picked the agent's own values in training. That is 92% for unmentioned orders and 60% for unmentioned products. With it, every dropped lookup was a detour. Most of the live flow's questions were cache hits: where it follows the agent's path, it asks byte-identical questions to the offline run.

**Smoke runs, GLM-5.3.** One episode per arm on task 90, a cancellation. Both passed the database check:

| Arm | LLM turns | Agent's own calls | Flow lookups | Agent input tokens (cached) | GLM requests, agent + customer | Time |
|---|---|---|---|---|---|---|
| Baseline | 12 | 7 | — | 76.6k (66.9k) | 12 + 6 | 112 s |
| Flows | **8** | 3 | 4 | **55.4k** (46.0k) | 8 + 6 | 105 s |

In the flows arm, the agent's first call found the user. The flow then looked up the user's details and all three orders in the same response, and handed back at the product step (p = 0.10). The agent used those results without repeating any of them. It looked up the camera itself, and the conversation went as in the baseline. This is one episode, so it shows that the mechanism works live, not how much it saves.

**Paired pilot, GLM-5.3.** Ten retail test tasks, drawn at random with `run_pilot.py` (seed 7), ran once in each arm. See [the summary](../docs/results/pilot-2026-09-24.md):

- 110 LLM turns without the flow and 84 with it, 23.6% fewer: 2.6 per episode (95% interval 1.0 to 4.2), with fewer turns in 9 of 10 pairs.
- Agent input tokens fell 21%. The flow made 26 lookups, and the agent repeated 3 of them.
- 8 of 10 passed the database check in each arm. The two failures were the same agent error in both.
- 236.5 Z.ai credits at the off-peak rate, 16% less in the flows arm.

**Paired pilot, airline.** The same setup on ten airline test tasks (`run_pilot.py --domain airline`, seed 7). See [the summary](../docs/results/pilot-airline-2026-09-24.md):

- 127 LLM turns without the flow and 105 with it, 17.3% fewer: 2.2 per episode (95% interval 0.5 to 3.9), with fewer turns in 7 of 10 pairs.
- Agent input tokens fell 15%. The flow acted on every task, with 24 lookups, and the agent repeated none of them.
- 8 of 10 passed the database check without the flow and 9 of 10 with it. None of the failures came from a flow decision. In the one in the flows arm, the agent upgraded a basic-economy ticket and then changed its flights, quoting a net $81 against a $100 limit the task counts differently.
- GLM-5.3 in Claude Code calls tools one at a time (parallel calls in 3.8% of tool turns), unlike GLM-5 in τ²-bench's harness (45%). So airline saved far more than the offline projection for GLM-5 (under 5%).
- 334 Z.ai credits, 11% less in the flows arm.

**Guards pilot, airline.** The guards arm (`run_episode.py --arm guards`) runs the tools behind `stretto-proxy --guards`, which refuses a write an enforced policy rule fails. It ran on the four airline test tasks where the guard audit finds the 2025 agents' refused writes concentrated (35, 45, 32, 48), plus four harm checks against the airline pilot's baselines. See [the summary](../docs/results/pilot-guards-2026-09-24.md):

- GLM-5.3 passed all four main tasks in both arms, and the guards refused nothing. It never tried the cancellations the policy forbids.
- With the guards on, 7 of the 8 episodes passed; the failure (task 31) was the agent's cost error from the airline pilot, with nothing refused.
- The proxy checked 9 writes live and passed them all, among them task 32's upgrade-then-change, which a rule briefly changed that morning would have refused.
- 223 Z.ai credits.

**Habit-only pilot, retail.** The habit arm (`run_episode.py --arm habit`) is the flows arm with the flow deciding on the habit alone (`--decider habit`), never asking Jev. It ran on the retail pilot's ten tasks, paired with that pilot's no-flow and D0 episodes. See [the summary](../docs/results/pilot-habit-2026-09-24.md):

- 79 LLM turns, against 110 with no flow and 84 with D0: 28.2% fewer than with no flow, with fewer turns in all ten pairs. Against D0, a change of −0.5 turns per episode (95% interval −1.4 to +0.4).
- 37 flow lookups, none repeated by the agent; 9 of 10 passed.
- 109 Z.ai credits.

## Check the flow without an LLM

[`check_flow.py`](check_flow.py) replays recorded episodes' tool calls through `tau2_mcp.py` with the flow behind it. It takes an episode from this harness, or τ²-bench results, by default one trial of each test-split task. A recorded call the flow has already made is skipped. A recorded turn whose calls were all skipped is a turn the flow saves, if the agent otherwise behaved the same. A flow lookup the agent never made is a detour. This is the offline projection's measure, but with the live flow's own argument bindings and questions, and no GLM calls.

```bash
PATH=~/.venvs/tau2/bin:$PATH python check_flow.py --out runs/check \
  --results ../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --oracle-cache ../.oracle-cache --flow-oracle jev
```

`--flow-oracle mock` checks the plumbing for free, and `--flow-oracle replay` reads the System-One answers from the cache only.

`--flow-decider habit` replays the habit alone: the flow never asks the System-One model, and acts on the habit's prediction by the same rule (arm C). At a high `--flow-threshold`, such as 0.99, it goes on only where training shows no branch, and hands every branch back, as TraceCompiler does.
