# Answer bundle, 2026-09-25: promotion

[answers-2026-09-25-promotion.jsonl.gz](answers-2026-09-25-promotion.jsonl.gz) holds 19 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 77,138 input tokens, less than a cent at Jev's price. They are the questions [the promotion round](promotion-2026-09-25.md) asked that no earlier round had. A promoted flow hands back where the unpromoted one looked something up, so its later decisions follow paths no earlier replay took. Every other answer the round read is in earlier bundles.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import these bundles, then this one:

```sh
for b in docs/results/phase0b-v2-2026-09-23-answers docs/results/phase0b-v2-goal-free-2026-09-24-answers \
         docs/results/answers-2026-09-24-arms-sweep-confirm docs/results/answers-2026-09-25-promotion; do
  gunzip -c $b.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
```

These four hold every answer the round read: a fresh cache with only them reproduces every number on [the results page](promotion-2026-09-25.md#reproduce), which gives the commands.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
