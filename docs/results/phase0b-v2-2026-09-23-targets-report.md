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
| qwen3.5 *(target)* | gpt-5.2 | 456 | 84% | 11.3 | 7.4 | 1.49 | 44% | 98% / 99% |
| qwen3-max *(target)* | gpt-4.1 | 456 | 75% | 12.6 | 7.4 | 1.46 | 38% | 92% / 98% |
| gpt-5.2 *(target)* | gpt-5.2 | 456 | 82% | 9.6 | 7.8 | 1.46 | 30% | 99% / 100% |
| gpt-5.2-none *(target)* | gpt-5.2 | 456 | 68% | 8.6 | 6.7 | 1.18 | 32% | 98% / 98% |
| claude-opus-4.5 *(target)* | gpt-5.2 | 456 | 80% | 8.9 | 7.6 | 1.36 | 28% | 98% / 99% |
| claude-sonnet-4.5 *(target)* | gpt-5.2 | 456 | 72% | 10.4 | 7.1 | 1.26 | 31% | 97% / 99% |
| gemini-3-pro *(target)* | gpt-5.2 | 456 | 76% | 9.9 | 7.3 | 1.38 | 39% | 95% / 99% |
| gemini-3-flash *(target)* | gpt-5.2 | 456 | 77% | 10.1 | 7.7 | 1.36 | 42% | 96% / 100% |
| **all** | | | | | | | **24%** | |

### Next-action predictability (held-out)

| Agent model | k=0 bits / top-1 | k=1 bits / top-1 | k=2 bits / top-1 | k=3 bits / top-1 |
|---|---|---|---|---|
| claude-3-7-sonnet | 2.65 / 44% | 1.86 / 60% | 1.70 / 63% | 1.62 / 66% |
| gpt-4.1 | 2.67 / 44% | 1.98 / 57% | 1.76 / 61% | 1.73 / 62% |
| gpt-4.1-mini | 2.63 / 46% | 2.11 / 53% | 1.68 / 64% | 1.69 / 67% |
| o4-mini | 2.57 / 48% | 2.09 / 49% | 1.76 / 58% | 1.70 / 59% |
| glm-5 *(target)* | 2.82 / 35% | 1.75 / 63% | 1.62 / 67% | 1.63 / 66% |
| qwen3.5 *(target)* | 2.89 / 34% | 1.81 / 60% | 1.63 / 64% | 1.62 / 66% |
| qwen3-max *(target)* | 2.72 / 41% | 1.81 / 62% | 1.62 / 65% | 1.55 / 67% |
| gpt-5.2 *(target)* | 2.98 / 34% | 2.00 / 57% | 1.74 / 63% | 1.79 / 63% |
| gpt-5.2-none *(target)* | 2.90 / 34% | 1.93 / 58% | 1.67 / 63% | 1.71 / 64% |
| claude-opus-4.5 *(target)* | 2.87 / 34% | 1.75 / 62% | 1.57 / 67% | 1.57 / 69% |
| claude-sonnet-4.5 *(target)* | 2.80 / 39% | 1.92 / 59% | 1.72 / 64% | 1.71 / 63% |
| gemini-3-pro *(target)* | 2.99 / 34% | 1.82 / 61% | 1.64 / 65% | 1.63 / 66% |
| gemini-3-flash *(target)* | 2.97 / 32% | 1.79 / 62% | 1.63 / 65% | 1.65 / 65% |
| **all models pooled** | 2.64 / 46% | 2.06 / 54% | 1.80 / 59% | 1.76 / 60% |

### Coverage: decisions the habit could take (k = 2)

| Agent model | τ=0.5 cover / agree | τ=0.7 cover / agree | τ=0.8 cover / agree | τ=0.9 cover / agree | τ=0.95 cover / agree |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 67% / 74% | 36% / 86% | 18% / 94% | 9% / 98% | 8% / 99% |
| gpt-4.1 | 63% / 70% | 35% / 80% | 20% / 89% | 15% / 90% | 8% / 95% |
| gpt-4.1-mini | 71% / 72% | 37% / 77% | 12% / 84% | 10% / 84% | 8% / 91% |
| o4-mini | 57% / 69% | 20% / 88% | 18% / 88% | 12% / 89% | 8% / 93% |
| glm-5 *(target)* | 73% / 75% | 40% / 84% | 34% / 87% | 17% / 89% | 8% / 99% |
| qwen3.5 *(target)* | 71% / 74% | 43% / 82% | 28% / 86% | 1% / 96% | 1% / 100% |
| qwen3-max *(target)* | 77% / 73% | 34% / 90% | 28% / 93% | 18% / 95% | 16% / 96% |
| gpt-5.2 *(target)* | 74% / 73% | 40% / 80% | 28% / 80% | 15% / 86% | 1% / 100% |
| gpt-5.2-none *(target)* | 73% / 74% | 39% / 79% | 29% / 79% | 7% / 100% | 1% / 100% |
| claude-opus-4.5 *(target)* | 77% / 76% | 42% / 85% | 36% / 86% | 15% / 96% | 7% / 95% |
| claude-sonnet-4.5 *(target)* | 67% / 68% | 32% / 79% | 17% / 85% | 1% / 85% | 0% / 0% |
| gemini-3-pro *(target)* | 71% / 76% | 43% / 85% | 31% / 87% | 10% / 99% | 9% / 99% |
| gemini-3-flash *(target)* | 74% / 74% | 42% / 84% | 19% / 97% | 10% / 99% | 2% / 100% |
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

The named intent is the set of write tools the episode goes on to call (37 distinct), standing in for what the LLM states when it calls a macro-tool. It is layered on top of the shared model, so rare intents fall back to it.

| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ=0.8 | After the user: cover / agree at τ=0.8 | All: cover / agree at τ=0.8 | at τ=0.9 |
|---|---|---|---|---|---|---|
| the action sequence | 1.80 | 59% | 8% / 74% | 23% / 92% | 15% / 87% | 8% / 96% |
| + code features | 1.81 | 59% | 14% / 80% | 24% / 91% | 18% / 86% | 11% / 91% |
| + code features + named intent | 1.71 | 59% | 19% / 76% | 29% / 87% | 24% / 82% | 15% / 91% |

### Projection: what macro-tools would save on held-out tasks

Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit above (code features + named intent). Where the habit may not act, the decision goes either to the LLM (a pause, one turn) or to a System-One model assumed to agree with the agent (the upper bound Phase 0b will test). Generated arguments always pause. Collapsing every run to one call would save 22.9% of LLM turns (the ceiling).

The habit may act either wherever its top option clears a threshold (with at least 5 training observations), or only in *validated* contexts: those where its top option matched the agent in at least 99% of at least 20 decisions from at least 10 distinct tasks, under 5-fold cross-validation grouped by task. A habit that hands back too early is safe: the LLM takes the step, at the cost of one turn. A habit that picks another tool, or carries on when the agent stopped, is the risk.

| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |
|---|---|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | **1.2%** | 3.71 | 0.74 | 0.00 | 9.1 | 2.7 | 2.7% |
| pause for the LLM | top option ≥ 0.95 | **1.0%** | 3.75 | 0.47 | 0.00 | 5.6 | 1.9 | 1.9% |
| pause for the LLM | in 1 validated context | **0.0%** | 3.88 | 0.00 | 0.00 | 0.0 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.9 | **21.7%** | 0.18 | 0.74 | 6.70 | 9.1 | 2.7 | 2.7% |
| ask a perfect System-One model | top option ≥ 0.95 | **21.9%** | 0.14 | 0.47 | 6.98 | 5.6 | 1.9 | 1.9% |
| ask a perfect System-One model | in 1 validated context | **22.3%** | 0.08 | 0.00 | 7.45 | 0.0 | 0.0 | 0.0% |

Validated contexts, and how the habit did in them on held-out tasks:

| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |
|---|---|---|---|
| `start` → `start` | respond | 99.0% of 831 (74 tasks) | 95.5% of 640 |

Tokens and dollars come from the usage the benchmark recorded for every LLM call. A removed turn saves its whole prompt and completion. A run that collapses to one call also stops carrying its intermediate tool outputs in every later prompt; their size is read off the recorded prompt growth, and priced at each model's effective input price, fitted to its recorded costs by least squares (so it absorbs any prompt-caching discount): claude-3-7-sonnet $3.00 in / $15.00 out per MTok; claude-opus-4.5 $5.00 in / $25.00 out per MTok; claude-sonnet-4.5 $3.00 in / $15.00 out per MTok; gemini-3-flash $0.19 in / $3.28 out per MTok; gemini-3-pro $0.92 in / $12.38 out per MTok; gpt-4.1 $0.92 in / $11.18 out per MTok; gpt-4.1-mini $0.21 in / $1.18 out per MTok; gpt-5.2 $0.66 in / $14.57 out per MTok; gpt-5.2-none $0.72 in / $14.59 out per MTok; o4-mini $0.60 in / $4.14 out per MTok.

| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | 1.2% | 1.2% | 0.6% | **1.2%** | 0.1123 → 0.1110 |
| pause for the LLM | top option ≥ 0.95 | 1.0% | 0.9% | 0.6% | **0.9%** | 0.1123 → 0.1113 |
| pause for the LLM | in 1 validated context | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1123 → 0.1123 |
| ask a perfect System-One model | top option ≥ 0.9 | 21.7% | 25.7% | 18.7% | **31.7%** | 0.1123 → 0.0768 |
| ask a perfect System-One model | top option ≥ 0.95 | 21.9% | 26.0% | 18.8% | **31.9%** | 0.1123 → 0.0765 |
| ask a perfect System-One model | in 1 validated context | 22.3% | 26.5% | 19.1% | **32.4%** | 0.1123 → 0.0759 |

By agent model, with the habit acting only in validated contexts and a perfect System-One model deciding the rest. *Read-only flows* follow RFC-001's plan/commit rule: between LLM turns a flow only reads, and every write goes back to the LLM. Pooled dollars weigh each model by its spend. Models marked *(target)* were never trained on: the habit, its features, the closed argument sets and the validated contexts all come from the other models, and the pooled tables above leave them out.

| Agent model | Tool turns with parallel calls | Turns saved | Read-only flows | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 0% | **33.0%** | 31.3% | 33.6% | 38.4% | 25.4% | 37.4% | 0.3247 → 0.2032 |
| gpt-4.1 | 20% | **19.7%** | 19.0% | 20.0% | 20.5% | 7.8% | 17.1% | 0.0579 → 0.0480 |
| gpt-4.1-mini | 20% | **12.4%** | 12.0% | 13.3% | 15.0% | 7.2% | 13.3% | 0.0134 → 0.0117 |
| o4-mini | 0% | **22.8%** | 20.5% | 23.6% | 24.9% | 21.0% | 23.4% | 0.0532 → 0.0407 |
| claude-opus-4.5 *(target)* | 28% | **28.6%** | 27.3% | 30.1% | 29.6% | 18.9% | 28.1% | 0.3799 → 0.2731 |
| claude-sonnet-4.5 *(target)* | 11% | **32.1%** | 29.3% | 33.7% | 34.9% | 25.6% | 34.0% | 0.2486 → 0.1640 |
| gemini-3-flash *(target)* | 8% | **41.0%** | 36.8% | 42.7% | 42.3% | 20.4% | 29.9% | 0.0193 → 0.0135 |
| gemini-3-pro *(target)* | 11% | **37.8%** | 34.0% | 39.5% | 38.1% | 22.0% | 28.7% | 0.0884 → 0.0630 |
| glm-5 *(target)* | 27% | **29.1%** | 28.2% | 30.0% | 29.5% | 18.8% | – | no cost recorded |
| gpt-5.2 *(target)* | 19% | **28.6%** | 25.6% | 30.6% | 30.8% | 10.6% | 18.5% | 0.0689 → 0.0562 |
| gpt-5.2-none *(target)* | 15% | **32.2%** | 29.7% | 34.9% | 35.5% | 10.7% | 23.9% | 0.0523 → 0.0398 |
| qwen3-max *(target)* | 0% | **35.7%** | 34.1% | 36.6% | – | – | – | no usage recorded |
| qwen3.5 *(target)* | 0% | **43.5%** | 40.7% | 44.8% | 45.8% | 20.2% | – | no cost recorded |

### Phase 0b: the System-One model in shadow mode

Questions v2 (RFC-001 §3.5), for read-only flows. At every held-out decision right after a tool returns, the System-One model was asked which lookup the agent makes next, with the options limited to the lookups the agent made after the same tool in training, or whether it hands back (to write to the customer, or to make a change, which only the LLM does). The same request also asks the stop decision on its own (*split*: "does it make another lookup now?", then which one). The state is a slice: the customer's messages, the agent's last message, the latest four results in full and older lookups one line each. *Combined* answers weigh the System-One answers against the habit's prediction and the model's record at that site on other tasks, fitted by cross-validation over tasks (RFC-001 §3.6). Numeric arguments are not asked about. Oracle: `replay` (answering model: jev-1.13.0). 14993 decisions, 14102 distinct questions, 14102 answered, 0 failed; 34021822 input tokens ($1.43 at $0.042/MTok). 724 next-step decisions came at sites where the agent never looked anything up in training, so the flow hands back without asking; the options held the agent's step in 99.1% of the source models' next-step decisions.

Next step, as a read-only flow takes it (a lookup, or hand back):

