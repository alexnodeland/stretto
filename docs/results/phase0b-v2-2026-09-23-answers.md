# Phase 0b answer bundle, 2026-09-23 (v2)

[phase0b-v2-2026-09-23-answers.jsonl.gz](phase0b-v2-2026-09-23-answers.jsonl.gz) holds 9,303 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`.

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **Excluded:** no request text, benchmark data or credentials.

It covers three sets of questions:

- **v2:** the Phase 0b v2 questions (`--questions v2`), 8,109 distinct;
- **v2 hints pilot:** the v2 questions with dataflow hints (`--questions v2 --dataflow-hints --oracle-limit 300`);
- **v1 pilot:** the v1 questions (`--questions v1 --oracle-limit 300`).

The two pilots have 300 per domain each. A few hinted questions are identical to unhinted ones.

## Replay without a key

```sh
gunzip -c docs/results/phase0b-v2-2026-09-23-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
stretto phase0 --tau2 ../tau2-bench $(scripts/fetch-leaderboard.sh glm-5 | sed 's/^/--target /') \
  --oracle replay --questions v2
```

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
