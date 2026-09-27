---
description: What a lookup is, when it saves a turn, what a detour costs, which tools a flow may call, and the caps and logs around them.
---

# Lookups and detours

A *lookup* is a read a flow makes on the agent's behalf, right after one of the agent's own calls returned. The proxy makes it against the same server, with arguments the flow bound from earlier results, and appends its result to the result of the agent's call. This is speculation: the flow bets that the agent was about to ask for it.

## What the agent gets

The agent's own result comes back unchanged, with one more text item after it:

```text
--- Also looked up automatically (current results; no need to repeat these calls) ---

get_user_details {"user_id":"user_7"}:
{"user_id":"user_7","orders":["#W7a","#W7b"],…}
```

Each lookup appears as its tool, its arguments and its result. The heading tells the agent the results are current and need not be fetched again.

## Used, or a detour

A lookup is *used* when the agent would have made that same call before its next write: the flow's result is then exactly what the agent's own call would have returned, because reads do not change the state. A turn is *saved* when every call in it was answered by a lookup. A lookup the agent never needed is a *detour*.

A detour is the worst a flow can do. It costs the tokens of its result in every later turn's context, and the tool's own cost; it cannot change what the agent's writes act on, because a flow only reads ([the paper, Proposition 1](/research/paper#22-speculation-and-its-replay-semantics)). Nearly every detour in the research was the right lookup of the wrong record, such as a second reservation when the customer had asked about one; the bindings now count those cases ([bindings](./bindings#the-wrong-record)).

## When a flow makes one

After each call, the flow scores each lookup it may make there: the tool's probability under the flow's [decider](./deciders), times the chance that the arguments it would bind are the agent's own ([bindings](./bindings)). It makes the best one if that score is at least the threshold, then decides again with the new result in hand, and hands back when nothing clears it.

```sh
stretto-proxy ... --flow orders.flow.json --flow-decider reach --flow-threshold 0.3 -- <server command>
```

The threshold is 0.3 by default. That is not a tuning knob but a ratio of costs: make a lookup when its chance of use $q$ satisfies

$$q \ge \theta^\star = \frac{\delta}{\beta + \delta},$$

with $\delta$ a detour's cost and $\beta$ a saved turn's value. Live, a detour carried 2,530 input tokens over the rest of its episode and a saved turn saved 6,000, which puts $\theta^\star$ at 0.30. Counted in τ²-bench's recorded episodes, retail's costs give 0.30 and airline's and telecom's 0.12–0.13, since their contexts are longer and their results shorter ([the paper, §3](/research/paper#3-setup)). A domain with long contexts and short results can afford a lower threshold.

## Which tools a flow may call

- Only tools it learned as lookups: after each call, the lookups the agent made next in training (`sites` in the flow).
- Only tools the server does not mark `readOnlyHint: false`, once it has listed its tools.
- Only tools whose input contract is the one the flow learned. A flow learned from sessions pins each tool's arguments, their types and which are required; if the server now lists another contract, the proxy makes no lookup of that tool, and the agent makes the call itself.
- With `--flow-tools`, only the tools named there:

  ```sh
  stretto-proxy ... --flow orders.flow.json --flow-tools get_order_details,get_user_details -- <server command>
  ```

  A server's `readOnlyHint` says a call changes nothing, not that it is free, unlogged, or fine to make unasked: a read can be metered, rate-limited, or recorded as an access. Name the tools you are content for the flow to call.

## Caps

| Option | Default | Caps |
|---|---|---|
| `--flow-per-call` | 8 | lookups appended to one result |
| `--flow-per-session` | 40 | lookups in one session |
| `--flow-questions` | 300 | questions to the System-One model in one session (the arbiter decider only) |

## The flow log

Every decision is appended to `<session>.flow.jsonl` beside the session log (or to `--flow-log`), one JSON line each, with the site, each option's probability (`probs`), the lookup's probability (`prob`) and its binding's chance (`binding`), and either the lookup made (`tool`, `arguments`) or the reason for handing back:

```json
{"action": "hand_back", "address": "decide#2", "site": "read_text_file", "prob": 0.43,
 "probs": {"read_text_file": 0.43, "respond": 0.57}, "reason": "read_text_file: nothing left to pass as `path`"}
```

After each run's decisions comes one line for the run itself, `run`: each site's value and log-probability, and `surprise`, how unexpected the server's answers were to the flow, in nats. `address` names the decision's site in the flow's [program](./flows#the-run-after-each-call).

## Exploring, to evaluate other rules

`--flow-explore EPSILON` makes the flow, with that probability, take a lookup other than the rule's choice, and log every option with the chance that the flow took what it took. `stretto evaluate` reads such logs, labelled with each option's outcome, and estimates what another decider or threshold would have done, without running it ([results](../../../docs/results/evaluate-2026-09-25.md)).

## Related

- [Deciders](./deciders): where the tool's probability comes from
- [Bindings](./bindings): where the arguments come from
- [`stretto-proxy` options](/reference/cli#stretto-proxy)
