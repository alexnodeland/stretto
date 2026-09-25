# Reviewing flows

A flow makes read-only calls on the agent's behalf. Before one is served, and whenever it is learned again, someone should see what it may call, after which calls, where the arguments come from, and whom it asks. The flow file holds all of that ([every field](formats.md)), but as habit counts and fold weights. Two commands say it plainly:

- `stretto flow-show FLOW` renders one flow for review, as Markdown.
- `stretto flow-diff OLD NEW` lists what changed between two flows, for a pull request. It exits with 1 when a change needs a reviewer.

The examples below are real flows, in [`examples/`](examples/). They were learned from GLM-5's published τ²-bench retail trajectories, the [cold start](results/cold-start-2026-09-24.md)'s samples of five and ten sessions.

## `stretto flow-show`

```sh
stretto flow-show docs/examples/retail-5-sessions.flow.json
```

<!-- begin show-5 -->

```markdown
# Flow: retail

Written by stretto 0.0.1 from openai/glm-5-fp8. The habit learned from 3 successful sessions or episodes. It has no arbiter: it decides with the habit alone and asks no one.

## Tools

- **Read, the only tools it may call:** `find_user_id_by_email`, `find_user_id_by_name_zip`, `get_item_details`, `get_order_details`, `get_product_details`, `get_user_details`, `list_all_product_types`
- **Write, never called:** `cancel_pending_order`, `exchange_delivered_order_items`, `modify_pending_order_address`, `modify_pending_order_items`, `modify_pending_order_payment`, `modify_user_address`, `return_delivered_order_items`
- **Neither, never called:** `calculate`, `transfer_to_human_agents`

## Run

What the flow does after each call, as a fugue program (`program`): `Decide` is a decision between handing back (0) and the lookups offered after the call just made, and `Outcome` whether a lookup succeeds. The proxy decides with the arbiter and takes each outcome from the server. This is the standard run: decide, look up, and decide again, until the flow hands back or has made `max_lookups` lookups (`--flow-per-call`).

~~~text
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
~~~

## Sites

After each call: the lookups the flow may make next, what the agent did next in training, and what the flow does there with the habit alone at a threshold of 0.3, which is the likeliest lookup's share times the chance its bound arguments are the agent's. The share pools over the calls before and the code features, so a live decision near the threshold can go either way.

| After | Lookups it may make next (times seen) | What the agent did next in training | With the habit alone |
|---|---|---|---|
| `find_user_id_by_email` | `get_user_details` (2) | get_user_details 100% (of 2) | looks up `get_user_details` 1.00 × 0.75 = 0.75 |
| `find_user_id_by_name_zip` | `get_order_details` (1) | get_order_details 100% (of 1) | looks up `get_order_details` 1.00 × 0.90 = 0.90 |
| `get_order_details` | `get_order_details` (6), `get_product_details` (2) | get_order_details 67%, get_product_details 22%, respond 11% (of 9) | looks up `get_order_details` 0.67 × 0.90 = 0.60 |
| `get_user_details` | `get_order_details` (2) | get_order_details 67%, respond 33% (of 3) | looks up `get_order_details` 0.67 × 0.90 = 0.60 |

## Bindings

Where each lookup's required arguments come from, and how often binding them that way gave the agent's own arguments in training, when the customer had not mentioned the values and when they had. With nothing tried, the chance is 1/2.

| Lookup | Argument | Bound from (values found there, of the values it took) | Chance it is the agent's (right/tried): not mentioned, mentioned |
|---|---|---|---|
| `find_user_id_by_email` | `email` | nothing: the flow never makes this lookup | — |
| `find_user_id_by_name_zip` | `first_name` | nothing: the flow never makes this lookup | — |
| `find_user_id_by_name_zip` | `last_name` | nothing: the flow never makes this lookup | — |
| `find_user_id_by_name_zip` | `zip` | nothing: the flow never makes this lookup | — |
| `get_order_details` | `order_id` | `get_user_details` at `$.orders[*]` (8 of 9) | 0.90 (8/8), 0.50 (0/0) |
| `get_product_details` | `product_id` | `get_order_details` at `$.items[*].product_id` (3 of 3) | 0.50 (0/0), 0.60 (2/3) |
| `get_user_details` | `user_id` | `find_user_id_by_email` at `$` (2 of 3) | 0.75 (2/2), 0.50 (0/0) |
```

