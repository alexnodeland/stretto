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

The flows arm needs `cargo build --release -p stretto-report` and a replay cache holding the goal-free v2 answers. Fill it with `stretto phase0 --oracle jev --questions v2 --predicates data/predicates-v2.json --no-intent`, or import the published bundles (the v2 bundle and its goal-free supplement, in `docs/results/`). Pilot tasks come from the test split, so the habit never trained on them. Each task is judged by the arbiter of its own fold, which never saw it.

The episode directory (`runs/pilot/baseline/task-90/`) holds everything:

- the conversation and tool calls (`trajectory.jsonl`);
- the agent's event stream (`events.jsonl`);
- the proxy's session log (`log/`);
- the scored τ²-bench simulation (`simulation.json`);
- `result.json`, which records the reward, LLM turns, tool calls, parallel-call turns and token usage (and, in the flows arm, the flow's lookups and queries);
- in the flows arm, every flow answer (`flow.jsonl`) and `flow-serve`'s log.

The first smoke run, task 90 (a cancellation), passed the database check:

- 12 agent LLM turns, 7 tool calls and 6 customer replies;
- 77k agent input tokens, 67k of them cached;
- 112 s.

## Check the flow without an LLM

[`check_flow.py`](check_flow.py) replays recorded episodes' tool calls through `tau2_mcp.py` with the flow behind it. It takes an episode from this harness, or τ²-bench results, by default one trial of each test-split task. A recorded call the flow has already made is skipped. A recorded turn whose calls were all skipped is a turn the flow saves, if the agent otherwise behaved the same. A flow lookup the agent never made is a detour. This is the offline projection's measure, but with the live flow's own argument bindings and questions, and no GLM calls.

```bash
PATH=~/.venvs/tau2/bin:$PATH python check_flow.py --out runs/check \
  --results ../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --oracle-cache ../.oracle-cache --flow-oracle jev
```

`--flow-oracle mock` checks the plumbing for free.
