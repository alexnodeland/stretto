# A shipped arbiter, across domains

[The cold start](cold-start-2026-09-24.md) found that a deployment's first few sessions are too few to fit an arbiter on: fitted on 3 to 11 of their decisions, it did worse than the habit alone. An arbiter fitted on four other agents' 4,513 retail decisions did better. It stayed within two points of the habit alone on four random draws of five sessions, and lifted a narrow draw from 2.7% to 11.1% of turns. But that arbiter came from the same domain as the sessions. A deployment in a new domain has none from its own. This page ships two arbiters with stretto, and tests each on the domain it was not fitted on.

## What ships

- **`compile --pooled-arbiter`** fits one arbiter on every held-out decision, where a compiled flow otherwise gets five cross-fitted ones.
- **`stretto export-arbiter`** writes a flow's arbiter to its own file: the eight weights, Jev's agreement with the agents at each tool, the three yes/no questions it weighs, and the model it asks.
- **`learn --arbiter-from`** now also reads such a file. It learns the habit from every session, asks Jev nothing while learning, and serves the habit with the arbiter.
- **[`data/arbiters/`](../../data/arbiters/)** holds the retail and the airline arbiter, fitted on τ²-bench's published 2025 trajectories of four agents: 4,513 held-out retail decisions and 2,042 airline ones. The answers they were fitted on are in the published bundles, so both rebuild without a key.

At a tool it never saw, as at every tool of another domain, an arbiter weighs Jev's answers by their agreement with the agents over all the sites it was fitted on: 75.2% in retail, 66.2% in airline.

## Method

- **The habits** are [the cold start](cold-start-2026-09-24.md)'s: learned from GLM-5's own sessions, trial 0 of each training task drawn. In retail: four random draws of five tasks, a clustered one, and ten random tasks. In airline: five random tasks, five clustered, ten and twenty random, and all thirty.
- **Each habit is served three ways:**
  - alone (the cold start's habit-only flows);
  - with its own domain's arbiter, fitted on four other agents' held-out decisions, cross-fitted by task, as D0's is (in retail, the cold start's "other agents' arbiter");
  - with the other domain's shipped arbiter (`--arbiter-from data/arbiters/<other>.json`).
- **The replays** are the cold start's: GLM-5's test episodes, all four trials, lookup first at 0.3. Differences are paired by episode and bootstrapped over tasks (4,000 draws).
- The shipped arbiter of a domain is not replayed on that domain's test tasks. It was fitted on other agents' decisions at those very tasks, so it would be judged on what it saw.

## Results

Turns saved on GLM-5's test episodes, with detours in brackets. The retail habits' own-domain arbiter is retail's, fitted on four other agents' decisions, and the shipped one airline's; for the airline habits, the other way round.

| Habit learned from | Habit alone | With its own domain's arbiter | With the other domain's shipped arbiter | Shipped minus habit alone, points (95% interval) | Shipped minus own domain's |
|---|---|---|---|---|---|
| Retail, 5 sessions, draw 1 | 18.9% (30) | 19.0% (57) | 19.1% (35) | +0.3 (−1.7 to +2.6) | +0.1 (−0.9 to +1.2) |
| Retail, 5 sessions, draw 2 | 18.9% (30) | 17.8% (28) | 17.9% (28) | −0.9 (−2.7 to +1.0) | +0.1 (−0.6 to +0.9) |
| Retail, 5 sessions, draw 3 | 11.8% (18) | 9.9% (16) | 10.1% (16) | −1.7 (−3.0 to −0.3) | +0.2 (−0.4 to +0.8) |
| Retail, 5 sessions, draw 4 | 17.8% (30) | 18.0% (18) | 17.7% (18) | −0.1 (−2.0 to +1.9) | −0.3 (−1.1 to +0.5) |
| Retail, 5 sessions, clustered | 2.7% (16) | 11.1% (18) | 10.6% (17) | +7.9 (+6.2 to +9.7) | −0.5 (−1.6 to +0.5) |
| Retail, 10 sessions, random | 20.9% (51) | 19.5% (54) | 19.7% (47) | −1.2 (−2.9 to +0.5) | +0.1 (−0.8 to +1.1) |
| Airline, 5 sessions, random | 8.7% (85) | 6.7% (54) | 6.0% (48) | −2.7 (−4.7 to −1.0) | −0.6 (−2.2 to +1.0) |
| Airline, 5 sessions, clustered | 8.7% (85) | 7.0% (64) | 7.9% (60) | −0.8 (−2.0 to +0.0) | +1.0 (+0.2 to +2.0) |
| Airline, 10 sessions, random | 8.7% (85) | 6.7% (58) | 5.9% (51) | −2.9 (−4.9 to −1.1) | −0.8 (−2.5 to +0.9) |
| Airline, 20 sessions, random | 8.7% (85) | 6.8% (16) | 5.7% (14) | −3.0 (−5.1 to −1.1) | −1.1 (−2.8 to +0.6) |
| Airline, all 30 tasks | 8.7% (85) | 6.3% (23) | 5.6% (20) | −3.2 (−5.2 to −1.3) | −0.8 (−2.5 to +0.9) |

