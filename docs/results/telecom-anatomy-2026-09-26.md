# Where compiling once works: telecom's troubleshooting

[The anatomy of retail and airline episodes](anatomy-2026-09-26.md) found that what a workflow compiled once from traces can do is the read skeleton, and that the rest of an episode is language. τ²-bench's third domain, telecom ([Barres et al., 2025](https://arxiv.org/abs/2506.07982)), is different in kind: the agent troubleshoots a phone by a procedure, and the procedure branches on what the phone reports. Its policy comes as a manual and, rewritten, as an explicit workflow, and in its solo mode the agent operates the phone itself instead of talking the customer through it. So it can say whether compiling once works when the work is a procedure over tool results.

It does, where the agent runs the tools. The anatomy is the same script, with telecom's read-only tools, features read from the phone's text results, and one more prediction: a decision tree over the whole state.

## Findings

- **With the customer holding the phone, telecom is language like the others.** Replies are 41–62% of the nine leaderboard agents' turns, and their own reads after a tool 16–32%: the phone's checks are the customer's to run, so their results reach the agent in the customer's words.
- **But the agent's decisions follow the state.** A tree over everything its tools returned so far predicts 63–84% of its next steps after a tool, 2–9 points above the best single feature for every one of the nine agents, and rules sure at 95% cover 13–62% of decisions at 96–99.8% held out. In retail and airline the same tree is no better than one feature (from 7 points worse to 8 better; worse for six of the nine agents in retail and five in airline), and its sure rules hold at 78–100%: there, what one feature misses is in the conversation, not in the results.
- **When the agent holds the phone, the procedure is the episode.** In τ²'s solo mode, replies fall to 5–6% of turns, and reads after a tool rise to 52–60% (41–49% bound, every argument fixed by an earlier result: the phone's checks take none). The fixes, the procedure's writes, are the other 29–35%.
- **A written workflow makes an agent's traces more compilable, if the agent follows it.** Solo, GPT-4.1's next steps under the workflow are predicted 79.7% of the time by the tree, and sure rules cover 46.5% of them at 97.9%; under the manual, 73.6% and 28.6%. It also passed 68% of episodes instead of 52%. o4-mini passed as often or more with either (67–72%) and followed neither: 55–57% predicted, 8–13% covered.

[Run as the agent](telecom-workflow-2026-09-26.md), a tree fitted on the four solo runs' successful training episodes passed 31 of the 40 held-out tasks with no model, as many as the agents did.

