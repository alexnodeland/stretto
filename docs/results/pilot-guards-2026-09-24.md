# Live guards pilot, 2026-09-24: GLM-5.3 in airline, with and without the policy guards

Does refusing policy-breaking writes raise the pass rate? The guards arm (RFC-001's arm B) runs the agent's tools behind `stretto-proxy --guards --domain airline`. Every call is checked before the tool server sees it, and a call an enforced rule refuses never runs. The agent gets an error result that says why. Nothing else changes: with nothing refused, the proxy forwards every line byte for byte.

- **Agent and customer:** GLM-5.3 in Claude Code, as in the flows pilots.
- **Tasks:** the guards can only matter where agents make the writes they refuse. So instead of a random sample, the pilot takes the four airline test tasks where the guard audit of τ²-bench's published trajectories (`guards-2026-09-24.md`) finds most failed episodes with a write an enforced rule refuses: 35, 45, 32 and 48. It adds four more as a harm check: 24, 26, 31 and 37, whose baselines passed in the airline flows pilot and are reused here.
- **Reward:** τ²-bench's database check.

| Task | What the customer wants | Published failed episodes a guard refuses | Baseline (LLM turns) | Guards (LLM turns) | Writes the guards checked | Refused |
|---|---|---|---|---|---|---|
| 35 | cancel a flight the policy does not allow cancelling, book another | 14 of 16 | passed (19) | passed (18) | book_reservation | 0 |
| 45 | cancel as soon as possible (not allowed) | 9 of 9 | passed (7) | passed (9) | none | 0 |
| 32 | change to a nonstop on the same day (upgrade the basic-economy cabin first) | 7 of 16 | passed (15) | passed (15) | update_reservation_flights ×2 | 0 |
| 48 | cancel a flight booked this morning by mistake (not allowed) | 5 of 9 | passed (17) | passed (7) | none | 0 |
| 24 | remove a passenger; book a new flight (harm check) | 7 of 14 | passed (22) | passed (19) | book_reservation | 0 |
| 26 | cancel a trip (harm check) | 7 of 7 | passed (9) | passed (9) | none | 0 |
| 31 | change a basic-economy flight only if it costs under $100 (harm check) | 2 of 2 | passed (7) | failed (14) | update_reservation_flights ×2 | 0 |
| 37 | cancel two reservations, upgrade a third (harm check) | 9 of 14 | passed (15) | passed (15) | update_reservation_flights, transfer_to_human_agents | 0 |

## Findings

- **GLM-5.3 does not make the writes the guards refuse.** On the four tasks where the 2025 agents' failures concentrate, it passed all four in both arms, and no rule refused anything. Tasks 45 and 48 ask for cancellations the policy does not allow. In 9 and 5 of the 2025 agents' failed episodes there, a guard would have refused a write. GLM-5.3 declined to cancel on its own, in both arms, and made no write at all.
- **No harm seen.** Over 8 episodes with the guards on, the proxy checked 9 writes and refused none. Every one was the agent's own legitimate call. That includes task 32's solution, which upgrades a basic-economy reservation and then changes its flights. Earlier the same day, a rule had briefly been changed to refuse that path. It was reverted before this pilot, and this episode shows why that mattered: the task expects exactly that path.
- **The one failure is not the guards'.** Task 31 failed with the guards on, as it did with the flow in the airline pilot, and passed in the baseline. It is the same agent error both times: the agent upgraded the cabin ($301), changed the flight ($220 back), and quoted a net $81 against the customer's $100 limit, while the task counts the upgrade's cost. Nothing was refused, so the agent saw exactly what it would have without the guards.
- **So guards are insurance whose value depends on the agent.** Against the 2025 baselines they would have refused a policy-breaking write in 38% of failed airline episodes. Against GLM-5.3, on the tasks where those episodes concentrate, they had nothing to refuse. They cost nothing when they do not fire. They should matter most for weaker or cheaper agents, which is where the RFC's plan (flows compiled from frontier traces, run by cheaper models) points.
- **Cost:** 223 Z.ai credits at the off-peak rate for the 12 episodes run today (the harm check reuses four baselines). That is under the 350 approved. A second trial per task was planned, but at about 28 credits per episode on these long tasks it would have cost about 500 in total, so it was left out.

Eight pairs cannot bound a pass-rate effect. They answer the pilot's question for this agent: on these tasks, GLM-5.3 gives the guards nothing to catch.

`pilot-guards-2026-09-24.json` holds the per-episode numbers. The episodes themselves, with the conversation each proxy was given, are in [pilot-guards-2026-09-24-episodes.tar.gz](pilot-guards-2026-09-24-episodes.tar.gz); see [the episodes page](episodes-2026-09-24.md).
