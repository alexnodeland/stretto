---
description: How a flow fills a lookup's arguments from earlier results - sources, JSON paths, lists, constants - and what it cannot bind.
---

# Bindings

A flow never makes up an argument. For each lookup it may make, it learned from the agent's own calls where each argument's value came from: which earlier result, at which JSON path. When it serves, it takes the value from the same place in this session's results. That is a *binding*.

## Where arguments come from

From [a flow learned on five retail sessions](/reference/formats#a-flow-walked-through):

- `order_id` for `get_order_details` came from `get_user_details`' result at `$.orders[*]`, all 10 times;
- `product_id` for `get_product_details` came from `get_order_details`' result at `$.items[*].product_id`, 4 of 4.

`$` is the whole result and `[*]` any element of a list. A result that is not JSON is read as one string, or, when it has several lines, as the list of its lines, so `$[*]` is one line of it: in [the walkthrough](../walkthrough), `read_text_file`'s `path` came from `search_files`' result at `$` (a search that found one file) or at `$[*]` (one line of a search that listed several).

`stretto flow-show` prints every binding:

```markdown
| Lookup | Argument | Bound from (values found there, of the values it took) | Chance it is the agent's (right/tried): not mentioned, mentioned |
|---|---|---|---|
| `read_text_file` | `path` | `search_files` at `$` (3 of 14); `search_files` at `$[*]` (11 of 14) | 0.81 (12/14), 0.50 (0/0) |
| `search_files` | `pattern` | nothing: the flow never makes this lookup | — |
```

## The rules

- **Required arguments only.** An argument the agent passed in at least 90% of a lookup's calls is required. The flow passes only required arguments, and [constants](#lists-searches-and-constants) when it learned them.
- **Sources seen twice.** A source is used only if it was found at least twice and accounts for at least 10% of the argument's values. A single observation is not a pattern.
- **The next value.** The flow binds each required argument to the first value at one of its sources that it has not passed already, so after a listing it reads the first record, then the next, and stops when the list is done.
- **In the agent's order.** Where an argument has several sources, the flow tries them in the order the agent used them after this tool, and otherwise the most recent result first. After reading one phone line, an agent working through a customer's lines takes the next line's id, not the plan id of the line it just read.
- **What the user mentioned first.** With the conversation ([`--context`](./sessions#the-conversation)), the flow prefers a value the user mentioned, or a record they mentioned by another of its fields.
- **Nothing to bind, no lookup.** A required argument with no source, such as an email only the customer knows, means the flow never makes that lookup itself; the agent does.

## The binding's chance

Each lookup's binding has a chance of being the agent's own call, which multiplies the tool's probability before the threshold. It is counted at the agent's own lookups in training, separately for values the customer had and had not mentioned:

$$\text{chance} = \frac{\text{agreed} + 1}{\text{calls} + 2}$$

In the walkthrough, the binding picked the agent's own `path` at 12 of its 14 reads: $13/16 \approx 0.81$.

## The wrong record

Nearly every detour in the research was the right lookup of the wrong record. Two counts in the flow cover the cases that produced them:

- **A record the customer did not ask about** (`named_other`). When the customer has named one record of a list, a lookup of another is scored by how often, in training, the agent went on to read another in that case. Replayed, that cut GLM-5's airline detours from 56 to 4 at no cost in turns ([results](../../../docs/results/named-other-2026-09-26.md)).
- **The record already found** (`described_read`). Once the record holding a value the customer gave has been read, such as the line with a ticket's phone number, a lookup of the next record of the list is scored by how often the agent read on in that case in training. In τ²-bench's solo telecom replays that halved the flow's detours ([results](../../../docs/results/telecom-flows-2026-09-26.md)).

## Lists, searches and constants

- **Lists** (`bindings.lists`). An argument that takes a list, such as the hotels a city's listing named passed to a lookup of their prices, is bound to every value at the path of the most recent result that held them all, in order.
- **Searches the agent always narrows** (`bindings.bare`). A lookup with no required argument is made with none. If the agent always narrowed it with some optional filter, such a bare call is never the agent's own, and its chance says so: near 0, so the flow does not make it.
- **Constants** (`stretto learn --constants`). An argument the agent passed with one value in every call of a lookup, at least five calls and at least half of the lookup's, and never found in an earlier result, such as a page size, is learned as a constant and passed as the agent did. A string a user wrote with a digit or an @, such as an id or an email, is never a constant, however many sessions shared it. Check each one in `flow-show` before serving.

## What a flow cannot bind

A value must appear whole in an earlier result: as a JSON string, or as a line of a result that is not JSON. A value the agent composes, such as a path joined from a folder and a file name, has no source, so a lookup that needs one hands back. So does a value only the user's words hold, such as a name or an email the user typed: binding those would take a model that reads the conversation.

## Related

- [`bindings` in the file format](/reference/formats#bindings): every field and formula
- [Lookups and detours](./lookups)
- [Audit and review](./audit-and-review): reading the bindings before you serve a flow
