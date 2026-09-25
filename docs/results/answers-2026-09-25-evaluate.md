# Answer bundle, 2026-09-25: counterfactual evaluation

[answers-2026-09-25-evaluate.jsonl.gz](answers-2026-09-25-evaluate.jsonl.gz) holds 1,265 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 4.3M input tokens, about $0.19 at Jev's price. They are the questions that [the counterfactual evaluation's](evaluate-2026-09-25.md) replays asked and no earlier bundle held. Most come from D0 exploring, which reaches states no earlier replay did. The rest are from the replays of D0 and of the arbiter at 0.5 on all four trials of GLM-5's test episodes.

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

A fresh cache with only the published bundles reproduces both ε = 0.1 exploring replays on [the results page](evaluate-2026-09-25.md#reproduce), decision for decision.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
