# Roadmap

Every piece of the first design is built: the flow compiler, the MCP proxy, the policy guards, the audit, the confirmation judge and the shipped arbiters ([implementation status](design.md#implementation-status-2026-09-26)). The evidence so far:

- offline replays on τ²-bench's published trajectories;
- live pilots of ten paired tasks per domain, with GLM-5.3 in Claude Code as the agent;
- live runs with Claude models as the agent and as the customer, on ten retail tasks and fewer ([results](results/claude-models-2026-09-25.md));
- a paired run on every retail and airline test task: 80 pairs, with 25.5% fewer LLM turns and the pass rate within −7.5 to +6.25 points ([results](results/paired-2026-09-25.md)).

What is left is tracked in GitHub issues, all of them sub-issues of [#1](https://github.com/alexnodeland/stretto/issues/1). This page groups them and says why each matters. The changes RFC-001 §3.9 asks of fugue itself are tracked in fugue's [#61](https://github.com/alexnodeland/fugue/issues/61).

## Evidence from live runs

Each of these spends the Z.ai coding-plan key, or Claude tokens where a Claude model plays a part, so each needs an approved budget before it starts. A pilot episode has cost about 12 credits in retail and 17 in airline.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#2](https://github.com/alexnodeland/stretto/issues/2) | A paired run on every test task | Done: 80 pairs. The flow saved 25.5% of LLM turns (95% interval 20.5% to 30.4%), and the pass rate moved −1.25 points (−7.5 to +6.25) ([results](results/paired-2026-09-25.md)). A one-point bound would take about 4,300 pairs | 1,897 credits |
| [#3](https://github.com/alexnodeland/stretto/issues/3) | The cold start live: the habit alone against the habit with a shipped arbiter | Done in both domains. In retail, the habit from five sessions took 79 LLM turns against 110 with no flow, and with the shipped airline arbiter 70, with fewer detours ([results](results/cold-start-live-2026-09-25.md)). In airline, both took 102 against 127, and the shipped retail arbiter cut the habit's detours from 8 to 1 ([results](results/cold-start-live-airline-2026-09-25.md)) | 205 credits for retail, 332 for airline |
| [#4](https://github.com/alexnodeland/stretto/issues/4) | A simulated customer that is not the agent's own model | Done: with Claude Sonnet 5 as the customer to GLM-5.3, on five retail tasks, the flow saved 23.1% of turns, against 25.8% with GLM-5.3 as the customer, and made the same lookups on four ([results](results/claude-models-2026-09-25.md)) | 110,016 Claude tokens and 112 credits |
| [#5](https://github.com/alexnodeland/stretto/issues/5) | More agent models live | Done for two Claude models. On the retail pilot's ten tasks, D0 saved Claude Haiku 4.5 18.6% of turns, less than GLM-5.3's 23.6%, as its more parallel calling predicts; on three tasks it saved Claude Sonnet 5 24.1% ([results](results/claude-models-2026-09-25.md)). Qwen3.5, the strongest candidate offline, waits on a key | 2.15M Claude tokens and 66 credits |
| [#6](https://github.com/alexnodeland/stretto/issues/6) | The confirmation judge enforced | Done: enforced on 20 episodes, it refused none of 40 writes; logged on 20 more, it would have stopped 2 real lapses for 2 false alarms, and passes did not move. The recommended setting enforces the first question and logs the second ([results](results/judge-live-2026-09-25.md)) | 782 credits |
| [#7](https://github.com/alexnodeland/stretto/issues/7) | `stretto_commit` live | Done offline: it could save at most 0.2–3.5% of LLM turns, too little for a ten-task pilot to see, so no live run for now ([results](results/commit-bound-2026-09-25.md)) | Free |
| [#8](https://github.com/alexnodeland/stretto/issues/8) | Flows and guards from frontier traces, serving a smaller agent | Guards bit in 38% of failed airline episodes on published runs, and never with GLM-5.3 | Priced by a smoke episode first |
| [#34](https://github.com/alexnodeland/stretto/issues/34) | The habit with the named-other count live, paired against D0 (in place of the searched flows) | Replayed on held-out tasks, the flow search found flows with far fewer detours than D0 ([results](results/search-2026-09-25.md)), and the named-other count matched them with no search: 6 airline detours, where D0 made 26 ([results](results/named-other-2026-09-26.md)). A replay assumes the agent acts the same with the flow's results in hand | About 240 credits for ten retail pairs and 340 for ten airline pairs |

## Evidence from offline runs

These cost Jev dollars, CPU time or people's time, and no LLM runs.

| Issue | What | Why | Cost |
|---|---|---|---|
| [#9](https://github.com/alexnodeland/stretto/issues/9) | The cold start on other agents, with more draws | The finding rests on one agent and five draws | About $10–15; several hundred replays, so #28 first |
| [#10](https://github.com/alexnodeland/stretto/issues/10) | Telecom, and an arbiter fitted on retail and airline together | Done: telecom compresses like the others, but neither shipped arbiter nor one fitted on both carries to it; the habit alone does better there ([results](results/telecom-2026-09-25.md)). Replayed, the habit-only flow saves 12.8% of the nine leaderboard agents' telecom turns once bindings order their sources by site ([results](results/telecom-flows-2026-09-26.md)) | $0.48 |
| [#11](https://github.com/alexnodeland/stretto/issues/11) | Independent labels for the confirmation judge | Its labels come from one annotator, the model that ran the analysis | Annotators' time |
| [#12](https://github.com/alexnodeland/stretto/issues/12) | Score the pilots as τ²-bench does | The pilots count the database check only; retail's natural-language assertions and airline's communication check are left out | Airline is free; retail needs a judge's key. Needs #29 |
| [#13](https://github.com/alexnodeland/stretto/issues/13) | Prompt injection against flows and the confirmation judge | Done: a flow stays inside its compiled lookups, but injected text steers the choice among them; one sentence in the questions and a higher bar absorb most of it ([results](results/injection-2026-09-25.md)) | $0.22 |

## Features from the design that are not built

| Issue | What | Where the design asks for it |
|---|---|---|
| [#14](https://github.com/alexnodeland/stretto/issues/14) | The confirmation judge in the proxy's guards, logged or enforced | Done: `stretto-proxy --confirm-judge log\|enforce` |
| [#15](https://github.com/alexnodeland/stretto/issues/15) | Counterfactual evaluation from logged decisions | Done: `--flow-explore` and `stretto evaluate`. On replays, the estimates match a rule's own replay for changes that keep a flow's chains, and miss a changed decider's detours ([results](results/evaluate-2026-09-25.md)) |
| [#16](https://github.com/alexnodeland/stretto/issues/16) | Predicate refinement | Done: `phase0 --candidates` and `stretto refine`. One round in airline kept two of eight candidates, which fitted the agents the proposer read but not GLM-5, and did not move the savings; the three hand-written predicates stay ([results](results/refine-2026-09-25.md)) |
| [#17](https://github.com/alexnodeland/stretto/issues/17) | Hand back when a session surprises the flow | RFC-001 §3.6; surprise is measured only after the fact. Simulated on replays, a hand-back, even one told of each detour at once, cut few: detours come in one burst per session, before anything surprises ([results](results/detours-2026-09-26.md)). Still open for sessions that go off script later |
| [#18](https://github.com/alexnodeland/stretto/issues/18) | Drift alarms | RFC-001 §3.3 and §4: change-point alarms and forgetting |
| [#19](https://github.com/alexnodeland/stretto/issues/19) | Shadow mode and per-site promotion | Done: `stretto-proxy --flow-shadow` and `stretto promote`; replayed, promotion halved D0's detours for 6% of its retail savings ([results](results/promotion-2026-09-25.md)) |
| [#20](https://github.com/alexnodeland/stretto/issues/20) | `stretto flow show` and `flow diff` | Done: `stretto flow-show` and `stretto flow-diff` ([reviewing flows](review.md)) |
| [#21](https://github.com/alexnodeland/stretto/issues/21) | Streamable HTTP for the proxy | Done: `stretto-proxy --upstream URL` |
| [#22](https://github.com/alexnodeland/stretto/issues/22) | Learn from several servers' logs of one session | One proxy wraps one server |
| [#23](https://github.com/alexnodeland/stretto/issues/23) | Learn from OpenTelemetry GenAI spans | RFC-001 §3.9's `stretto-trace` |
| [#24](https://github.com/alexnodeland/stretto/issues/24) | Privacy for recorded sessions | Done: [an inventory](privacy.md), `stretto redact` and `stretto-proxy --retain-days`. Leaving fields out of the questions' state is still open |
| [#25](https://github.com/alexnodeland/stretto/issues/25) | Flow search with fugue-evo | Done: `stretto search` and per-site thresholds in the flow IR. Replayed on held-out tasks, its front held flows with far fewer detours than D0 in both domains, for 7 more turns saved in airline and one fewer in retail, each by changing one or two sites' thresholds ([results](results/search-2026-09-25.md)). They have not run live . The named-other count reaches the same gains from the traces, with no search ([results](results/named-other-2026-09-26.md)) |
| [#35](https://github.com/alexnodeland/stretto/issues/35) | Staged learning: update flows as sessions arrive, evaluate in the background, commit explicitly | Served sessions teach a flow as much as clean ones, and relearning on its own sessions holds with the named-other count ([results](results/served-sessions-2026-09-26.md)). What changes a flow's lookups or bindings still needs review (`flow-diff`) |
| [#36](https://github.com/alexnodeland/stretto/issues/36) | A web app: a sanctioned container and a UI for flows, servers and commits | Reviewing a flow, its diff and its pinned inputs, and managing the servers behind the proxy |
| [#37](https://github.com/alexnodeland/stretto/issues/37) | Macros that write: flows that prepare a write sequence for one confirmation | RFC-001 §3.5's plan/commit; a write-bearing flow is judged on pass rate and compliance, not turns ([the anatomy](results/anatomy-2026-09-26.md)) |
| [#38](https://github.com/alexnodeland/stretto/issues/38) | Compiled procedures for agents that work alone, handing back on the outcome | In telecom's solo mode a workflow compiled once from traces passed as many held-out tasks as the agents, with no model, and its outcome check caught every one of its failures ([results](results/telecom-workflow-2026-09-26.md)). With that check as the verifier, its own tries in τ²-bench's environment took a workflow fitted on half the demonstrations from 18 to 22–23 of 40 ([learning from its own runs](results/telecom-workflow-2026-09-26.md#learning-from-its-own-runs)). Learned as where they came from, its identifiers bound for a customer no trace saw: renamed throughout τ²-bench's database and tasks, it passed the same 32 of 40 as for the original, against 16 with constants ([a customer no trace saw](results/telecom-workflow-2026-09-26.md#a-customer-no-trace-saw)). Next: customers whose accounts differ, not only their names |
| [#39](https://github.com/alexnodeland/stretto/issues/39) | Stop reading once the described record is found | What is left of the detours on an agent a flow never trained on is a semantic stop ([results](results/detours-2026-09-26.md)): a question at the sites that iterate, scored per agent. Where the description is an exact value (a ticket's phone number), the binding now scores it with no model (`bindings.described_read`), which halved the solo telecom flow's detours ([results](results/telecom-flows-2026-09-26.md#when-the-agent-holds-the-phone)); airline's cities are still language |

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
4. **Live runs, as budgets are approved.** The paired run (#2), the cold start live in both domains (#3), Claude models as the agent and the customer (#4, #5) and the confirmation judge enforced (#6) are done. The paired run puts the pass-rate change between −7.5 and +6.25 points, not within one. Next come the searched flows live (#34) and flows and guards serving a smaller agent (#8).
5. **Phase 3 features** as the evidence calls for them:
   - counterfactual evaluation (#15), done;
   - predicate refinement (#16), done;
   - flow search (#25), done.