| Agent model | Decisions | One question | Split | Combined | Stop vs. go on | ECE | Combined p ≥ 0.9: share / agreed | Combined p ≥ 0.99: share / agreed |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 1211 | 78.9% | 73.7% | 82.1% | 79.9% | 0.058 | 24.4% / 96.6% | 5.9% / 95.8% |
| gpt-4.1 | 1173 | 76.5% | 71.7% | 79.8% | 78.3% | 0.066 | 25.8% / 97.4% | 6.1% / 100.0% |
| gpt-4.1-mini | 1252 | 71.5% | 73.6% | 73.3% | 73.0% | 0.106 | 28.7% / 97.2% | 5.5% / 100.0% |
| o4-mini | 1060 | 78.3% | 76.4% | 76.6% | 79.2% | 0.063 | 27.9% / 94.9% | 7.7% / 100.0% |
| claude-opus-4.5 *(target)* | 1192 | 78.4% | 72.1% | 82.5% | 81.2% | 0.025 | 26.3% / 98.4% | 6.1% / 100.0% |
| claude-sonnet-4.5 *(target)* | 1092 | 81.5% | 71.2% | 83.1% | 82.5% | 0.057 | 29.3% / 98.4% | 5.7% / 100.0% |
| gemini-3-flash *(target)* | 1196 | 78.5% | 72.6% | 81.3% | 80.1% | 0.048 | 27.6% / 91.2% | 6.9% / 86.6% |
| gemini-3-pro *(target)* | 1143 | 81.5% | 75.4% | 84.2% | 82.7% | 0.051 | 31.1% / 93.5% | 9.3% / 86.8% |
| glm-5 *(target)* | 1153 | 77.9% | 68.8% | 81.7% | 80.5% | 0.038 | 24.9% / 97.9% | 5.4% / 100.0% |
| gpt-5.2 *(target)* | 1214 | 78.2% | 75.8% | 80.2% | 79.5% | 0.057 | 31.6% / 92.2% | 9.9% / 85.0% |
| gpt-5.2-none *(target)* | 1057 | 77.1% | 75.5% | 81.0% | 78.9% | 0.060 | 31.4% / 96.1% | 7.8% / 97.6% |
| qwen3-max *(target)* | 1115 | 77.6% | 70.1% | 83.0% | 79.9% | 0.048 | 22.0% / 95.5% | 6.9% / 98.7% |
| qwen3.5 *(target)* | 1132 | 79.2% | 70.8% | 82.9% | 81.0% | 0.064 | 26.2% / 96.6% | 5.6% / 98.4% |
| **all source models** | 4696 | 76.2% | 73.8% | 77.9% | 77.5% | 0.067 | 26.7% / 96.6% | 6.2% / 99.0% |

The arbiter's weights, averaged over folds: habit 0.38, one question 0.15, split 0.54, handing back -0.41, the model's record at the site 0.26.

Projection with the System-One model, pooled over the source models, for read-only flows: the habit acts in validated contexts, a System-One pick is trusted at or above the threshold (*two keys*: only when it is also the habit's top option; *combined*: the arbiter's probability), and anything else pauses. A write always goes back to the LLM. Nothing a read-only flow does changes the environment, so its wrong picks are *detours*, extra lookups that cost a pause, not risks. Handing back costs a turn and a detour does not, so *lookup first* takes the likeliest lookup whenever it clears the (lower) threshold. Each detour is charged a typical lookup's output (251 tokens) in every later prompt, so the dollars saved include the detours' cost.

| Pick trusted at | Turns saved | $ saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour | Writes handed back/ep |
|---|---|---|---|---|---|---|---|---|
| p ≥ 0.5 | **14.0%** | 16.2% | 1.54 | 6.85 | 96.1 | 67.0 | 53.9% | 0.34 |
| p ≥ 0.7 | **10.6%** | 12.4% | 2.08 | 5.45 | 59.4 | 39.1 | 34.8% | 0.34 |
| p ≥ 0.9 | **3.5%** | 3.4% | 3.26 | 3.12 | 21.7 | 8.1 | 8.0% | 0.34 |
| p ≥ 0.95 | **1.7%** | 1.5% | 3.59 | 2.23 | 10.5 | 1.9 | 1.9% | 0.34 |
| p ≥ 0.99 | **0.5%** | 0.3% | 3.81 | 1.13 | 2.7 | 0.3 | 0.3% | 0.34 |
| two keys, p ≥ 0.5 | **11.3%** | 12.4% | 1.98 | 4.92 | 39.8 | 49.8 | 44.4% | 0.34 |
| two keys, p ≥ 0.7 | **8.8%** | 9.8% | 2.41 | 4.04 | 24.8 | 31.7 | 29.5% | 0.34 |
| two keys, p ≥ 0.9 | **2.8%** | 2.9% | 3.38 | 2.41 | 8.3 | 6.9 | 6.9% | 0.34 |
| two keys, p ≥ 0.95 | **1.4%** | 1.2% | 3.64 | 1.84 | 4.4 | 1.7 | 1.7% | 0.34 |
| two keys, p ≥ 0.99 | **0.5%** | 0.3% | 3.81 | 1.01 | 1.9 | 0.3 | 0.3% | 0.34 |
| combined, p ≥ 0.5 | **15.0%** | 18.1% | 1.37 | 6.74 | 73.8 | 73.0 | 56.9% | 0.34 |
| combined, p ≥ 0.7 | **11.6%** | 14.1% | 1.87 | 4.95 | 34.2 | 40.2 | 35.2% | 0.34 |
| combined, p ≥ 0.9 | **2.5%** | 2.3% | 3.41 | 1.88 | 4.5 | 2.2 | 2.2% | 0.34 |
| combined, p ≥ 0.95 | **0.6%** | 0.5% | 3.79 | 1.02 | 1.1 | 0.5 | 0.5% | 0.34 |
| combined, p ≥ 0.99 | **0.1%** | 0.0% | 3.87 | 0.44 | 0.5 | 0.0 | 0.0% | 0.34 |
| lookup first, p ≥ 0.2 | **18.1%** | 22.1% | 0.87 | 4.86 | 0.0 | 152.7 | 78.8% | 0.34 |
| lookup first, p ≥ 0.3 | **17.2%** | 20.8% | 1.02 | 4.32 | 0.0 | 113.6 | 69.4% | 0.34 |
| lookup first, p ≥ 0.4 | **16.2%** | 19.3% | 1.20 | 3.88 | 0.0 | 88.0 | 62.2% | 0.34 |

