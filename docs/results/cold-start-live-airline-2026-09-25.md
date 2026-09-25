# The cold start, live, in airline

[The cold start in retail](cold-start-live-2026-09-25.md) learned two flows from five sessions GLM-5.3 recorded through the proxy, and ran them on the retail pilot's ten tasks. The habit alone saved 28.2% of turns, and a shipped arbiter from the other domain cut its detours from 6 to 2. Offline, airline is harder: the habit alone saves less there, and an arbiter trades savings for fewer detours ([the cold start, offline](cold-start-2026-09-24.md)). This is the same run in airline ([#3](https://github.com/alexnodeland/stretto/issues/3)).

- **Recording.** GLM-5.3 in Claude Code ran five airline training tasks through the proxy, with the conversation handed to it: tasks 1, 15, 38, 41 and 42. They are the five-task random sample, which `--train-fraction 0.1667` draws, as retail's 15, 24, 76, 88 and 99 were. All five passed. They were recorded with `--read-only-hints`, so `learn` took the tools' kinds from the session logs and needed no manifest.
- **Flows,** both learned from all five sessions:
  - the habit alone (`stretto learn --habit-only`);
  - the same habit with the retail arbiter that ships in `data/arbiters/` (`--arbiter-from data/arbiters/retail.json`). That arbiter was fitted on four agents' retail decisions, so no airline decision went into it.
- **Tasks:** the airline pilot's ten test tasks, one episode per flow. Each is paired with that pilot's episodes without a flow and with D0.
- **Agent and customer:** GLM-5.3, both, as in the pilots.
- **Reward:** τ²-bench's database check.

## Findings

- **Five sessions saved as much as D0.** The habit alone took 102 LLM turns against 127 without a flow and 105 with D0: 19.7% fewer than without (95% interval −3.3% to 39.9%).
- **But it made 8 detours, where D0 made 1.** All eight were reservation reads. After finding the user, the habit reads each of their reservations in turn, and the agent needed only some of them. This is the airline counterpart of the product walks the retail flows made.
- **The shipped retail arbiter cut the detours to 1, at no cost in turns.** It took 102 turns, 19.7% fewer than without a flow (7.6% to 32.0%). It made 24 lookups, 23 of them the agent's own, as D0 did.
- **Passes:** 8 of 10 with the arbiter, 7 with the habit alone, against 8 without a flow and 9 with D0. None of the failures came from a flow decision.
  - Tasks 6 and 18 failed without a flow too.
  - Task 31, which failed with the habit alone, expects no change at all: a basic-economy flight cannot be changed. The agent upgraded the cabin and then rebooked, the same error it made with D0 in the pilot. The flow's lookups there were the agent's own.
- **Offline,** on GLM-5's 20 trial-0 airline test episodes, both flows saved 8.9% of turns, with 11 and 9 detours. That is what the habit alone from five of GLM-5's own published sessions saved offline (8.7%).

So airline confirms retail's reading. A deployment's first five sessions give a habit that saves what a flow compiled from four other agents saves. An arbiter shipped from the other domain adds the precision, here cutting detours from 8 to 1.

| | No flow | D0 | Habit, five sessions | With the shipped retail arbiter |
|---|---|---|---|---|
| LLM turns, ten tasks | 127 | 105 | 102 | 102 |
| Fewer than without a flow (95% interval) | | 17.3% (8.5% to 25.5%) | 19.7% (−3.3% to 39.9%) | 19.7% (7.6% to 32.0%) |
| Passed | 8 | 9 | 7 | 8 |
| Lookups | | 24 | 33 | 24 |
| Detours | | 1 | 8 | 1 |
| Agent input tokens | 996,580 | 849,367 | 836,872 | 882,616 |

Detours are lookups the agent did not make in the same task's episode without a flow.

## Per task

LLM turns, and whether the episode passed (✗ failed):

