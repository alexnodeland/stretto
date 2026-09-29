# The named-other count, live: the habit alone against D0

[The flow search](search-2026-09-25.md) found flows that made far fewer detours than D0 in replay, by setting a threshold per site. [The named-other count](named-other-2026-09-26.md) then reached the same decisions with no search. The count is how often the agent went on to read a record the customer had not named. D0 compiled again with it and served on the habit alone was replayed on GLM-5's 240 test-task episodes. In airline it saved 55 turns with 6 detours, where D0 saved 41 with 26. In retail it saved 307 turns with 33 detours, where D0 saved 294 with 58. A replay assumes the agent acts the same with the flow's results in hand. This round runs the two flows live, paired, on the pilots' ten tasks per domain ([#34](https://github.com/alexnodeland/stretto/issues/34)).

## Findings

- **Live, both flows made almost no detours.**
  - D0 made 1 detour in each domain: 1 of its 30 retail lookups and 1 of its 22 airline lookups.
  - The habit with the count made 2 of 33 in retail and none of 25 in airline.
  - On these twenty tasks, with GLM-5.3, there were few detours for the count to remove. D0 made 0.1 an episode in each domain. In the replays of GLM-5's episodes that motivated the count, it made 0.36 in retail (58 in 160) and 0.33 in airline (26 in 80).
- **The habit with the count saved as many turns as D0, or more.**
  - In retail it took 80 LLM turns, and D0 78, against 110 without a flow. Paired task by task, the habit took 0.2 more turns an episode (95% interval −0.4 to +0.9).
  - In airline it took 95, and D0 118, against 127. That is 2.3 fewer an episode (−6.5 to +0.4), 19.5% fewer turns than D0.
  - D0's airline arm saved less than D0 did in the airline pilot: 7.1% of turns, against 17.3% there. D0's two recordings on the same tasks, the pilot's and this one, differ by that much. Ten tasks, one episode each, cannot separate the agent's and the simulated customer's variation from a flow's.
- **Passes did not fall.**
  - In retail, both passed 9 of 10, one more than without a flow. D0 failed task 27 and the habit failed task 64, the two tasks the agent also failed without a flow.
  - In airline, the habit passed 9 and D0 7, against 8 without a flow. The habit's two extra passes were the agent's own decisions:
    - On task 31, asked for an exception on a basic-economy booking, D0's agent moved it to economy on other flights, as the pilot's D0 agent had. The task expects no change, and the habit's agent made none.
    - On task 18, both agents made the five changes the task expects. D0's then added a paid bag to one reservation, which the task does not expect. The habit's agent made its calls a few at a time, 18 calls in 6 turns, where D0's took 25 turns.
  - With every check in τ²-bench's reward basis, the retail assertions judged by Claude Haiku 4.5, the habit passed 8 of 10 in retail. It also lost task 68, the task where the judge splits on whether listing every order's total tells the customer the one they asked for ([scoring](scoring-2026-09-29.md)).
- **What this settles.** The habit with the named-other count, asking no model, is no worse live than D0 with Jev on these tasks, in turns or passes. That was the replay's claim. Whether it avoids D0's detours live needs an agent and tasks where D0 makes them. With GLM-5.3 on the pilots' tasks, D0 barely did.

## Setup

- **Agent and customer.** GLM-5.3 in Claude Code on Z.ai's GLM Coding Plan, with τ²-bench's prompts, as in the pilots.
- **Tasks.** The pilots' ten test tasks per domain: retail 17, 18, 27, 36, 51, 60, 64, 68, 77 and 101, and airline 6, 8, 16, 18, 24, 25, 26, 30, 31 and 37.
- **Arms.** Each task ran once in each arm, the two arms side by side (`pilot/run_paired.py --arms`).
  - *D0.* The pilots' flow as it compiles today (`stretto compile --questions v2 --predicates data/predicates-v2.json`, from τ²-bench's four 2025 runs and the cached answers), without the named-other count. It is served with its arbiter at 0.3 and asks Jev live.
  - *The habit with the count.* The same compile with the count, served on the habit alone at 0.3. It asks no model.

  The two flow files differ only in `bindings.named_other`.
- **Reference.** The pilots' recorded episodes without a flow, on the same tasks. They give the turns saved, and they tell a flow's lookup apart: it is the agent's own when the agent made the same call without the flow, and otherwise a detour. D0's pilot episodes, recorded on 2026-09-24, are shown beside it.
- **Rewards.** τ²-bench's database check, and every check in its reward basis. The retail assertions were judged once each by Claude Haiku 4.5 (`run_episode.py --judge`).
- **Cost.** 554 Z.ai credits for the 40 episodes, 215 in retail and 339 in airline, under the runner's weekly and five-hour caps. D0's 158 decisions asked Jev.

## Retail

| | Without a flow | D0, the pilot (2026-09-24) | D0 | The habit with the count |
|---|---|---|---|---|
| LLM turns | 110 | 84 | 78 | 80 |
| Fewer than without a flow (95%) | | 23.6% (14.3% to 32.8%) | 29.1% (19.1% to 37.4%) | 27.3% (16.4% to 37.1%) |
| Agent input tokens | 723,926 | 571,528 | 520,704 | 553,964 |
| Flow lookups: the agent's own, detours | | 26, 0 | 29, 1 | 31, 2 |
| Lookups the agent made again | | 3 | 1 | 0 |
| Passed the database check | 8 | 8 | 9 | 9 |
| Passed every check | 7 | 8 | 9 | 8 |
| Z.ai credits | 128.5 | 108.0 | 103.8 | 111.6 |

