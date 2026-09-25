# Claude models, live: two agents and a customer

Every live result so far had GLM-5.3 play both the agent and the simulated customer. Offline, flows carried over to nine agent models, and what they saved followed each model's calling style ([v2 targets](phase0b-v2-2026-09-23-targets-report.md)). This round puts Claude models in both seats, on small samples. Claude Haiku 4.5 and Claude Sonnet 5 play the agent ([#5](https://github.com/alexnodeland/stretto/issues/5)), and Claude Sonnet 5 plays the customer to GLM-5.3's agent ([#4](https://github.com/alexnodeland/stretto/issues/4)).

- **Harness.** `run_episode.py --agent-cli claude --model M` runs the agent in Claude Code on a Claude model, through [`claude-agent.sh`](../../pilot/claude-agent.sh). `--customer-cli claude --customer-model M` runs the customer the same way. The agent gets τ²-bench's system prompt and τ²-bench's tools over MCP, behind `stretto-proxy`, as GLM-5.3 did.
- **Agents:**
  - Claude Haiku 4.5, on the retail pilot's ten test tasks;
  - Claude Sonnet 5, on three of them drawn at random (seed 5): 51, 60 and 101.

  GLM-5.3 played the customer, as in the pilots.
- **Customer:** Claude Sonnet 5, with GLM-5.3 as the agent, on five of the ten drawn at random (seed 4): 18, 36, 51, 60 and 101. Each episode is paired with the retail pilot's episode of the same task and arm, where GLM-5.3 played the customer too.
- **Arms:** without a flow, and with D0, the pilot's flow, one episode per task and arm. D0 was compiled from four other agents' 2025 episodes, so no Claude session went into it.
- **Reward:** τ²-bench's database check.

## Findings

### Claude Haiku 4.5 as the agent

- **D0 saved 18.6% of its turns.** It took 79 LLM turns against 97 without a flow (95% interval 10.1% to 27.4%): fewer on seven tasks and more on none. Agent input tokens fell 13.3%, from 847,723 to 734,903 (−0.3% to 26.6%).
- **That is less than D0 saved GLM-5.3, as Haiku's calling style predicts.** On the same tasks, GLM-5.3 went from 110 turns to 84, 23.6% fewer ([the pilot](pilot-2026-09-24.md)). Haiku makes more calls at once, 1.37 per tool turn against GLM-5.3's 1.05, and a flow spares a turn only when it makes all of that turn's calls. Ten tasks cannot tell the two savings apart.
- **D0's lookups were nearly all Haiku's own.** It made 30 lookups, and Haiku made 28 of them itself in the same task without a flow. The other two were detours, both in task 27 (below). Haiku repeated 3 of the flow's lookups, all in task 36, where it read three products again after the flow had read them.
- **Passes: 7 of 10 with D0, 6 without.** Task 64 failed in both arms: Haiku chose the same wrong variant of the item that GLM-5.3 chose in both of the pilot's arms. The three other failures without a flow:
  - on task 18, Haiku exchanged a chair for the same item, where the task expects another;
  - on task 77, it transferred the customer to a human agent before making the exchange;
  - on task 101, it misread the order as already shipping to the customer's New York address, so it never changed the address.

  The two other failures with D0:
  - *Task 36.* Haiku read every product itself and priced the cheapest T-shirt at $46.85, where one at $46.66 was available. Its total came to $1,131.04, four cents over the customer's $1,131 limit, so the customer had it cancel the order. The task expects the cheaper items. Without a flow, Haiku found the $1,130.85 total and passed.
  - *Task 27.* The customer asked to return two items from a delivered order and exchange a third. The policy allows only one of the two per order, and the customer prefers the exchange. After Haiku read the boots, the flow read the other two products, its only detours. Haiku then returned the two items, and the exchange failed. Without a flow, Haiku had proposed both too, but caught the rule before acting. GLM-5.3 made the same error in both of the pilot's arms. Whether the flow's reads tipped Haiku, one episode cannot say.

### Claude Sonnet 5 as the agent

- **On three tasks, D0 cut its turns from 29 to 22, 24.1% fewer** (95% interval 0% to 44.4%). GLM-5.3 went from 35 to 26 on the same three.
- **All 7 lookups were Sonnet's own,** and it repeated none.
- **It passed 3 of 3 with D0 and 2 without.** On task 60 without a flow, it offered the customer one pair of blue earbuds, water-resistant like theirs. The task expects the other blue pair: the customer would have taken the one without water resistance had the agent offered both.

### Claude Sonnet 5 as the customer

- **The savings held.** With GLM-5.3 as the agent and Sonnet 5 as the customer, D0 saved 23.1% of turns on the five tasks: 50 against 65 without a flow (95% interval 13.8% to 32.1%). With GLM-5.3 as the customer on the same tasks, in the pilot, it saved 25.8%: 46 against 62 (16.4% to 38.6%).
- **The flow did the same.** It made 12 lookups with either customer, with the same tools and arguments on four of the five tasks. On task 51 it read a different one of the customer's orders. All 24 lookups were the agent's own, and the agent repeated 2 in each arm, both on task 101.
- **Nothing else moved much.** All 20 episodes passed. Sonnet wrote shorter messages than GLM-5.3, 15 words on average against 22 to 25, and about as many of them (65 against 59). Without a flow, the five episodes took 3 turns more than with GLM-5.3 as the customer.

So on these five tasks, a customer played by the agent's own model did not inflate what a flow saves.

| | Agent | Customer | Tasks | LLM turns, no flow | LLM turns, D0 | Fewer (95% interval) | Passed, no flow | Passed, D0 | Lookups | Detours |
|---|---|---|---|---|---|---|---|---|---|---|
| The retail pilot | GLM-5.3 | GLM-5.3 | 10 | 110 | 84 | 23.6% (14.3% to 32.8%) | 8 | 8 | 26 | 0 |
| Haiku as the agent | Claude Haiku 4.5 | GLM-5.3 | 10 | 97 | 79 | 18.6% (10.1% to 27.4%) | 6 | 7 | 30 | 2 |
| Sonnet as the agent | Claude Sonnet 5 | GLM-5.3 | 3 | 29 | 22 | 24.1% (0% to 44.4%) | 2 | 3 | 7 | 0 |
| The pilot, same five tasks | GLM-5.3 | GLM-5.3 | 5 | 62 | 46 | 25.8% (16.4% to 38.6%) | 5 | 5 | 12 | 0 |
| Sonnet as the customer | GLM-5.3 | Claude Sonnet 5 | 5 | 65 | 50 | 23.1% (13.8% to 32.1%) | 5 | 5 | 12 | 0 |

Detours are lookups the agent did not make in the same task's episode without a flow.

## Per task

LLM turns, and whether the episode passed (✗ failed). The customer is GLM-5.3 in every column but the last two.

| Task | GLM-5.3, no flow | GLM-5.3, D0 | Haiku, no flow | Haiku, D0 | Sonnet, no flow | Sonnet, D0 | GLM-5.3 with Sonnet as customer, no flow | The same, D0 |
|---|---|---|---|---|---|---|---|---|
| 17 | 6 | 7 | 9 | 6 | | | | |
| 18 | 11 | 9 | 9 ✗ | 8 | | | 12 | 10 |
| 27 | 14 ✗ | 8 ✗ | 9 | 7 ✗ | | | | |
| 36 | 16 | 11 | 12 | 12 ✗ | | | 16 | 11 |
| 51 | 10 | 5 | 8 | 6 | 9 | 5 | 10 | 6 |
| 60 | 6 | 5 | 7 | 7 | 6 ✗ | 6 | 7 | 7 |
| 64 | 11 ✗ | 9 ✗ | 9 ✗ | 7 ✗ | | | | |
| 68 | 9 | 8 | 9 | 5 | | | | |
| 77 | 8 | 6 | 8 ✗ | 8 | | | | |
| 101 | 19 | 16 | 17 ✗ | 13 | 14 | 11 | 20 | 16 |

## Caveats

- One episode per task and arm, and few tasks: ten for Haiku, five for the customer, three for Sonnet as the agent.
- Claude Code ran the Claude models in its normal mode and GLM-5.3 with `--bare`, so the Claude agents' context also held Claude Code's short note: the working directory, the model's name and the date ([the pilot harness](../../pilot/README.md)).
- The Claude agents had GLM-5.3 as their customer, so they are not paired with the pilot's episodes on the customer's side. Only the D0 and no-flow arms of each agent are paired.

## Published

- [claude-models-2026-09-25.json](claude-models-2026-09-25.json): every arm's numbers, per task.
- [claude-models-2026-09-25-episodes.tar.gz](claude-models-2026-09-25-episodes.tar.gz): the 36 episodes, with the proxy's session and flow logs: `haiku-agent/`, `sonnet-agent/` and `sonnet-customer/`, each with `baseline/` and `flows/`.
- [The answer bundle](answers-2026-09-25-claude.md): the 126 Jev answers D0 read in these runs.

**Cost.** Claude tokens count input, cache reads, cache writes and output, as Claude Code reported them:

- Haiku as the agent: 1,649,698 in 20 episodes, 88% of them cache reads. Claude Code priced them at $2.21 at API rates.
- Sonnet as the agent: 502,568 in 6 episodes, 91% cache reads, priced at $0.91.
- Sonnet as the customer: 110,016 in 10 episodes. Each reply is a fresh call, so most of them are cache writes.

A smoke episode took 59,147 more. On Z.ai's side, GLM-5.3 cost 65.7 credits as the Claude agents' customer and 111.8 as the agent to Sonnet's customer.

## Reproduce

[The CLI reference](../cli.md) lists every option. Each episode ran as one of:

```sh
cd pilot
python run_episode.py --domain retail --task-id 36 --arm flows --agent-cli claude --model claude-haiku-4-5-20251001 \
  --oracle-cache ../.oracle-cache --out runs/haiku-agent
python run_episode.py --domain retail --task-id 36 --arm flows --customer-cli claude --customer-model claude-sonnet-5 \
  --oracle-cache ../.oracle-cache --out runs/sonnet-customer
```

with `--arm baseline` for the arm without a flow. They need `ZAI_API_KEY` for GLM-5.3's side and a Claude Code login for the Claude side. The arm with D0 also needs `TYPESAFE_API_KEY`, since a new episode asks Jev new questions. The bundle holds the answers these episodes read.

The comparison, from the archives:

```sh
mkdir -p /tmp/episodes && tar -xzf docs/results/claude-models-2026-09-25-episodes.tar.gz -C /tmp/episodes
E=/tmp/episodes/claude-models-2026-09-25-episodes
python analyze_paired.py arms --tasks 17 18 27 36 51 60 64 68 77 101 \
  --arm "Haiku 4.5, no flow=$E/haiku-agent/baseline" --arm "Haiku 4.5, D0=$E/haiku-agent/flows"
python analyze_paired.py arms --tasks 18 36 51 60 101 \
  --arm "Sonnet customer, no flow=$E/sonnet-customer/baseline" --arm "Sonnet customer, D0=$E/sonnet-customer/flows"
```
