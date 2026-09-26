# stretto's flow in telecom, and sources in the order the agent used them

Until today, stretto's read-only flow had only been projected for telecom ([Phase 0](telecom-2026-09-25.md)), never replayed. `pilot/check_flow.py` now replays telecom too, re-running the agent's calls against the flow and recording the customer's calls on their own phone as the customer's turns. The first replay went badly. A flow learned habit-only from τ²-bench's 2025 telecom runs saved 1.9% of the nine leaderboard agents' LLM turns and made 2,936 detours, one in 1,393 of the 1,440 episodes. Nearly all were right lookups with the wrong record. One change to how a binding orders its sources took the same flow to 12.8% of turns saved with 443 detours.

## What went wrong

Telecom's lookups share one tool for every kind of record: `get_details_by_id` reads a line (`L1002`), a plan (`P1002`) or a device alike. After the flow had read the customer's first line, its binding took the first value at any of the argument's sources, most recent output first. That was the plan id of the line it had just read. The agents mostly go on through the customer's lines first, then read the plan of the line whose number the customer gave. On GLM-5's episodes 142 of the flow's 143 `get_details_by_id` lookups were detours.

## The change: a site's sources in the order the agent used them

`learn` and `compile` now count, for each lookup argument with more than one source, where the agent took its values at each site: the tool whose result came last before the call (`bindings.site_sources`, [formats](../formats.md)). Where a site has at least five such values, the binding tries its sources in that order, the most recent output first within each. Elsewhere it keeps recency alone.

One detail mattered for agents that batch. GLM-5 reads all of a customer's lines in one turn, so each of those reads follows the customer lookup. A flow makes a batch's reads one after another, so after the first line it is at the site of a line read. `learn` therefore counts each read of a batch at the site of the read before it, as the flow will be when it makes the next. Without that, GLM-5's own flow did not move (304 detours); with it, it made 162 at the same turns saved.

## Telecom: the nine leaderboard agents

The flow learned habit-only from τ²-bench's four 2025 telecom runs (Claude 3.7 Sonnet, GPT-4.1, GPT-4.1 mini, o4-mini), served at 0.3, replayed on each agent's 160 test-task episodes (40 tasks, four trials). *Before* is the same flow without `site_sources`, which recency alone then binds.

| Agent | LLM turns | Before: turns saved · detours · episodes with one | After | 
|---|---|---|---|
| Claude Opus 4.5 (high) | 1,962 | 29 (1.5%) · 320 · 158 | 291 (14.8%) · 22 · 20 |
| Claude Sonnet 4.5 | 2,083 | 26 (1.2%) · 318 · 159 | 344 (16.5%) · 15 · 14 |
| GLM-5 | 1,573 | 56 (3.6%) · 289 · 143 | 67 (4.3%) · 50 · 50 |
| GPT-5.2 (high) | 1,675 | 8 (0.5%) · 360 · 156 | 50 (3.0%) · 62 · 58 |
| GPT-5.2 (none) | 1,784 | 16 (0.9%) · 332 · 145 | 164 (9.2%) · 73 · 59 |
| Qwen3.5 | 2,263 | 22 (1.0%) · 355 · 160 | 347 (15.3%) · 38 · 38 |
| Gemini 3 Flash | 2,422 | 121 (5.0%) · 316 · 160 | 291 (12.0%) · 40 · 40 |
| Gemini 3 Pro | 1,789 | 34 (1.9%) · 302 · 152 | 316 (17.7%) · 62 · 54 |
| Qwen3-Max | 2,111 | 16 (0.8%) · 344 · 160 | 390 (18.5%) · 81 · 81 |
| **All** | **17,662** | **328 (1.9%) · 2,936 · 1,393** | **2,260 (12.8%) · 443 · 414** |

Every agent gained. The spread afterwards is the agents' own habits: GLM-5, for one, reads a customer's lines in parallel, where the 2025 agents read them one at a time. D0's arbiter for telecom, fitted on the same four runs with Jev's answers, did no better than the habit alone on GLM-5: 59 turns saved and 46 detours, against 67 and 50. It asked Jev 910 questions no cache held, about $0.08 ([the answers](answers-2026-09-26-telecom-flows.md)). A flow learned from GLM-5's own telecom runs saved 108 turns with 162 detours. Most of those detours read data usage the customer's problem did not need, a choice that follows from what the customer said.

