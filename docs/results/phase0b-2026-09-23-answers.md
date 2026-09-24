# Phase 0b answer bundle, 2026-09-23 (v1)

[phase0b-2026-09-23-answers.jsonl.gz](phase0b-2026-09-23-answers.jsonl.gz) holds the answers from the first Phase 0b run: jev-1.13.0's answers to its 8,946 distinct questions (`--questions v1`, GLM-5 as the target), 5,812 in retail and 3,134 in airline. That is 8,736 next-step questions and 210 closed-set arguments. There is one JSON object per line, `{"key", "response"}`:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities, model version and token usage.
- **What else is in it:** the options the probabilities are keyed by. These are tool names and, for the 210 argument questions, closed-set values from τ²-bench's synthetic database: airports, dates, amounts, flight numbers and payment ids.
- **Excluded:** request text and credentials.

## Replay without a key

Import the answers, then run the commit that made [the report](phase0b-2026-09-23-report.md):

```sh
gunzip -c docs/results/phase0b-2026-09-23-answers.jsonl.gz | cargo run --release -p stretto-report -- import-answers --oracle-cache .oracle-cache
git checkout fb10755
cargo run --release -p stretto-report -- phase0 --tau2 ../tau2-bench \
  $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') --oracle replay
```

This reproduces the report exactly, line for line. Current `main` also replays `--questions v1` from the bundle without a key. It asks 3,120 airline questions instead of 3,134, so its airline figures differ slightly.

## Asked twice

While the original answers were thought lost, the same 8,946 questions were asked again of the same model version: 8,346 on 2026-09-24 ($1.11), and 600 in the 2026-09-23 pilot, which asked them apart from the first run. That second set is kept as a determinism check, in [phase0b-2026-09-23-reasked-answers.jsonl.gz](phase0b-2026-09-23-reasked-answers.jsonl.gz). It replays at fb10755 to [phase0b-2026-09-23-reasked-report.md](phase0b-2026-09-23-reasked-report.md), with [its aggregates](phase0b-2026-09-23-reasked-aggregates.json).

Jev does not answer identically twice:

- It chose the same option in 97.0% of the 8,946 answers.
- In each answer, the option whose probability moved most moved by 0.02 at the median, 0.06 at the 90th percentile and 0.12 at the 99th. The largest move anywhere was 0.38.

The report's headline figures barely move:

| | First run | Asked again |
|---|---|---|
| Retail: next step agreed, all source models | 71.8% | 72.1% |
| Airline: next step agreed, all source models | 71.8% | 71.8% |
| Retail: turns saved, pick trusted at p ≥ 0.9 | 3.9% | 4.0% |
| Airline: turns saved, pick trusted at p ≥ 0.9 | 4.7% | 4.6% |
| Gate verdicts | none passes | the same |

39 of the report's 487 lines change. The largest changes are 6 to 9 points, on cells with few decisions, such as `book_reservation`'s arguments and one model's share of episodes with a risky decision. A replay cache is what makes a Phase 0b result reproducible: re-asking gives a nearby result, not the same one.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
