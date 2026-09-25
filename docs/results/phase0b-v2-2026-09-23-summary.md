# Phase 0b v2 results, 2026-09-23: summary

v2 changes what flows are allowed to do and how the System-One model is asked, as RFC-001 §3.5–3.6 specify.

Three generated reports and their aggregate metrics, all replayed from cached answers at commit 90d834b:

| Run | Report | Aggregates |
|---|---|---|
| v2, GLM-5 as the transfer target | [phase0b-v2-2026-09-23-report.md](phase0b-v2-2026-09-23-report.md) | [aggregates](phase0b-v2-2026-09-23-aggregates.json) |
| v2, all nine leaderboard models as transfer targets | [phase0b-v2-2026-09-23-targets-report.md](phase0b-v2-2026-09-23-targets-report.md) | [aggregates](phase0b-v2-2026-09-23-targets-aggregates.json) |
| v2 sharpened: dataflow hints and state predicates, GLM-5 | [phase0b-v2-2026-09-23-sharpened-report.md](phase0b-v2-2026-09-23-sharpened-report.md) | [aggregates](phase0b-v2-2026-09-23-sharpened-aggregates.json) |

Jev's raw answers to every question are in [phase0b-v2-2026-09-23-answers.jsonl.gz](phase0b-v2-2026-09-23-answers.jsonl.gz); see [the notice](phase0b-v2-2026-09-23-answers.md).

- **Jev model:** jev-1.13.0, the same version as v1. It answered every question; none failed.
- **Spend:** 80.7M input tokens, $3.39 at $0.042/MTok:
  - v2 on the GLM-5 set: $0.80;
  - the other eight targets: $1.43;
  - the sharpened run: $1.06;
  - two 300-per-domain pilots: $0.10.
- **Source models:** the four τ²-bench baselines. Everything is learned from their training tasks. The leaderboard models are transfer targets only; nothing, the arbiter included, is fitted on them.

## What changed

