# Answer bundle, 2026-09-26: stretto's flow in telecom

[answers-2026-09-26-telecom-flows.jsonl.gz](answers-2026-09-26-telecom-flows.jsonl.gz) holds 910 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 1.99M input tokens, about $0.08 at Jev's price. They are every answer that [the telecom flow page's](telecom-flows-2026-09-26.md) arbiter replay read on GLM-5's telecom test episodes that no earlier bundle holds. The arbiter itself was fitted on [the telecom bundle](answers-2026-09-25-telecom.md).

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

```sh
for b in docs/results/answers-2026-09-25-telecom docs/results/answers-2026-09-26-telecom-flows; do
  gunzip -c $b.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
```

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
