# stretto Phase 0 report

Source: τ²-bench's published baseline trajectories (4 trials per task). The habit is a hierarchical Dirichlet back-off model of the agent's next action, learned from the **successful episodes of the official training tasks** and evaluated on the **held-out test tasks**. Coverage uses context length k = 2 and needs at least 5 training observations of a context before the habit may act on it.

## telecom

74 training tasks, 40 held-out tasks. 13 tools: 6 read, 6 write, 1 other.
Back-off concentration α (fugue MH, 600 draws): median 14.145, 90% interval [12.445, 15.850].

### Runs

| Agent model | User simulator | Episodes | Success | LLM turns/ep | Tool calls/ep | Writes/ep | Macro-tool headroom | Writes after "yes" / any assent |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | gpt-4.1 | 456 | 49% | 17.6 | 6.8 | 0.93 | 20% | 35% / 54% |
| gpt-4.1 | gpt-4.1 | 456 | 34% | 17.1 | 6.7 | 0.77 | 21% | 12% / 23% |
| gpt-4.1-mini | gpt-4.1 | 456 | 44% | 29.1 | 18.2 | 0.75 | 42% | 36% / 48% |
| o4-mini | gpt-4.1 | 456 | 42% | 17.7 | 8.9 | 1.72 | 27% | 9% / 18% |
| **all** | | | | | | | **30%** | |

### Lookups inside runs

Shares of all LLM turns. A read-only flow behind the tools can take the first kind; only a macro-tool the LLM names, and passes the value to, could take the second.

| Agent model | Arguments from earlier outputs | Arguments from the conversation |
|---|---|---|
| claude-3-7-sonnet | 17.4% | 0.0% |
| gpt-4.1 | 18.0% | 0.0% |
| gpt-4.1-mini | 40.8% | 0.0% |
| o4-mini | 21.5% | 0.2% |
| **all** | 26.7% | 0.0% |

### Next-action predictability (held-out)

| Agent model | k=0 bits / top-1 | k=1 bits / top-1 | k=2 bits / top-1 | k=3 bits / top-1 |
|---|---|---|---|---|
| claude-3-7-sonnet | 2.09 / 59% | 1.69 / 69% | 1.18 / 78% | 1.16 / 78% |
| gpt-4.1 | 1.93 / 56% | 1.45 / 70% | 1.10 / 78% | 1.10 / 73% |
| gpt-4.1-mini | 2.24 / 41% | 1.84 / 60% | 1.62 / 64% | 1.57 / 63% |
| o4-mini | 2.31 / 51% | 2.03 / 56% | 1.77 / 63% | 1.73 / 64% |
| **all models pooled** | 2.20 / 50% | 1.84 / 63% | 1.55 / 69% | 1.53 / 67% |

### Coverage: decisions the habit could take (k = 2)

| Agent model | τ=0.5 cover / agree | τ=0.7 cover / agree | τ=0.8 cover / agree | τ=0.9 cover / agree | τ=0.95 cover / agree |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 76% / 88% | 70% / 88% | 63% / 90% | 54% / 90% | 5% / 100% |
| gpt-4.1 | 80% / 81% | 66% / 86% | 56% / 89% | 13% / 100% | 7% / 100% |
| gpt-4.1-mini | 59% / 76% | 40% / 83% | 34% / 85% | 10% / 100% | 4% / 99% |
| o4-mini | 63% / 76% | 32% / 89% | 18% / 99% | 16% / 99% | 6% / 99% |
| **all models pooled** | 74% / 77% | 53% / 85% | 50% / 85% | 16% / 99% | 15% / 99% |

### Where the habit can act (all models pooled, k = 2, τ = 0.8)

| Decision made | Share of decisions | Bits/step | Top-1 | Cover / agree at τ |
|---|---|---|---|---|
| right after the user spoke | 50% | 1.30 | 78% | 72% / 84% |
| right after a tool returned | 50% | 1.79 | 61% | 28% / 89% |

### What the habit gains from code features and a named intent (all models pooled, k = 2)

38 candidate fields in tool outputs. Kept, in order, because each raised the log-likelihood of held-out tasks (5-fold cross-validation grouped by task):

| Tool | Field | Values | Gain (nats) |
|---|---|---|---|
| `get_details_by_id` | `status` | 8 | 506.3 |
| `get_details_by_id` | `plan_id` | 3 | 298.8 |
| `get_details_by_id` | `roaming_enabled` | 3 | 94.5 |
| `get_data_usage` | `data_used_gb` | 3 | 49.8 |
| `get_data_usage` | `data_refueling_gb` | 2 | 28.2 |
| `get_details_by_id` | `contract_end_date` | 4 | 14.9 |
| `refuel_data` | `message` | 3 | 5.0 |

