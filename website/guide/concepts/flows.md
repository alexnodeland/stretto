---
description: What a flow is, what stretto learn puts in it, and how it is learned from recorded sessions by counting.
---

# Flows

A *flow* is what stretto serves behind an agent: a JSON file that says, for one server's tools, which lookups the agent made after each call, how often, and where each lookup's arguments came from. It holds counts, tool and argument names and JSON paths, and no model weights, so a person can read it, and a change to it can be reviewed like code.

## Learn one

```sh
stretto learn --sessions ~/.stretto/logs --domain orders --habit-only --out orders.flow.json
```

- `--sessions DIR` reads the session logs `stretto-proxy --record` wrote.
- `--domain NAME` names the flow's domain.
- `--habit-only` learns from counts alone and asks no model. Without it, `learn` also fits an *arbiter* on held-out sessions, which asks a System-One model and needs `TYPESAFE_API_KEY` ([deciders](./deciders)).
- `--manifest FILE` gives tool kinds when the server's `tools/list` has no `readOnlyHint` annotations ([tool kinds](./sessions#tool-kinds)).
- `--constants` also learns arguments the agent always passes the same way, such as a page size ([bindings](./bindings#lists-searches-and-constants)).

Every option is in [the CLI reference](/reference/cli#stretto-learn). `stretto compile` writes a flow from τ²-bench's published results instead, for research.

## What is in it

| Part | What it holds |
|---|---|
| `manifest` | The server's tools and their kinds: `read`, `write` or `generic`. A flow calls only `read` tools. |
| `habit` | Counts of what the agent did next after each short history of steps. |
| `reach` | The same histories, counted for whether each tool was called before the agent's next write. |
| `sites` | After each tool, the lookups the agent made next in training, and how often. They are the only lookups the flow can make there. |
| `bindings` | For each lookup's arguments, the earlier results and JSON paths their values came from, and how often binding them that way matched the agent. |
| `contracts` | Each tool's input contract as the server listed it. The proxy makes no lookup of a tool whose server now lists another. |
| `program` | The flow's run after each call, as a [fugue](https://github.com/alexnodeland/fugue) program. |
| `provenance` | What the flow learned from: the agent model the logs name, and how many successful sessions. |

A flow may also hold an arbiter (`folds`, `predicates`, `model`), code features (`map`), a promotion record (`promoted`) and per-site thresholds (`thresholds`). [File formats](/reference/formats) names every field and says what a reviewer should check. Flows are written on one line; `jq . orders.flow.json` prints one for reading, and `stretto flow-show` renders one for review.

## The counts

Each step of a session is abstracted to its tool, whether it returned or failed, and sometimes a feature of its result. The *habit* is a hierarchical Dirichlet back-off model of the next step given the last two, so that a history never seen in training falls back to a shorter one. With $c_j$ the last $j$ steps, $n(c_j, a)$ the times action $a$ followed them in training and $\alpha$ a concentration fitted with fugue:

$$P_j(a \mid c_j) = \frac{n(c_j, a) + \alpha\, P_{j-1}(a \mid c_{j-1})}{n(c_j) + \alpha}$$

The `reach` counts ask a different question of the same histories: not which action came next, but whether an action came at all before the agent's next write, $m(c_j, a)$ times of $n(c_j)$. A read's result stays current until the next write, so that is the chance a lookup made now answers a call the agent will make:

$$R_j(a \mid c_j) = \frac{m(c_j, a) + \alpha\, R_{j-1}(a \mid c_{j-1})}{n(c_j) + \alpha}$$

The chances need not sum to one: after finding a customer, an agent reads their record and then an order before it writes anything. [Deciders](./deciders) says how a flow uses each.

## Learning is counting

Every estimate in a flow is a posterior predictive of conjugate counts, so learning from another session adds its counts. There is no gradient and no training run to schedule, and learning is quick. That makes the loop cheap:

1. Serve a flow, and keep recording.
2. Learn again from the sessions recorded since, or from all of them.
3. Compare the new flow with the one being served, with [`stretto flow-diff`](./audit-and-review#compare-two-flows). It exits with 1 when the new flow can do something the old one could not, such as a new lookup or a new source for an argument.

Other agents' sessions can teach a flow too. Where agents act alike, they teach as much as the agent's own; where they do not, only the agent's own sessions teach its habits. Replayed in τ²-bench's telecom domain, a hundred of an agent's own sessions saved 15.2% of its turns, against 12.6% from all 1,184 of four other agents' ([the paper, §4.3](/research/paper#43-learning-from-few-sessions)).

## The run after each call

A flow's run after each of the agent's calls is a probabilistic program, in fugue's serializable format. `stretto flow-show` prints it:

```text
let prev = call;
let failed = call_failed;
for i in 0..max_lookups {
    let d <- sample(addr!("decide", i), Decide(prev, failed));
    if d == 0 {
        break;
    }
    let ok <- sample(addr!("outcome", i), Outcome(d));
    prev = d;
    failed = !ok;
}
pure(prev)
```

`Decide` is a decision between handing back (0) and the lookups offered after the call just made; `Outcome` is whether a lookup succeeds. The proxy decides each `decide#i` with the flow's decider and takes each `outcome#i` from the server. Because the run is a fugue program, the same flow can be executed against a server, simulated, and used to score a recorded session: that is how [`stretto audit`](./audit-and-review#audit-a-flow) works.

## Related

- [Lookups and detours](./lookups): what serving a flow does
- [Bindings](./bindings): where arguments come from
- [File formats](/reference/formats): every field
