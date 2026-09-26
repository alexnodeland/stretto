# Changelog

The `stretto` CLI and `stretto-proxy` are the product. The library crates (`stretto-trace`, `stretto-oracle`, `stretto-model`, `stretto-report`) make no stability promise before 1.0. The file formats carry versions of their own ([docs/formats.md](docs/formats.md#versions)).

## Unreleased

- **Counterfactual evaluation** (RFC-001 §3.7, [#15](https://github.com/alexnodeland/stretto/issues/15)). `stretto-proxy --flow-explore EPSILON`, and `serve --explore` and `flow-serve --explore`, explore at read-only sites: with that probability the flow takes a lookup other than its rule's choice, drawn by the decider's probabilities among the lookups that bind. Each decision then logs its `policy`: every option and the chance that the flow took what it took. `stretto evaluate` reads such decisions, with each option's outcome, and estimates what another rule (the arbiter's or the habit's probabilities at a threshold) would have done, per site and in total: directly from every option's outcome, and by IPS, self-normalized IPS and a doubly robust estimate from the taken option's alone, with each site's effective sample size. `pilot/check_flow.py --explore` writes the labelled decisions from replays.
- **Predicate refinement** (RFC-001 §3.4, [#16](https://github.com/alexnodeland/stretto/issues/16)). `phase0 --candidates FILE` asks candidate predicates alone at every asked next-step decision, with the same state as the next-step question, so every other answer keeps its cache key. Their answers go to `--oracle-log`, and `--weigh ID` weighs one in the arbiter. `stretto refine` reads such a log, fits the arbiter by cross-validation over tasks with each candidate added, and keeps candidates greedily while one raises the held-out log-likelihood of the agents' steps by more than a penalty (by default what BIC charges a parameter). A transfer target's decisions, judged but never fitted on, are scored alongside as an out-of-sample check. `--examples` writes examples from the sites where the arbiter is weakest, for whoever proposes the candidates.
- A predicate can bear on one named lookup: `"favors": {"lookup": TOOL}` ([formats](docs/formats.md)).
- **Flow search** (RFC-001 §3.10, [#25](https://github.com/alexnodeland/stretto/issues/25)). `stretto search` runs NSGA-II, from [fugue-evo](https://github.com/alexnodeland/fugue-evo), over each site's threshold (0.10 to 0.95, or off) and the decider (the arbiter, or the habit alone). It scores each setting by a replay command that prints `pilot/check_flow.py`'s `CHECK` totals, and reports the settings that save the most turns for the fewest detours, starting from the hand-set ones. It counts the decisions a replay could not answer, and says so when a setting on the front has any. `--rescore` replays an earlier search's front on other episodes, such as held-out ones.
- **Per-site thresholds in the flow IR** (`thresholds`): at a listed site a flow acts on that threshold in place of the served one, and above 1 it never acts there. A flow with them is format version 2, which 0.1.0 refuses rather than ignores; this build reads versions 1 and 2. `flow-show` and `flow-diff` show them. `flow-diff` lists a changed threshold without flagging it, as it would the served one, and flags a site switched back on.
- `stretto-proxy --flow-tools TOOLS` names the only tools a flow may call on its own. A read-only tool can still be metered, rate-limited or recorded as an access; without the option, a flow may call every tool it reads as a lookup, as before.
- **Pinned inputs.** A flow learned from recorded sessions pins each tool's input contract (`contracts`): its arguments, their JSON types and which are required, as the server listed them. `stretto-proxy` makes no lookup of a tool whose server now lists another, and says so once. `flow-show` lists them and `flow-diff` shows a change.
- **A record the customer did not ask about.** `learn` and `compile` count, per lookup, the picks the binding would make where the customer had mentioned a value at its sources and the binding would pass another, and how many of them the agent went on to pass (`bindings.named_other`). The binding's chance there is that rate, shrunk towards its unmentioned chance. `flow-show` and `flow-diff` show it. A flow with the count is format version 2. Replayed on GLM-5's test episodes, airline detours fell from 56 to 4 at no cost in turns ([results](docs/results/named-other-2026-09-26.md)).
- `scripts/proposal_check.py` checks each write of τ²-bench episodes against what the agent proposed, with no model: it flags a write when the confirmation chose another record of the list its value came from ([results](docs/results/proposal-check-2026-09-26.md)).
- `scripts/anatomy.py` classes every LLM turn, every decision after a tool returns and every argument of τ²-bench episodes by where its information came from: copied from an earlier result (one value at its path, or one of several), the customer's words, or made up, after TraceCompiler's binding classes; and how well the previous tool, structured features of the results and the goal predict each decision ([results](docs/results/anatomy-2026-09-26.md)).
- `scripts/anatomy.py` reads telecom too: τ²-bench's read-only tools for the agent and the phone, features from results in text (each `Key: value` line's value, or its `|`-separated parts), and results without ids paired with their calls in order. It also predicts each decision with a decision tree over the whole state (every tool's last result and the tools called), its depth chosen by cross-validation, as decision mining does at a process's branch points ([results](docs/results/telecom-anatomy-2026-09-26.md)).
- **Sources in the order the agent used them at the site.** `learn` and `compile` count, for each lookup argument with more than one source, where the agent took its values at each site, the tool whose result came last (`bindings.site_sources`), each read of a batch at the site of the read before it, as a flow makes them. Where a site has enough of them, the binding tries its sources in that order before recency: after reading a phone line, the next line id, not the line's plan id. In telecom, replayed on GLM-5's test episodes, a flow learned from τ²-bench's 2025 runs made 50 detours instead of 289 and saved 67 turns instead of 56, and one learned from GLM-5's own runs 162 instead of 304 at the same savings; retail and airline replay exactly as before. A flow with them is format version 2.
- `stretto-proxy --confirm-judge` also logs, with each judgment, the proposal check (`proposal_check`): the call's values whose record the confirmation chose another of, with no model. Only values with a digit are checked, as ids carry them; on τ²-bench's 2025 runs that flags exactly what `scripts/proposal_check.py`'s closed-choice rule flagged. It refuses nothing.
- `scripts/telecom_workflow.py` runs a workflow compiled once from τ²-bench telecom's solo runs (a decision tree per site over the whole state, its steps whole calls) with no model in τ²-bench's environment, and scores it with τ²-bench's evaluator; `--sure` hands back where the tree is not sure, `--no-guard` lets it repeat itself, and `--train-share` learns from fewer tasks ([results](docs/results/telecom-workflow-2026-09-26.md)). `anatomy.tree(counts=True)` returns a leaf's calls by count.
- `scripts/detours.py` says where a replayed flow's detours come from: a session hand-back simulated on the replay, each lookup's place in its chain against its probabilities and use, and, in airline, reads after a reservation the customer had described ([results](docs/results/detours-2026-09-26.md)).
- `pilot/check_flow.py` names each recorded episode it replays after its folder, or, when two folders share a name (a task's trials, each in its own folder), after its path below their common parent. Such episodes shared one replay folder before, and with `--jobs` one trajectory, which the flow reads; the published replays were of τ²-bench results files, whose episodes are named by task and trial, and are unaffected.

## 0.1.0 (2026-09-25)

Everything so far: the first design of [RFC-001](docs/rfc/001-habit-compiler.md), built and measured. [docs/results](docs/results/README.md) has every result.

### Measure an agent (`stretto phase0`)

- How compressible an agent's behavior is, from τ²-bench's published trajectories, with no key: a hierarchical Dirichlet habit of the next tool call, its concentration fitted with fugue, coverage and agreement on held-out tasks, transfer between agents, macro-tool headroom and argument provenance.
- With `--oracle`, System-One questions at every held-out decision (Phase 0b): `v1`, over every tool, and `v2`, read-only lookups with a state slice, a stop question, yes/no predicates (`data/predicates-v2.json`) and an arbiter that weighs the answers against the habit. Answers go through a replay cache, and published answer bundles replay without a key (`export-answers`, `import-answers`). `stretto ask` answers questions of your own through the same cache. `--domain telecom` measures τ²-bench's third domain.

### Flows

- `stretto compile` writes a flow (the flow IR, `stretto_flow: 1`) from τ²-bench results. `stretto learn` learns one from sessions the proxy recorded: with the habit alone (`--habit-only`), with an arbiter fitted on held-out sessions, or with a shipped arbiter (`--arbiter-from`). A flow's lookups bind their arguments from earlier results, including results that list values one per line.
- `stretto serve` and `stretto flow-serve` answer a flow's decisions over a local port. `stretto export-arbiter` writes a flow's arbiter to its own file (`stretto_arbiter: 1`), and `stretto fit-arbiter` fits one on the decision logs of several compiles. `data/arbiters/` ships two, fitted on four agents' retail and airline decisions; they carry between those two domains, not to one unlike both.
- `stretto audit` scores recorded sessions under a flow, run as a fugue program: agreement, calibration and surprise per site. A flow without an arbiter is audited with its habit.
- `stretto promote` keeps a flow to the sites where its lookups were the agent's own, scored on recorded sessions or τ²-bench results; a promoted flow hands back after every other call. `stretto-proxy --flow-shadow` records the sessions to promote on: the flow decides and logs, but makes no lookups.
- `stretto flow-show` renders a flow for review: the tools it may call, the lookups it may make after each call and where their arguments come from, and how it decides. `stretto flow-diff` lists what changed between two flows for a pull request, and exits with 1 when a change needs review ([reviewing flows](docs/review.md)).

### Checks on writes

- `stretto guards`: typed policy checks for τ²-bench's retail and airline domains, audited against the published trajectories.
- `stretto confirm`: a System-One model judges whether the customer confirmed each write, with an optional second question. `stretto match` asks which record a customer means.

### `stretto-proxy`

- A stdio MCP proxy that forwards byte for byte and records sessions (`stretto_mcp_log: 2`). `--upstream URL` wraps a Streamable HTTP server instead of a command, with headers from the environment that are never logged.
- Active mode: `--flow` runs a flow behind the agent's calls and appends its lookups to the result. `--guards` refuses writes a policy check fails. `--confirm-judge log|enforce` adds the confirmation judge, and `--confirm-second-shadow` asks its second question but only logs the answer. `--commit` adds a tool for confirmed writes in one call. `--context` reads the conversation the host writes.
- Jev's key can come from a file: `TYPESAFE_API_KEY_FILE` names it when `TYPESAFE_API_KEY` is not set.
- `stretto-mcp-demo`, a tiny server for trying it.
- `--retain-days N` deletes, at start, the logs and cached answers older than N days.
- Each run of a flow after one of the agent's calls is a fugue program ([`program.rs`](crates/stretto-report/src/program.rs), RFC-001 §3.2):
  - **The sites.** A decision site `decide#i` comes before each lookup, and the flow's arbiter decides it. An outcome site `outcome#i` comes after, and takes the server's answer and scores it.
  - **The program is in the flow.** The flow IR holds it as `program`, in fugue's serializable program format (RFC-001 §3.5), and it is checked when the flow loads. A flow written without one runs the standard run, which draws exactly what the Rust it replaced drew. `flow-show` prints the program, and `flow-diff` lists a change to it as needing review.
  - **Its distributions.** The flow registers two, `Decide` and `Outcome`, from its own statistics: a flat Dirichlet's predictive over what the agent did next at a site, and a flat Beta's over how often a tool succeeded, from fugue's conjugate helpers. They carry their site as metadata (`WithMeta`).
  - **Three interpreters.** The proxy runs the program with fugue's `run_async`. `PriorHandler` simulates a flow with it, and `ScoreGivenTrace` scores a recorded run.
  - **The flow log.** Decision lines carry their `address`, and each run ends with a `run` line holding its trace and its surprise.

### Privacy

- `stretto redact` writes a pseudonymized copy of recorded sessions: a value fewer than `--keep-shared` sessions contain becomes a salted hash, the same wherever it appears, and `--hash-field` hashes named fields however many sessions share them. A flow learned from the copy matches one learned from the originals, on synthetic sessions, 40 real ones and 5 with their conversation ([privacy](docs/privacy.md)).
- `stretto learn --sessions` and `redact` skip the confirmation logs the proxy writes beside the sessions; they failed on them before.

### Live runs

- The pilot harness runs a Claude model as the agent or the customer (`run_episode.py --agent-cli claude`, `--customer-cli claude --customer-model`, through `pilot/claude-agent.sh`), and Z.ai credits count the GLM side only. The guards arm takes the confirmation judge (`--confirm-judge`, `--confirm-second`, `--confirm-second-shadow`), with Jev's key handed to the proxy in a file, and `--label` names an arm's directory.
- `pilot/run_paired.py` runs every test task of a domain in both arms, reusing a pilot's pairs, under a Z.ai credit budget that it checks against a ledger before each episode. `pilot/analyze_paired.py` reports on the result: paired pass rates with a bootstrap interval and McNemar's test, turns and tokens saved, detours and their token cost, and a pooled estimate across domains ([the paired run](docs/results/paired-2026-09-25.md)). Its `arms` command compares any arms run on the same tasks ([the cold start, live](docs/results/cold-start-live-2026-09-25.md)).

### Documentation

- [The walkthrough](docs/walkthrough.md) runs the whole loop on the official MCP filesystem server, and CI runs it.
- [Reviewing flows](docs/review.md), with example diffs of real flows that a test keeps current.
- [Privacy](docs/privacy.md): what each file holds, what is sent to the System-One model, retention and redaction.
- [The file formats](docs/formats.md), [the CLI reference](docs/cli.md) (generated from the code, kept current by CI), [the design](docs/design.md) and [the roadmap](docs/roadmap.md).
- The working paper, published at [alexnodeland.github.io/stretto](https://alexnodeland.github.io/stretto/).

### Release

- Install from the tag: `cargo install --git https://github.com/alexnodeland/stretto --tag v0.1.0 stretto-proxy stretto-report`. It is not on crates.io yet: it depends on fugue's `program` feature, which is not in a fugue release yet.
- The shipped arbiters stay in [`data/arbiters/`](data/arbiters/README.md); take them from a checkout or from the tag.
- A version tag, or the release workflow run by hand, makes a GitHub release with that version's section of this file as its notes.