So a workflow compiled once from traces can run a procedure that branches on what the tools return, and runs furthest when the agent runs the tools itself and follows a written procedure. Where the procedure branches on what the customer says, as in retail and airline, it is the read skeleton and no more. stretto's flows compile only reads; in telecom's solo mode, where a third of the turns are fixes that sure rules cover, the next question is whether a fix a rule is 98% sure of can be made without asking, which is [the write guards'](guards-2026-09-24.md) question, not the flow's.

## Method

[`scripts/anatomy.py`](../../scripts/anatomy.py), as for [retail and airline](anatomy-2026-09-26.md), with three additions:

- **Telecom's reads** are the tools τ²-bench annotates as read-only: the agent's six (customer, line and bill lookups) and the phone's fifteen checks (status bar, network status, SIM, APN settings, speed test and the rest). In solo mode the agent calls both; in the default mode the customer calls the phone's.
- **Features of results in text.** The phone reports in lines such as `Airplane Mode: ON` or `Status Bar: ✈️ Airplane Mode | 🔋 80%`. Each line's value, or each of its `|`-separated parts, is a feature (`airplane mode=on`), and so is each short line of its own (`No SIM card detected in the phone.`); parts with digits (a battery level, a speed) are left out, as ids are from JSON results.
- **The whole state.** Decision mining in process mining fits a decision tree at each branch point on the case's attributes ([Rozinat and van der Aalst, 2006](https://doi.org/10.1007/11841760_33)). Here each site (the tool that just returned) gets a tree over the features of every tool's last result so far, and which tools the agent has called: at each split, the feature with the most information gain among those that at least five cases have and five lack. Each site's depth, up to six, is chosen by two-fold cross-validation over the training half's tasks, as the single feature is. It is fitted on half the tasks and scored on the other half, both ways; a rule is sure when its leaf held in at least 95% of at least ten training cases.

Some trajectories (Gemini 3 Flash's telecom run) give tool results no ids; the script then pairs results with calls in order. The retail and airline rows of the other predictions are unchanged.

## Telecom: the nine leaderboard agents

The customer holds the phone (τ²-bench's default), four trials of each of 114 tasks.

| Agent | Episodes | Passed | LLM turns | Replies | After the customer: reads · writes | After a tool: bound · every record · picked · customer or made up | Writes after a tool |
|---|---|---|---|---|---|---|---|
| Claude Opus 4.5 (high) | 456 | 92% | 5,948 | 62% | 11% · 5% | 3.5% · 3.1% · 11.6% · 0.0% | 3.6% |
| Claude Sonnet 4.5 | 456 | 85% | 6,377 | 62% | 12% · 5% | 3.4% · 0.0% · 14.0% · 0.0% | 3.8% |
| GLM-5 | 456 | 87% | 4,636 | 61% | 10% · 5% | 10.4% · 8.7% · 0.1% · 0.2% | 5.1% |
| GPT-5.2 (high) | 456 | 90% | 5,021 | 56% | 16% · 5% | 5.0% · 8.2% · 0.2% · 2.8% | 6.3% |
| GPT-5.2 (none) | 456 | 62% | 5,098 | 59% | 13% · 6% | 2.3% · 8.4% · 3.2% · 2.5% | 5.8% |
| Qwen3.5 | 456 | 98% | 6,951 | 60% | 9% · 4% | 5.2% · 7.3% · 8.8% · 1.1% | 4.2% |
| Gemini 3 Flash | 456 | 91% | 7,534 | 41% | 14% · 8% | 21.0% · 4.6% · 5.5% · 0.5% | 4.4% |
| Gemini 3 Pro | 456 | 91% | 5,621 | 55% | 12% · 6% | 7.6% · 1.1% · 13.4% · 0.2% | 5.2% |
| Qwen3-Max | 456 | 96% | 6,390 | 59% | 8% · 5% | 8.8% · 0.0% · 14.2% · 1.3% | 3.9% |

| Agent | Decisions | Sequence | + one feature | Whole state (tree) | + goal | Sure rules (tree): share · accuracy |
|---|---|---|---|---|---|---|
| Claude Opus 4.5 (high) | 2,265 | 73.1% | 81.7% | 84.2% | 86.8% | 48.2% · 99.1% |
| Claude Sonnet 4.5 | 2,445 | 69.4% | 78.5% | 83.8% | 83.1% | 29.3% · 98.0% |
| GLM-5 | 1,831 | 72.8% | 77.2% | 79.7% | 88.5% | 45.1% · 99.8% |
| GPT-5.2 (high) | 2,216 | 66.4% | 70.8% | 75.8% | 79.5% | 20.2% · 97.1% |
| GPT-5.2 (none) | 2,098 | 68.9% | 73.0% | 75.9% | 80.9% | 24.6% · 98.8% |
| Qwen3.5 | 2,760 | 66.4% | 69.6% | 78.8% | 81.7% | 32.1% · 99.2% |
| Gemini 3 Flash | 4,412 | 57.8% | 57.8% | 62.7% | 62.3% | 19.0% · 95.8% |
| Gemini 3 Pro | 2,528 | 65.1% | 72.9% | 77.5% | 77.7% | 13.3% · 97.9% |
| Qwen3-Max | 2,629 | 73.0% | 79.1% | 83.9% | 89.1% | 62.0% · 99.0% |

## Retail and airline: the whole state adds nothing

| Agent | Retail: one feature · tree · sure rules (tree) | Airline: one feature · tree · sure rules (tree) |
|---|---|---|
| Claude Opus 4.5 (high) | 83.4% · 84.6% · 36.2% · 92.7% | 65.5% · 68.4% · 15.7% · 92.3% |
| Claude Sonnet 4.5 | 69.3% · 66.5% · 4.1% · 77.8% | 58.5% · 56.3% · 6.2% · 89.7% |
| GLM-5 | 81.6% · 83.7% · 41.0% · 93.2% | 69.4% · 73.5% · 19.4% · 85.7% |
| GPT-5.2 (high) | 70.7% · 72.5% · 2.6% · 100.0% | 57.7% · 57.8% · 12.1% · 97.2% |
| GPT-5.2 (none) | 71.2% · 67.0% · 2.8% · 96.9% | 55.3% · 63.6% · 12.4% · 97.5% |
| Qwen3.5 | 74.6% · 70.8% · 9.4% · 87.4% | 66.0% · 61.6% · 10.2% · 90.1% |
| Gemini 3 Flash | 74.1% · 69.2% · 12.0% · 97.2% | 56.6% · 52.5% · 11.9% · 92.8% |
| Gemini 3 Pro | 73.9% · 69.9% · 15.9% · 92.4% | 57.0% · 54.9% · 14.0% · 85.8% |
| Qwen3-Max | 79.2% · 73.3% · 12.9% · 92.2% | 69.9% · 62.7% · 6.4% · 95.7% |

## Who holds the phone, and a written workflow

τ²-bench's own 2025 runs, four trials of each of 114 tasks: the policy as the manual or as the workflow, and the phone the customer's (default) or the agent's (solo).

| Agent | Policy | Phone | Passed | Replies | Reads after a tool (bound) | Writes | Decisions | Sequence | + one feature | Whole state (tree) | Sure rules (tree) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| GPT-4.1 | manual | the customer's | 34% | 62% | 18% (11%) | 8% | 2,994 | 69.9% | 72.4% | 77.3% | 22.9% · 99.6% |
| GPT-4.1 | manual | the agent's | 52% | 6% | 55% (43%) | 32% | 6,959 | 56.0% | 63.9% | 73.6% | 28.6% · 97.1% |
| GPT-4.1 | workflow | the customer's | 52% | 64% | 17% (11%) | 8% | 2,780 | 71.2% | 72.8% | 79.6% | 23.4% · 99.1% |
| GPT-4.1 | workflow | the agent's | 68% | 5% | 60% (49%) | 29% | 7,994 | 55.5% | 65.7% | 79.7% | 46.5% · 97.9% |
| o4-mini | manual | the customer's | 42% | 50% | 22% (11%) | 14% | 4,052 | 55.0% | 58.3% | 60.5% | 9.5% · 98.2% |
| o4-mini | manual | the agent's | 67% | 6% | 52% (41%) | 35% | 6,800 | 41.5% | 48.9% | 56.7% | 12.9% · 95.1% |
| o4-mini | workflow | the customer's | 59% | 67% | 12% (2%) | 9% | 1,963 | 61.3% | 72.2% | 75.8% | 24.0% · 100.0% |
| o4-mini | workflow | the agent's | 72% | 6% | 57% (44%) | 31% | 7,043 | 40.8% | 49.4% | 55.4% | 7.7% · 90.4% |

The runs where the agent is also handed the task's plan (τ²-bench's `op` modes) are in the JSON; there the agent relays or executes a given plan, and nothing is left to compile.

## Caveats

- The same caveats as [the anatomy's](anatomy-2026-09-26.md#caveats): copies matched verbatim, the goal known in hindsight, and calling styles that move the shares.
- A tree fitted on half the tasks meets new combinations on the other half; its depth is chosen to hold up on held-out tasks, but the sure rules are those of this fit, not a proof about the procedure.
- The workflow comparison is two agents in τ²-bench's 2025 harness; the leaderboard agents ran telecom with the manual and the customer holding the phone only.

## Reproduce

No keys. With the leaderboard's telecom trajectories fetched and τ²-bench's own runs in its checkout:

```sh
python3 scripts/anatomy.py $(scripts/fetch-leaderboard.sh -t all | cut -d= -f2) \
  ../tau2-bench/data/tau2/results/final/*telecom*.json --json telecom-anatomy.json
```

The numbers are in [telecom-anatomy-2026-09-26.json](telecom-anatomy-2026-09-26.json).
