# The compiled procedure on 2,171 tickets no trace showed

[The telecom procedure](telecom-workflow-2026-09-26.md) is compiled once from four solo runs' successful training episodes and runs with no model. On the 40 held-out base tasks it passed 35, and with GLM-5.3 on its four hand-backs the cascade passed 39 ([the reach round](reach-2026-09-26.md#the-procedure-in-stretto-and-the-cascade)). Those 40 are held out from 114 base tasks that share one customer and one fault vocabulary, and [#38](https://github.com/alexnodeland/stretto/issues/38) asked for a second solo workload.

τ²-bench offers its solo mode, where the agent operates the phone and no user speaks, only in telecom, plus a ten-task mock domain. Of the other benchmarks replayed so far, those without a simulated user need a model to read the request, and most often one to write ([the anatomy](benchmarks-2026-09-27.md)). So the second workload here is telecom's own full task set. It has 2,285 tickets, of which the base set is 114. The other 2,171 combine the same faults in combinations no base task has, one to nine at a time: 1,935 MMS tickets, 218 mobile-data tickets and 18 no-service tickets. The procedure, compiled exactly as before, ran on every one of them. It ran for the customer the traces saw, and for a customer renamed throughout τ²-bench's database and tasks. Its hand-backs went to Claude Haiku 4.5.

## Findings

- **With no model, it passed 1,831 of the 2,171 tickets (84.3%).** On the base set's 40 held-out tasks it passed 35 (87.5%). The number of faults does not bring it down: it passed 60 of the 65 tickets with eight faults, and 14 of the 20 with one.
- **Its identifiers bound for a customer no trace saw.** Renamed throughout, it passed the same 1,831 tickets, run for run. With identifiers held as constants it passed 363.
- **Its outcome check held.**
  - It judged 1,829 runs resolved, and 1,824 of them passed: it missed 5 failures, 0.3%. All five are no-service tickets whose line was suspended at the end of its contract. The procedure made the phone's fixes and judged the service restored.
  - It transferred 8 to a person as the policy directs, and 7 of those passed.
  - It handed back 334, all of them failures. So it caught 334 of its 340 failures.
- **stretto runs it.** On 100 of the tickets drawn at random, `stretto-procedure`, over MCP against τ²-bench's tools, made exactly the calls of the reference implementation, with the same rewards and verdicts.
- **Against the agent alone, live.**
  - Claude Haiku 4.5 alone (τ²-bench's solo protocol and manual policy) passed 8 of 20 tickets drawn at random (40%; 95% interval 22–61%), at 21.5 LLM turns a ticket.
  - On the same 20, the procedure passed 19 and handed one back, which Haiku, taking over, resolved in 5 LLM turns. So the cascade passed all 20, at 0.25 LLM turns a ticket.
  - Taking over the procedure's hand-backs, with its calls and results as context, Haiku 4.5 resolved 19 of 20 drawn at random from the 334, at 5.2 LLM turns each.
  - From scratch it resolved 12 of the same 20 (39–78%), at 19.5 turns each. Taking over, it resolved 7 more of these tickets than from scratch, in about a quarter of the turns.
- **Projected over all 2,171 tickets, the cascade passes 99.0%** (1,831 + 334 × 19/20), at 0.80 LLM turns a ticket. Haiku 4.5 alone passes 40% at 21.5.
- **What defeats it is a few faults, not how many.** Of the tickets with a bad VPN, it failed 66 of 112, and 49 of 109 with data saver on. Those two faults are in 11 and 12 of the 74 training tickets. Roaming abroad costs it about a quarter of its tickets.

## Setup

- **The procedure.** The same fit as [the workflow page](telecom-workflow-2026-09-26.md): a decision tree per site over every tool result so far and the ticket's words, with identifiers learned as where they came from (`--symbolic`) and the guard against repeats. It is fitted on 730 successful training episodes of four solo runs (GPT-4.1 and o4-mini, each under the manual and the workflow policy). The exported procedure is byte for byte [the published one](telecom-workflow-2026-09-26.procedure.json).
- **The tickets.** τ²-bench's `full` task split, less every combination of faults the `base` split has, the persona aside (solo mode has no user). That leaves 2,171 tickets, and none of them is a base task under another persona. Each is scored by τ²-bench's evaluator in solo mode: its environment assertions.
- **The renamed customer.** `--rename` renames John Smith's name, ids and phone numbers throughout τ²-bench's database, the phone and each ticket, as on the workflow page.
- **Live.** Claude Haiku 4.5 (`claude-haiku-4-5`) in Claude Code, through `pilot/claude-agent.sh`, on τ²-bench's solo protocol (`run_episode.py --solo`, telecom's manual policy), one episode per ticket. The hand-back episodes start from the procedure's run (`--handback`): its calls are made first, with their effects in place, and the agent is told what they returned and that the procedure's check failed.
  - Three sets ran: 20 tickets drawn at random from the 2,171 (seed 1), Haiku alone; 20 drawn at random from the 334 hand-backs, Haiku taking over; and the same 20 from scratch. The random sample's one hand-back also ran as a hand-back.
  - The 61 episodes cost $3.41 at list prices, on the Claude subscription. Its seven-day window went from 0.71 to 0.74.

