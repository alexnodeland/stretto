# Changelog

The `stretto` CLI and `stretto-proxy` are the product. The library crates (`stretto-trace`, `stretto-oracle`, `stretto-model`, `stretto-report`) make no stability promise before 1.0. The file formats carry versions of their own ([docs/formats.md](docs/formats.md#versions)).

## Unreleased (0.1.0)

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
- Active mode: `--flow` runs a flow behind the agent's calls and appends its lookups to the result. `--guards` refuses writes a policy check fails. `--confirm-judge log|enforce` adds the confirmation judge. `--commit` adds a tool for confirmed writes in one call. `--context` reads the conversation the host writes.
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

- `pilot/run_paired.py` runs every test task of a domain in both arms, reusing a pilot's pairs, under a Z.ai credit budget that it checks against a ledger before each episode. `pilot/analyze_paired.py` reports on the result: paired pass rates with a bootstrap interval and McNemar's test, turns and tokens saved, detours and their token cost, and a pooled estimate across domains ([the paired run](docs/results/paired-2026-09-25.md)).

### Documentation

- [The walkthrough](docs/walkthrough.md) runs the whole loop on the official MCP filesystem server, and CI runs it.
- [Reviewing flows](docs/review.md), with example diffs of real flows that a test keeps current.
- [Privacy](docs/privacy.md): what each file holds, what is sent to the System-One model, retention and redaction.
- [The file formats](docs/formats.md), [the CLI reference](docs/cli.md) (generated from the code, kept current by CI), [the design](docs/design.md) and [the roadmap](docs/roadmap.md).
- The working paper, published at [alexnodeland.github.io/stretto](https://alexnodeland.github.io/stretto/).
