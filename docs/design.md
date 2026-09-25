# Design summary

The full rationale, prior work and risks are in [RFC-001](rfc/001-habit-compiler.md), which began in fugue's decision log and moved here on 2026-09-25. This page records what we decided and how the pieces fit.

## Decisions (2026-09-23)

| Question | Decision |
|---|---|
| What v1 is for | Compile and run flows, measured first |
| Workload | τ²-bench: airline and retail together; telecom later ([#10](https://github.com/alexnodeland/stretto/issues/10)) |
| Where the harness sits | A Rust MCP proxy. Compiled flows are served to the agent as macro-tools |
| Pausing a flow | Resumable: `plan_*`, `resume_*(token, choice)`, `commit_*(token)` |
| Writes | Plan/commit pairs. The agent gets the user's explicit "yes" between them, as τ²-bench requires |
| Between LLM turns | Flows only read. Writes, and tools not marked read-only, go back to the LLM, so a wrong pick is a detour (an extra lookup), not a risk (Phase 0b v2) |
| Taking Jev's answer | Combined with the habit's prediction and Jev's record at that site on other tasks: a conditional logit, fitted by cross-validation over tasks (RFC-001 §3.6) |
| Jev's roles inside a flow | Match descriptions to records; classify stated reasons; judge tool outputs; pick the next sub-flow. Matching was tested on 2026-09-24. From the customer's words alone, Jev matched records less well than the agents did, so it is not used as a check on writes ([matching](results/matching-2026-09-24.md)) |
| Dates, amounts, eligibility | Code. Rule checks are compiled from the policy by an LLM, tested against traces, and reviewed by a person |
| Flow discovery | Traces first. The policy only names flows and checks the rules |
| Checking raw writes | A separate experimental arm |
| Models | Transfer first: flows compiled from published frontier-model trajectories, run by GLM (Z.ai) and MiniMax. American frontier models later |
| Phase 0 data | τ²-bench's published trajectories |
| Win conditions | Fewer LLM calls, tokens and dollars; higher pass^k; Jev agreeing with the frontier model at branch points, and well calibrated; fewer policy violations |
| First live form (2026-09-24) | Transparent continuation: after each of the agent's own calls, a goal-free read-only flow makes the lookups it is sure enough of and returns them in the same tool response. No new tools and no prompt change. |
| Named macro-tools (2026-09-24) | Not built. Naming could add at most 0–1.9% of LLM turns in retail and 0.9–7.9% in airline over D0: only lookups that need a value from the conversation ([arms](results/arms-2026-09-24.md)) |
| Deciding without Jev (2026-09-24) | Allowed: `--flow-decider habit`, once there are enough traces. Replayed, the habit alone saves as many turns as the arbiter, and live it did too; the arbiter makes fewer detours in airline ([arms](results/arms-2026-09-24.md), [pilot](results/pilot-habit-2026-09-24.md)). With a deployment's own first sessions, start on the habit alone: from five random sessions it saved more than an arbiter fitted on those sessions, which pays from about twenty ([cold start](results/cold-start-2026-09-24.md)). [The sweep](results/sweep-2026-09-24.md)'s finding that Jev's answers carry the savings with few traces held only for its clustered samples, with an arbiter fitted on thousands of other agents' decisions |

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
| Flow IR: compile from τ²-bench, learn from sessions, serve | Built; fields documented in [formats.md](formats.md) | `stretto compile`, `learn`, `serve`; `flow.rs` |
| Read-only flows live (arm D0) | Built. Paired pilots ran in retail and airline, then every test task: 80 pairs, 25.5% fewer LLM turns (95% interval 20.5% to 30.4%), and a pass-rate change of −1.25 points (−7.5 to +6.25) ([results](results/paired-2026-09-25.md)) | `stretto-proxy --flow`; the pilot harness (`pilot/`, `run_paired.py`) |
| Policy guards (arms B and E) | Built, audited against τ²-bench; arm B live in airline (GLM-5.3 gave them nothing to refuse) | `guards.rs`; `stretto guards`; `stretto-proxy --guards`; `pilot/run_episode.py --arm guards` |
| Confirmed writes in one call | Built; no live run has used it yet ([#7](https://github.com/alexnodeland/stretto/issues/7)) | `stretto-proxy --commit` (`stretto_commit`) |
| The conversation for flows and guards | Built | `stretto-proxy --context` |
| Record, learn, serve from any MCP server | Built, tested end to end | `crates/stretto-proxy/tests/active.rs` |
| A flow as a fugue program: score and simulate | Built | `stretto audit`; `audit.rs` |
| A flow's live run as a fugue program | Built. The flow IR holds the run as a program in fugue's format (fugue#65), checked when the flow loads, with a decision site before each lookup and an outcome site after it. The proxy runs it with `run_async` (fugue#62). The sites carry their site as metadata (fugue#63), and their distributions are the flow's statistics (fugue#66). It draws exactly what the Rust it replaced drew, bit for bit. The proxy logs each run's trace, which `ScoreGivenTrace` scores again | `program.rs`; `active.rs`; `stretto flow-show` |
| Shadow mode and per-site promotion | Built, replayed: promoted on half the test tasks and replayed on the other half, D0 kept 277 of 294 retail turns saved and all 41 in airline, with detours down from 58 to 28 and from 26 to 11 ([results](results/promotion-2026-09-25.md)). No live shadow traffic yet | `stretto-proxy --flow-shadow`; `stretto promote`; `promote.rs` |
| Reviewing flows as code | Built: a flow rendered for review, and a change list between two that exits with 1 when a change needs review ([reviewing flows](review.md)) | `stretto flow-show`, `stretto flow-diff`; `review.rs` |
| Privacy for recorded sessions | Built: what each file holds and what is sent ([privacy](privacy.md)); pseudonymized copies that teach the same flow; retention | `stretto redact`, `redact.rs`; `stretto-proxy --retain-days` |
| `plan_*` / `resume_*` macro-tools the LLM names (arms C and D) | Not built, by decision: naming could add at most 0–1.9% of turns in retail and 0.9–7.9% in airline | Phase 0's *Lookups inside runs* |
| Arm C and the habit alone, as flows behind the tools | Built; replayed against D0 on GLM-5's test episodes ([arms](results/arms-2026-09-24.md)); the habit alone live in retail ([pilot](results/pilot-habit-2026-09-24.md)) | `--flow-decider habit` in `stretto-proxy`, `serve`, `flow-serve` and `pilot/check_flow.py`; `pilot/run_episode.py --arm habit` |
| Fewer traces | Built, and replayed: the habit trains on a fixed, nested share of the training tasks. With 3 retail tasks, the arbiter saves 20.4% of turns and the habit alone 1.8% ([sweep](results/sweep-2026-09-24.md)). Those samples were clustered by task id, so the order is now mixed before sampling (see the sweep's correction) | `--train-fraction` on `phase0`, `compile` and `learn` |
| Judging a confirmation with Jev | Built, measured offline against the word list and hand labels ([results](results/confirm-2026-09-24.md)); in the proxy's guards ([#14](https://github.com/alexnodeland/stretto/issues/14)). Enforced live on 20 episodes it refused nothing, and logged on 20 more it would have stopped 2 real lapses for 2 false alarms, so the recommended setting enforces it ([results](results/judge-live-2026-09-25.md)) | `stretto confirm`; `confirm.rs`; `stretto-proxy --confirm-judge enforce` |
| Prompt injection against flows and the judge | Measured offline: a flow stays inside its compiled lookups whatever the System-One model answers (tested), but injected text steers its choice among them; an instruction in the questions and a higher bar absorb most of it ([results](results/injection-2026-09-25.md)). The instruction is not yet the default | `scripts/injection.py`; `stretto ask` |
| A second confirmation question | Built, measured: "had the agent proposed this change?" flags lapses the first question passes, such as a call that differs from what the customer agreed to. Offline half its flags are real lapses ([results](results/confirm-second-2026-09-24.md)), and live 1 of 8 was, so it stays logged ([results](results/judge-live-2026-09-25.md)) | `stretto confirm --second-question`; `stretto-proxy --confirm-second proposed --confirm-second-shadow` |
| Matching descriptions to records | Built, measured: Jev picked the expected record less often than the agents did (82% against 90%), so it is not used as a check ([results](results/matching-2026-09-24.md)) | `stretto match`; `matching.rs` |
| A flow from a deployment's first sessions | Built, measured offline on GLM-5's own sessions (random draws of 5 to 40 tasks) and live on five sessions GLM-5.3 recorded through the proxy ([cold start](results/cold-start-2026-09-24.md)). The habit alone is the steadier start; `--refit-habit` did not help reliably; another flow's arbiter (`--arbiter-from`) lifted a narrow draw | `stretto learn --results`, `--habit-only`, `--refit-habit`, `--arbiter-from`; `pilot/run_episode.py --record-context` |
| A shipped arbiter | Built, measured across domains: `data/arbiters/` holds retail and airline arbiters fitted on four agents' published decisions. Served with a habit learned in the other domain, each did what that domain's own arbiter did, within about a point ([results](results/arbiter-transfer-2026-09-24.md)). In telecom, a domain unlike both, they and an arbiter fitted on both handed back too often, and the habit alone did better ([results](results/telecom-2026-09-25.md)) | `compile --pooled-arbiter`, `stretto export-arbiter`, `learn --arbiter-from` |
| Options from the tool manifest | Built, measured on the sweep's one- and three-task samples: nothing gained in retail, and detours in airline, where a lookup no trace showed is bound by argument name and gets the wrong values ([results](results/manifest-options-2026-09-24.md)); off by default | `--manifest-options` on `phase0`, `compile` and `learn` |
| Counterfactual evaluation from logged propensities | Built, validated on replays ([#15](https://github.com/alexnodeland/stretto/issues/15)): exploration at read-only sites logs every option's propensity, and four estimators score another rule. They rank changes that keep a flow's chains, such as another threshold. A changed decider's detours, and turns saved, need a replay ([results](results/evaluate-2026-09-25.md)) | `stretto-proxy --flow-explore`; `stretto evaluate`; `evaluate.rs` |
| Predicate refinement (§3.4) | Built ([#16](https://github.com/alexnodeland/stretto/issues/16)). One round in airline kept two of eight candidates, which fitted the agents the proposer read but not GLM-5, so the three hand-proposed predicates stay ([results](results/refine-2026-09-25.md)) | `stretto refine`; `phase0 --candidates`; `refine.rs`; `data/predicates-v2.json` |
| Streamable HTTP servers | Built: the proxy speaks MCP's HTTP transport to the server and stdio to the host; tested against the reference TypeScript server and a strict test server | `stretto-proxy --upstream`; `http.rs` |

Everything else not built yet, and the experiments still to run, are grouped in the [roadmap](roadmap.md).

Flows only read, so a flow's wrong pick costs a lookup, not an action. Writes stay with the LLM: one at a time, or several confirmed ones in one `stretto_commit`. The guards check both before the server sees them.
