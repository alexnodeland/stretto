---
description: The research behind stretto - the working paper, the claims ledger, every results page, the benchmarks round, and how to reproduce them.
---

# Research

stretto is built on measurements, and publishes them: the paper, every round's results with their rows and episodes, and the answers of the System-One model, so that each number can be recomputed.

## The paper

**[Compile What the Environment Decides: Read-Only Speculation and Compiled Procedures for LLM Agents](./paper)** (draft, 2026-09-27).

An LLM agent pays a model turn for every decision, yet many of its decisions are fixed by what its tools returned, not by what the user said. The paper makes that precise and uses it in two regimes. While a user is present, a program learned from traces can take over only reads, speculatively, and should make each read whose probability of use before the next write exceeds $\delta/(\beta+\delta)$. Where no user speaks, a whole procedure, writes included, can be compiled once and handed to a model only when its outcome check fails.

The headline numbers, each with its evidence in [the claims ledger](./claims):

| Claim | Number | Evidence |
|---|---|---|
| It works live with no model of its own | GLM-5.3 took 27.9% fewer LLM turns (19.1–35.9%) on 28 retail and airline tasks; 21 passed, against 24 | Live |
| It holds live beyond τ²-bench | In AgentDojo's Slack and travel suites, GLM-5.3 and Claude Haiku 4.5 took 10.1% fewer LLM turns (5.8–14.0%), with passes unchanged; 6.0% over all 41 tasks, where the replay projected 7.2%. BFCL, projected at 1.6%, shows none | Live, in the benchmarks' own environments |
| The speculator takes most of retail's ceiling | 86.4% of it [80.2, 93.1], 10.2 points more than the next-step speculator [6.7, 14.2] | Replay, nine agents it never saw |
| The ceiling is the domain's | 3.5% of turns in WorkBench to 47.1% in AgentDojo's travel suite; 29.0% in τ²-bench | Record, 89 more agents on six benchmarks |
| Where no user speaks, compile once | 35 of 40 held-out solo telecom tasks with no model; 39 of 40 with GLM-5.3 on its four hand-backs, at 0.48 LLM turns per ticket against 15.9 | Environment and live |

*Live* means agents ran with the speculator; *replay*, a speculator replayed against τ²-bench's environment; *record*, a replay from published trajectories alone, whose savings are exact and whose detours are lower bounds.

## Everything else

| Page | What it holds |
|---|---|
| [Claims and evidence](./claims) | Each number in the paper's abstract and contributions, the kind of evidence it rests on, and what recomputes it |
| [Results index](./results) | Every results page, in the order it was found, with what each found |
| [Seven benchmarks](./benchmarks) | τ²-bench's results carried to τ-bench, BFCL, AgentDojo, WorkBench, DTap-Bench and MCPMark |
| [Live on AgentDojo and BFCL](./live) | Two agents run live in two of those benchmarks' own environments, with and without the published flow, and promotion on the agents' own sessions |
| [Reproduce the results](./reproduce) | The commands, from Phase 0 with no key to the benchmarks round |
| [Design summary](/reference/design) | The decisions, and the implementation status of each piece |
| [Research notebook](/notebook/) | The illustrated notebook of the rounds through 2026-09-25, the project's first site |

The full design rationale is [RFC-001](../../docs/rfc/001-habit-compiler.md), which records what each round changed, amendment by amendment. What is still open is on [the roadmap](/community/roadmap).
