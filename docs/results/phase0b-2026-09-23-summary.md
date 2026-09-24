# Phase 0b results, 2026-09-23: summary

No code was changed for this run. The full generated report is in [phase0b-2026-09-23-report.md](phase0b-2026-09-23-report.md), and aggregate metrics are in [phase0b-2026-09-23-aggregates.json](phase0b-2026-09-23-aggregates.json). Both were produced by commit fb10755 (latest `main` merged in, which adds the two-key rule), replaying the cached Jev answers from the live run.

[The v1 answer bundle](phase0b-2026-09-23-answers.md) holds the live run's answers, and replays this report exactly without a key. The same questions asked again gave the same pick 97% of the time, and a [re-run of this report](phase0b-2026-09-23-reasked-report.md) whose headline figures move by at most 0.3 points, with every gate verdict the same.

- Jev model: jev-1.13.0 (answered every question)
- Questions: retail 5812 distinct (5932 decisions) and airline 3134 distinct (3211 decisions), all answered, 0 failed
- Input tokens: retail 19,252,613 ($0.81) and airline 8,979,283 ($0.38), 28.2M in total ($1.19 at $0.042/MTok)
- Wall-clock: the live Jev run took 281 s, after a 35 s pilot of 200 questions per domain; replays take about 25 s

With the pick trusted at p ≥ 0.9, the pooled projection saves 3.9% of LLM turns in retail and 4.7% in airline (3.0% and 2.9% with two keys). No agent model passes the gate in either domain.

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

Closed-set arguments by argument (source models):

| Tool | Argument | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|---|
| `cancel_pending_order` | `reason` | 14 | 100.0% | 92.9% / 100.0% |
| `modify_pending_order_address` | `state` | 38 | 100.0% | 100.0% / 100.0% |
| `modify_user_address` | `state` | 13 | 92.3% | 92.3% / 100.0% |

