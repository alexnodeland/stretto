# Answer bundle, 2026-09-25: prompt injection

[answers-2026-09-25-injection.jsonl.gz](answers-2026-09-25-injection.jsonl.gz) holds 2,300 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 5.2M input tokens, $0.22 at Jev's price. They are the questions [the injection round](injection-2026-09-25.md) changed: flow questions with a note in the latest tool result, confirmation questions with a note in the agent's message, and both with the mitigation's sentence. The clean questions they are compared with are in earlier bundles.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import every published bundle, then this one:

```sh
for b in docs/results/phase0b-v2-2026-09-23-answers docs/results/phase0b-v2-goal-free-2026-09-24-answers \
         docs/results/answers-2026-09-24-arms-sweep-confirm docs/results/answers-2026-09-24-cold-manifest-match \
         docs/results/answers-2026-09-24-arbiter-transfer docs/results/answers-2026-09-25-injection; do
  gunzip -c $b.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
```

[The results page](injection-2026-09-25.md#reproduce) gives the commands that read them.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
