# How many turns `stretto_commit` could save

`stretto-proxy --commit` adds `stretto_commit`, which makes several writes in one call, in order, each checked by the guards first. It is meant for the writes a customer has confirmed, in one LLM turn instead of one turn each. It is built and tested, and no live run has used it. This page bounds what it could save, before any budget is spent on it.

**The bound.** After the customer last spoke, a run of k assistant turns that each make only writes could be one call, saving k − 1 turns. A turn that already makes several writes at once counts once. Reads in between end a run, since `stretto_commit` only bundles what the agent would have sent anyway. It is an upper bound: it assumes the agent bundles every such run, and that the customer's confirmation covered all of it.

| Domain | Agent | LLM turns | Runs of 2+ write turns | Turns a commit call saves |
|---|---|---|---|---|
| retail | Claude 3.7 Sonnet | 6,743 | 119 | 147 (2.2%) |
| retail | GPT-4.1 | 5,560 | 13 | 13 (0.2%) |
| retail | GPT-4.1 mini | 6,124 | 24 | 27 (0.4%) |
| retail | o4-mini | 6,409 | 113 | 157 (2.4%) |
| retail | GLM-5.3 in Claude Code, the retail pilot | 123 | 1 | 1 (0.8%) |
| airline | Claude 3.7 Sonnet | 2,900 | 56 | 102 (3.5%) |
| airline | GPT-4.1 | 2,183 | 18 | 30 (1.4%) |
| airline | GPT-4.1 mini | 2,444 | 24 | 28 (1.1%) |
| airline | o4-mini | 2,187 | 32 | 54 (2.5%) |
| airline | GLM-5.3 in Claude Code, the airline pilot | 143 | 1 | 5 (3.5%) |

The first four rows of each domain are τ²-bench's published baselines (456 retail and 200 airline episodes each); the last are the baseline arms of [the retail](pilot-2026-09-24.md) and [airline](pilot-airline-2026-09-24.md) pilots. On the 40 baseline episodes of the retail paired run (#2), GLM-5.3 would save 13 of 538 turns (2.4%).

## Findings

- **At most 0.2–3.5% of LLM turns.** The sequential callers, Claude 3.7 Sonnet and o4-mini, gain the most, since they make each write in its own turn. GPT-4.1 and GPT-4.1 mini already put several writes in one turn when they have them.
- **Too small for a live pilot now.** A flow saves a quarter to a third of turns live. Ten paired tasks would give `stretto_commit` about three turns to save in all, which no run of that size can tell from the noise between episodes, so the 240–340 credits a pilot costs would buy no answer. The tool stays built.
- **Where it could matter:** tasks that make many writes after one confirmation. The airline pilot's one run was six writes in six turns. A live test should choose such tasks, or wait for an agent or domain that writes more.

## Reproduce

With τ²-bench's Python environment, and the pilots' episodes unpacked from [their archives](episodes-2026-09-24.md):

```sh
python3 scripts/commit_bound.py ../tau2-bench pilot-2026-09-24-episodes/baseline pilot-airline-2026-09-24-episodes/baseline
```
