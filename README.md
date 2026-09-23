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
  $(scripts/fetch-targets.sh | sed 's/^/--target /') --out reports/phase0.md
```

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
  - with a System-One model that always agrees with the agent, savings reach 22.0% (retail) and 20.6% (airline), against a 23–24% ceiling;
  - input tokens fall faster (26–28%) and output tokens slower (19–25%). Dollars fall 32–38%, but that figure is dominated by Claude 3.7 Sonnet's uncached runs.
- **The gate depends on how the agent calls tools.** Models that call tools one at a time leave the most to collapse: Claude 3.7 Sonnet saves 33–37%, o4-mini 23–25%. Models that batch parallel calls save less: GPT-4.1 11–19%, GPT-4.1-mini 7–12%. GLM-5, never trained on, saves 28.5% in retail but only 15.4% in airline, where its ceiling is 18.9%.
- **Validated arbitration makes the habit a stopping rule.** Requiring ≥99% cross-validated agreement per context leaves only contexts where the habit hands back to the LLM. That is safe (no risky decisions on held-out tasks), but it leaves about 7 decisions per episode to Jev. Phase 0b measures how well Jev takes them.

## Crates

| Crate | What it does |
|---|---|
| `stretto-trace` | Canonical episode schema; τ²-bench results ingest; tool manifests |
| `stretto-model` | Action abstraction; hierarchical Dirichlet back-off world model; concentration posterior via fugue; argument provenance; tool runs; policy checks |
| `stretto-oracle` | `Oracle` trait; Jev HTTP client (`POST /v1/systemone`); on-disk replay cache; mock |
| `stretto-report` | The `stretto` CLI and the Phase 0 report |

## Roadmap

| Phase | What | Needs |
|---|---|---|
| 0a | Predictability, headroom and provenance on published trajectories | Nothing (done) |
| 0b | Replayed shadow mode: ask Jev at every recorded decision, and measure agreement and calibration for its four roles | `TYPESAFE_API_KEY` |
| 1 | Rust MCP proxy that records traffic; rule checks compiled from policy and tested against traces | — |
| 2 | Flow compiler; `plan_*` / `resume_*` / `commit_*` macro-tools; arbitration runtime; live τ²-bench arms on GLM and MiniMax | GLM (Z.ai) and MiniMax keys |
| 3 | Predicate refinement, per-decision counterfactual evaluation, flow search with fugue-evo, then American frontier models | — |

## Development

```bash
cargo test            # unit tests, plus Phase 0 on a miniature fixture checkout
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

MIT
