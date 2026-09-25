# Shadow mode and per-site promotion

RFC-001 §3.7 automates a site only after shadow mode has shown, on real traffic, that the flow agrees with the agent there. Until now a flow acted after every call where a lookup cleared one global threshold, 0.3. This round builds the two missing pieces and replays them on recorded episodes.

- **`stretto-proxy --flow-shadow`.** The flow decides after each call and logs what it would look up, but makes no lookups, so the agent gets the server's results unchanged. The questions it asked are left in the proxy's answer cache.
- **`stretto promote`.** It scores each lookup the flow would make in recorded sessions, or in τ²-bench results, at the threshold it will be served with. A lookup is *used* when the agent makes it later in the session, and a *detour* when it never does, as the replays count them. It then writes the flow with a per-site allow-list (`promoted` in [the flow IR](../formats.md#promoted)). A site is promoted when three things hold:
  - at least 70% of the flow's lookups there were used;
  - the lower bound of a 90% interval on that share is at least 0.5 (Wilson);
  - the lookups came from at least three distinct tasks, or sessions.

  The promoted flow hands back after every other call. `stretto flow-show` shows each site's record, and `stretto flow-diff` flags a site newly promoted, or a promotion lifted, as needing review.

## Findings

- **With D0's arbiter, promotion halves the detours at a small cost in turns.** In retail, the flow kept 277 of its 294 saved turns (20.0% of turns against 21.2%). Its detours fell from 58 to 28, and the episodes with a detour from 27 to 17. In airline it kept all 41 saved turns, and its detours fell from 26 to 11, in 2 episodes instead of 8.
- **The sites it held back are where the flow's lookups were not the agent's.**
  - In airline, lookups after `get_flight_status`: 32 would-be lookups in one half, from one task, none of them the agent's.
  - In retail, lookups after `list_all_product_types`, and after `get_product_details` in one half, where 65% were the agent's with a lower bound of 0.45. Replayed, the flow's lookups after a product fell from 67 to 14, and after `list_all_product_types` from 7 to none.
- **Part of the cost is thin evidence.** In one half, the lookups after `find_user_id_by_email` were all the agent's own, but they came from a single task, under the bar of three. So the flow stopped reading the user's details after finding them there: its lookups after that call fell from 20 to 4. A deployment with more sessions would promote the site.
- **The habit alone gains little.** Its detours are mostly at the busy sites, which pass the bar: 58 to 51 in retail and 50 to 45 in airline, for 17 fewer turns saved in retail and none in airline. Per-site promotion does not replace the arbiter's judgment at each decision.
- **Score as the flow sees calls.** The first version of `promote` scored only at the end of each turn, and against the agent's next turn alone. That held back airline's reservation reads: GLM-5 reads several reservations in one parallel turn, so at the end of the turn nothing is left to read. It cut D0's airline savings from 41 turns to 4. The flow decides after every call and sees parallel calls one at a time, so `promote` now splits a turn into one call per turn. It also counts a lookup as used whenever the agent makes it later, as the replays do.

## The replay

GLM-5's recorded test episodes, all four trials: 160 retail episodes (1,384 LLM turns) and 80 airline (630). The flows are D0 and the habit alone, compiled goal free from the training tasks exactly as for [the arms](arms-2026-09-24.md), whose numbers the unpromoted rows reproduce. The test tasks, sorted by id, are split into two halves by alternating. Each half is replayed with a flow promoted on the other half's episodes, so no promotion saw the tasks it was replayed on. Totals are summed over both halves.

Three bars, each as `--min-used` / `--min-lower` / `--min-tasks`:

- **a**: 0.5 / 0.3 / 3;
- **b**: 0.7 / 0.5 / 3, the default;
- **c**: 0.8 / 0.6 / 5.

| Domain | Decider | Promotion | Turns saved | Lookups | The agent's own | Detours | Episodes with a detour |
|---|---|---|---|---|---|---|---|
| retail | D0 (arbiter) | none | 294 (21.2%) | 656 | 598 | 58 | 27 |
| retail | D0 | a | 277 (20.0%) | 627 | 581 | 46 | 23 |
| retail | D0 | **b** | **277 (20.0%)** | 591 | 563 | **28** | **17** |
| retail | D0 | c | 273 (19.7%) | 587 | 559 | 28 | 17 |
| retail | habit | none | 311 (22.5%) | 649 | 591 | 58 | 27 |
| retail | habit | a, b | 294 (21.2%) | 625 | 574 | 51 | 20 |
| retail | habit | c | 290 (21.0%) | 621 | 570 | 51 | 20 |
| airline | D0 | none | 41 (6.5%) | 210 | 184 | 26 | 8 |
| airline | D0 | a, b, c | 41 (6.5%) | 187 | 176 | 11 | 2 |
| airline | habit | none | 55 (8.7%) | 250 | 200 | 50 | 20 |
| airline | habit | a, b, c | 55 (8.7%) | 245 | 200 | 45 | 15 |

A saved turn is a recorded turn whose calls the flow had all made already. A detour is a flow lookup the agent never made. The replays assume the agent would have acted the same with the flow's results in hand, as in every replay so far.

Each half's promotion, site by site, is in [promotion-2026-09-25.json](promotion-2026-09-25.json), with the halves' task ids. The replays asked Jev 19 questions no earlier round had, on paths only a promoted flow takes. They cost less than a cent and are in [this round's answer bundle](answers-2026-09-25-promotion.md).

## Shadow mode, live

No live traffic has been run in shadow mode yet. `stretto-proxy --flow-shadow` is tested end to end: the flow logs a would-be lookup, the agent's result passes through unchanged, and `promote` scores the recorded session. `promote --sessions` takes the proxy's logs and `--results` takes τ²-bench's. Both are scored by the same code, the code the replay above used. The recommended loop for a deployment:

1. Record sessions with `--flow-shadow`.
2. Promote on them: `stretto promote --sessions … --oracle-cache ~/.stretto/oracle-cache`.
3. Review the change with `stretto flow-diff`.
4. Serve the promoted flow.
5. Promote again as sessions accumulate. A site short of three tasks today may pass next week.

## What changes

- `stretto promote`, the `promoted` field of the flow IR, and `stretto-proxy --flow-shadow`.
- The bar's defaults are bar b.
- Serving is unchanged for a flow that was never promoted.

## Reproduce

With the pilot's environment, GLM-5's results from `scripts/fetch-leaderboard.sh`, and every published answer bundle imported into `.oracle-cache` ([this round's bundle page](answers-2026-09-25-promotion.md) lists them):

```sh
cargo build --release -p stretto-report
python3 scripts/promotion_study.py --tau2 ../tau2-bench --oracle-cache .oracle-cache \
  --python ~/.venvs/tau2/bin/python --out runs/promotion
```

It compiles each flow, promotes it on each half, replays both halves and the unpromoted flow through `pilot/check_flow.py`, and prints the table above. One promotion by hand, D0's retail flow on the second half's tasks, to replay on the first half's:

```sh
stretto promote --flow retail.flow.json --results .data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --task-ids 9,17,26,32,36,39,42,49,53,56,61,64,68,71,77,86,94,100,102,111 \
  --oracle replay --oracle-cache .oracle-cache --out retail-promoted.flow.json
```
