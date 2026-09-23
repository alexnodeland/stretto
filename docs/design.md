# Design summary

The full rationale, prior work and risks are in fugue's [RFC-001](https://github.com/alexnodeland/fugue/blob/main/docs/decisions/rfc/001-habit-compiler.md). This page records what we decided and how the pieces fit.

## Decisions (2026-09-23)

| Question | Decision |
|---|---|
| What v1 is for | Compile and run flows, measured first |
| Workload | τ²-bench: airline and retail together; telecom later |
| Where the harness sits | A Rust MCP proxy. Compiled flows are served to the agent as macro-tools |
| Pausing a flow | Resumable: `plan_*`, `resume_*(token, choice)`, `commit_*(token)` |
| Writes | Plan/commit pairs. The agent gets the user's explicit "yes" between them, as τ²-bench requires |
| Between LLM turns | Flows only read. Writes, and tools not marked read-only, go back to the LLM, so a wrong pick is a detour (an extra lookup), not a risk (Phase 0b v2) |
| Taking Jev's answer | Combined with the habit's prediction and Jev's record at that site on other tasks: a conditional logit, fitted by cross-validation over tasks (RFC-001 §3.6) |
| Jev's roles inside a flow | Match descriptions to records; classify stated reasons; judge tool outputs; pick the next sub-flow |
| Dates, amounts, eligibility | Code. Rule checks are compiled from the policy by an LLM, tested against traces, and reviewed by a person |
| Flow discovery | Traces first. The policy only names flows and checks the rules |
| Checking raw writes | A separate experimental arm |
| Models | Transfer first: flows compiled from published frontier-model trajectories, run by GLM (Z.ai) and MiniMax. American frontier models later |
| Phase 0 data | τ²-bench's published trajectories |
| Win conditions | Fewer LLM calls, tokens and dollars; higher pass^k; Jev agreeing with the frontier model at branch points, and well calibrated; fewer policy violations |

## The principle for macro-tools

**The LLM names it; Jev finds it.**

- When the agent calls a macro-tool it has already read the conversation. So intent, descriptions and the user's stated reasons cost nothing to pass as arguments.
- The flow then fetches data the LLM has not seen, and decides mid-flow using:
  - code, for rules;
  - the habit, when it is confident;
  - Jev, for judgments about content.
- A decision nothing inside the flow can settle pauses the flow and hands a token back to the LLM.

## Experimental arms (τ²-bench, held-out tasks, k trials each)

| Arm | Agent sees | Branches resolved by |
|---|---|---|
| A | Raw tools | The LLM (baseline) |
| B | Raw tools, with rule checks on writes | The LLM |
| C | Raw tools + macro-tools | The LLM, via a pause at every branch |
| D | Raw tools + macro-tools | Habit, then Jev, then LLM, by arbitration |
| E | As D, plus rule checks on raw writes | As D |

## Pieces

```text
τ²-bench logs ──────────────────┐
stretto-proxy (records MCP) ────┼─► stretto-trace (Episode) ─► stretto-model (abstraction, world model,
                                │                              provenance, projection)
                                │                                      │
                                │         stretto-report (Phase 0, 0b) ◄── stretto-oracle (Jev, cache)
                                │
                                └─ later: stretto-compile (flow IR → fugue Model) ─► stretto-proxy
                                          serving macro-tools, with the arbitration runtime
```

Today `stretto-proxy` only records: it forwards every line unchanged and writes a session log. Serving compiled flows as `plan_*` / `resume_*` / `commit_*` macro-tools, and checking raw writes against compiled rules, are later phases in the same process.
