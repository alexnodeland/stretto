# Answer bundle, 2026-09-26: what is left of the detours

[answers-2026-09-26-detours.jsonl.gz](answers-2026-09-26-detours.jsonl.gz) holds 615 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 1.57M input tokens, about $0.07 at Jev's price. They are every answer [the detours page](detours-2026-09-26.md) read: D0's arbiter, replayed on Claude Sonnet 4.5's 80 airline test-task episodes, which no earlier bundle held.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

```sh
gunzip -c docs/results/answers-2026-09-26-detours.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
```

Then replay D0 with its arbiter as the page says, with `--flow-oracle replay`: every question the replay asks is in the bundle.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