The named intent is the set of write tools the episode goes on to call (1 distinct), standing in for what the LLM states when it calls a macro-tool. It is layered on top of the shared model, so rare intents fall back to it.

| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ=0.8 | After the user: cover / agree at τ=0.8 | All: cover / agree at τ=0.8 | at τ=0.9 |
|---|---|---|---|---|---|---|
| the action sequence | 1.55 | 69% | 28% / 89% | 72% / 84% | 50% / 85% | 16% / 99% |
| + code features | 1.43 | 69% | 28% / 94% | 72% / 84% | 50% / 87% | 21% / 98% |
| + code features + named intent | 1.49 | 70% | 28% / 94% | 75% / 84% | 51% / 86% | 21% / 97% |

### Projection: what macro-tools would save on held-out tasks

Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit above (code features + named intent). Where the habit may not act, the decision goes either to the LLM (a pause, one turn) or to a System-One model assumed to agree with the agent (the upper bound Phase 0b will test). Generated arguments always pause. Collapsing every run to one call would save 31.3% of LLM turns (the ceiling).

The habit may act either wherever its top option clears a threshold (with at least 5 training observations), or only in *validated* contexts: those where its top option matched the agent in at least 99% of at least 20 decisions from at least 10 distinct tasks, under 5-fold cross-validation grouped by task. A habit that hands back too early is safe: the LLM takes the step, at the cost of one turn. A habit that picks another tool, or carries on when the agent stopped, is the risk.

| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |
|---|---|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | **4.8%** | 5.46 | 2.22 | 0.00 | 1.4 | 7.2 | 6.1% |
| pause for the LLM | top option ≥ 0.95 | **3.9%** | 5.65 | 1.90 | 0.00 | 0.2 | 4.4 | 3.4% |
| pause for the LLM | in 2 validated contexts | **0.0%** | 6.47 | 0.15 | 0.00 | 0.2 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.9 | **30.3%** | 0.20 | 2.22 | 7.99 | 1.4 | 7.2 | 6.1% |
| ask a perfect System-One model | top option ≥ 0.95 | **30.4%** | 0.17 | 1.90 | 8.32 | 0.2 | 4.4 | 3.4% |
| ask a perfect System-One model | in 2 validated contexts | **30.5%** | 0.17 | 0.15 | 10.06 | 0.2 | 0.0 | 0.0% |

Validated contexts, and how the habit did in them on held-out tasks:

| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |
|---|---|---|---|
| `start` → `respond (user replied)` | get_customer_by_phone | 100.0% of 442 (59 tasks) | 99.7% of 629 |
| `respond (user replied)` → `refuel_data (ok; message=Successfully added 2 GB of data for line L1002 for $4.00)` | respond | 100.0% of 106 (24 tasks) | 98.9% of 94 |

Tokens and dollars come from the usage the benchmark recorded for every LLM call. A removed turn saves its whole prompt and completion. A run that collapses to one call also stops carrying its intermediate tool outputs in every later prompt; their size is read off the recorded prompt growth, and priced at each model's effective input price, fitted to its recorded costs by least squares (so it absorbs any prompt-caching discount): claude-3-7-sonnet $3.00 in / $15.00 out per MTok; gpt-4.1 $0.78 in / $5.39 out per MTok; gpt-4.1-mini $0.15 in / $1.63 out per MTok; o4-mini $0.47 in / $3.91 out per MTok.

| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | 4.8% | 4.6% | 4.5% | **6.0%** | 0.2093 → 0.1969 |
| pause for the LLM | top option ≥ 0.95 | 3.9% | 3.8% | 4.2% | **4.9%** | 0.2093 → 0.1990 |
| pause for the LLM | in 2 validated contexts | 0.0% | 0.0% | 0.0% | **0.0%** | 0.2093 → 0.2093 |
| ask a perfect System-One model | top option ≥ 0.9 | 30.3% | 37.7% | 19.3% | **27.7%** | 0.2093 → 0.1515 |
| ask a perfect System-One model | top option ≥ 0.95 | 30.4% | 37.8% | 19.4% | **27.7%** | 0.2093 → 0.1514 |
| ask a perfect System-One model | in 2 validated contexts | 30.5% | 37.9% | 19.5% | **27.7%** | 0.2093 → 0.1513 |

