# Cold start, live, 2026-09-25: five sessions, with and without a shipped arbiter

The [first cold start](cold-start-2026-09-24.md) learned a flow from five retail sessions GLM-5.3 recorded through the proxy. Its arbiter was fitted on two of those sessions. It ran live on only three tasks before its budget ran out. Offline, two other ways to start did better:

- the habit alone, learned from all five sessions;
- the habit with an arbiter fitted elsewhere ([shipped arbiters](arbiter-transfer-2026-09-24.md)).

This round takes both live, on all ten of the retail pilot's tasks ([#3](https://github.com/alexnodeland/stretto/issues/3)).

- **Sessions:** the five retail training sessions GLM-5.3 recorded on 2026-09-24, tasks 15, 24, 76, 88 and 99. All five passed.
- **Flows,** both learned from all five sessions:
  - the habit alone (`stretto learn --habit-only`);
  - the same habit with the airline arbiter that ships in `data/arbiters/` (`--arbiter-from data/arbiters/airline.json`). That arbiter was fitted on four agents' airline decisions, so no retail decision went into it.
- **Tasks:** the retail pilot's ten test tasks, one episode per flow. Each is paired with that pilot's episodes without a flow and with D0, and with the habit-only pilot's episodes. D0 and the habit-only pilot's flow were compiled from four other agents' 2025 episodes.
- **Agent, customer, tools and reward:** as in [the pilots](pilot-2026-09-24.md).

## Offline first

Before they ran live, both flows replayed GLM-5's 40 trial-0 retail test episodes (`pilot/check_flow.py`), as the first cold start's flow did:

| Flow | Turns saved, of 347 | Flow lookups | Detours (episodes with one) |
|---|---|---|---|
| Habit, five sessions | 71 (20.5%) | 151 | 13 (5) |
| Habit, five sessions, airline arbiter | 68 (19.6%) | 154 | 10 (7) |
| The first cold start's flow, its arbiter fitted on two of the sessions | 66 (19.0%) | | 14 |
| D0, from four other agents | 77 (22.2%) | | |

## Live

| | No flow | D0 | Habit, four agents | Habit, five sessions | Habit, five sessions, airline arbiter |
|---|---|---|---|---|---|
| LLM turns | 110 | 84 | 79 | 79 | **70** |
| Fewer than with no flow (95% interval) | | 23.6% (14.3% to 32.8%) | 28.2% (20.8% to 36.7%) | 28.2% (19.0% to 36.9%) | 36.4% (26.3% to 44.5%) |
| Agent input tokens | 723,926 | 571,528 | 550,910 | 552,523 | 477,557 |
| Passed the database check | 8 | 8 | 9 | 8 | 8 |
| Flow lookups: the agent's own, detours | | 26, 0 | 31, 6 | 31, 6 | 32, 2 |
| Lookups the agent repeated | | 3 | 0 | 0 | 4 |
| Z.ai credits | 128.5 | 108.0 | 109.1 | 109.0 | 95.6 |

A lookup is the agent's own when the agent made the same call in the task's episode without a flow; otherwise it is a detour. The intervals come from a bootstrap over the ten tasks.

- **Five sessions teach the habit what four agents' episodes do.** The habit learned from GLM-5.3's five sessions made exactly the lookups, tools and arguments alike, that the habit learned from four other agents' 2025 episodes made, on all ten tasks. Both took 79 turns, 28.2% fewer than without a flow, and fewer than D0's 84.
- **So the two habit arms differ only by chance.** Their flows did the same, yet their turns differ on six tasks, and the four-agent habit passed task 27 where the five-session habit failed it. That is how much the agent and the simulated customer vary from one episode to the next.
- **The shipped arbiter on top saved the most.** It took 70 turns, 36.4% fewer than without a flow. That is 0.9 fewer per episode than the habit alone (95% interval −2.3 to +0.2), with fewer turns on 6 tasks and more on 2. It made 2 detours where the habit alone made 6. Ten tasks, one episode each, cannot separate the two.
- **Passes.** Both new flows passed 8 of 10. They failed tasks 27 and 64, as the arms without a flow and with D0 did, with the same agent errors: on task 27 the agent filed a return before the exchange the task expects, and on task 64 it chose the wrong variant.
- **Offline and live.** Offline, the arbiter flow saved a little less than the habit alone (19.6% against 20.5%) with fewer detours. Live it saved more, which is within what one episode per task can show. The new arms ran a day after the pilot's, with the same model and harness, so a change in how the model is served cannot be ruled out.

<details><summary>Every task</summary>

LLM turns, the flow's lookups, and the database check.

