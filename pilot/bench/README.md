# Live runs on other benchmarks

The τ²-bench pilots ran agents live with a flow behind their tools. This folder runs the same setup on other benchmarks' own environments, so what the replays from published trajectories projected ([benchmarks results](../../docs/results/benchmarks-2026-09-27.md)) can be checked on live traffic. The first round, GLM-5.3 and Claude Haiku 4.5 on AgentDojo and BFCL, is [its own results page](../../docs/results/live-benchmarks-2026-09-27.md), with every episode.

- **Agent:** Claude Code with no built-in tools, on GLM-5.3 through Z.ai's GLM Coding Plan endpoint ([`../glm-claude.sh`](../glm-claude.sh)) or on a Claude model ([`../claude-agent.sh`](../claude-agent.sh)), as in the τ²-bench pilots.
- **Tools:** [`bench_mcp.py`](bench_mcp.py) serves one task's tools over MCP, running each call with the benchmark's own code against the task's environment, behind `stretto-proxy`, which records the session. Results are printed as the benchmark prints them to its agents, then as the converter turned the published runs into JSON, so the flows learned from those runs find their arguments at the same paths.
- **Flow arm:** `stretto-proxy --flow FILE --flow-decider reach --flow-threshold 0.3`, the product's own serving path: after each of the agent's calls, the flow's lookups ride in the same result. The flows are the published ones ([`../../docs/results/benchmarks-2026-09-27-flows.tar.gz`](../../docs/results/benchmarks-2026-09-27-flows.tar.gz)), learned from older agents' runs of each benchmark's training tasks. The tasks run here are held out (the first four of every ten, as the converters split them), so no flow saw them.
- **Scoring:** the benchmark's own check. AgentDojo: `utility_from_traces` on the agent's own calls where the task defines it, else `utility` on the agent's answer and the environment before and after. BFCL: its `multi_turn_checker`, which runs the agent's calls turn by turn on fresh instances and compares their state and results with the ground truth's.

| Benchmark | Tasks | User | Flows |
|---|---|---|---|
| AgentDojo v1.2.1 (`agentdojo`), four suites | the 41 held-out user tasks | one instruction | `agentdojo-<suite>.flow.json`, from 11 models released before May 2024 |
| BFCL v4 multi-turn base (`bfcl`) | 20 of the 80 held-out tasks, seeded | its scripted turns, each sent when the agent has answered the last | `bfcl.flow.json`, from 8 models' logged runs |

## Setup

```bash
uv venv --python 3.12 ~/.venvs/bench
VIRTUAL_ENV=~/.venvs/bench uv pip install "agentdojo==0.1.35" "mcp>=1.10,<2" pyyaml
# BFCL: an unpacked bfcl-eval wheel (2026.3.23), without its dependencies
pip download --no-deps bfcl-eval==2026.3.23 -d wheels && unzip -q wheels/bfcl_eval-*.whl -d bfcl
cargo build --release -p stretto-proxy
```

`ZAI_API_KEY` must be set for GLM. A Claude model runs on Claude Code's own endpoint and credentials.

## Check the harness with no model

`--agent scripted` makes each task's ground-truth calls through the same proxy and server, as an MCP client, and answers with the ground truth's output. Every held-out task should pass in both arms, and the flow arm shows the lookups the flow makes on the ground truth's path:

```bash
python run_bench_episode.py agentdojo --suite travel --task user_task_3 --agent scripted --out runs/check
python run_bench_episode.py bfcl --task multi_turn_base_0 --bfcl-dir bfcl --agent scripted --arm reach \
    --flow bfcl.flow.json --out runs/check
```

## Run

```bash
# One episode
python run_bench_episode.py agentdojo --suite travel --task user_task_2 --arm reach \
    --flow agentdojo-travel.flow.json --agent-cli glm --model glm-5.3 --out runs/agentdojo/glm
# Every held-out task in both arms, resumable; runners can share a plan
python run_bench_paired.py agentdojo --suites travel slack banking workspace --agent-cli glm --model glm-5.3 \
    --flows FLOWS --out runs/agentdojo/glm --credit-cap 300
python run_bench_paired.py bfcl --bfcl-dir bfcl --sample 20 --agent-cli claude \
    --model claude-haiku-4-5-20251001 --flows FLOWS --out runs/bfcl/haiku
# Pair the arms task by task, per model and pooled; --set names a group of domains
python analyze_bench.py runs/agentdojo/glm runs/agentdojo/haiku --set "Slack and travel=slack,travel" \
    --json summary.json --md summary.md
```

Each episode's folder holds the agent's event stream (`events.jsonl`), the proxy's session log (`log/`), the flow's decisions (`flow.jsonl`), every call the server ran (`trajectory.jsonl`, the flow's lookups included), the environment's final state (`state.pkl`), and `result.json`: the reward, the LLM turns, the agent's calls, tokens, GLM credits and the flow's lookups.
