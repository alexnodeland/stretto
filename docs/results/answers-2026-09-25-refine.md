# Answer bundle, 2026-09-25: predicate refinement

[answers-2026-09-25-refine.jsonl.gz](answers-2026-09-25-refine.jsonl.gz) holds 20,600 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 36.0M input tokens, about $1.51 at Jev's price. They are the eight candidate predicates of [the refinement round](refine-2026-09-25.md), each asked alone at every asked next-step decision in airline. That covers 2,019 distinct states of the four 2025 baselines and 556 of GLM-5's. No earlier bundle held any of them.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's probability of a yes to the one question asked (`pred_<id>`), and token usage.
- **Excluded:** request text and credentials.

## Replay without a key

Import every bundle in `docs/results/`:

```sh
for b in docs/results/*answers*.jsonl.gz; do
  gunzip -c $b | stretto import-answers --oracle-cache .oracle-cache
done
```

A fresh cache with only the published bundles reproduces every table on [the results page](refine-2026-09-25.md#reproduce).

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
