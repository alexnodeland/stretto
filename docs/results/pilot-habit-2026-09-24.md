# Live habit-only pilot, 2026-09-24: the flow without Jev, in retail

Replayed on recorded episodes, a flow that decides on the habit alone saves as many turns as D0's arbiter, which weighs the habit against Jev's answers and the predicates ([arms](arms-2026-09-24.md)). This pilot checks that live.

- **Setup:** GLM-5.3 in Claude Code as the agent, GLM-5.3 as the customer, and the same ten retail test tasks as [the retail pilot](pilot-2026-09-24.md).
- **The habit arm:** the flow runs behind the tools as in D0, but decides with `--decider habit`, so it never asks Jev.
- **Comparisons:** the no-flow runs and the D0 runs on these tasks are the retail pilot's own, recorded earlier the same day. Only the habit arm ran now, one episode per task.

| | No flow | D0: arbiter (with Jev) | Habit alone (no Jev) |
|---|---|---|---|
| LLM turns | 110 | 84 (−23.6%) | **79 (−28.2%)** |
| Turns saved per episode, 95% interval | | 2.6 (1.0 to 4.2) | 3.1 (1.7 to 4.5) |
| Pairs with fewer turns | | 9 of 10 | 10 of 10 |
| Agent input tokens | 723,926 | 571,528 (−21.1%) | 550,910 (−23.9%) |
| Flow lookups (the agent repeated) | | 26 (3) | 37 (0) |
| Passed the database check | 8 of 10 | 8 of 10 | 9 of 10 |
| Z.ai credits, off-peak | 128.5 | 108.0 | 109.1 |

- **Live, the habit alone does what D0 does.** Against D0 it changed the turns per episode by −0.5 (95% interval −1.4 to +0.4). That is no difference that ten tasks can resolve, and it matches the replay.
- **It looked up more, and the agent used all of it.** The habit flow made 37 lookups to D0's 26, and the agent repeated none of them. It repeated 3 of D0's.
- **Passes.** The extra pass is task 27. It failed with no flow and with D0 (a return processed before the exchange it blocked), and passed with the habit flow. One episode per arm cannot credit that to the flow. Task 64 failed in all three arms.
- **Cost.** The habit arm cost 109.1 credits, under the 150 approved. It asked Jev nothing.

So the finding of the replays holds live. For the savings, a read-only flow needs its habit, not a System-One model. The runs were made at different times on the same day, so model drift between them cannot be ruled out, and ten tasks cannot bound a pass-rate effect.

`pilot-habit-2026-09-24.json` holds the per-episode numbers of all three arms. The habit arm's episodes are in [pilot-habit-2026-09-24-episodes.tar.gz](pilot-habit-2026-09-24-episodes.tar.gz), and the other two arms' in the retail pilot's archive; see [the episodes page](episodes-2026-09-24.md).
