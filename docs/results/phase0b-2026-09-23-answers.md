# Phase 0b answer bundle, 2026-09-23 (v1)

[phase0b-2026-09-23-answers.jsonl.gz](phase0b-2026-09-23-answers.jsonl.gz) holds 8,946 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. They answer every question of the first Phase 0b run (`--questions v1`, GLM-5 as the target): 8,736 next-step questions and 210 closed-set arguments.

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by. These are tool names and, for the 210 argument questions, closed-set values from τ²-bench's synthetic database: airports, dates, amounts, flight numbers and payment ids.
- **Excluded:** request text and credentials.

## Re-asked, so it reproduces a re-run

The answers from the first run on 2026-09-23 were not kept. For this bundle, the same questions were asked again:

- 600 answers are from the 2026-09-23 pilot, which asked 300 questions per domain.
- The other 8,346 were asked again on 2026-09-24, of the same model version, for $1.11.

Jev does not answer identically every time. So the bundle reproduces [phase0b-2026-09-23-reasked-report.md](phase0b-2026-09-23-reasked-report.md) (with [its aggregates](phase0b-2026-09-23-reasked-aggregates.json)), made by the same commit as the [original report](phase0b-2026-09-23-report.md). The two reports differ only where the answers differ:

| | Original | Re-asked |
|---|---|---|
| Retail: next step agreed, all source models | 71.8% | 72.1% |
| Airline: next step agreed, all source models | 71.8% | 71.8% |
| Retail: turns saved, pick trusted at p ≥ 0.9 | 3.9% | 4.0% |
| Airline: turns saved, pick trusted at p ≥ 0.9 | 4.7% | 4.6% |
| Gate verdicts | none passes | the same |

39 of the report's 487 lines change. The largest changes are 6 to 9 points on cells with few decisions, such as `book_reservation`'s arguments and one model's share of episodes with a risky decision. The [summary](phase0b-2026-09-23-summary.md) quotes the original report.

## Replay without a key

Import the answers, then run the commit that made both reports:

```sh
gunzip -c docs/results/phase0b-2026-09-23-answers.jsonl.gz | cargo run --release -p stretto-report -- import-answers --oracle-cache .oracle-cache
git checkout fb10755
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench \
  $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') --oracle replay
```

Current `main` also replays `--questions v1` from this bundle without a key. It asks 3,120 airline questions instead of 3,134, so its figures differ slightly from both reports.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