## What is left in dual control

Replayed with each call tagged, the 443 detours left on the nine agents are tool choices, not bindings, and the bills, the largest share, are mostly reads the agents make too, with another argument:

| Lookup, in tickets about | Detours | Used |
|---|---|---|
| `get_bills_for_customer`, no service | 160 | 155 |
| `get_data_usage`, MMS | 136 | 54 |
| `get_data_usage`, mobile data | 78 | 105 |
| `get_details_by_id`, all | 55 | 2,785 |
| Other | 14 | |

- **A word in the customer's messages does not choose them.** A split on one word or pair, learned per site from the 2025 runs, lands on words such as "it" and "please go" at most sites. The one real split, "no service", holds data-usage reads to 1% of those tickets, and the flow already makes almost none there. MMS tickets are where the data-usage detours are, and the 2025 agents read data usage in 53% of theirs (59% of mobile-data tickets), since MMS needs mobile data. How often one agent does so is that agent's habit: in all but one of the 227 data-usage detours, the agent read no data usage at all in that episode, so they are not reads of the wrong line.
- **The bills come right after a suspended line, and four agents ask for them with a limit.** In no-service tickets the 2025 agents read the bills in 86 of 112 episodes where a line they read was suspended, and in 2 of 49 where none was, and the habit holds that: after a line read whose combined feature (status, plan, roaming, contract end) has the most common suspended combination, it predicts the bills at 71% (58 of 82 training steps), after the other two at 18–19% (15 of 82), and after an active line at 1% or less. So the status is not lost in the combination. Backing off from the combined feature to the status alone before dropping it, as factored language models back off one factor at a time ([Bilmes & Kirchhoff 2003](https://aclanthology.org/N03-2002/)), replays exactly as before on all nine agents, and a flow learned with the status as its only feature (feature selection capped at one field) saved 51 more turns (13.1%) but made 810 detours, not 443, having lost the data-usage and roaming fields its other lookups decide by. What the tags show instead is that 149 of the 160 detours are GPT-5.2's (at both efforts) and the two Qwen agents'. When the ticket's own line is suspended, they read the bills in 52–95% of episodes, as the flow does, but always with a `limit` (from 5 to 12, except 3 in 12 of Qwen3-Max's 80 calls), where the flow passes the customer alone. The account has three or four bills, so a limit of four or more returns what the flow's lookup returned. `check_flow.py --same-result` counts a recorded call as made when a flow lookup of the same tool, agreeing on every argument both pass, has already returned its recorded result: only an optional argument may differ. With it, the nine agents' replays save 2,350 turns (13.3%) with 298 detours, 15 of them bills, against 2,260 and 443. Whether an agent with the flow's bills in front of it would still ask with its own limit, only a live run can tell.

## When the agent holds the phone

In τ²-bench's solo mode the agent calls the phone's tools itself, and reads after a tool are 52–60% of its turns ([the anatomy](telecom-anatomy-2026-09-26.md)). `learn --results` now takes such runs: when the agent's calls to the customer's tools returned, they join the flow's tools, read-only or not as τ²-bench marks them. (A call that did not return does not count: when the customer holds the phone, an agent that calls its tools is told they do not exist, as Claude 3.7 Sonnet was 130 times in the 2025 runs.) `check_flow.py --solo` and `tau2_mcp.py --solo` serve them. A flow learned habit-only from GPT-4.1's and o4-mini's solo training episodes (the manual policy), served at 0.3 and replayed on their 160 test-task episodes each:

| Agent | LLM turns | Turns saved | Detours · episodes with one |
|---|---|---|---|
| GPT-4.1 | 2,386 | 799 (33.5%) | 203 · 121 |
| o4-mini | 2,354 | 647 (27.5%) | 231 · 117 |

A third of the turns, the most any flow has saved in replay, against 12.8% when the customer holds the phone. The fixes stay with the agent: this flow only reads.

**The line the ticket names.** As first learned here, the flow saved 776 and 633 turns with 387 and 430 detours. The largest share, 160 and 197, was another of the customer's lines, read after the agent had found the one with the ticket's number and stopped. That is the stop [the detours page](detours-2026-09-26.md#what-is-left-stopping-once-the-record-is-found) found missing in airline, with an exact value in place of a description: the phone number the agent passed to `get_customer_by_phone`, which no result had shown it, is on one line record and not on the other. The binding now counts, per lookup, the picks where the record the customer described had been read (another value of the same list returned a value they gave, and another's did not), and how many of them the agent went on to pass (`bindings.described_read`, format 2). Here that was 7 of 297, so the flow hands back there. Detours fell to 203 and 231, and turns saved rose to 799 and 647: the lookups it no longer makes had led it on to others the agents did not make either, such as the bills of the customer it had just looked up again (100 and 127 detours, now 1 and 17). Most of what is left is an extra check of the phone (143 and 127). Scored with `--same-result` ([above](#what-is-left-in-dual-control)), GPT-4.1's replay is as it was, and o4-mini's saves 663 turns with 215 detours: 16 of its reads of the bills pass a `limit` that returns the same bills.

A value the customer gave has a digit and at least four characters, and either the customer wrote it or the agent passed it before any result held it, which is how a ticket reaches an agent working alone. All nine leaderboard agents replay retail and airline exactly as before with it, and so does a flow learned from Claude Sonnet 4.5's own episodes, the agent that stops at the record the customer meant: there it counted 11 of 17 such order picks and 0 of 5 reservation picks, too few or too high to move a lookup across the bar. In airline it rarely applies, since customers name cities, not a reservation's digits, and that stop is still language. D0's telecom flow, where the customer holds the phone, learns no such count, and needs none: there the customer writes their number, the matching line's record carries it, so the other lines are records the customer did not name, and the named-other count covers them: of 366 such `get_details_by_id` picks in the 2025 runs, the agents made 2. Working alone, the agent is handed the number in the ticket, which the flow never sees: the only trace of it is the argument the agent passes to `get_customer_by_phone`. [The compiled workflow](telecom-workflow-2026-09-26.md) makes the fixes too, with no model, where a check of the ticket's outcome says when to hand back.

## Retail and airline: unchanged

D0, compiled again with this build from the same traces and cached answers, replays exactly as before at 0.3, habit alone, on all nine leaderboard agents' retail and airline test episodes: the same turns saved, detours and lookups in each of the 18 replays. For GLM-5 that is 55 turns saved and 6 detours in airline and 307 and 33 in retail, and for Claude Sonnet 4.5 130 and 63, and 412 and 82. Retail's bindings agree with the agents exactly as often as before. Airline's flight search binds better (22 of 112 unmentioned picks agreed, from 12), with no change in the replays.

Scored with `--same-result`, all 18 retail and airline replays are as they were, with no pair: none of those agents' calls differs from a flow lookup only in an optional argument. A first version that matched on the result alone moved seven of the airline replays, by pairing calls whose required arguments differed but whose results were equal; hence the rule that the arguments both pass agree.

## Reproduce

```sh
R=../tau2-bench/data/tau2/results/final
stretto learn --results $R/claude-3-7-sonnet-20250219_telecom_default_gpt-4.1-2025-04-14_4trials.json \
  --results $R/gpt-4.1-2025-04-14_telecom_default_gpt-4.1-2025-04-14_4trials.json \
  --results $R/gpt-4.1-mini-2025-04-14_telecom_base_gpt-4.1-2025-04-14_4trials.json \
  --results $R/o4-mini-2025-04-16_telecom_default_gpt-4.1-2025-04-14_4trials.json \
  --tau2 ../tau2-bench --domain telecom --habit-only --out telecom.flow.json
for f in $(scripts/fetch-leaderboard.sh -t all | grep telecom | cut -d= -f2); do
  python3 pilot/check_flow.py --domain telecom --trials 0 1 2 3 --results $f --oracle-cache .oracle-cache \
    --flow-decider habit --flow telecom.flow.json --in-process --jobs 4 --tau2 ../tau2-bench
done
```

*Before* is the same flow with `bindings.site_sources` removed. The breakdown of what is left replays with `pilot/check_flow_tagged.py`, which writes each episode's calls by author (`tags.json`), and the same-result counts add `--same-result`. The solo flow comes from the two `telecom_no-user` runs, replayed with `--solo`; its first numbers are the same flow without `bindings.described_read`.