The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes holding a risky decision (a conservative stand-in for losing at most one point of pass^1). Read-only flows take no risky decisions, so offline the gate is the turns saved; each cell is turns saved · episodes with a detour. Whether detours cost pass^1 is for a live run to show.

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | lookup first, p ≥ 0.3 | Gate |
|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 31.3% | 20.2% · 18.8% | 15.6% · 12.5% | 4.1% · 1.9% | 22.4% · 21.9% | 17.2% · 11.9% | 2.5% · 0.0% | 25.8% · 50.6% | **passes offline** (p ≥ 0.5; detours in 19% of episodes) |
| gpt-4.1 | 19.0% | 12.7% · 55.0% | 8.7% · 35.0% | 3.2% · 6.2% | 14.0% · 57.5% | 9.3% · 34.4% | 2.4% · 0.6% | 15.5% · 67.5% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 12.0% | 6.8% · 76.2% | 5.1% · 37.5% | 1.8% · 12.5% | 6.9% · 75.0% | 5.7% · 42.5% | 1.7% · 1.2% | 9.3% · 80.6% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.5% | 15.5% · 65.6% | 12.4% · 54.4% | 4.6% · 11.2% | 16.0% · 73.1% | 13.1% · 51.9% | 3.3% · 6.9% | 17.3% · 78.8% | fails |
| claude-opus-4.5 *(target)* | 27.3% | 11.5% · 13.1% | 6.3% · 3.1% | 2.3% · 1.2% | 14.7% · 13.8% | 7.3% · 4.4% | 1.6% · 1.2% | 19.5% · 35.0% | fails |
| claude-sonnet-4.5 *(target)* | 29.3% | 19.6% · 25.0% | 12.9% · 11.9% | 4.5% · 1.9% | 21.1% · 25.0% | 14.0% · 13.8% | 4.6% · 0.6% | 23.8% · 46.9% | **passes offline** (combined, p ≥ 0.5; detours in 25% of episodes) |
| gemini-3-flash *(target)* | 36.8% | 21.2% · 11.2% | 14.9% · 1.9% | 4.8% · 1.2% | 24.1% · 12.5% | 16.7% · 3.8% | 4.1% · 1.2% | 28.4% · 34.4% | **passes offline** (p ≥ 0.5; detours in 11% of episodes) |
| gemini-3-pro *(target)* | 34.0% | 20.7% · 8.8% | 14.2% · 1.9% | 4.9% · 0.0% | 23.1% · 11.2% | 16.9% · 0.0% | 3.9% · 0.0% | 27.1% · 36.2% | **passes offline** (p ≥ 0.5; detours in 9% of episodes) |
| glm-5 *(target)* | 28.2% | 11.6% · 11.9% | 6.5% · 1.2% | 2.6% · 0.0% | 14.7% · 12.5% | 7.9% · 4.4% | 2.5% · 0.0% | 19.4% · 36.2% | fails |
| gpt-5.2 *(target)* | 25.6% | 12.3% · 25.0% | 8.6% · 11.9% | 3.7% · 3.1% | 13.9% · 29.4% | 10.2% · 12.5% | 2.5% · 1.9% | 18.5% · 46.9% | fails |
| gpt-5.2-none *(target)* | 29.7% | 15.8% · 28.7% | 11.5% · 12.5% | 5.0% · 3.8% | 18.1% · 21.9% | 13.7% · 11.9% | 4.2% · 0.0% | 22.8% · 38.1% | **passes offline** (lookup first, p ≥ 0.3; detours in 38% of episodes) |
| qwen3-max *(target)* | 34.1% | 21.3% · 21.9% | 14.4% · 3.1% | 3.9% · 0.0% | 24.2% · 17.5% | 16.5% · 6.9% | 2.1% · 0.0% | 28.4% · 43.8% | **passes offline** (p ≥ 0.5; detours in 22% of episodes) |
| qwen3.5 *(target)* | 40.7% | 27.0% · 11.9% | 18.7% · 5.0% | 5.1% · 0.6% | 29.7% · 13.1% | 21.6% · 2.5% | 4.9% · 0.6% | 34.1% · 36.9% | **passes offline** (p ≥ 0.5; detours in 12% of episodes) |

### Transfer: top-1 when the habit comes from another model (k = 2)

| habit from ↓ / tested on → | claude-3-7-sonnet | gpt-4.1 | gpt-4.1-mini | o4-mini | glm-5 *(target)* | qwen3.5 *(target)* | qwen3-max *(target)* | gpt-5.2 *(target)* | gpt-5.2-none *(target)* | claude-opus-4.5 *(target)* | claude-sonnet-4.5 *(target)* | gemini-3-pro *(target)* | gemini-3-flash *(target)* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 63% | 59% | 48% | 54% | 63% | 60% | 67% | 56% | 57% | 62% | 61% | 61% | 60% |
| gpt-4.1 | 62% | 61% | 52% | 56% | 62% | 59% | 65% | 57% | 58% | 62% | 60% | 60% | 60% |
| gpt-4.1-mini | 52% | 53% | 64% | 51% | 60% | 56% | 52% | 58% | 59% | 61% | 56% | 57% | 59% |
| o4-mini | 59% | 60% | 52% | 58% | 56% | 53% | 61% | 52% | 53% | 56% | 57% | 54% | 53% |
| glm-5 *(target)* | 56% | 54% | 53% | 45% | 67% | 64% | 58% | 61% | 62% | 67% | 61% | 65% | 65% |
| qwen3.5 *(target)* | 54% | 53% | 50% | 43% | 65% | 64% | 58% | 60% | 61% | 66% | 60% | 63% | 64% |
| qwen3-max *(target)* | 61% | 58% | 47% | 53% | 62% | 59% | 65% | 55% | 57% | 61% | 60% | 60% | 59% |
| gpt-5.2 *(target)* | 54% | 53% | 51% | 49% | 64% | 62% | 57% | 63% | 63% | 65% | 58% | 63% | 63% |
| gpt-5.2-none *(target)* | 55% | 56% | 54% | 50% | 64% | 63% | 57% | 63% | 63% | 65% | 60% | 64% | 64% |
| claude-opus-4.5 *(target)* | 57% | 57% | 55% | 49% | 66% | 63% | 59% | 62% | 63% | 67% | 63% | 65% | 65% |
| claude-sonnet-4.5 *(target)* | 57% | 58% | 56% | 49% | 67% | 64% | 60% | 62% | 62% | 68% | 64% | 65% | 66% |
| gemini-3-pro *(target)* | 53% | 53% | 51% | 46% | 64% | 62% | 56% | 62% | 63% | 65% | 58% | 65% | 64% |
| gemini-3-flash *(target)* | 54% | 53% | 50% | 43% | 66% | 63% | 57% | 61% | 61% | 66% | 61% | 65% | 65% |

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
| qwen3.5 *(target)* | gpt-5.2 | 200 | 82% | 12.0 | 8.4 | 1.06 | 50% | 85% / 92% |
| qwen3-max *(target)* | gpt-4.1 | 200 | 71% | 12.3 | 7.4 | 0.99 | 41% | 86% / 96% |
| gpt-5.2 *(target)* | gpt-5.2 | 200 | 83% | 8.1 | 10.6 | 0.87 | 27% | 97% / 97% |
| gpt-5.2-none *(target)* | gpt-5.2 | 200 | 80% | 8.5 | 10.2 | 0.85 | 29% | 99% / 100% |
| claude-opus-4.5 *(target)* | gpt-5.2 | 200 | 84% | 7.6 | 7.9 | 0.87 | 20% | 98% / 99% |
| claude-sonnet-4.5 *(target)* | gpt-5.2 | 200 | 72% | 11.3 | 7.6 | 0.82 | 41% | 93% / 98% |
| gemini-3-pro *(target)* | gpt-5.2 | 200 | 80% | 9.7 | 8.2 | 0.92 | 37% | 87% / 93% |
| gemini-3-flash *(target)* | gpt-5.2 | 200 | 67% | 11.2 | 10.9 | 0.81 | 48% | 93% / 98% |
| **all** | | | | | | | **25%** | |