By agent model, with the habit acting only in validated contexts and a perfect System-One model deciding the rest. *Read-only flows* follow RFC-001's plan/commit rule: between LLM turns a flow only reads, and every write goes back to the LLM. Pooled dollars weigh each model by its spend.

| Agent model | Tool turns with parallel calls | Turns saved | Read-only flows | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 0% | **22.3%** | 19.4% | 22.7% | 26.4% | 15.5% | 25.7% | 0.6002 → 0.4458 |
| gpt-4.1 | 1% | **26.0%** | 22.3% | 26.7% | 32.7% | 8.2% | 31.9% | 0.1131 → 0.0770 |
| gpt-4.1-mini | 8% | **41.6%** | 40.5% | 42.1% | 54.2% | 20.3% | 52.4% | 0.0416 → 0.0198 |
| o4-mini | 0% | **25.8%** | 21.1% | 27.7% | 25.6% | 22.8% | 24.0% | 0.0825 → 0.0627 |

### Phase 0b: the System-One model in shadow mode

Questions v2 (RFC-001 §3.5), for read-only flows. At every held-out decision right after a tool returns, the System-One model was asked which lookup the agent makes next, with the options limited to the lookups the agent made after the same tool in training, or whether it hands back (to write to the customer, or to make a change, which only the LLM does). The same request also asks the stop decision on its own (*split*: "does it make another lookup now?", then which one). The state is a slice: the customer's messages, the agent's last message, the latest four results in full and older lookups one line each. *Combined* answers weigh the System-One answers against the habit's prediction and the model's record at that site on other tasks, fitted by cross-validation over tasks (RFC-001 §3.6). Numeric arguments are not asked about. Oracle: `jev` (answering model: jev-1.13.0). 6520 decisions, 4713 distinct questions, 4713 answered, 0 failed; 11331833 input tokens ($0.48 at $0.042/MTok). 622 next-step decisions came at sites where the agent never looked anything up in training, so the flow hands back without asking; the options held the agent's step in 99.7% of the source models' next-step decisions.

Next step, as a read-only flow takes it (a lookup, or hand back):

| Agent model | Decisions | One question | Split | Combined | Stop vs. go on | ECE | Combined p ≥ 0.9: share / agreed | Combined p ≥ 0.99: share / agreed |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 1172 | 70.3% | 67.4% | 82.1% | 71.9% | 0.074 | 40.8% / 99.6% | 19.2% / 99.1% |
| gpt-4.1 | 1214 | 52.8% | 55.8% | 70.3% | 53.2% | 0.219 | 30.6% / 98.9% | 14.7% / 100.0% |
| gpt-4.1-mini | 2675 | 43.1% | 32.4% | 60.7% | 52.7% | 0.232 | 13.8% / 94.6% | 5.6% / 98.7% |
| o4-mini | 1282 | 66.6% | 68.5% | 67.6% | 70.5% | 0.066 | 33.4% / 94.9% | 15.4% / 98.5% |
| **all source models** | 6343 | 54.8% | 50.6% | 67.9% | 60.0% | 0.158 | 26.0% / 97.1% | 11.9% / 99.1% |

The arbiter's weights, averaged over folds: habit 0.63, one question 0.14, split -0.04, handing back -0.86, predicate list_pending -0.24, predicate needs_options -0.01, predicate must_ask -0.94, the model's record at the site 0.35.

Predicates asked with every next-step question:

- `list_pending`: Is the agent working through a list of records one lookup at a time (orders, items, products, reservations, flights or dates), with some it has not looked up yet?
- `needs_options`: Does the customer's request need options the agent has not looked up yet, such as another version of an item, other products, or flights on a date or route not yet searched?
- `must_ask`: Does the agent need something only the customer can give before any further lookup would help, such as identification, a choice between options, or a confirmation?

Closed-set arguments:

| Agent model | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|
| claude-3-7-sonnet | 0 | 0.0% | – |
| gpt-4.1 | 0 | 0.0% | – |
| gpt-4.1-mini | 91 | 27.5% | 0.0% / 0.0% |
| o4-mini | 1 | 0.0% | 100.0% / 0.0% |
| **all source models** | 92 | 27.2% | 1.1% / 0.0% |

Closed-set arguments by argument (source models):

| Tool | Argument | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|---|
| `get_customer_by_name` | `dob` | 91 | 27.5% | 0.0% / 0.0% |
| `get_customer_by_phone` | `phone_number` | 1 | 0.0% | 100.0% / 0.0% |

