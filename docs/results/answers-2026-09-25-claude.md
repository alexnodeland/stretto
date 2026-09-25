# Answer bundle, 2026-09-25: Claude models, live

[answers-2026-09-25-claude.jsonl.gz](answers-2026-09-25-claude.jsonl.gz) holds 126 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 359,530 input tokens, under two cents at Jev's price. They are the questions D0 asked in [the Claude models' live runs](claude-models-2026-09-25.md): with Claude Haiku 4.5 and Claude Sonnet 5 as the agent, and with GLM-5.3 as the agent to Claude Sonnet 5's customer. No earlier bundle held any of them, since a different agent or customer makes different conversations and so different questions.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import every bundle in `docs/results/`:

```sh
for b in docs/results/*answers*.jsonl.gz; do
  gunzip -c $b | stretto import-answers --oracle-cache .oracle-cache
done
```

The flow logs in [the episodes archive](claude-models-2026-09-25-episodes.tar.gz) name each decision's key, so every answer the flow acted on can be looked up in the bundle.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
