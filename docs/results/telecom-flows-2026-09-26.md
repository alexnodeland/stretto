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

## Retail and airline: unchanged

D0, compiled again with this build from the same traces and cached answers, replays exactly as before at 0.3 on GLM-5's and Claude Sonnet 4.5's test episodes: 55 turns saved and 6 detours in airline and 307 and 33 in retail for GLM-5, and 130 and 63, 412 and 82 for Sonnet. Retail's bindings agree with the agents exactly as often as before. Airline's flight search binds better (22 of 112 unmentioned picks agreed, from 12), with no change in the replays.

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

*Before* is the same flow with `bindings.site_sources` removed.
