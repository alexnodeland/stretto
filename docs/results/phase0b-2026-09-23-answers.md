# Phase 0b answer bundle, 2026-09-23

`phase0b-2026-09-23-answers.jsonl.gz` holds jev-1.13.0's answers to the 8,946 distinct Phase 0b questions (5,812 retail and 3,134 airline). Each line is `{"key", "response"}`: the key is the SHA-256 of the request, and the response is Jev's picks and probabilities, the model version and token usage. The bundle holds no request text and no benchmark transcripts.

## Replaying it

```bash
gunzip -c docs/results/phase0b-2026-09-23-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
stretto phase0 --tau2 ../tau2-bench $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle replay --out reports/phase0b.md --json reports/phase0b.json
```

This needs no API key. It reproduces [the report](phase0b-2026-09-23-report.md), provided the questions are still asked the same way (any change to the request format changes the keys).

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
