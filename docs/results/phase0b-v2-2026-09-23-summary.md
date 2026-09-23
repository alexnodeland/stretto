# Phase 0b v2 results, 2026-09-23: summary

v2 changes what flows are allowed to do and how the System-One model is asked, as RFC-001 §3.5–3.6 specify. The full generated report is [phase0b-v2-2026-09-23-report.md](phase0b-v2-2026-09-23-report.md). Aggregate metrics are in [phase0b-v2-2026-09-23-aggregates.json](phase0b-v2-2026-09-23-aggregates.json). Jev's raw answers are in [phase0b-v2-2026-09-23-answers.jsonl.gz](phase0b-v2-2026-09-23-answers.jsonl.gz); see [the notice](phase0b-v2-2026-09-23-answers.md). All were produced by commit dfa8c07.

- **Jev model:** jev-1.13.0, the same version as v1. It answered every question.
- **Questions:**
  - retail: 5,497 distinct (5,852 decisions);
  - airline: 2,612 distinct (3,099 decisions);
  - 0 failed.
- **Input tokens:**
  - retail 13.4M ($0.56) and airline 5.7M ($0.24);
  - with the two 300-per-domain pilots below, this session's Jev spend was 22.5M tokens ($0.94).
- **Agent models:** the four τ²-bench baselines are the source models. GLM-5 is the transfer target, as in v1.

## What changed

1. **Read-only flows.** The v1 projection let a flow call any tool between LLM turns. The design never allowed that: writes go through plan/commit pairs.
   - v2 flows only read between LLM turns. Writes, and any tool not marked read-only, go back to the LLM.
   - A wrong pick is then an extra lookup that changes nothing: a *detour*. It costs a pause and some tokens, but it is not a risk.
   - This costs little. With a perfect System-One model, GLM-5's ceiling goes from 29.1% to 28.2% of LLM turns in retail, and from 15.4% to 14.9% in airline.
2. **Narrower questions.** A site's options are the lookups the agent made after the same tool in training, plus "hand back".
   - A site with no lookups hands back without asking: 230 retail and 466 airline decisions.
   - The options hold the agent's step in 99.1% of decisions.
   - The stop decision is also asked on its own: "does it make another lookup now?", then which one. This is the *split* answer.
   - The state is a slice rather than a transcript.
   - Numeric arguments are not asked about, because Jev does no arithmetic.
3. **Arbitration.** A conditional logit combines three things over each decision's options:
   - the habit's prior;
   - both question designs;
   - Jev's record at the site on other tasks (a one-coin Dawid–Skene sensor).

   It is fitted by cross-validation over tasks, on the source models only.

## Findings

1. **Read-only flows remove the risk; they don't raise the savings.** No v2 projection takes a risky decision, by construction. In v1, trusting Jev at p ≥ 0.9 left a risky call in 6–13% of episodes.

2. **Narrower questions did not make Jev more accurate.** This compares the same decisions, scored the same way (a write counts as "hand back"), on the 300-per-domain v1 pilot sample:

   | Domain | Decisions | v1 question | v2 one question | v2 combined |
   |---|---|---|---|---|
   | Retail | 301 | 77.7% | 73.1% | 79.7% |
   | Airline | 293 | 73.4% | 73.4% | 75.1% |

   - The split answer is worse than the one question: 73.8% and 70.2% on the source models.
   - Its stop probability is rarely confident enough to use on its own.

3. **Combining with the habit helps a little, mostly at the top of the scale.** Over all source-model decisions:
   - **Retail:** 76.2% for the one question, 77.9% combined. At p ≥ 0.99 the combined answer covers 6.2% of decisions at 99.0% agreement.
   - **Airline:** 72.4% one question, 74.3% combined. At p ≥ 0.99 it covers 16.2% at 97.7%.

   The site's record carries weight (0.26 in retail, 0.69 in airline), as §3.6 expected.

4. **The savings stay small at high confidence.**
   - A run collapses only if every decision in it is taken. High-confidence answers are scattered, so at p ≥ 0.9 flows save only 2.5–4.9% of turns.
   - Because read-only picks are harmless, flows can act at p ≥ 0.5 instead. Pooled over the source models, combined answers then save:

   | Domain | Turns saved | Episodes with a detour |
   |---|---|---|
   | Retail | 15.0% | 57% |
   | Airline | 10.4% | 49% |

5. **The gate:**
   - Claude 3.7 Sonnet in retail passes offline: 20.2% of turns saved at p ≥ 0.5, with detours in 19% of episodes. Combined, it is 22.4% with detours in 22%.
   - GLM-5 fails in both domains. In retail it saves 14.7% (combined, p ≥ 0.5) against a 28.2% ceiling. In airline it saves 2.7% against 14.9%, and even the ceiling is below 20%.
   - Offline, a detour costs no pass^1 by construction. Whether that holds is for the live run to show.

6. **Where the gap is.** Most lost savings are early hand-backs inside lookup chains. These counts cover all models, broken down by the agent's next step:
   - **Retail:** where the agent's next step was a product lookup, the combined answer matched it only 41% of the time and handed back in 58%.
   - **Airline:** where it was a flight search, the combined answer matched 57% and handed back in 36%.

   Jev doesn't know what the lookup is for. *Dataflow hints* describe each lookup by the write arguments its results supplied in training, e.g. "its results supply `new_item_ids` for `exchange_delivered_order_items`". A 300-per-domain pilot compared hinted and unhinted answers on the same decisions:

   | Domain | Without hints | With hints | Targeted site |
   |---|---|---|---|
   | Retail | 72.8% | 73.2% | product lookups 26% → 49% |
   | Airline | 63.8% | 65.1% | flight searches 62% → 71% |

   The targeted sites improve, but losses at other sites cancel most of it: retail order lookups fell from 71% to 65%. The hints are kept behind `--dataflow-hints` and were not run at full scale.

