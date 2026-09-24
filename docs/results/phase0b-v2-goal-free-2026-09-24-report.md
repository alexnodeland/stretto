# stretto Phase 0 report

Source: τ²-bench's published baseline trajectories (4 trials per task). The habit is a hierarchical Dirichlet back-off model of the agent's next action, learned from the **successful episodes of the official training tasks** and evaluated on the **held-out test tasks**. Coverage uses context length k = 2 and needs at least 5 training observations of a context before the habit may act on it.

Models marked *(target)* come from τ²-bench leaderboard submissions and are transfer targets: nothing is learned from them, except their own row of each transfer matrix and their own-habit numbers in the model tables.

## retail

74 training tasks, 40 held-out tasks. 16 tools: 7 read, 7 write, 2 other.
Back-off concentration α (fugue MH, 600 draws): median 6.535, 90% interval [5.803, 7.289].

### Runs

| Agent model | User simulator | Episodes | Success | LLM turns/ep | Tool calls/ep | Writes/ep | Macro-tool headroom | Writes after "yes" / any assent |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | gpt-4.1 | 456 | 79% | 13.8 | 7.9 | 1.66 | 35% | 87% / 91% |
| gpt-4.1 | gpt-4.1 | 456 | 74% | 11.2 | 7.7 | 1.63 | 21% | 88% / 94% |
| gpt-4.1-mini | gpt-4.1 | 456 | 66% | 12.4 | 8.0 | 1.68 | 13% | 93% / 95% |
| o4-mini | gpt-4.1 | 456 | 71% | 13.1 | 6.9 | 1.44 | 25% | 88% / 96% |
| glm-5 *(target)* | gpt-5.2 | 456 | 74% | 8.6 | 7.3 | 1.29 | 29% | 97% / 99% |
| **all** | | | | | | | **24%** | |

### Next-action predictability (held-out)

| Agent model | k=0 bits / top-1 | k=1 bits / top-1 | k=2 bits / top-1 | k=3 bits / top-1 |
|---|---|---|---|---|
| claude-3-7-sonnet | 2.65 / 44% | 1.86 / 60% | 1.70 / 63% | 1.62 / 66% |
| gpt-4.1 | 2.67 / 44% | 1.98 / 57% | 1.76 / 61% | 1.73 / 62% |
| gpt-4.1-mini | 2.63 / 46% | 2.11 / 53% | 1.68 / 64% | 1.69 / 67% |
| o4-mini | 2.57 / 48% | 2.09 / 49% | 1.76 / 58% | 1.70 / 59% |
| glm-5 *(target)* | 2.82 / 35% | 1.75 / 63% | 1.62 / 67% | 1.63 / 66% |
| **all models pooled** | 2.64 / 46% | 2.06 / 54% | 1.80 / 59% | 1.76 / 60% |

### Coverage: decisions the habit could take (k = 2)

| Agent model | τ=0.5 cover / agree | τ=0.7 cover / agree | τ=0.8 cover / agree | τ=0.9 cover / agree | τ=0.95 cover / agree |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 67% / 74% | 36% / 86% | 18% / 94% | 9% / 98% | 8% / 99% |
| gpt-4.1 | 63% / 70% | 35% / 80% | 20% / 89% | 15% / 90% | 8% / 95% |
| gpt-4.1-mini | 71% / 72% | 37% / 77% | 12% / 84% | 10% / 84% | 8% / 91% |
| o4-mini | 57% / 69% | 20% / 88% | 18% / 88% | 12% / 89% | 8% / 93% |
| glm-5 *(target)* | 73% / 75% | 40% / 84% | 34% / 87% | 17% / 89% | 8% / 99% |
| **all models pooled** | 71% / 68% | 27% / 80% | 15% / 87% | 8% / 96% | 8% / 96% |

### Where the habit can act (all models pooled, k = 2, τ = 0.8)

| Decision made | Share of decisions | Bits/step | Top-1 | Cover / agree at τ |
|---|---|---|---|---|
| right after the user spoke | 46% | 2.20 | 53% | 23% / 92% |
| right after a tool returned | 54% | 1.46 | 63% | 8% / 74% |

### What the habit gains from code features and a named intent (all models pooled, k = 2)

30 candidate fields in tool outputs. Kept, in order, because each raised the log-likelihood of held-out tasks (5-fold cross-validation grouped by task):

| Tool | Field | Values | Gain (nats) |
|---|---|---|---|
| `get_user_details` | `len(orders)` | 2 | 78.0 |
| `exchange_delivered_order_items` | `len(items)` | 2 | 21.0 |
| `modify_pending_order_items` | `len(items)` | 2 | 9.7 |

The named intent is the set of write tools the episode goes on to call (1 distinct), standing in for what the LLM states when it calls a macro-tool. It is layered on top of the shared model, so rare intents fall back to it.

| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ=0.8 | After the user: cover / agree at τ=0.8 | All: cover / agree at τ=0.8 | at τ=0.9 |
|---|---|---|---|---|---|---|
| the action sequence | 1.80 | 59% | 8% / 74% | 23% / 92% | 15% / 87% | 8% / 96% |
| + code features | 1.81 | 59% | 14% / 80% | 24% / 91% | 18% / 86% | 11% / 91% |
| + code features + named intent | 1.90 | 59% | 14% / 80% | 24% / 91% | 18% / 87% | 11% / 91% |

### Projection: what macro-tools would save on held-out tasks

Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit above (code features + named intent). Where the habit may not act, the decision goes either to the LLM (a pause, one turn) or to a System-One model assumed to agree with the agent (the upper bound Phase 0b will test). Generated arguments always pause. Collapsing every run to one call would save 22.9% of LLM turns (the ceiling).

The habit may act either wherever its top option clears a threshold (with at least 5 training observations), or only in *validated* contexts: those where its top option matched the agent in at least 99% of at least 20 decisions from at least 10 distinct tasks, under 5-fold cross-validation grouped by task. A habit that hands back too early is safe: the LLM takes the step, at the cost of one turn. A habit that picks another tool, or carries on when the agent stopped, is the risk.

| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |
|---|---|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | **1.2%** | 3.73 | 0.46 | 0.00 | 6.7 | 2.0 | 2.0% |
| pause for the LLM | top option ≥ 0.95 | **1.2%** | 3.73 | 0.43 | 0.00 | 6.4 | 1.9 | 1.9% |
| pause for the LLM | in 1 validated context | **0.0%** | 3.88 | 0.00 | 0.00 | 0.0 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.9 | **21.8%** | 0.15 | 0.46 | 6.99 | 6.7 | 2.0 | 2.0% |
| ask a perfect System-One model | top option ≥ 0.95 | **21.8%** | 0.15 | 0.43 | 7.02 | 6.4 | 1.9 | 1.9% |
| ask a perfect System-One model | in 1 validated context | **22.3%** | 0.08 | 0.00 | 7.45 | 0.0 | 0.0 | 0.0% |

Validated contexts, and how the habit did in them on held-out tasks:

| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |
|---|---|---|---|
| `start` → `start` | respond | 99.0% of 831 (74 tasks) | 95.5% of 640 |

Tokens and dollars come from the usage the benchmark recorded for every LLM call. A removed turn saves its whole prompt and completion. A run that collapses to one call also stops carrying its intermediate tool outputs in every later prompt; their size is read off the recorded prompt growth, and priced at each model's effective input price, fitted to its recorded costs by least squares (so it absorbs any prompt-caching discount): claude-3-7-sonnet $3.00 in / $15.00 out per MTok; gpt-4.1 $0.92 in / $11.18 out per MTok; gpt-4.1-mini $0.21 in / $1.18 out per MTok; o4-mini $0.60 in / $4.14 out per MTok.

| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | 1.2% | 1.1% | 0.6% | **1.0%** | 0.1123 → 0.1112 |
| pause for the LLM | top option ≥ 0.95 | 1.2% | 1.1% | 0.6% | **1.0%** | 0.1123 → 0.1112 |
| pause for the LLM | in 1 validated context | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1123 → 0.1123 |
| ask a perfect System-One model | top option ≥ 0.9 | 21.8% | 25.9% | 18.7% | **32.0%** | 0.1123 → 0.0764 |
| ask a perfect System-One model | top option ≥ 0.95 | 21.8% | 25.9% | 18.7% | **32.0%** | 0.1123 → 0.0764 |
| ask a perfect System-One model | in 1 validated context | 22.3% | 26.5% | 19.1% | **32.4%** | 0.1123 → 0.0759 |

By agent model, with the habit acting only in validated contexts and a perfect System-One model deciding the rest. *Read-only flows* follow RFC-001's plan/commit rule: between LLM turns a flow only reads, and every write goes back to the LLM. Pooled dollars weigh each model by its spend. Models marked *(target)* were never trained on: the habit, its features, the closed argument sets and the validated contexts all come from the other models, and the pooled tables above leave them out.

| Agent model | Tool turns with parallel calls | Turns saved | Read-only flows | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 0% | **33.0%** | 31.3% | 33.6% | 38.4% | 25.4% | 37.4% | 0.3247 → 0.2032 |
| gpt-4.1 | 20% | **19.7%** | 19.0% | 20.0% | 20.5% | 7.8% | 17.1% | 0.0579 → 0.0480 |
| gpt-4.1-mini | 20% | **12.4%** | 12.0% | 13.3% | 15.0% | 7.2% | 13.3% | 0.0134 → 0.0117 |
| o4-mini | 0% | **22.8%** | 20.5% | 23.6% | 24.9% | 21.0% | 23.4% | 0.0532 → 0.0407 |
| glm-5 *(target)* | 27% | **29.1%** | 28.2% | 30.0% | 29.5% | 18.8% | – | no cost recorded |

### Phase 0b: the System-One model in shadow mode

Questions v2 (RFC-001 §3.5), for read-only flows. At every held-out decision right after a tool returns, the System-One model was asked which lookup the agent makes next, with the options limited to the lookups the agent made after the same tool in training, or whether it hands back (to write to the customer, or to make a change, which only the LLM does). The same request also asks the stop decision on its own (*split*: "does it make another lookup now?", then which one). The state is a slice: the customer's messages, the agent's last message, the latest four results in full and older lookups one line each. *Combined* answers weigh the System-One answers against the habit's prediction and the model's record at that site on other tasks, fitted by cross-validation over tasks (RFC-001 §3.6). Numeric arguments are not asked about. Oracle: `replay` (answering model: jev-1.13.0). 5852 decisions, 5482 distinct questions, 5482 answered, 0 failed; 15588889 input tokens ($0.65 at $0.042/MTok). 230 next-step decisions came at sites where the agent never looked anything up in training, so the flow hands back without asking; the options held the agent's step in 99.1% of the source models' next-step decisions.

Next step, as a read-only flow takes it (a lookup, or hand back):

| Agent model | Decisions | One question | Split | Combined | Stop vs. go on | ECE | Combined p ≥ 0.9: share / agreed | Combined p ≥ 0.99: share / agreed |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 1211 | 78.7% | 74.2% | 84.7% | 79.9% | 0.053 | 37.7% / 96.7% | 6.3% / 100.0% |
| gpt-4.1 | 1173 | 76.8% | 71.3% | 83.4% | 78.6% | 0.064 | 39.9% / 96.8% | 7.9% / 100.0% |
| gpt-4.1-mini | 1252 | 70.9% | 73.9% | 76.8% | 72.4% | 0.114 | 41.0% / 97.3% | 9.2% / 100.0% |
| o4-mini | 1060 | 78.7% | 76.6% | 76.8% | 79.9% | 0.064 | 42.8% / 95.8% | 11.2% / 99.2% |
| glm-5 *(target)* | 1153 | 78.1% | 68.8% | 85.6% | 80.5% | 0.050 | 37.5% / 98.8% | 7.2% / 100.0% |
| **all source models** | 4696 | 76.1% | 73.9% | 80.5% | 77.6% | 0.066 | 40.3% / 96.7% | 8.6% / 99.8% |

The arbiter's weights, averaged over folds: habit 0.27, one question 0.17, split 0.35, handing back 0.23, predicate list_pending 0.94, predicate needs_options 0.26, predicate must_ask 0.30, the model's record at the site 0.34.

Predicates asked with every next-step question:

