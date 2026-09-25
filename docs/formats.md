# File formats

Two kinds of stretto file are meant to be kept, reviewed and shared:

- **a flow** (`*.flow.json`), written by `stretto compile` and `stretto learn` and served by `stretto serve`, `stretto flow-serve` and `stretto-proxy --flow`;
- **an arbiter** (`data/arbiters/*.json`), written by `stretto export-arbiter` and read by `stretto learn --arbiter-from`.

Both are JSON. This page names every field, says what sets it, and says what a reviewer should check. The proxy's session logs are documented in [its README](../crates/stretto-proxy/README.md#log-format). The Rust types are `Flow`, `Arbiter` and `Fitted` in `crates/stretto-report/src/flow.rs` and `arbitrate.rs`; a test (`crates/stretto-report/tests/formats.rs`) fails when a serialized field is missing from this page.

Flows are written on one line. `jq . some.flow.json` prints one for reading. Maps whose keys are not strings are written as lists of `[key, value]` pairs sorted by key, so two flows compiled from the same data differ only in `compiled_unix_ms`.

## Reviewing a flow

A flow makes read-only calls on the agent's behalf, so a review asks what it may call, when, and with what:

1. **`manifest.tools`.** The flow calls only tools marked `read`. A tool that writes but is marked `read` is the one mistake that matters here. Kinds come from the server's `readOnlyHint` annotations or a `--manifest` file, so check them against what the tools do.
2. **`sites.next`.** After each call, these are the lookups the flow may make next, with how often the agent made each in training. A lookup seen once or twice is a thin basis.
3. **`bindings.sources`.** These say where each lookup's arguments come from. A required argument with no source, such as an email only the customer knows, means the flow never makes that lookup itself.
4. **`provenance`.** Check which sources the flow learned from, how many successful episodes (`habit_episodes`) and how many held-out decisions (`arbiter_cases`). The [cold start](results/cold-start-2026-09-24.md) found an arbiter fitted on a handful of decisions worse than none.
5. **`map.ids`.** Holds values copied from training outputs, such as user ids. Treat a flow with features like the data it was learned from before sharing it. The rest of the file holds tool names, argument names, JSON paths, counts, the tools' documentation, and the questions' text.

## The flow IR (`stretto_flow: 1`)

| Field | What it holds |
|---|---|
| `stretto_flow` | The format version, 1 |
| `provenance` | Where the flow came from |
| `vocab` | The actions the habit predicts, in id order |
| `manifest` | The domain's tools, their kinds and their documentation |
| `map` | Code features: tool output fields that sharpen the habit |
| `group` | The habit's condition for a live episode |
| `habit` | The habit: counts of what the agent did next after each short history |
| `sites` | The lookups the flow may make after each call |
| `predicates`, `weighed` | The yes/no questions asked with each next-step question, and those the arbiter weighs |
| `folds` | The arbiter's fitted weights and Jev's record, one per fold of tasks |
| `bindings` | Where each lookup's arguments came from in training, and how often binding them that way matched the agent |
| `model` | The System-One model the flow asks |

### `provenance`

- `stretto`: the version of stretto that wrote the flow.
- `sources`: the training sources, by label. For `compile` these are τ²-bench's agent models. For `learn` it is the agent model the session logs name: the proxy's `--agent-model`, or else the name the host gave in `initialize`. A flow serving an arbiter from elsewhere also lists that arbiter's sources, as `arbiter (<domain>): <source>` when the arbiter came from a file and `arbiter: <source>` when it came from another flow.
- `habit_episodes`: the successful training episodes the habit learned from.
- `arbiter_cases`: the held-out decisions the arbiter was fitted on; 0 for a flow with no arbiter. A flow serving a shipped arbiter carries that arbiter's count.
- `compiled_unix_ms`: when the flow was written, in milliseconds since the Unix epoch.

### `vocab`

The actions the habit predicts: `"Respond"` (a message to the customer with no tool call) first, then `{"Tool": name}` for each tool, sorted. An action's id is its position. One more id, the list's length, stands for any action not in the list. The habit's counts use these ids.

### `manifest`

- `domain`: the domain's name, from `--domain`.
- `tools`: each tool's kind: `read` (reads without changing anything), `write` (changes state) or `generic` (neither, or unknown). `compile` takes kinds from τ²-bench's tool types. `learn` takes them from the servers' `readOnlyHint` annotations in the recorded `tools/list`, or from `--manifest`. A tool that gave no hint is `generic`. Only `read` tools are ever looked up by the flow.
- `docs`: each tool's documentation, where the source had any: `summary`, one paragraph, and `args`, each argument's description. The System-One questions show it.

### `map`

Code features ([RFC-001 §3.3](rfc/001-habit-compiler.md)): values in tool outputs that change what the agent does next, such as whether an order has one item or several. `compile` and `learn` choose them from training outputs. A flow learned from a few sessions often has none; `compile --no-features` skips them.

