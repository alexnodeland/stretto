# A smaller agent, with flows and guards from frontier traces

The design's decision on models is transfer first: flows compiled from frontier models' published trajectories, run by cheaper agents. Two findings pointed at a smaller agent ([#8](https://github.com/alexnodeland/stretto/issues/8)):

- **Guards are insurance whose value depends on the agent.** On τ²-bench's published airline runs, the enforced guards would refuse a write the tool accepted in 38% of failed episodes ([the audit](guards-2026-09-24.md)). Live, GLM-5.3 gave them nothing to refuse ([the guards pilot](pilot-guards-2026-09-24.md)).
- **Frontier traces make good habits.** Habits from Claude Opus 4.5, Claude Sonnet 4.5 and Gemini 3 predict GLM-5 as well as its own in retail, and better in airline ([Phase 0](phase0-2026-09-23.md)).

This round runs the model Claude Code's small-model slot maps to on Z.ai's coding plan, `glm-5.3-flash`, on the pilots' ten tasks per domain. Each task ran once with no flow (arm A) and once with a flow learned from those three frontier models' traces and the policy guards together (arm E).

## Findings

- **The flow cut the smaller agent's LLM turns by a fifth.** Over the 20 pairs they fell from 240 to 190, 20.8% fewer (95% interval 11.0% to 29.9%), with fewer turns on 16 tasks and more on 3.
  - In retail they fell 33.3% (25.6% to 41.5%), on all ten tasks, and the agent made 35 tool calls of its own against 66.
  - In airline they fell 10.1% (−7.9% to 22.6%): fewer on six tasks and more on three.
  - The agent's input tokens fell 26.3% in retail and 6.0% in airline, 14.3% pooled (−1.3% to 25.1%).
  - The flow learned only from the three frontier models' training episodes. The agent it served contributed none.
- **Passes did not move.** Both arms passed 9 of 10 in each domain, and 8 of 10 in retail with every check in τ²-bench's reward basis. Two pairs went each way (McNemar p = 1.0):
  - With the flow, the agent failed retail task 27. Asked to return two items and exchange a third, with the exchange preferred if only one could be done, it made the return first. The exchange it then tried was refused, as the tool would have refused it: the order was no longer delivered.
  - With the flow, it also failed airline task 31. It upgraded a basic-economy reservation to economy, then moved it to a nonstop flight. The customer had agreed only to a change under $100.
  - Without the flow, it failed retail task 64, exchanging for another camera than the one the task expects, and airline task 6, booking a new reservation for a customer who wanted insurance added to an existing one.
- **The guards had almost nothing to refuse.** They refused one write in arm E's 20 episodes: the exchange on retail task 27, which the tool refuses too.
  - On these tasks the smaller agent was not the weaker one. Alone it passed 18 of 20, where GLM-5.3 passed 16 in the pilots ([the named-other count, live](named-other-live-2026-09-29.md)), and it made no write a guard knows to break the policy.
  - Its airline failure with the flow took a path the policy allows: an upgraded basic-economy reservation's flights can change, and τ²-bench's task 32 expects exactly that. What it broke was the customer's price limit, which only the customer states.
- **The agent used the flow's lookups.** Of the flow's 65 lookups, 58 were calls the agent made itself without the flow: 33 of 39 in retail and 25 of 26 in airline. It made 3 of them again, all in airline.
- **The guards must see what the flow read.** The pilots run the flow inside the MCP server, behind the proxy, and the server appends the flow's lookups to the agent's own result. The first attempt at this round ran the guards reading that result whole.
  - A user id with lookups after it authenticated nobody, and a looked-up order was never seen. In six retail episodes the guards refused 15 writes the policy allows, at least twice in each, and the agent read the records again to get past them.
  - Those six took 76 LLM turns, against 63 without a flow and 43 once fixed.
  - The guards now read the lookups appended to a result as reads the agent was shown (`stretto_trace::mcp::appended`), and so does the confirmation judge's check of the records a write names. The run above is the fixed one. The first attempt's episodes are in the archive, under `first-attempt/`.

## Setup

- **Agent and customer.** The agent is `glm-5.3-flash` in Claude Code, the model `pilot/glm-claude.sh` gives Claude Code's small-model slot, on Z.ai's GLM Coding Plan. The customer is GLM-5.3, from τ²-bench's user-simulator prompt, as in the pilots. The prompts are τ²-bench's own.
- **Tasks.** The pilots' ten test tasks per domain: retail 17, 18, 27, 36, 51, 60, 64, 68, 77 and 101, and airline 6, 8, 16, 18, 24, 25, 26, 30, 31 and 37.
- **The flows, from frontier traces.** `stretto learn --results … --habit-only`, from τ²-bench's published runs of Claude Opus 4.5, Claude Sonnet 4.5 and Gemini 3 Pro on the training tasks: 678 successful retail episodes and 296 airline ones. No test task is among them. Each flow is the habit alone, served with the reach decider at 0.3, so it asks no model ([retail](smaller-agent-2026-09-29-retail.flow.json), [airline](smaller-agent-2026-09-29-airline.flow.json)).
- **Arms.** Each task ran once in each arm, the two arms side by side (`pilot/run_paired.py --arms baseline reach-guards=FLOW --model glm-5.3-flash`).
  - *A, no flow.* The agent alone.
  - *E, the flow and the guards.* The flow runs behind the agent's calls, and `stretto-proxy --guards` checks each call against the domain's enforced rules before it reaches the tools, with the conversation handed to the proxy (`--context`). This is `run_episode.py --arm reach-guards`.
- **Rewards.** τ²-bench's database check, and every check in its reward basis. The retail assertions were judged once each by Claude Haiku 4.5 (`run_episode.py --judge`).
- **Cost.** 629 Z.ai credits for the 40 episodes, at GLM-5.3's rates, which overstate a smaller model's. The first attempt's six episodes and two cut short cost 118 more (15 of them estimated), and two smoke episodes 19. The judge ran on the Claude subscription.

## Retail

| | A, no flow | E, the flow and the guards |
|---|---|---|
| LLM turns | 111 | 74 |
| Fewer than without a flow (95%) |  | 33.3% (25.6% to 41.5%) |
| Agent input tokens | 706,355 | 520,461 |
| Fewer tokens (95%) |  | 26.3% (17.3% to 37.1%) |
| The agent's own tool calls | 66 | 35 |
| Flow lookups: the agent's own, detours |  | 33, 6 |
| Lookups the agent made again |  | 0 |
| Writes the guards refused |  | 1 |
| Passed the database check | 9 | 9 |
| Passed every check | 8 | 8 |
| Z.ai credits | 133.7 | 109.0 |

| Task | Turns: A, E | Lookups | Passed: A, E |
|---|---|---|---|
| 17 | 8, 6 | 4 | ✓ ✓ |
| 18 | 11, 7 | 4 | ✓ ✓ |
| 27 | 12, 8 | 6 | ✓ ✗ |
| 36 | 15, 13 | 2 | ✓ ✓ |
| 51 | 10, 5 | 6 | ✓ ✓ |
| 60 | 7, 4 | 3 | ✓ ✓ |
| 64 | 12, 6 | 4 | ✗ ✓ |
| 68 | 12, 7 | 4 | ✓ ✓ |
| 77 | 8, 6 | 2 | ✓ ✓ |
| 101 | 16, 12 | 4 | ✓ ✓ |

Passed is by the database check. With every check, task 68 fails in both arms: the judge splits on whether listing every order's total tells the customer the one they asked for ([scoring](scoring-2026-09-29.md)).

## Airline

| | A, no flow | E, the flow and the guards |
|---|---|---|
| LLM turns | 129 | 116 |
| Fewer than without a flow (95%) |  | 10.1% (−7.9% to 22.6%) |
| Agent input tokens | 1,030,632 | 969,086 |
| Fewer tokens (95%) |  | 6.0% (−25.9% to 21.2%) |
| The agent's own tool calls | 79 | 67 |
| Flow lookups: the agent's own, detours |  | 25, 1 |
| Lookups the agent made again |  | 3 |
| Writes the guards refused |  | 0 |
| Passed (database × communication) | 9 | 9 |
| Z.ai credits | 195.2 | 190.6 |

| Task | Turns: A, E | Lookups | Passed: A, E |
|---|---|---|---|
| 6 | 15, 12 | 1 | ✗ ✓ |
| 8 | 12, 7 | 5 | ✓ ✓ |
| 16 | 10, 8 | 2 | ✓ ✓ |
| 18 | 29, 21 | 6 | ✓ ✓ |
| 24 | 22, 22 | 1 | ✓ ✓ |
| 25 | 8, 9 | 1 | ✓ ✓ |
| 26 | 8, 7 | 2 | ✓ ✓ |
| 30 | 7, 9 | 1 | ✓ ✓ |
| 31 | 7, 12 | 3 | ✓ ✗ |
| 37 | 11, 9 | 4 | ✓ ✓ |

## Limits

- **Ten tasks per domain, one episode each.** Twenty pairs show a saving in turns, but only a large change in the pass rate would show: GLM-5.3's paired run needed 80 pairs to bound it within about seven points ([the paired run](paired-2026-09-25.md)). The episodes vary with the simulated customer, as the pilots' did.
- **One smaller model, not weaker here.** `glm-5.3-flash` passed as many of these tasks as GLM-5.3 did, so this round cannot say what the guards do for an agent that makes the writes they refuse. The audit of published runs says where those are: in failed airline episodes, among the 2025 agents.
- **D0 and arm B did not run.** The plan was to add them if A and E differed. They did not, in passes, and the guards refused one write.

## Reproduce

The flows need no key. With τ²-bench's checkout and its Python environment, and its leaderboard submissions in `R`:

```sh
stretto learn --results $R/claude-opus-4-5_high_retail_gpt-5.2_4trials.json \
  --results $R/claude-sonnet-4-5_enabled_retail_gpt-5.2_4trials.json --results $R/geminipro-retail.json \
  --tau2 ../tau2-bench --domain retail --habit-only --out frontier-retail.flow.json
```

The live runs need the Z.ai key, and a Claude Code login for the judge:

```sh
cd pilot
python run_paired.py retail --arms baseline reach-guards=../docs/results/smaller-agent-2026-09-29-retail.flow.json \
  --tasks 17 18 27 36 51 60 64 68 77 101 --model glm-5.3-flash --flow-oracle mock --oracle-cache ../.oracle-cache \
  --judge claude:claude-haiku-4-5 --out runs/smaller-retail
python analyze_paired.py arms --tasks 17 18 27 36 51 60 64 68 77 101 \
  --arm baseline=runs/smaller-retail/baseline --arm reach-guards=runs/smaller-retail/reach-guards
```

Airline is the same with its tasks (6 8 16 18 24 25 26 30 31 37) and flow. The 40 episodes are in [smaller-agent-2026-09-29-episodes.tar.gz](smaller-agent-2026-09-29-episodes.tar.gz), and every number here in [smaller-agent-2026-09-29.json](smaller-agent-2026-09-29.json).