- `list_pending`: Is the agent working through a list of records one lookup at a time (orders, items, products, reservations, flights or dates), with some it has not looked up yet?
- `needs_options`: Does the customer's request need options the agent has not looked up yet, such as another version of an item, other products, or flights on a date or route not yet searched?
- `must_ask`: Does the agent need something only the customer can give before any further lookup would help, such as identification, a choice between options, or a confirmation?

Projection with the System-One model, pooled over the source models, for read-only flows: the habit acts in validated contexts, a System-One pick is trusted at or above the threshold (*two keys*: only when it is also the habit's top option; *combined*: the arbiter's probability), and anything else pauses. A write always goes back to the LLM. Nothing a read-only flow does changes the environment, so its wrong picks are *detours*, extra lookups that cost a pause, not risks. Handing back costs a turn and a detour does not, so *lookup first* takes the likeliest lookup whenever it clears the (lower) threshold. Each detour is charged a typical lookup's output (251 tokens) in every later prompt, so the dollars saved include the detours' cost.

| Pick trusted at | Turns saved | $ saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour | Writes handed back/ep |
|---|---|---|---|---|---|---|---|---|
| p ≥ 0.5 | **13.8%** | 16.0% | 1.57 | 6.84 | 98.1 | 64.8 | 51.7% | 0.34 |
| p ≥ 0.7 | **10.3%** | 12.2% | 2.11 | 5.45 | 62.5 | 34.4 | 30.3% | 0.34 |
| p ≥ 0.9 | **3.4%** | 3.5% | 3.28 | 3.18 | 23.4 | 7.2 | 6.9% | 0.34 |
| p ≥ 0.95 | **1.8%** | 1.5% | 3.57 | 2.30 | 10.2 | 1.7 | 1.7% | 0.34 |
| p ≥ 0.99 | **0.5%** | 0.3% | 3.81 | 1.14 | 3.1 | 0.3 | 0.3% | 0.34 |
| two keys, p ≥ 0.5 | **12.3%** | 13.8% | 1.82 | 4.93 | 34.8 | 51.6 | 43.6% | 0.34 |
| two keys, p ≥ 0.7 | **9.3%** | 10.7% | 2.30 | 4.03 | 25.2 | 29.8 | 26.9% | 0.34 |
| two keys, p ≥ 0.9 | **3.1%** | 3.4% | 3.32 | 2.42 | 9.1 | 6.1 | 5.9% | 0.34 |
| two keys, p ≥ 0.95 | **1.7%** | 1.5% | 3.59 | 1.85 | 3.9 | 1.7 | 1.7% | 0.34 |
| two keys, p ≥ 0.99 | **0.5%** | 0.3% | 3.81 | 1.00 | 1.9 | 0.3 | 0.3% | 0.34 |
| combined, p ≥ 0.5 | **15.6%** | 19.1% | 1.23 | 6.75 | 58.9 | 70.6 | 50.0% | 0.34 |
| combined, p ≥ 0.7 | **10.1%** | 12.8% | 1.99 | 4.90 | 32.5 | 31.6 | 25.9% | 0.34 |
| combined, p ≥ 0.9 | **4.9%** | 6.7% | 2.82 | 2.81 | 6.4 | 3.4 | 3.4% | 0.34 |
| combined, p ≥ 0.95 | **3.4%** | 4.6% | 3.07 | 1.93 | 2.5 | 2.0 | 2.0% | 0.34 |
| combined, p ≥ 0.99 | **0.7%** | 0.6% | 3.70 | 0.63 | 0.0 | 0.2 | 0.2% | 0.34 |
| lookup first, p ≥ 0.2 | **17.8%** | 21.7% | 0.83 | 4.67 | 0.0 | 130.0 | 71.6% | 0.34 |
| lookup first, p ≥ 0.3 | **17.3%** | 21.1% | 0.96 | 4.27 | 0.0 | 102.0 | 62.3% | 0.34 |
| lookup first, p ≥ 0.4 | **16.5%** | 20.2% | 1.09 | 3.97 | 0.0 | 86.1 | 57.7% | 0.34 |

The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes holding a risky decision (a conservative stand-in for losing at most one point of pass^1). Read-only flows take no risky decisions, so offline the gate is the turns saved; each cell is turns saved · episodes with a detour. Whether detours cost pass^1 is for a live run to show.

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | lookup first, p ≥ 0.3 | Gate |
|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 31.3% | 19.8% · 15.0% | 15.1% · 8.1% | 4.3% · 1.2% | 23.4% · 17.5% | 14.9% · 7.5% | 7.1% · 0.0% | 25.8% · 33.1% | **passes offline** (combined, p ≥ 0.5; detours in 18% of episodes) |
| gpt-4.1 | 19.0% | 12.5% · 51.2% | 8.5% · 27.5% | 3.1% · 3.8% | 13.8% · 49.4% | 7.1% · 25.6% | 2.4% · 1.2% | 15.5% · 59.4% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 12.0% | 6.5% · 76.9% | 4.9% · 40.6% | 1.8% · 10.6% | 8.0% · 64.4% | 5.6% · 26.9% | 1.5% · 3.1% | 9.4% · 79.4% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.5% | 15.5% · 63.7% | 12.2% · 45.0% | 4.2% · 11.9% | 16.2% · 68.8% | 12.0% · 43.8% | 8.0% · 9.4% | 17.3% · 77.5% | fails |
| glm-5 *(target)* | 28.2% | 12.0% · 11.2% | 6.4% · 2.5% | 2.7% · 0.0% | 16.6% · 11.2% | 8.5% · 3.1% | 2.1% · 0.0% | 20.3% · 28.1% | **passes offline** (lookup first, p ≥ 0.3; detours in 28% of episodes) |

### Transfer: top-1 when the habit comes from another model (k = 2)

| habit from ↓ / tested on → | claude-3-7-sonnet | gpt-4.1 | gpt-4.1-mini | o4-mini | glm-5 *(target)* |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 63% | 59% | 48% | 54% | 63% |
| gpt-4.1 | 62% | 61% | 52% | 56% | 62% |
| gpt-4.1-mini | 52% | 53% | 64% | 51% | 60% |
| o4-mini | 59% | 60% | 52% | 58% | 56% |
| glm-5 *(target)* | 56% | 54% | 53% | 45% | 67% |

### Most common tool runs (successful training episodes, all models)

