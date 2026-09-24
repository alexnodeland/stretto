# Options from the manifest: letting Jev act where traces are silent

A flow acts only at sites where training shows a lookup, and offers only the lookups it saw there. With few traces that is little: [the sweep](sweep-2026-09-24.md)'s one-task retail flow knew 4 sites and 6 lookups, and its one-task airline flow 2 sites, where it saved nothing. Jev chooses among what the traces offer; it cannot add to it. This run lets the options come from the tool manifest instead: every read-only tool the server lists, at every site.

## Method

- **`--manifest-options`**, on `phase0`, `compile` and `learn`, offers every read-only tool at every site, including sites training never showed. The habit gives an unseen lookup little weight, and the arbiter weighs that against Jev's answer.
- **Binding.** A lookup training never made has no learned bindings, so the flow binds it by name. Each argument takes the first string under a key of that name in an earlier result, preferring one the customer mentioned. With no calls to go on, the chance that these are the agent's own arguments is 1/2. So such a lookup needs a probability of at least 0.6 to clear the 0.3 rule.
- **The flows.** The sweep's one- and three-task samples, retail and airline, were compiled again with manifest options. With every read-only tool at every site, a question no longer depends on the sample, so the arbiter's questions were asked once for every size. There are 4,696 retail decisions and 2,419 airline ones, against the sweep's 2,344–4,092 and 992, because every tool call is now a site. Each flow replays GLM-5's test episodes, all four trials, deciding by the arbiter at 0.3, as the sweep's did. These are the sweep's samples, so they are clustered (see [its correction](sweep-2026-09-24.md#correction-the-samples-are-clustered-not-random)), and the comparison is like for like.

## Results

Turns saved on GLM-5's test episodes, all four trials, by the arbiter at 0.3. Differences are in points, with manifest options minus without, paired by episode and bootstrapped over tasks (4,000 draws).

| | Training tasks | Without: turns saved (lookups, detours) | With manifest options | Difference, 95% interval | Lookups at a site where training never showed them |
|---|---|---|---|---|---|
| Retail | 1 | 10.0% (439, 28) | 10.0% (432, 23) | +0.1 (−0.3 to +0.5) | 0 of 432 |
| Retail | 3 | 20.4% (578, 26) | 20.3% (641, 54) | −0.1 (−0.8 to +0.6) | 62 of 641 |
| Airline | 1 | 0.0% (11, 0) | 1.1% (106, 83) | +1.1 (0.0 to +2.7) | 95 of 106 |
| Airline | 3 | 5.6% (169, 1) | 3.7% (247, 91) | −1.9 (−4.4 to +0.2) | 89 of 247 |

- **Retail: nothing gained.** The arbiter keeps to the lookups training showed. It gives the habit's prediction positive weight, and the habit gives an unseen lookup little probability. It saves the same turns, and with three tasks makes twice the detours.
- **Airline: a little reach, many detours.**
  - The fitted arbiter gives the habit almost no weight (−0.04 with one task and 0.00 with three), so Jev's picks decide.
  - Most of the new lookups are flight searches. One task's flow made 75 `search_direct_flight` calls, never seen in training, and the three-task flow made as many again. Bound by name, each argument takes the first value under its name in an earlier result, on its own. A search then gets its origin, destination and date from the flights just returned or from the reservation, not from the trip the customer wants. Sometimes the values come from different records, as in a search from ORD to ORD. So nearly every one is a detour: 83 and 91 of them, against 0 and 1 without manifest options.
  - With one task it saves 1.1% of turns where the flow saved none. With three it saves less than before.

## What this means

- **The options are not what limits a flow on few traces. Binding is.** Jev can pick a lookup that no trace showed, but the flow still has to fill in its arguments. The traces are what teach it that, for example, a customer's orders come from `get_user_details` at `$.orders[*]`. Binding by name fills a search with the wrong trip.
- **Manifest options stay off by default.** `--manifest-options` remains for experiments. A flow's reach grows with its traces, as [the cold start](cold-start-2026-09-24.md) shows. Five sessions of the agent's own already cover the lookups it makes most.

## Cost

No LLM ran. Jev answered 9,100 new questions, 27.8M input tokens, $1.17. The arbiter's questions were 6,955 of them ($0.89), and the replays' were 2,145 ($0.28). They are in [this round's answer bundle](answers-2026-09-24-cold-manifest-match.md).

## Reproduce

With the answer bundles imported, name each of the sweep's samples (retail: `109`, and `104,105,109`; airline: `49`, and `42,43,49`; see [the samples](sweep-2026-09-24-samples.json)):

```sh
stretto compile --tau2 ../tau2-bench --domain retail --oracle replay --oracle-cache .oracle-cache --questions v2 \
  --predicates data/predicates-v2.json --manifest-options --train-tasks 104,105,109 --out flows/manifest-retail-F.flow.json
cd pilot
python check_flow.py --domain retail --flow ../flows/manifest-retail-F.flow.json --trials 0 1 2 3 \
  --results ../.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json \
  --oracle-cache ../.oracle-cache --flow-oracle replay --flow-decider arbiter --flow-threshold 0.3 --out runs/manifest-retail-F
```

Per-episode rows are in [manifest-options-2026-09-24.json](manifest-options-2026-09-24.json).
