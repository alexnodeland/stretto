# Answer bundle, 2026-09-25: the confirmation judge, live

[answers-2026-09-25-judge-live.jsonl.gz](answers-2026-09-25-judge-live.jsonl.gz) holds 156 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 152,546 input tokens, under a cent at Jev's price. They are the two questions the proxy asked about each of the 78 writes it judged in [the judge's live run](judge-live-2026-09-25.md): did the customer confirm, and had the agent proposed the change.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's probability of a yes to each question, and token usage.
- **Excluded:** request text and credentials.

## Replay without a key

Import it with the other bundles:

```sh
for b in docs/results/*answers*.jsonl.gz; do
  gunzip -c $b | stretto import-answers --oracle-cache .oracle-cache
done
```

The proxy's confirmation logs in [the episodes archive](judge-live-2026-09-25-episodes.tar.gz) name each question's key (`key`, `second_key`), so every judgment can be looked up in the bundle.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