- `fields`: for each tool, the output fields read: `{"Scalar": path}`, a scalar at a JSON path such as `status` or `address.state`, or `{"Len": path}`, whether an array at a path (`""` for the whole output) is empty, has one element or more (`"0"`, `"1"`, `"2+"`).
- `ids`: `[[tool, values], id]`: each combination of values seen in training, per tool, and its feature id (1 to 63). Combinations never seen, and those past 63, share id 0, "no feature".

Check: `ids` holds training values verbatim.

### `group`

The habit's condition for a live episode. The habit can be conditioned on an episode's goal, but a flow serves sessions nobody has named, so flows are compiled goal free: every training episode has group 1, and so does every live one.

### `habit`

The habit: a hierarchical Dirichlet back-off model of the agent's next action, given its last `order` steps (RFC-001 §3.3; `stretto_model::world`). With `c_j` the last `j` steps,

```text
P_j(a | c_j) = (n(c_j, a) + α · P_{j-1}(a | c_{j-1})) / (n(c_j) + α)
```

and `P_{-1}` uniform over the vocabulary, so a history never seen in training falls back to a shorter one.

- `base.order`: the steps of history, 2.
- `base.alpha`: the concentration α, shared by every level. It is the posterior median given the successful training episodes, from `--alpha-samples` Metropolis–Hastings draws (with fugue), or `--alpha` when that is 0.
- `base.vocab_size`: the number of action ids, the vocabulary's length plus one.
- `base.levels`: one list per history length, 0 to `order`. Each entry is `[history, counts]`, where `counts` is `{"by_action": [[action id, count], …], "total": count}`. A history is a list of step symbols; a symbol is `(action id × 4 + outcome) × 4096 + feature id`, where the outcome is 0 for a tool call that succeeded, 1 for one that failed, 2 for a message the customer answered and 3 for one that ended the episode. So `8192` is a message to the customer that they answered.
- `beta`: the concentration of the layer conditioned on `group`, equal to α.
- `top`: that layer's counts, `[[group, history], counts]`, over histories of `order` steps. In a goal-free flow they repeat the longest level's.

### `sites`

- `reads`: the read-only tools, from the manifest.
- `next`: `[[tool, failed], {lookup: count}]`: after a call to `tool` that failed (`true`) or not, the lookups the agent made next in training, and how often. They and handing back are the options at that site. At a site with no lookups the flow hands back without asking.
- `feeds`: with `compile --dataflow-hints`, for each lookup, `[[write, argument], count]`: how many training episodes passed a value from its results to that write argument. The System-One questions describe each lookup by the two arguments it supplied most often ("Its results supply `order_id` for `cancel_pending_order`"). Empty otherwise.
- `every_read`: written only when true, with `--manifest-options`: every read-only tool is offered at every site, not only the lookups seen there.

### `predicates` and `weighed`

The yes/no questions asked with each next-step question ([RFC-001 §3.4](rfc/001-habit-compiler.md); `data/predicates-v2.json`), and those of them the arbiter weighs. Each has an `id` (asked as `pred_<id>`), `favors` (the options its answer bears on: `same_lookup`, the lookup just made, made again; `any_lookup`, every lookup; `hand_back`), the `question`, and what `yes` and `no` mean. With `--no-predicate-features` they are asked but not weighed, and `weighed` is empty. A flow with no arbiter has neither.

### `folds`

