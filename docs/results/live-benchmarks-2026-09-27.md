# Live on AgentDojo and BFCL: GLM-5.3 and Claude Haiku 4.5

The [benchmarks page](benchmarks-2026-09-27.md) replayed flows on six more benchmarks from their published trajectories. A replay answers the flow's lookups from the record, so it cannot show what an agent does when a lookup's result reaches it early. This round runs two of those benchmarks live, in their own environments: AgentDojo's four suites and BFCL's multi-turn tasks. Two agents run each held-out task twice, once with no flow and once with the published flow behind their tools, and each task is scored by the benchmark's own check.

- **Agents:** GLM-5.3 on Z.ai's GLM Coding Plan endpoint, and Claude Haiku 4.5 on Claude Code's own endpoint. Both run in Claude Code 2.1.283 with no built-in tools.
- **Tools:** the task's own. [`pilot/bench/bench_mcp.py`](../../pilot/bench/bench_mcp.py) runs each call with the benchmark's code against the task's environment, over MCP, behind `stretto-proxy`.
- **Flow arm:** `stretto-proxy --flow FILE --flow-decider reach --flow-threshold 0.3`. After each of the agent's calls, the flow's lookups ride in the same result. The flows are byte for byte the published ones ([archive](benchmarks-2026-09-27-flows.tar.gz)). They were learned from older agents' runs of each benchmark's training tasks, 11 models in AgentDojo and 8 in BFCL, and never saw these tasks or these agents.
- **Tasks:**
  - AgentDojo v1.2.1: all 41 held-out user tasks (workspace 16, Slack 9, travel 8, banking 8), with AgentDojo's default system message.
  - BFCL v4 multi-turn base: 20 of its 80 held-out tasks, drawn with seed 7, each user turn sent when the agent has answered the last.

  Held out means the first four of every ten, as the converters split them.
- **Scoring:** the benchmark's own check.
  - AgentDojo uses `utility_from_traces` on the agent's calls where the task defines it, else `utility` on its answer and the environment.
  - BFCL uses `multi_turn_checker`, which replays the agent's calls turn by turn on fresh instances and compares their state and results with the ground truth's.
- **Harness check:** with no model, the harness makes the ground truth's calls through the same proxy, server and flow (`--agent scripted`). These pass all 41 AgentDojo tasks and all 80 BFCL held-out tasks, in both arms.
- **Runs:**
  - Each pair was run once. The order of the two arms was drawn per task, and 244 episodes were run in all. A promotion test on Slack added 42 more (below).
  - Every episode ended with the agent's own answer, with no timeout and no call budget reached.
  - GLM used 508.8 credits: 209.6 on AgentDojo, 252.9 on BFCL, 39.0 on the promotion test and 7.3 on smoke tests. Haiku ran on a subscription and is reported in tokens.
- **Intervals:**
  - Intervals are 95%, from a bootstrap over tasks that draws a task's pairs for both models together.
  - The sign test is exact and two-sided, on the pairs whose turns differ.
  - This page reports many groups, so about one interval in twenty should miss by chance.

## Findings

- **Where the replay found reads to take, the flow cut LLM turns by a tenth.**
  - The replay found reads to take in Slack and travel, and none in banking or workspace, so the split below was fixed before these runs.
  - In Slack and travel, the agents took 124 LLM turns with the flow against 138 without it: 10.1% fewer (5.8% to 14.0%).
  - 14 of those 34 pairs took fewer turns with the flow and one took more (sign test p = 0.001). Both models agree: 12.5% fewer for Haiku 4.5 and 8.1% for GLM-5.3.
  - In banking and workspace the flow made one lookup in 48 episodes, and the arms differed by 2.1% (−3.6% to 8.7%).
  - Over all of AgentDojo: 6.0% fewer turns (2.1% to 9.9%, 19 pairs fewer and 6 more, p = 0.015). The replay projected 7.2% for other agents.
  - Of the 14 turns saved in Slack and travel, 10 came from the 15 pairs in which the agent used one of the flow's lookups. The other 4 came from pairs in which it used none, within the run-to-run variation.
