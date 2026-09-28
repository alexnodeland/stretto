# What bundling writes could save, on a write-heavy benchmark

Flows only read. [#37](https://github.com/alexnodeland/stretto/issues/37) proposes flows that also prepare writes: learn which writes follow which, and where each argument comes from, and offer the whole sequence for the agent to confirm and commit in one call. On τ²-bench, bundling consecutive confirmed writes could save at most 0.2–3.5% of LLM turns ([the commit bound](commit-bound-2026-09-25.md)), since its tasks write little. The issue asked for the same bound on a workload that writes a lot before anything is built. This page takes WorkBench.

**The workload.** WorkBench's tasks are an office worker's requests to an email client, a calendar, a CRM, a project board and a web-analytics tool, alone or across them ([the benchmarks round](benchmarks-2026-09-27.md)). Every send, reply, forward, create, update and delete is a write. The runs are the published trajectories of 24 models, from GPT-3.5 to Claude Opus 4.8, on every task, converted as that round did: 16,507 episodes and 59,157 LLM turns.

- 83% of the episodes write, and a quarter write twice or more (9,549 once, 4,129 twice or more).
- 26% of all LLM turns make only writes.

**The bound.** The same as τ²-bench's. A run of k assistant turns that each make only writes could be one call, saving k − 1 turns. A turn that already makes several writes counts once. A read in between ends the run, since a bundle of writes carries only what the agent would have sent anyway. WorkBench has no customer to confirm anything: each request is given whole at the start, so every run counts. It is an upper bound, which assumes that the agent bundles every run.

| Domain | LLM turns | Turns that only write | Runs of 2+ | Turns saved |
|---|---|---|---|---|
| email | 7,339 | 2,215 (30.2%) | 31 | 37 (0.5%) |
| calendar | 8,614 | 2,411 (28.0%) | 62 | 95 (1.1%) |
| analytics | 7,916 | 2,372 (30.0%) | 153 | 161 (2.0%) |
| CRM | 6,902 | 1,669 (24.2%) | 35 | 97 (1.4%) |
| project management | 7,494 | 1,629 (21.7%) | 85 | 239 (3.2%) |
| multi-domain | 20,892 | 5,109 (24.5%) | 376 | 542 (2.6%) |
| **all** | **59,157** | **15,405 (26.0%)** | **742** | **1,171 (2.0%)** |

| Model | LLM turns | Turns that only write | Runs of 2+ | Turns saved |
|---|---|---|---|---|
| o3 | 2,978 | 931 (31.3%) | 163 | 357 (12.0%) |
| GPT-3.5 | 2,402 | 662 (27.6%) | 96 | 131 (5.5%) |
| GLM-4.6 | 2,897 | 716 (24.7%) | 50 | 113 (3.9%) |
| GPT-5.4 nano | 2,619 | 670 (25.6%) | 52 | 69 (2.6%) |
| GPT-5.4 mini | 2,553 | 727 (28.5%) | 42 | 67 (2.6%) |
| Mistral Medium 3.5 | 2,942 | 782 (26.6%) | 56 | 70 (2.4%) |
| Qwen3.5 Flash | 2,771 | 717 (25.9%) | 44 | 58 (2.1%) |
| Mistral Small 2603 | 2,622 | 740 (28.2%) | 42 | 49 (1.9%) |
| Kimi K2.6 | 2,238 | 579 (25.9%) | 15 | 34 (1.5%) |
| GPT-4.1 | 2,458 | 614 (25.0%) | 29 | 33 (1.3%) |
| Gemini 3.5 Flash | 2,928 | 649 (22.2%) | 21 | 36 (1.2%) |
| GPT-4 Turbo | 2,298 | 548 (23.8%) | 28 | 28 (1.2%) |
| GPT-5.2 | 2,428 | 616 (25.4%) | 27 | 28 (1.2%) |
| GPT-5.1 | 2,175 | 564 (25.9%) | 20 | 23 (1.1%) |
| GPT-5.4 | 2,345 | 618 (26.4%) | 12 | 24 (1.0%) |
| GPT-5 | 2,238 | 592 (26.5%) | 15 | 18 (0.8%) |
| Gemini 3.1 Pro | 2,449 | 626 (25.6%) | 9 | 11 (0.4%) |
| GPT-4o | 2,192 | 524 (23.9%) | 6 | 6 (0.3%) |
| DeepSeek V4 Pro | 2,286 | 585 (25.6%) | 6 | 6 (0.3%) |
| Claude Sonnet 4.6 | 2,155 | 584 (27.1%) | 3 | 3 (0.1%) |
| GPT-5.5 | 2,238 | 597 (26.7%) | 2 | 3 (0.1%) |
| Claude Haiku 4.5 | 2,267 | 547 (24.1%) | 3 | 3 (0.1%) |
| Claude Fable 5 | 2,329 | 614 (26.4%) | 1 | 1 (0.0%) |
| Claude Opus 4.8 | 2,349 | 603 (25.7%) | 0 | 0 (0.0%) |

## Findings

- **At most 2.0% of LLM turns, though a quarter of them write.** Two things leave little to bundle:
  - Most models already make a task's consecutive writes in one turn. Claude Opus 4.8, GPT-5.5 and Gemini 3.1 Pro average 1.5–1.6 writes per turn that only writes, and at most 0.4% of their turns are left to save.
  - Most other writes follow a read that finds their target, such as a search for the email to delete. A bundle cannot hold the read.
- **The exception is an agent that writes one call per turn.** o3 never put two writes in one turn, and bundling could save it 12.0% of its turns. GPT-3.5 could save 5.5% and GLM-4.6 3.9%. These are the agents `stretto_commit` was built for, and they need nothing learned: the agent knows which writes it is about to make, and needs only the tool and a word on when to use it. `stretto-proxy --commit` now adds that word to the server's instructions ([#37](https://github.com/alexnodeland/stretto/issues/37), [PR #67](https://github.com/alexnodeland/stretto/pull/67)).
- **So learned write sequences are not built.** Their ceiling is this bound, the agents that could reach it reach it with `stretto_commit` alone, and a wrong prepared write is an action, not a detour. What would change this is a workload where the agent repeats a write sequence it cannot see ahead, such as a sequence whose later writes depend on what the earlier ones return. On τ²-bench and WorkBench, the writes that follow each other are ones the agent already knows it will make.

## Reproduce

With WorkBench's published runs converted as [the benchmarks round](benchmarks-2026-09-27.md#reproduce) converts them (`scripts/workbench_to_tau2.py`, into `CONVERTED_DIR/<domain>/<model>.json`):

```sh
python3 scripts/commit_bound.py --workbench CONVERTED_DIR
```

It prints both tables. τ²-bench's bound runs as before, and needs τ²-bench's Python environment only for that.
