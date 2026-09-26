# Learning from the sessions a flow served

A deployment has no clean data after its first flow: every session it records is served, and the flow's own lookups sit in it beside the agent's calls. `stretto-proxy` logs them as calls of its own (`stretto-1`, `stretto-2`, …), and `learn --sessions` reads them as steps of the session, the same as the agent's. Is that the right data to learn the next flow from, and what happens when a flow is learned again and again from sessions it served itself?

## One round: learn from the sessions as they are

The paired run's episodes ([#2](paired-2026-09-25.md)) hold both kinds for the same tasks: GLM-5.3 without a flow, and with one. Three training sets were built from them, two folds of tasks each: the episodes without a flow (**clean**), the episodes with one, the flow's lookups kept as ordinary calls (**as served**), and the same with the flow's lookups deleted (**agent only**).

**The next step.** A smoothed model of the next call after each call, fitted on one fold and scored on the other's episodes without a flow:

| Trained on | Retail: nats per step · top-1 | Airline |
|---|---|---|
| Clean episodes | 0.957 · 70.1% | 1.307 · 65.1% |
| Served episodes, as they are | 0.959 · 72.0% | 1.264 · 65.1% |
| Served, the flow's calls as context only | 1.880 · 37.3% | 2.321 · 48.1% |
| Served, the flow's calls deleted | 1.942 · 37.0% | 2.143 · 48.1% |

**Flows.** Habit-only flows learned from each fold's 20 training episodes, replayed at 0.3 on the other fold's 20 episodes without a flow (LLM turns saved · detours, both folds):

| Trained on | Retail (498 turns) | Airline (545 turns) |
|---|---|---|
| Clean episodes | 154 · 43 | 102 · 48 |
| Served episodes, as they are | 152 · 11 | 108 · 55 |
| Served, the flow's calls deleted | 0 · 0 | 6 · 6 |

Served sessions as they are teach as much as clean ones. Deleting the flow's calls teaches that the agent never looks anything up where the flow always did, so the flow switches itself off; treating them as context only does the same to the model. These flows were learned before [the named-other score](named-other-2026-09-26.md), and airline's were replayed from a results file, whose episodes are named by task and trial.

## Many rounds: it drifted, then it did not

Learn, serve, learn again: the flow of round *r* replays the training fold's episodes, its lookups are kept in the sessions as the agent's calls, the next flow learns from them, and each round's flow is replayed on the held-out fold. The agent's steps are those it recorded without a flow, so the replay measures the flow's side of the loop only. Detours per round, both folds (turns saved moved by at most 2 in every row but the last, which shows turns saved):

| Learner | Sessions learned from | Retail, r0 → r3 | Airline |
|---|---|---|---|
| Before the named-other score | as served | 43 → 68 → 69 → 71 | 48 → 48 → 51 → 51 |
| Before it | the detours dropped (knowing which) | 43 → 49 → 49 → 49 | 48 → 51 → 51 → 51 |
| With it | as served | 15 → 16 → 16 → 16 | 17 → 19 → 19 → 19 |
| With it | the detours dropped (knowing which) | 15 → 16 → 16 → 16 | 17 → 18 → 18 → 18 |
| With it | unused lookups dropped, and bindings scored at the agent's own calls only | turns 154 → 106 → 154 → 106 | turns 95 → 40 → 73 → 23 |

Before, a flow relearned on its own sessions grew bolder each round: its lookups sit right after the call that prompted them, so each round read its chains as the agent's habit, and its readings of records the customer had not asked about as the agent's own. It settled within two or three rounds, but retail's at 65% more detours for 2 more turns. Most of that drift was reading the wrong record, and [the named-other score](named-other-2026-09-26.md) counts exactly that from the sessions themselves. With it, the flow learned from its own sessions does what the one that knew which lookups were detours does.

## What did not work: pruning by use

A lookup whose result nothing later used was probably not needed. Measured on the paired run's served episodes (a value that first came with the lookup's result appears later in the agent's calls or messages), 80% of the flow's lookups that the agent also made without the flow were used, and 5 of the 17 that it did not make (retail 90%, airline 67%). The signal is blind to status checks: none of the six flight-status lookups the agent needed were "used". So `learn` was made to drop the flow's lookups nothing used, and to score each binding at the agent's own calls only, since a flow's lookup was bound by the same rule and agrees with it by construction.

The last row above is the result: savings fell by up to three quarters and swung between rounds. Apart, over one round:

- **Dropping unused lookups** held retail (155 turns, 13 detours, against 155 and 16) and cost airline nearly half its savings (55 turns against 96). A reservation read the agent needed but did not quote is dropped, the next flow learns that the agent does not read reservations there, and the round after it relearns that it does.
- **Scoring bindings at the agent's own calls only** removed the only evidence at the sites the flow always serves: there the agent makes no lookups of its own any more, so the binding's chance falls back to 1/2, and the flow stops.

Both are the same failure of a loop that learns from its own actions, a missing counterfactual where the flow always acts, and neither shipped. The lesson for learning from served sessions is to count the flow's lookups as the agent's steps, and to score a pick by what the sessions show about it, as the named-other score does, rather than to prune by a signal that misses what the agent needed without saying so.

## Reproduce

With the paired run's episodes and τ²-bench's Python environment. The flows are learned with `stretto learn --results … --habit-only` on shadow checkouts whose `split_tasks.json` names one fold as `train` and the other as `test`. Each round replays the training fold with [`pilot/check_flow_tagged.py`](../../pilot/check_flow_tagged.py), which is `check_flow.py` writing which calls were the flow's, and builds the next round's sessions with [`scripts/relearn_round.py`](../../scripts/relearn_round.py) (`unified`, `used` or `marked`).