### Next-action predictability (held-out)

| Agent model | k=0 bits / top-1 | k=1 bits / top-1 | k=2 bits / top-1 | k=3 bits / top-1 |
|---|---|---|---|---|
| claude-3-7-sonnet | 2.93 / 38% | 2.47 / 46% | 2.42 / 52% | 2.49 / 51% |
| gpt-4.1 | 2.87 / 41% | 2.10 / 50% | 2.10 / 54% | 2.11 / 54% |
| gpt-4.1-mini | 2.75 / 45% | 2.14 / 49% | 2.09 / 51% | 2.08 / 51% |
| o4-mini | 2.60 / 49% | 2.22 / 49% | 2.17 / 56% | 2.20 / 55% |
| glm-5 *(target)* | 2.76 / 34% | 2.02 / 58% | 1.83 / 57% | 1.78 / 64% |
| qwen3.5 *(target)* | 2.87 / 26% | 2.19 / 46% | 2.19 / 53% | 2.22 / 51% |
| qwen3-max *(target)* | 2.61 / 42% | 1.96 / 60% | 1.67 / 67% | 1.71 / 67% |
| gpt-5.2 *(target)* | 3.16 / 23% | 2.38 / 36% | 2.11 / 42% | 2.18 / 50% |
| gpt-5.2-none *(target)* | 3.03 / 27% | 2.07 / 46% | 1.98 / 51% | 2.01 / 51% |
| claude-opus-4.5 *(target)* | 2.90 / 32% | 2.04 / 56% | 1.93 / 61% | 1.91 / 61% |
| claude-sonnet-4.5 *(target)* | 3.01 / 35% | 2.21 / 56% | 2.18 / 51% | 2.25 / 50% |
| gemini-3-pro *(target)* | 2.91 / 31% | 2.04 / 54% | 1.92 / 60% | 1.92 / 60% |
| gemini-3-flash *(target)* | 2.90 / 23% | 2.11 / 57% | 1.96 / 61% | 2.00 / 60% |
| **all models pooled** | 2.81 / 43% | 2.17 / 55% | 2.21 / 52% | 2.27 / 53% |

### Coverage: decisions the habit could take (k = 2)

| Agent model | τ=0.5 cover / agree | τ=0.7 cover / agree | τ=0.8 cover / agree | τ=0.9 cover / agree | τ=0.95 cover / agree |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 52% / 60% | 18% / 79% | 8% / 99% | 1% / 100% | 0% / 0% |
| gpt-4.1 | 55% / 58% | 10% / 91% | 8% / 100% | 8% / 100% | 8% / 100% |
| gpt-4.1-mini | 51% / 56% | 11% / 91% | 8% / 99% | 7% / 100% | 7% / 100% |
| o4-mini | 28% / 72% | 13% / 97% | 13% / 97% | 13% / 98% | 13% / 98% |
| glm-5 *(target)* | 64% / 65% | 26% / 73% | 7% / 83% | 2% / 93% | 2% / 93% |
| qwen3.5 *(target)* | 58% / 70% | 37% / 79% | 8% / 93% | 2% / 100% | 2% / 100% |
| qwen3-max *(target)* | 83% / 69% | 34% / 79% | 14% / 89% | 9% / 99% | 1% / 100% |
| gpt-5.2 *(target)* | 43% / 70% | 26% / 75% | 5% / 89% | 2% / 100% | 2% / 100% |
| gpt-5.2-none *(target)* | 59% / 69% | 32% / 76% | 10% / 88% | 3% / 100% | 3% / 100% |
| claude-opus-4.5 *(target)* | 57% / 65% | 31% / 73% | 7% / 78% | 1% / 100% | 1% / 100% |
| claude-sonnet-4.5 *(target)* | 44% / 65% | 15% / 63% | 1% / 92% | 1% / 92% | 1% / 92% |
| gemini-3-pro *(target)* | 53% / 65% | 29% / 74% | 7% / 80% | 2% / 100% | 2% / 100% |
| gemini-3-flash *(target)* | 62% / 71% | 33% / 72% | 14% / 80% | 2% / 100% | 2% / 100% |
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

The named intent is the set of write tools the episode goes on to call (19 distinct), standing in for what the LLM states when it calls a macro-tool. It is layered on top of the shared model, so rare intents fall back to it.

| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ=0.8 | After the user: cover / agree at τ=0.8 | All: cover / agree at τ=0.8 | at τ=0.9 |
|---|---|---|---|---|---|---|
| the action sequence | 2.21 | 52% | 6% / 99% | 18% / 99% | 11% / 99% | 9% / 99% |
| + code features | 2.22 | 52% | 8% / 97% | 18% / 99% | 12% / 98% | 10% / 99% |
| + code features + named intent | 2.23 | 51% | 8% / 95% | 18% / 99% | 12% / 97% | 11% / 99% |

### Projection: what macro-tools would save on held-out tasks

Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit above (code features + named intent). Where the habit may not act, the decision goes either to the LLM (a pause, one turn) or to a System-One model assumed to agree with the agent (the upper bound Phase 0b will test). Generated arguments always pause. Collapsing every run to one call would save 23.9% of LLM turns (the ceiling).

The habit may act either wherever its top option clears a threshold (with at least 5 training observations), or only in *validated* contexts: those where its top option matched the agent in at least 99% of at least 20 decisions from at least 10 distinct tasks, under 5-fold cross-validation grouped by task. A habit that hands back too early is safe: the LLM takes the step, at the cost of one turn. A habit that picks another tool, or carries on when the agent stopped, is the risk.

| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |
|---|---|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | **0.0%** | 4.60 | 0.46 | 0.00 | 0.6 | 0.0 | 0.0% |
| pause for the LLM | top option ≥ 0.95 | **0.0%** | 4.60 | 0.31 | 0.00 | 0.0 | 0.0 | 0.0% |
| pause for the LLM | in 1 validated context | **0.0%** | 4.60 | 0.16 | 0.00 | 0.0 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.9 | **20.5%** | 0.54 | 0.46 | 7.29 | 0.6 | 0.0 | 0.0% |
| ask a perfect System-One model | top option ≥ 0.95 | **20.6%** | 0.54 | 0.31 | 7.44 | 0.0 | 0.0 | 0.0% |
| ask a perfect System-One model | in 1 validated context | **20.6%** | 0.54 | 0.16 | 7.59 | 0.0 | 0.0 | 0.0% |

