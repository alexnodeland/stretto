# Cold start: a flow from an agent's own first sessions

[The fewer-traces sweep](sweep-2026-09-24.md) found that with few traces, Jev's answers carry a flow's savings. But it shrank only the habit's data. The arbiter was still fitted on thousands of the source agents' held-out decisions at every size, and its samples turned out to be clustered. A deployment starting out has only its own first sessions. `stretto learn` holds out 30% of them and fits the arbiter there, so with five sessions it has 3 to 15 decisions to fit eight weights on. This page tests that setup two ways:

- offline, on GLM-5's own published sessions, drawn at random;
- live, on five sessions GLM-5.3 ran through the proxy, then on three of the retail pilot's tasks.

In short: from a deployment's first few sessions, an arbiter fitted on those sessions does worse than the habit alone, and which sessions they are matters more than how the flow decides.

## What changed in `stretto learn`

Three changes make `learn` work from a handful of sessions:

- **The split.** `learn` puts every session of a task on one side. It holds out 30% of the tasks, at least one, and leaves at least one to train the habit. A fixed pseudo-random order picks them.
  - Before, a task went to the held-out side when its FNV hash ended in 7, 8 or 9. That rule can leave either side empty, and three airline tasks all landed on the training side, so `learn` refused.
  - It also tied the split to the folds: a hash ending in 7, 8 or 9 is 2, 3 or 4 mod 5. So held-out tasks never fell in two of the five folds.
