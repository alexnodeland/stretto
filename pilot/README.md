# Live pilot harness

Runs τ²-bench episodes live, with the agent in an MCP host, so stretto can
be measured on real traffic rather than replays.

- **Agent:** GLM in Claude Code, on Z.ai's GLM Coding Plan endpoint. That is the setup Z.ai documents for its coding plan, which may only be used in supported tools. [`glm-claude.sh`](glm-claude.sh) runs Claude Code with a clean environment, so a Claude Code session that starts it is not affected. The agent has no built-in tools; it gets τ²-bench's system prompt (instructions and domain policy).
- **Tools:** [`tau2_mcp.py`](tau2_mcp.py) serves one task's tools over MCP, behind [`stretto-proxy`](../crates/stretto-proxy), which records the session. It appends every call and result to the episode's `trajectory.jsonl` in τ²-bench's message format.
- **Customer:** τ²-bench's user-simulator prompt, answered by GLM through the same route ([`customer.py`](customer.py)), one short request per reply.
- **Episode:** [`run_episode.py`](run_episode.py) keeps one Claude Code process alive for the whole episode (stream-json in and out). Each agent turn ends with a message to the customer, and the customer's reply is the next turn, as in τ²-bench. The episode ends when the customer stops, or when the agent transfers them to a human agent. τ²-bench's customer is told to end the conversation at a transfer, and the simulated one here did not always do so (the cold start's task 27, below).
- **Scoring:** τ²-bench's own evaluator checks the final database. Natural-language assertions need an LLM judge and are left out, so the reward here is the database check alone.
- **Flows arm:** the same episode with a read-only flow behind the tools (RFC-001 §3.13). `stretto flow-serve` compiles the flow before the agent starts, from cached System-One answers, goal free (no one names the episode's goal live). After each of the agent's calls, `tau2_mcp.py` asks it what comes next, makes the lookup it names, records it like any other call and asks again, until the flow hands back. The agent gets its own result followed by the flow's lookups in the same tool response, so those lookups cost it no turns. The flow asks Jev one question per step (`TYPESAFE_API_KEY`, read by `flow-serve` only, never by the agent's process).

## Setup

```bash
uv venv --python 3.12 ~/.venvs/tau2
VIRTUAL_ENV=~/.venvs/tau2 uv pip install -e ../../tau2-bench websockets "mcp>=1.10,<2"
cargo build --release -p stretto-proxy
```

`ZAI_API_KEY` must be set; `GLM_CLAUDE_CONFIG_DIR` optionally keeps the nested Claude Code's state apart.

A Claude model can play the agent or the customer instead: `--agent-cli claude --model M` and `--customer-cli claude --customer-model M` run it through [`claude-agent.sh`](claude-agent.sh). That wrapper passes on Claude Code's own endpoint and credentials (`ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`) with proxy and certificate settings, and nothing else. It runs Claude Code in its normal mode, since `--bare` takes only `ANTHROPIC_API_KEY`, from an empty directory, so no `CLAUDE.md` is read. The normal mode adds a short note to the model's context, with the working directory, the model's name and the date; `--bare` adds the date alone. `CLAUDE_AGENT_CONFIG_DIR` keeps its state apart. Z.ai credits count the GLM side of an episode only, and `run_pilot.py` reports a Claude side in tokens (`claude_tokens`).

## Run one episode

```bash
PATH=~/.venvs/tau2/bin:$PATH python run_episode.py --task-id 90 --out runs/pilot
PATH=~/.venvs/tau2/bin:$PATH python run_episode.py --task-id 90 --out runs/pilot \
  --arm flows --oracle-cache ../.oracle-cache
```

`--confirm-judge log|enforce` adds [the confirmation judge](../crates/stretto-proxy/README.md) to the guards arm: the proxy puts each write the guards check for a confirmation to Jev as well, logs the judgment in `log/*.confirm.jsonl`, and in `enforce` refuses a write Jev fails. `--confirm-second proposed` asks the second question too, and `--confirm-second-shadow` only logs its answer. The proxy runs under the agent's process, so the harness hands it Jev's key in a file only the proxy opens (`TYPESAFE_API_KEY_FILE`): mode 0600, outside the episode directory, and deleted when the agent exits. The agent's process gets the file's path, never the key. `--label` names the arm's directory under `--out`, so two judge settings can share one. `result.json` lists the judgments.

`--record-context` hands the proxy the conversation, as the guards arm does, so the session log in `log/` can train a flow with `stretto learn` (see the cold start below). `--read-only-hints` makes `tau2_mcp.py` mark τ²-bench's read tools `readOnlyHint: true` and its writes `false` in `tools/list`, as a real server would, so `stretto learn --sessions` takes the tools' kinds from the log and needs no `--manifest`. The pilots ran without it, so their agents all saw the same tool list. `--flow` serves a compiled or learned flow instead of compiling one per episode.

The flows arm needs `cargo build --release -p stretto-report` and a replay cache holding the goal-free v2 answers. `--arm habit` runs the same flow on the habit alone; it still compiles from the cache, but asks Jev nothing live. Fill it with `stretto phase0 --oracle jev --questions v2 --predicates data/predicates-v2.json --no-intent`, or import the published bundles (the v2 bundle and its goal-free supplement, in `docs/results/`). Pilot tasks come from the test split, so the habit never trained on them. Each task is judged by the arbiter of its own fold, which never saw it.

The episode directory (`runs/pilot/baseline/task-90/`) holds everything:

- the conversation and tool calls (`trajectory.jsonl`);
- the agent's event stream (`events.jsonl`);
- the proxy's session log (`log/`);
- the scored τ²-bench simulation (`simulation.json`);
- `result.json`, which records the reward, LLM turns, tool calls, parallel-call turns and token usage (and, in the flows arm, the flow's lookups and queries);
- in the flows arm, every flow answer (`flow.jsonl`) and `flow-serve`'s log.

`rescore.py` scores recorded episodes again with τ²-bench's evaluator. By default it uses the database check, as the pilots did, and it can apply τ²-bench's communication check or each task's full reward basis (`--scoring communicate`, `--scoring basis`). The published pilots' episodes are in `docs/results/*-episodes.tar.gz` ([the episodes page](../docs/results/episodes-2026-09-24.md)), and `rescore.py` reproduces every recorded reward from them.

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

**Paired run, every test task.** [`run_paired.py`](run_paired.py) ran every retail test task once and every airline test task twice, in both arms, reusing the two pilots' pairs. That is 80 pairs, and it stayed under a credit budget it checked before each episode. [`analyze_paired.py`](analyze_paired.py) reports on it. See [the summary](../docs/results/paired-2026-09-25.md):

- 1,019 LLM turns without the flow and 759 with it, 25.5% fewer (95% interval 20.5% to 30.4%). Agent input tokens fell 21.0%.
- 71 of 80 pairs passed the database check without the flow and 70 with it: −1.25 points (−7.5 to +6.25). That cannot rule out a loss of a few points.
- 242 of the flow's 259 lookups were calls the agent made without the flow too. The 17 detours carried under 3% of the input tokens the flow saved.

**Cold start, live.** Two flows learned from the five sessions GLM-5.3 recorded, the habit alone and the habit with the shipped airline arbiter, ran on the retail pilot's ten tasks (`run_episode.py --arm habit` and `--arm flows`, each with `--flow`). See [the summary](../docs/results/cold-start-live-2026-09-25.md):

- The habit alone took 79 LLM turns, against 110 without a flow. It made exactly the lookups the habit from four other agents made.
- With the arbiter it took 70 turns, with 2 detours where the habit alone made 6. Both passed 8 of 10.

**Cold start, live, airline.** GLM-5.3 recorded five airline training sessions (`--arm baseline --record-context --read-only-hints`), and two flows learned from them ran on the airline pilot's ten tasks. See [the summary](../docs/results/cold-start-live-airline-2026-09-25.md):

- The habit alone took 102 LLM turns, against 127 without a flow and 105 with D0, with 8 detours, all reservation reads.
- With the shipped retail arbiter it took 102 turns too, with 1 detour. It passed 8 of 10, and the habit alone 7.
- 332 Z.ai credits.

**The confirmation judge, live.** The guards arm with `--confirm-judge log` against `--confirm-judge enforce`, the second question logged in both (`--confirm-second proposed --confirm-second-shadow`), on ten tasks per domain. See [the summary](../docs/results/judge-live-2026-09-25.md):

- Enforced, the judge refused none of 40 writes. Logged, it would have refused 4 of 38: 2 real lapses and 2 false alarms.
- Passes did not move: 8 and 8 of 10 in retail, 7 and 6 in airline, where the one difference failed with nothing refused.
- 782 Z.ai credits, and Jev's 156 answers cost under a cent.

**Claude models.** `--agent-cli claude --model M` and `--customer-cli claude --customer-model M`, on retail. See [the summary](../docs/results/claude-models-2026-09-25.md):

- Claude Haiku 4.5 as the agent, on the pilot's ten tasks: 79 LLM turns with D0 against 97 without a flow, 18.6% fewer; 7 and 6 of 10 passed.
- Claude Sonnet 5 as the agent, on three: 22 turns against 29.
- Claude Sonnet 5 as the customer to GLM-5.3, on five: D0 saved 23.1% of turns, against 25.8% with GLM-5.3 as the customer, and all 20 episodes passed.
- 2.32 million Claude tokens, most of them cache reads, and 178 Z.ai credits for the GLM side.

**Guards pilot, airline.** The guards arm (`run_episode.py --arm guards`) runs the tools behind `stretto-proxy --guards`, which refuses a write an enforced policy rule fails. It ran on the four airline test tasks where the guard audit finds the 2025 agents' refused writes concentrated (35, 45, 32, 48), plus four harm checks against the airline pilot's baselines. See [the summary](../docs/results/pilot-guards-2026-09-24.md):

- GLM-5.3 passed all four main tasks in both arms, and the guards refused nothing. It never tried the cancellations the policy forbids.
- With the guards on, 7 of the 8 episodes passed; the failure (task 31) was the agent's cost error from the airline pilot, with nothing refused.
- The proxy checked 9 writes live and passed them all, among them task 32's upgrade-then-change, which a rule briefly changed that morning would have refused.
- 223 Z.ai credits.

**Habit-only pilot, retail.** The habit arm (`run_episode.py --arm habit`) is the flows arm with the flow deciding on the habit alone (`--decider habit`), never asking Jev. It ran on the retail pilot's ten tasks, paired with that pilot's no-flow and D0 episodes. See [the summary](../docs/results/pilot-habit-2026-09-24.md):

- 79 LLM turns, against 110 with no flow and 84 with D0: 28.2% fewer than with no flow, with fewer turns in all ten pairs. Against D0, a change of −0.5 turns per episode (95% interval −1.4 to +0.4).
- 37 flow lookups, none repeated by the agent; 9 of 10 passed.
- 109 Z.ai credits.

**Cold start, live.** A flow learned from five sessions GLM-5.3 ran on training tasks through the proxy (`--record-context`), with `stretto learn`. Three sessions trained the habit, and two held out the 15 decisions its arbiter was fitted on. It ran on the retail pilot's tasks, costliest first, until the round's 200-credit budget was nearly spent. See [the results](../docs/results/cold-start-2026-09-24.md):

- **Three tasks finished.** Task 101 took 12 LLM turns against 19 without the flow, and task 36 took 14 against 16. Both passed.
- **Task 27 failed, as it did without the flow and with D0.** The agent filed a return, which blocks the exchange the customer also wanted. The simulated customer then carried on after each transfer to a human, and the episode ran to 26 LLM turns. The harness now ends an episode at a transfer.
- **Cost.** 182 Z.ai credits in all: 79 to record the five sessions, 97 for the three tasks, and 6 for a fourth, stopped to stay under the budget.

## Check the flow without an LLM

[`check_flow.py`](check_flow.py) replays recorded episodes' tool calls through `tau2_mcp.py` with the flow behind it. It takes an episode from this harness, or τ²-bench results, by default one trial of each test-split task. A recorded call the flow has already made is skipped. A recorded turn whose calls were all skipped is a turn the flow saves, if the agent otherwise behaved the same. A flow lookup the agent never made is a detour. This is the offline projection's measure, but with the live flow's own argument bindings and questions, and no GLM calls.

```bash
PATH=~/.venvs/tau2/bin:$PATH python check_flow.py --out runs/check \
  --results ../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --oracle-cache ../.oracle-cache --flow-oracle jev
```

`--flow-oracle mock` checks the plumbing for free, and `--flow-oracle replay` reads the System-One answers from the cache only.

`--flow-decider habit` replays the habit alone: the flow never asks the System-One model, and acts on the habit's prediction by the same rule (arm C). At a high `--flow-threshold`, such as 0.99, it goes on only where training shows no branch, and hands every branch back, as TraceCompiler does.

**Faster replays.** By default each episode's tools run in their own `tau2_mcp.py` process over MCP, one episode at a time. About 5.4 seconds of every episode goes to starting Python and importing τ²-bench. Two options skip that:

- `--in-process` calls `tau2_mcp.Episode` directly instead.
- `--jobs N` replays N episodes at once, in forked workers that inherit the imports.

With both, GLM-5's 160 retail test episodes replay in about 33 seconds on two workers, where they took about 13 minutes. The rows are the same. For the arms round's habit-alone, arm C and D0 replays, every row matched the published ones, and the MCP path with `--jobs 2` matched too.

Each recorded episode replays in a folder of its own under `--out`, named after its folder. Folders that share a name, such as a task's trials each in their own folder, are named by their path below the folders' common parent instead (`baseline-task-2`, `trial-1-baseline-task-2`). Before, they shared one folder, and with `--jobs` they wrote one trajectory, which the flow reads: an airline replay of both trials saved 23 turns where it saves 45.

[`replay_study.py`](replay_study.py) runs a list of replays from a JSON file, each with its own flow and settings, and skips any run whose `check.json` already exists. An interrupted study resumes where it stopped.