| Count | Run |
|---|---|
| 216 | `return_delivered_order_items` |
| 205 | `find_user_id_by_name_zip` |
| 182 | `exchange_delivered_order_items` |
| 142 | `find_user_id_by_name_zip → get_user_details` |
| 141 | `get_order_details` |
| 128 | `modify_pending_order_items` |
| 116 | `get_product_details` |
| 115 | `find_user_id_by_email` |
| 96 | `get_user_details` |
| 88 | `cancel_pending_order` |
| 76 | `get_order_details → get_order_details → get_order_details` |
| 64 | `get_order_details → get_order_details` |

### Argument provenance (all episodes, all models)

| Tool | Kind | Argument | Values | User | Tool output | Both | Generated | Literal | Short |
|---|---|---|---|---|---|---|---|---|---|
| `calculate` | other | `expression` | 117 |  | 3% |  | 97% |  |  |
| `cancel_pending_order` | **write** | `order_id` | 434 |  | 58% | 42% |  |  |  |
| `cancel_pending_order` | **write** | `reason` | 434 | 45% | 8% | 6% | 41% |  |  |
| `exchange_delivered_order_items` | **write** | `item_ids` | 725 |  | 95% | 5% |  |  |  |
| `exchange_delivered_order_items` | **write** | `new_item_ids` | 725 | 0% | 81% | 18% |  |  | 1% |
| `exchange_delivered_order_items` | **write** | `order_id` | 619 |  | 79% | 21% |  |  |  |
| `exchange_delivered_order_items` | **write** | `payment_method_id` | 619 | 0% | 96% | 2% | 2% |  |  |
| `find_user_id_by_email` | read | `email` | 494 | 100% |  |  | 0% |  |  |
| `find_user_id_by_name_zip` | read | `first_name` | 1535 | 100% |  | 0% | 0% |  | 0% |
| `find_user_id_by_name_zip` | read | `last_name` | 1535 | 91% |  | 0% | 0% |  | 8% |
| `find_user_id_by_name_zip` | read | `zip` | 1535 | 99% |  |  | 0% |  | 1% |
| `get_order_details` | read | `order_id` | 5010 | 2% | 90% | 5% | 4% |  | 0% |
| `get_product_details` | read | `product_id` | 1805 | 0% | 99% | 1% | 0% |  |  |
| `get_user_details` | read | `user_id` | 1730 | 0% | 99% | 1% |  |  |  |
| `modify_pending_order_address` | **write** | `address1` | 318 | 10% | 67% | 23% |  |  |  |
| `modify_pending_order_address` | **write** | `address2` | 318 | 5% | 66% | 19% | 0% |  | 10% |
| `modify_pending_order_address` | **write** | `city` | 318 | 5% | 30% | 65% |  |  |  |
| `modify_pending_order_address` | **write** | `country` | 318 |  | 95% | 5% |  |  |  |
| `modify_pending_order_address` | **write** | `order_id` | 318 |  | 79% | 21% |  |  |  |
| `modify_pending_order_address` | **write** | `state` | 318 |  |  |  |  |  | 100% |
| `modify_pending_order_address` | **write** | `zip` | 318 | 5% | 59% | 36% |  |  |  |
| `modify_pending_order_items` | **write** | `item_ids` | 911 |  | 99% | 1% |  |  |  |
| `modify_pending_order_items` | **write** | `new_item_ids` | 843 | 1% | 85% | 13% |  |  | 0% |
| `modify_pending_order_items` | **write** | `order_id` | 649 |  | 82% | 18% |  |  |  |
| `modify_pending_order_items` | **write** | `payment_method_id` | 649 |  | 100% | 0% | 0% |  |  |
| `modify_pending_order_payment` | **write** | `order_id` | 18 |  | 100% |  |  |  |  |
| `modify_pending_order_payment` | **write** | `payment_method_id` | 18 |  | 100% |  |  |  |  |
| `modify_user_address` | **write** | `address1` | 163 | 29% | 57% | 13% | 1% |  |  |
| `modify_user_address` | **write** | `address2` | 163 | 1% | 63% | 24% | 1% |  | 12% |
| `modify_user_address` | **write** | `city` | 163 | 12% | 12% | 77% |  |  |  |
| `modify_user_address` | **write** | `country` | 163 |  | 85% | 15% | 1% |  |  |
| `modify_user_address` | **write** | `state` | 163 |  |  |  |  |  | 100% |
| `modify_user_address` | **write** | `user_id` | 163 |  | 90% | 10% |  |  |  |
| `modify_user_address` | **write** | `zip` | 163 | 13% | 48% | 38% | 1% |  |  |
| `return_delivered_order_items` | **write** | `item_ids` | 1281 |  | 97% | 3% | 0% |  |  |
| `return_delivered_order_items` | **write** | `order_id` | 719 |  | 73% | 27% |  |  |  |
| `return_delivered_order_items` | **write** | `payment_method_id` | 719 |  | 99% | 1% | 0% |  |  |
| `transfer_to_human_agents` | other | `summary` | 104 |  |  |  | 100% |  |  |

## airline

30 training tasks, 20 held-out tasks. 14 tools: 6 read, 6 write, 2 other.
Back-off concentration α (fugue MH, 600 draws): median 8.876, 90% interval [7.540, 10.566].

### Runs

| Agent model | User simulator | Episodes | Success | LLM turns/ep | Tool calls/ep | Writes/ep | Macro-tool headroom | Writes after "yes" / any assent |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | gpt-4.1 | 200 | 50% | 13.5 | 8.4 | 1.46 | 41% | 66% / 85% |
| gpt-4.1 | gpt-4.1 | 200 | 56% | 9.9 | 7.7 | 1.11 | 17% | 68% / 89% |
| gpt-4.1-mini | gpt-4.1 | 200 | 50% | 11.2 | 7.9 | 1.60 | 11% | 85% / 98% |
| o4-mini | gpt-4.1 | 200 | 59% | 9.9 | 5.2 | 0.87 | 28% | 76% / 86% |
| glm-5 *(target)* | gpt-5.2 | 200 | 82% | 7.3 | 7.3 | 0.83 | 18% | 97% / 99% |
| **all** | | | | | | | **25%** | |

### Next-action predictability (held-out)

