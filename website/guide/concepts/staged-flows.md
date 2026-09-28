---
description: Learn the next version of a deployment's flow as sessions arrive, score it against the flow being served on each session before it learns from it, and commit it, with every version kept for a rollback.
---

# Staged flows

The proxy loads its flow when it starts, and the flow does not change by itself. As sessions arrive, `stretto stage` learns the next version beside it, the **staged** flow, and scores it against the **committed** flow, the one the proxy serves. Nothing reaches the proxy until you commit.

## 1. Stage

```sh
stretto stage --flow ~/.stretto/orders.flow.json --sessions ~/.stretto/logs/orders
```

- **It learns as `learn` does.** The staged flow is learned from every session in the directory, as `stretto learn --habit-only` would learn it from them. It is written beside the committed flow as `orders.staged.flow.json`.
  - It keeps what the committed flow has that it does not learn: its arbiter, its promotion and its per-site thresholds.
  - `--half-life` makes older sessions count less, as for `learn`.
  - The committed flow need not exist yet: give `--domain`, and the first commit makes it.
- **It learns from served sessions as they are.** The flow's lookups count as the agent's steps. That teaches a flow as much as sessions recorded without one ([results](../../../docs/results/served-sessions-2026-09-26.md)).
- **It scores each new session first.** A session that is new since the last run is scored before the staged flow learns from it, as `stretto promote` scores a flow as it is served. It is scored twice: by the committed flow, and by the staged flow as it was before the session arrived. So the comparison is out of sample for both flows.
- **Served lookups count as neither used nor a detour.** In a session the committed flow served, its lookups are there already, and the agent had no reason to make them again. A lookup the proxy made counts as *served*, for either flow.
  - The comparison is exact on sessions recorded in [shadow](./shadow-and-promotion).
  - In served sessions, it is exact wherever the flow handed back.
- **Run it as sessions arrive**, from a scheduled job or after each session. With no new session, it learns nothing and reports again. Its state is in `orders.stage.json`.

The report compares the two flows on the last sessions both were scored on (`--window`, 50 by default). For each site it gives, for both flows:
- the lookups the flow would have made;
- how many of them the agent made later, with a 90% interval on the share;
- the detours.

It then lists what committing the staged flow would change, as `stretto flow-diff` lists it.

## 2. Commit

```sh
stretto flow-commit --flow ~/.stretto/orders.flow.json --note "reads the order after the account"
```

- **The staged flow replaces the committed one whole.** A proxy that starts reads the old flow or the new one, never part of each. An MCP host starts its servers, and so the proxy, for each session, so the next session is served the new flow.
- **Every committed version is kept** in `orders.history/`. Each version is `N.flow.json`, with a record `N.json`: when, why, what changed, and the comparison the commit rested on.
- **A committed flow changed by other means is kept first**, as a version of its own. `learn --out` onto it is one such way.

## 3. Look back, and roll back

```sh
stretto flow-log --flow ~/.stretto/orders.flow.json
stretto flow-rollback --flow ~/.stretto/orders.flow.json
stretto flow-rollback --flow ~/.stretto/orders.flow.json --to 3
```

- **`flow-log`** lists the versions, the latest first.
- **`flow-rollback`** restores the version before the current one, or `--to` another. The restored flow becomes a new version, so a rollback can be rolled back too.

## In the console

A flow's **Staged** tab in [the console](../console) shows the same: the staged flow, the comparison site by site, what committing it would change, and every version. A `stage` job learns the staged flow, and the tab commits it, with a note, or rolls back to any version. The API under it, `GET /api/flows/:key/stage` and `POST /api/flows/:key/commit` and `/rollback`, does the same from a script.

## Why it learns again, rather than update

The habit's counts and the bindings' chances are conjugate, and could take in one session at a time. The flow's structure is not. It is fitted to all the sessions at once:
- its vocabulary of calls;
- the features of results it conditions on;
- the habit's concentration;
- its sites;
- the sources its bindings trust.

Learning the staged flow again keeps it the flow `learn` would write from the same sessions, which is what a review reads.

## Related

- [Shadow mode and promotion](./shadow-and-promotion)
- [Audit and review](./audit-and-review)
- [`stretto stage`](/reference/cli#stretto-stage), [`flow-commit`](/reference/cli#stretto-flow-commit), [`flow-rollback`](/reference/cli#stretto-flow-rollback) and [`flow-log`](/reference/cli#stretto-flow-log)
