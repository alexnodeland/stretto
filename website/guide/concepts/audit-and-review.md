---
description: Review a flow before serving it with stretto flow-show, review every change with stretto flow-diff, and audit a flow on sessions it never saw.
---

# Audit and review

A flow makes calls on the agent's behalf, so treat it like code: read it before it runs, review every change, and check it against new traffic.

## Review a flow

```sh
stretto flow-show ~/.stretto/orders.flow.json
```

`flow-show` renders a flow as Markdown, section by section:

- **Tools.** The only tools it may call, the read tools, and those it never calls. A tool that writes but is marked read is the one mistake that matters here, so check the kinds against what the tools do.
- **Sites.** After each call: the lookups the flow may make next, how often the agent made each in training, and what the flow does there with the habit alone at the served threshold. A lookup seen once or twice is a thin basis.
- **Bindings.** Where each required argument comes from, and how often that way gave the agent's own arguments. "Nothing" means the flow never makes that lookup itself ([bindings](./bindings)).
- **Constants**, **code features** and the **arbiter**, when the flow has them. Constants should be values every call passes, such as a page size, and not one user's id. Code features hold values copied from training results. An arbiter sends the conversation to a System-One model at each decision.

[Reviewing flows](/reference/review) has a real flow rendered in full, and what to check in each section.

## Compare two flows

Learn again as sessions arrive, and compare the new flow with the one being served:

```sh
stretto flow-diff old.flow.json new.flow.json
```

`flow-diff` speaks in the same terms as `flow-show` and lists first what needs a reviewer: every way the new flow can do something the old one could not.

| Change | Needs review |
|---|---|
| A tool newly marked read-only, or a lookup offered after a call where it was not | Yes |
| An argument bound from a new source, or a new or changed constant | Yes |
| An arbiter added, another System-One model, or new or reworded predicates | Yes |
| A site newly promoted, a promotion lifted, or a site switched back on | Yes |
| Anything removed, so that the flow does less | No |
| Thresholds, shares, binding chances and weights that moved | No; listed |

It exits with 0 when nothing needs review, 1 when something does, and 2 on an error, as `diff` does. So a CI job can post the diff on a pull request that changes a flow, and ask for a review when it exits with 1:

```sh
git show origin/main:flows/orders.flow.json > old.flow.json
stretto flow-diff old.flow.json flows/orders.flow.json > flow-diff.md
```

Keep flows in version control, and run `flow-diff` on every change. [Reviewing flows](/reference/review#stretto-flow-diff) lists every kind of change.

## Audit a flow

Before trusting a flow on new sessions, score it on sessions it never saw, recorded without it:

```sh
stretto audit --flow ~/.stretto/orders.flow.json --sessions ~/.stretto/new-sessions
```

At each point where the flow would decide, the audit compares its likeliest option with what the agent did next, and reports, per site and overall:

- **agreement**: how often the flow's likeliest option was the agent's step;
- **surprise**: nats per decision, the agent's path scored under the flow, run as a fugue program with `ScoreGivenTrace`;
- **calibration**: how well the flow's probabilities match how often it was right.

From [the walkthrough](../walkthrough#4-audit-it-on-sessions-it-never-saw):

```text
The flow decides with the habit alone. 3 episodes, 8 decisions scored (0 left out: no answer from the oracle).

| Site | Decisions | Agreement | Nats per decision |
|---|---|---|---|
| `read_text_file` | 6 | 33.3% | 0.747 |
| `search_files` | 2 | 100.0% | 0.009 |
```

A site with low agreement is not a site the flow gets wrong, but a site to look at. Here the habit expects a second read and then a stop, while the agent read as many files as the search found; the binding, which reads on while the search listed a file not yet read, is what serves that site.

The audit scores next-step predictions, the habit's by default for a flow with no arbiter. To score the lookups a flow would make, used or not before the agent's next write, which is what `reach` decides on, record in [shadow mode](./shadow-and-promotion) and read `stretto promote`'s report.

**When to learn again:** a site whose agreement falls on new sessions, a request type the flow has not seen, or a server whose tools changed.

## Related

- [Reviewing flows](/reference/review), in full
- [`stretto audit`](/reference/cli#stretto-audit), [`flow-show`](/reference/cli#stretto-flow-show) and [`flow-diff`](/reference/cli#stretto-flow-diff)