| Agent model | k=0 bits / top-1 | k=1 bits / top-1 | k=2 bits / top-1 | k=3 bits / top-1 |
|---|---|---|---|---|
| claude-3-7-sonnet | 2.93 / 38% | 2.47 / 46% | 2.42 / 52% | 2.49 / 51% |
| gpt-4.1 | 2.87 / 41% | 2.10 / 50% | 2.10 / 54% | 2.11 / 54% |
| gpt-4.1-mini | 2.75 / 45% | 2.14 / 49% | 2.09 / 51% | 2.08 / 51% |
| o4-mini | 2.60 / 49% | 2.22 / 49% | 2.17 / 56% | 2.20 / 55% |
| glm-5 *(target)* | 2.76 / 34% | 2.02 / 58% | 1.83 / 57% | 1.78 / 64% |
| **all models pooled** | 2.81 / 43% | 2.17 / 55% | 2.21 / 52% | 2.27 / 53% |

### Coverage: decisions the habit could take (k = 2)

| Agent model | τ=0.5 cover / agree | τ=0.7 cover / agree | τ=0.8 cover / agree | τ=0.9 cover / agree | τ=0.95 cover / agree |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 52% / 60% | 18% / 79% | 8% / 99% | 1% / 100% | 0% / 0% |
| gpt-4.1 | 55% / 58% | 10% / 91% | 8% / 100% | 8% / 100% | 8% / 100% |
| gpt-4.1-mini | 51% / 56% | 11% / 91% | 8% / 99% | 7% / 100% | 7% / 100% |
| o4-mini | 28% / 72% | 13% / 97% | 13% / 97% | 13% / 98% | 13% / 98% |
| glm-5 *(target)* | 64% / 65% | 26% / 73% | 7% / 83% | 2% / 93% | 2% / 93% |
| **all models pooled** | 50% / 59% | 22% / 80% | 11% / 99% | 9% / 99% | 9% / 99% |

### Where the habit can act (all models pooled, k = 2, τ = 0.8)

| Decision made | Share of decisions | Bits/step | Top-1 | Cover / agree at τ |
|---|---|---|---|---|
| right after the user spoke | 43% | 2.25 | 59% | 18% / 99% |
| right after a tool returned | 57% | 2.18 | 48% | 6% / 99% |

### What the habit gains from code features and a named intent (all models pooled, k = 2)

50 candidate fields in tool outputs. Kept, in order, because each raised the log-likelihood of held-out tasks (5-fold cross-validation grouped by task):

| Tool | Field | Values | Gain (nats) |
|---|---|---|---|
| `get_user_details` | `membership` | 3 | 17.4 |
| `update_reservation_flights` | `insurance` | 2 | 5.9 |

The named intent is the set of write tools the episode goes on to call (1 distinct), standing in for what the LLM states when it calls a macro-tool. It is layered on top of the shared model, so rare intents fall back to it.

| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ=0.8 | After the user: cover / agree at τ=0.8 | All: cover / agree at τ=0.8 | at τ=0.9 |
|---|---|---|---|---|---|---|
| the action sequence | 2.21 | 52% | 6% / 99% | 18% / 99% | 11% / 99% | 9% / 99% |
| + code features | 2.22 | 52% | 8% / 97% | 18% / 99% | 12% / 98% | 10% / 99% |
| + code features + named intent | 2.42 | 51% | 8% / 96% | 18% / 99% | 12% / 98% | 11% / 99% |

### Projection: what macro-tools would save on held-out tasks

Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit above (code features + named intent). Where the habit may not act, the decision goes either to the LLM (a pause, one turn) or to a System-One model assumed to agree with the agent (the upper bound Phase 0b will test). Generated arguments always pause. Collapsing every run to one call would save 23.9% of LLM turns (the ceiling).

The habit may act either wherever its top option clears a threshold (with at least 5 training observations), or only in *validated* contexts: those where its top option matched the agent in at least 99% of at least 20 decisions from at least 10 distinct tasks, under 5-fold cross-validation grouped by task. A habit that hands back too early is safe: the LLM takes the step, at the cost of one turn. A habit that picks another tool, or carries on when the agent stopped, is the risk.

| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |
|---|---|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | **0.0%** | 4.60 | 0.46 | 0.00 | 1.2 | 0.0 | 0.0% |
| pause for the LLM | top option ≥ 0.95 | **0.0%** | 4.60 | 0.27 | 0.00 | 0.3 | 0.0 | 0.0% |
| pause for the LLM | in 1 validated context | **0.0%** | 4.60 | 0.16 | 0.00 | 0.0 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.9 | **20.5%** | 0.55 | 0.46 | 7.29 | 1.2 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.95 | **20.6%** | 0.54 | 0.27 | 7.48 | 0.3 | 0.0 | 0.0% |
| ask a perfect System-One model | in 1 validated context | **20.6%** | 0.54 | 0.16 | 7.59 | 0.0 | 0.0 | 0.0% |

Validated contexts, and how the habit did in them on held-out tasks:

| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |
|---|---|---|---|
| `respond (user replied)` → `transfer_to_human_agents (ok)` | respond | 100.0% of 96 (15 tasks) | 100.0% of 52 |

Tokens and dollars come from the usage the benchmark recorded for every LLM call. A removed turn saves its whole prompt and completion. A run that collapses to one call also stops carrying its intermediate tool outputs in every later prompt; their size is read off the recorded prompt growth, and priced at each model's effective input price, fitted to its recorded costs by least squares (so it absorbs any prompt-caching discount): claude-3-7-sonnet $3.00 in / $15.00 out per MTok; gpt-4.1 $1.04 in / $6.69 out per MTok; gpt-4.1-mini $0.20 in / $1.44 out per MTok; o4-mini $0.65 in / $4.04 out per MTok.

| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| pause for the LLM | top option ≥ 0.95 | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| pause for the LLM | in 1 validated context | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| ask a perfect System-One model | top option ≥ 0.9 | 20.5% | 28.2% | 24.9% | **37.3%** | 0.1170 → 0.0733 |
| ask a perfect System-One model | top option ≥ 0.95 | 20.6% | 28.3% | 24.9% | **37.6%** | 0.1170 → 0.0730 |
| ask a perfect System-One model | in 1 validated context | 20.6% | 28.3% | 24.9% | **37.6%** | 0.1170 → 0.0730 |