Validated contexts, and how the habit did in them on held-out tasks:

| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |
|---|---|---|---|
| `respond (user replied)` → `transfer_to_human_agents (ok)` | respond | 100.0% of 96 (15 tasks) | 100.0% of 52 |

Tokens and dollars come from the usage the benchmark recorded for every LLM call. A removed turn saves its whole prompt and completion. A run that collapses to one call also stops carrying its intermediate tool outputs in every later prompt; their size is read off the recorded prompt growth, and priced at each model's effective input price, fitted to its recorded costs by least squares (so it absorbs any prompt-caching discount): claude-3-7-sonnet $3.00 in / $15.00 out per MTok; claude-opus-4.5 $5.00 in / $25.00 out per MTok; claude-sonnet-4.5 $3.00 in / $15.00 out per MTok; gemini-3-flash $0.20 in / $3.14 out per MTok; gemini-3-pro $0.78 in / $12.42 out per MTok; gpt-4.1 $1.04 in / $6.69 out per MTok; gpt-4.1-mini $0.20 in / $1.44 out per MTok; gpt-5.2 $0.83 in / $14.41 out per MTok; gpt-5.2-none $0.70 in / $15.21 out per MTok; o4-mini $0.65 in / $4.04 out per MTok.

| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|
| pause for the LLM | top option ≥ 0.9 | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| pause for the LLM | top option ≥ 0.95 | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| pause for the LLM | in 1 validated context | 0.0% | 0.0% | 0.0% | **0.0%** | 0.1170 → 0.1170 |
| ask a perfect System-One model | top option ≥ 0.9 | 20.5% | 28.2% | 24.9% | **37.4%** | 0.1170 → 0.0732 |
| ask a perfect System-One model | top option ≥ 0.95 | 20.6% | 28.3% | 24.9% | **37.6%** | 0.1170 → 0.0730 |
| ask a perfect System-One model | in 1 validated context | 20.6% | 28.3% | 24.9% | **37.6%** | 0.1170 → 0.0730 |

By agent model, with the habit acting only in validated contexts and a perfect System-One model deciding the rest. *Read-only flows* follow RFC-001's plan/commit rule: between LLM turns a flow only reads, and every write goes back to the LLM. Pooled dollars weigh each model by its spend. Models marked *(target)* were never trained on: the habit, its features, the closed argument sets and the validated contexts all come from the other models, and the pooled tables above leave them out.

| Agent model | Tool turns with parallel calls | Turns saved | Read-only flows | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 0% | **36.6%** | 32.6% | 40.7% | 44.1% | 28.0% | 42.8% | 0.3508 → 0.2008 |
| gpt-4.1 | 22% | **10.9%** | 8.9% | 15.3% | 14.6% | 9.3% | 14.8% | 0.0517 → 0.0440 |
| gpt-4.1-mini | 24% | **6.6%** | 5.3% | 9.1% | 9.2% | 6.9% | 9.8% | 0.0128 → 0.0115 |
| o4-mini | 0% | **24.5%** | 20.7% | 26.8% | 30.2% | 29.7% | 32.4% | 0.0530 → 0.0358 |
| claude-opus-4.5 *(target)* | 41% | **13.4%** | 11.9% | 18.2% | 14.4% | 9.0% | 13.3% | 0.3859 → 0.3346 |
| claude-sonnet-4.5 *(target)* | 4% | **33.2%** | 30.8% | 42.5% | 38.6% | 22.8% | 37.1% | 0.3091 → 0.1943 |
| gemini-3-flash *(target)* | 17% | **42.4%** | 39.5% | 53.0% | 44.1% | 26.1% | 33.3% | 0.0314 → 0.0210 |
| gemini-3-pro *(target)* | 20% | **30.8%** | 27.6% | 38.0% | 32.1% | 16.7% | 21.7% | 0.1276 → 0.0999 |
| glm-5 *(target)* | 45% | **15.4%** | 14.9% | 18.9% | 17.6% | 13.7% | – | no cost recorded |
| gpt-5.2 *(target)* | 37% | **20.8%** | 17.0% | 26.9% | 24.5% | 17.2% | 19.6% | 0.1267 → 0.1018 |
| gpt-5.2-none *(target)* | 32% | **17.9%** | 14.7% | 25.8% | 20.3% | 12.1% | 15.6% | 0.0932 → 0.0787 |
| qwen3-max *(target)* | 0% | **36.3%** | 33.2% | 39.2% | – | – | – | no usage recorded |
| qwen3.5 *(target)* | 0% | **44.1%** | 40.7% | 53.1% | 45.9% | 21.6% | – | no cost recorded |

### Phase 0b: the System-One model in shadow mode

Questions v2 (RFC-001 §3.5), for read-only flows. At every held-out decision right after a tool returns, the System-One model was asked which lookup the agent makes next, with the options limited to the lookups the agent made after the same tool in training, or whether it hands back (to write to the customer, or to make a change, which only the LLM does). The same request also asks the stop decision on its own (*split*: "does it make another lookup now?", then which one). The state is a slice: the customer's messages, the agent's last message, the latest four results in full and older lookups one line each. *Combined* answers weigh the System-One answers against the habit's prediction and the model's record at that site on other tasks, fitted by cross-validation over tasks (RFC-001 §3.6). Numeric arguments are not asked about. Oracle: `replay` (answering model: jev-1.13.0). 10071 decisions, 8746 distinct questions, 8746 answered, 0 failed; 19065551 input tokens ($0.80 at $0.042/MTok). 1270 next-step decisions came at sites where the agent never looked anything up in training, so the flow hands back without asking; the options held the agent's step in 99.1% of the source models' next-step decisions.

Next step, as a read-only flow takes it (a lookup, or hand back):