- **Passes did not move.** AgentDojo passed 68 of 82 pairs without the flow and 69 with it; Slack and travel passed 27 and 27. BFCL passed 29 and 29 of 40.
- **BFCL shows no effect, which is what the replay projected.**
  - The replay projected 1.6% of turns. Live, the agents took 451 turns with the flow against 444 without it: −1.6% (−6.3% to 4.2%).
  - Most of that gap comes from the 25 pairs in which the flow made no lookup, so both arms ran under identical conditions. They differed by 3.9% more turns.
  - With one run per arm, telling a 1.6% effect from that noise would take about 900 pairs.
- **Slack's savings and detours came from one walk.** AgentDojo's flows made 88 lookups. In 36 of them, the agent had made the same call in its episode without the flow.
  - After `get_channels`, the Slack flow reads every channel in turn, as the older agents did on the tasks that go through all of them.
  - On the two held-out tasks that do, both agents read all four channels themselves, and the walk spared them that turn.
  - On five others, the agents listed the channels to post or invite, and read none. There the walk made 41 of Slack's 43 detours.
  - No Slack pair took more turns.
- **Promotion cannot split the walk.** `stretto promote`, run on each agent's own sessions of Slack's 12 training tasks, held back the walk's first read. That removed all 43 detours, and with them the saved turns: 72 turns against 75 without a flow, where the walk took 68. Which kind of task it is, the request says, and an MCP proxy does not see the request.
- **Cost is not resolved.**
  - Agent input tokens fell 5.4% in AgentDojo, and 8.3% in Slack and travel.
  - Priced as billed, cost varied from run to run by as much as any difference between the arms. In the pairs where the flow made no lookup, cost moved by −5% to +13%.
  - A task's two arms also ran side by side and shared the prompt cache. The runner now runs them apart.

## AgentDojo: 41 held-out tasks, two models

| Model | Group | Pairs | LLM turns, no flow → flow | Fewer (95% interval) | Pairs fewer / more (sign test p) | Passed, no flow → flow | Lookups (own / detours / repeated) | Agent input tokens, no flow → flow |
|---|---|---|---|---|---|---|---|---|
| both | **all** | 82 | 284 → 267 | **6.0%** (2.1% to 9.9%) | 19 / 6 (0.015) | 68 → 69 | 88 (36 / 52 / 5) | 1,193,610 → 1,128,660 |
| both | **Slack and travel** | 34 | 138 → 124 | **10.1%** (5.8% to 14.0%) | 14 / 1 (0.001) | 27 → 27 | 87 (36 / 51 / 5) | 520,609 → 477,214 |
| both | banking and workspace | 48 | 146 → 143 | 2.1% (−3.6% to 8.7%) | 5 / 5 (1) | 41 → 42 | 1 (0 / 1 / 0) | 673,001 → 651,446 |
| both | the flow looked up | 26 | 106 → 94 | 11.3% (6.0% to 15.3%) | 13 / 1 (0.002) | 22 → 22 | 88 (36 / 52 / 5) | 422,320 → 392,757 |
| both | the flow made no lookup | 56 | 178 → 173 | 2.8% (−2.4% to 8.2%) | 6 / 5 (1) | 46 → 47 | 0 | 771,290 → 735,903 |
| GLM-5.3 | all | 41 | 148 → 140 | 5.4% (−0.7% to 11.8%) | 9 / 4 (0.27) | 37 → 37 | 44 (16 / 28 / 1) | 535,213 → 514,605 |
| GLM-5.3 | Slack and travel | 17 | 74 → 68 | 8.1% (1.7% to 13.7%) | 7 / 1 (0.07) | 14 → 14 | 44 (16 / 28 / 1) | 235,778 → 227,251 |
| GLM-5.3 | banking and workspace | 24 | 74 → 72 | 2.7% (−7.5% to 13.8%) | 2 / 3 (1) | 23 → 23 | 0 | 299,435 → 287,354 |
| Claude Haiku 4.5 | all | 41 | 136 → 127 | 6.6% (1.5% to 11.4%) | 10 / 2 (0.039) | 31 → 32 | 44 (20 / 24 / 4) | 658,397 → 614,055 |
| Claude Haiku 4.5 | Slack and travel | 17 | 64 → 56 | 12.5% (5.7% to 18.2%) | 7 / 0 (0.016) | 13 → 13 | 43 (20 / 23 / 4) | 284,831 → 249,963 |
| Claude Haiku 4.5 | banking and workspace | 24 | 72 → 71 | 1.4% (−4.4% to 7.2%) | 3 / 2 (1) | 18 → 19 | 1 (0 / 1 / 0) | 373,566 → 364,092 |