By agent model, with the habit acting only in validated contexts and a perfect System-One model deciding the rest. *Read-only flows* follow RFC-001's plan/commit rule: between LLM turns a flow only reads, and every write goes back to the LLM. Pooled dollars weigh each model by its spend. Models marked *(target)* were never trained on: the habit, its features, the closed argument sets and the validated contexts all come from the other models, and the pooled tables above leave them out.

| Agent model | Tool turns with parallel calls | Turns saved | Read-only flows | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 0% | **36.6%** | 32.6% | 40.7% | 44.1% | 28.0% | 42.8% | 0.3508 → 0.2008 |
| gpt-4.1 | 22% | **10.9%** | 8.9% | 15.3% | 14.6% | 9.3% | 14.8% | 0.0517 → 0.0440 |
| gpt-4.1-mini | 24% | **6.6%** | 5.3% | 9.1% | 9.2% | 6.9% | 9.8% | 0.0128 → 0.0115 |
| o4-mini | 0% | **24.5%** | 20.7% | 26.8% | 30.2% | 29.7% | 32.4% | 0.0530 → 0.0358 |
| glm-5 *(target)* | 45% | **15.4%** | 14.9% | 18.9% | 17.6% | 13.7% | – | no cost recorded |

### Phase 0b: the System-One model in shadow mode

Questions v2 (RFC-001 §3.5), for read-only flows. At every held-out decision right after a tool returns, the System-One model was asked which lookup the agent makes next, with the options limited to the lookups the agent made after the same tool in training, or whether it hands back (to write to the customer, or to make a change, which only the LLM does). The same request also asks the stop decision on its own (*split*: "does it make another lookup now?", then which one). The state is a slice: the customer's messages, the agent's last message, the latest four results in full and older lookups one line each. *Combined* answers weigh the System-One answers against the habit's prediction and the model's record at that site on other tasks, fitted by cross-validation over tasks (RFC-001 §3.6). Numeric arguments are not asked about. Oracle: `replay` (answering model: jev-1.13.0). 3099 decisions, 2610 distinct questions, 2610 answered, 0 failed; 6775074 input tokens ($0.28 at $0.042/MTok). 466 next-step decisions came at sites where the agent never looked anything up in training, so the flow hands back without asking; the options held the agent's step in 99.1% of the source models' next-step decisions.

Next step, as a read-only flow takes it (a lookup, or hand back):

| Agent model | Decisions | One question | Split | Combined | Stop vs. go on | ECE | Combined p ≥ 0.9: share / agreed | Combined p ≥ 0.99: share / agreed |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 672 | 69.8% | 67.6% | 73.7% | 75.9% | 0.118 | 39.3% / 92.0% | 19.3% / 92.3% |
| gpt-4.1 | 643 | 71.9% | 68.6% | 83.2% | 75.7% | 0.063 | 38.1% / 99.2% | 22.1% / 100.0% |
| gpt-4.1-mini | 680 | 66.0% | 65.1% | 77.9% | 69.4% | 0.148 | 41.6% / 97.5% | 27.4% / 99.5% |
| o4-mini | 424 | 80.2% | 81.6% | 79.2% | 81.6% | 0.069 | 51.9% / 94.5% | 35.4% / 96.7% |
| glm-5 *(target)* | 645 | 62.8% | 59.5% | 76.1% | 67.8% | 0.174 | 32.7% / 98.1% | 20.8% / 98.5% |
| **all source models** | 2419 | 71.1% | 69.6% | 78.4% | 75.0% | 0.099 | 41.8% / 95.8% | 25.1% / 97.4% |

The arbiter's weights, averaged over folds: habit 0.25, one question 0.21, split 0.13, handing back 0.27, predicate list_pending 1.25, predicate needs_options -0.04, predicate must_ask 0.80, the model's record at the site 0.63.

Predicates asked with every next-step question:

- `list_pending`: Is the agent working through a list of records one lookup at a time (orders, items, products, reservations, flights or dates), with some it has not looked up yet?
- `needs_options`: Does the customer's request need options the agent has not looked up yet, such as another version of an item, other products, or flights on a date or route not yet searched?
- `must_ask`: Does the agent need something only the customer can give before any further lookup would help, such as identification, a choice between options, or a confirmation?

Closed-set arguments:

| Agent model | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|
| claude-3-7-sonnet | 8 | 100.0% | 100.0% / 100.0% |
| gpt-4.1 | 3 | 100.0% | 100.0% / 100.0% |
| gpt-4.1-mini | 4 | 25.0% | 25.0% / 100.0% |
| o4-mini | 7 | 100.0% | 100.0% / 100.0% |
| glm-5 *(target)* | 13 | 61.5% | 84.6% / 72.7% |
| **all source models** | 22 | 86.4% | 86.4% / 100.0% |

Closed-set arguments by argument (source models):

| Tool | Argument | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|---|
| `search_onestop_flight` | `date` | 19 | 100.0% | 100.0% / 100.0% |
| `search_onestop_flight` | `destination` | 2 | 0.0% | 0.0% / 0.0% |
| `search_onestop_flight` | `origin` | 1 | 0.0% | 0.0% / 0.0% |

Projection with the System-One model, pooled over the source models, for read-only flows: the habit acts in validated contexts, a System-One pick is trusted at or above the threshold (*two keys*: only when it is also the habit's top option; *combined*: the arbiter's probability), and anything else pauses. A write always goes back to the LLM. Nothing a read-only flow does changes the environment, so its wrong picks are *detours*, extra lookups that cost a pause, not risks. Handing back costs a turn and a detour does not, so *lookup first* takes the likeliest lookup whenever it clears the (lower) threshold. Each detour is charged a typical lookup's output (214 tokens) in every later prompt, so the dollars saved include the detours' cost.