| Agent model | Decisions | One question | Split | Combined | Stop vs. go on | ECE | Combined p ≥ 0.9: share / agreed | Combined p ≥ 0.99: share / agreed |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 672 | 70.5% | 66.7% | 72.8% | 76.6% | 0.100 | 32.9% / 95.5% | 14.3% / 92.7% |
| gpt-4.1 | 643 | 73.9% | 70.0% | 75.4% | 77.3% | 0.046 | 33.1% / 100.0% | 12.1% / 100.0% |
| gpt-4.1-mini | 680 | 67.4% | 66.3% | 71.0% | 71.3% | 0.130 | 39.0% / 94.0% | 16.8% / 99.1% |
| o4-mini | 424 | 81.1% | 82.3% | 80.4% | 82.8% | 0.071 | 45.0% / 95.3% | 24.8% / 99.0% |
| claude-opus-4.5 *(target)* | 685 | 64.4% | 62.2% | 69.6% | 71.2% | 0.163 | 29.1% / 98.0% | 13.4% / 98.9% |
| claude-sonnet-4.5 *(target)* | 646 | 66.9% | 65.5% | 69.3% | 70.1% | 0.158 | 35.4% / 93.4% | 15.3% / 89.9% |
| gemini-3-flash *(target)* | 922 | 53.6% | 51.1% | 59.0% | 64.3% | 0.225 | 22.9% / 98.1% | 11.8% / 99.1% |
| gemini-3-pro *(target)* | 754 | 54.9% | 55.2% | 62.2% | 63.9% | 0.239 | 25.5% / 97.9% | 13.7% / 99.0% |
| glm-5 *(target)* | 645 | 63.3% | 60.6% | 69.5% | 67.9% | 0.171 | 27.1% / 97.7% | 14.6% / 97.9% |
| gpt-5.2 *(target)* | 1058 | 68.0% | 65.1% | 75.0% | 72.4% | 0.109 | 22.3% / 97.0% | 10.6% / 98.2% |
| gpt-5.2-none *(target)* | 899 | 62.3% | 59.7% | 72.0% | 67.2% | 0.157 | 27.3% / 99.2% | 12.8% / 100.0% |
| qwen3-max *(target)* | 612 | 66.8% | 68.8% | 74.0% | 70.6% | 0.137 | 30.7% / 98.9% | 16.7% / 100.0% |
| qwen3.5 *(target)* | 762 | 65.0% | 62.7% | 70.7% | 73.8% | 0.150 | 29.7% / 97.3% | 14.0% / 97.2% |
| **all source models** | 2419 | 72.4% | 70.2% | 74.3% | 76.4% | 0.084 | 36.8% / 96.1% | 16.2% / 97.7% |

The arbiter's weights, averaged over folds: habit 0.39, one question 0.24, split 0.25, handing back -0.90, the model's record at the site 0.69.

Closed-set arguments:

| Agent model | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|
| claude-3-7-sonnet | 8 | 100.0% | 100.0% / 100.0% |
| gpt-4.1 | 3 | 100.0% | 100.0% / 100.0% |
| gpt-4.1-mini | 4 | 25.0% | 25.0% / 100.0% |
| o4-mini | 7 | 100.0% | 100.0% / 100.0% |
| claude-opus-4.5 *(target)* | 59 | 32.2% | 49.2% / 62.1% |
| claude-sonnet-4.5 *(target)* | 40 | 40.0% | 47.5% / 78.9% |
| gemini-3-flash *(target)* | 48 | 29.2% | 35.4% / 58.8% |
| gemini-3-pro *(target)* | 54 | 13.0% | 16.7% / 77.8% |
| glm-5 *(target)* | 13 | 61.5% | 84.6% / 72.7% |
| gpt-5.2 *(target)* | 366 | 30.3% | 29.0% / 70.8% |
| gpt-5.2-none *(target)* | 30 | 23.3% | 33.3% / 70.0% |
| qwen3-max *(target)* | 10 | 100.0% | 80.0% / 100.0% |
| qwen3.5 *(target)* | 27 | 33.3% | 40.7% / 72.7% |
| **all source models** | 22 | 86.4% | 86.4% / 100.0% |

Closed-set arguments by argument (source models):

| Tool | Argument | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|---|
| `search_onestop_flight` | `date` | 19 | 100.0% | 100.0% / 100.0% |
| `search_onestop_flight` | `destination` | 2 | 0.0% | 0.0% / 0.0% |
| `search_onestop_flight` | `origin` | 1 | 0.0% | 0.0% / 0.0% |

Projection with the System-One model, pooled over the source models, for read-only flows: the habit acts in validated contexts, a System-One pick is trusted at or above the threshold (*two keys*: only when it is also the habit's top option; *combined*: the arbiter's probability), and anything else pauses. A write always goes back to the LLM. Nothing a read-only flow does changes the environment, so its wrong picks are *detours*, extra lookups that cost a pause, not risks. Handing back costs a turn and a detour does not, so *lookup first* takes the likeliest lookup whenever it clears the (lower) threshold. Each detour is charged a typical lookup's output (201 tokens) in every later prompt, so the dollars saved include the detours' cost.

| Pick trusted at | Turns saved | $ saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour | Writes handed back/ep |
|---|---|---|---|---|---|---|---|---|
| p ≥ 0.5 | **10.0%** | 14.6% | 2.63 | 6.16 | 96.9 | 65.0 | 46.6% | 0.68 |
| p ≥ 0.7 | **8.1%** | 11.2% | 3.04 | 4.69 | 57.8 | 30.9 | 25.6% | 0.68 |
| p ≥ 0.9 | **4.6%** | 5.6% | 3.69 | 2.87 | 17.8 | 7.8 | 7.2% | 0.68 |
| p ≥ 0.95 | **3.4%** | 4.3% | 3.93 | 2.27 | 10.0 | 2.8 | 2.8% | 0.68 |
| p ≥ 0.99 | **0.7%** | 1.1% | 4.42 | 1.32 | 5.0 | 0.3 | 0.3% | 0.68 |
| two keys, p ≥ 0.5 | **5.4%** | 7.5% | 3.51 | 3.27 | 36.9 | 16.9 | 15.9% | 0.68 |
| two keys, p ≥ 0.7 | **4.2%** | 5.5% | 3.69 | 2.65 | 15.6 | 9.1 | 8.8% | 0.68 |
| two keys, p ≥ 0.9 | **2.8%** | 3.7% | 3.99 | 2.04 | 7.8 | 2.2 | 2.2% | 0.68 |
| two keys, p ≥ 0.95 | **2.0%** | 2.8% | 4.17 | 1.73 | 6.6 | 0.9 | 0.9% | 0.68 |
| two keys, p ≥ 0.99 | **0.3%** | 0.5% | 4.50 | 1.11 | 3.4 | 0.3 | 0.3% | 0.68 |
| combined, p ≥ 0.5 | **10.4%** | 15.0% | 2.58 | 5.64 | 58.8 | 70.9 | 49.4% | 0.68 |
| combined, p ≥ 0.7 | **8.1%** | 10.8% | 3.08 | 3.66 | 20.6 | 31.9 | 28.7% | 0.68 |
| combined, p ≥ 0.9 | **4.9%** | 6.1% | 3.59 | 2.23 | 4.7 | 5.9 | 5.9% | 0.68 |
| combined, p ≥ 0.95 | **1.2%** | 1.5% | 4.29 | 1.28 | 2.8 | 0.0 | 0.0% | 0.68 |
| combined, p ≥ 0.99 | **0.1%** | 0.0% | 4.59 | 0.76 | 2.8 | 0.0 | 0.0% | 0.68 |
| lookup first, p ≥ 0.2 | **13.7%** | 21.3% | 1.69 | 5.68 | 2.5 | 175.9 | 81.6% | 0.68 |
| lookup first, p ≥ 0.3 | **12.6%** | 18.9% | 1.96 | 4.97 | 2.5 | 133.1 | 70.3% | 0.68 |
| lookup first, p ≥ 0.4 | **11.7%** | 17.4% | 2.28 | 4.28 | 2.5 | 97.8 | 59.7% | 0.68 |