| Task | No flow | D0 | Habit, five sessions | With the retail arbiter |
|---|---|---|---|---|
| 6 | 13 ✗ | 7 | 12 ✗ | 11 ✗ |
| 8 | 8 | 7 | 8 | 9 |
| 16 | 8 | 8 | 7 | 8 |
| 18 | 28 ✗ | 23 | 10 ✗ | 24 ✗ |
| 24 | 22 | 18 | 21 | 11 |
| 25 | 9 | 8 | 7 | 8 |
| 26 | 9 | 6 | 8 | 7 |
| 30 | 8 | 8 | 9 | 8 |
| 31 | 7 | 8 ✗ | 11 ✗ | 4 |
| 37 | 15 | 12 | 9 | 12 |

## Published

- [cold-start-live-airline-2026-09-25-episodes.tar.gz](cold-start-live-airline-2026-09-25-episodes.tar.gz):
  - the five recorded sessions (`recorded/`), with the conversations handed to the proxy;
  - the twenty episodes (`habit/`, `flows/`);
  - `rewards.json`, which marks the five sessions as passed.
- The two flows: [the habit alone](cold-start-live-airline-2026-09-25-habit.flow.json) and [with the retail arbiter](cold-start-live-airline-2026-09-25-retail-arbiter.flow.json).
- [The answer bundle](answers-2026-09-25-cold-live-airline.md): the 75 Jev answers the arbiter flow's live and offline runs read that no earlier bundle held.

The 25 episodes cost 332 Z.ai credits at the off-peak rate.

## Reproduce

[The CLI reference](../cli.md) lists every option.

**The flows.** Learn both again from the archive's five sessions:

```sh
mkdir -p /tmp/episodes && tar -xzf docs/results/cold-start-live-airline-2026-09-25-episodes.tar.gz -C /tmp/episodes
X=/tmp/episodes/cold-start-live-airline-2026-09-25-episodes
mkdir -p /tmp/sessions-air && cp $X/recorded/*/log/*.jsonl /tmp/sessions-air/
stretto learn --sessions /tmp/sessions-air --domain airline --rewards $X/rewards.json --habit-only --out habit.flow.json
stretto learn --sessions /tmp/sessions-air --domain airline --rewards $X/rewards.json \
  --arbiter-from data/arbiters/retail.json --out retail-arbiter.flow.json
```

Both come out identical to the published flows, apart from the time that wrote them. `learn` asks no one for either.

**The offline check.** Import every answer bundle in `docs/results/`, fetch GLM-5's published results with `scripts/fetch-leaderboard.sh`, and replay:

```sh
R=.data/tau2-targets/glm-5_enabled_airline_gpt-5.2_4trials.json
cd pilot
python check_flow.py --domain airline --trials 0 --results ../$R --oracle-cache ../.oracle-cache \
  --flow ../habit.flow.json --flow-decider habit --flow-threshold 0.3 --flow-oracle replay --in-process --jobs 4
python check_flow.py --domain airline --trials 0 --results ../$R --oracle-cache ../.oracle-cache \
  --flow ../retail-arbiter.flow.json --flow-decider arbiter --flow-threshold 0.3 --flow-oracle replay --in-process --jobs 4
```

A fresh cache with only the published bundles gives both rows exactly.

**The live episodes** ran with `pilot/run_episode.py`, one per task and flow. The recordings used `--arm baseline --record-context --read-only-hints`:

```sh
python run_episode.py --domain airline --task-id 6 --arm habit --flow ../habit.flow.json --oracle-cache ../.oracle-cache --out runs/cold5-airline
python run_episode.py --domain airline --task-id 6 --arm flows --flow ../retail-arbiter.flow.json --oracle-cache ../.oracle-cache --out runs/cold5-airline
```

**The comparison,** from the archives:

```sh
E=/tmp/episodes
python analyze_paired.py arms --tasks 6 8 16 18 24 25 26 30 31 37 \
  --arm "no flow=$E/pilot-airline-2026-09-24-episodes/baseline" --arm "D0=$E/pilot-airline-2026-09-24-episodes/flows" \
  --arm "habit, five sessions=$E/cold-start-live-airline-2026-09-25-episodes/habit" \
  --arm "habit, five sessions, retail arbiter=$E/cold-start-live-airline-2026-09-25-episodes/flows"
```
