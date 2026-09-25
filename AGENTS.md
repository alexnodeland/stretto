# Agent context: stretto

stretto compiles LLM agent behavior into typed probabilistic flows. It learns a Bayesian model of an agent's tool calls, and resolves each branch point with a learned habit, a System-One model (TypeSafe's Jev), the LLM or a person. The design is RFC-001 (`docs/rfc/001-habit-compiler.md`); the decisions are summarized in `docs/design.md`.

## Layout

```text
crates/
  stretto-trace/   episode schema, τ²-bench and MCP proxy log ingest, tool manifests
  stretto-model/   abstraction, back-off world model, α posterior (fugue), provenance, runs, policy
  stretto-oracle/  Oracle trait, Jev client (feature "http"), replay cache, mock
  stretto-report/  `stretto` CLI: the Phase 0 report; Phase 0b (`shadow`: System-One questions at held-out decisions; `arbitrate`: combining them with the habit); read-only flows (`flow`: compile, learn, serve); policy guards (`guards`); the flow audit as a fugue program (`audit`)
  stretto-proxy/   stdio MCP proxy: records sessions, and in active mode (`active`) runs a flow, guards, `stretto_commit` and the conversation context; plus a demo MCP server (echo or a tiny retail world)
pilot/             live τ²-bench episodes: GLM in Claude Code, tools over MCP, the flows arm, and a replay check
site/              the working paper, deployed to GitHub Pages by .github/workflows/pages.yml
docs/
  design.md        decisions, architecture and implementation status
  results/         dated results with interpretation
```

## Conventions

- **One schema.** Everything downstream reads `stretto_trace::Episode`, never a benchmark's native format. Add new sources as ingest modules.
- **Posteriors, not frequencies.** Anything that drives a decision is a posterior predictive, with evidence counts next to it. Use closed forms where they exist, and fugue where they do not.
- **Replay by default.** Oracle calls go through `ReplayCache`, so experiments pay for each distinct question once and re-run for free. Never commit cache contents that contain third-party data.
- **Honest reports.** Every metric in a report gets a definition in the "Reading the numbers" section. A proxy must say it is a proxy.
- **Secrets.** Keys come from the environment (`TYPESAFE_API_KEY`, `ZAI_API_KEY`, and later MiniMax's). Never log or commit them. The Z.ai key is a coding-plan key: use it only through Claude Code (`pilot/glm-claude.sh`), and keep runs small.

## Before committing

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```
