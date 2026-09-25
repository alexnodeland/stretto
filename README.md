# stretto

**Compile LLM agent behavior into typed probabilistic flows.**

In a fugue, a *stretto* is where entries of the subject overlap and compress. stretto does that to agent traces. It watches an agent's tool calls, learns a Bayesian model of what the agent does over the tools it is allowed to use, and compiles the predictable parts into flows. Each branch point in a flow is resolved by the cheapest source that is good enough:

- the learned habit;
- a System-One model (TypeSafe's [Jev](https://typesafe.ai)), which returns typed choices with calibrated probabilities in about 100 ms;
- the LLM;
- or a person.

Flows are [fugue](https://github.com/alexnodeland/fugue) programs. The same flow can be simulated, executed, audited against recorded traces and evaluated counterfactually, just by swapping its interpreter.

The design is [RFC-001](docs/rfc/001-habit-compiler.md), first accepted in fugue's decision log, and the decisions are summarized in [docs/design.md](docs/design.md). The working paper, with every result so far, is at [alexnodeland.github.io/stretto](https://alexnodeland.github.io/stretto/) (source: [`site/`](site/index.html)).

## Status

Pre-alpha, with every piece of the first design built ([implementation status](docs/design.md#implementation-status-2026-09-24)). 0.1.0 is the first release ([changelog](CHANGELOG.md)). It is not on crates.io yet, because it depends on a fugue release that is not out yet (its `program` feature).

## Use it with your agent

`stretto-proxy` wraps any MCP server, run as a command or reached over Streamable HTTP (`--upstream`). Record your agent's sessions through it, learn a flow from them, serve the flow behind the agent's calls, and audit it on new sessions:

```bash
cargo install --git https://github.com/alexnodeland/stretto --tag v0.1.0 stretto-proxy stretto-report
# or, from a checkout: cargo install --path crates/stretto-proxy && cargo install --path crates/stretto-report

# 1. In the MCP host's config, run the server behind the proxy. The host may append
#    the conversation to the context file, one JSON line per message.
stretto-proxy --record ~/.stretto/logs --domain orders --context ~/.stretto/context.jsonl -- <server command>

# 2. Learn a flow from the recorded sessions (asks Jev held-out questions: TYPESAFE_API_KEY).
stretto learn --sessions ~/.stretto/logs --domain orders --out ~/.stretto/orders.flow.json

#    Review it: the tools it may call, the lookups it may make and where their arguments
#    come from. After learning again, `stretto flow-diff OLD NEW` lists what changed.
stretto flow-show ~/.stretto/orders.flow.json

# 3. Run it in shadow first: it decides and logs, but looks nothing up. Then promote it,
#    so it acts only after the calls where its lookups were the agent's own.
stretto-proxy --record ~/.stretto/shadow --domain orders --flow ~/.stretto/orders.flow.json --flow-shadow -- <server command>
stretto promote --flow ~/.stretto/orders.flow.json --sessions ~/.stretto/shadow \
  --oracle-cache ~/.stretto/oracle-cache --out ~/.stretto/orders-promoted.flow.json

#    Serve it: after each of the agent's calls, the flow's lookups ride in the same result.
stretto-proxy --record ~/.stretto/logs --domain orders --flow ~/.stretto/orders-promoted.flow.json -- <server command>

# 4. Before trusting it on new sessions, audit it.
stretto audit --flow ~/.stretto/orders.flow.json --sessions ~/.stretto/logs --oracle jev
```

Flows only call tools the server marks `readOnlyHint: true`. `--flow-decider habit` serves a flow on its habit alone, which asks no System-One model and needs no key. Replayed, it saves as many turns as the arbiter, with more detours in airline ([results](docs/results/arms-2026-09-24.md)), and a cold start needs no key either. From five of an agent's own sessions, `stretto learn --habit-only` saved more than a flow whose arbiter was fitted on those sessions, which trails it until about twenty ([results](docs/results/cold-start-2026-09-24.md)). `--arbiter-from data/arbiters/retail.json` adds an arbiter shipped with stretto, fitted on four agents' published decisions, as insurance against a narrow start. It works between retail and airline ([results](docs/results/arbiter-transfer-2026-09-24.md)), but not in telecom, a domain unlike both, where the habit alone did better ([results](docs/results/telecom-2026-09-25.md)). `--commit` adds `stretto_commit`, for confirmed writes in one call. `--guards` checks writes against the policy guards compiled for τ²-bench's retail and airline domains. See the [proxy's README](crates/stretto-proxy/README.md). A flow is a JSON file to review before serving it. `stretto flow-show` renders it for a reviewer, and `stretto flow-diff` lists what changed between two flows, exiting with 1 when a change needs review, such as a lookup the flow could not make before ([reviewing flows](docs/review.md)). `stretto-proxy --flow-shadow` runs a flow without letting it act, and `stretto promote` then keeps it to the sites where its lookups were the agent's own. Replayed, that halved D0's detours and kept 94% of its retail savings and all of its airline savings ([results](docs/results/promotion-2026-09-25.md)). [docs/formats.md](docs/formats.md) says what each field holds, and [docs/privacy.md](docs/privacy.md) what each file keeps and what is sent to Jev; `stretto redact` pseudonymizes sessions to share. Every command and option is in [the CLI reference](docs/cli.md). [The walkthrough](docs/walkthrough.md) runs the whole loop on the official MCP filesystem server, with no key.

## Reproduce the results

[The CLI reference](docs/cli.md) lists every option of the commands below.

**Phase 0 (measure) runs with no API keys**, on τ²-bench's published trajectories:

```bash
git clone --depth 1 https://github.com/sierra-research/tau2-bench ../tau2-bench
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench --out reports/phase0.md
```

It takes about 20 seconds. To also measure transfer to GLM-5, whose trajectories Sierra publishes with its τ²-bench leaderboard entry, fetch them and pass them as targets:

```bash
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench \
  $(scripts/fetch-leaderboard.sh | sed 's/^/--target /') --out reports/phase0.md
```

**Phase 0b** asks a System-One model at every held-out decision a flow would hand it: right after a tool returns, which tool comes next or whether to hand back, and each closed-set argument. It needs `TYPESAFE_API_KEY` in the environment:

```bash
cargo run --release -p stretto-report -- jev-check     # key, TLS and latency, one question
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench \
  $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle jev --out reports/phase0b.md --json reports/phase0b.json
```

Answers are cached in `.oracle-cache/`, so each distinct question is paid for once. A full run is about 9,000 questions, roughly $1 at Jev's price. The options:

- `--questions v2` asks the questions RFC-001 §3.5 specifies, for read-only flows. It combines the answers with the habit (§3.6). The default, `v1`, reproduces the first run.
- `--predicates data/predicates-v2.json` also asks yes/no questions about the state, and weighs them in the combination.
- `--dataflow-hints` describes each lookup by what its results supply.
- `--oracle-budget` refuses to start above a dollar limit (default $5).
- `--oracle-limit` asks a stable sample, for a pilot.
- `--oracle mock` checks the pipeline for free.
- `--oracle-dump` writes every question for audit.
- `--oracle-log` writes every decision with the agent's step and the picks.

The published answers replay without a key: see [the v1 bundle](docs/results/phase0b-2026-09-23-answers.md), [the v2 bundle](docs/results/phase0b-v2-2026-09-23-answers.md) and [the goal-free supplement](docs/results/phase0b-v2-goal-free-2026-09-24-summary.md#answers).

[docs/results](docs/results/README.md) indexes every results page. First results, with their interpretation, are in [docs/results/phase0-2026-09-23.md](docs/results/phase0-2026-09-23.md). The headlines for airline and retail:

- **Macro-tool headroom.** About a quarter of LLM turns are spent inside runs of consecutive tool calls that a macro-tool could perform in one call.
- **Arguments can mostly be bound.** Identifiers, items, payment methods and flights in write calls almost always appear verbatim in an earlier tool output or user message, so flows can bind them instead of generating them. The values agents actually generate are mostly closed-set choices (a cancellation reason, a flight type), which suit a Jev `Choice`, and arithmetic, which belongs in code.
- **A habit that only sees the tool sequence is not enough.** It can take just 6–8% of decisions right after a tool returns at 80% confidence.
  - Code features read from tool outputs help a little: they are kept only if they help on held-out tasks, which rejects fields that merely identify a task.
  - Naming the intent, as a macro-tool call does, lifts retail coverage from 15% to 24%.
  - About three quarters of decisions still need content-aware judgment: that is Phase 0b's job, with Jev.
- **Behavior transfers across models.** A habit learned from one model predicts another within 3–7 points of top-1 of that model's own habit.
- **Projection.** Replaying held-out episodes through macro-tool flows:
  - the habit alone saves at most 1.2% of LLM turns;
  - with a System-One model that always agrees with the agent, savings reach 22.3% (retail) and 20.6% (airline), against a 23–24% ceiling;
  - input tokens fall faster (27–28%) and output tokens slower (19–25%). Dollars fall 32–38%, but that figure is dominated by Claude 3.7 Sonnet's uncached runs.
- **The gate depends on how the agent calls tools.** Across nine current leaderboard models the habit never trained on, all clear 20% in retail, from 28.6% (Claude Opus 4.5) to 43.5% (Qwen3.5). In airline, the heaviest parallel callers fall short: Claude Opus 4.5 (13.4%), GLM-5 (15.4%) and GPT-5.2 with reasoning off (17.9%). Models that never call in parallel, such as Qwen3.5 and Qwen3-Max, leave the most to collapse.
- **Validated arbitration makes the habit a stopping rule.** Requiring ≥99% cross-validated agreement per context, from at least 10 distinct tasks, leaves one context per domain, and both hand back to the LLM. That is safe (no risky decisions on held-out tasks), but it leaves about 7.5 decisions per episode to Jev. Counting tasks matters: a context validated on 28 decisions from a few tasks held only 43% on new ones.
- **Compile from current frontier runs.** Habits from Claude Opus 4.5, Sonnet 4.5 and Gemini 3 predict GLM-5 as well as its own habit in retail (66–67%) and better in airline (62% against 57%); the 2025 baselines do worse. `--no-baselines --source ...` trains on them.
- **Phase 0b: zero-shot Jev is calibrated but not accurate enough to carry flows.** jev-1.13.0 answered 8,946 held-out questions for $1.19. It agrees with the agent's next step 72% of the time (ECE 0.06; 92–93% on the third of decisions where it is at least 0.9 sure). Trusted at p ≥ 0.9, flows save only 3.9–4.7% of LLM turns, with a risky call in 6–13% of episodes. A two-key rule (Jev must match the habit) barely helps. Next: the narrower questions and Bayesian arbitration of the design. See [the summary](docs/results/phase0b-2026-09-23-summary.md).
- **Phase 0b v2: read-only flows remove the risk; sequential callers clear the gate offline.** See [the v2 summary](docs/results/phase0b-v2-2026-09-23-summary.md).
  - Under the design's plan/commit rule, flows only read between LLM turns, so a wrong pick is an extra lookup (a detour) rather than a risk. That costs little ceiling: GLM-5 drops from 29.1% to 28.2% in retail.
  - Narrower questions did not make Jev more accurate: 73–78% on the same decisions.
  - Combining its answers with the habit and three state predicates lifts agreement to 80.5% (retail) and 78.9% (airline). Dataflow hints add nothing.
  - Because detours are harmless offline, flows can act on weaker picks and take the likeliest lookup. They then save 15–17% of LLM turns in retail and 10–13% in airline, pooled over the 2025 baselines. Detours then show up in half to two thirds of episodes.
  - Across nine current models:
    - Qwen3.5 clears 20% offline in both domains (29.7% and 22.5%). Qwen3-Max does too, but in airline only with lookup first.
    - Gemini 3 and Claude Sonnet 4.5 clear it in retail, and so does GPT-5.2 with reasoning off, with lookup first.
    - GLM-5 clears it in retail only with predicates (20.5%). The heavy parallel callers stay under 7% in airline.
- **Goal-free flows lose nothing offline.** A live flow continues the agent's lookups, and nobody names the episode's goal. `--no-intent` stops conditioning the habit on the goal and leaves it out of the questions. Agreement and savings hold:
  - Retail: 80.5% combined agreement, and 17.3% of turns saved with lookup first at p ≥ 0.3.
  - Airline: 78.4% and 12.6%.
  - GLM-5 still clears 20% in retail (20.3%).
  - See [the goal-free summary](docs/results/phase0b-v2-goal-free-2026-09-24-summary.md).
- **Live pilots.** [`pilot/`](pilot/README.md) runs τ²-bench retail and airline episodes live. GLM-5.3 is the agent, in Claude Code on Z.ai's coding endpoint, and its tools are served over MCP behind `stretto-proxy`. The flows arm adds a read-only flow behind the tools: `stretto flow-serve` compiles it goal free from cached answers, then answers each step with a lookup and its bound arguments, or hands back. First results ([details](pilot/README.md#results-so-far)):
  - **Replay check, no LLM** ([`check_flow.py`](pilot/check_flow.py)). GLM-5's recorded retail test episodes, replayed through the live flow, save 21.2% of LLM turns over all four trials (the offline projection for the same episodes is 20.3%). 91% of the flow's lookups are the agent's own.
  - **Live smoke episode (task 90).** GLM-5.3 used the flow's lookups without repeating them: 8 LLM turns instead of 12, 28% fewer input tokens, and the same database outcome.
  - **Paired pilot, ten random retail test tasks.** With the flow, GLM-5.3 took 23.6% fewer LLM turns (2.6 per episode, 95% interval 1.0 to 4.2) and read 21% fewer input tokens. 8 of 10 passed in each arm, and the two failures were the same agent error in both. See [the summary](docs/results/pilot-2026-09-24.md).
  - **Paired pilot, ten random airline test tasks.** 17.3% fewer LLM turns (2.2 per episode, 95% interval 0.5 to 3.9) and 15% fewer input tokens. 8 of 10 passed without the flow and 9 of 10 with it, and no failure came from a flow decision. GLM-5.3 in Claude Code calls tools one at a time, so airline saved far more than the projection for GLM-5 in τ²-bench's own harness, a heavy parallel caller. See [the summary](docs/results/pilot-airline-2026-09-24.md).
  - **Paired run, every retail and airline test task** (80 pairs, airline twice). 25.5% fewer LLM turns (95% interval 20.5% to 30.4%) and 21.0% fewer input tokens. 70 of 80 passed with the flow and 71 without it: −1.25 points (−7.5 to +6.25), too wide to rule out a loss of a few points. Bounding a one-point loss would take about 4,300 pairs. 242 of the flow's 259 lookups were the agent's own. See [the summary](docs/results/paired-2026-09-25.md).
  - **Claude models.** `--agent-cli claude` and `--customer-cli claude` run a Claude model as the agent or the customer. On the retail pilot's ten tasks, the same flow saved Claude Haiku 4.5 18.6% of LLM turns (95% interval 10.1% to 27.4%), where it saved GLM-5.3 23.6%: Haiku makes more calls at once. It saved Claude Sonnet 5 24.1% on three tasks. With Claude Sonnet 5 as the customer to GLM-5.3, on five tasks, it saved 23.1% against 25.8% with GLM-5.3 as the customer, so a customer played by the agent's own model had not inflated the savings. See [the summary](docs/results/claude-models-2026-09-25.md).
- **Flow audit, with fugue.** `stretto audit` scores recorded episodes under a flow, run as a fugue program with `ScoreGivenTrace`: agreement, calibration, and surprise per site and per episode. On GLM-5's published test episodes, the pilots' retail flow picks the agent's own step 85.9% of the time (0.42 nats per decision), and the airline flow 64.5% (1.02). The audit also shows where the airline flow fails: GLM-5 in τ²-bench's harness reads several reservations at once. See [the audit](docs/results/audit-2026-09-24.md).
- **Policy guards.** Typed checks compiled from the written policy run before a write: 12 rules for retail, 10 for airline. Each passes, fails, or does not know. `stretto guards` tests them against τ²-bench's published trajectories. In airline, the enforced rules would refuse a write the tool accepted in 38% of failed episodes and 1% of successful ones, all five of which τ²-bench's own task notes forbid. In retail the tools already enforce most of their policy. See [the audit](docs/results/guards-2026-09-24.md). Live, on the airline tasks where those refused writes concentrate, GLM-5.3 made none: 4 of 4 passed in both arms, with nothing refused and no harm in four more checks ([the pilot](docs/results/pilot-guards-2026-09-24.md)). Guards are insurance whose value depends on the agent.
- **Arm C, the habit alone, and naming.** Replayed on GLM-5's and Claude 3.7 Sonnet's recorded test episodes, a flow that hands every branch back (arm C) saves 0–1.7% of turns. The habit alone, without Jev, saves as many turns as the arbiter (1–3 points more); what the arbiter's weighed Jev answers buy is fewer detours, mostly in airline. Naming flows as macro-tools could add at most 1.9% of turns in retail and 7.9% in airline, so `plan_*` is not built. See [the arms results](docs/results/arms-2026-09-24.md).
- **The habit alone, live.** On the retail pilot's ten tasks, the habit-only flow took 79 LLM turns, against 110 with no flow and 84 with D0 (with Jev). It passed 9 of 10, and asked Jev nothing ([the pilot](docs/results/pilot-habit-2026-09-24.md)).
- **Fewer traces.** `--train-fraction` trains the habit on a nested share of the training tasks. From 7 retail tasks and 8 airline tasks, the habit alone saves as many turns as with all of them. Below that, Jev's answers carry the savings. With 3 retail tasks, the flow saves 20.4% of turns with the arbiter and 1.8% on the habit alone. The traces still set where a flow can act: with one airline task, neither saves a turn ([results](docs/results/sweep-2026-09-24.md)). Correction: those samples were clustered by task id rather than pseudo-random. The sampler now mixes its order, and `--train-tasks` names a sample exactly. A cold start from an agent's own random sessions does not bear the finding out (next item).
- **A cold start.** `stretto learn --results` learns a flow from an agent's first sessions, as a deployment would, and `--habit-only` does it without Jev. Replayed on GLM-5's test episodes, a flow from five of its own sessions saved 11.8–18.9% of retail turns on the habit alone, across four random draws. A flow whose arbiter was fitted on those sessions saved less on every draw. It caught up at about twenty sessions in retail, and never in airline. Which sessions matters more than how the flow decides: a clustered draw saved 2.7%, and an arbiter fitted on other agents' decisions (`--arbiter-from`) lifted it to 11.1%. Live, a flow learned from five sessions GLM-5.3 recorded through the proxy ran on three pilot tasks ([results](docs/results/cold-start-2026-09-24.md)). Live, on the retail pilot's ten tasks, the habit from five of GLM-5.3's recorded sessions took 79 LLM turns against 110 with no flow, making exactly the lookups the habit from four other agents made. With a shipped arbiter it took 70 ([results](docs/results/cold-start-live-2026-09-25.md)). In airline, both took 102 turns against 127, and the shipped retail arbiter cut the habit's detours from 8 to 1 ([results](docs/results/cold-start-live-airline-2026-09-25.md)).
- **A shipped arbiter.** [`data/arbiters/`](data/arbiters/) holds two arbiters, fitted on four agents' published retail and airline decisions (`compile --pooled-arbiter`, then `export-arbiter`). Served with a habit learned in the other domain, each did what that domain's own arbiter did, within about a point of turns saved. In retail that is insurance: a narrow start rose from 2.7% to 10.6% of turns. In airline it trades 1 to 3 points of savings for far fewer unneeded lookups ([results](docs/results/arbiter-transfer-2026-09-24.md)).
- **Jev as a confirmation judge.** `stretto confirm` asks Jev whether the customer explicitly agreed to each write. Where Jev and the guards' word list disagree, hand labels side with Jev 75% of the time. It catches the agent making a different change from the one the customer agreed to ([results](docs/results/confirm-2026-09-24.md)). The proxy asks it before each write, and logs or enforces the answer (`--confirm-judge`). Enforced live on 20 episodes, it refused none of 40 writes; logged on 20 more, it would have stopped 2 real lapses for 2 false alarms, and passes did not move. So the recommended setting enforces it ([results](docs/results/judge-live-2026-09-25.md)).
- **A second confirmation question.** `--second-question proposed` also asks whether the agent had proposed the change. It catches lapses the first question passes, such as a refund to a card the customer never named, or a whole order cancelled where three items were to go. Half the writes it flags in fresh blind labels are real lapses ([results](docs/results/confirm-second-2026-09-24.md)). It is logged, not enforced: live, 7 of its 8 flags were false alarms. A literal first wording failed most confirmed writes.
- **Matching descriptions to records.** `stretto match` asks Jev which item, variant, payment method or reservation a customer means, from their words alone. It picked the expected record 82% of the time, against 90% for the agents themselves, so it is no use as a check on writes ([results](docs/results/matching-2026-09-24.md)).
- **Options from the manifest.** `--manifest-options` lets a flow offer every read-only tool, not only the lookups its traces showed. On the sweep's one- and three-task samples it adds nothing in retail. In airline it adds detours: a lookup no trace showed is bound by argument name, and a flight search then gets the wrong trip. What limits a flow on few traces is its bindings, not its options ([results](docs/results/manifest-options-2026-09-24.md)).
- **Prompt injection.** Whatever Jev answers, a flow calls only the lookups compiled for the site: a test holds it to that with an oracle that names other tools. Inside that set, a note in a tool result steered the flow to the lookup an attacker named at 86 of 300 retail decisions, and a note to stop cut its lookups from 168 to 70. One sentence in each question telling Jev that tool results are data cut the steering to 19, at no cost on clean decisions, and a threshold of 0.5 as well cut it to 4. An injected claim in the agent's message passed 9 of 140 writes the confirmation judge had refused ([results](docs/results/injection-2026-09-25.md)).
- **Counterfactual evaluation.** `--flow-explore ε` makes a flow take another lookup that binds with probability ε and log the chance of what it took, and `stretto evaluate` estimates another rule from such logs: direct, IPS, self-normalized IPS and doubly robust. On replays of GLM-5's test episodes, the estimates match a rule's own replay for changes that keep a flow's chains, such as another threshold. They miss a changed decider's detours, since two-thirds of a flow's lookups follow its own previous one ([results](docs/results/evaluate-2026-09-25.md)).
- **Predicate refinement.** `stretto refine` shows a proposer the decisions where the arbiter is weakest, `phase0 --candidates` asks its candidate predicates alone, and `refine` keeps one when it raises the held-out likelihood of the agents' steps. One round in airline kept two of eight, which fitted the four agents the proposer read (0.654 to 0.624 nats per decision) but not GLM-5, which it never saw, and did not move the savings. The three hand-written predicates stay ([results](docs/results/refine-2026-09-25.md)).
- **Telecom.** τ²-bench's third domain compresses like the others: 30% macro-tool headroom, and the habit's top guess right 69% of the time. Jev is weaker there (67.9% agreement weighed with the habit, against about 80%), and a flow projects 15–26% fewer turns with frequent detours. The shipped arbiters, and one fitted on retail and airline together (`stretto fit-arbiter`), handed back too often there: they made 43–50% of the agent's lookups, against 66% for the habit alone ([results](docs/results/telecom-2026-09-25.md)).

## Crates

| Crate | What it does |
|---|---|
| `stretto-trace` | Canonical episode schema; τ²-bench results ingest; MCP proxy log ingest; tool manifests with docs |
| `stretto-model` | Action abstraction; hierarchical Dirichlet back-off world model; concentration posterior via fugue; argument provenance; tool runs; policy checks |
| `stretto-oracle` | `Oracle` trait; Jev HTTP client (`POST /v1/systemone`); on-disk replay cache; mock |
| `stretto-report` | The `stretto` CLI: the Phase 0 report; Phase 0b (System-One questions at held-out decisions); the arbiter; flows (`compile` a flow IR from τ²-bench results, `learn` one from recorded sessions, `serve` it, `export-arbiter` to ship an arbiter); policy guards and their audit (`guards`); the flow audit, with fugue (`audit`); the confirmation judge (`confirm`); matching descriptions to records (`match`); per-site promotion from shadow mode (`promote`); counterfactual evaluation (`evaluate`); predicate refinement (`refine`); flow search, with fugue-evo (`search`) |
| `stretto-proxy` | An MCP proxy for any MCP server, stdio or Streamable HTTP, that the host runs as a stdio server. It records sessions for `stretto-trace`, and in active mode runs a flow behind the agent's calls, refuses writes the guards fail, adds a `stretto_commit` tool, and logs the conversation a host hands it ([README](crates/stretto-proxy/README.md)) |

## Roadmap

What is left is tracked in [issue #1](https://github.com/alexnodeland/stretto/issues/1) and grouped, with context, in [docs/roadmap.md](docs/roadmap.md).

| Phase | What | Needs |
|---|---|---|
| 0a | Predictability, headroom and provenance on published trajectories | Nothing (done) |
| 0b | Replayed shadow mode: ask Jev at every decision a flow would hand it (next step, closed-set arguments), score agreement and calibration, and re-run the projection with its answers. Done: v1 questions, then v2's narrower questions and Bayesian arbitration; Jev as a confirmation judge, with a second question; matching descriptions to records (no better than the agents) | `TYPESAFE_API_KEY` (set) |
| 1 | Rust MCP proxy that records traffic; rule checks compiled from policy and tested against traces. Done: `stretto-proxy`, `stretto guards` | — |
| 2 | Flow compiler; arbitration runtime; macro-tools; live τ²-bench arms. Done: the flow IR (`compile`, `learn`, `serve`); read-only flows live in the proxy (arm D0) and in the pilot harness, with paired pilots in retail and airline on GLM-5.3; guards and `stretto_commit` in the proxy. Measured: arm C (a flow that hands every branch back) saves 0–1.7% of turns; the habit alone saves as much as the arbiter, live too, and from a deployment's first five sessions it saves more than an arbiter fitted on them; and naming a flow could add at most 2% of turns in retail and 8% in airline, so `plan_*` is not built. A flow learned from five recorded sessions ran live. A paired run on every test task saved 25.5% of turns (95% interval 20.5% to 30.4%) and moved the pass rate by −1.25 points (−7.5 to +6.25) ([#2](https://github.com/alexnodeland/stretto/issues/2)). The cold start ran live on ten tasks in each domain ([#3](https://github.com/alexnodeland/stretto/issues/3)). The confirmation judge ran enforced live ([#6](https://github.com/alexnodeland/stretto/issues/6)), and Claude models as the agent and the customer ([#4](https://github.com/alexnodeland/stretto/issues/4), [#5](https://github.com/alexnodeland/stretto/issues/5)) | GLM (Z.ai coding plan, set); Claude, through Claude Code |
| 3 | Done: per-decision counterfactual evaluation ([#15](https://github.com/alexnodeland/stretto/issues/15)); predicate refinement ([#16](https://github.com/alexnodeland/stretto/issues/16)), whose first round found nothing that carries to an unseen agent; flow search with fugue-evo ([#25](https://github.com/alexnodeland/stretto/issues/25)), whose front held flows with far fewer detours than the hand-set flow on held-out tasks in both domains, each changing one or two sites' thresholds; and two Claude models live ([#5](https://github.com/alexnodeland/stretto/issues/5)). Next: the searched flows live ([#34](https://github.com/alexnodeland/stretto/issues/34)), and flows serving a smaller agent ([#8](https://github.com/alexnodeland/stretto/issues/8)) | — |

## Development

```bash
cargo test            # unit tests, plus Phase 0 on a miniature fixture checkout
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

MIT