1. **Read-only flows.** The v1 projection let a flow call any tool between LLM turns. The design never allowed that: writes go through plan/commit pairs.
   - v2 flows only read between LLM turns. Writes, and any tool not marked read-only, go back to the LLM.
   - A wrong pick is then an extra lookup that changes nothing: a *detour*. It costs a pause, and the projection charges its output (a typical lookup's, about 250 tokens in retail and 200 in airline) to every later prompt. It is not a risk.
   - This costs little. With a perfect System-One model, GLM-5's ceiling goes from 29.1% to 28.2% of LLM turns in retail, and from 15.4% to 14.9% in airline.
2. **Narrower questions.**
   - **Options:** a site's options are the lookups the agent made after the same tool in training, plus "hand back". A site with no lookups hands back without asking. The options hold the agent's step in 99.1% of decisions.
   - **Stop question:** the stop decision is also asked on its own: "does it make another lookup now?", then which one. This is the *split* answer.
   - **State:** a slice rather than a transcript.
   - **Arguments:** numeric arguments are not asked about, because Jev does no arithmetic.
3. **Arbitration.** A conditional logit combines:
   - the habit's prior;
   - both question designs;
   - Jev's record at the site on other tasks (a one-coin Dawid–Skene sensor);
   - optionally, the answers to state predicates.

   It is fitted by cross-validation over tasks, on the source models only.
4. **Lookup first.** Handing back costs a turn and a wrong lookup does not. So a read-only flow can take its likeliest lookup whenever that lookup clears a lower bar (p ≥ 0.2–0.4), even when handing back is likelier.
5. **Sharpening,** tried on the GLM-5 set:
   - **Dataflow hints** describe each lookup by the write arguments its results supplied in training, for example "its results supply `new_item_ids` for `exchange_delivered_order_items`".
   - **State predicates** follow RFC-001 §3.4, with Claude as the proposing LLM. Three domain-agnostic yes/no questions, in [`data/predicates-v2.json`](../../data/predicates-v2.json), ask:
     - whether records in a list are still unchecked;
     - whether the request needs options not yet looked up;
     - whether only the customer can give what is needed next.

## Findings

1. **Read-only flows remove the risk; they don't raise the savings.** No v2 projection takes a risky decision, by construction. In v1, trusting Jev at p ≥ 0.9 left a risky call in 6–13% of episodes.

2. **Narrower questions did not make Jev more accurate.** This compares the same decisions, scored the same way (a write counts as "hand back"), on the 300-per-domain v1 pilot sample:

   | Domain | Decisions | v1 question | v2 one question | v2 combined |
   |---|---|---|---|---|
   | Retail | 301 | 77.7% | 73.1% | 79.7% |
   | Airline | 293 | 73.4% | 73.4% | 75.1% |

3. **Combining helps a little; state predicates help more; dataflow hints don't help.** This is next-step agreement on the same answers, pooled over the source models (GLM-5 in brackets):

   | Arbiter inputs | Retail, one question | Retail, combined | Airline, one question | Airline, combined |
   |---|---|---|---|---|
   | Habit + Jev (v2) | 76.2% | 77.9% (81.7%) | 72.4% | 74.3% (69.5%) |
   | + dataflow hints (predicates asked, not used) | 75.7% | 77.6% (80.7%) | 73.3% | 74.5% (68.7%) |
   | + state predicates | 75.7% | **80.5%** (85.7%) | 73.3% | **78.9%** (75.8%) |

   - "Records still unchecked in a list" carries the most weight in the arbiter: 1.12 in retail and 1.15 in airline.
   - With predicates, the combined answer is at least 0.99 sure on 10.6% of retail decisions (99.2% agreement) and 25.2% of airline decisions (97.0%).

4. **At high confidence the savings stay small. Harmless detours buy more.** A run collapses only if every decision in it is taken. Pooled over the source models:

   | Rule | Retail: turns / $ saved | Retail detours | Airline: turns / $ saved | Airline detours |
   |---|---|---|---|---|
   | Combined, p ≥ 0.9 | 2.5% / 2.3% | 2.2% | 4.9% / 6.1% | 5.9% |
   | Combined, p ≥ 0.5 | 15.0% / 18.1% | 56.9% | 10.4% / 15.0% | 49.4% |
   | Lookup first, p ≥ 0.3 | 17.2% / 20.8% | 69.4% | 12.6% / 18.7% | 70.3% |
   | Sharpened, combined, p ≥ 0.9 | 4.4% / 6.4% | 3.6% | 5.9% / 8.5% | 5.3% |
   | Sharpened, lookup first, p ≥ 0.3 | 17.4% / 20.8% | 67.3% | 12.8% / 19.1% | 57.2% |

   The detour columns are the share of episodes with at least one detour. Dollars saved include the detours' cost.

5. **Sequential callers clear 20% with Jev; parallel callers don't.**

   Read-only flows, combined answers trusted at p ≥ 0.5 (v2, without sharpening), on the nine leaderboard models:

   | Agent model | Parallel tool turns | Retail: ceiling / saved (detour episodes) | Airline: ceiling / saved (detour episodes) |
   |---|---|---|---|
   | Qwen3.5 | 0% / 0% | 40.7% / **29.7%** (13%) | 40.7% / **22.5%** (45%) |
   | Qwen3-Max | 0% / 0% | 34.1% / **24.2%** (18%) | 33.2% / 19.9% (31%) |
   | Gemini 3 Flash | 8% / 17% | 36.8% / **24.1%** (12%) | 39.5% / 10.8% (49%) |
   | Gemini 3 Pro | 11% / 20% | 34.0% / **23.1%** (11%) | 27.6% / 10.6% (41%) |
   | Claude Sonnet 4.5 | 11% / 4% | 29.3% / **21.1%** (25%) | 30.8% / 15.2% (44%) |
   | GPT-5.2, reasoning off | 15% / 32% | 29.7% / 18.1% (22%) | 14.7% / 2.4% (32%) |
   | GLM-5 | 27% / 45% | 28.2% / 14.7% (12%) | 14.9% / 2.7% (36%) |
   | Claude Opus 4.5 | 28% / 41% | 27.3% / 14.7% (14%) | 11.9% / 2.8% (38%) |
   | GPT-5.2, reasoning high | 19% / 37% | 25.6% / 13.9% (29%) | 17.0% / 2.8% (39%) |

   With lookup first at p ≥ 0.3, Qwen3.5 reaches 34.1% in retail and 27.8% in airline, and Qwen3-Max reaches 28.4% and 26.3%, with detours in 37–75% of episodes.

6. **The gate, offline.** Read-only flows take no risky decisions, so offline the gate is turns saved. Whether detours cost pass^1 is for the live run. From the nine-target report's gate tables:
   - **Passes in both domains:**
     - Qwen3.5;
     - Qwen3-Max, which needs lookup first in airline;
     - the 2025 baseline Claude 3.7 Sonnet, which needs lookup first in airline.
   - **Passes in retail only:** Gemini 3 Flash, Gemini 3 Pro, Claude Sonnet 4.5, and GPT-5.2 with reasoning off (lookup first).
   - **GLM-5** passes in retail only with sharpening: 20.5% saved (lookup first, p ≥ 0.3), with detours in 32% of episodes. It fails in airline, where even its ceiling is 14.9%.
   - **Fails in both:** Claude Opus 4.5, GPT-5.2 with reasoning high, and the other 2025 baselines.

7. **Where the gap is.** Most lost savings are early hand-backs inside lookup chains. These counts cover all models, broken down by the agent's next step:
   - **Retail:** where the agent's next step was a product lookup, the combined answer matched it only 41% of the time and handed back in 58%.
   - **Airline:** where it was a flight search, the combined answer matched 57% and handed back in 36%.

   The dataflow hints targeted exactly these sites. In a 300-per-domain pilot on the same decisions:
   - retail product lookups went from 26% to 49%;
   - airline flight searches went from 62% to 71%;
   - retail order lookups fell from 71% to 65%.

   At full scale the one-question agreement didn't move (row 2 of finding 3). The predicates, which ask about the state rather than describe the tools, are what helped.

## Caveats

- **Agreement is a proxy.** It counts an equally valid next step as a disagreement.
- **Detours are costed only in tokens.** A detour's effect on the LLM's later behavior is not modeled.
- **The predicates saw held-out cases.** They were written after reading about a dozen held-out failure cases, as well as the training runs. They are few and generic, and the arbiter's weights are cross-validated, but a strict protocol would propose them from training episodes only.
- **The v1 comparison uses a sample.** The paired v1 comparison uses the 300-per-domain v1 pilot sample.

## Reproduce

[The CLI reference](../cli.md) lists every option.

```sh
gunzip -c docs/results/phase0b-v2-2026-09-23-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
stretto phase0 --tau2 ../tau2-bench $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle replay --questions v2
```

To reproduce the other runs:

- **Nine targets:** pass `$(scripts/fetch-leaderboard.sh all | sed 's/^/--target /')` instead.
- **Sharpened run:** add `--dataflow-hints --predicates data/predicates-v2.json`.
- **Ablation, predicates asked but not weighed:** also add `--no-predicate-features`.
- **Pilots:** `--questions v1 --oracle-limit 300`, or `--questions v2 --dataflow-hints --oracle-limit 300`.
