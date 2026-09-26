# A workflow compiled once, run with no model: telecom

[The telecom anatomy](telecom-anatomy-2026-09-26.md) found that, where the agent operates the phone itself, a decision tree over the tool results predicts most of its steps. This page runs that tree as the agent. It is fitted once on the successful training episodes of τ²-bench's own solo runs, then run with no model on the 40 held-out test tasks in τ²-bench's environment, and scored by τ²-bench's evaluator.

It passed 31 of the 40 held-out tasks (77.5%). The LLM agents whose traces it was fitted on passed 49–78% of the same tasks, and the tree made no LLM call; each of them made 15–18 per episode. Its own check of the ticket's stated outcome caught every one of its failures, so handing only those to an agent projects 87.5–89.4%, with a model in one episode in five.

## The workflow

- **Its steps are whole calls.** Each is a tool with its arguments, such as `grant_app_permission{"app_name": "messaging", "permission": "sms"}` or `run_speed_test`, or stopping. The telecom base tasks share one customer and one line, so every argument comes from a closed set and nothing needs binding (see the caveats).
- **Its state** is each tool's last result, as features (`airplane mode=on`, `says no sim card detected in the phone.`); which tools it has called; which writes it has made, with their arguments; and the ticket's words.
- **Its decisions** come from a decision tree per site (the tool that just returned, or the start), fitted as [the anatomy](telecom-anatomy-2026-09-26.md#method) fits it: information gain, each site's depth chosen by cross-validation over the training tasks. It learns from every call of every successful training episode of four runs: GPT-4.1 and o4-mini, each with the troubleshooting policy as the manual and as the workflow, 730 episodes of the 74 training tasks, 12,059 decisions and 62 distinct calls.
- **One guard.** The workflow takes its leaf's likeliest call that does not repeat a read made since the last write (it would return the same thing) or a write already made. Without it, the tree loops: the same lookup twenty times, or a fix made, undone and made again.
- **It runs** from each task's initial state, one call at a time, until the tree says stop or it has made 50 calls. The longest successful episode of GPT-4.1's workflow run made 38.

## Results

Held-out test tasks, passed by τ²-bench's evaluator: environment assertions, and the required actions where the task asks for them. The agents' rows are their published runs on the same tasks, averaged over four trials. The workflow is deterministic and ran once per task. None of the 40 test tasks' combinations of faults appears among the training tasks, whatever the persona.

| | Passed | MMS (16 tasks) | Mobile data (9) | No service (15) | LLM turns per episode | Calls per episode |
|---|---|---|---|---|---|---|
| **The compiled workflow** | **77.5%** (31 of 40) | 69% | 67% | 93% | **0** | 21.3 |
| GPT-4.1, workflow policy | 78.1% | 56% | 92% | 93% | 18.1 | 17.8 |
| GPT-4.1, manual policy | 49.4% | 45% | 83% | 33% | 14.9 | 15.5 |
| o4-mini, workflow policy | 77.5% | 55% | 94% | 92% | 15.1 | 14.1 |
| o4-mini, manual policy | 78.1% | 56% | 94% | 92% | 14.7 | 13.7 |

It did better than every agent on MMS, where a task can stack up to nine faults, and worse on mobile data.

**What it takes.**

| Workflow fitted on | Passed |
|---|---|
| All four runs (above) | 31 of 40 |
| All four, without the repeat guard | 22 |
| All four, and the four runs where the agent was handed the task's plan (τ²-bench's `op`) | 31 |
| GPT-4.1, workflow policy, alone | 26 |
| GPT-4.1, manual policy, alone | 20 |
| o4-mini, workflow policy, alone | 16 |
| o4-mini, manual policy, alone | 17 |
| All four, 50% of the training tasks (37) | 18 |
| All four, 25% (18 tasks) | 18 |
| All four, 10% (7 tasks) | 12 |

It needs many demonstrations, and consistent ones. o4-mini passes 78% itself, but a workflow fitted on its traces alone passes 40–43%: it takes varied routes to the same fixes (the anatomy predicts 55–57% of its steps), and a tree fitted on them picks the commonest step at each point rather than any one coherent route. GPT-4.1 under the written workflow is the most consistent demonstrator of the four, and the four together cover more fault combinations than any one.

**A sure-only workflow hands back at once.** Run so that it acts only where its leaf held one call in at least 95% of at least ten training cases, and hands back otherwise, it handed back at the first step of every episode: 690 of the 730 training episodes open with the customer lookup, 94.5%. With the bar at 70–90% it handed back within its first three calls, where the demonstrators take different routes to the same fixes. A tree's confidence here measures agreement among demonstrators, not whether a step is right, so it is not the gate to hand back on. A check on the outcome is: the ticket says when the issue is resolved (the speed test excellent, an MMS sent), and the workflow can run that check itself.

## What was compiled