- **The arbiter transfers.** Served with a retail habit, the airline arbiter saved what retail's own did, within half a point on all six habits. Served with an airline habit, the retail arbiter came within about a point of airline's own, from −1.1 to +1.0 points.
- **In retail it is insurance, as before.** With the airline arbiter, the four random five-session habits saved between 1.7 points less and 0.3 points more than the habit alone. The narrow draw gained 7.9 points, from 2.7% to 10.6% of turns. Over the five draws, the mean rose from 14.0% to 15.1% (15.2% with retail's own arbiter), and the worst draw from 2.7% to 10.1%.
- **In airline it trades savings for precision, as airline's own arbiter does.** Both save 1 to 3 points less than the habit alone, and look up far less that the agent never needed: 14 to 64 detours against 85, and at 20 or 30 sessions about a fifth as many. [The arms replay](arms-2026-09-24.md) found the same trade for the arbiter on every task.
- **So one arbiter can ship with the compiler.** In a new domain it does what that domain's own arbiter would: it insures against a narrow start, and trades a point or two of savings for fewer wasted lookups. A deployment in a domain of its own can start with the habit from its first sessions and a shipped arbiter, and fit its own arbiter once it has some twenty sessions. Either file serves; `retail.json` was fitted on twice the decisions.
- **What carries over is the weights, not Jev's record at each tool.** Tool names differ between the domains, so across them the arbiter weighs Jev's answers by their overall agreement with the agents, 75.2% for the retail arbiter and 66.2% for the airline one. The habit's weight is close in both (0.27 and 0.25), and it is the habit that carries a flow's savings.

## Cost

No LLM ran. Fitting the two shipped arbiters asked nothing: their answers are in the published bundles. The replays asked Jev 718 new questions, 1.8M input tokens, $0.08. They are published in [answers-2026-09-24-arbiter-transfer.jsonl.gz](answers-2026-09-24-arbiter-transfer.jsonl.gz) ([its page](answers-2026-09-24-arbiter-transfer.md)).

## Reproduce

[The CLI reference](../cli.md) lists every option.

Import the answer bundles as [this round's bundle page](answers-2026-09-24-arbiter-transfer.md) shows, then build each flow and replay it as in [the cold start](cold-start-2026-09-24.md#reproduce). For example, a retail habit from five sessions with the airline arbiter:

```sh
stretto learn --tau2 ../tau2-bench --domain retail --results .data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --trials 0 --train-fraction 0.0676 --arbiter-from data/arbiters/airline.json \
  --out flows/retail-5-airline.flow.json
```

The arbiter file brings its own predicates, so `learn` takes no `--predicates` with `--arbiter-from`.

The airline habits' own-domain arbiter is the all-task airline flow's (`stretto compile --domain airline ...`, as [the sweep](sweep-2026-09-24.md#reproduce) builds it), taken with `--arbiter-from` that flow. Per-episode rows are in [arbiter-transfer-2026-09-24.json](arbiter-transfer-2026-09-24.json).