The arbiter ([RFC-001 §3.6](rfc/001-habit-compiler.md#36-execution-arbitration-not-pooling)): for each option at a site, `P(option) ∝ exp(Σ weights[i] · x[i])`, times the site's coverage. The flow makes the most likely lookup when that probability, times its binding's chance (see `bindings`), reaches the serving threshold; otherwise it hands back. A compiled flow has five arbiters, one per fold of tasks, each fitted on the other folds' held-out decisions, and a task is judged by its fold's. A flow compiled with `--pooled-arbiter`, learned from sessions, or serving a shipped arbiter has five copies of one fit. A flow with no arbiter (`learn --habit-only`) has none, and decides with the habit alone.

- `weights`: the conditional logit's weights, in this order:
  1. the log of the habit's probability of the option (every step the flow cannot take counts as handing back);
  2. the log of the System-One model's probability for it in the next-step question, a choice among the options;
  3. the log of its probability from the split questions: whether to go on at all, then which lookup;
  4. 1 for handing back;
  5. one for each weighed predicate: the log-odds of its yes, on the options it favors;
  6. last, the reliability weight: the log-odds of Jev's agreement with the agent at this site, on Jev's pick.

  With the three predicates of `data/predicates-v2.json`, that is eight weights.
- `rates.sites`: `[[site, [decisions, agreed, covered]], …]`: for each site (a tool, with ` (error)` after its name when the call failed), the held-out decisions there, how many of them Jev's pick matched the agent's step, and how many offered the agent's step among the options.
- `rates.agree` and `rates.cover`: the same shares over every site. A site's reliability is `(agreed + 10 · agree) / (decisions + 10)`, and its coverage `(covered + 10 · cover) / (decisions + 10)`. At a site the arbiter never saw, such as any site of another domain, they are `agree` and `cover`.

### `bindings`

How the flow fills a lookup's arguments, learned from the agent's own lookups in training.

- `args`: for each lookup, `[calls, {argument: calls that passed it}]`. An argument passed in at least 90% of the calls is required. The flow passes only required arguments.
- `sources`: `[[lookup, argument], {"values": n, "found": [[[tool, path], count], …]}]`: of the `n` string values the argument took, how many were found in an earlier successful output of `tool` at the JSON path `path` (`$` is the whole output, `[*]` any element). The flow binds each required argument to the first value at one of its sources that it has not passed already, starting with the most recent output. It prefers a value the customer mentioned, or one whose record they mentioned by another of its fields. It uses only sources found at least twice that account for at least 10% of the values. A lookup with no required arguments is made once per session.
- `agreed`: for each lookup, `[[agreed, calls], [agreed, calls]]`: at the agent's own lookups in training, how often the binding picked the agent's arguments, first when the customer had not mentioned the values picked, then when they had. The binding's chance is `(agreed + 1) / (calls + 2)`.

### `model`

The System-One model id the flow requests, `jev-latest` by default. It is part of each question's cache key. Empty in a flow with no arbiter.

## A flow, walked through

[The live cold start's flow](results/cold-start-2026-09-24-live.flow.json) was learned from five retail sessions GLM-5.3 ran through the proxy, and it served the three tasks of [the cold start's live run](results/cold-start-2026-09-24.md).

- **`provenance`**: `sources` is `["glm-5.3"]`. The habit learned from 3 successful sessions; the arbiter was fitted on the other sessions' 15 decisions.
- **`manifest`** lists retail's 16 tools: 7 `read`, 7 `write` and 2 `generic` (`calculate`, `transfer_to_human_agents`).
- **`map`** is empty: three sessions support no code feature.
- **`sites.next`** has five sites. After `find_user_id_by_email` the agent called `get_user_details` (once). After `get_user_details` it called `get_order_details` (3 times). After `get_order_details` it called `get_order_details` 7 times and `get_product_details` once. Those are the only lookups the flow can make.
- **`bindings.sources`**:
  - `order_id` for `get_order_details` came from `get_user_details` at `$.orders[*]`, all 10 times;
  - `product_id` came from `get_order_details` at `$.items[*].product_id`, 4 of 4;
  - `user_id` came from `find_user_id_by_name_zip` at `$` twice and from `find_user_id_by_email` once. A source found once is not used, so after `find_user_id_by_email` the flow cannot bind `get_user_details` and hands back.

  The email, name and zip code were never found in an output, since they come from the customer, so the flow never finds the user itself.
- **`bindings.agreed`**: the binding picked the agent's own order id at all 10 of its `get_order_details` calls, a chance of 11/12. For `get_product_details` it matched 2 of 4 times, all where the customer had mentioned the product, a chance of 3/6.
- **`folds`** holds five copies of one fit. Jev's pick matched the agent at 9 of the 15 held-out decisions (`agree` 0.6), and the agent's step was always among the options (`cover` 1.0). At `get_order_details` Jev matched 3 of 9, so the arbiter trusts it less there than elsewhere.

## The arbiter file (`stretto_arbiter: 1`)

A flow's arbiter on its own, to serve with a habit learned elsewhere ([data/arbiters](../data/arbiters/README.md)). `stretto export-arbiter` writes it from a flow whose folds share one fit, and refuses a flow whose five folds were fitted apart, since none of those is the arbiter for new tasks. It is pretty-printed.

| Field | What it holds |
|---|---|
| `stretto_arbiter` | The format version, 1 |
| `domain` | The domain whose decisions it was fitted on |
| `provenance` | The flow it came from, as in a flow |
| `predicates`, `weighed` | The questions it asks and weighs, as in a flow |
| `fitted` | One fit: `weights` and `rates`, as in one of a flow's `folds` |
| `model` | The System-One model it asks |

`learn --arbiter-from` gives the learned flow the arbiter's `predicates`, `weighed` and `model`, five copies of `fitted`, its `arbiter_cases`, and its sources. The file holds no answer or conversation text, only counts, weights and the questions.

## Versions

Every reader checks the version field first and refuses any other version, with a message naming both. Nothing has been released yet, and fields added so far, such as `every_read`, kept version 1. From the first release on:

- **A new version** comes with any change after which one build would read another's file wrongly. That covers a field removed, renamed, or given a new meaning or encoding (the symbols, the vocabulary's ids, the order of the weights). It also covers a new field an older build would ignore but must not, as it would ignore `every_read` and offer fewer lookups.
- **The same version** holds for a new field that an older build can ignore without acting differently, read with a default that keeps today's behavior.
- **Old files.** Before 1.0, a build reads only its own version, and the release notes say which version each release reads. A flow is cheap to learn again from the recorded sessions or results it came from, so a new version may not convert old files. A shipped arbiter is rebuilt from the published answer bundles ([data/arbiters](../data/arbiters/README.md#rebuild-them)).