[The workflow, as rules](telecom-workflow-2026-09-26-rules.md) (`--show`), is 376 rules at 39 sites. 40 of them are sure, one call in at least 95% of at least ten training cases, and they cover 17% of the training decisions; the rest take the likeliest of the demonstrators' choices at their leaf. τ²-bench's own workflow, drawn as three graphs, has about a hundred steps. What was compiled is a policy that works, readable rule by rule, not the procedure a person wrote: it asks which tools have been called nearly as often as what they returned (131 of its questions against 173; 28 ask about the ticket), because where the demonstrators are in their routine is what best predicts their next step.

## Knowing when it failed

Each ticket states when the customer will consider the issue resolved: an MMS sent, the speed test excellent, the status bar showing signal. Each is a check the workflow can run itself (`can_send_mms`, `run_speed_test`, `check_status_bar`), and a transfer to a human is the policy's own ending for what the agent may not fix, such as a locked SIM. After each run, the workflow's own verdict against the evaluator's:

| The workflow's own check | Passed | Failed |
|---|---|---|
| Resolved | 20 | 1 |
| Transferred to a human | 11 | 0 |
| Not resolved | 0 | 8 |

It knew when it had failed: every run its check called unresolved had failed, and 31 of the 32 it called done had passed. So the gate to hand back on is the outcome, not the tree's confidence. Hand the 8 unresolved episodes to an agent, which passes each as often as its four trials of that task did (assuming it does as well from where the workflow stopped as from the start), and the pair passes 87.5–89.4%, with a model in 8 episodes of 40: above any of the four agents alone (49–78%), because the workflow passes MMS tasks the agents often fail, and the agents pass the mobile-data tasks it does not.

| Workflow, then this agent where its check says unresolved | Projected pass rate | The agent alone |
|---|---|---|
| GPT-4.1, workflow policy | 87.5% | 78.1% |
| GPT-4.1, manual policy | 88.7% | 49.4% |
| o4-mini, workflow policy | 89.4% | 77.5% |
| o4-mini, manual policy | 89.4% | 78.1% |

## What it says

Compiling once works when three things hold: the procedure branches on what the tools return, the agent runs the tools itself, and the arguments come from a closed set or from earlier results. In τ²-bench telecom's solo mode all three hold. A decision tree fitted on successful traces then does the whole job as well as the agents that made the traces, with no model. In retail and airline, and in telecom with the customer holding the phone, the procedure branches on what the customer says, and what a compiled workflow can take is [the read skeleton](anatomy-2026-09-26.md), which stretto's flows already take.

For stretto, this is the case its flows stop short of. They compile reads only. A compiled procedure that also makes the fixes needs [the write guards](guards-2026-09-24.md) or a confirmation, and hands back on its outcome check, which here caught every failure. It is a different product from a read-ahead flow: a procedure that runs to completion, with a model only where its own check says it did not.

The closest prior work: decision mining fits a tree at each of a process's branch points ([Rozinat and van der Aalst, 2006](https://doi.org/10.1007/11841760_33)); VIPER distils a policy into a decision tree ([Bastani et al., 2018](https://arxiv.org/abs/1805.08328)); and the loops the guard stops are behaviour cloning's compounding errors ([Ross et al., 2011](https://arxiv.org/abs/1011.0686)), which DAgger fixes with on-policy corrections, where this uses a rule instead.

## Caveats

- **One customer.** All 114 base tasks share one customer and one line, so every argument of every call is a constant of the task set, and the workflow's steps are whole calls. A deployment with many customers needs the arguments bound from the ticket and earlier results, as stretto's flows bind them.
- **A known success signal.** The workflow learns only from successful episodes. A deployment knows which of its sessions succeeded only through its own signal.
- **Many demonstrations.** 730 successful episodes of 74 tasks; with 37 tasks it passed 18 of 40.
- **One run per task** against the agents' four trials, on 40 tasks: a difference of a task or two is noise.
- **Solo mode is τ²-bench's own variant**, with the customer's device tools given to the agent. The leaderboard's telecom runs keep the customer in the loop.

## Reproduce

No keys. With τ²-bench's Python environment and its checkout:

```sh
R=../tau2-bench/data/tau2/results/final
python3 scripts/telecom_workflow.py \
  $R/gpt-4.1-2025-04-14_telecom-workflow_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/gpt-4.1-2025-04-14_telecom_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/o4-mini-2025-04-16_telecom-workflow_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/o4-mini-2025-04-16_telecom_no-user_gpt-4.1-2025-04-14_4trials.json \
  --tau2 ../tau2-bench --json telecom-workflow.json
```

`--no-guard`, `--sure [SHARE]` and `--train-share` give the other rows, and `--show FILE` writes the rules. The runs, call by call, are in [telecom-workflow-2026-09-26.json](telecom-workflow-2026-09-26.json). Scored this way, GPT-4.1's recorded test episodes (first trial) get the rewards τ²-bench recorded for all 40 of them.
