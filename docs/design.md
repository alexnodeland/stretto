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
| First live form (2026-09-24) | Transparent continuation: after each of the agent's own calls, a goal-free read-only flow makes the lookups it is sure enough of and returns them in the same tool response. No new tools and no prompt change. Named macro-tools come next |

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
| D0 (pilot) | Raw tools; each response may carry a flow's extra lookups | Habit and Jev by arbitration, lookup first; the LLM for everything else |

## Live flows (2026-09-24)

The first live arm is D0: the smallest change to what the agent sees.

1. The agent calls a tool.
2. Its result goes back to the flow. The flow asks the v2 questions, arbitrates, and makes the likeliest lookup if its probability is at least 0.3.
3. The flow repeats step 2 until it hands back.
4. The agent gets its own result and the flow's lookups in the same response.

Nobody names the goal. The offline run without it (`--no-intent`) agrees and saves as much as the run with it.

The flow binds arguments from earlier outputs. Offline, an argument counted as bindable if its value appeared earlier. Live, the flow must pick one: for each lookup argument, it learns which tool and path the values came from in training (`get_user_details` at `$.orders[*]`, for example). It then takes the first value there that it has not already looked up, preferring one the customer mentioned. An argument only the customer or the LLM can supply hands back.

`stretto flow-serve` compiles the flow from the replay cache and answers over a local port. The agent's process never holds the Jev key.

## Pieces

```text
τ²-bench logs ──────────────────┐
stretto-proxy (records MCP) ────┼─► stretto-trace (Episode) ─► stretto-model (abstraction, world model,
                                │                              provenance, projection)
                                │                                      │
                                │         stretto-report (Phase 0, 0b) ◄── stretto-oracle (Jev, cache)
                                │
                                ├─ stretto flow-serve (live read-only flows, D0) ◄── the pilot's MCP server
                                │
                                └─ later: stretto-compile (flow IR → fugue Model) ─► stretto-proxy
                                          serving macro-tools, with the arbitration runtime
```

Today `stretto-proxy` only records: it forwards every line unchanged and writes a session log. In the pilot, the flow runs next to the tool server rather than in the proxy. Serving compiled flows as `plan_*` / `resume_*` / `commit_*` macro-tools, and checking raw writes against compiled rules, are later phases in the proxy.
