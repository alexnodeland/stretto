# Messaging

How stretto describes itself: the tagline, the one-liner, descriptions of fixed lengths, the key messages and the numbers behind them, and the words to leave out. Every number here is from [the claims ledger](../docs/results/claims.md) or [the paper](../paper/stretto.md), with its scope. When a number changes there, change it here.

## Tagline

1. **Read ahead of your agent.** *(recommended)*
2. Your agent's next reads, in the result it already asked for.
3. Fewer LLM turns, learned from your agent's own tool calls.

The first is the shortest true statement of what stretto does: it makes the reads the agent would make next, before the agent asks for them. It also carries the name. A musician reads ahead in the score, and in a fugue's stretto each entry comes in before the last one ends. The second says how, for a reader who already knows MCP. The third says what it buys, and suits a context that shows no number.

Pair the tagline with the one-liner, never with a bare number.

## One-liner

stretto learns, from your agent's recorded tool calls, which reads it makes next and where their arguments come from, and serves them through an MCP proxy in the same tool result, so the agent needs fewer LLM turns.

## 50 words

stretto is an open-source MCP proxy, written in Rust. It records an agent's tool calls, learns which reads follow which and where their arguments come from, and makes those reads: after each call, the lookups ride in the same result, so the agent needs fewer LLM turns. It only reads.

## 150 words

stretto is an open-source MCP proxy for LLM agents, written in Rust and MIT-licensed. `stretto-proxy` wraps any MCP server and records the agent's sessions. From them, `stretto learn` writes a flow: which reads follow which calls, and where each argument comes from. Served, the flow makes the lookups the agent will likely need, and they ride in the same tool result, with no new tool and no change to the prompt.

The flow only reads, so a wrong lookup, a detour, costs tokens and changes no state. The reach decider asks no model: it makes a lookup when its chance of being used before the agent's next write clears a threshold set by costs. Live, on 28 τ²-bench retail and airline tasks, Claude Sonnet 5 took 20.5% fewer LLM turns (95% CI 16.5–24.4%) and Claude Haiku 4.5 22.4% fewer (15.8–29.4%), in three trials paired by task; GLM-5.3 took 27.9% fewer (19.1–35.9%) against its recorded baseline.

Where no user speaks, `stretto-procedure` runs a workflow compiled once from traces, with no model.

## Key messages

Each message rests on one row of [docs/results/claims.md](../docs/results/claims.md). Quote the number with its scope, as written in the evidence column, or not at all.

| # | Message | The number, with its scope | Claims row |
|---|---|---|---|
| 1 | **It works live, on frontier models, with no model of its own.** The reach decider counts; it asks no model and needs no key. | Live, pre-registered, three trials of 28 τ²-bench retail and airline tasks: Claude Sonnet 5 took 20.5% fewer LLM turns (95% CI 16.5–24.4%) and Claude Haiku 4.5 22.4% fewer (15.8–29.4%). GLM-5.3, one trial against its recorded baseline: 27.9% fewer (19.1–35.9%). The savings are in retail; airline's are not established. Pass rates are underpowered. | "It works on frontier models"; "It works live with no model of its own" |
| 1a | **A prompt does not get there.** Asking the agent to batch its reads is not the same as reading what the results reveal. | Live: Anthropic's sample prompt for parallel tool calls saved 3.4% (Sonnet 5), 5.9% (Haiku 4.5) and 7.6% (GLM-5.3) of turns; with the prompt in both arms, the flow still saved 22.9% (Sonnet 5, 95% CI 19.5–26.1%) and 17.2% (Haiku 4.5, 10.2–24.0%). | "A prompt does not get what it gets" |
| 1b | **It holds live on another benchmark's own environment.** Where the replay found reads to take, the live run found the savings. | Live: in AgentDojo's Slack and travel suites, GLM-5.3 and Claude Haiku 4.5 took 10.1% fewer LLM turns (95% CI 5.8–14.0%), with passes unchanged (27 of 34 in both arms); over all 41 AgentDojo tasks, 6.0% (2.1–9.9%), where the replay projected 7.2%. One run per arm. | "It holds live beyond τ²-bench" |
| 2 | **It estimates the probability that matters.** Not the chance a read comes next, but the chance it is used before the agent's next write. | Replay, nine agents: expected calibration error 0.01–0.08 in every τ²-bench domain, against 0.06–0.16 for the next-step probability. | "Counting the right event calibrates the probability of use" |
| 3 | **It takes most of what can be taken.** | Replay, nine agents it never saw: 86.4% of retail's read-only ceiling (95% CI 80.2–93.1), 10.2 points more than the next-step speculator (6.7–14.2). | "The speculator takes most of retail's ceiling" |
| 4 | **How much it saves is the domain's, and it stays out where it cannot help.** Set expectations before a number. | Record, 89 more agents on six benchmarks: the read-only ceiling runs from 3.5% of turns in WorkBench to 47.1% in AgentDojo's travel suite, and is 29.0% in τ²-bench. In WorkBench's 13,869 turns it makes no lookup. | "The ceiling is the domain's"; "It stays out where it cannot help" |
| 5 | **It learns fast.** A deployment learns from the sessions it serves. | Replay, three agents, three orders: ten of an agent's own sessions give 96% (retail) and 93% (airline) of what all of them do. | "It learns fast" |
| 6 | **Where no user speaks, compile once.** | The procedure alone passes 35 of 40 held-out solo telecom tasks with no model; with GLM-5.3 on its four hand-backs, 39 of 40, at 0.48 LLM turns per ticket against 15.9 for GLM-5.3 alone. | "Where no user speaks, compile once" |