<!-- end show-5 -->

What to check, section by section:

- **Tools.** The flow calls only the read tools. A tool that writes but is marked read is the one mistake that matters here. Kinds come from the server's `readOnlyHint` annotations or a `--manifest` file, so check them against what the tools do.
- **Sites.** After each call: the lookups the flow may make next, how often the agent made each in training, and what the flow does there with the habit alone. That last column uses the threshold the flow will be served with (`--threshold`, as `stretto-proxy --flow-threshold`). It pools over what came before the call, so a live decision near the threshold can go either way. A lookup seen once or twice is a thin basis.
- **Bindings.** Where each required argument comes from, and how often that way gave the agent's own arguments. "Nothing" means the flow never makes that lookup itself. Here, `find_user_id_by_email` needs an email only the customer knows.
- **Code features**, when a flow has them, hold values copied from training outputs. Treat such a flow like the data it was learned from.
- **Arbiter.** A flow with an arbiter asks a System-One model at each decision and sends it the conversation so far. The section shows the arbiter's weights, the model's record, and the text of each yes/no predicate it asks alongside.

## `stretto flow-diff`

```sh
stretto flow-diff OLD NEW
```

The diff speaks in the same terms as `flow-show` and lists first what needs a reviewer: every way the new flow can do something the old one could not.

| Change | Needs review |
|---|---|
| A tool newly marked read-only | Yes: the flow may now call it |
| A lookup offered after a call where it was not | Yes |
| Every read tool offered after every call (`--manifest-options`) | Yes |
| An argument bound from a new source (tool and JSON path) | Yes |
| An arbiter added, another System-One model, or new or reworded predicates | Yes: the flow sends the conversation to a model, or asks it something new |
| A site newly promoted, or a promotion lifted (`stretto promote`) | Yes: the flow acts after a call where it handed back |
| Lookups, sources or an arbiter removed, a tool no longer read-only, a site no longer promoted | No: the flow does less |
| What the flow does after each call with the habit alone, at `--threshold` (0.3) | No; listed |
| Shares, binding chances and arbiter weights that moved by `--tolerance` (0.05) or more | No; listed |
| Code features, the promotion's bar, and provenance | No; listed |

The exit status is 0 when nothing needs review, 1 when something does and 2 on an error, as with `diff`. A CI job can post the diff on a pull request that changes a flow, and ask for a review when it exits with 1:

```sh
git show origin/main:flows/orders.flow.json > old.flow.json
stretto flow-diff old.flow.json flows/orders.flow.json > flow-diff.md
```

### Five sessions, then ten

```sh
stretto flow-diff docs/examples/retail-5-sessions.flow.json docs/examples/retail-10-sessions.flow.json
```

<!-- begin diff-5-10 -->

