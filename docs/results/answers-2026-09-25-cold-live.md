# Answer bundle, 2026-09-25: the cold start, live

[answers-2026-09-25-cold-live.jsonl.gz](answers-2026-09-25-cold-live.jsonl.gz) holds 56 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 136,138 input tokens, under a cent at Jev's price. They are the questions the arbiter flow of [the live cold start](cold-start-live-2026-09-25.md) asked live that no earlier round had. The offline check's questions were all in earlier bundles.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import these bundles:

```sh
for b in docs/results/answers-2026-09-24-arbiter-transfer docs/results/answers-2026-09-24-arms-sweep-confirm \
         docs/results/answers-2026-09-24-cold-manifest-match docs/results/answers-2026-09-25-cold-live; do
  gunzip -c $b.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
```

These four hold every answer the round read. A fresh cache with only them reproduces both offline checks on [the results page](cold-start-live-2026-09-25.md#reproduce).

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
