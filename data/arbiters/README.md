# Shipped arbiters

A flow's arbiter decides each branch by weighing the habit's prediction against Jev's answers ([RFC-001 §3.6](../../docs/rfc/001-habit-compiler.md#36-execution-arbitration-not-pooling)). With a deployment's first few sessions there are too few decisions to fit one: [the cold start](../../docs/results/cold-start-2026-09-24.md) found an arbiter fitted on 3 to 11 of them did worse than the habit alone. These two were fitted in advance, where decisions are plentiful, to serve with a habit learned from new sessions:

```sh
stretto learn --sessions ~/.stretto/logs --domain orders --arbiter-from data/arbiters/retail.json \
  --out ~/.stretto/orders.flow.json
```

`--arbiter-from` asks Jev nothing while learning. The flow asks Jev at each decision when it serves, with `--flow-decider arbiter`.

| File | Fitted on | Held-out decisions |
|---|---|---|
| [`retail.json`](retail.json) | τ²-bench retail | 4,513 |
| [`airline.json`](airline.json) | τ²-bench airline | 2,042 |

- **The decisions** are those of τ²-bench's published 2025 trajectories of four agents (Claude 3.7 Sonnet, GPT-4.1, GPT-4.1 mini and o4-mini) on the test tasks, with Jev's answers from the published bundles.
- **Each file holds** the eight fitted weights, Jev's agreement with the agents at each tool (per-site counts), the three yes/no questions it weighs (`data/predicates-v2.json`), and the model it asks. It holds no answer or conversation text. [docs/formats.md](../../docs/formats.md#the-arbiter-file-stretto_arbiter-1) describes every field.
- **At a site it never saw**, as at any tool of another domain, the arbiter weighs Jev's answers by their agreement with the agents over all the sites it was fitted on.

## Which to use

[Tested across domains](../../docs/results/arbiter-transfer-2026-09-24.md), each did in the other's domain what that domain's own arbiter did, within about a point of turns saved. With a retail habit from five sessions, the airline arbiter matched the habit alone on random draws and lifted a narrow draw from 2.7% to 10.6% of turns. With an airline habit, the retail arbiter saved 1 to 3 points less than the habit alone and made far fewer unneeded lookups, as airline's own does. For a domain like these, either serves; `retail.json` was fitted on twice the decisions.

Neither carries to a domain unlike both. [In telecom](../../docs/results/telecom-2026-09-25.md), where agents look things up at most decisions and Jev agrees with them half the time, both handed back too often: at its held-out decisions they made 43–50% of the agent's lookups, against 66% for the habit alone and 75% for telecom's own arbiter. An arbiter fitted on retail and airline together (`stretto fit-arbiter`) did no better, so it does not ship. In a new kind of domain, start on the habit alone (`learn --habit-only`) and fit the domain's own arbiter as its sessions accumulate.

## Rebuild them

From the published answer bundles (see [docs/results](../../docs/results/)), with no key:

```sh
stretto compile --tau2 ../tau2-bench --domain retail --oracle replay --oracle-cache .oracle-cache \
  --questions v2 --predicates data/predicates-v2.json --pooled-arbiter --out retail.flow.json
stretto export-arbiter --flow retail.flow.json --out data/arbiters/retail.json
```

`--pooled-arbiter` fits one arbiter on every held-out decision, where a compiled flow otherwise gets five, each fitted without one fifth of the tasks. Those five cannot ship, since none is the arbiter for new tasks, and `export-arbiter` refuses them. The file differs from the one here only in `compiled_unix_ms`.

## Terms

The weights and counts were fitted on Jev's answers. Under TypeSafe's Master Customer Agreement (§2.3(b)), Jev's outputs may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
