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
| First live form (2026-09-24) | Transparent continuation: after each of the agent's own calls, a goal-free read-only flow makes the lookups it is sure enough of and returns them in the same tool response. No new tools and no prompt change. |
| Named macro-tools (2026-09-24) | Not built. Naming could add at most 0–1.9% of LLM turns in retail and 0.9–7.9% in airline over D0: only lookups that need a value from the conversation ([arms](results/arms-2026-09-24.md)) |
| Deciding without Jev (2026-09-24) | Allowed: `--flow-decider habit`. Replayed, the habit alone saves as many turns as the arbiter; the arbiter makes fewer detours in airline ([arms](results/arms-2026-09-24.md)) |

## The principle for macro-tools

**The LLM names it; Jev finds it.**

- When the agent calls a macro-tool it has already read the conversation. So intent, descriptions and the user's stated reasons cost nothing to pass as arguments.
- The flow then fetches data the LLM has not seen, and decides mid-flow using:
  - code, for rules;
  - the habit, when it is confident;
  - Jev, for judgments about content.
- A decision nothing inside the flow can settle pauses the flow and hands a token back to the LLM.

Measured before building (2026-09-24): a flow behind the tools already binds every lookup argument that came from an earlier output. A name would add only the lookups that need a value from the conversation: at most 0–1.9% of LLM turns in retail and 0.9–7.9% in airline. So named macro-tools are not built ([arms](results/arms-2026-09-24.md)).

## Experimental arms (τ²-bench, held-out tasks, k trials each)

| Arm | Agent sees | Branches resolved by |
|---|---|---|
| A | Raw tools | The LLM (baseline) |
| B | Raw tools, with rule checks on writes | The LLM |
| C | Raw tools + macro-tools | The LLM, via a pause at every branch. Replayed as a flow behind the tools: 0–1.7% of turns saved |
| D | Raw tools + macro-tools | Habit, then Jev, then LLM, by arbitration |
| E | As D, plus rule checks on raw writes | As D |
| D0 (pilot) | Raw tools; each response may carry a flow's extra lookups | Habit and Jev by arbitration, lookup first; the LLM for everything else |
| D0, habit alone | As D0 | The habit alone, lookup first; never asks Jev |

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
τ²-bench results ───────────┐
stretto-proxy session logs ─┼─► stretto-trace (Episode) ─► stretto-model (abstraction, habit, α posterior
                            │                              with fugue, provenance, projection)
                            │                                      │
                            │   stretto-report ◄───────────────────┤◄── stretto-oracle (Jev, replay cache)
                            │     phase0 (measure), compile / learn ─► flow IR (JSON)
                            │     guards (policy checks, audited), audit (a flow as a fugue program)
                            │                                      │
                            └── stretto-proxy, active mode ◄───────┘
                                  flow behind the agent's calls (D0), guards on its calls (B, E),
                                  stretto_commit, conversation context, session logs (v2)
```

## Implementation status (2026-09-24)

| Piece | State | Where |
|---|---|---|
| Measure on published trajectories (Phase 0) | Built | `stretto phase0` |
| System-One questions and the arbiter (Phase 0b, v2) | Built | `stretto phase0 --oracle … --questions v2`; `shadow.rs`, `arbitrate.rs` |
| Flow IR: compile from τ²-bench, learn from sessions, serve | Built | `stretto compile`, `learn`, `serve`; `flow.rs` |
| Read-only flows live (arm D0) | Built; paired pilots in retail and airline | `stretto-proxy --flow`; the pilot harness (`pilot/`) |
| Policy guards (arms B and E) | Built, audited against τ²-bench; arm B live in airline (GLM-5.3 gave them nothing to refuse) | `guards.rs`; `stretto guards`; `stretto-proxy --guards`; `pilot/run_episode.py --arm guards` |
| Confirmed writes in one call | Built | `stretto-proxy --commit` (`stretto_commit`) |
| The conversation for flows and guards | Built | `stretto-proxy --context` |
| Record, learn, serve from any MCP server | Built, tested end to end | `crates/stretto-proxy/tests/active.rs` |
| A flow as a fugue program: score and simulate | Built | `stretto audit`; `audit.rs` |
| `plan_*` / `resume_*` macro-tools the LLM names (arms C and D) | Not built, by decision: naming could add at most 0–1.9% of turns in retail and 0.9–7.9% in airline | Phase 0's *Lookups inside runs* |
| Arm C and the habit alone, as flows behind the tools | Built; replayed against D0 on GLM-5's test episodes ([arms](results/arms-2026-09-24.md)) | `--flow-decider habit` in `stretto-proxy`, `serve`, `flow-serve` and `pilot/check_flow.py` |
| Counterfactual evaluation from logged propensities | Not built; the proxy logs every decision's probabilities | — |
| Predicate refinement (§3.4) | Not built; three hand-proposed predicates | `data/predicates-v2.json` |
| Streamable HTTP transport | Not supported; stdio only | — |

Flows only read, so a flow's wrong pick costs a lookup, not an action. Writes stay with the LLM: one at a time, or several confirmed ones in one `stretto_commit`. The guards check both before the server sees them.