- **One arbiter.** A learned flow judges new sessions, which no fold has seen. So it gets one arbiter, fitted on every held-out decision, in place of one per fold. Before, with one held-out task, one fold's arbiter was fitted on nothing, and the sessions that hashed to it met an arbiter that could not decide.
- **The sample.** `--train-fraction` ranked tasks by an FNV hash that barely mixes the last characters of an id. So neighbouring task ids sorted together (see [the sweep's correction](sweep-2026-09-24.md#correction-the-samples-are-clustered-not-random)). The order is now mixed first, for `learn`'s split too.

Four additions serve the tests here:

- **`learn --results`** learns from τ²-bench results as if a deployment had recorded them. It takes the episodes on a checkout's training tasks, a `--train-fraction` of them or the `--train-tasks` named, and only the `--trials` named.
- **`learn --habit-only`** learns a flow of the habit alone from every session. It asks nothing and needs no key, and it serves with `--decider habit`.
- **`learn --refit-habit`** fits the arbiter on the held-out 30% as before. Then it learns the habit, the sites and the bindings again from every session, as stacking refits its base model once the combiner is fitted.
- **`learn --arbiter-from <flow>`** learns the habit from every session and asks nothing. The flow serves the arbiter of another flow instead. Here that is the arbiter D0's compile fitted on four other agents' held-out decisions.

## Offline: GLM-5's own sessions

### Method

- **The sessions.** GLM-5's published retail and airline results, trial 0 of each training task drawn: one session per task, as a deployment's first sessions would be. They were drawn four ways:
  - **random:** `--train-fraction`, now a mixed order: 5, 10, 20 and 40 of retail's 74 training tasks, and 5, 10 and 20 of airline's 30;
  - **three more random draws** of five retail tasks, named with `--train-tasks`;
  - **clustered:** the old sampler's order, five and ten retail tasks and five airline tasks, like the sweep's samples;
  - **all:** every training task.
- **The flows.** From each draw, `learn --results`:
  - **the arbiter flow:** 70% of the tasks train the habit, the sites and the bindings, and the arbiter is fitted on Jev's answers at the other 30%;
  - **the habit alone:** every task trains the habit, and nothing is asked (`--habit-only`);
  - at five and ten sessions, **refitted:** the arbiter flow's arbiter, with the habit, sites and bindings of every session (`--refit-habit`);
  - at five and ten retail sessions, **with other agents' arbiter:** the habit alone's habit, sites and bindings, with the arbiter of the all-task flow compiled from four other agents (`--arbiter-from`). That arbiter was fitted on 4,513 held-out decisions, cross-fitted by task, as D0's is.
- **The replays.** Each flow replayed GLM-5's test episodes, all four trials (160 in retail, 80 in airline), lookup first at 0.3. That is the sweep's test, so the numbers compare with [its table](sweep-2026-09-24.md). Differences are paired by episode and bootstrapped over tasks (4,000 draws).

### Results

Turns saved on GLM-5's test episodes, with detours in brackets. First, by size, on the random samples and on every task:

| Domain | Sessions | Arbiter flow: habit's successful sessions, arbiter's decisions | Arbiter flow: turns saved (detours) | Habit alone: successful sessions | Habit alone: turns saved (detours) | Habit alone minus arbiter flow, points (95% interval) |
|---|---|---|---|---|---|---|
| Retail | 5 | 2, 11 | 9.2% (17) | 3 | 18.9% (30) | +9.6 (+8.0 to +11.1) |
| Retail | 10 | 4, 20 | 11.6% (25) | 6 | 20.9% (51) | +9.2 (+7.1 to +11.4) |
| Retail | 20 | 8, 36 | 22.6% (31) | 13 | 20.4% (34) | −2.2 (−4.6 to −0.1) |
| Retail | 40 | 17, 75 | 23.9% (160) | 27 | 22.4% (58) | −1.5 (−3.6 to +0.6) |
| Retail | 74, all | 38, 101 | 22.3% (47) | 56 | 22.4% (51) | +0.1 (−1.2 to +1.6) |
| Airline | 5 | 3, 9 | 6.2% (30) | 5 | 8.7% (85) | +2.5 (+0.6 to +4.8) |
| Airline | 10 | 7, 13 | 4.3% (41) | 10 | 8.7% (85) | +4.4 (+2.0 to +7.1) |
| Airline | 20 | 13, 36 | 6.3% (20) | 19 | 8.7% (85) | +2.4 (+0.6 to +4.4) |
| Airline | 30, all | 19, 52 | 6.2% (39) | 28 | 8.7% (85) | +2.5 (+0.8 to +4.6) |

- **The habit alone is the steadier start.** From five or ten random retail sessions it saves 18.9% and 20.9% of turns, near the 22.4% it saves from all 74 tasks. In airline it saves 8.7% from five sessions, what it saves from all 30. It asks nothing.
- **The arbiter flow trails it until about 20 sessions.** With 5 and 10 retail sessions it saves 9.2% and 11.6%, and in airline less than the habit alone at every size, all 30 tasks included. With 20 and 40 retail sessions it saves 22.6% and 23.9%, at 40 with nearly three times the detours. With every task the two tie, 22.3% and 22.4%.
- **Two things hold it back.**
  - The split takes 30% of the sessions from the habit, the sites and the bindings: with five sessions, the habit learns from two successful ones.
  - Eight weights fitted on a handful of decisions can land anywhere. Across the five-session draws below, the weight on the habit ranged from −0.32 to 0.30. With 20 decisions or more it settled between 0.88 and 1.15.

Then five draws of five retail sessions, and the two ways of mending the arbiter flow. The refitted flow keeps the arbiter fitted on the held-out sessions, with the habit, sites and bindings of all five. The other agents' arbiter comes with the habit alone's habit, sites and bindings:

| Draw | Tasks | Arbiter's decisions | Arbiter flow | Habit alone | Refitted | Other agents' arbiter | Other agents' arbiter minus habit alone, points (95% interval) |
|---|---|---|---|---|---|---|---|
| Draw 1 | 15, 24, 76, 88, 99 | 11 | 9.2% (17) | 18.9% (30) | 22.6% (355) | 19.0% (57) | +0.1 (−2.3 to +2.8) |
| Draw 2 | 10, 20, 22, 47, 112 | 3 | 16.5% (25) | 18.9% (30) | 16.5% (25) | 17.8% (28) | −1.1 (−3.0 to +1.0) |
| Draw 3 | 8, 13, 14, 28, 75 | 7 | 6.5% (12) | 11.8% (18) | 7.6% (12) | 9.9% (16) | −1.9 (−3.3 to −0.4) |
| Draw 4 | 21, 44, 76, 93, 107 | 7 | 9.3% (18) | 17.8% (30) | 1.9% (7) | 18.0% (18) | +0.1 (−1.9 to +2.4) |
| Clustered | 104–107, 109 | 7 | 7.9% (16) | 2.7% (16) | 9.2% (16) | 11.1% (18) | +8.5 (+6.6 to +10.3) |

With ten retail sessions:

| Draw | Arbiter's decisions | Arbiter flow | Habit alone | Refitted | Other agents' arbiter |
|---|---|---|---|---|---|
| Random | 20 | 11.6% (25) | 20.9% (51) | 21.1% (46) | 19.5% (54) |
| Clustered | 15 | 18.2% (39) | 21.2% (51) | 18.9% (37) | not run |

In airline, from five random sessions, the refitted flow saves 6.0% (40 detours), against 8.7% for the habit alone. On the clustered airline draw, the arbiter flow and the habit alone both save 8.7%.

- **Which five sessions matters more than how the flow decides.** On the four random draws the habit alone saves 11.8% to 18.9%, more than the arbiter flow on each, by 2.3 to 9.6 points. On the clustered draw it saves 2.7%. One of its sessions looked an order up straight after finding the user, so after the user's details the habit expects the next order at 0.31 at the median, against 0.75 on draw 1. Weighted by the binding, that is under the 0.3 rule, and the flow hands back.
- **The refit is no remedy.** It gains where the habit alone is weak, 6.6 points on the clustered draw, and loses up to 16 points elsewhere. On draw 4 its arbiter puts a weight of −0.32 on the habit, overrules the refitted habit, and saves 1.9%. On draw 1 it saves 22.6%, with 355 detours: its arbiter, fitted on 11 decisions, meets sites the held-out sessions never showed, such as a product lookup after a product lookup, and looks up there. With ten sessions it saves no more than the habit alone.
- **Other agents' arbiter is insurance.** Fitted on 4,513 decisions, it stays within two points of the habit alone on the four random draws (−1.9 to +0.1) and adds 8.5 on the clustered one. Across the five draws it has the highest floor, 9.9%, and the highest mean, 15.2% against the habit alone's 14.0%. That is still far below the sweep's 20.4% from three clustered tasks, whose habit learned from 21 successful episodes of four agents.

## Live: GLM-5.3's first five sessions

- **Recording.** GLM-5.3 in Claude Code ran five retail training tasks through the proxy, with the conversation handed to it (`pilot/run_episode.py --record-context`). They were tasks 15, 24, 76, 88 and 99, the five-task random sample. All five passed τ²-bench's database check.
- **Learning.** `stretto learn --sessions` learned a flow from the five proxy logs. Three sessions trained the habit, and two held out the 15 decisions its arbiter is fitted on. It knows 5 sites and 6 lookups ([the flow](cold-start-2026-09-24-live.flow.json)).
- **Offline check.** Before it ran live, the flow replayed GLM-5's 40 trial-0 test episodes. It saved 66 of 347 turns (19.0%) with 14 detours. D0, compiled from 831 episodes of four other agents, saves 77 (22.2%) on the same episodes.
  - On those 40 episodes, the flows learned offline from GLM-5's own sessions on the same five tasks saved 8.6% with their arbiter and 18.7% on the habit alone.
  - This flow's arbiter, fitted on 15 decisions, put a weight of 1.15 on the habit, where GLM-5's, fitted on 11, put 0.19. So it decided much as the habit alone would.
- **Live.** It then ran on the retail pilot's tasks, costliest first, paired with that pilot's episodes without a flow and with D0, and with the habit-only pilot's:

| Task | No flow | D0 | Habit alone | Learned from five sessions |
|---|---|---|---|---|
| 101 | 19 turns, passed | 16, passed | 14, passed | **12**, passed |
| 36 | 16, passed | 11, passed | 13, passed | 14, passed |
| 27 | 14, failed | 8, failed | 7, passed | 26, failed |

- **Tasks 101 and 36.** The learned flow made its lookups and the agent used them. It saved 7 turns on task 101 and 2 on task 36.
- **Task 27.**
  - The agent failed it as it had without the flow and with D0. It filed the return the customer asked for first, which blocks the exchange they also wanted, and then transferred them to a human.
  - The flow looked up the customer's details and all four of their orders. The agent looked up the boots' product itself.
  - The simulated customer then carried on after each transfer, for 19 more turns. τ²-bench's customer is told to end the conversation at a transfer. The pilot harness now ends an episode there.
- **Budget.** That one episode cost 61 credits, so the run stopped after three tasks: a fourth, task 68, was stopped a minute in. One more episode could have taken the round past its 200-credit budget. Three tasks show that a flow learned from a few recorded sessions runs live, but not how many turns it saves. The offline check is the better measure of that.

## What this means

- **A flow can start from a deployment's first handful of sessions.** The habit alone, learned from every one of them, saved 17.8–18.9% of turns on three of four random draws of five, against 22.4% from every training task, and needs no key. That revises [the sweep](sweep-2026-09-24.md)'s reading that Jev's answers carry the savings when traces are scarce. That held for its clustered samples, with an arbiter fitted on thousands of other agents' decisions. It does not hold for a random handful of an agent's own sessions.
- **Fit a deployment's own arbiter once it has some twenty sessions.** Below that, the held-out split costs the habit more than the arbiter gives back.
- **Where an arbiter fitted elsewhere exists, it is cheap insurance against a narrow start.** It raised the narrow draw from 2.7% to 11.1% of turns and cost at most two points on the others. It need not come from the same domain: [an arbiter fitted on airline decisions](arbiter-transfer-2026-09-24.md) did the same for these retail habits, and two such arbiters now ship in `data/arbiters/`.
- **Five draws are still few.** The spread between them, 2.7% to 18.9% for the habit alone, is as large as any difference between deciders on one draw.

## Cost

- **Z.ai, 182 credits of the 200 approved:** 79.2 to record the five sessions, 96.6 for the three live episodes, and about 6.5 for a fourth episode, task 68, stopped a minute in when the budget ran short.
- **Jev, $0.62:** 5,447 answers, 14.8M input tokens. That is the held-out questions `learn` asked, with a few others ($0.06), the replays of every flow that asks Jev ($0.56), and the live flow's questions (under a cent).

## Reproduce

Import the answer bundles as [this round's bundle page](answers-2026-09-24-cold-manifest-match.md) shows, and fetch GLM-5's published results with `scripts/fetch-leaderboard.sh`. Each flow, from the cache:

```sh
R=.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json
learn() { stretto learn --tau2 ../tau2-bench --domain retail --results $R --trials 0 \
  --oracle replay --oracle-cache .oracle-cache --predicates data/predicates-v2.json "$@"; }
learn --train-fraction 0.0676 --out flows/cold-retail-5.flow.json                  # the arbiter flow
learn --train-fraction 0.0676 --habit-only --out flows/cold-retail-5-habit.flow.json
learn --train-fraction 0.0676 --refit-habit --out flows/cold-retail-5-refit.flow.json
stretto compile --tau2 ../tau2-bench --domain retail --oracle replay --oracle-cache .oracle-cache \
  --questions v2 --predicates data/predicates-v2.json --out flows/retail.flow.json   # every task, four other agents
learn --train-fraction 0.0676 --arbiter-from flows/retail.flow.json --out flows/cold-retail-5-transfer.flow.json
```

- **Random samples:** retail `--train-fraction` 0.0676, 0.1351, 0.2703, 0.5405 and 1 give 5, 10, 20, 40 and 74 tasks; airline 0.1667, 0.3333, 0.6667 and 1 give 5, 10, 20 and 30 (with the airline results file).
- **The other draws** name their tasks in place of `--train-fraction`: `--train-tasks 10,20,22,47,112`, `8,13,14,28,75` and `21,44,76,93,107` (draws 2 to 4), and the clustered `104,105,106,107,109` and `98,103,104,105,106,107,109,110,112,113` in retail and `40,41,42,43,49` in airline.

Then replay GLM-5's test episodes through each flow, as in [the sweep](sweep-2026-09-24.md#reproduce):

```sh
cd pilot
python check_flow.py --domain retail --flow ../flows/cold-retail-5.flow.json --trials 0 1 2 3 --results ../$R \
  --oracle-cache ../.oracle-cache --flow-oracle replay --flow-decider arbiter --flow-threshold 0.3 --out runs/cold-retail-5
```

with `--flow-decider habit` for the habit-only flows. Every question the replays ask is in the bundles, so none needs a key.

The live flow itself is published: [cold-start-2026-09-24-live.flow.json](cold-start-2026-09-24-live.flow.json). It holds the habit's counts, the sites, the bindings and the arbiter, and no transcript. `pilot/check_flow.py --flow` replays it, and `pilot/run_episode.py --arm flows --flow` serves it. Learning it again needs the five sessions' proxy logs. Like the pilots' transcripts, they are not published. With them, the command was:

```sh
stretto learn --sessions sessions/ --domain retail --manifest retail-manifest.json --rewards rewards.json \
  --predicates data/predicates-v2.json --oracle jev --out cold-live.flow.json
```

`pilot/run_episode.py --record-context` records such sessions. `tau2_mcp.py` does not mark its tools read-only, so the manifest comes from τ²-bench's `tools.py`, as any compiled flow's does.

Every answer these read is in [this round's answer bundle](answers-2026-09-24-cold-manifest-match.md). Per-episode rows for every replay, and the live pairs, are in [cold-start-2026-09-24.json](cold-start-2026-09-24.json).
