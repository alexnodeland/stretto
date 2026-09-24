# Answer bundle, 2026-09-24: arms, fewer traces, confirmations

[answers-2026-09-24-arms-sweep-confirm.jsonl.gz](answers-2026-09-24-arms-sweep-confirm.jsonl.gz) holds 15,393 answers from TypeSafe's Jev (jev-1.13.0), one JSON object per line: `{"key", "response"}`. That is 32.3M input tokens, $1.36 at Jev's price. They are every answer that three runs read and no earlier bundle holds:

- **The arms replays** ([results](arms-2026-09-24.md)): 593 answers ($0.08), new questions from D0's replays of GLM-5's and Claude 3.7 Sonnet's test episodes. Those replays also read 256 answers ($0.04) asked earlier the same day, by the replay checks behind the live pilots. No bundle held them until now.
- **The fewer-traces sweep** ([results](sweep-2026-09-24.md)): 10,681 answers ($1.12). Fewer training tasks leave a flow fewer sites and lookups, and a site's lookups are part of its questions, so each share of the training tasks needs answers of its own. The arbiter's replays ask more.
- **The confirmation judge** ([results](confirm-2026-09-24.md)): 3,863 answers ($0.12), one yes/no question per write.

What each line holds:

- **Key:** the SHA-256 of the exact request.
- **Response:** Jev's picks, probabilities and token usage.
- **What else is in it:** the options the probabilities are keyed by. These are tool names and handing back (`respond`), except in 112 answers to argument questions, whose options are closed-set values from τ²-bench's synthetic database: order ids, zip codes, dates and airports.
- **Excluded:** request text and credentials.

## Replay without a key

Import this bundle together with the v2 bundle and its goal-free supplement:

```sh
for b in phase0b-v2-2026-09-23 phase0b-v2-goal-free-2026-09-24; do
  gunzip -c docs/results/$b-answers.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
done
gunzip -c docs/results/answers-2026-09-24-arms-sweep-confirm.jsonl.gz | stretto import-answers --oracle-cache .oracle-cache
```

Then every run above replays from the cache:

- `stretto confirm --tau2 ../tau2-bench --oracle replay --oracle-cache .oracle-cache` reproduces the confirmation run. It needs only this bundle.
- `stretto compile --oracle replay --train-fraction F` compiles each flow of the sweep.
- `pilot/check_flow.py --flow-oracle replay` replays D0 and the sweep's arbiters.

The results pages give the exact commands.

## Terms

Jev's answers are provided only to reproduce and audit stretto's analysis. Under TypeSafe's Master Customer Agreement (§2.3(b)) they may not be used for model distillation, to train a model to imitate TypeSafe's services, or to develop a similar or competing product.
