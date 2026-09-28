# Agent context: stretto

stretto learns, from an LLM agent's recorded tool calls, which reads it makes next and where their arguments come from, and serves those reads through an MCP proxy in the same tool result, so the agent needs fewer LLM turns (tagline: *Read ahead of your agent.*). A flow only reads. Its lookups are decided by counting (the reach decider, with no model), by a learned habit, or by an arbiter that also asks a System-One model (TypeSafe's Jev). Where no user speaks, `stretto-procedure` runs a workflow compiled once from traces. The design is RFC-001 (`docs/rfc/001-habit-compiler.md`); the decisions are summarized in `docs/design.md`; the paper is `paper/stretto.md`. Brand words and numbers follow `brand/messaging.md`: every number comes from `docs/results/claims.md` or the paper, with its scope.

## Layout

```text
crates/
  stretto-trace/   episode schema, τ²-bench and MCP proxy log ingest, tool manifests
  stretto-model/   abstraction, back-off world model, α posterior (fugue), provenance, runs, policy
  stretto-oracle/  Oracle trait, Jev client (feature "http"), replay cache, mock
  stretto-report/  `stretto` CLI: first run (`init`, `doctor`, `completions`); the Phase 0 report; Phase 0b (`shadow`: System-One questions at held-out decisions; `arbitrate`: combining them with the habit); read-only flows (`flow`: compile, learn, serve); policy guards (`guards`); the flow audit as a fugue program (`audit`)
  stretto-proxy/   stdio MCP proxy: records sessions, and in active mode (`active`) runs a flow, guards, `stretto_commit` and the conversation context; plus `stretto-procedure`, the procedure runtime, and a demo MCP server (echo or a tiny retail world)
pilot/             live τ²-bench episodes: GLM in Claude Code, tools over MCP, the flows arm, and a replay check
pilot/bench/       live runs of other benchmarks (AgentDojo, BFCL) in their own environments
website/           the documentation site (VitePress), deployed to GitHub Pages by .github/workflows/pages.yml
site/              the research notebook of the rounds through 2026-09-25, served by the site at /notebook/
paper/             the working paper, paper/stretto.md
brand/             the brand kit: logo, tokens, messaging, the explainer and the videos
docs/
  design.md        decisions, architecture and implementation status
  results/         dated results with interpretation
  install.md       every way to install; releasing.md, how a release is cut
```

## Conventions

- **One schema.** Everything downstream reads `stretto_trace::Episode`, never a benchmark's native format. Add new sources as ingest modules.
- **Posteriors, not frequencies.** Anything that drives a decision is a posterior predictive, with evidence counts next to it. Use closed forms where they exist, and fugue where they do not.
- **Replay by default.** Oracle calls go through `ReplayCache`, so experiments pay for each distinct question once and re-run for free. Never commit cache contents that contain third-party data.
- **Honest reports.** Every metric in a report gets a definition in the "Reading the numbers" section. A proxy must say it is a proxy.
- **Secrets.** Keys come from the environment (`TYPESAFE_API_KEY`, `ZAI_API_KEY`, and later MiniMax's). Never log or commit them. The Z.ai key is a coding-plan key: use it only through Claude Code (`pilot/glm-claude.sh`), and keep runs small.

## Before committing

```bash
make fmt   # cargo fmt --all
make ci    # what CI's check job runs: the formatting check, clippy, the tests and doctests, and rustdoc
```

`make help` lists the other targets, among them `make bless` after a change to a command or an option (it regenerates `docs/cli.md`).
