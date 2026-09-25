# Answer bundle, 2026-09-25: flow search

[answers-2026-09-25-search.jsonl.gz](answers-2026-09-25-search.jsonl.gz) holds 8,788 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 26.0M input tokens, about $1.09 at Jev's price. They are the questions [the flow search](search-2026-09-25.md) asked that no earlier bundle held:

- **Before the search,** D0 replayed on GLM-5's training-task episodes at every threshold from 0.1 to 0.7. The searches read these answers and asked nothing.
- **After it,** the hand-set settings, the three searches' fronts and the site-by-site flows replayed on GLM-5's test-task episodes, with the one-threshold baseline there. Each takes paths no earlier replay did.
- **On other agents,** D0, the picks and the arbiter at 0.4 and 0.5 replayed on Claude Sonnet 4.5's and Qwen3.5's test-task episodes: 4,055 of the answers.

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

A fresh cache with only the published bundles holds every answer this round read: the same 128,744 as the cache it ran on. From it, airline's search gives the same front, setting for setting. Every replay on [the results page](search-2026-09-25.md#reproduce) that was checked gives the same totals: the hand-set settings and the fronts on both sets of episodes, and the replays on other agents.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
