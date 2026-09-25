# Roadmap

Every piece of the first design is built: the flow compiler, the MCP proxy, the policy guards, the audit, the confirmation judge and the shipped arbiters ([implementation status](design.md#implementation-status-2026-09-24)). The evidence so far:

- offline replays on τ²-bench's published trajectories;
- live pilots of ten paired tasks per domain, with GLM-5.3 in Claude Code as the agent.

What is left is tracked in GitHub issues, all of them sub-issues of [#1](https://github.com/alexnodeland/stretto/issues/1). This page groups them and says why each matters. The changes RFC-001 §3.9 asks of fugue itself are tracked in fugue's [#61](https://github.com/alexnodeland/fugue/issues/61).

## Evidence from live runs

Each of these spends the Z.ai coding-plan key, so each needs an approved credit budget before it starts. A pilot episode has cost about 12 credits in retail and 17 in airline.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#2](https://github.com/alexnodeland/stretto/issues/2) | A paired run on every test task | "The same pass rate" rests on ten pairs per domain, too few to rule out a one-point loss | About 950 credits for retail, 1,350 for airline |
| [#3](https://github.com/alexnodeland/stretto/issues/3) | The cold start live: the habit alone against the habit with a shipped arbiter | Offline, both beat an arbiter fitted on the first sessions; live, only that arbiter flow ran, on three tasks | About 240 credits for retail; airline adds 420 |
| [#4](https://github.com/alexnodeland/stretto/issues/4) | A simulated customer that is not the agent's own model | In every pilot, GLM-5.3 played both parts | A key for the customer's model |
| [#5](https://github.com/alexnodeland/stretto/issues/5) | More agent models live | Every live result is one model; offline, savings follow calling style | A key and a budget per model |
| [#6](https://github.com/alexnodeland/stretto/issues/6) | The confirmation judge enforced | Enforced, it would refuse 5–11% of the writes accepted in successful episodes; whether that costs passes or turns is untested. The proxy can enforce it since #14 | About 240 credits for retail, 340 for airline |
| [#7](https://github.com/alexnodeland/stretto/issues/7) | `stretto_commit` live | Built, never used live, and its possible saving is unmeasured. The offline bound comes first | The bound is free; a pilot is 240–340 credits |
| [#8](https://github.com/alexnodeland/stretto/issues/8) | Flows and guards from frontier traces, serving a smaller agent | Guards bit in 38% of failed airline episodes on published runs, and never with GLM-5.3 | Priced by a smoke episode first |

## Evidence from offline runs

These cost Jev dollars, CPU time or people's time, and no LLM runs.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#9](https://github.com/alexnodeland/stretto/issues/9) | The cold start on other agents, with more draws | The finding rests on one agent and five draws | About $10–15; several hundred replays, so #28 first |
| [#10](https://github.com/alexnodeland/stretto/issues/10) | Telecom, and an arbiter fitted on retail and airline together | A third domain, and the next test of a shipped arbiter; telecom's customer acts on their own phone | A few dollars |
| [#11](https://github.com/alexnodeland/stretto/issues/11) | Independent labels for the confirmation judge | Its labels come from one annotator, the model that ran the analysis | Annotators' time |
| [#12](https://github.com/alexnodeland/stretto/issues/12) | Score the pilots as τ²-bench does | The pilots count the database check only; retail's natural-language assertions and airline's communication check are left out | Airline is free; retail needs a judge's key. Needs #29 |
| [#13](https://github.com/alexnodeland/stretto/issues/13) | Prompt injection against flows and the confirmation judge | Done: a flow stays inside its compiled lookups, but injected text steers the choice among them; one sentence in the questions and a higher bar absorb most of it ([results](results/injection-2026-09-25.md)) | $0.22 |

## Features from the design that are not built

| Issue | What | Where the design asks for it |
|---|---|---|
| [#14](https://github.com/alexnodeland/stretto/issues/14) | The confirmation judge in the proxy's guards, logged or enforced | Done: `stretto-proxy --confirm-judge log\|enforce` |
| [#15](https://github.com/alexnodeland/stretto/issues/15) | Counterfactual evaluation from logged decisions | RFC-001 §3.7. Flows act deterministically, so it needs a little exploration first |
| [#16](https://github.com/alexnodeland/stretto/issues/16) | Predicate refinement | RFC-001 §3.4; the three predicates were written by hand |
| [#17](https://github.com/alexnodeland/stretto/issues/17) | Hand back when a session surprises the flow | RFC-001 §3.6; surprise is measured only after the fact |
| [#18](https://github.com/alexnodeland/stretto/issues/18) | Drift alarms | RFC-001 §3.3 and §4: change-point alarms and forgetting |
| [#19](https://github.com/alexnodeland/stretto/issues/19) | Shadow mode and per-site promotion | RFC-001 §3.7; today one threshold serves every site |
| [#20](https://github.com/alexnodeland/stretto/issues/20) | `stretto flow show` and `flow diff` | RFC-001 question 4: review flows as code |
| [#21](https://github.com/alexnodeland/stretto/issues/21) | Streamable HTTP for the proxy | The proxy speaks stdio only |
| [#22](https://github.com/alexnodeland/stretto/issues/22) | Learn from several servers' logs of one session | One proxy wraps one server |
| [#23](https://github.com/alexnodeland/stretto/issues/23) | Learn from OpenTelemetry GenAI spans | RFC-001 §3.9's `stretto-trace` |
| [#24](https://github.com/alexnodeland/stretto/issues/24) | Privacy for recorded sessions | RFC-001 question 5 |
| [#25](https://github.com/alexnodeland/stretto/issues/25) | Flow search with fugue-evo | RFC-001's Phase 3 |

## Fixes and tooling

| Issue | What |
|---|---|
| [#26](https://github.com/alexnodeland/stretto/issues/26) | Done: `learn` and `phase0` refuse options they would ignore, such as `--manifest-options` with `--habit-only` or `--arbiter-from` |
| [#27](https://github.com/alexnodeland/stretto/issues/27) | Mark τ²-bench's read-only tools in `pilot/tau2_mcp.py`, so pilot recordings learn without `--manifest` |
| [#28](https://github.com/alexnodeland/stretto/issues/28) | Done: `check_flow.py --in-process --jobs N` makes the tools' calls in one process and replays in parallel; `replay_study.py` runs a resumable list of replays |
| [#29](https://github.com/alexnodeland/stretto/issues/29) | Done: the pilots' recorded episodes are [published](results/episodes-2026-09-24.md) |
| [#30](https://github.com/alexnodeland/stretto/issues/30) | A first release, 0.1.0 |

## Documentation

| Issue | What |
|---|---|
| [#31](https://github.com/alexnodeland/stretto/issues/31) | The flow IR and arbiter file formats |
| [#32](https://github.com/alexnodeland/stretto/issues/32) | A CLI reference, generated from the code |
| [#33](https://github.com/alexnodeland/stretto/issues/33) | A walkthrough with your own MCP server |

## In fugue

RFC-001 §3.9 asks six additive changes of `fugue-ppl`. They are tracked in [fugue#61](https://github.com/alexnodeland/fugue/issues/61):

- async interpretation ([#62](https://github.com/alexnodeland/fugue/issues/62));
- site metadata for handlers ([#63](https://github.com/alexnodeland/fugue/issues/63));
- a delegating handler adapter ([#64](https://github.com/alexnodeland/fugue/issues/64));
- a serializable program format ([#65](https://github.com/alexnodeland/fugue/issues/65));
- vector-valued sites ([#66](https://github.com/alexnodeland/fugue/issues/66));
- a how-to for sample-or-observe sites ([#67](https://github.com/alexnodeland/fugue/issues/67)).

Async interpretation, site metadata and the program format would let a live flow run as a fugue program. Today stretto uses fugue only offline: for the audit and for the habit's concentration.

## A suggested order

1. **What cannot wait.** Publish the pilots' episodes (#29) while the recordings exist.
2. **What costs nothing and unblocks the rest:**
   - the bug (#26);
   - the pilot's read-only hints (#27);
   - faster replays (#28);
   - the judge in the proxy (#14).
3. **Offline evidence**, cheapest first:
   - airline's communication check (#12);
   - prompt injection (#13);
   - telecom (#10);
   - the cold start's other agents and draws (#9);
   - independent labels (#11).
4. **Live runs, as budgets are approved.** The paired run (#2) comes first, since every other live claim leans on pass^1. Next come the confirmation judge enforced (#6) and the cold start live (#3).
5. **Phase 3 features** as the evidence calls for them:
   - counterfactual evaluation (#15);
   - predicate refinement (#16);
   - flow search (#25).
