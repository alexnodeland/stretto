# Live pilot harness

Runs τ²-bench episodes live, with the agent in an MCP host, so stretto can
be measured on real traffic rather than replays.

- **Agent:** GLM in Claude Code, on Z.ai's GLM Coding Plan endpoint. That is the setup Z.ai documents for its coding plan, which may only be used in supported tools. [`glm-claude.sh`](glm-claude.sh) runs Claude Code with a clean environment, so a Claude Code session that starts it is not affected. The agent has no built-in tools; it gets τ²-bench's system prompt (instructions and domain policy).
- **Tools:** [`tau2_mcp.py`](tau2_mcp.py) serves one task's tools over MCP, behind [`stretto-proxy`](../crates/stretto-proxy), which records the session. It appends every call and result to the episode's `trajectory.jsonl` in τ²-bench's message format.
- **Customer:** τ²-bench's user-simulator prompt, answered by GLM through the same route ([`customer.py`](customer.py)), one short request per reply.
- **Episode:** [`run_episode.py`](run_episode.py) keeps one Claude Code process alive for the whole episode (stream-json in and out). Each agent turn ends with a message to the customer, and the customer's reply is the next turn, as in τ²-bench.
- **Scoring:** τ²-bench's own evaluator checks the final database. Natural-language assertions need an LLM judge and are left out, so the reward here is the database check alone.

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
```

The episode directory (`runs/pilot/baseline/task-90/`) holds everything:

- the conversation and tool calls (`trajectory.jsonl`);
- the agent's event stream (`events.jsonl`);
- the proxy's session log (`log/`);
- the scored τ²-bench simulation (`simulation.json`);
- `result.json`, which records the reward, LLM turns, tool calls, parallel-call turns and token usage.

The first smoke run, task 90 (a cancellation), passed the database check:

- 12 agent LLM turns, 7 tool calls and 6 customer replies;
- 77k agent input tokens, 67k of them cached;
- 112 s.
