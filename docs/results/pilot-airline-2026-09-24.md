# Live pilot, 2026-09-24: GLM-5.3 in airline, with and without a flow

The second paired live run of stretto's read-only flow (RFC-001's arm D0), set up exactly as [the retail pilot](pilot-2026-09-24.md): ten airline test-split tasks, drawn at random (seed 7, `pilot/run_pilot.py --domain airline`), one episode in each arm.

- **Agent:** GLM-5.3 in Claude Code, on Z.ai's GLM Coding Plan endpoint, with τ²-bench's system prompt and no built-in tools.
- **Tools:** τ²-bench's, over MCP, behind `stretto-proxy`.
- **Customer:** GLM-5.3, from τ²-bench's user-simulator prompt.
- **Flow:** compiled goal-free from the 2025 baselines' airline episodes. It takes the likeliest lookup when the tool's probability times the binding's agreement is at least 0.3.
- **Reward:** τ²-bench's database check. The natural-language assertions need an LLM judge and were left out. *Rescored (2026-09-25):* τ²-bench's own airline reward is the database check times the communication check, and it needs no judge. Scored that way, every episode keeps its reward. Task 18 is the only airline test task with a figure the agent must tell the customer, and the agent told it in both arms (`pilot/rescore.py --scoring basis` on [the published episodes](episodes-2026-09-24.md)).

| Task | Turns without | Turns with | Flow lookups | Repeated by the agent | Input tokens without | Input tokens with | Credits without | Credits with | Without | With |
|---|---|---|---|---|---|---|---|---|---|---|
| 6 | 13 | 7 | 1 | 0 | 104,973 | 42,231 | 27.85 | 12.58 | failed | passed |
| 8 | 8 | 7 | 5 | 0 | 55,742 | 48,374 | 10.99 | 9.68 | passed | passed |
| 16 | 8 | 8 | 2 | 0 | 56,954 | 56,592 | 13.19 | 10.59 | passed | passed |
| 18 | 28 | 23 | 4 | 0 | 304,056 | 256,215 | 39.81 | 32.97 | failed | passed |
| 24 | 22 | 18 | 1 | 0 | 161,999 | 138,374 | 23.93 | 23.81 | passed | passed |
| 25 | 9 | 8 | 1 | 0 | 62,638 | 55,681 | 11.54 | 11.05 | passed | passed |
| 26 | 9 | 6 | 2 | 0 | 55,785 | 36,303 | 15.39 | 10.76 | passed | passed |
| 30 | 8 | 8 | 1 | 0 | 47,244 | 50,082 | 8.38 | 11.77 | passed | passed |
| 31 | 7 | 8 | 3 | 0 | 39,408 | 68,479 | 7.29 | 16.44 | passed | failed |
| 37 | 15 | 12 | 4 | 0 | 107,781 | 97,036 | 18.04 | 17.95 | passed | passed |

## Findings

- **Turns:** 127 LLM turns without the flow and 105 with it, 17.3% fewer. That is 2.2 fewer per episode (95% interval 0.5 to 3.9; paired t = −2.96, p = 0.016), with fewer turns in 7 of 10 pairs, one more in 1, and the same in 2 (sign test p = 0.07).
- **Tokens:** agent input tokens fell 14.8%, from 997k to 849k.
- **Calls:** the agent made 60 calls of its own, against 84 without the flow. The flow made 24 lookups, and the agent repeated none of them.
- **Where the flow acted:** on all 10 tasks.
- **The customer's side:** 59 customer turns without the flow and 57 with it.
- **Outcome:** 8 of 10 passed the database check without the flow and 9 of 10 with it. None of the three failures came from a flow decision, since flows only read:
  - without the flow, the agent booked a flight on task 6, which expects no write, and added a paid bag nobody asked for on task 18;
  - with the flow, on task 31 the customer would pay under $100 for a change. The agent upgraded the basic-economy reservation to economy (a $301 charge), then changed its flights (a $220 refund), and quoted the net, $81. The task expects no change: it counts the upgrade's cost against the limit. The upgrade path itself is allowed, since τ²-bench's own task 32 expects it, and no guard checks a price the customer named. The flow's three reservation lookups on that task were the agent's own.
  - *Correction:* an earlier version of this page said the upgrade path breaks the policy, and a guard was changed to refuse it. Task 32 shows it does not, so that change was reverted.
- **The agent calls tools one at a time.** GLM-5.3 in Claude Code made parallel calls in 3.8% of its tool turns here (3.0% in retail). GLM-5 in τ²-bench's own harness, on Sierra's leaderboard, did in 45% of airline tool turns, which is why the offline projection for GLM-5 in airline was under 5% (`phase0b-v2-2026-09-23-summary.md`). The live agent differs from that run in both model version and harness. What counts for flows is how the agent in front of them calls tools, so a deployment should measure that first.
- **Cost:** 334.0 Z.ai credits: 176.4 without the flow and 157.6 with it, 11% less.

Ten pairs show that the mechanism works live in a second domain and give a first effect size. They cannot bound a one-point loss of pass^1, which needs a much larger run.

`pilot-airline-2026-09-24.json` holds the per-task numbers. The episodes themselves (conversations, proxy logs and flow answers) are in [pilot-airline-2026-09-24-episodes.tar.gz](pilot-airline-2026-09-24-episodes.tar.gz); see [the episodes page](episodes-2026-09-24.md).