## Retail

Pooled projection over the source models, read-only flows:

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **14.0%** | 1.54 | 6.85 | 96.1 | 67.0 | 53.9% |
| p ≥ 0.9 | **3.5%** | 3.26 | 3.12 | 21.7 | 8.1 | 8.0% |
| combined, p ≥ 0.5 | **15.0%** | 1.37 | 6.74 | 73.8 | 73.0 | 56.9% |
| combined, p ≥ 0.7 | **11.6%** | 1.87 | 4.95 | 34.2 | 40.2 | 35.2% |
| combined, p ≥ 0.9 | **2.5%** | 3.41 | 1.88 | 4.5 | 2.2 | 2.2% |
| combined, p ≥ 0.99 | **0.1%** | 3.87 | 0.44 | 0.5 | 0.0 | 0.0% |

Gate (turns saved · episodes with a detour):

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | Gate |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 31.3% | 20.2% · 18.8% | 15.6% · 12.5% | 4.1% · 1.9% | 22.4% · 21.9% | 17.2% · 11.9% | 2.5% · 0.0% | **passes offline** (p ≥ 0.5; detours in 19% of episodes) |
| gpt-4.1 | 19.0% | 12.7% · 55.0% | 8.7% · 35.0% | 3.2% · 6.2% | 14.0% · 57.5% | 9.3% · 34.4% | 2.4% · 0.6% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 12.0% | 6.8% · 76.2% | 5.1% · 37.5% | 1.8% · 12.5% | 6.9% · 75.0% | 5.7% · 42.5% | 1.7% · 1.2% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.5% | 15.5% · 65.6% | 12.4% · 54.4% | 4.6% · 11.2% | 16.0% · 73.1% | 13.1% · 51.9% | 3.3% · 6.9% | fails |
| glm-5 *(target)* | 28.2% | 11.6% · 11.9% | 6.5% · 1.2% | 2.6% · 0.0% | 14.7% · 12.5% | 7.9% · 4.4% | 2.5% · 0.0% | fails |

## Airline

Pooled projection over the source models, read-only flows:

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Detours (/100 ep) | Episodes with a detour |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **10.0%** | 2.63 | 6.16 | 96.9 | 65.0 | 46.6% |
| p ≥ 0.9 | **4.6%** | 3.69 | 2.87 | 17.8 | 7.8 | 7.2% |
| combined, p ≥ 0.5 | **10.4%** | 2.58 | 5.64 | 58.8 | 70.9 | 49.4% |
| combined, p ≥ 0.7 | **8.1%** | 3.08 | 3.66 | 20.6 | 31.9 | 28.7% |
| combined, p ≥ 0.9 | **4.9%** | 3.59 | 2.23 | 4.7 | 5.9 | 5.9% |
| combined, p ≥ 0.99 | **0.1%** | 4.59 | 0.76 | 2.8 | 0.0 | 0.0% |

Gate (turns saved · episodes with a detour):

| Agent model | Perfect System-One (read-only) | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | combined, p ≥ 0.5 | combined, p ≥ 0.7 | combined, p ≥ 0.9 | Gate |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 32.6% | 15.7% · 35.0% | 11.2% · 20.0% | 5.4% · 3.8% | 16.6% · 36.2% | 11.9% · 10.0% | 6.8% · 1.2% | fails |
| gpt-4.1 | 8.9% | 5.0% · 52.5% | 4.6% · 17.5% | 3.2% · 1.2% | 5.5% · 60.0% | 5.3% · 32.5% | 3.3% · 0.0% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 5.3% | 2.1% · 62.5% | 2.0% · 38.8% | 1.7% · 13.8% | 2.6% · 63.7% | 2.4% · 47.5% | 1.7% · 13.8% | fails: below 20% even with a perfect System-One model |
| o4-mini | 20.7% | 16.1% · 36.2% | 14.3% · 26.2% | 8.2% · 10.0% | 16.1% · 37.5% | 12.1% · 25.0% | 7.6% · 8.8% | fails |
| glm-5 *(target)* | 14.9% | 2.1% · 48.8% | 1.6% · 30.0% | 0.8% · 15.0% | 2.7% · 36.2% | 1.7% · 10.0% | 0.6% · 3.8% | fails: below 20% even with a perfect System-One model |

## Caveats

- **Agreement is a proxy.** It counts an equally valid next step as a disagreement.
- **Detours are not charged tokens.** The token accounting does not charge a detour for the extra tool output it adds to later prompts.
- **The v1 comparison uses a sample.** The paired v1 comparison uses the 300-per-domain v1 pilot sample, not all of v1's answers.

## Reproduce

```sh
gunzip -c docs/results/phase0b-v2-2026-09-23-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
stretto phase0 --tau2 ../tau2-bench $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle replay --questions v2
```

- **Hints pilot:** add `--dataflow-hints --oracle-limit 300`.
- **v1 pilot:** use `--questions v1 --oracle-limit 300`.

The bundle holds the answers to both pilots too.
