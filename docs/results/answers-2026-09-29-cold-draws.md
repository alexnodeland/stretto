# Answer bundle, 2026-09-29: the cold start's draws

[answers-2026-09-29-cold-draws.jsonl.gz](answers-2026-09-29-cold-draws.jsonl.gz) holds 39,541 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 103.8M input tokens, $4.36 at Jev's price. They are every answer [the cold start's draws](cold-draws-2026-09-29.md) asked that no earlier bundle holds: the held-out questions each draw's own arbiter was fitted on, and the decisions of the arbiter and shipped-arbiter flows in the replays.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by: tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import every bundle in `docs/results`, this one with them:

```sh
for b in docs/results/answers-*.jsonl.gz docs/results/phase0b-*-answers.jsonl.gz; do
  gunzip -c $b | stretto import-answers --oracle-cache .oracle-cache
done
```

A fresh cache holding them all reproduces every row on [the results page](cold-draws-2026-09-29.md#reproduce).

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