By suite, both models together:

| Suite | Pairs | LLM turns, no flow → flow | Fewer (95% interval) | Pairs fewer / more | Passed | Lookups (own / detours / repeated) | Replay, other agents |
|---|---|---|---|---|---|---|---|
| Slack | 18 | 75 → 68 | 9.3% (4.1% to 14.3%) | 6 / 0 | 18 → 18 | 63 (20 / 43 / 3) | 16.8% |
| travel | 16 | 63 → 56 | 11.1% (2.2% to 16.4%) | 8 / 1 | 9 → 9 | 24 (16 / 8 / 2) | 8.7% |
| banking | 16 | 51 → 52 | −2.0% (−8.0% to 4.2%) | 2 / 3 | 15 → 15 | 0 | 0 |
| workspace | 32 | 95 → 91 | 4.2% (−3.2% to 13.9%) | 3 / 2 | 26 → 27 | 1 (0 / 1 / 0) | 0 |

The last column is the share of turns the same flows saved in the replay of ten newer agents' published runs.

- **Travel** saved a little more live than in replay.
- **Slack** saved about half as much. The live agents read fewer channels than the published ones, so fewer of the flow's channel reads were theirs.
- **Banking and workspace:** the flows make no lookup there, live or in replay. Banking's likeliest read takes a number, which a binding does not pass. Every workspace search takes a query, a day or a file name that came from the user's request.

Splitting the pairs by whether the flow made a lookup uses the flow arm's own run, so it is not fixed in advance the way the suites are. Its silent half is still useful. In those 56 pairs the agent saw the same tools, the same server handshake and the same results in both arms. The proxy's logs match. So their 2.8% is the run-to-run variation of one run per arm.

## BFCL: 20 held-out tasks, two models

| Model | Group | Pairs | LLM turns, no flow → flow | Fewer (95% interval) | Pairs fewer / more (sign test p) | Passed, no flow → flow | Lookups (own / detours / repeated) |
|---|---|---|---|---|---|---|---|
| both | **all** | 40 | 444 → 451 | **−1.6%** (−6.3% to 4.2%) | 7 / 14 (0.19) | 29 → 29 | 19 (8 / 11 / 7) |
| both | the flow looked up | 15 | 164 → 160 | 2.4% (−3.8% to 11.0%) | 2 / 1 (1) | 10 → 10 | 19 (8 / 11 / 7) |
| both | the flow made no lookup | 25 | 280 → 291 | −3.9% (−9.8% to 3.8%) | 5 / 13 (0.096) | 19 → 19 | 0 |
| GLM-5.3 | all | 20 | 216 → 227 | −5.1% (−9.3% to −1.0%) | 3 / 9 (0.15) | 15 → 15 | 10 (4 / 6 / 3) |
| GLM-5.3 | the flow made no lookup | 12 | 126 → 136 | −7.9% (−13.6% to −2.4%) | 2 / 8 (0.11) | 10 → 10 | 0 |
| Claude Haiku 4.5 | all | 20 | 228 → 224 | 1.8% (−6.1% to 12.7%) | 4 / 5 (1) | 14 → 14 | 9 (4 / 5 / 4) |

BFCL's replay projected 1.57% of turns saved: BFCL's 200 base tasks are 200 different requests over 128 tools, and eight models' runs were too few to learn what comes next. Live, the flow made 19 lookups in 40 episodes. GLM-5.3's 5.1% more turns comes almost entirely from the 12 pairs in which the flow made none. There, both arms ran under identical conditions, and 10 more turns in 126 is run-to-run variation. Its interval excludes zero, as about one in twenty will.

At BFCL's noise, about two turns per pair, resolving a 1.6% effect at 80% power would take about 900 pairs. That is BFCL's 80 held-out tasks run eleven times over. AgentDojo's 6–7% needs 60–90 pairs; this run has 82.

## What the flow looked up

