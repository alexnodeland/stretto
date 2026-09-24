# Answer bundle, 2026-09-24: shipped arbiters across domains

[answers-2026-09-24-arbiter-transfer.jsonl.gz](answers-2026-09-24-arbiter-transfer.jsonl.gz) holds 718 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 1.8M input tokens, $0.08 at Jev's price. They are every answer the replays of [the shipped-arbiter test](arbiter-transfer-2026-09-24.md) read that no earlier bundle holds. Fitting the two shipped arbiters asked nothing new.

What each line holds, as in the earlier bundles:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage. The options are tool names and handing back (`respond`).
- **Excluded:** request text and credentials.

## Replay without a key

Import every published bundle, then this one:

```sh
for b in docs/results/phase0b-v2-2026-09-23-answers docs/results/phase0b-v2-goal-free-2026-09-24-answers \
         docs/results/answers-2026-09-24-arms-sweep-confirm docs/results/answers-2026-09-24-cold-manifest-match \
         docs/results/answers-2026-09-24-arbiter-transfer; do
  gunzip -c $b.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
```

A cache built from these five bundles alone holds every answer the replays read, and replays a flow to the same totals.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
