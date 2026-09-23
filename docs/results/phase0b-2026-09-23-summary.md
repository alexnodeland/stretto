# Phase 0b results, 2026-09-23: summary

No code was changed for this run. The full generated report is in [phase0b-2026-09-23-report.md](phase0b-2026-09-23-report.md). It was produced by commit b85bd14 (`main` at dfb2ba7, which requires 10 distinct training tasks per validated context), replaying the cached Jev answers from the live run.

- Jev model: jev-1.13.0 (answered every question)
- Questions: retail 5812 distinct (5932 decisions) and airline 3134 distinct (3211 decisions), all answered, 0 failed
- Input tokens: retail 19,252,613 ($0.81) and airline 8,979,283 ($0.38), 28.2M in total ($1.19 at $0.042/MTok)
- Wall-clock: the live Jev run took 281 s, after a 35 s pilot of 200 questions per domain; the replay took 23 s

With the pick trusted at p ≥ 0.9, the pooled projection saves 3.9% of LLM turns in retail and 4.7% in airline. No agent model passes the gate in either domain.

## Retail

Next step:

| Agent model | Decisions | Agreed | Stop vs. go on | Brier | ECE | p ≥ 0.9: share / agreed |
|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 1211 | 71.3% | 77.6% | 0.406 | 0.053 | 28.7% / 91.6% |
| gpt-4.1 | 1173 | 74.0% | 78.7% | 0.356 | 0.041 | 30.4% / 96.4% |
| gpt-4.1-mini | 1252 | 67.7% | 70.9% | 0.471 | 0.105 | 34.6% / 90.5% |
| o4-mini | 1060 | 75.0% | 77.6% | 0.375 | 0.068 | 35.8% / 93.1% |
| glm-5 *(target)* | 1153 | 74.8% | 81.4% | 0.370 | 0.030 | 27.6% / 94.0% |
| **all source models** | 4696 | 71.8% | 76.1% | 0.404 | 0.063 | 32.3% / 92.8% |

Pooled projection:

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Risky decisions (/100 ep) | Episodes with one |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **14.6%** | 1.29 | 7.00 | 67.0 | 110.2 | 67.0% |
| p ≥ 0.7 | **11.5%** | 1.79 | 5.16 | 31.9 | 64.1 | 48.1% |
| p ≥ 0.9 | **3.9%** | 3.10 | 2.47 | 10.6 | 6.4 | 6.2% |

Gate (turns saved · episodes with a risky decision):

| Agent model | Perfect System-One | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | Gate |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 33.0% | 21.2% · 50.0% | 16.6% · 24.4% | 5.0% · 3.1% | fails |
| gpt-4.1 | 19.7% | 12.6% · 65.6% | 9.4% · 42.5% | 2.5% · 0.6% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 12.4% | 6.7% · 82.5% | 4.7% · 73.8% | 1.7% · 12.5% | fails: below 20% even with a perfect System-One model |
| o4-mini | 22.8% | 17.0% · 70.0% | 14.5% · 51.9% | 6.3% · 8.8% | fails |
| glm-5 *(target)* | 29.1% | 13.2% · 45.6% | 8.0% · 16.9% | 1.5% · 1.2% | fails |

## Airline

Next step:

| Agent model | Decisions | Agreed | Stop vs. go on | Brier | ECE | p ≥ 0.9: share / agreed |
|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 672 | 68.0% | 75.0% | 0.472 | 0.075 | 30.8% / 88.4% |
| gpt-4.1 | 643 | 72.3% | 75.4% | 0.402 | 0.049 | 34.7% / 92.4% |
| gpt-4.1-mini | 680 | 71.2% | 74.6% | 0.417 | 0.065 | 36.3% / 93.9% |
| o4-mini | 424 | 78.3% | 80.2% | 0.320 | 0.038 | 44.3% / 92.0% |
| glm-5 *(target)* | 645 | 64.5% | 77.2% | 0.524 | 0.128 | 27.6% / 87.1% |
| **all source models** | 2419 | 71.8% | 75.9% | 0.411 | 0.058 | 35.8% / 91.8% |

Pooled projection:

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Risky decisions (/100 ep) | Episodes with one |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **12.0%** | 2.10 | 6.94 | 104.4 | 75.3 | 50.9% |
| p ≥ 0.7 | **9.3%** | 2.64 | 5.04 | 52.5 | 33.4 | 29.4% |
| p ≥ 0.9 | **4.7%** | 3.63 | 2.76 | 14.1 | 13.8 | 13.1% |

Gate (turns saved · episodes with a risky decision):

| Agent model | Perfect System-One | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | Gate |
|---|---|---|---|---|---|
| claude-3-7-sonnet | 36.6% | 19.5% · 52.5% | 13.9% · 28.7% | 5.5% · 12.5% | fails |
| gpt-4.1 | 10.9% | 6.0% · 46.2% | 5.3% · 26.2% | 3.1% · 12.5% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 6.6% | 2.9% · 68.8% | 2.4% · 35.0% | 1.3% · 13.8% | fails: below 20% even with a perfect System-One model |
| o4-mini | 24.5% | 18.4% · 36.2% | 15.1% · 27.5% | 9.2% · 13.8% | fails |
| glm-5 *(target)* | 15.4% | 2.4% · 67.5% | 2.2% · 40.0% | 1.0% · 12.5% | fails: below 20% even with a perfect System-One model |
