# Frontier models and a prompting baseline, live: the plan

This page was written, committed and pushed before any episode of the design below ran. Two plumbing episodes ran first, in a scratch directory, and are not part of the results. The results page will link here and list every deviation from this plan.

## Questions

1. **Does a flow save LLM turns on current Claude models?** Every live τ²-bench result so far with the reach decider had GLM-5.3 as the agent. Offline, what a flow saves follows each model's calling style, and Claude models make more calls at once than GLM-5.3 does (Claude Haiku 4.5 made 1.37 calls per tool turn in [the Claude round](claude-models-2026-09-25.md), GLM-5.3 1.05).
2. **Does asking the agent to batch its reads get what a flow gets?** Anthropic's prompting guide gives a sample prompt that raises parallel tool calling to "~100%". If an agent already makes every independent read at once when told to, a flow may add nothing. A flow's lookups take their arguments from results the agent has not yet seen, which no prompt can batch, so we expect it still saves turns on top of the prompt.

## Design

- **Tasks.** The 28 tasks of [the live reach round](reach-2026-09-26.md#live), the same for every arm and model: retail 5, 12, 17, 27, 32, 33, 38, 40, 42, 60, 61, 62, 64, 68, 71, 74, 77, 79, 97 and 108; airline 6, 16, 19, 22, 30, 31, 32 and 48.
- **Agents.** Claude Sonnet 5 (`claude-sonnet-5`) and Claude Haiku 4.5 (`claude-haiku-4-5-20251001`), each in Claude Code through [`claude-agent.sh`](../../pilot/claude-agent.sh), on the user's Claude Code subscription. The agent gets τ²-bench's system prompt and τ²-bench's tools over MCP, behind `stretto-proxy`, as in every earlier live round.
- **Customer.** Claude Haiku 4.5 as τ²-bench's user simulator in every episode, so the customer does not change with the agent.
- **Arms.**
  - `baseline`: no flow.
  - `reach`: the flows the reach round served ([retail](reach-2026-09-26-retail.flow.json), [airline](reach-2026-09-26-airline.flow.json)), learned from τ²-bench's four 2025 runs, with the reach decider at 0.3. No model is asked, and no Claude session went into the flows.
  - `batch`: no flow, and the system prompt ends with the sample prompt for maximum parallel efficiency from Anthropic's prompting guide ("Optimize parallel tool calling"), word for word (`BATCH_READS` in [`run_episode.py`](../../pilot/run_episode.py)).
  - `batch-reach`: both.
- **Which arms.** Claude Sonnet 5 runs `baseline` and `reach`; Claude Haiku 4.5 runs all four. Sonnet 5 runs `batch` and `batch-reach` too if the budget allows once the rest are done, after them rather than interleaved with its other arms.
- **Trials.** Three per task, arm and model: 168 episodes for Sonnet 5's two arms, 336 for Haiku 4.5's four.
- **Order.** Trial by trial. Within a trial the tasks run in a shuffled order (seed 27), and each task's arms back to back in a shuffled order, a few episodes at a time ([`run_trials.py`](../../pilot/run_trials.py)). So a stop leaves whole trials of every arm, and the arms of a task run minutes apart. The two models may run at the same time.
- **Scoring.** τ²-bench's database check, as in every earlier live round.
- **GLM-5.3.** The prompting baseline on the model of every earlier live reach result: the `batch` arm with GLM-5.3 as agent and customer (`glm-claude.sh`, as in the recorded arms), on the same 28 tasks, one trial, within 400 Z.ai credits. It is compared with the reach round's recorded `baseline` and `reach` arms on the same tasks, as that round compared its `reach` arm with the recorded baseline. Those were recorded one and two days earlier, so these comparisons are not concurrent, and the results page says so.

## Budget

The subscription reports two windows in the agent's stream: the share of its five-hour and of its seven-day allowance used. At the start, they stood at 0.07 and 0.66. The runner:

- stops for good once the seven-day window reaches 0.85, leaving the rest of the week's allowance to the user;
- waits for the five-hour window to reset once it reaches 0.80;
- stops if the subscription reports anything but `allowed`, or starts drawing on overage (which is off for this account).

The runs have an order of priority: Sonnet 5's two arms, then Haiku 4.5's four, then Sonnet 5's `batch` arms. When two run at once, the lower one stops earlier, at 0.82 of the seven-day window, so the higher one gets what is left. If the budget runs out, the results report the trials that finished, and say so.

## Outcomes

Reported by [`analyze_trials.py`](../../pilot/analyze_trials.py), for each model, in each domain and in both:

- **Primary: LLM turns**, the agent's model requests in an episode. For each pair of arms, the change in total turns over the episodes of the same task and trial, with a 95% interval from a bootstrap over tasks (4,000 draws, a task's trials drawn together, tasks drawn within each domain), and an exact sign test on each task's mean change.
- **Secondary:**
  - the agent's input tokens, cached or not;
  - the agent's cost at list prices (Claude Code's `total_cost_usd`; the subscription bills no tokens), and the customer's;
  - the episode's wall-clock time;
  - passes: pass^1 with its interval and an exact McNemar test, and pass^3 (τ²-bench's pass^k, the chance that k trials of a task all pass);
  - the flow's lookups: the agent's own (the agent made the same call in the paired episode without a flow), detours, and lookups the agent made again.

The comparisons, each on the same task and trial:

| Comparison | Question | Expected |
|---|---|---|
| baseline → reach | Does a flow save turns on this model? | Fewer turns (both models) |
| baseline → batch | Does the prompt save turns? | No prediction |
| batch → batch-reach | Does a flow save turns on top of the prompt? | Fewer turns |
| reach → batch-reach | Does the prompt add to a flow? | No prediction |

## What would count

- A saving is established when the 95% interval of the change in turns lies wholly below zero.
- A harm to passes is established when the 95% interval of the change in pass^1 lies wholly below zero. With 84 pairs per comparison and model, the interval will be about ±10 points wide, so smaller harms cannot be seen. Pass^3 is reported and not tested.
- GLM-5.3's single `batch` trial is reported against its recorded arms with the same statistics, and read as a smaller, non-concurrent check.
- An episode that fails on the harness or the API (the agent's process dies, or a turn ends in an API error) is run once more, and both attempts are kept and counted. No other episode is dropped.