| Pick trusted at | Turns saved | $ saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour | Writes handed back/ep |
|---|---|---|---|---|---|---|---|---|
| p ≥ 0.5 | **9.8%** | 14.0% | 2.69 | 6.16 | 103.4 | 68.1 | 46.9% | 0.68 |
| p ≥ 0.7 | **8.3%** | 11.1% | 3.04 | 4.68 | 53.8 | 34.7 | 28.1% | 0.68 |
| p ≥ 0.9 | **4.9%** | 6.0% | 3.63 | 2.95 | 18.1 | 8.1 | 8.1% | 0.68 |
| p ≥ 0.95 | **3.5%** | 4.5% | 3.90 | 2.33 | 10.0 | 4.1 | 4.1% | 0.68 |
| p ≥ 0.99 | **0.8%** | 1.1% | 4.39 | 1.34 | 5.0 | 0.9 | 0.9% | 0.68 |
| two keys, p ≥ 0.5 | **5.7%** | 7.5% | 3.53 | 3.40 | 45.6 | 19.4 | 18.8% | 0.68 |
| two keys, p ≥ 0.7 | **5.1%** | 6.3% | 3.65 | 2.79 | 17.5 | 12.2 | 11.9% | 0.68 |
| two keys, p ≥ 0.9 | **3.4%** | 4.2% | 3.94 | 2.16 | 9.1 | 2.5 | 2.5% | 0.68 |
| two keys, p ≥ 0.95 | **2.5%** | 3.4% | 4.12 | 1.82 | 5.9 | 1.2 | 1.2% | 0.68 |
| two keys, p ≥ 0.99 | **0.6%** | 0.9% | 4.45 | 1.15 | 3.4 | 0.3 | 0.3% | 0.68 |
| combined, p ≥ 0.5 | **11.0%** | 15.8% | 2.22 | 6.03 | 56.6 | 62.5 | 39.7% | 0.68 |
| combined, p ≥ 0.7 | **8.4%** | 11.7% | 2.86 | 4.17 | 21.9 | 25.0 | 16.9% | 0.68 |
| combined, p ≥ 0.9 | **5.5%** | 7.6% | 3.39 | 2.61 | 6.6 | 6.2 | 4.7% | 0.68 |
| combined, p ≥ 0.95 | **4.3%** | 5.6% | 3.62 | 2.10 | 3.1 | 5.0 | 3.8% | 0.68 |
| combined, p ≥ 0.99 | **2.5%** | 2.6% | 3.95 | 1.43 | 2.8 | 1.9 | 1.6% | 0.68 |
| lookup first, p ≥ 0.2 | **13.5%** | 20.8% | 1.66 | 5.42 | 2.5 | 147.5 | 72.5% | 0.68 |
| lookup first, p ≥ 0.3 | **12.6%** | 18.9% | 1.81 | 4.80 | 2.5 | 102.8 | 60.0% | 0.68 |
| lookup first, p ≥ 0.4 | **11.8%** | 17.5% | 1.99 | 4.39 | 2.5 | 81.2 | 50.9% | 0.68 |

The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes holding a risky decision (a conservative stand-in for losing at most one point of pass^1). Read-only flows take no risky decisions, so offline the gate is the turns saved; each cell is turns saved · episodes with a detour. Whether detours cost pass^1 is for a live run to show.

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | lookup first, p ≥ 0.3 | Gate |
|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 32.6% | 15.2% · 32.5% | 11.4% · 20.0% | 5.7% · 3.8% | 17.5% · 17.5% | 12.2% · 7.5% | 7.9% · 5.0% | 21.3% · 46.2% | **passes offline** (lookup first, p ≥ 0.3; detours in 46% of episodes) |
| gpt-4.1 | 8.9% | 5.0% · 55.0% | 4.6% · 27.5% | 3.6% · 5.0% | 6.4% · 41.2% | 5.0% · 12.5% | 2.7% · 2.5% | 7.1% · 61.3% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 5.3% | 2.1% · 61.3% | 2.1% · 40.0% | 1.7% · 13.8% | 2.6% · 55.0% | 1.8% · 22.5% | 1.1% · 3.8% | 2.7% · 75.0% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.7% | 15.9% · 38.8% | 14.8% · 25.0% | 8.8% · 10.0% | 16.6% · 45.0% | 13.9% · 25.0% | 10.3% · 7.5% | 17.7% · 57.5% | fails |
| glm-5 *(target)* | 14.9% | 2.2% · 43.8% | 1.6% · 30.0% | 0.8% · 15.0% | 4.4% · 36.2% | 1.4% · 8.8% | 0.6% · 3.8% | 7.1% · 56.2% | fails: below 20% even with a perfect System-One model |

### Transfer: top-1 when the habit comes from another model (k = 2)

| habit from ↓ / tested on → | claude-3-7-sonnet | gpt-4.1 | gpt-4.1-mini | o4-mini | glm-5 *(target)* |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 52% | 47% | 48% | 56% | 57% |
| gpt-4.1 | 49% | 54% | 53% | 54% | 50% |
| gpt-4.1-mini | 46% | 47% | 51% | 53% | 48% |
| o4-mini | 51% | 47% | 48% | 56% | 54% |
| glm-5 *(target)* | 53% | 48% | 49% | 55% | 57% |

### Most common tool runs (successful training episodes, all models)

| Count | Run |
|---|---|
| 101 | `get_reservation_details` |
| 97 | `transfer_to_human_agents` |
| 63 | `get_user_details` |
| 27 | `update_reservation_flights` |
| 19 | `get_user_details → get_reservation_details` |
| 17 | `search_direct_flight → search_direct_flight` |
| 15 | `update_reservation_passengers` |
| 14 | `get_reservation_details → get_user_details` |
| 12 | `book_reservation` |
| 12 | `get_reservation_details → transfer_to_human_agents` |
| 11 | `send_certificate` |
| 9 | `search_onestop_flight` |

### Argument provenance (all episodes, all models)

