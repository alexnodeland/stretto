# A record the customer did not ask about

Nearly every detour a flow makes is the right lookup of the wrong record. Replaying flows learned on GLM-5.3's paired-run episodes on held-out ones, 46 of 48 airline detours were `get_reservation_details` or a flight search with arguments the agent never passed, and 38 of 43 retail detours were `get_product_details` or `get_order_details` of records the agent never read. In airline, 43 of the 48 came after the customer had named a different reservation: in that situation only 7 of the flow's 50 lookups were the agent's own. The agent reads the record the customer asked about, and stops; the flow, seeing a habit of reading one reservation after another, went on to the next.

The binding's chance could not see it. It is scored at the agent's own lookups, where the binding picks what the agent picks, so it read 8 of 8 even where the customer had named another value. What it misses is how often the agent reads a second record at all.

## The change

`learn` and `compile` now count, per lookup, the picks the binding would make where the customer had mentioned a value at the lookup's sources and the binding would pass another. After each successful result in training, each distinct such value is one pick, `used` if the agent went on to pass it ([formats](../formats.md): `bindings.named_other`). The binding's chance there is `(used + 2c) / (picks + 2)`, shrunk towards `c`, its chance when nothing was mentioned, so that one or two picks cannot raise it. Everywhere else the chance is as before. `flow-show` shows the count in a last column, and `flow-diff` lists it when it moves. A flow with the count is format version 2, which 0.1.0 refuses rather than read as if the count were not there.

## Replays

Flows on the habit alone at 0.3, before and after, from the same training episodes. Before is the same flow without the count.

| Trained on | Replayed on | Retail: turns saved · detours | Airline: turns saved · detours |
|---|---|---|---|
| GLM-5's published training-task episodes | its 160 retail and 80 airline test-task episodes (four trials) | 311 · 63 → 307 · 38 | 55 · 56 → 55 · 4 |
| GLM-5.3's paired-run episodes, one half of the tasks | the other half, both ways (40 episodes a domain) | 154 · 43 → 154 · 15 | 102 · 48 → 95 · 17 |

In GLM-5's training episodes the agent went on to read none of the records the customer had not named: 0 of 28 reservations, 0 of 90 products and 0 of 24 orders. In GLM-5.3's, one half of the airline tasks showed 8 of 11, where that half's detours did not move; the other showed 0 of 7, and its detours fell from 32 to 1 for 7 turns.

Airline's detours fell by 93% at no cost in turns on GLM-5's episodes, more than [the flow search](search-2026-09-25.md) found by setting per-site thresholds (26 to 6), and from one signal the traces carry rather than a search over settings. It is the same lesson as [the anatomy](anatomy-2026-09-26.md): which record to read is the customer's choice, and the traces show when the customer has made it.

## Against the flow search

[The flow search](search-2026-09-25.md) set a threshold per site to cut D0's detours. D0, compiled again with the count from the same four 2025 agents' training episodes and the same cached answers, replayed at 0.3 on the search's test set, GLM-5's 240 test-task episodes:

| Flow | Airline: turns saved · detours | Retail: turns saved · detours |
|---|---|---|
| D0, with Jev (the arbiter) | 41 · 26 → 41 · 15 | 294 · 58 → 294 · 40 |
| The habit alone | 55 · 50 → 55 · 6 | 311 · 58 → 307 · 33 |
| The search's picks, per-site thresholds on D0 (F2, F6) | 48 · 6 | 293 · 30 |

The habit alone, with the count and without a System-One model, now makes as few airline detours as the search's pick and saves 7 more turns; in retail it saves 14 more turns for 3 more detours. The search's gains came from two sites where it cut the flow back, after a reservation read and after a product read, which is where the flow went on to records the customer had not asked about. The count reaches the same decisions from what the traces show, with nothing to search and no threshold to carry to another agent. Three of retail's 1,193 arbiter decisions were not in the answer cache and handed back; airline's were all answered.

## Reproduce

```sh
stretto learn --results glm-5_enabled_airline_gpt-5.2_4trials.json --tau2 ../tau2-bench \
  --domain airline --habit-only --out airline.flow.json
cd pilot && python check_flow.py --results ../glm-5_enabled_airline_gpt-5.2_4trials.json \
  --trials 0 1 2 3 --domain airline --flow ../airline.flow.json --flow-decider habit \
  --flow-threshold 0.3 --oracle-cache ../.oracle-cache --in-process --jobs 4
```

For "before", delete `bindings.named_other` from the flow and set `stretto_flow` to 1. The GLM-5.3 rows learn from the paired run's baseline episodes on one half of its test tasks and replay the other half's.