| Task | Turns: without, D0, the habit | Lookups: D0, the habit | Passed: without, D0, the habit |
|---|---|---|---|
| 17 | 6, 7, 6 | 1, 2 | ✓ ✓ ✓ |
| 18 | 11, 8, 8 | 2, 3 | ✓ ✓ ✓ |
| 27 | 14, 7, 7 | 5, 5 | ✗ ✗ ✓ |
| 36 | 16, 11, 14 | 6, 2 | ✓ ✓ ✓ |
| 51 | 10, 5, 5 | 2, 5 | ✓ ✓ ✓ |
| 60 | 6, 5, 5 | 1, 2 | ✓ ✓ ✓ |
| 64 | 11, 8, 8 | 3, 4 | ✗ ✓ ✗ |
| 68 | 9, 8, 9 | 4, 4 | ✓ ✓ ✓ |
| 77 | 8, 6, 6 | 2, 2 | ✓ ✓ ✓ |
| 101 | 19, 13, 12 | 4, 4 | ✓ ✓ ✓ |

Passed is by the database check. With every check, task 68 fails without a flow and with the habit.

## Airline

| | Without a flow | D0, the pilot (2026-09-24) | D0 | The habit with the count |
|---|---|---|---|---|
| LLM turns | 127 | 105 | 118 | 95 |
| Fewer than without a flow (95%) | | 17.3% (8.5% to 25.5%) | 7.1% (−3.5% to 17.4%) | 25.2% (4.0% to 49.3%) |
| Agent input tokens | 996,580 | 849,367 | 1,010,768 | 730,014 |
| Flow lookups: the agent's own, detours | | 23, 1 | 21, 1 | 25, 0 |
| Lookups the agent made again | | 0 | 0 | 1 |
| Passed (database × communication) | 8 | 9 | 7 | 9 |
| Z.ai credits | 176.4 | 157.6 | 188.2 | 150.8 |

| Task | Turns: without, D0, the habit | Lookups: D0, the habit | Passed: without, D0, the habit |
|---|---|---|---|
| 6 | 13, 15, 12 | 0, 0 | ✗ ✗ ✗ |
| 8 | 8, 9, 9 | 4, 5 | ✓ ✓ ✓ |
| 16 | 8, 8, 7 | 2, 2 | ✓ ✓ ✓ |
| 18 | 28, 25, 6 | 4, 6 | ✗ ✗ ✓ |
| 24 | 22, 21, 22 | 1, 1 | ✓ ✓ ✓ |
| 25 | 9, 8, 7 | 1, 1 | ✓ ✓ ✓ |
| 26 | 9, 7, 9 | 2, 2 | ✓ ✓ ✓ |
| 30 | 8, 8, 8 | 1, 1 | ✓ ✓ ✓ |
| 31 | 7, 8, 4 | 3, 3 | ✓ ✗ ✓ |
| 37 | 15, 9, 11 | 4, 4 | ✓ ✓ ✓ |

## Limits

- **Ten tasks, one episode each.** D0's two recordings on the same airline tasks differ by ten points of turns saved. A difference between the arms smaller than that is not resolved here, and passes differ by a task or two.
- **The detours the count removes were not there.** The replays that motivated the count found D0's detours in GLM-5's episodes, over 240 of them. GLM-5.3 on these twenty tasks gave D0 two places to make one.
- **One judging.** The every-check rewards come from one Haiku 4.5 judging per episode, not the three by majority of [the scoring page](scoring-2026-09-29.md).

## Reproduce

The two flows are published: [D0](named-other-live-2026-09-29-d0-retail.flow.json) and [with the count](named-other-live-2026-09-29-named-retail.flow.json) in retail, and [D0](named-other-live-2026-09-29-d0-airline.flow.json) and [with the count](named-other-live-2026-09-29-named-airline.flow.json) in airline. The episodes are in [named-other-live-2026-09-29-episodes.tar.gz](named-other-live-2026-09-29-episodes.tar.gz), laid out as the paired run's. Each episode's `flow.jsonl` holds every decision its flow made, with Jev's answers in D0's.

```sh
# the runs (GLM-5.3; ZAI_API_KEY, and TYPESAFE_API_KEY for D0's arbiter)
python pilot/run_paired.py retail --arms flows=named-other-live-2026-09-29-d0-retail.flow.json \
  habit=named-other-live-2026-09-29-named-retail.flow.json \
  --tasks 17 18 27 36 51 60 64 68 77 101 --oracle-cache CACHE --judge claude:claude-haiku-4-5 --out paired-retail
python pilot/run_paired.py airline --arms flows=named-other-live-2026-09-29-d0-airline.flow.json \
  habit=named-other-live-2026-09-29-named-airline.flow.json \
  --tasks 6 8 16 18 24 25 26 30 31 37 --oracle-cache CACHE --judge claude:claude-haiku-4-5 --out paired-airline
# the tables, against the pilots' episodes without a flow (in paired-2026-09-25-episodes)
python pilot/analyze_paired.py arms --tasks 17 18 27 36 51 60 64 68 77 101 \
  --arm baseline=paired-2026-09-25-episodes/retail/baseline --arm d0-pilot=paired-2026-09-25-episodes/retail/flows \
  --arm d0=named-other-live-2026-09-29-episodes/retail/flows --arm named=named-other-live-2026-09-29-episodes/retail/habit
python pilot/analyze_paired.py arms --tasks 17 18 27 36 51 60 64 68 77 101 \
  --arm d0=named-other-live-2026-09-29-episodes/retail/flows --arm named=named-other-live-2026-09-29-episodes/retail/habit
```

The airline tables come from the same commands with the airline tasks. The second command pairs the two flows directly.