| Suite | Tool | Own | Detours |
|---|---|---|---|
| Slack | `read_channel_messages` | 16 | 36 |
| Slack | `get_channels`, `get_users_in_channel` | 4 | 7 |
| travel | `get_hotels_prices`, `get_rating_reviews_for_hotels`, `get_cuisine_type_for_restaurants`, `get_rating_reviews_for_car_rental` | 16 | 2 |
| travel | car prices and fuel, dietary restrictions | 0 | 6 |
| workspace | `get_received_emails` | 0 | 1 |

- **Travel's lookups were mostly the agent's own.** These are the list bindings added for travel: every hotel or restaurant that a city's listing named.
- **Slack's detours are one walk.** After `get_channels`, the flow reads `general`, then `random`, `private` and `External_0`, each after the last. The older agents read every channel on the tasks that ask about all of them.
  - Two held-out tasks do: 10 (add everyone from the channels to one) and 13 (congratulate the most active user). There both agents read all four channels in one turn, and the flow's reads were theirs.
  - On five tasks for GLM-5.3 and four for Haiku, the agents listed the channels to post, invite or find a user, and read none. There the walk made 22 of GLM-5.3's 24 Slack detours and all 19 of Haiku's.

## Promotion on the agents' own sessions, live

Shadow mode and promotion ([results](promotion-2026-09-25.md)) are the deployment's answer to a site whose lookups are not the agent's own. Here they get their first live test, with each agent's own sessions:

1. **Record.** Each agent ran Slack's 12 training tasks with no flow, through the proxy: 24 episodes. GLM-5.3 passed 11 and Haiku 4.5 passed 10.
2. **Promote.** `stretto promote --decider reach --threshold 0.3` scored the published Slack flow on each agent's 12 sessions, at the default bar: 70% of a site's lookups used, a lower bound of 0.5 and three tasks.
3. **Serve.** Each promoted flow ran on the 9 held-out Slack tasks, as a third arm.

