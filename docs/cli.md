# CLI reference

Every command and option of the two binaries: `stretto`, which measures agents' traces and compiles, learns, serves and audits flows, and `stretto-proxy`, which records an MCP server's sessions and runs flows, guards and the confirmation judge on them. `--help` prints the same text.

The sections below are generated from the code. After changing an option, run `STRETTO_BLESS=1 cargo test --bins` to rewrite them; CI fails while they are out of date.

<!-- begin stretto -->

## `stretto`

Compile an agent's recorded behavior into flows, serve them, and measure them against published τ²-bench trajectories.

| Command | What it does |
|---|---|
| [`phase0`](#stretto-phase0) | Measure how compressible an agent's behavior is (no API keys needed), and with --oracle, how well a System-One model takes the decisions flows would hand it (Phase 0b). |
| [`flow-serve`](#stretto-flow-serve) | Serve a live read-only flow (RFC-001 §3.13). |
| [`compile`](#stretto-compile) | Compile a live read-only flow and write its IR (JSON) to a file. |
| [`learn`](#stretto-learn) | Learn a live flow from sessions recorded by `stretto-proxy` (JSONL logs in a directory) and write its IR, as `compile` does from τ²-bench results. |
| [`serve`](#stretto-serve) | Serve a compiled flow (from `compile`), as `flow-serve` does. |
| [`guards`](#stretto-guards) | Test the policy guards (typed checks a proxy runs before a write) against recorded τ²-bench trajectories. |
| [`confirm`](#stretto-confirm) | Judge the customer's confirmation before each write with a System-One model, next to the guards' word list, on τ²-bench's published trajectories. |
| [`match`](#stretto-match) | Match descriptions to records (RFC-001). |
| [`audit`](#stretto-audit) | Audit a flow against recorded episodes. |
| [`jev-check`](#stretto-jev-check) | Check that Jev is reachable with TYPESAFE_API_KEY. |
| [`export-arbiter`](#stretto-export-arbiter) | Write a flow's arbiter to its own file, to ship. |
| [`export-answers`](#stretto-export-answers) | Write every cached oracle answer to stdout as JSON lines (`{"key", "response"}`). |
| [`import-answers`](#stretto-import-answers) | Read JSON lines from `export-answers` on stdin into a replay cache. |

### `stretto phase0`

Measure how compressible an agent's behavior is (no API keys needed), and with --oracle, how well a System-One model takes the decisions flows would hand it (Phase 0b).

```text
Usage: stretto phase0 [OPTIONS] --tau2 <DIR>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout.
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to analyze.
- `--no-baselines`: Do not train on τ²-bench's published baselines in the checkout (then pass --source).
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to train on: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--target <[LABEL=]PATH>` (repeatable): τ²-bench results for an agent model the habit never trains on, measured as a transfer target: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--train-fraction <SHARE>` (default `1`; not with `--train-tasks`): Train on this share of the training tasks: a fixed sample by task id, each smaller share part of every larger one. To see how flows do with fewer traces.
- `--train-tasks <IDS>` (repeatable; not with `--train-fraction`): Train on these training tasks only (comma-separated), in place of `--train-fraction`: a sample named exactly.

**The habit**

- `--order <N>` (default `2`): Context length for coverage and transfer.
- `--alpha-samples <N>` (default `600`): MH draws for the posterior over α (0 to use --alpha).
- `--alpha <ALPHA>` (default `1`): α to use when --alpha-samples is 0.
- `--min-evidence <N>` (default `5`): Training observations a context needs before the habit may act on it.
- `--seed <N>` (default `7`): Seed for the MH chain.
- `--no-features`: Skip learning code features from tool outputs.
- `--no-intent`: Do not let flows know the episode's goal: the habit is not conditioned on it and System-One questions do not name it (for live flows nobody names).

**Phase 0b: the System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`): Phase 0b: who answers the System-One questions. `jev` needs TYPESAFE_API_KEY and pays once per distinct question; `replay` reads the cache only; `mock` checks the pipeline for free. The other options under this heading need it.
- `--questions <QUESTIONS>` (one of `v1`, `v2`; default `v1`): Which Phase 0b questions to ask: `v1` (one question over every tool, for flows that may call any tool) or `v2` (RFC-001 §3.5: read-only flows, a site's own lookups as options, a state slice, the stop decision asked on its own, and the answers combined with the habit).
- `--dataflow-hints`: v2: also describe each lookup by what its results supply, learned from argument dataflow in training.
- `--predicates <FILE>`: v2: a JSON file of yes/no predicates about the state (see `data/predicates-v2.json`) to ask with every next-step question and weigh in the arbiter.
- `--no-predicate-features`: v2: ask the predicates but leave them out of the arbiter, to measure what they add.
- `--manifest-options`: v2: offer every read-only tool at every site, not only the lookups seen there in training, for the System-One model to choose from. A lookup training never made is bound by argument name.
- `--pooled-arbiter`: v2, `compile`: give the flow one arbiter, fitted on every held-out decision, in place of one per fold, so that `export-arbiter` can ship it. Replayed on the same test tasks it has seen other agents' decisions on them, so compare flows without it.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--oracle-concurrency <N>` (default `8`): Oracle requests in flight at once.
- `--oracle-limit <N>`: Ask at most this many distinct questions (a stable sample), for a pilot run.
- `--oracle-budget <DOLLARS>` (default `5`): Refuse to start if uncached questions could cost more than this many dollars.
- `--oracle-model <MODEL>`: Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
- `--oracle-dump <FILE>`: Write every distinct oracle request to this file (JSON lines; the domain is added to the file name).
- `--oracle-log <FILE>`: Write every decision, with the agent's option and the oracle's pick, to this file (JSON lines; the domain is added to the file name).

**Output**

- `--out <FILE>`: Write the Markdown report here (default: stdout).
- `--json <FILE>`: Also write the full report as JSON here.

### `stretto flow-serve`

Serve a live read-only flow (RFC-001 §3.13): compile it from the same data and questions as `phase0 --questions v2`, goal free, then answer one query per connection on a local TCP port. A query is a JSON line `{"task_id", "messages"}` (τ²-bench messages so far, ending with a tool result); the answer is a JSON line with `"action": "lookup"` (and `tool`, `arguments`) or `"action": "hand_back"` (and `reason`).

```text
Usage: stretto flow-serve [OPTIONS] --tau2 <DIR>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout.
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to analyze.
- `--no-baselines`: Do not train on τ²-bench's published baselines in the checkout (then pass --source).
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to train on: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--target <[LABEL=]PATH>` (repeatable): τ²-bench results for an agent model the habit never trains on, measured as a transfer target: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--train-fraction <SHARE>` (default `1`; not with `--train-tasks`): Train on this share of the training tasks: a fixed sample by task id, each smaller share part of every larger one. To see how flows do with fewer traces.
- `--train-tasks <IDS>` (repeatable; not with `--train-fraction`): Train on these training tasks only (comma-separated), in place of `--train-fraction`: a sample named exactly.

**The habit**

- `--order <N>` (default `2`): Context length for coverage and transfer.
- `--alpha-samples <N>` (default `600`): MH draws for the posterior over α (0 to use --alpha).
- `--alpha <ALPHA>` (default `1`): α to use when --alpha-samples is 0.
- `--min-evidence <N>` (default `5`): Training observations a context needs before the habit may act on it.
- `--seed <N>` (default `7`): Seed for the MH chain.
- `--no-features`: Skip learning code features from tool outputs.
- `--no-intent`: Do not let flows know the episode's goal: the habit is not conditioned on it and System-One questions do not name it (for live flows nobody names).

**Phase 0b: the System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`): Phase 0b: who answers the System-One questions. `jev` needs TYPESAFE_API_KEY and pays once per distinct question; `replay` reads the cache only; `mock` checks the pipeline for free. The other options under this heading need it.
- `--questions <QUESTIONS>` (one of `v1`, `v2`; default `v1`): Which Phase 0b questions to ask: `v1` (one question over every tool, for flows that may call any tool) or `v2` (RFC-001 §3.5: read-only flows, a site's own lookups as options, a state slice, the stop decision asked on its own, and the answers combined with the habit).
- `--dataflow-hints`: v2: also describe each lookup by what its results supply, learned from argument dataflow in training.
- `--predicates <FILE>`: v2: a JSON file of yes/no predicates about the state (see `data/predicates-v2.json`) to ask with every next-step question and weigh in the arbiter.
- `--no-predicate-features`: v2: ask the predicates but leave them out of the arbiter, to measure what they add.
- `--manifest-options`: v2: offer every read-only tool at every site, not only the lookups seen there in training, for the System-One model to choose from. A lookup training never made is bound by argument name.
- `--pooled-arbiter`: v2, `compile`: give the flow one arbiter, fitted on every held-out decision, in place of one per fold, so that `export-arbiter` can ship it. Replayed on the same test tasks it has seen other agents' decisions on them, so compare flows without it.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--oracle-concurrency <N>` (default `8`): Oracle requests in flight at once.
- `--oracle-limit <N>`: Ask at most this many distinct questions (a stable sample), for a pilot run.
- `--oracle-budget <DOLLARS>` (default `5`): Refuse to start if uncached questions could cost more than this many dollars.
- `--oracle-model <MODEL>`: Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
- `--oracle-dump <FILE>`: Write every distinct oracle request to this file (JSON lines; the domain is added to the file name).
- `--oracle-log <FILE>`: Write every decision, with the agent's option and the oracle's pick, to this file (JSON lines; the domain is added to the file name).

**Serving**

- `--listen <ADDR>` (default `127.0.0.1:0`): Address to listen on (port 0: any free port; the ready line on stderr names it).
- `--threshold <P>` (default `0.3`): Take the most likely lookup when its probability, times the chance that its arguments are the agent's, is at least this.
- `--decider <DECIDER>` (one of `arbiter`, `habit`; default `arbiter`): Where each option's probability comes from: `arbiter` (the habit, the System-One model and the predicates, combined: arm D0) or `habit` (the habit alone, never asking the System-One model: a flow compiled from traces only, arm C at a high threshold).
- `--max-questions <N>` (default `300`): Stop asking the System-One model after this many live questions.
- `--log <FILE>`: Append every query's answer here (JSON lines).

### `stretto compile`

Compile a live read-only flow and write its IR (JSON) to a file: the same data and questions as `phase0 --questions v2`, goal free, from cached System-One answers only. `serve` and `stretto-proxy --flow` load the file.

```text
Usage: stretto compile [OPTIONS] --tau2 <DIR> --out <FILE>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout.
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to analyze.
- `--no-baselines`: Do not train on τ²-bench's published baselines in the checkout (then pass --source).
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to train on: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--target <[LABEL=]PATH>` (repeatable): τ²-bench results for an agent model the habit never trains on, measured as a transfer target: `path` or `label=path` (repeatable; files for other domains are skipped).
- `--train-fraction <SHARE>` (default `1`; not with `--train-tasks`): Train on this share of the training tasks: a fixed sample by task id, each smaller share part of every larger one. To see how flows do with fewer traces.
- `--train-tasks <IDS>` (repeatable; not with `--train-fraction`): Train on these training tasks only (comma-separated), in place of `--train-fraction`: a sample named exactly.

**The habit**

- `--order <N>` (default `2`): Context length for coverage and transfer.
- `--alpha-samples <N>` (default `600`): MH draws for the posterior over α (0 to use --alpha).
- `--alpha <ALPHA>` (default `1`): α to use when --alpha-samples is 0.
- `--min-evidence <N>` (default `5`): Training observations a context needs before the habit may act on it.
- `--seed <N>` (default `7`): Seed for the MH chain.
- `--no-features`: Skip learning code features from tool outputs.
- `--no-intent`: Do not let flows know the episode's goal: the habit is not conditioned on it and System-One questions do not name it (for live flows nobody names).

**Phase 0b: the System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`): Phase 0b: who answers the System-One questions. `jev` needs TYPESAFE_API_KEY and pays once per distinct question; `replay` reads the cache only; `mock` checks the pipeline for free. The other options under this heading need it.
- `--questions <QUESTIONS>` (one of `v1`, `v2`; default `v1`): Which Phase 0b questions to ask: `v1` (one question over every tool, for flows that may call any tool) or `v2` (RFC-001 §3.5: read-only flows, a site's own lookups as options, a state slice, the stop decision asked on its own, and the answers combined with the habit).
- `--dataflow-hints`: v2: also describe each lookup by what its results supply, learned from argument dataflow in training.
- `--predicates <FILE>`: v2: a JSON file of yes/no predicates about the state (see `data/predicates-v2.json`) to ask with every next-step question and weigh in the arbiter.
- `--no-predicate-features`: v2: ask the predicates but leave them out of the arbiter, to measure what they add.
- `--manifest-options`: v2: offer every read-only tool at every site, not only the lookups seen there in training, for the System-One model to choose from. A lookup training never made is bound by argument name.
- `--pooled-arbiter`: v2, `compile`: give the flow one arbiter, fitted on every held-out decision, in place of one per fold, so that `export-arbiter` can ship it. Replayed on the same test tasks it has seen other agents' decisions on them, so compare flows without it.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--oracle-concurrency <N>` (default `8`): Oracle requests in flight at once.
- `--oracle-limit <N>`: Ask at most this many distinct questions (a stable sample), for a pilot run.
- `--oracle-budget <DOLLARS>` (default `5`): Refuse to start if uncached questions could cost more than this many dollars.
- `--oracle-model <MODEL>`: Model id to request (default: TYPESAFE_DEFAULT_MODEL, else jev-latest).
- `--oracle-dump <FILE>`: Write every distinct oracle request to this file (JSON lines; the domain is added to the file name).
- `--oracle-log <FILE>`: Write every decision, with the agent's option and the oracle's pick, to this file (JSON lines; the domain is added to the file name).

**Output**

- `--out <FILE>` (required): Where to write the flow.

### `stretto learn`

Learn a live flow from sessions recorded by `stretto-proxy` (JSONL logs in a directory) and write its IR, as `compile` does from τ²-bench results. The tools come from the sessions' `tools/list` responses (their `readOnlyHint` annotations), or from `--manifest`. With `--results`, the sessions are τ²-bench episodes on a checkout's training tasks instead, as if a deployment had recorded them.

```text
Usage: stretto learn [OPTIONS] --domain <NAME> --out <FILE>
```

**Inputs**

- `--sessions <DIR>` (not with `--results`): Directory of session logs (`*.jsonl`); needed unless `--results` is given.
- `--results <FILE>` (repeatable; not with `--sessions`, `--manifest`, `--rewards`): τ²-bench results to learn from in place of `--sessions` (repeatable): their episodes on the training tasks of the `--tau2` checkout's split, with their rewards. The tools come from the checkout.
- `--tau2 <DIR>`: The τ²-bench checkout that `--results` belong to.
- `--train-fraction <SHARE>` (default `1`; not with `--train-tasks`): With `--results`, learn from this share of the training tasks: the sample `compile --train-fraction` takes.
- `--train-tasks <IDS>` (repeatable; not with `--train-fraction`): With `--results`, learn from these training tasks only (comma-separated), in place of `--train-fraction`.
- `--trials <N>...` (repeatable): With `--results`, only these trials of each task (default: all).
- `--domain <NAME>` (required): The domain to name the flow for.
- `--manifest <FILE>` (not with `--results`): A tool manifest (JSON, as stretto-trace writes it) instead of the sessions' own `tools/list`.
- `--rewards <FILE>` (not with `--results`): Rewards by session id (JSON object). Sessions without one count as successful.

**The arbiter**

- `--habit-only` (not with `--refit-habit`, `--arbiter-from`, `--manifest-options`, `--oracle`, `--oracle-cache`, `--oracle-budget`, `--predicates`): Ask no System-One model: every session trains the habit, and the flow has no arbiter (serve it with `--decider habit`).
- `--refit-habit` (not with `--habit-only`, `--arbiter-from`): Once the arbiter is fitted on the held-out sessions, learn the habit, the sites and the bindings again from every session.
- `--arbiter-from <FILE>` (not with `--habit-only`, `--refit-habit`, `--manifest-options`, `--oracle`, `--oracle-cache`, `--oracle-budget`, `--predicates`): Ask no System-One model while learning: every session trains the habit, and the flow serves this arbiter instead. It is an arbiter file (`export-arbiter`; `data/arbiters/` ships two), or a flow whose arbiter to take, such as one `compile` fitted on other agents' traces.
- `--manifest-options` (not with `--habit-only`, `--arbiter-from`): Offer every read-only tool at every site (see `compile`). Only the arbiter a flow fits here weighs such options.
- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `jev`; not with `--habit-only`, `--arbiter-from`): Who answers the held-out questions the arbiter is fitted on. `--habit-only` and `--arbiter-from` ask nothing, so they take none of the oracle's options.
- `--oracle-cache <DIR>` (default `.oracle-cache`; not with `--habit-only`, `--arbiter-from`): Replay cache for oracle answers.
- `--oracle-budget <DOLLARS>` (default `1`; not with `--habit-only`, `--arbiter-from`): Refuse to start if uncached questions could cost more than this many dollars.
- `--predicates <FILE>` (not with `--habit-only`, `--arbiter-from`): Yes/no predicates to ask and weigh (see `data/predicates-v2.json`). An arbiter from `--arbiter-from` brings its own.

**Output**

- `--out <FILE>` (required): Where to write the flow.

### `stretto serve`

Serve a compiled flow (from `compile`), as `flow-serve` does.

```text
Usage: stretto serve [OPTIONS] --flow <FILE>
```

**Inputs**

- `--flow <FILE>` (required): The flow IR to load.

**The System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `jev`): Who answers live questions: `jev` (needs TYPESAFE_API_KEY), `replay` (the cache only) or `mock`.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.

**Serving**

- `--listen <ADDR>` (default `127.0.0.1:0`): Address to listen on (port 0: any free port; the ready line on stderr names it).
- `--threshold <P>` (default `0.3`): Take the most likely lookup when its probability, times the chance that its arguments are the agent's, is at least this.
- `--decider <DECIDER>` (one of `arbiter`, `habit`; default `arbiter`): Where each option's probability comes from: `arbiter` (the habit, the System-One model and the predicates, combined: arm D0) or `habit` (the habit alone, never asking the System-One model: a flow compiled from traces only, arm C at a high threshold).
- `--max-questions <N>` (default `300`): Stop asking the System-One model after this many live questions.
- `--log <FILE>`: Append every query's answer here (JSON lines).

### `stretto guards`

Test the policy guards (typed checks a proxy runs before a write) against recorded τ²-bench trajectories: every write is checked against what came before it, as `stretto-proxy --guards` checks it. A rule that fails the writes of successful episodes is too strict, or wrong.

```text
Usage: stretto guards [OPTIONS] --tau2 <DIR>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout (its published baselines are read).
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to audit.
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to audit: `path` or `label=path` (repeatable; files for other domains are skipped).

**Output**

- `--out <FILE>`: Write the Markdown report here (default: stdout).
- `--json <FILE>`: Also write the audit as JSON here.

### `stretto confirm`

Judge the customer's confirmation before each write with a System-One model, next to the guards' word list, on τ²-bench's published trajectories: one yes/no question per write, sorted by whether the tool accepted it and the episode passed.

```text
Usage: stretto confirm [OPTIONS] --tau2 <DIR>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout (its published baselines are read).
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to judge.
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to judge: `path` or `label=path` (repeatable; files for other domains are skipped).

**The judge**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `replay`): Who judges: `jev` (needs TYPESAFE_API_KEY; pays once per distinct question), `replay` (the cache only) or `mock`.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--oracle-budget <DOLLARS>` (default `2`): Refuse to start if uncached questions could cost more than this many dollars.
- `--threshold <P>` (default `0.5`): The judge fails a write below this probability of an explicit yes.
- `--second-question <SECOND_QUESTION>` (one of `described`, `proposed`): Also ask a second question about each write: whether the agent had `proposed` this change before the customer's reply, or (the first wording, too literal) whether its message `described` it. The judge then fails a write unless both answers are yes.

**Output**

- `--examples <N>` (default `8`): Disagreements to show, of each kind, per domain.
- `--oracle-dump <FILE>`: Write every distinct question to this file (JSON lines; the domain is added to the file name).
- `--out <FILE>`: Write the Markdown report here (default: stdout).
- `--json <FILE>`: Also write every judged write, and the audits, as JSON here.

### `stretto match`

Match descriptions to records (RFC-001): at each write in τ²-bench's published trajectories that picks records out of earlier results (items of an order, a new variant, a payment method, a reservation), ask the System-One model which one the customer means, without the agent's pick, and score both against the task's expected actions.

```text
Usage: stretto match [OPTIONS] --tau2 <DIR>
```

**Inputs**

- `--tau2 <DIR>` (required): Path to a τ²-bench checkout.
- `--domain <NAME>` (repeatable; default `retail`, `airline`): Domains to judge.
- `--source <[LABEL=]PATH>` (repeatable): Extra τ²-bench results to judge, besides the published baselines (repeatable).

**The System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `jev`): Who answers: `jev` (needs TYPESAFE_API_KEY), `replay` (the cache only) or `mock`.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--oracle-budget <DOLLARS>` (default `2`): Refuse to start if uncached questions could cost more than this many dollars.

**Output**

- `--examples <N>` (default `8`): Disagreements to show, of each kind, per domain.
- `--oracle-dump <FILE>`: Write every distinct question to this file (JSON lines; the domain is added to the file name).
- `--out <FILE>`: Write the Markdown report here (default: stdout).
- `--json <FILE>`: Also write every choice, and the audits, as JSON here.

### `stretto audit`

Audit a flow against recorded episodes: score the agent's own steps under the flow's decisions, run as a fugue program, for agreement, calibration and surprise per site and per episode. Run it on new sessions before trusting a flow compiled from older ones.

```text
Usage: stretto audit [OPTIONS] --flow <FILE>
```

**Inputs**

- `--flow <FILE>` (required): The flow IR to audit.
- `--sessions <DIR>`: Sessions recorded by stretto-proxy (a directory of `*.jsonl`).
- `--results <FILE>` (repeatable): τ²-bench results files (repeatable); files for other domains are skipped.
- `--tau2 <DIR>`: With --results: keep only the test split of this τ²-bench checkout, the tasks a flow compiled from it never trained on.

**The System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `replay`): Who answers the flow's questions: `replay` (the cache only; decisions it cannot answer are left out), `jev` (needs TYPESAFE_API_KEY; about $0.0001 per decision) or `mock`.
- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache for oracle answers.
- `--decider <DECIDER>` (one of `arbiter`, `habit`): How the flow decides: `arbiter` (weighing the System-One model's answers) or `habit` (the habit alone, asking nothing). Default: the arbiter, or the habit for a flow without one (`learn --habit-only`).

**Output**

- `--out <FILE>`: Write the Markdown report here (default: stdout).
- `--json <FILE>`: Also write the audit as JSON here.

### `stretto jev-check`

Check that Jev is reachable with TYPESAFE_API_KEY: ask one small question (uncached) and print the answer, model version and latency.

```text
Usage: stretto jev-check
```

### `stretto export-arbiter`

Write a flow's arbiter to its own file, to ship: the predicates it weighs, the model it asks, and one fit of its weights. `learn --arbiter-from` serves it with a habit learned from new sessions. The flow's folds must share one fit (`compile --pooled-arbiter`, or a flow from `learn`).

```text
Usage: stretto export-arbiter --flow <FILE> --out <FILE>
```

**Options**

- `--flow <FILE>` (required): The flow whose arbiter to write.
- `--out <FILE>` (required): Where to write it.

### `stretto export-answers`

Write every cached oracle answer to stdout as JSON lines (`{"key", "response"}`). Answers carry no benchmark text, so the bundle can be shared to replay Phase 0b without a key.

```text
Usage: stretto export-answers [OPTIONS]
```

**Options**

- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache to read.

### `stretto import-answers`

Read JSON lines from `export-answers` on stdin into a replay cache.

```text
Usage: stretto import-answers [OPTIONS]
```

**Options**

- `--oracle-cache <DIR>` (default `.oracle-cache`): Replay cache to fill.

<!-- end stretto -->

<!-- begin stretto-proxy -->

## `stretto-proxy`

Forward a stdio MCP server's traffic, record it for stretto, and optionally run a flow, policy guards and a commit tool on it. Put this in an MCP host's configuration in place of the server's command, with the real command after `--`. stdout carries only the protocol; the proxy's own messages go to stderr. The exit status is the server's, or 125 if the proxy itself fails. Without --flow, --guards, --commit, --context or --confirm-judge, every line is forwarded byte for byte and nothing is parsed.

```text
Usage: stretto-proxy [OPTIONS] -- <SERVER_COMMAND>...
```

**Recording**

- `--record <DIR>`: Write a session log (JSONL) into this directory, created if missing. A leading `~` is expanded, since hosts start servers without a shell.
- `--domain <NAME>`: Domain for the log header, e.g. `retail`; --guards uses its rules.
- `--agent-model <MODEL>`: Model that drives the agent, for the log header.

**Flows**

- `--flow <FILE>`: Run this flow (from `stretto compile` or `stretto learn`) after each of the agent's calls, and append its lookups to the result.
- `--flow-threshold <P>` (default `0.3`): Take a lookup when the tool's probability times its arguments' agreement is at least this.
- `--flow-decider <FLOW_DECIDER>` (one of `arbiter`, `habit`; default `arbiter`): Where the tool's probability comes from: `arbiter` (the habit, the System-One model's answers and the predicates, combined) or `habit` (the habit alone, which never asks a System-One model and needs no key).
- `--flow-per-call <N>` (default `8`): Lookups appended to one result, at most.
- `--flow-per-session <N>` (default `40`): Lookups per session, at most.
- `--flow-questions <N>` (default `300`): Questions to the System-One model per session, at most.
- `--flow-log <FILE>`: Append the flow's decisions here (default: next to the session log).
- `--task-id <ID>`: Task id, which picks the flow's fold (default: the session).

**The System-One model**

- `--oracle <ORACLE>` (one of `jev`, `replay`, `mock`; default `jev`): Who answers the flow's and the confirmation judge's questions: `jev` (needs TYPESAFE_API_KEY), `replay` (the cache only) or `mock`.
- `--oracle-cache <DIR>` (default `~/.stretto/oracle-cache`): Replay cache for the System-One model's answers.

**Writes**

- `--guards`: Check each of the agent's calls against the policy guards of --domain (`retail` or `airline`), and refuse the ones they fail.
- `--confirm-judge <MODE>` (one of `log`, `enforce`): Put each write the guards check for a confirmation to the System-One model too, with the questions `stretto confirm` asks: `log` records each judgment; `enforce` also refuses a write the judge fails. Needs --guards and --context. A judge that cannot answer refuses nothing.
- `--confirm-second <QUESTION>` (one of `proposed`, `described`): Also ask the second question (`proposed`: had the agent proposed the change?); a write then fails unless both answers are yes.
- `--confirm-threshold <P>` (default `0.5`): A write fails when an answer's probability of a yes is below this.
- `--confirm-questions <N>` (default `100`): The judge's questions per session, at most.
- `--confirm-log <FILE>`: Append the judgments here (default: next to the session log).
- `--commit`: Add `stretto_commit`, which makes several calls in one, in order, each checked by the guards.

**The conversation**

- `--context <FILE>`: Read the conversation from this file, which the host appends to as JSON lines: `{"role": "user" | "assistant", "content": text}`.

**Arguments**

- `<SERVER_COMMAND>...` (required): The MCP server to run, and its arguments.

<!-- end stretto-proxy -->
