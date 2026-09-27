# Claims and their evidence

Each number in [the working paper](../../paper/stretto.md)'s abstract and contributions, what kind of evidence it rests on, and where it is recomputed. *Live* means agents run with the speculator; *replay* means a speculator replayed against τ²-bench's environment; *record* means a replay or count from published trajectories alone (`pilot/check_flow.py --trace`), whose savings are exact and whose detours are lower bounds. Intervals are 95%, from bootstraps over tasks.

| Claim | Number | Evidence | Recomputed by |
|---|---|---|---|
| Counting the right event calibrates the probability of use | expected calibration error 0.01–0.08 in every τ²-bench domain, against 0.06–0.16 for the next-step probability | Replay, nine agents | `scripts/calibration.py` ([results](reach-2026-09-26.md)) |
| The speculator takes most of retail's ceiling | 86.4% of it [80.2, 93.1], 10.2 points more than the next-step speculator [6.7, 14.2] | Replay, nine agents it never saw | [results](reach-2026-09-26.md), the paper's Table 3 |
| It works live with no model of its own | GLM-5.3 took 27.9% fewer LLM turns (19.1–35.9%) on 28 retail and airline tasks; 21 passed, against 24 | Live, paired with the recorded baseline | [results](reach-2026-09-26.md) and its episodes archive |
| The ceiling is the domain's | 3.5% of turns in WorkBench to 47.1% in AgentDojo's travel suite; 29.0% in τ²-bench | Record, 89 more agents on six benchmarks | `scripts/ceiling.py`, `scripts/bench/replay.sh ceilings` ([results](benchmarks-2026-09-27.md)) |
| Reading the request is a model's job | of the 3–7 points the user's words would add, a small model that picks the value among them takes 30–53% and a pattern 0–17% | Record, with Jev's answers (8,532 questions) | `ceiling.py --questions`, `scripts/model_questions.py`, `ceiling.py --model-answers` ([results](benchmarks-2026-09-27.md#what-decides-an-agents-turns)) |
| It gains where agents read ahead | τ-bench retail +1.4 points (0.8–2.1); BFCL +0.85 (0.35–1.45); DTap-Bench, the agent's own sessions, +1.3 (1.1–1.5) | Record | `scripts/bench/replay.sh`, `scripts/bench/tables.py` ([results](benchmarks-2026-09-27.md)) |
| It stays out where it cannot help | no lookup in WorkBench's 13,869 turns; in DTap-Bench's medical domain both speculators make the same lookups | Record | the same |
| It carries across harnesses | learned in other harnesses, 59% of what the agent's own sessions save (64% of their detours); from its harness-mates, 79% | Record, DTap-Bench's three agent SDKs | the same ([results](benchmarks-2026-09-27.md#across-harnesses)) |
| It learns fast | ten of an agent's own sessions give 96% (retail) and 93% (airline) of what all of them do | Replay, three agents, three orders | `scripts/learning_curve.py` ([results](reach-2026-09-26.md#how-fast-it-learns)) |
| Where no user speaks, compile once | the procedure alone passes 35 of 40 held-out solo telecom tasks with no model; with GLM-5.3 on its four hand-backs, 39 of 40 at 0.48 LLM turns per ticket, against 15.9 for GLM-5.3 alone | The procedure run in τ²-bench's environment; the hand-backs live, two trials | `scripts/telecom_workflow.py`, `stretto-procedure` ([results](reach-2026-09-26.md), [workflow](telecom-workflow-2026-09-26.md)) |

What the evidence does not show, in brief (the paper's §6 has the rest):

- **Replays assume the agent skips what a lookup already answered.** GLM-5.3 did live. In the record, agents repeat 0.8–2.6% of their reads with no write between on five benchmarks, and a few models many more (gpt-oss-120b 23%, Qwen3.5 Flash 17.5%), whose replayed savings are upper bounds.
- **Detours from the record are lower bounds**: 55–72% of the environment's on τ²-bench.
- **Few agents live.** The use-before-write speculator ran live with GLM-5.3 only; an earlier flow also ran with Claude Haiku 4.5 and Claude Sonnet 5 on ten and three retail tasks ([results](claude-models-2026-09-25.md)). Every other agent is replayed.
- **Pass rates are underpowered**, and τ²-bench's users are LLMs.
