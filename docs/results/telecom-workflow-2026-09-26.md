# A workflow compiled once, run with no model: telecom

[The telecom anatomy](telecom-anatomy-2026-09-26.md) found that, where the agent operates the phone itself, a decision tree over the tool results predicts most of its steps. This page runs that tree as the agent. It is fitted once on the successful training episodes of τ²-bench's own solo runs, then run with no model on the 40 held-out test tasks in τ²-bench's environment, and scored by τ²-bench's evaluator.

It passed 31 of the 40 held-out tasks (77.5%), and 25–31 (mean 71%) when refitted on ten resamples of its training episodes. The LLM agents whose traces it was fitted on passed 49–78% of the same tasks, and the tree made no LLM call; each of them made 15–18 per episode. Its own check of the ticket's stated outcome caught 8 of its 9 failures, and handing only those 8 to an agent projects 87.5–89.4%, with a model in one episode in five. The same check lets it learn from its own tries: fitted on a quarter or half of the demonstrations, then trying each training ticket 17 times and keeping the shortest run the check verified, it went from 18 to 22–23 of 40. Learned as where they came from, its identifiers bind for a customer no trace saw: renamed throughout, it passed the same 35 of 40 as for the original customer, where constants pass 16.

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
| All four, refitted on ten resamples of their episodes (with replacement) | 25–31, mean 28.4 |
| All four, without the repeat guard | 22 |
| All four, and the four runs where the agent was handed the task's plan (τ²-bench's `op`) | 31 |
| GPT-4.1, workflow policy, alone | 26 |
| GPT-4.1, manual policy, alone | 20 |
| o4-mini, workflow policy, alone | 16 |
| o4-mini, manual policy, alone | 17 |
| All four, 50% of the training tasks (37) | 18 |
| All four, 25% (18 tasks) | 18 |
| All four, 10% (7 tasks) | 12 |
| All four, ten trees each fitted on a resample of the episodes, their leaves' shares averaged (bagging) | 31 |
| All four, 25%, bagged the same way | 14 |

It needs many demonstrations, and consistent ones, and averaging trees fitted on resamples does not stand in for them. o4-mini passes 78% itself, but a workflow fitted on its traces alone passes 40–43%: it takes varied routes to the same fixes (the anatomy predicts 55–57% of its steps), and a tree fitted on them picks the commonest step at each point rather than any one coherent route. GPT-4.1 under the written workflow is the most consistent demonstrator of the four, and the four together cover more fault combinations than any one.

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

It knew when it had failed: every run its check called unresolved had failed, and 31 of the 32 it called done had passed. Over the ten refits' 400 runs, the check called 300 done, of which 16 had failed (5%), and 100 unresolved, every one of which had failed. So the gate to hand back on is the outcome, not the tree's confidence. Hand the 8 unresolved episodes to an agent, which passes each as often as its four trials of that task did (assuming it does as well from where the workflow stopped as from the start), and the pair passes 87.5–89.4% (86–88% on average over the refits, 79–96% at the extremes), with a model in 8 episodes of 40: above any of the four agents alone (49–78%), because the workflow passes MMS tasks the agents often fail, and the agents pass the mobile-data tasks it does not.

| Workflow, then this agent where its check says unresolved | Projected pass rate | The agent alone |
|---|---|---|
| GPT-4.1, workflow policy | 87.5% | 78.1% |
| GPT-4.1, manual policy | 88.7% | 49.4% |
| o4-mini, workflow policy | 89.4% | 77.5% |
| o4-mini, manual policy | 89.4% | 78.1% |

## Learning from its own runs