| Tool | Kind | Argument | Values | User | Tool output | Both | Generated | Literal | Short |
|---|---|---|---|---|---|---|---|---|---|
| `book_reservation` | **write** | `cabin` | 148 |  | 34% | 66% |  |  |  |
| `book_reservation` | **write** | `destination` | 148 |  | 48% | 52% |  |  |  |
| `book_reservation` | **write** | `flight_type` | 148 |  | 76% |  | 24% |  |  |
| `book_reservation` | **write** | `flights` | 616 |  | 83% | 9% | 7% |  |  |
| `book_reservation` | **write** | `insurance` | 148 |  |  | 5% |  |  | 95% |
| `book_reservation` | **write** | `nonfree_baggages` | 148 |  |  |  |  |  | 100% |
| `book_reservation` | **write** | `origin` | 148 |  | 49% | 51% |  |  |  |
| `book_reservation` | **write** | `passengers` | 570 | 10% | 38% | 46% | 1% |  | 5% |
| `book_reservation` | **write** | `payment_methods` | 574 | 4% | 65% | 11% | 11% |  | 9% |
| `book_reservation` | **write** | `total_baggages` | 148 |  |  |  |  |  | 100% |
| `book_reservation` | **write** | `user_id` | 148 |  |  | 100% |  |  |  |
| `calculate` | other | `expression` | 328 |  | 1% |  | 99% |  |  |
| `cancel_reservation` | **write** | `reservation_id` | 306 | 1% | 54% | 45% |  |  |  |
| `get_flight_status` | read | `date` | 349 |  | 93% | 1% | 7% |  |  |
| `get_flight_status` | read | `flight_number` | 349 | 6% | 91% | 3% |  |  |  |
| `get_reservation_details` | read | `reservation_id` | 2078 | 16% | 79% | 5% |  |  |  |
| `get_user_details` | read | `user_id` | 640 | 78% | 1% | 21% |  |  |  |
| `search_direct_flight` | read | `date` | 991 | 2% | 71% | 1% | 27% |  |  |
| `search_direct_flight` | read | `destination` | 991 | 6% | 65% | 23% | 6% |  | 0% |
| `search_direct_flight` | read | `origin` | 991 | 5% | 57% | 37% | 1% |  | 0% |
| `search_onestop_flight` | read | `date` | 209 | 5% | 58% | 2% | 35% |  |  |
| `search_onestop_flight` | read | `destination` | 209 |  | 67% | 29% | 3% |  |  |
| `search_onestop_flight` | read | `origin` | 209 | 0% | 43% | 56% | 1% |  |  |
| `send_certificate` | **write** | `amount` | 51 | 20% | 12% |  | 20% |  | 49% |
| `send_certificate` | **write** | `user_id` | 51 | 2% |  | 96% | 2% |  |  |
| `transfer_to_human_agents` | other | `summary` | 215 |  |  |  | 100% |  |  |
| `update_reservation_baggages` | **write** | `nonfree_baggages` | 96 |  |  |  |  |  | 100% |
| `update_reservation_baggages` | **write** | `payment_id` | 96 |  | 91% | 8% | 1% |  |  |
| `update_reservation_baggages` | **write** | `reservation_id` | 96 |  | 41% | 59% |  |  |  |
| `update_reservation_baggages` | **write** | `total_baggages` | 96 |  |  |  |  |  | 100% |
| `update_reservation_flights` | **write** | `cabin` | 368 |  | 20% | 80% |  |  |  |
| `update_reservation_flights` | **write** | `flights` | 1736 | 0% | 89% | 7% | 3% |  |  |
| `update_reservation_flights` | **write** | `payment_id` | 368 |  | 93% | 5% | 2% |  |  |
| `update_reservation_flights` | **write** | `reservation_id` | 368 |  | 51% | 49% |  |  |  |
| `update_reservation_passengers` | **write** | `passengers` | 177 |  | 34% | 66% |  |  |  |
| `update_reservation_passengers` | **write** | `reservation_id` | 39 |  | 28% | 72% |  |  |  |

## Reading the numbers

- **Bits/step** is the cross-entropy of the agent's actual next action under the habit: 0 means perfectly predictable. **Top-1** is how often the habit's first choice is what the agent did. Actions are tools plus `respond` (a message to the user).
- **Coverage @ τ** is the share of held-out decisions where the habit's top option has probability ≥ τ; **agree** is how often that option matched the agent there. Agreeing with the agent is not the same as being right: the agent fails some tasks.
- **Macro-tool headroom** is Σ(n − 1) over runs of n consecutive tool-calling LLM turns, as a share of all LLM turns: an upper bound on the turns `plan_*` macro-tools could remove.
- **Argument provenance** looks each argument value up, verbatim and lower-cased, in what the user had said and in earlier tool outputs. *Generated* values appear in neither, so the agent had to produce them. *Short* values (< 3 characters) are not attributed.
- **Code features** are enum-like fields read from JSON tool outputs by code, kept only if they raise the likelihood of *held-out tasks*: a field that merely identifies the task (a user's city) looks predictive on repeated trials and is rejected.
- **Writes after "yes" / any assent** look at the user's most recent message before each write call: the strict proxy needs the word "yes", the lenient one also accepts phrases like "go ahead" or "proceed". The true confirmation rate lies between them; the System-One question "did the user explicitly confirm?" will measure it.
- **Turns saved** (projection) is the share of all LLM turns a `plan_*` flow would remove on held-out episodes: a run of t LLM turns costs min(t, 1 + pauses). The **ceiling** is every run collapsing to one call. A **pause** is a decision inside a run that nobody in the flow may take, or an argument only the LLM can produce.
- **Validated contexts** are those where the habit's top option matched the agent in at least the stated share of at least the stated number of decisions, drawn from at least the stated number of distinct tasks (a task's trials and models are near-copies), under cross-validation grouped by task.
- A **risky decision** is one taken inside the flow that differs from the agent: another tool, carrying on when the agent stopped, or another argument value. Handing back early is counted separately, as safe: it costs one LLM turn. **Episodes with a risky decision** stand in, conservatively, for the pass^1 a flow could lose.
- **Read-only flows** follow RFC-001's plan/commit rule: between LLM turns a flow only calls tools the benchmark marks as reads, and every write (and any tool not marked read-only, such as a transfer or the calculator) goes back to the LLM. A wrong pick is then a **detour**: an extra lookup that changes nothing and costs a pause, not a risk.
- **Tokens and dollars saved** come from the usage the benchmark recorded for each call: removed turns, plus intermediate tool outputs a collapsed run no longer carries in later prompts, priced at the model's effective input price.
- **Phase 0b agreement** is how often the System-One model's pick matched the agent's next step (or argument value) on held-out decisions. **Stop vs. go on** scores only whether it handed back when the agent did. **Brier** is the squared error of its whole distribution (0 is perfect, 2 the worst); **ECE** is the gap between the probability it put on its pick and how often the pick was right, averaged over ten bins.