| Site, after | GLM-5.3: lookups used in training | Promoted | Haiku 4.5: lookups used in training | Promoted |
|---|---|---|---|---|
| `get_channels` (the walk's first read) | 4 of 7 (57%) | no | 4 of 6 (67%) | no |
| `read_channel_messages` (the walk's next read) | 13 of 13 | yes | 9 of 10 | yes |
| `get_users_in_channel` | 9 of 11 | yes | 6 of 6, from 2 tasks | no |
| `get_webpage` | 2 of 4 | no | 1 of 3 | no |

| Slack, 9 held-out tasks × 2 models | LLM turns, no flow → flow | Fewer (95% interval) | Pairs fewer / more | Passed | Lookups (own / detours / repeated) |
|---|---|---|---|---|---|
| the published flow | 75 → 68 | 9.3% (4.1% to 14.3%) | 6 / 0 | 18 → 18 | 63 (20 / 43 / 3) |
| promoted on the agent's own sessions | 75 → 72 | 4.0% (0.0% to 9.6%) | 2 / 0 | 18 → 18 | 8 (8 / 0 / 8) |

- **Promotion removed every detour, and the savings with them.** Holding back the walk's first read removed all 43 detours, and so the walk never started. On the two tasks that read every channel, the agents read them in one turn, as before. The published flow's lookups had saved 4 turns in Slack's pairs where the agent used one, and the promoted flow saved 3 fewer turns than it did.
- **The few turns the promoted flow saved are noise.** Its 8 lookups were all calls the agents made again themselves. The 3 turns GLM-5.3 saved came from pairs where the flow made no lookup, so they are run-to-run variation. The promoted arm also ran after the other two.
- **The site cannot tell the tasks apart.** After `get_channels`, the walk pays on a task that asks about every channel and is a detour on one that posts to a channel. Only the request says which. A per-site bar can keep the walk or drop it, but it cannot keep it for one kind of task only. That takes the conversation, which an MCP proxy does not see ([the paper](../../paper/stretto.md), §6, "What a proxy sees").
- **The kept chain was asked for already.** The sites promotion kept, the walk's next read and `get_users_in_channel`, followed a call whose sibling calls the agent had asked for in the same LLM turn.
  - Claude Code runs a turn's calls as the model streams them. So a call can reach the proxy after an earlier call of the same turn has returned, and the proxy counts it as a new turn.
  - `stretto promote` counted those later calls as the lookups being used, and the lookups spared nothing.
  - Scoring a lookup only against calls the agent could not already have asked for is [issue #40](https://github.com/alexnodeland/stretto/issues/40).

This test cost 39.0 GLM credits (26.2 for the training sessions, 12.7 for the promoted arm).

## Tokens, cost and time

Cost is priced as billed.
- **GLM credits** are counted as Z.ai's plan bills them: input ×6.9, cache reads ×1.7 and output ×24 per 10,000 tokens, halved off-peak.
- **Haiku 4.5** is priced at list: $1 per million input tokens, $0.10 for cache reads, $1.25 for cache writes and $5 for output.

These runs cannot resolve cost, for two reasons.

**The two arms shared a prompt cache.**
- Two runners shared each model's plan, and each took the next episode not yet taken. So a task's two arms usually ran side by side: their starts were a median of 5 to 18 seconds apart, and never more than five minutes.
- A task's arms send the same system prompt, tools and request. Whichever arm reached the model first wrote that prefix to the prompt cache, and the other read it at a tenth of the price.
- The flow arm happened to start first in 28 of 41 AgentDojo tasks and 14 of 20 BFCL tasks.
- For Haiku 4.5 in AgentDojo, the arm that started first cost more, by 14% to 42%, whichever arm it was.

`run_bench_paired.py` now runs every task's first arm, then every task's second, so a task's arms run far apart.

**Cost varies a lot from run to run.** The table weights the two orders equally: the mean of the log cost ratio among tasks where the no-flow arm ran first, and among those where the flow arm did.

| Benchmark | Model | Group | Pairs | Cost, no flow → flow | Change | Change, orders weighted equally |
|---|---|---|---|---|---|---|
| AgentDojo | GLM-5.3 | all | 41 | 102.8 → 106.7 credits | +3.8% | +5.8% |
| AgentDojo | GLM-5.3 | the flow made no lookup | 28 | 69.1 → 71.1 credits | +2.9% | +7.0% |
| AgentDojo | Claude Haiku 4.5 | all | 41 | $0.530 → $0.547 | +3.1% | −4.0% |
| AgentDojo | Claude Haiku 4.5 | Slack and travel | 17 | $0.245 → $0.230 | −6.0% | −7.4% |
| AgentDojo | Claude Haiku 4.5 | the flow made no lookup | 28 | $0.356 → $0.368 | +3.4% | −5.4% |
| BFCL | GLM-5.3 | all | 20 | 118.8 → 134.0 credits | +12.8% | +8.7% |
| BFCL | GLM-5.3 | the flow made no lookup | 12 | 66.8 → 75.8 credits | +13.3% | +11.0% |
| BFCL | Claude Haiku 4.5 | all | 20 | $0.483 → $0.514 | +6.5% | +6.2% |
| BFCL | Claude Haiku 4.5 | the flow made no lookup | 13 | $0.308 → $0.348 | +12.8% | +12.5% |

- **The noise is as large as any effect.** The pairs where the flow made no lookup still moved by −5% to +13%, with no difference between the arms. The groups where it acted fall inside that range.
- **Why cost may not follow turns.** A lookup's result arrives as new input, written to the cache once. A turn the flow saves is mostly a cache read of a prompt already written, so priced as billed it is worth less than its tokens suggest. See the paper's "Priced as billed".
- **Wall time moved by no more than its noise either.** Each episode starts Claude Code and the MCP server, and the arms ran side by side.

## What these runs cannot show

- **One run per arm, two models.** The intervals are over tasks. The run-to-run variation is measured, by the pairs in which the flow made no lookup, but not averaged away.
- **Twenty of BFCL's 80 held-out tasks.** The sample was set by the GLM credit budget. At BFCL's projected effect, even all 80 would not resolve it (above).
- **One promotion test, on one suite.** Promotion was scored on 12 training sessions per agent, and its arm ran after the other two.
- **The flows are the published ones.** They were learned from older agents, as a deployment would start. A flow learned from these agents' own sessions, which is what `stretto learn --sessions` does after a few days of traffic, is not tested here. In τ²-bench, an agent's own sessions saved more than other agents' (the paper's §4.3).
- **Not run live:**
  - WorkBench: its flows make no lookup in replay, so both arms would run alike.
  - τ-bench: τ²-bench's live runs cover its domains.
  - DTap-Bench and MCPMark: they need their services, such as Jira, Salesforce, Notion and GitHub, with the benchmark's accounts.

## The episodes

[live-benchmarks-2026-09-27-episodes.tar.gz](live-benchmarks-2026-09-27-episodes.tar.gz) holds all 286 episodes.
- **Layout:** `agentdojo/` and `bfcl/`, then the model (`glm-5.3/`, `claude-haiku-4.5/`), then the arm (`baseline/`, `reach/`, and on Slack `promoted/`), then one folder per task. `agentdojo-train/` holds the Slack training sessions that promotion scored, and `promoted-flows/` the two promoted flows.
- **Each task folder:** the agent's event stream (`events.jsonl`), the proxy's session log (`log/`), the flow's decisions (`flow.jsonl`), every call the server ran (`trajectory.jsonl`, the flow's lookups included), the server's call count (`tools-state.json`), the MCP config (`mcp.json`), the agent's stderr and `result.json`.
- **Left out:** the environments' final states (`state.pkl`), which scoring used.

[live-benchmarks-2026-09-27.json](live-benchmarks-2026-09-27.json) holds every pair's row and every group's summary, as `analyze_bench.py` wrote them.

## Reproduce

Setup, the harness check and the runners are in [`pilot/bench/README.md`](../../pilot/bench/README.md). The flows are in [`benchmarks-2026-09-27-flows.tar.gz`](benchmarks-2026-09-27-flows.tar.gz).

```bash
cd pilot/bench
# AgentDojo: every held-out task, both arms (GLM-5.3 shown; Haiku with --agent-cli claude --model claude-haiku-4-5-20251001)
python run_bench_paired.py agentdojo --suites travel slack banking workspace --agent-cli glm --model glm-5.3 \
    --flows FLOWS --out runs/dojo/glm --credit-cap 300
# BFCL: 20 of the 80 held-out tasks, seed 7
python run_bench_paired.py bfcl --bfcl-dir bfcl --sample 20 --seed 7 --agent-cli glm --model glm-5.3 \
    --flows FLOWS --out runs/bfcl/glm --credit-cap 300
# The tables on this page, from the episodes archive or your own runs
python analyze_bench.py runs/dojo/glm runs/dojo/haiku runs/bfcl/glm runs/bfcl/haiku \
    --set "Slack and travel=slack,travel" --set "banking and workspace=banking,workspace" \
    --json live-benchmarks.json --md live-benchmarks.md
```

The promotion test, for one model:

```bash
python run_bench_paired.py agentdojo --suites slack --split train --arms baseline --agent-cli glm --model glm-5.3 \
    --flows FLOWS --out runs/promo/glm/train
mkdir -p runs/promo/glm/sessions
for d in runs/promo/glm/train/baseline/slack-*; do cp "$d"/log/*.jsonl "runs/promo/glm/sessions/$(basename "$d").jsonl"; done
stretto promote --flow FLOWS/agentdojo-slack.flow.json --sessions runs/promo/glm/sessions --decider reach \
    --threshold 0.3 --out slack-promoted.flow.json
for t in user_task_0 user_task_1 user_task_2 user_task_3 user_task_10 user_task_11 user_task_12 user_task_13 user_task_20; do
  python run_bench_episode.py agentdojo --suite slack --task $t --arm reach --label promoted \
      --flow slack-promoted.flow.json --agent-cli glm --model glm-5.3 --out runs/dojo/glm
done
```

From the archive:

```bash
tar -xzf docs/results/live-benchmarks-2026-09-27-episodes.tar.gz -C /tmp
E=/tmp/live-benchmarks-2026-09-27-episodes
python pilot/bench/analyze_bench.py $E/agentdojo/glm-5.3 $E/agentdojo/claude-haiku-4.5 $E/bfcl/glm-5.3 \
    $E/bfcl/claude-haiku-4.5 --set "Slack and travel=slack,travel" --set "banking and workspace=banking,workspace"
```
