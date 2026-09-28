---
description: Review a flow before serving it with stretto flow-show, review every change with stretto flow-diff, audit a flow on sessions it never saw, watch for drift with stretto drift, and hand back when a session surprises the flow.
---

# Audit and review

A flow makes calls on the agent's behalf, so treat it like code: read it before it runs, review every change, and check it against new traffic.

## Review a flow

```sh
stretto flow-show ~/.stretto/orders.flow.json
```

`flow-show` renders a flow as Markdown, section by section:

- **Tools.** The only tools it may call, the read tools, and those it never calls. A tool that writes but is marked read is the one mistake that matters here, so check the kinds against what the tools do.
- **Sites.** After each call: the lookups the flow may make next, how often the agent made each in training, and what the flow does there at the served threshold: with `reach`, as the proxy serves a flow without an arbiter, or with the habit alone for a flow with one, whose model a review does not ask. A lookup seen once or twice is a thin basis.
- **Bindings.** Where each required argument comes from, and how often that way gave the agent's own arguments. "Nothing" means the flow never makes that lookup itself ([bindings](./bindings)).
- **Constants**, **code features** and the **arbiter**, when the flow has them. Constants should be values every call passes, such as a page size, and not one user's id. Code features hold values copied from training results. An arbiter sends the conversation to a System-One model at each decision.

[Reviewing flows](/reference/review) has a real flow rendered in full, and what to check in each section.

[The console](../console)'s page for a flow shows the same review, and compares the flow with another as `flow-diff` does. Its *Audit* button starts `stretto audit` as a job, and the job's page renders the report.

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

**When to learn again:** a site whose agreement falls on new sessions, a request type the flow has not seen, or a server whose tools changed. `stretto drift` watches for the first of these as sessions arrive, and [`stretto stage`](./staged-flows) learns the next version as they do, scored against the flow being served, for `flow-commit` to serve.

## Watch for drift

A flow is learned from one agent, one prompt, one harness and one set of tools, and any of them can change. `drift` scores the sessions a flow served, in the order they ran:

```sh
stretto drift --flow ~/.stretto/orders.flow.json --sessions ~/.stretto/logs/orders
```

For each session it takes three scores under the flow, as the audit does: the surprise of the agent's steps, how often the flow's likeliest option was not the agent's step, and the share of the agent's steps that follow a tool training never saw it call. Bayesian online change-point detection (Adams and MacKay, 2007) watches the three together. The alarm sounds when a change is more likely than not to have happened in the last ten sessions (`--window`), after at least ten sessions and with three sessions of the new behavior since. One odd session is not enough, nor two alike, which can be one task's retries.

The report names the change and the sites where the flow's surprise rose most across it, lists the tools the agent called that training never saw, and scores every session. `drift` exits with 1 while the alarm sounds after the last session, 0 when it does not, and 2 on an error, so a scheduled job can act on it.

When it sounds, learn again with the recent sessions weighed more:

```sh
stretto learn --sessions ~/.stretto/logs/orders --domain orders --habit-only \
  --half-life 20 --out orders.flow.json
```

With `--half-life 20`, a session 20 sessions older than the newest counts half as much in the habit, and one 40 older a quarter. The flow follows the change without discarding everything it learned before. Review the new flow with `flow-diff` before serving it.

On τ²-bench, `drift` found agents that fit a flow clearly worse than the one it was learned from, or called tools it never saw, a median of 8 to 10 sessions after they took over. It stayed quiet where they fit about as well, and on one agent's own sessions it sounded in 5 of 160 random orders ([results](../../../docs/results/drift-2026-09-28.md)).

The proxy watches the server's side on its own. Once the server lists its tools, a tool the flow looks up is left to the agent in any of these cases:

- the server no longer lists it;
- it marks the tool as a write;
- it no longer marks the tool read-only, for a flow learned from that server's listing;
- the tool's input changed since the flow was learned.

The flow log records why on the decision (`"withheld"`), and stderr says so once per tool.

## Hand back when a session surprises the flow

`drift` watches sessions go by. Within one session, a flow can hand back once the agent's steps stop looking like the sessions it learned from. Learn it with a surprise gate:

```sh
stretto learn --sessions ~/.stretto/logs/orders --domain orders --habit-only \
  --surprise 0.95 --out orders.flow.json
```

After each call where the flow decides, the gate scores what the agent did next by its surprise: −ln of the habit's probability of that step. A reply, a write or a lookup the flow does not offer there counts as handing back, and the flow's own lookups are not the agent's steps. Once the agent's steps average more than the gate's threshold over five in a row (`--surprise-window`), the flow hands back after every call for the rest of the session. The flow log gives the reason `the session surprised the flow`.

`--surprise 0.95` sets the threshold from the training sessions. Each one is scored by a habit learned without its task, and the threshold is the 0.95 quantile of their most surprising runs of steps, so about one session like them in twenty trips it. `flow-show` shows the gate, and `flow-diff` lists a change to it. `stretto stage` learns it again at the same quantile. The proxy serves a flow without its gate with `--flow-surprise off`, or with another threshold with `--flow-surprise NATS`; `serve` and `flow-serve` take `--surprise`.

Replayed on two τ²-bench agents' test episodes, the gate rarely had anything left to stop. At the quantiles a deployment would choose, 0.8 to 0.99, turns saved and detours moved by at most one in 62 of 64 runs. Where it tripped, the flow had already made 90% or more of its lookups: they come early in a session, and the steps that surprise it come later. No setting paid in more than one of the four agent and domain pairs, so no flow has a gate unless `--surprise` gives it one ([results](../../../docs/results/surprise-2026-09-28.md)).

## Related

- [Reviewing flows](/reference/review), in full
- [`stretto audit`](/reference/cli#stretto-audit), [`drift`](/reference/cli#stretto-drift), [`flow-show`](/reference/cli#stretto-flow-show) and [`flow-diff`](/reference/cli#stretto-flow-diff)
- [The console](../console): the review, the comparison and audits, in a browser
