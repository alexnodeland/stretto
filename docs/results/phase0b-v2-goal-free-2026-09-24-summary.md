# Phase 0b v2 without a goal, 2026-09-24

A live flow that continues an agent's lookups (the pilot's flows arm, design D0) has no goal: nobody names the writes the episode will make. Every earlier v2 run gave flows that goal. The habit was conditioned on it, and the System-One questions named it (`flow_goal`). This run takes the goal away (`--no-intent`) and asks everything again: 8,092 new questions, $0.93 at Jev's price.

| Run | Report | Aggregates |
|---|---|---|
| v2, state predicates, no goal, GLM-5 as the transfer target | [phase0b-v2-goal-free-2026-09-24-report.md](phase0b-v2-goal-free-2026-09-24-report.md) | [aggregates](phase0b-v2-goal-free-2026-09-24-aggregates.json) |

The comparison is the sharpened run of [2026-09-23](phase0b-v2-2026-09-23-summary.md), which had the goal. It also had dataflow hints, which add nothing.

## Finding: the goal adds nothing measurable to read-only flows

Pooled over the source models (GLM-5 in brackets):

| | Retail, with goal | Retail, no goal | Airline, with goal | Airline, no goal |
|---|---|---|---|---|
| Next step, one question | 75.7% | 76.1% | 73.3% | 71.1% |
| Next step, combined | 80.5% (85.7%) | 80.5% (85.6%) | 78.9% (75.8%) | 78.4% (76.1%) |
| Turns saved, combined, p ≥ 0.9 | 4.4% | 4.9% | 5.9% | 5.5% |
| Turns saved, combined, p ≥ 0.5 | 15.9% | 15.6% | 11.4% | 11.0% |
| Turns saved, lookup first, p ≥ 0.3 | 17.4% | 17.3% | 12.8% | 12.6% |
| Episodes with a detour, lookup first, p ≥ 0.3 | 67.3% | 62.3% | 57.2% | 60.0% |

GLM-5 still clears the gate in retail: 20.3% of turns saved with lookup first at p ≥ 0.3, with detours in 28% of episodes (20.5% and 32% with the goal). It still fails in airline (7.1%).

This fits what read-only flows do. The goal names the writes, and a read-only flow hands every write back. What it must decide, which record to look up next and whether it has enough, is in the lookups already made and in what the customer said. The combined answer recovers the 2-point loss of the one-question answer in airline.

For the live pilot, a flow can run behind the agent's own calls, with no new tools and no goal, at the offline savings of a flow the LLM names.

## Caveats

The caveats of [the 2026-09-23 summary](phase0b-v2-2026-09-23-summary.md#caveats) apply. This is one run per condition, so differences under a point are noise.

## Answers

[phase0b-v2-goal-free-2026-09-24-answers.jsonl.gz](phase0b-v2-goal-free-2026-09-24-answers.jsonl.gz) holds the 8,092 new answers. It uses the same format and terms as [the v2 bundle](phase0b-v2-2026-09-23-answers.md): keys and responses only, provided only to reproduce and audit this analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)), the answers may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.

Together with the v2 bundle, it also compiles the live flow (`stretto flow-serve`) without a key. Only the flow's live questions need one.

## Reproduce

[The CLI reference](../cli.md) lists every option.

```sh
for b in phase0b-v2-2026-09-23 phase0b-v2-goal-free-2026-09-24; do
  gunzip -c docs/results/$b-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
stretto phase0 --tau2 ../tau2-bench $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle replay --questions v2 --predicates data/predicates-v2.json --no-intent
```