Pooled projection (two keys: the pick is trusted only when it is also the habit's top option):

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Risky decisions (/100 ep) | Episodes with one |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **14.6%** | 1.29 | 7.00 | 67.0 | 110.2 | 67.0% |
| p ≥ 0.7 | **11.5%** | 1.79 | 5.16 | 31.9 | 64.1 | 48.1% |
| p ≥ 0.9 | **3.9%** | 3.10 | 2.47 | 10.6 | 6.4 | 6.2% |
| two keys, p ≥ 0.5 | **11.3%** | 1.94 | 4.58 | 24.1 | 49.7 | 44.1% |
| two keys, p ≥ 0.7 | **9.1%** | 2.31 | 3.67 | 13.4 | 38.4 | 35.6% |
| two keys, p ≥ 0.9 | **3.0%** | 3.35 | 1.80 | 3.8 | 4.4 | 4.4% |

Gate (turns saved · episodes with a risky decision):

| Agent model | Perfect System-One | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | two keys, p ≥ 0.5 | two keys, p ≥ 0.7 | two keys, p ≥ 0.9 | Gate |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 33.0% | 21.2% · 50.0% | 16.6% · 24.4% | 5.0% · 3.1% | 16.5% · 11.9% | 13.2% · 8.8% | 3.5% · 0.6% | fails |
| gpt-4.1 | 19.7% | 12.6% · 65.6% | 9.4% · 42.5% | 2.5% · 0.6% | 10.8% · 36.9% | 8.5% · 26.2% | 2.3% · 0.0% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 12.4% | 6.7% · 82.5% | 4.7% · 73.8% | 1.7% · 12.5% | 4.7% · 68.8% | 3.4% · 60.6% | 1.2% · 9.4% | fails: below 20% even with a perfect System-One model |
| o4-mini | 22.8% | 17.0% · 70.0% | 14.5% · 51.9% | 6.3% · 8.8% | 12.6% · 58.8% | 11.0% · 46.9% | 4.7% · 7.5% | fails |
| glm-5 *(target)* | 29.1% | 13.2% · 45.6% | 8.0% · 16.9% | 1.5% · 1.2% | 9.7% · 6.2% | 6.2% · 3.1% | 1.2% · 0.0% | fails |

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

Closed-set arguments by argument (source models):

| Tool | Argument | Decisions | Agreed | p ≥ 0.9: share / agreed |
|---|---|---|---|---|
| `book_reservation` | `flight_type` | 3 | 0.0% | 100.0% / 0.0% |
| `book_reservation` | `flights` | 6 | 0.0% | 0.0% / 0.0% |
| `book_reservation` | `insurance` | 11 | 100.0% | 100.0% / 100.0% |
| `book_reservation` | `nonfree_baggages` | 11 | 100.0% | 90.9% / 100.0% |
| `book_reservation` | `payment_methods` | 7 | 0.0% | 14.3% / 0.0% |
| `book_reservation` | `total_baggages` | 11 | 45.5% | 100.0% / 45.5% |
| `search_onestop_flight` | `date` | 19 | 100.0% | 100.0% / 100.0% |
| `search_onestop_flight` | `destination` | 2 | 0.0% | 0.0% / 0.0% |
| `search_onestop_flight` | `origin` | 1 | 0.0% | 100.0% / 0.0% |
| `send_certificate` | `amount` | 3 | 100.0% | 33.3% / 100.0% |
| `update_reservation_baggages` | `nonfree_baggages` | 21 | 71.4% | 85.7% / 83.3% |
| `update_reservation_baggages` | `payment_id` | 1 | 0.0% | 0.0% / 0.0% |
| `update_reservation_baggages` | `total_baggages` | 21 | 33.3% | 100.0% / 33.3% |
| `update_reservation_flights` | `payment_id` | 5 | 0.0% | 100.0% / 0.0% |

Pooled projection (two keys: the pick is trusted only when it is also the habit's top option):

| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Risky decisions (/100 ep) | Episodes with one |
|---|---|---|---|---|---|---|
| p ≥ 0.5 | **12.0%** | 2.10 | 6.94 | 104.4 | 75.3 | 50.9% |
| p ≥ 0.7 | **9.3%** | 2.64 | 5.04 | 52.5 | 33.4 | 29.4% |
| p ≥ 0.9 | **4.7%** | 3.63 | 2.76 | 14.1 | 13.8 | 13.1% |
| two keys, p ≥ 0.5 | **6.1%** | 3.35 | 3.40 | 52.5 | 19.1 | 17.8% |
| two keys, p ≥ 0.7 | **4.9%** | 3.59 | 2.70 | 29.4 | 12.5 | 12.5% |
| two keys, p ≥ 0.9 | **2.9%** | 3.96 | 1.70 | 7.8 | 8.4 | 8.4% |

Gate (turns saved · episodes with a risky decision):

| Agent model | Perfect System-One | p ≥ 0.5 | p ≥ 0.7 | p ≥ 0.9 | two keys, p ≥ 0.5 | two keys, p ≥ 0.7 | two keys, p ≥ 0.9 | Gate |
|---|---|---|---|---|---|---|---|---|
| claude-3-7-sonnet | 36.6% | 19.5% · 52.5% | 13.9% · 28.7% | 5.5% · 12.5% | 9.5% · 11.2% | 6.7% · 10.0% | 3.4% · 5.0% | fails |
| gpt-4.1 | 10.9% | 6.0% · 46.2% | 5.3% · 26.2% | 3.1% · 12.5% | 3.4% · 16.2% | 3.3% · 10.0% | 1.8% · 8.8% | fails: below 20% even with a perfect System-One model |
| gpt-4.1-mini | 6.6% | 2.9% · 68.8% | 2.4% · 35.0% | 1.3% · 13.8% | 1.6% · 25.0% | 1.4% · 16.2% | 0.7% · 12.5% | fails: below 20% even with a perfect System-One model |
| o4-mini | 24.5% | 18.4% · 36.2% | 15.1% · 27.5% | 9.2% · 13.8% | 9.3% · 18.8% | 8.1% · 13.8% | 5.6% · 7.5% | fails |
| glm-5 *(target)* | 15.4% | 2.4% · 67.5% | 2.2% · 40.0% | 1.0% · 12.5% | 0.5% · 3.8% | 0.5% · 3.8% | 0.2% · 1.2% | fails: below 20% even with a perfect System-One model |