Projection with the System-One model, pooled over the source models, for read-only flows: the habit acts in validated contexts, a System-One pick is trusted at or above the threshold (*two keys*: only when it is also the habit's top option; *combined*: the arbiter's probability), and anything else pauses. A write always goes back to the LLM. Nothing a read-only flow does changes the environment, so its wrong picks are *detours*, extra lookups that cost a pause, not risks. Handing back costs a turn and a detour does not, so *lookup first* takes the likeliest lookup whenever it clears the (lower) threshold. Each detour is charged a typical lookup's output (102 tokens) in every later prompt, so the dollars saved include the detours' cost.

| Pick trusted at | Turns saved | $ saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour | Writes handed back/ep |
|---|---|---|---|---|---|---|---|---|
| p ≥ 0.5 | **8.2%** | 7.8% | 4.63 | 8.04 | 271.6 | 102.8 | 59.5% | 0.71 |
| p ≥ 0.7 | **3.9%** | 2.9% | 5.65 | 4.24 | 110.6 | 39.5 | 29.4% | 0.71 |
| p ≥ 0.9 | **0.9%** | 0.6% | 6.28 | 1.72 | 7.8 | 5.6 | 4.1% | 0.71 |
| p ≥ 0.95 | **0.1%** | 0.0% | 6.45 | 1.28 | 3.6 | 2.2 | 1.7% | 0.71 |
| p ≥ 0.99 | **0.0%** | 0.0% | 6.46 | 0.99 | 1.6 | 0.6 | 0.6% | 0.71 |
| two keys, p ≥ 0.5 | **7.5%** | 7.4% | 4.77 | 6.04 | 138.6 | 84.5 | 53.6% | 0.71 |
| two keys, p ≥ 0.7 | **3.5%** | 2.8% | 5.72 | 3.64 | 71.2 | 36.9 | 28.1% | 0.71 |
| two keys, p ≥ 0.9 | **0.9%** | 0.6% | 6.28 | 1.70 | 6.9 | 5.0 | 3.6% | 0.71 |
| two keys, p ≥ 0.95 | **0.1%** | 0.0% | 6.45 | 1.27 | 3.3 | 1.7 | 1.2% | 0.71 |
| two keys, p ≥ 0.99 | **0.0%** | 0.0% | 6.46 | 0.99 | 1.6 | 0.3 | 0.3% | 0.71 |
| combined, p ≥ 0.5 | **13.4%** | 11.1% | 3.58 | 6.54 | 41.4 | 134.2 | 69.2% | 0.71 |
| combined, p ≥ 0.7 | **5.2%** | 4.7% | 5.25 | 3.16 | 14.2 | 17.3 | 13.8% | 0.71 |
| combined, p ≥ 0.9 | **3.0%** | 3.3% | 5.80 | 2.12 | 3.0 | 4.1 | 3.4% | 0.71 |
| combined, p ≥ 0.95 | **0.3%** | 0.1% | 6.39 | 1.23 | 1.6 | 1.4 | 1.1% | 0.71 |
| combined, p ≥ 0.99 | **0.0%** | -0.0% | 6.47 | 0.95 | 1.1 | 0.2 | 0.2% | 0.71 |
| lookup first, p ≥ 0.2 | **20.7%** | 20.2% | 2.13 | 8.32 | 1.1 | 297.2 | 88.4% | 0.71 |
| lookup first, p ≥ 0.3 | **20.3%** | 19.8% | 2.21 | 7.95 | 1.1 | 268.8 | 83.4% | 0.71 |
| lookup first, p ≥ 0.4 | **17.3%** | 14.7% | 2.79 | 6.79 | 1.1 | 211.6 | 78.3% | 0.71 |

The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes holding a risky decision (a conservative stand-in for losing at most one point of pass^1). Read-only flows take no risky decisions, so offline the gate is the turns saved; each cell is turns saved · episodes with a detour. Whether detours cost pass^1 is for a live run to show.

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | lookup first, p ≥ 0.3 | Gate |
|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 19.4% | 9.4% · 43.1% | 3.2% · 16.2% | 0.6% · 1.2% | 12.2% · 52.5% | 5.4% · 1.2% | 4.2% · 0.0% | 17.4% · 69.4% | fails: below 20% even with a perfect System-One model |
| gpt-4.1 | 22.3% | 5.8% · 63.1% | 2.2% · 18.8% | 0.0% · 0.0% | 9.4% · 66.9% | 4.9% · 4.4% | 2.2% · 1.9% | 19.0% · 89.4% | fails |
| gpt-4.1-mini | 40.5% | 8.5% · 86.2% | 5.0% · 60.0% | 1.6% · 10.0% | 17.1% · 88.8% | 4.7% · 28.7% | 1.7% · 5.6% | 25.9% · 92.5% | **passes offline** (lookup first, p ≥ 0.3; detours in 92% of episodes) |
| o4-mini | 21.1% | 8.8% · 45.6% | 4.8% · 22.5% | 1.2% · 5.0% | 12.8% · 68.8% | 6.1% · 20.6% | 4.8% · 6.2% | 15.4% · 82.5% | fails |

