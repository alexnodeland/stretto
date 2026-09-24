# stretto

**Compile LLM agent behavior into typed probabilistic flows.**

In a fugue, a *stretto* is where entries of the subject overlap and compress. stretto does that to agent traces. It watches an agent's tool calls, learns a Bayesian model of what the agent does over the tools it is allowed to use, and compiles the predictable parts into flows. Each branch point in a flow is resolved by the cheapest source that is good enough:

- the learned habit;
- a System-One model (TypeSafe's [Jev](https://typesafe.ai)), which returns typed choices with calibrated probabilities in about 100 ms;
- the LLM;
- or a person.

Flows are [fugue](https://github.com/alexnodeland/fugue) programs. The same flow can be simulated, executed, audited against recorded traces and evaluated counterfactually, just by swapping its interpreter.

The design is fugue's [RFC-001](https://github.com/alexnodeland/fugue/blob/main/docs/decisions/rfc/001-habit-compiler.md), and the decisions are summarized in [docs/design.md](docs/design.md).

## Status

Pre-alpha. **Phase 0 (measure) runs today, with no API keys**, on τ²-bench's published trajectories:

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

The published answers replay without a key: see [the v2 bundle](docs/results/phase0b-v2-2026-09-23-answers.md) and [the goal-free supplement](docs/results/phase0b-v2-goal-free-2026-09-24-summary.md#answers).

First results, with their interpretation, are in [docs/results/phase0-2026-09-23.md](docs/results/phase0-2026-09-23.md). The headlines for airline and retail:

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
- **Live pilot (in progress).** [`pilot/`](pilot/README.md) runs τ²-bench retail episodes live. GLM-5.3 is the agent, in Claude Code on Z.ai's coding endpoint, and its tools are served over MCP behind `stretto-proxy`. The flows arm adds a read-only flow behind the tools: `stretto flow-serve` compiles it goal free from cached answers, then answers each step with a lookup and its bound arguments, or hands back. A baseline smoke episode passed. Before any flows episode, the flow is checked on GLM-5's recorded episodes without an LLM ([`check_flow.py`](pilot/check_flow.py)).

## Crates

| Crate | What it does |
|---|---|
| `stretto-trace` | Canonical episode schema; τ²-bench results ingest; MCP proxy log ingest; tool manifests with docs |
| `stretto-model` | Action abstraction; hierarchical Dirichlet back-off world model; concentration posterior via fugue; argument provenance; tool runs; policy checks |
| `stretto-oracle` | `Oracle` trait; Jev HTTP client (`POST /v1/systemone`); on-disk replay cache; mock |
| `stretto-report` | The `stretto` CLI, the Phase 0 report, Phase 0b (System-One questions at held-out decisions), and live read-only flows (`stretto flow-serve`) |
| `stretto-proxy` | A stdio MCP proxy that forwards every line unchanged and records sessions for `stretto-trace` ([README](crates/stretto-proxy/README.md)) |

## Roadmap

| Phase | What | Needs |
|---|---|---|
| 0a | Predictability, headroom and provenance on published trajectories | Nothing (done) |
| 0b | Replayed shadow mode: ask Jev at every decision a flow would hand it (next step, closed-set arguments), score agreement and calibration, and re-run the projection with its answers. First run done (v1 questions); next: narrower questions, Bayesian arbitration, matching descriptions to records | `TYPESAFE_API_KEY` (set) |
| 1 | Rust MCP proxy that records traffic (recording done: `stretto-proxy`); rule checks compiled from policy and tested against traces | — |
| 2 | Flow compiler; `plan_*` / `resume_*` / `commit_*` macro-tools; arbitration runtime; live τ²-bench arms on GLM and MiniMax. Started: live read-only flows (`flow-serve`) and the retail pilot harness ([`pilot/`](pilot/README.md)) | GLM (Z.ai coding plan, set); MiniMax |
| 3 | Predicate refinement, per-decision counterfactual evaluation, flow search with fugue-evo, then American frontier models | — |

## Development

```bash
cargo test            # unit tests, plus Phase 0 on a miniature fixture checkout
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

MIT