The ticket's check is a verifier, so the workflow can also learn from itself: try, keep what the verifier accepts, refit. That is expert iteration ([Anthony et al., 2017](https://arxiv.org/abs/1705.08439)), and it is how STaR ([Zelikman et al., 2022](https://arxiv.org/abs/2203.14465)) and rejection-sampling fine-tuning ([Yuan et al., 2023](https://arxiv.org/abs/2308.01825)) improve language models on their own filtered samples; Agent Workflow Memory ([Wang et al., 2024](https://arxiv.org/abs/2409.07429)) induces an agent's workflows from its own past runs, and Agent Skill Induction ([Wang et al., 2025](https://arxiv.org/abs/2504.06821)) and SkillWeaver ([Zheng et al., 2025](https://arxiv.org/abs/2504.07079)) induce them as programs, verified by running them, for the agent to call. Here nothing is a model: the learner is the tree, the tries are drawn from its own leaves, and the verifier is the ticket's criterion.

Each round (`--self-train`), the workflow ran on each of the 74 training tickets once as it stood and 16 times drawing each call from its leaf's calls by their counts. It kept, per ticket, the shortest run its check said resolved, and never a transfer, which the check cannot verify and a search would learn to reach for. It was then refitted on the demonstrations and those runs, and scored on the 40 test tasks. For the tickets outside the share it had no demonstration, only the ticket and the phone's starting state.

| Fitted on | Demonstrations alone | Then its own runs: round 1 | Round 2 | Round 3 | Round 4 |
|---|---|---|---|---|---|
| 25% of the training tasks (18), three seeds | 18 | 21 (19–24) | 21.7 (20–23) | 22 (19–26) | **22.3** (21–23) |
| 50% (37), three seeds | 18 | 20.7 (19–23) | 22.7 (21–25) | 24.3 (23–26) | **22.7** (22–23) |
| 10% (7) | 12 | 13 | 13 | 14 | 13 |
| All (74), two seeds | 31 | 30.5 (30–31) | 31 (27–35) | 28.5 (26–31) | 32 (31–33) |
| 25%, keeping its runs as they stand, with no draws | 18 | 19 | 17 | 17 | 17 |
| 50%, the same | 18 | 18 | 18 | 18 | 18 |
| 25%, keeping the first run that passes, not the shortest | 18 | 17 | 18 | 17 | 20 |
| 25%, keeping runs τ²-bench's evaluator passes (an oracle) | 18 | 16 | 22 | 24 | 21 |

Test tasks passed, of 40; one run per seed, the mean with its range.

- **From a quarter or half of the demonstrations, its own tries take it from 18 to 22–23 of 40,** part of the way to the 31 that all of them give (25–31 on resamples, mean 28.4). Tried on 74 tickets, it kept runs on 63–67 of them by the fourth round. Handing the runs its check calls unresolved to an agent then projects 79–87%, against 87.5–89.4% for the workflow fitted on everything.
- **It needs somewhere to try.** Kept as they stand, its runs teach it nothing: those are the runs it already makes. A deployment's own sessions are exactly that, one run per ticket, so the gain needs the draws: a simulator, a sandbox or a staging copy where a ticket can be tried again.
- **The shortest run it can verify is the one to keep.** Keeping the first that passes, which is its own run where that passes, gained 2 at most. The shortest is the most direct route to a fix, and one route per ticket.
- **Its check served as well as the evaluator.** Kept on the evaluator's reward instead, the runs taught it no more (21 in the fourth round, one seed). Of the up to 67 runs the check kept each round, 0–4 failed the evaluator, and over every configuration's last round, no run the check called unresolved had passed.
- **It cannot learn what no demonstrator did.** The draws come from its own leaves. Fitted on 7 tasks, its leaves hold 44 of the 62 calls the demonstrators made on all 74; it kept runs on 34 tickets and passed 13–14. With every demonstration, its own runs added nothing: 26–35 across the rounds, a mean of 30.5 against 31, about as far as a refit on resampled demonstrations moves it (25–31).

## A customer no trace saw

Every base task has one customer, so the workflow above learned John Smith's ids and phone number as constants. To see whether a compiled workflow can bind them, `--rename` runs the test tasks for a customer no trace saw: his name, email, customer, line, bill and device ids and phone numbers renamed throughout τ²-bench's database, the phone's and each task (Maria Lopez, `C7001`, `L7002`, `555-987-6502`). `--symbolic` learns each identifier a call passes (four characters or more, with a digit) as where it came from: the path of the most recent result that held it, or the ticket, by its shape. When the workflow runs, it binds the identifier again from that run's results. That is the ticket's first match, or a field of the most recent record of that tool, preferring the record that holds a value the ticket gives (the line with the customer's number), or the next value of a list not yet passed. For a list of records, it also learns the fields that set the chosen record apart from the others, such as a bill's `status: Overdue`, and prefers a record that has them. And a tool's state follows the record the ticket names: another record of the same kind read after it, such as the customer's next line, does not replace its fields.

| Workflow | The base tasks | Renamed | Refitted on five resamples, renamed |
|---|---|---|---|
| Identifiers as constants | 31 | 16 | 13–16 |
| **Identifiers as where they came from** | **35** | **35** | 25–30, as on the base tasks |

Held-out test tasks passed, of 40 ([the runs](telecom-workflow-2026-09-26-binding.json)). As it was built, one rule at a time: with the list's next value alone it passed 24; preferring the record that holds the ticket's value, 29; with the fields that set a record apart, 32; with the state following the record the ticket names, 35.

- **Constants break on the account.** They still pass 16 renamed tasks, because most fixes are on the phone and take no id. Every fix on the account (a payment, roaming, a data refuel) names a customer that no longer exists.
- **Bound again, nothing changes.** For the renamed customer the workflow passed exactly the tasks it passed for the original, on every fit. The identifiers were all it had learned about him.
- **What binding takes is what the agents' choice depends on.** The agents act on the customer's line, the one carrying the ticket's number, pay the overdue bill, and judge the line by its own record, not by the last one they read. A person could state each rule; here they were learned from the traces, and each raised the pass rate.
- **Its check still separates.** All 4 runs it called unresolved had failed, and 35 of the 36 it called done or transferred had passed. Handing the unresolved to an agent projects 93.8–96.3%, above every agent alone (49–78%).
- **With half the demonstrations, and its own tries.** Fitted on half the training tasks with identifiers learned as where they came from, it passed 17 renamed tasks. Self-trained on its own verified runs of the original customer's tickets, as [above](#learning-from-its-own-runs), it passed 19–28 over the next four rounds (three seeds; mean 22.0), about what the same does with constants on the original tasks (19–26, mean 22.6), and for a customer no trace saw.

**Where it fails.** Of its 5 failures for John Smith, 3 are mobile-data tasks it stopped short on, though their fixes are well covered in training (the data saver was switched off 167 times in the successful training episodes), and 1 is an MMS task stacking six faults, where it wandered for 42 calls. The last is a suspension at a contract's end, a fault 2 training tasks show and 6 test tasks carry; there its check said resolved and the evaluator did not. Its check caught the other 4.

**Another customer's account.** Renaming tests binding, not an account in another shape. `--as-customer C1003` moves each test task to Michael Lee, another customer in τ²-bench's database: one line where John Smith has three, a phone without eSIM, a PayPal account that has expired and a disputed bill. His line takes John's line's state (active, roaming, data used) so that each task's faults still hold, and the tasks' own gold actions, made as the agent's calls, still solve all 40.

| Workflow | John Smith (three lines) | Michael Lee (one line) | Refitted on five resamples, Michael Lee |
|---|---|---|---|
| Identifiers as constants | 31 | 19 | 17–19 |
| **Identifiers as where they came from** | **35** | **35** | 24–30 |

Bound again, the workflow passed the same tasks for an account of another shape as for John's. Its check again caught every failure but one (4 unresolved, all failed), and handing those to an agent projects 93.8–96.3%. The line took John's state, so the faults are the same; an account in another state (a suspended line, an expired card the fix needs) would ask the workflow for steps no trace shows.

Moved to Sarah Johnson (`--as-customer C1002`: five lines on an unlimited plan, with an overdue bill already), the gold actions solve 28 of the 40 tasks; the others cannot be set up, since her account cannot take a second overdue bill. Only 16 of the 28 keep their faults. τ²-bench derives a line's flags from the account (active, roaming allowed, data used up), and on her unlimited plan (999 GB) the usage copied from John's line uses up nothing: in 12 tasks the data-usage fault is not there, and the gold actions, written for John's account, make a refuel the ticket no longer needs. `--as-customer` now keeps a task only if those flags match John's. On the 16, the workflow passed 14 with identifiers as where they came from and 11 with constants, against 14 and 12 for John on the same tasks, and its check called both of its failures unresolved (handing those to an agent projects 98.4–100%). On all 28 it had passed 22 against John's 24; the two it lost were such data-usage tasks, where it made no refuel and its check called them resolved.

## What it says

Compiling once works when three things hold: the procedure branches on what the tools return, the agent runs the tools itself, and the arguments come from a closed set or from earlier results. In τ²-bench telecom's solo mode all three hold. A decision tree fitted on successful traces then does the whole job as well as the agents that made the traces, with no model. In retail and airline, and in telecom with the customer holding the phone, the procedure branches on what the customer says, and what a compiled workflow can take is [the read skeleton](anatomy-2026-09-26.md), which stretto's flows already take.

**Together, projected.** With GPT-4.1 under the manual policy as the agent, a projection stacks the pieces: the symbolic workflow runs every ticket, and GPT-4.1, with the read-only flow behind its tools, takes the 4 its check calls unresolved. The agent alone takes 14.9 LLM turns per episode and passes 49%; with the flow, 9.9 ([the telecom flows](telecom-flows-2026-09-26.md#when-the-agent-holds-the-phone)). With the workflow first, it takes 1.2 turns per episode and projects 93.8%. The projection assumes the agent does as well, and the flow saves as much, from where the workflow stopped as from the start.

For stretto, this is the case its flows stop short of. They compile reads only. A compiled procedure that also makes the fixes needs [the write guards](guards-2026-09-24.md) or a confirmation, and hands back on its outcome check, which here caught all but one of its failures. It is a different product from a read-ahead flow: a procedure that runs to completion, with a model only where its own check says it did not.

The closest prior work: decision mining fits a tree at each of a process's branch points ([Rozinat and van der Aalst, 2006](https://doi.org/10.1007/11841760_33)); VIPER distils a policy into a decision tree ([Bastani et al., 2018](https://arxiv.org/abs/1805.08328)); and the loops the guard stops are behaviour cloning's compounding errors ([Ross et al., 2011](https://arxiv.org/abs/1011.0686)), which DAgger fixes with on-policy corrections, where this uses a rule instead. The workflow, then an agent where its check fails, is a cascade: FrugalGPT queries cheaper models first and escalates when a learned scorer rejects the answer ([Chen et al., 2023](https://arxiv.org/abs/2305.05176)), and AutoMix escalates when a small model's self-verification does ([Madaan et al., 2023](https://arxiv.org/abs/2310.12963)). Here the first tier is no model at all, and the scorer is the task's own outcome check, which needs no training.

## Caveats

- **One customer.** All 114 base tasks share one customer and one line, so every argument of every call is a constant of the task set, and the workflow's steps are whole calls. A deployment with many customers needs the arguments bound from the ticket and earlier results, as stretto's flows bind them; [renamed](#a-customer-no-trace-saw), the workflow bound them and passed the same tasks.
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

`--no-guard`, `--sure [SHARE]`, `--train-share`, `--bootstrap SEED` and `--bag N` give the other rows, and `--show FILE` writes the rules. `--self-train 4 --rollouts 16` (with `--seed`, `--keep first` or `--verifier evaluator`) gives the self-training rows, whose rounds are in [telecom-workflow-2026-09-26-self.json](telecom-workflow-2026-09-26-self.json); four rounds take about ten minutes. `--symbolic` and `--rename` give the renamed customer's rows ([the runs](telecom-workflow-2026-09-26-binding.json)). The runs, call by call, are in [telecom-workflow-2026-09-26.json](telecom-workflow-2026-09-26.json). Scored this way, GPT-4.1's recorded test episodes (first trial) get the rewards τ²-bench recorded for all 40 of them.