### Transfer: top-1 when the habit comes from another model (k = 2)

| habit from ↓ / tested on → | claude-3-7-sonnet | gpt-4.1 | gpt-4.1-mini | o4-mini |
|---|---|---|---|---|
| claude-3-7-sonnet | 78% | 74% | 61% | 62% |
| gpt-4.1 | 71% | 78% | 54% | 59% |
| gpt-4.1-mini | 74% | 72% | 64% | 63% |
| o4-mini | 78% | 76% | 62% | 63% |

### Most common tool runs (successful training episodes, all models)

| Count | Run |
|---|---|
| 168 | `get_customer_by_phone` |
| 135 | `refuel_data` |
| 100 | `get_details_by_id` |
| 85 | `transfer_to_human_agents` |
| 76 | `get_customer_by_phone → get_details_by_id → get_details_by_id` |
| 70 | `get_details_by_id → resume_line` |
| 67 | `send_payment_request` |
| 57 | `enable_roaming` |
| 44 | `get_customer_by_phone → get_details_by_id → get_details_by_id → get_details_by_id` |
| 44 | `get_details_by_id → get_details_by_id → get_details_by_id` |
| 38 | `get_details_by_id → get_details_by_id` |
| 33 | `get_data_usage` |

### Argument provenance (all episodes, all models)

| Tool | Kind | Argument | Values | User | Tool output | Both | Generated | Literal | Short |
|---|---|---|---|---|---|---|---|---|---|
| `disable_roaming` | **write** | `customer_id` | 71 |  | 100% |  |  |  |  |
| `disable_roaming` | **write** | `line_id` | 71 |  | 100% |  |  |  |  |
| `enable_roaming` | **write** | `customer_id` | 318 |  | 99% |  | 1% |  |  |
| `enable_roaming` | **write** | `line_id` | 318 |  | 99% |  | 1% |  |  |
| `get_bills_for_customer` | read | `customer_id` | 531 |  | 100% |  |  |  |  |
| `get_bills_for_customer` | read | `limit` | 215 |  |  |  |  |  | 100% |
| `get_customer_by_id` | read | `customer_id` | 160 |  | 100% |  |  |  |  |
| `get_customer_by_id` | read | `id` | 3 |  | 100% |  |  |  |  |
| `get_customer_by_name` | read | `dob` | 286 | 0% | 6% |  | 72% |  | 21% |
| `get_customer_by_name` | read | `full_name` | 286 | 94% | 3% | 3% |  |  |  |
| `get_customer_by_phone` | read | `customer_id` | 4 |  |  |  |  |  | 100% |
| `get_customer_by_phone` | read | `phone_number` | 4310 | 43% | 2% | 55% | 0% |  | 0% |
| `get_data_usage` | read | `customer_id` | 743 |  | 100% |  |  |  |  |
| `get_data_usage` | read | `line_id` | 743 |  | 100% |  |  |  |  |
| `get_details_by_id` | read | `app_name` | 2 | 50% |  | 50% |  |  |  |
| `get_details_by_id` | read | `arguments` | 2 | 100% |  |  |  |  |  |
| `get_details_by_id` | read | `id` | 9300 | 0% | 99% | 0% | 1% |  | 0% |
| `get_details_by_id` | read | `line_id` | 1 |  | 100% |  |  |  |  |
| `get_details_by_id` | read | `permission` | 2 | 100% |  |  |  |  |  |
| `refuel_data` | **write** | `customer_id` | 538 |  | 100% |  |  |  |  |
| `refuel_data` | **write** | `gb_amount` | 538 | 1% | 2% | 14% | 1% | 0% | 82% |
| `refuel_data` | **write** | `line_id` | 538 |  | 100% |  |  |  |  |
| `resume_line` | **write** | `customer_id` | 311 |  | 100% |  |  |  |  |
| `resume_line` | **write** | `line_id` | 311 |  | 100% |  |  |  |  |
| `send_payment_request` | **write** | `bill_id` | 642 |  | 98% | 0% | 2% |  | 0% |
| `send_payment_request` | **write** | `client_id` | 1 |  | 100% |  |  |  |  |
| `send_payment_request` | **write** | `customer_id` | 641 |  | 99% |  | 0% |  | 0% |
| `send_payment_request` | **write** | `error` | 1 |  |  |  | 100% |  |  |
| `set_network_mode_preference` | ? | `mode` | 4 |  |  |  | 100% |  |  |
| `suspend_line` | **write** | `customer_id` | 8 |  | 100% |  |  |  |  |
| `suspend_line` | **write** | `line_id` | 8 |  | 100% |  |  |  |  |
| `suspend_line` | **write** | `reason` | 7 |  |  |  | 100% |  |  |
| `toggle_data` | ? | `enabled` | 1 |  |  |  | 100% |  |  |
| `toggle_data_saver_mode` | ? | `enabled` | 1 |  | 100% |  |  |  |  |
| `transfer_to_human_agents` | other | `summary` | 1154 |  |  |  | 100% |  |  |

