---
description: Why an agent spends turns on decisions its tools already made, what stretto takes over, and the evidence for what it saves.
---

# Why stretto?

An LLM agent pays a model turn for every decision it makes, yet many of its decisions are fixed by what its tools returned, not by what the user said. The customer's record lists their orders, so the agent reads an order. A search found three files, so the agent reads them. Each of those reads costs a turn: the model rereads the whole context to decide on a call that the previous result already implied.

stretto takes those reads off the model.

## Where the turns go

Over 39,298 LLM turns of nine frontier agents on τ²-bench's test tasks, 46.8% reply to the user, 12.2% write, and 41.0% only read. A program that binds a read's arguments from earlier tool results could have saved at most 29.0% of all turns: the reads whose every argument sits in an earlier result since the last write ([the paper, §4.1](/research/paper#41-what-decides-an-agents-turns)). That is the *read-only ceiling*. The rest, about seven turns in ten, stay with the model: they reply, write, or read what only the user's words can name.

The ceiling belongs to the domain. Where an agent walks records by the ids that results name, much of its work is in it; where each request names what to read, little is:

| Benchmark | Read-only ceiling |
|---|---|
| WorkBench (each request names its customer, task or date) | 3.5% of turns |
| τ²-bench (retail, airline, telecom) | 29.0% |
| AgentDojo's travel suite (one listing names every later read) | 47.1% |

From [the claims ledger](/research/claims), which gives the evidence behind every number on this page.

## What stretto does about it

After each of the agent's tool calls, a flow decides which reads to make before the agent's next turn, makes them, and appends their results to the call's result. The agent reads them there and does not ask. This has three properties that matter in production:

- **Only reads, so the worst case is a detour.** A flow calls only tools it learned as lookups and that the server does not mark as writes. A read leaves the environment's state unchanged, so the state after every write is the same with and without the flow ([Proposition 1](/research/paper#22-speculation-and-its-replay-semantics)). A lookup the agent never needed costs the tokens of its result in the context, and nothing else.
- **No model of its own.** The `reach` decider, the one to start with, counts, in recorded sessions, how often the agent made each read before its next write. Deciding took under a millisecond per tool response in the live run (at most 4 ms), and needs no API key.
- **Nothing changes for the agent.** No new tool, no prompt change, no framework. The proxy sits between the host and the server, where MCP already puts a server.

## The probability that matters

Speculative-action systems rank a candidate by the probability that it is the agent's *next* action. For reads, that is the wrong event: a read's result stays current until the next write, so a lookup pays off whenever the agent makes that call before its next write, not only next. stretto estimates the probability of use before the next write, by counting the same contexts for that event, and makes each read whose probability clears

$$\theta^\star = \frac{\delta}{\beta + \delta},$$

where $\delta$ is a detour's cost and $\beta$ a saved turn's value. The threshold is a ratio of costs, not a tuning knob, and both costs can be counted. Live, a detour carried 2,530 input tokens over the rest of its episode and a saved turn saved 6,000, so $\theta^\star \approx 0.30$. Counted in recorded episodes it comes out at 0.30 in retail and at 0.12–0.13 in airline and telecom, whose contexts are longer and whose results are shorter. See [deciders](./concepts/deciders).

## The evidence

*Live* means an agent ran with the flow; *replay* means recorded episodes re-run against τ²-bench's environment with the flow serving; *record* means a replay from published trajectories alone.

| Claim | Number | Evidence |
|---|---|---|
| It works live with no model of its own | GLM-5.3 took 27.9% fewer LLM turns (19.1–35.9%) on 28 τ²-bench retail and airline tasks; 21 passed, against 24 | Live, paired with the recorded baseline |
| Counting the right event calibrates the probability of use | expected calibration error 0.01–0.08 in every τ²-bench domain, against 0.06–0.16 for the next-step probability | Replay, nine agents |
| It takes most of retail's ceiling | 86.4% of it [80.2, 93.1], 10.2 points more than the next-step speculator [6.7, 14.2] | Replay, nine agents it never saw |
| It learns fast | ten of an agent's own sessions give 96% (retail) and 93% (airline) of what all of them do | Replay, three agents, three orders |
| It stays out where it cannot help | no lookup in WorkBench's 13,869 turns | Record |
| It carries across harnesses | learned in other harnesses, 59% of what the agent's own sessions save | Record, DTap-Bench's three agent SDKs |

Intervals are 95%, from bootstraps over tasks. The [claims ledger](/research/claims) lists every headline number, the kind of evidence it rests on, and the script that recomputes it.

In time and money, replayed on the six agents whose episodes record each turn's time and cost: in retail, the turns the flow saves, 28% of all turns, are 24% of the episodes' generation time, 36 seconds an episode, and 18% of their cost net of its detours at the agent's input price ([the paper, §4.2](/research/paper#42-speculation-in-replay)).

## How it compares

- **Workflow memory and induced skills** give the agent text or callable routines mined from its successes. They change what the model sees. stretto changes neither the prompt nor the tool list: its reads arrive inside results the agent asked for.
- **Speculative tool calls** hide latency: the agent still makes each call, and its result is ready sooner. stretto spares the turn, since the agent sees the result before it would ask for it.
- **Compiling traces into programs** removes the model where a task recurs without branching. stretto does that too, where no user speaks, as a [procedure](./concepts/procedures). While a user is present it compiles only the reads, which the environment decides.

The paper's [related work](/research/paper#5-related-work) has the details and the references.

## What the evidence does not show

- **Replays assume the agent skips what a lookup already answered.** GLM-5.3 did, live: of the flow's 101 lookups it made none again before the next write. In the published record, agents repeat 0.8–2.6% of their reads with no write between on five benchmarks, and a few models many more (gpt-oss-120b 23%, Qwen3.5 Flash 17.5%), whose replayed savings are upper bounds.
- **Few agents live.** The `reach` decider ran live with GLM-5.3 only; an earlier flow also ran with Claude Haiku 4.5 and Claude Sonnet 5 on ten and three retail tasks. Every other agent is replayed.
- **Pass rates are underpowered**, and τ²-bench's users are LLMs. The live runs bound turns, not success.

So measure it on your own traffic: record, run the flow in [shadow mode](./concepts/shadow-and-promotion), and [audit](./concepts/audit-and-review) it before you let it act.

## Next

- [Quick start](./quick-start)
- [How it works](./how-it-works)
- [The paper](/research/paper)