Supporting facts, from the paper, for longer copy:

- **Reads only.** A flow calls only tools the server does not mark `readOnlyHint: false`, and `--flow-tools` narrows that further. Since reads leave the state unchanged, a speculator that only reads can change an episode only through what the agent reads, never through the tools (§2.2, Proposition 1). Its worst case is a detour.
- **The threshold is a ratio of costs.** Live, a detour carried 2,530 input tokens over the rest of its episode and a saved turn saved 6,000, so the threshold is about 0.3 (§2.3, §3).
- **The live runs held the replay's assumption.** Of the speculator's 101 lookups, GLM-5.3 made none again before the next write; Claude Sonnet 5 made 4 of 289 again, and Claude Haiku 4.5 1 of 304. Deciding took under a millisecond per tool response, at most 4 ms (§4.4).
- **Input tokens and cost.** With GLM-5.3, input tokens fell 21.9% (95% CI 9.9–32.9%) (§4.4). With the Claude models, input tokens fell 17.7% (Sonnet 5) and 20.6% (Haiku 4.5), and the agent's cost at list prices, with prompt caching, 11.8% and 8.7%: cost falls less than turns.

What the evidence does not show, and copy must not imply (claims ledger, "What the evidence does not show"):

- Live with the reach decider: GLM-5.3 (one run), Claude Sonnet 5 and Claude Haiku 4.5 (three trials) on τ²-bench, and GLM-5.3 and Claude Haiku 4.5 on AgentDojo and BFCL, one run per arm. Every other agent is replayed. In τ²-bench's airline domain the savings are not established; in BFCL the live run shows no effect.
- Pass rates are underpowered; τ²-bench's users are LLMs. Do not claim stretto keeps or raises task success.
- Replayed savings assume the agent skips what a lookup already answered.

## Terms

| Say | Not | Why |
|---|---|---|
| stretto, always lowercase, also at the start of a sentence | Stretto, STRETTO | The name is a lowercase word. |
| LLM turns | steps, API calls, requests | The measured unit. |
| reads, lookups | prefetches, cache hits | stretto makes a real call with bound arguments; nothing is cached. |
| a flow | a model, an agent | A flow is a JSON file of counts and bindings that a person can review. |
| a detour | a miss, an error | A lookup the agent did not use. It costs tokens and changes no state. |
| the next write | the next step | The event the decision is about. |
| the reach decider | the AI, the predictor | It counts; it asks no model. |
| live; replay; record | tested, proven | The three kinds of evidence, as the claims ledger defines them. |
| a procedure (`stretto-procedure`) | an autopilot, an agent | A compiled workflow that hands back what its check cannot confirm. |

## Words to avoid

- **Hype:** revolutionary, game-changing, blazing, lightning-fast, seamless, supercharge, turbocharge, 10x, magic, unleash, next-generation, cutting-edge, state-of-the-art, best-in-class, effortless.
- **Unscoped claims:** "cuts costs by X%", "X% faster", "saves X% of tokens" without the model, the benchmark and live or replayed; "guaranteed", "always", "never fails", "zero risk", "safe" (say *reads only* and what that implies).
- **Wrong mechanism:** "predicts what your agent will do" (it proposes reads only), "caches", "prefetches", "fine-tunes", "trains a model", "autonomous", "replaces your LLM", "hallucination-free".
- **Rounding up:** 27.9% is not "almost 30%" or "a third". Write the number the ledger writes.
- **Model names as endorsements:** name a model only as the scope of a result.