```markdown
# Flow diff: retail

**Needs review:**

- a new lookup: `get_user_details` after `find_user_id_by_name_zip`
- a new lookup: `get_product_details` after `get_item_details`
- a new lookup: `get_product_details` after `get_product_details`
- a new binding: `get_user_details`'s `user_id` from `find_user_id_by_name_zip` at `$`

## Sites

- after `find_user_id_by_name_zip`: may now look up `get_user_details`
- after `get_item_details`: may now look up `get_product_details`
- after `get_product_details`: may now look up `get_product_details`

## With the habit alone, at 0.3

- after `find_user_id_by_name_zip`: looks up `get_user_details` 0.75 × 0.90 = 0.68 (was: looks up `get_order_details` 1.00 × 0.90 = 0.90)
- after `get_item_details`, a new site: looks up `get_product_details` 1.00 × 0.36 = 0.36

## What the agent did next in training

- after `find_user_id_by_name_zip`: get_order_details 100% → 25%
- after `find_user_id_by_name_zip`: get_user_details 0% → 75%
- after `get_order_details`: get_product_details 22% → 11%
- after `get_order_details`: respond 11% → 18%
- after `get_user_details`: get_order_details 67% → 88%
- after `get_user_details`: respond 33% → 12%

## Bindings

- `get_order_details`'s chance (not mentioned): 0.90 → 0.97
- `get_product_details`'s chance (mentioned): 0.60 → 0.36
- `get_user_details` binds `user_id` from `find_user_id_by_name_zip` at `$`
- `get_user_details`'s chance (not mentioned): 0.75 → 0.90

## Provenance

- the habit learned from 3 → 8 successful sessions or episodes
```

<!-- end diff-5-10 -->

Five more sessions taught the flow three lookups and one binding it did not have. Two deserve a question. `get_product_details` after `get_item_details` rests on a single call in training. It clears the threshold at 0.36 only because the agent made it the one time it could. `get_product_details` after another `get_product_details` rests on two calls. The habit rarely takes it (0.08, below the threshold), but the flow may. The binding of `get_user_details`' `user_id` from `find_user_id_by_name_zip`'s result is the same kind as the one from `find_user_id_by_email`, which the flow already had. Everything else moved within what the flow could already do.

### The habit alone, then with the shipped arbiter

```sh
stretto flow-diff docs/examples/retail-5-sessions.flow.json docs/examples/retail-5-sessions-shipped-arbiter.flow.json
```

<!-- begin diff-shipped -->

```markdown
# Flow diff: retail

**Needs review:**

- the flow now asks `jev-latest` at each decision, with the predicates `list_pending`, `needs_options`, `must_ask`

## Arbiter

- now has an arbiter, fitted on 4,513 held-out decisions, which asks `jev-latest`, with the predicates `list_pending`, `needs_options`, `must_ask`

## Provenance

- sources: openai/glm-5-fp8 → openai/glm-5-fp8, arbiter (retail): claude-3-7-sonnet-20250219, arbiter (retail): gpt-4.1-2025-04-14, arbiter (retail): gpt-4.1-mini-2025-04-14, arbiter (retail): o4-mini-2025-04-16
- held-out decisions behind the arbiter: 0 → 4,513
```

<!-- end diff-shipped -->

`learn --arbiter-from data/arbiters/retail.json` adds [a shipped arbiter](../data/arbiters/README.md) to the same habit. The tools, lookups and bindings do not change. Who decides does: at each decision the flow now asks Jev which lookup comes next, and three yes/no predicates, sending it the conversation so far. A reviewer should approve that, and the flow needs `TYPESAFE_API_KEY` to serve.

## Reproduce

The example flows, from GLM-5's published results (`scripts/fetch-leaderboard.sh`) and a τ²-bench checkout, with no key:

```sh
R=.data/tau2-targets/glm-5_enabled_retail_gpt-5.2_4trials.json
learn() { stretto learn --tau2 ../tau2-bench --domain retail --results $R --trials 0 "$@"; }
learn --train-tasks 104,105,106,107,109 --habit-only --out retail-5-sessions.flow.json
learn --train-tasks 98,103,104,105,106,107,109,110,112,113 --habit-only --out retail-10-sessions.flow.json
learn --train-tasks 104,105,106,107,109 --arbiter-from data/arbiters/retail.json \
  --out retail-5-sessions-shipped-arbiter.flow.json
```

A test checks that this page shows what `flow-show` and `flow-diff` print for them. After changing either, `STRETTO_BLESS=1 cargo test` rewrites the examples.
