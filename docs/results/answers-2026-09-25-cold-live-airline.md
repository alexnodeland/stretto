# Answer bundle, 2026-09-25: the cold start, live, in airline

[answers-2026-09-25-cold-live-airline.jsonl.gz](answers-2026-09-25-cold-live-airline.jsonl.gz) holds 75 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 177,278 input tokens, under a cent at Jev's price. They are the questions the arbiter flow of [the airline cold start](cold-start-live-airline-2026-09-25.md) asked, live and in its offline check, that no earlier bundle held. The habit alone asks nothing.

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

A fresh cache with only the published bundles reproduces both rows of the offline check on [the results page](cold-start-live-airline-2026-09-25.md#reproduce).

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
