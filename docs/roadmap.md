# Roadmap

Every piece of the first design is built: the flow compiler, the MCP proxy, the policy guards, the audit, the confirmation judge and the shipped arbiters ([implementation status](design.md#implementation-status-2026-09-24)). The evidence so far:

- offline replays on τ²-bench's published trajectories;
- live pilots of ten paired tasks per domain, with GLM-5.3 in Claude Code as the agent;
- a paired run on every retail and airline test task: 80 pairs, with 25.5% fewer LLM turns and the pass rate within −7.5 to +6.25 points ([results](results/paired-2026-09-25.md)).

What is left is tracked in GitHub issues, all of them sub-issues of [#1](https://github.com/alexnodeland/stretto/issues/1). This page groups them and says why each matters. The changes RFC-001 §3.9 asks of fugue itself are tracked in fugue's [#61](https://github.com/alexnodeland/fugue/issues/61).

## Evidence from live runs

Each of these spends the Z.ai coding-plan key, so each needs an approved credit budget before it starts. A pilot episode has cost about 12 credits in retail and 17 in airline.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#2](https://github.com/alexnodeland/stretto/issues/2) | A paired run on every test task | Done: 80 pairs. The flow saved 25.5% of LLM turns (95% interval 20.5% to 30.4%), and the pass rate moved −1.25 points (−7.5 to +6.25) ([results](results/paired-2026-09-25.md)). A one-point bound would take about 4,300 pairs | 1,897 credits |
| [#3](https://github.com/alexnodeland/stretto/issues/3) | The cold start live: the habit alone against the habit with a shipped arbiter | Done in retail. On the pilot's ten tasks, the habit from five sessions took 79 LLM turns against 110 with no flow, making exactly the lookups the four-agent habit made. With the shipped airline arbiter it took 70, with fewer detours ([results](results/cold-start-live-2026-09-25.md)). Airline is left | 205 credits for retail; airline would add about 420 |
| [#4](https://github.com/alexnodeland/stretto/issues/4) | A simulated customer that is not the agent's own model | In every pilot, GLM-5.3 played both parts | A key for the customer's model |
| [#5](https://github.com/alexnodeland/stretto/issues/5) | More agent models live | Every live result is one model; offline, savings follow calling style | A key and a budget per model |
| [#6](https://github.com/alexnodeland/stretto/issues/6) | The confirmation judge enforced | Enforced, it would refuse 5–11% of the writes accepted in successful episodes; whether that costs passes or turns is untested. The proxy can enforce it since #14 | About 240 credits for retail, 340 for airline |
| [#7](https://github.com/alexnodeland/stretto/issues/7) | `stretto_commit` live | Done offline: it could save at most 0.2–3.5% of LLM turns, too little for a ten-task pilot to see, so no live run for now ([results](results/commit-bound-2026-09-25.md)) | Free |
| [#8](https://github.com/alexnodeland/stretto/issues/8) | Flows and guards from frontier traces, serving a smaller agent | Guards bit in 38% of failed airline episodes on published runs, and never with GLM-5.3 | Priced by a smoke episode first |

## Evidence from offline runs

These cost Jev dollars, CPU time or people's time, and no LLM runs.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#9](https://github.com/alexnodeland/stretto/issues/9) | The cold start on other agents, with more draws | The finding rests on one agent and five draws | About $10–15; several hundred replays, so #28 first |
| [#10](https://github.com/alexnodeland/stretto/issues/10) | Telecom, and an arbiter fitted on retail and airline together | Done: telecom compresses like the others, but neither shipped arbiter nor one fitted on both carries to it; the habit alone does better there ([results](results/telecom-2026-09-25.md)) | $0.48 |
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
| [#19](https://github.com/alexnodeland/stretto/issues/19) | Shadow mode and per-site promotion | Done: `stretto-proxy --flow-shadow` and `stretto promote`; replayed, promotion halved D0's detours for 6% of its retail savings ([results](results/promotion-2026-09-25.md)) |
| [#20](https://github.com/alexnodeland/stretto/issues/20) | `stretto flow show` and `flow diff` | Done: `stretto flow-show` and `stretto flow-diff` ([reviewing flows](review.md)) |
| [#21](https://github.com/alexnodeland/stretto/issues/21) | Streamable HTTP for the proxy | Done: `stretto-proxy --upstream URL` |
| [#22](https://github.com/alexnodeland/stretto/issues/22) | Learn from several servers' logs of one session | One proxy wraps one server |
| [#23](https://github.com/alexnodeland/stretto/issues/23) | Learn from OpenTelemetry GenAI spans | RFC-001 §3.9's `stretto-trace` |
| [#24](https://github.com/alexnodeland/stretto/issues/24) | Privacy for recorded sessions | Done: [an inventory](privacy.md), `stretto redact` and `stretto-proxy --retain-days`. Leaving fields out of the questions' state is still open |
| [#25](https://github.com/alexnodeland/stretto/issues/25) | Flow search with fugue-evo | RFC-001's Phase 3 |

## Fixes and tooling

| Issue | What |
|---|---|
| [#26](https://github.com/alexnodeland/stretto/issues/26) | Done: `learn` and `phase0` refuse options they would ignore, such as `--manifest-options` with `--habit-only` or `--arbiter-from` |
| [#27](https://github.com/alexnodeland/stretto/issues/27) | Done: `run_episode.py --read-only-hints` marks τ²-bench's read-only tools in `tools/list`, so pilot recordings learn without `--manifest` |
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

All six are built. A live flow now runs as a fugue program: the flow IR holds its run in fugue's program format, and the proxy runs it with `run_async`. stretto depends on fugue by git revision until fugue's next release.

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
4. **Live runs, as budgets are approved.** The paired run (#2) and the cold start live in retail (#3) are done. The paired run puts the pass-rate change between −7.5 and +6.25 points, not within one. Next comes the confirmation judge enforced (#6).
5. **Phase 3 features** as the evidence calls for them:
   - counterfactual evaluation (#15);
   - predicate refinement (#16);
   - flow search (#25).