## By issue

| | Tickets | The procedure passed | Handed back |
|---|---|---|---|
| MMS | 1,935 | 1,686 (87.1%) | 249 |
| Mobile data | 218 | 134 (61.5%) | 84 |
| No service | 18 | 11 | 1 |
| All | 2,171 | 1,831 (84.3%) | 334 |

## Limits

- **One domain's variant.** The second workload is telecom's own full task set: new combinations of the faults the base tasks already show, on the same tools and policy. τ²-bench offers no other solo domain, and a benchmark without a simulated user would need a model to read the request.
- **Faults no trace fixes.** The procedure's failures concentrate where a fault's fix is rare in the training traces. A deployment would hand those back, as here, until its own sessions teach the fix.
- **Samples of twenty, live.** Haiku's 8 of 20 alone and 19 of 20 on hand-backs are small samples, one episode each. The projection assumes that hand-backs outside the sample are resolved as often. The comparison with GLM-5.3's cascade on the base tasks is across models.

## Reproduce

No keys for the procedure. With τ²-bench's Python environment and its checkout (`R` is `tau2-bench/data/tau2/results/final`):

```sh
python3 scripts/telecom_workflow.py $R/gpt-4.1-2025-04-14_telecom-workflow_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/gpt-4.1-2025-04-14_telecom_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/o4-mini-2025-04-16_telecom-workflow_no-user_gpt-4.1-2025-04-14_4trials.json \
  $R/o4-mini-2025-04-16_telecom_no-user_gpt-4.1-2025-04-14_4trials.json \
  --tau2 ../tau2-bench --symbolic --unseen 0 --json unseen-symbolic.json   # --rename: the renamed customer
python pilot/run_procedure.py docs/results/telecom-workflow-2026-09-26.procedure.json --out procedure-unseen \
  --compare unseen-symbolic.json --unseen 100 --seed 1
python pilot/run_episode.py --domain telecom --solo --agent-cli claude --model claude-haiku-4-5 --task-id "$TICKET"
python pilot/run_episode.py --domain telecom --solo --agent-cli claude --model claude-haiku-4-5 --task-id "$TICKET" \
  --handback unseen-symbolic.json
```

`--unseen N` draws N of the 2,171 tickets with `--seed` (0: all of them). The live episodes are in [procedure-unseen-2026-09-29-episodes.tar.gz](procedure-unseen-2026-09-29-episodes.tar.gz), and every run of the procedure, call by call, in [procedure-unseen-2026-09-29.json.gz](procedure-unseen-2026-09-29.json.gz).
