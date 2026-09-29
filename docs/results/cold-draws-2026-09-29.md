# The cold start on three more agents, with twenty draws

[The cold start round](cold-start-2026-09-24.md) found that a deployment should start on the habit alone and fit its own arbiter only after about twenty sessions. It rested on one agent, GLM-5, a heavy parallel caller in τ²-bench's harness. Its draws were five sessions each, four random and one clustered. From the random ones the habit alone saved 11.8–18.9% of GLM-5's retail turns, and from the clustered one 2.7%, so how often a start is that narrow was unknown ([#9](https://github.com/alexnodeland/stretto/issues/9)).

This round repeats the cold start on three agents of τ²-bench's current leaderboard, with twenty random draws of 5 sessions and twenty of 10 per agent and domain. It also adds a third way to start: the habit with a shipped arbiter.

## Findings

- **From five sessions, about one start in eight is narrow.** In 16 of the 120 five-session draws, the habit alone saved less than half of what it saves from every training task: 9 of 60 in airline and 7 of 60 in retail. At ten sessions, 1 of 120 did.
  - Claude Sonnet 4.5's retail is the hardest start. Six of its twenty five-session draws were narrow, and its median draw saved 14.5% of turns, against 25.1% from every task.
  - For the other agents and domains, the median five-session draw came within 0.2 to 3.1 points of what every task gives.
- **A shipped arbiter lifted most narrow starts.** On the same sessions, the habit with a shipped arbiter saved more than half in 13 of the 17 narrow draws.
  - It was below half in 4 of 120 five-session draws and none of the ten-session ones, each time on a draw where the habit alone was narrow too.
  - Its median five-session draw saved 0.5 to 2.7 points less than the habit alone's in airline and in Claude Opus 4.5's retail. It saved 7.7 and 3.3 points more in Claude Sonnet 4.5's and Qwen3.5-397B's retail.
  - Paired by draw and episode, it made fewer detours in 11 of the 12 comparisons, significantly in 6.
- **An arbiter fitted on the draw's own sessions was the worst start.** It was below half in 27 of 120 five-session draws and in 6 of 120 ten-session ones, all six Claude Opus 4.5's airline.
  - The 27 include a Claude Sonnet 4.5 retail draw from which no arbiter flow could be learned at all: none of the sessions left to train its habit had succeeded.
  - It lifted 6 of the 17 narrow draws above half. Its median draw made the fewest detours of the three flows in 11 of the 12 agent, domain and size groups.
- **GLM-5's clustered draw was a start like these.** Twenty random draws per agent put a number on it: about one five-session start in eight saves under half, with no clustering needed. Ten sessions nearly remove such starts for every flow but the draw's own arbiter.
- **What this changes.**
  - Where a System-One model is available, start on the habit with a shipped arbiter. It gave up at most 3.3 points of the median and removed most narrow starts. Live, from five of GLM-5.3's sessions, it took fewer turns than the habit alone in retail and cut its detours in airline ([retail](cold-start-live-2026-09-25.md), [airline](cold-start-live-airline-2026-09-25.md)).
  - Without one, start on the habit alone, and expect about one start in eight to save under half until ten sessions in.
  - Either way, fit the deployment's own arbiter later, as the cold start round found.

## Setup

- **The agents.** Three of τ²-bench's current leaderboard submissions, four trials each with GPT-5.2 as the user: Qwen3.5-397B, the strongest sequential caller offline; Claude Sonnet 4.5; and Claude Opus 4.5, a heavy parallel caller.
- **The draws.** For each agent and domain, twenty random sets of 5 training tasks and twenty of 10, drawn by `scripts/cold_draws.py` with a seed per agent, domain, size and draw. Each set's trial-0 episodes are a deployment's first sessions, one per task. Retail has 74 training tasks and airline 30.
- **Three flows from each draw,** all learned with `stretto learn --results --train-tasks`:
  - *the habit alone* (`--habit-only`), which asks no model;
  - *the draw's own arbiter*, fitted on Jev's answers at the draw's held-out 30%, as `learn` does by default;
  - *the habit with a shipped arbiter* (`--arbiter-from data/arbiters/<domain>.json`), which asks Jev at each decision, as the arbiter flow does.
- **The reference.** The habit alone learned from every training task.
- **The replays.** Each flow replayed the agent's test-task episodes, all four trials (160 in retail, 80 in airline), lookup first at 0.3 (`pilot/check_flow.py --in-process`), the habit alone on the habit decider and the others on the arbiter. That is the cold start round's test, so its GLM-5 numbers compare.
- **Paired differences.** The shipped arbiter against the habit alone is paired by draw and episode, averaged over draws, with a 95% interval from resampling test tasks.
- **Cost.** Jev answered 39,541 questions no earlier bundle holds: 103.8M input tokens, about $4.36 at Jev's price. The 726 learns and replays took four and a half hours on four cores. No LLM ran.

## By agent and domain

Turns saved on the agent's test episodes, as a share of its turns: the median draw, with the range over the twenty draws in brackets. The last column counts the draws that saved less than half of what the habit from every training task saves (the *Every task* column), out of twenty.

| Agent | Domain | Sessions | Every task | Habit alone | Its own arbiter | A shipped arbiter | Below half: habit, own, shipped |
|---|---|---|---|---|---|---|---|
| Qwen3.5-397B | Retail | 5 | 32.1% | 29.0% (14.1–33.3) | 27.5% (17.2–35.5) | 32.3% (21.9–33.8) | 1, 0, 0 |
| Qwen3.5-397B | Retail | 10 | 32.1% | 31.3% (20.0–32.8) | 31.9% (27.6–34.9) | 33.0% (30.6–34.0) | 0, 0, 0 |
| Claude Sonnet 4.5 | Retail | 5 | 25.1% | 14.5% (1.0–24.0) | 14.1% (0.0–22.8) | 22.2% (4.4–25.5) | 6, 8, 3 |
| Claude Sonnet 4.5 | Retail | 10 | 25.1% | 23.7% (13.9–25.4) | 21.6% (16.8–26.3) | 23.7% (22.3–25.0) | 0, 0, 0 |
| Claude Opus 4.5 | Retail | 5 | 21.8% | 20.4% (16.9–21.9) | 18.6% (8.0–22.2) | 19.9% (17.2–21.7) | 0, 2, 0 |
| Claude Opus 4.5 | Retail | 10 | 21.8% | 20.4% (18.5–21.8) | 20.0% (11.4–22.6) | 20.0% (19.2–22.2) | 0, 0, 0 |
| Qwen3.5-397B | Airline | 5 | 24.2% | 22.9% (0.0–23.7) | 19.2% (0.0–22.4) | 20.2% (5.3–21.1) | 2, 3, 1 |
| Qwen3.5-397B | Airline | 10 | 24.2% | 23.4% (22.8–24.2) | 20.1% (12.6–22.8) | 20.1% (18.7–21.2) | 0, 0, 0 |
| Claude Sonnet 4.5 | Airline | 5 | 12.7% | 12.4% (1.9–12.7) | 10.5% (0.0–12.1) | 11.9% (11.1–12.3) | 3, 5, 0 |
| Claude Sonnet 4.5 | Airline | 10 | 12.7% | 12.7% (4.8–12.8) | 11.1% (10.3–12.5) | 11.9% (11.3–12.8) | 1, 0, 0 |
| Claude Opus 4.5 | Airline | 5 | 7.5% | 7.3% (0.0–7.7) | 4.6% (0.0–7.5) | 6.5% (5.5–6.7) | 4, 9, 0 |
| Claude Opus 4.5 | Airline | 10 | 7.5% | 7.3% (5.4–7.7) | 5.4% (0.0–7.2) | 6.4% (5.5–6.8) | 0, 6, 0 |

For Claude Sonnet 4.5's retail at five sessions, the draw from which no arbiter flow could be learned counts as saving nothing.

## The shipped arbiter against the habit alone

Turns saved and detours over the agent's test episodes (160 in retail, 80 in airline). The shipped arbiter's flow minus the habit alone's on the same draw, averaged over the twenty draws, with a 95% interval from resampling test tasks. The *Every task* column is the habit from every training task.

| Agent | Domain | Sessions | Every task: turns saved, detours | Shipped arbiter minus habit alone: turns saved (95%) | Detours (95%) |
|---|---|---|---|---|---|
| Qwen3.5-397B | Retail | 5 | 549, 78 | +54.5 (+27.5 to +80.0) | −40.1 (−70.2 to −14.8) |
| Qwen3.5-397B | Retail | 10 | 549, 78 | +42.0 (+15.6 to +68.8) | −46.7 (−83.3 to −12.1) |
| Claude Sonnet 4.5 | Retail | 5 | 411, 87 | +67.0 (+48.4 to +85.4) | −4.0 (−21.0 to +12.6) |
| Claude Sonnet 4.5 | Retail | 10 | 411, 87 | +16.3 (−6.0 to +36.8) | −48.7 (−71.9 to −27.9) |
| Claude Opus 4.5 | Retail | 5 | 306, 113 | −8.6 (−32.8 to +14.8) | −42.7 (−66.3 to −19.3) |
| Claude Opus 4.5 | Retail | 10 | 306, 113 | −3.4 (−27.6 to +20.6) | −8.2 (−38.1 to +17.6) |
| Qwen3.5-397B | Airline | 5 | 261, 46 | −11.2 (−33.8 to +9.9) | −8.4 (−23.9 to +9.4) |
| Qwen3.5-397B | Airline | 10 | 261, 46 | −35.0 (−61.8 to −11.5) | −8.1 (−23.9 to +9.3) |
| Claude Sonnet 4.5 | Airline | 5 | 119, 51 | +7.1 (−6.3 to +16.8) | −21.8 (−43.5 to −1.7) |
| Claude Sonnet 4.5 | Airline | 10 | 119, 51 | +0.1 (−14.6 to +10.3) | −7.0 (−23.3 to +10.6) |
| Claude Opus 4.5 | Airline | 5 | 46, 0 | +3.5 (−4.4 to +9.4) | −20.1 (−39.0 to −1.6) |
| Claude Opus 4.5 | Airline | 10 | 46, 0 | −6.0 (−15.2 to +0.2) | +1.1 (−11.3 to +16.7) |

## Limits

- **Replays assume the agent acts the same with the flow's results in hand.** Live, the cold start ran with GLM-5.3 on the pilots' ten tasks per domain. From five sessions, the habit with a shipped arbiter took 70 retail turns against the habit alone's 79 (110 without a flow). In airline both took 102 (127 without), and the shipped arbiter cut the habit's detours from 8 to 1.
- **One user simulator, one harness.** The three agents' published runs share τ²-bench's harness, with GPT-5.2 as the user. Savings follow an agent's calling style, which the harness shapes.
- **A shipped arbiter asks a model at each decision.** The habit alone asks nothing. The shipped arbiter's safer start costs a System-One model's answers at serving time, as the arbiter flow's does.

## Reproduce

In τ²-bench's Python environment, with its checkout beside this repository, its leaderboard submissions in `R`, and a replay cache holding every published answer bundle:

```sh
for b in docs/results/answers-*.jsonl.gz docs/results/phase0b-*-answers.jsonl.gz; do
  gunzip -c $b | stretto import-answers --oracle-cache .oracle-cache
done
python scripts/cold_draws.py run $R/qwen3.5-397b-a17b_enabled_retail_gpt-5.2_4trials.json \
  $R/claude-sonnet-4-5_enabled_retail_gpt-5.2_4trials.json $R/claude-opus-4-5_high_retail_gpt-5.2_4trials.json \
  --domain retail --sizes 5 10 --draws 20 --every --out cold --oracle-cache .oracle-cache --jobs 12
python scripts/cold_draws.py report cold
```

Airline is the same with its results files. With every bundle imported, including [this round's](answers-2026-09-29-cold-draws.md), the run asks Jev nothing. Every row of this round, with each draw's tasks and each replay's episodes, is in [cold-draws-2026-09-29.jsonl.gz](cold-draws-2026-09-29.jsonl.gz). One row's error is the message a rerun gave: the run kept only the end of the backtrace.