| Task | No flow | D0 | Habit, four agents | Habit, five sessions | Habit, five sessions, airline arbiter |
|---|---|---|---|---|---|
| 17 | 6, passed | 7, 0 lookups, passed | 5, 4 lookups, passed | 6, 4 lookups, passed | 6, 2 lookups, passed |
| 18 | 11, passed | 9, 2 lookups, passed | 8, 3 lookups, passed | 9, 3 lookups, passed | 10, 2 lookups, passed |
| 27 | 14, failed | 8, 5 lookups, failed | 7, 5 lookups, passed | 8, 5 lookups, failed | 7, 5 lookups, failed |
| 36 | 16, passed | 11, 4 lookups, passed | 13, 2 lookups, passed | 14, 2 lookups, passed | 8, 6 lookups, passed |
| 51 | 10, passed | 5, 2 lookups, passed | 5, 5 lookups, passed | 5, 5 lookups, passed | 5, 2 lookups, passed |
| 60 | 6, passed | 5, 1 lookups, passed | 5, 4 lookups, passed | 5, 4 lookups, passed | 4, 3 lookups, passed |
| 64 | 11, failed | 9, 3 lookups, failed | 8, 4 lookups, failed | 8, 4 lookups, failed | 6, 4 lookups, failed |
| 68 | 9, passed | 8, 4 lookups, passed | 7, 4 lookups, passed | 7, 4 lookups, passed | 6, 4 lookups, passed |
| 77 | 8, passed | 6, 2 lookups, passed | 7, 2 lookups, passed | 6, 2 lookups, passed | 5, 3 lookups, passed |
| 101 | 19, passed | 16, 3 lookups, passed | 14, 4 lookups, passed | 11, 4 lookups, passed | 13, 3 lookups, passed |

</details>

## What this means

- **A deployment can start from its first five sessions,** on the habit alone, which needs no key. Here it did live what a habit learned from four other agents did.
- **A shipped arbiter adds precision.** Fitted on another domain's decisions, it cut detours from 6 to 2 and cost no turns on these tasks.

## Cost

- **Z.ai:** 204.6 credits for the 20 episodes: 109.0 for the habit alone and 95.6 with the arbiter. The issue estimated about 240.
- **Jev:** the arbiter flow asked 56 questions live, 136,138 input tokens, under a cent. They are [this round's answer bundle](answers-2026-09-25-cold-live.md).

## The episodes and the flows

- [cold-start-live-2026-09-25-episodes.tar.gz](cold-start-live-2026-09-25-episodes.tar.gz): the 20 episodes, as `habit/task-<id>/` and `flows/task-<id>/` (the arbiter flow). Each holds the files [the episodes page](episodes-2026-09-24.md#what-each-episode-holds) lists.
- The two flows: [the habit alone](cold-start-live-2026-09-25-habit.flow.json) and [with the airline arbiter](cold-start-live-2026-09-25-airline-arbiter.flow.json).
- [cold-start-live-2026-09-25.json](cold-start-live-2026-09-25.json): every arm's numbers, per task, and the offline checks' totals.

## Reproduce

[The CLI reference](../cli.md) lists every option.

**The flows.** Learn both again from the five sessions, published with the first cold start:

```sh
mkdir -p /tmp/episodes && for a in cold-start-2026-09-24 pilot-2026-09-24 pilot-habit-2026-09-24 cold-start-live-2026-09-25; do
  tar -xzf docs/results/$a-episodes.tar.gz -C /tmp/episodes
done
X=/tmp/episodes/cold-start-2026-09-24-episodes
mkdir -p /tmp/sessions && cp $X/recorded/*/log/*.jsonl /tmp/sessions/
stretto learn --sessions /tmp/sessions --domain retail --manifest $X/retail-manifest.json --rewards $X/rewards.json \
  --habit-only --out cold5-habit.flow.json
stretto learn --sessions /tmp/sessions --domain retail --manifest $X/retail-manifest.json --rewards $X/rewards.json \
  --arbiter-from data/arbiters/airline.json --out cold5-airline.flow.json
```

Both come out identical to the published flows, apart from the time and version that wrote them, and the `program` that newer flows carry. `learn` asks no one for either.

**The offline check.** Import [the four answer bundles](answers-2026-09-25-cold-live.md#replay-without-a-key), fetch GLM-5's published results with `scripts/fetch-leaderboard.sh`, and replay:

```sh
R=.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json
cd pilot
python check_flow.py --domain retail --trials 0 --results ../$R --oracle-cache ../.oracle-cache \
  --flow ../cold5-habit.flow.json --flow-decider habit --flow-threshold 0.3 --flow-oracle replay --in-process --jobs 4
python check_flow.py --domain retail --trials 0 --results ../$R --oracle-cache ../.oracle-cache \
  --flow ../cold5-airline.flow.json --flow-decider arbiter --flow-threshold 0.3 --flow-oracle replay --in-process --jobs 4
```

A fresh cache with only those four bundles gives both rows of the offline table exactly.

**The live comparison,** from the archives:

```sh
E=/tmp/episodes
python analyze_paired.py arms --tasks 17 18 27 36 51 60 64 68 77 101 \
  --arm "no flow=$E/pilot-2026-09-24-episodes/baseline" --arm "D0=$E/pilot-2026-09-24-episodes/flows" \
  --arm "habit, four agents=$E/pilot-habit-2026-09-24-episodes/habit" \
  --arm "habit, five sessions=$E/cold-start-live-2026-09-25-episodes/habit" \
  --arm "habit, five sessions, airline arbiter=$E/cold-start-live-2026-09-25-episodes/flows"
```

**The live episodes** ran with `pilot/run_episode.py`, one per task and flow, under the budget [the paired run](paired-2026-09-25.md) used. They need `ZAI_API_KEY`, and `TYPESAFE_API_KEY` for the arbiter flow:

```sh
python run_episode.py --task-id 17 --arm habit --flow ../cold5-habit.flow.json --oracle-cache ../.oracle-cache --out runs/cold5
python run_episode.py --task-id 17 --arm flows --flow ../cold5-airline.flow.json --oracle-cache ../.oracle-cache --out runs/cold5
```