## Reading the numbers

- **Bits/step** is the cross-entropy of the agent's actual next action under the habit: 0 means perfectly predictable. **Top-1** is how often the habit's first choice is what the agent did. Actions are tools plus `respond` (a message to the user).
- **Coverage @ τ** is the share of held-out decisions where the habit's top option has probability ≥ τ; **agree** is how often that option matched the agent there. Agreeing with the agent is not the same as being right: the agent fails some tasks.
- **Macro-tool headroom** is Σ(n − 1) over runs of n consecutive tool-calling LLM turns, as a share of all LLM turns: an upper bound on the turns `plan_*` macro-tools could remove.
- **Lookups inside runs** are LLM turns that only look something up and follow another tool call with no message in between. When every argument value appeared in an earlier tool output (or is a literal or too short to trace), a read-only flow behind the tools can bind it (arm D0). When some value only the user said, or the agent produced (a date, an airport), a flow cannot, and a named macro-tool could take the turn only if the LLM passed the value: that share bounds what naming a flow adds.
- **Argument provenance** looks each argument value up, verbatim and lower-cased, in what the user had said and in earlier tool outputs. *Generated* values appear in neither, so the agent had to produce them. *Short* values (< 3 characters) are not attributed.
- **Code features** are enum-like fields read from JSON tool outputs by code, kept only if they raise the likelihood of *held-out tasks*: a field that merely identifies the task (a user's city) looks predictive on repeated trials and is rejected.
- **Writes after "yes" / any assent** look at the user's most recent message before each write call: the strict proxy needs the word "yes", the lenient one also accepts phrases like "go ahead" or "proceed". The true confirmation rate lies between them; the System-One question "did the user explicitly confirm?" will measure it.
- **Turns saved** (projection) is the share of all LLM turns a `plan_*` flow would remove on held-out episodes: a run of t LLM turns costs min(t, 1 + pauses). The **ceiling** is every run collapsing to one call. A **pause** is a decision inside a run that nobody in the flow may take, or an argument only the LLM can produce.
- **Validated contexts** are those where the habit's top option matched the agent in at least the stated share of at least the stated number of decisions, drawn from at least the stated number of distinct tasks (a task's trials and models are near-copies), under cross-validation grouped by task.
- A **risky decision** is one taken inside the flow that differs from the agent: another tool, carrying on when the agent stopped, or another argument value. Handing back early is counted separately, as safe: it costs one LLM turn. **Episodes with a risky decision** stand in, conservatively, for the pass^1 a flow could lose.
- **Read-only flows** follow RFC-001's plan/commit rule: between LLM turns a flow only calls tools the benchmark marks as reads, and every write (and any tool not marked read-only, such as a transfer or the calculator) goes back to the LLM. A wrong pick is then a **detour**: an extra lookup that changes nothing and costs a pause, not a risk.
- **Tokens and dollars saved** come from the usage the benchmark recorded for each call: removed turns, plus intermediate tool outputs a collapsed run no longer carries in later prompts, priced at the model's effective input price.
- **Phase 0b agreement** is how often the System-One model's pick matched the agent's next step (or argument value) on held-out decisions. **Stop vs. go on** scores only whether it handed back when the agent did. **Brier** is the squared error of its whole distribution (0 is perfect, 2 the worst); **ECE** is the gap between the probability it put on its pick and how often the pick was right, averaged over ten bins.
