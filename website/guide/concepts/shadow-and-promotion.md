---
description: Run a flow in shadow mode, where it decides and logs but makes no lookups, then promote it to act only where its lookups were the agent's own.
---

# Shadow mode and promotion

A flow learned from yesterday's sessions is a prediction about today's. Before it acts, you can watch it decide on real traffic without letting it do anything, then keep it to the places where it was right.

## 1. Run it in shadow

```sh
stretto-proxy --record ~/.stretto/shadow/orders --domain orders \
  --flow ~/.stretto/orders.flow.json --flow-decider reach --flow-shadow \
  -- <server command>
```

`stretto init --host HOST --domain orders --flow ~/.stretto/orders.flow.json --shadow -- <server command>` prints this configuration for your host, recording to `~/.stretto/shadow/orders`; for a flow with no arbiter it writes `--flow-decider habit`, which you can change to `reach` ([integrations](/integrations/)).

With `--flow-shadow`, the flow decides after each of the agent's calls and logs what it would look up, marked `"shadow": true` in the flow log, but makes no lookups. The agent gets the server's results unchanged. Serve it this way for as many sessions as it takes to see each kind of request a few times.

## 2. Promote it

`stretto promote` makes the same decisions again on the shadow sessions and scores each lookup the flow would have made: *used* if the agent made that call later in the session, a *detour* if it never did.

```sh
stretto promote --flow ~/.stretto/orders.flow.json \
  --sessions ~/.stretto/shadow/orders --decider reach \
  --out ~/.stretto/orders-promoted.flow.json
```

It writes a report with one row per site (the tool whose call the flow decided after): the decisions there, the lookups it would have made, how many the agent made later, a lower bound on that share, and how many sessions they came from. A site is *promoted* when, at the threshold the flow will be served with:

| Option | Default | The bar |
|---|---|---|
| `--min-used` | 0.7 | at least this share of the flow's lookups at the site were made by the agent later |
| `--min-lower` | 0.5 | the lower bound on that share (Wilson, 90% two-sided) is at least this |
| `--min-tasks` | 3 | the lookups came from at least this many sessions (each session counts as its own task) |
| `--threshold` | 0.3 | the threshold the flow will be served with |

The promoted flow acts only after the calls whose site met the bar, and hands back after every other call with the reason `the site is not promoted`. A site never scored is not promoted.

A flow with an arbiter asks the System-One model in shadow too. The proxy caches the answers, so pass its cache to `promote`, which then asks nothing: `--oracle-cache ~/.stretto/oracle-cache` (the proxy's default). A flow that asks no model, deciding with `habit` or `reach`, can also be promoted on sessions recorded without it: `promote` makes its decisions again from the sessions alone.

## 3. Serve the promoted flow

```sh
stretto-proxy --record ~/.stretto/logs/orders --domain orders \
  --flow ~/.stretto/orders-promoted.flow.json --flow-decider reach \
  -- <server command>
```

`stretto init` prints it for your host, as for the shadow run, without `--shadow`.

`stretto flow-diff` lists a newly promoted site as needing review, since the flow now acts after a call where it handed back ([audit and review](./audit-and-review#compare-two-flows)).

## What it bought

**Replayed.** Promoted on half of τ²-bench's test tasks and replayed on the other half, the arbiter flow (D0) kept 277 of its 294 retail turns saved and all 41 in airline, with detours down from 58 to 28 and from 26 to 11. The habit alone gained little: its detours were at sites that pass the bar ([results](../../../docs/results/promotion-2026-09-25.md)).

**Live, once.** On AgentDojo's Slack suite, promotion scored the published flow on each agent's own sessions of the 12 training tasks, recorded without a flow, and served the promoted flow on the 9 held-out tasks. It removed all 43 detours, and with them the saved turns: over both agents' held-out tasks, 72 LLM turns against 75 without a flow, where the published flow took 68. The detours came from one walk through the channels, which pays on a task that asks about every channel and is a detour on one that posts to a channel. Only the request says which, and a per-site bar can keep the walk or drop it, not keep it for one kind of task ([results](/research/live#promotion-on-the-agents-own-sessions-live)). Shadow mode itself has not yet run on live traffic.

## Related

- [Audit and review](./audit-and-review)
- [`stretto promote`](/reference/cli#stretto-promote) and [`--flow-shadow`](/reference/cli#stretto-proxy)
- [`promoted` in the file format](/reference/formats#promoted)