The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes holding a risky decision (a conservative stand-in for losing at most one point of pass^1). Read-only flows take no risky decisions, so offline the gate is the turns saved; each cell is turns saved · episodes with a detour. Whether detours cost pass^1 is for a live run to show.

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | lookup first, p ≥ 0.3 | Gate |
|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 32.6% | 15.7% · 35.0% | 11.2% · 20.0% | 5.4% · 3.8% | 16.6% · 36.2% | 11.9% · 10.0% | 6.8% · 1.2% | 21.8% · 73.8% | **passes offline** (lookup first, p ≥ 0.3; detours in 74% of episodes) |
| gpt-4.1 | 8.9% | 5.0% · 52.5% | 4.6% · 17.5% | 3.2% · 1.2% | 5.5% · 60.0% | 5.3% · 32.5% | 3.3% · 0.0% | 6.4% · 73.8% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 5.3% | 2.1% · 62.5% | 2.0% · 38.8% | 1.7% · 13.8% | 2.6% · 63.7% | 2.4% · 47.5% | 1.7% · 13.8% | 2.7% · 77.5% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.7% | 16.1% · 36.2% | 14.3% · 26.2% | 8.2% · 10.0% | 16.1% · 37.5% | 12.1% · 25.0% | 7.6% · 8.8% | 17.6% · 56.2% | fails |
| claude-opus-4.5 *(target)* | 11.9% | 2.0% · 43.8% | 1.6% · 37.5% | 1.3% · 18.8% | 2.8% · 37.5% | 2.3% · 18.8% | 0.8% · 5.0% | 4.9% · 71.2% | fails: below 20% even with a perfect System-One model |
| claude-sonnet-4.5 *(target)* | 30.8% | 14.5% · 45.0% | 12.1% · 27.5% | 8.9% · 15.0% | 15.2% · 43.8% | 12.8% · 21.2% | 8.4% · 5.0% | 19.3% · 70.0% | fails |
| gemini-3-flash *(target)* | 39.5% | 9.4% · 55.0% | 7.4% · 40.0% | 4.9% · 17.5% | 10.8% · 48.8% | 7.0% · 21.2% | 4.1% · 7.5% | 16.3% · 87.5% | fails |
| gemini-3-pro *(target)* | 27.6% | 8.3% · 45.0% | 7.4% · 32.5% | 4.6% · 17.5% | 10.6% · 41.2% | 7.8% · 20.0% | 4.5% · 3.8% | 14.1% · 75.0% | fails |
| glm-5 *(target)* | 14.9% | 2.1% · 48.8% | 1.6% · 30.0% | 0.8% · 15.0% | 2.7% · 36.2% | 1.7% · 10.0% | 0.6% · 3.8% | 4.4% · 78.8% | fails: below 20% even with a perfect System-One model |
| gpt-5.2 *(target)* | 17.0% | 1.9% · 46.2% | 1.4% · 35.0% | 0.7% · 11.2% | 2.8% · 38.8% | 2.1% · 21.2% | 1.0% · 5.0% | 4.3% · 67.5% | fails: below 20% even with a perfect System-One model |
| gpt-5.2-none *(target)* | 14.7% | 1.7% · 33.8% | 1.1% · 25.0% | 0.6% · 7.5% | 2.4% · 32.5% | 1.6% · 12.5% | 0.6% · 3.8% | 4.0% · 71.2% | fails: below 20% even with a perfect System-One model |
| qwen3-max *(target)* | 33.2% | 16.2% · 32.5% | 11.1% · 18.8% | 5.9% · 3.8% | 19.9% · 31.2% | 13.4% · 11.2% | 7.0% · 1.2% | 26.3% · 75.0% | **passes offline** (lookup first, p ≥ 0.3; detours in 75% of episodes) |
| qwen3.5 *(target)* | 40.7% | 19.1% · 43.8% | 16.0% · 35.0% | 8.8% · 21.2% | 22.5% · 45.0% | 16.8% · 22.5% | 10.0% · 5.0% | 27.8% · 73.8% | **passes offline** (combined, p ≥ 0.5; detours in 45% of episodes) |

### Transfer: top-1 when the habit comes from another model (k = 2)

| habit from ↓ / tested on → | claude-3-7-sonnet | gpt-4.1 | gpt-4.1-mini | o4-mini | glm-5 *(target)* | qwen3.5 *(target)* | qwen3-max *(target)* | gpt-5.2 *(target)* | gpt-5.2-none *(target)* | claude-opus-4.5 *(target)* | claude-sonnet-4.5 *(target)* | gemini-3-pro *(target)* | gemini-3-flash *(target)* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 52% | 47% | 48% | 56% | 57% | 54% | 65% | 40% | 45% | 55% | 48% | 53% | 47% |
| gpt-4.1 | 49% | 54% | 53% | 54% | 50% | 45% | 59% | 39% | 45% | 50% | 43% | 48% | 41% |
| gpt-4.1-mini | 46% | 47% | 51% | 53% | 48% | 43% | 55% | 35% | 38% | 43% | 41% | 43% | 37% |
| o4-mini | 51% | 47% | 48% | 56% | 54% | 54% | 62% | 37% | 42% | 51% | 47% | 49% | 42% |
| glm-5 *(target)* | 53% | 48% | 49% | 55% | 57% | 55% | 66% | 41% | 47% | 55% | 49% | 53% | 48% |
| qwen3.5 *(target)* | 47% | 45% | 43% | 52% | 53% | 53% | 63% | 38% | 42% | 53% | 46% | 48% | 43% |
| qwen3-max *(target)* | 53% | 53% | 52% | 56% | 59% | 53% | 67% | 44% | 52% | 58% | 50% | 52% | 49% |
| gpt-5.2 *(target)* | 47% | 49% | 45% | 50% | 55% | 51% | 63% | 42% | 51% | 53% | 44% | 49% | 49% |
| gpt-5.2-none *(target)* | 47% | 46% | 42% | 47% | 53% | 50% | 61% | 41% | 51% | 51% | 46% | 48% | 48% |
| claude-opus-4.5 *(target)* | 57% | 57% | 56% | 59% | 62% | 62% | 66% | 47% | 56% | 61% | 57% | 58% | 55% |
| claude-sonnet-4.5 *(target)* | 51% | 50% | 50% | 55% | 58% | 53% | 66% | 43% | 50% | 58% | 51% | 50% | 46% |
| gemini-3-pro *(target)* | 55% | 55% | 55% | 56% | 62% | 62% | 65% | 47% | 57% | 61% | 56% | 60% | 57% |
| gemini-3-flash *(target)* | 53% | 51% | 49% | 50% | 62% | 61% | 62% | 47% | 57% | 60% | 54% | 59% | 61% |

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
