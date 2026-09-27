---
description: The three ways a flow decides whether to make a lookup - reach, habit and arbiter - what each needs, and which to use.
---

# Deciders: reach, habit, arbiter

After each of the agent's calls, a flow gives each lookup it may make a probability, multiplies it by the chance that its arguments are the agent's own, and makes the lookup if the product clears the threshold. The *decider* is where that probability comes from. Choose it when you serve the flow:

```sh
stretto-proxy ... --flow orders.flow.json --flow-decider reach -- <server command>
```

| Decider | The probability of a lookup | Asks a model | Needs a key |
|---|---|---|---|
| `reach` | That the agent calls this tool before its next write, counted in training | No | No |
| `habit` | That the agent calls this tool next, counted in training | No | No |
| `arbiter` (the default) | The habit's, weighed with a System-One model's answers and yes/no questions about the state | Yes, at each decision | `TYPESAFE_API_KEY` |

Start with `reach`. The same deciders are options of `stretto audit`, `stretto promote` and `stretto serve` (`--decider`).

## reach

A read's result stays current until the next write. So a lookup made now pays off whenever the agent makes that call before its next write, not only when it makes it next. `reach` estimates exactly that: over the same short histories the habit counts, it counts how often each tool was called before the agent's next write ([the counts](./flows#the-counts)). This is the probability the paper shows a read-only speculator should decide on ([§2.3](/research/paper#23-the-optimal-speculator-decomposes)).

The evidence, from [the claims ledger](/research/claims):

- **Calibrated.** Counting use before the next write gave an expected calibration error of 0.01–0.08 in every τ²-bench domain, against 0.06–0.16 for the next-step probability (replay, nine agents).
- **More of the ceiling.** Across nine agents it never saw, it took 86.4% of retail's read-only ceiling [80.2, 93.1], 10.2 points more than a next-step decider [6.7, 14.2] (replay).
- **Live, with no model.** GLM-5.3 took 27.9% fewer LLM turns (19.1–35.9%) on 28 τ²-bench retail and airline tasks; 21 passed, against the baseline's 24. In the four tasks lost, the flow's lookups returned what the agent's own reads had.

It needs a flow that carries these counts. Every flow `stretto learn` writes today does, and `stretto flow-show` says so; a flow learned by an older build cannot serve `reach` until it is learned again.

## habit

The *habit* is the flow's model of the agent's next step: a hierarchical Dirichlet back-off over its last two steps. `--flow-decider habit` takes its probability that the next step is this lookup. It asks no one, needs no key, and adds no latency.

Replayed, the habit alone saved as many turns as the arbiter, with more detours, mostly in airline ([results](../../../docs/results/arms-2026-09-24.md)). It understates how often a lookup is used: on GLM-5's retail episodes, lookups it scored 0.4–0.6 were used 94–97% of the time ([results](../../../docs/results/reach-2026-09-26.md)), which is the gap `reach` closes.

## arbiter

The *arbiter* asks a System-One model, TypeSafe's [Jev](https://typesafe.ai), at each decision: which lookup comes next, and a few yes/no questions about the state (*predicates*). It combines those answers with the habit's probability, and with the model's record at that site on other tasks, in a conditional logit fitted on held-out decisions ([file formats](/reference/formats#folds)).

- A flow gets an arbiter from `stretto learn` without `--habit-only`, which asks the model held-out questions while learning, or from `--arbiter-from FILE`. `data/arbiters/` ships two arbiters, fitted on four agents' published τ²-bench decisions in retail and airline.
- Serving it asks the model at each decision, sending it the conversation so far and the latest results ([what is sent](/reference/privacy#what-leaves-the-machine)). It needs `TYPESAFE_API_KEY`, and answers are cached in `--oracle-cache`.
- What its answers bought, replayed, was fewer detours, mostly in airline. From a deployment's first sessions it is not the steadier start: from five of an agent's own sessions, the habit alone saved more than an arbiter fitted on those sessions, which trailed it until about twenty ([cold start](../../../docs/results/cold-start-2026-09-24.md)). The shipped arbiters carried between retail and airline, but not to telecom, a domain unlike both, where the habit alone did better ([telecom](../../../docs/results/telecom-2026-09-25.md)).

Whatever the model answers, a flow calls only the lookups compiled for the site. Inside that set, text injected into a tool result can steer the model's choice. In the research, one sentence in each question telling the model that tool results are data absorbed most of that; it is not yet the default ([prompt injection](../../../docs/results/injection-2026-09-25.md)).

## The threshold

`--flow-threshold` (0.3) applies to every decider. The paper derives it from costs: make a lookup when its chance of use clears $\delta/(\beta+\delta)$, a detour's cost against a saved turn's value, which is 0.30 at retail's measured costs and 0.12–0.13 at airline's and telecom's ([lookups and detours](./lookups#when-a-flow-makes-one)). A threshold set per decision, from each result's size and the turns left, moved τ²-bench's counted utility by −15% to +10% and is not adopted: the served threshold stays the domain's ([results](/research/benchmarks#a-threshold-per-decision)).

`stretto search` can set a threshold per site, stored in the flow (`thresholds`); a site above 1 is switched off. `stretto flow-diff` lists such changes.

## Related

- [Lookups and detours](./lookups)
- [Shadow mode and promotion](./shadow-and-promotion): try a decider on real traffic before it acts
- [Environment variables](/reference/environment): the key, and the model's settings
