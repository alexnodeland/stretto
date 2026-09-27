# What the environment decides, on seven benchmarks

Every number before this round came from τ²-bench. Its three domains were built for coverage, and its users are LLMs. This round takes the flow to six more benchmarks, whose authors published their agents' trajectories: τ-bench, the Berkeley Function Calling Leaderboard's multi-turn tasks, AgentDojo, WorkBench, DTap-Bench and MCPMark. That is 89 more agents, from GPT-3.5 to models released this year, and 21 new domains. Seven of the agents are built on three agent SDKs: the Claude Agent SDK, the OpenAI Agents SDK and Google's ADK. MCPMark's tasks run on real MCP servers: a filesystem, PostgreSQL, GitHub and Notion. In five of the benchmarks no simulated user speaks: the requests were written by the benchmarks' authors. None of these benchmarks can be re-run cheaply, so the replays answer the flow's lookups from the record itself. [The working paper](../../paper/stretto.md) states the method; this page holds the runs.

- **Replaying from the record.** A lookup the agent's own later call answers, before its next write, is answered by that call's recorded result, which is exact. Any other lookup is a detour whatever it returned, so it gets a stand-in and never counts as a saving. Checked against τ²-bench's environment on the paper's nine agents, this counts 96.7% of the turns the environment replay saves, and keeps 91% of the reach decider's lead over the next-step decider. It finds 55–72% of the detours, so its detours are lower bounds.
- **The ceiling belongs to the domain.** The read-only ceiling is 3.5% of turns in WorkBench, whose requests name every entity. It is 7.0% on MCPMark's real servers, 13.2% in BFCL, 22.1% in AgentDojo (47.1% in its travel suite), 3.8–34.3% in DTap-Bench's six domains, 24–30% in τ-bench and 29.0% in τ²-bench. Where the user's words carry the arguments, a flow that binds from results has little to take. Reading the request would add 3–7 points in BFCL, AgentDojo, WorkBench and DTap-Bench, against 0.7 in τ²-bench; a model that picks the value among the user's words takes 30–53% of those points, a pattern 0–17%.
- **Reach saves more where agents read ahead of their next write, and ties elsewhere.**
  - **τ-bench.** Flows learned from τ²-bench's 2025 runs save 22.9% of retail turns for GPT-4o and Claude 3.5 Sonnet, recorded in τ-bench's own harness a year earlier: 76% of the ceiling. Reach is +1.4 points ahead of the next-step decider (0.8–2.1); the two tie in airline.
  - **BFCL.** Learned from eight older models and replayed on ten late-2025 ones, reach saves twice what next-step does at every threshold: 134 turns against 62 at θ = 0.3, +0.85 points of all turns (0.35–1.45).
  - **AgentDojo.** The two tie (−0.5 points, −1.7 to 0.0). In Slack they make the same lookups and take 78% of the ceiling. In travel, whose only write comes at the end, reach saves a little less than next-step, from a flow of 38 sessions.
  - **DTap-Bench.** Over six domains, reach saves more than next-step whichever agents it learned from: +1.3 points with each agent's own runs (1.1–1.5), +0.9 with the other harnesses' agents (0.7–1.1), with more detours. Medical, half the turns, has nothing to take; over the other five domains, +2.6 (2.2–3.0) and +1.7 (1.4–2.1).
  - **MCPMark.** On real MCP servers, where agents write their own SQL and choose which files, issues and pages to read, both deciders save 0.2% of turns at 0.3, with next to no detours, and 0.4% with each model's own flows. On Notion the agents walk a page's blocks by the ids each listing returns: the flows learn the walk but not which block comes next.
  - **WorkBench.** Neither decider saves a turn or makes a detour. The ceiling is 3.5%, its reads take a name or a date the request gave, and once a search the agent always narrows is never made bare, the flows make no lookup at all in 13,869 turns, at 0.3 or at 0.1.
- **Flows carry across harnesses.** Learned from the other harnesses' agents, a flow keeps 59% of what the agent's own saves, with 64% of its detours; learned from its harness-mates, 79%. Each SDK runs one vendor's models here, so harness and model family go together: the GPT-5 models keep 90% from each other, and Claude Opus 4.6 keeps as much from the other harnesses (62%) as from Claude Sonnet 4.5 in its own (61%). Agents that repeat their own reads would repeat a lookup's too, and six of the seven repeated 22 of 21,524 reads.
- **A threshold per decision is not free.** Pricing each lookup's detour and saved turn where it stands, rather than the domain's averages, changes τ²-bench's counted utility by −15% in retail, +7% in airline and +10% in telecom. It raises the threshold early in an episode, and retail's product lookups there are scored 0.33 and used 64% of the time.
- **Three things new benchmarks taught the binder.**
  - **Read results as data.** AgentDojo prints results as YAML and Python literals. Read as JSON, the form an MCP server returns, its ceiling doubles, from 10.0% to 22.1% of turns.
  - **Some lookups take lists.** Travel's price lookup takes every hotel the city's listing gave. A binding now passes a whole list: `bindings.lists`.
  - **A search the agent always narrows is never made bare.** WorkBench's searches each take one of several optional filters, so none is required, and the flows searched with none: 289 detours for one saved turn. A lookup without required arguments is now only as likely as the agent's own calls with none: `bindings.bare`.

  Both are in flow format 2.

## Replaying from the record

`pilot/check_flow.py --trace` replays a recorded episode as before, with the flow serving lookups after each of the agent's calls. Every call is answered from the recorded trajectory instead of an environment (`pilot/trace_env.py`):

- The agent's own call returns what it returned.
- A lookup returns what the agent's own later call with the same arguments returned, if the agent made that call before its next write. With no write in between, a read returns the same result, so that answer is exact, and it is the only kind the replay rule counts as a saving.
- Any other lookup is a detour whatever it returned. The record cannot say what that was, so the lookup returns the nearest result of the same tool, which gives the flow's next decisions a result of the right shape. It is marked, and never counts as a saving, even if the agent later makes the same call after a write and gets the same result.

Other parties' writes, such as the customer's on their own phone in telecom, do not stop a lookup, since the record cannot say whether they changed what the agent reads. In τ²-bench's telecom they seldom do. Stopping at them cut telecom's savings from the environment's 164 turns to 141, for one agent.

On the paper's batch — nine agents in three domains plus two in solo telecom, with both deciders at θ = 0.3 — the trace replay compares with the environment replay as follows:

| Decider | Domain | Saved, trace vs environment | Detours, trace vs environment | Episodes with the same saved turns |
|---|---|---|---|---|
| reach | retail | 4,069 vs 4,173 (−2.5%) | 459 vs 721 (0.64×) | 93.1% |
| reach | airline | 1,034 vs 1,034 | 243 vs 251 (0.97×) | 100% |
| reach | telecom | 2,234 vs 2,255 (−0.9%) | 289 vs 956 (0.30×) | 98.5% |
| reach | solo telecom | 858 vs 1,011 (−15.1%) | 372 vs 543 (0.69×) | 56.9% |
| reach | all | 8,195 vs 8,473 (−3.3%) | 1,363 vs 2,471 (0.55×) | 93.4% |
| next-step | all | 7,528 vs 7,741 (−2.8%) | 1,193 vs 1,661 (0.72×) | 95.0% |

The trace replay counts fewer savings than the environment replay. After a write, the environment replay still credits a lookup whose result the write left unchanged, which the record cannot show. That matters most in solo telecom, where the agent writes to the phone every few turns. It also finds fewer detours: a detour's real result starts chains of further lookups that a stand-in does not. The comparison between deciders survives. Reach saves 667 more turns than next-step in the trace replay and 732 more in the environment, 91% of it, so a trace replay's detours are lower bounds, and reach's more so than next-step's.

## The benchmarks

Each benchmark's published runs are rewritten as τ²-bench results, with a checkout-shaped folder that lists the tools, each marked a read or a write, and a train/test split by task: 40% of tasks held out, the same share of every ten in order. `stretto learn --results` and the replay read them unchanged. A flow is learned from other agents than the ones it is replayed on, older ones in every benchmark but τ-bench and DTap-Bench. DTap-Bench's agents are contemporaries, so each agent's own training runs are one of three sources compared.

| Benchmark | Converter | Domains | Learned from | Replayed on | Test episodes | Turns |
|---|---|---|---|---|---|---|
| τ-bench (v1) historical trajectories | `taubench_v1_to_tau2.py` | retail, airline | τ²-bench's four 2025 runs (the paper's flows) | GPT-4o, Claude 3.5 Sonnet (new), in τ-bench's own harness | 480 | 6,244 |
| BFCL v4 multi-turn, base | `bfcl_to_tau2.py --runs` | 8 APIs in one: files, messaging, social, tickets, trading, travel, a car, math | 8 models: GPT-4.1, GPT-4.1 mini, o3, o4-mini, Mistral Large, Llama 3.3 70B, Qwen3 235B, Gemini 2.5 Flash | 10: Claude Opus, Sonnet and Haiku 4.5, GPT-5.2, GPT-5 mini, Gemini 3 Pro, Grok 4.1 Fast, Kimi K2, GLM-4.6, DeepSeek V3.2 | 799 | 8,510 |
| AgentDojo (benign runs) | `agentdojo_to_tau2.py` | workspace, Slack, banking, travel | 11 released before May 2024 | 10 released after | 410 | 1,523 |
| WorkBench (2026 re-run) | `workbench_to_tau2.py` | email, calendar, analytics, CRM, project management, multi-domain | 10: GPT-3.5 to GPT-5.2, o3, GLM-4.6, Claude Haiku 4.5 | 14, among them Claude Opus 4.8, GPT-5.5, Gemini 3.1 Pro and 3.5 Flash, Kimi K2.6, DeepSeek V4 Pro | 3,858 | 13,869 |
| MCPMark v1 (run 1) | `mcpmark_to_tau2.py --learn-from-all` | filesystem, PostgreSQL, GitHub, Notion (real MCP servers) | 8: GPT-4.1, GPT-4.1 mini, o4-mini, Gemini 2.5 Flash, DeepSeek-V3, GLM-4.5, Kimi K2 (0711), Qwen3 Coder Plus | 9: Claude Sonnet 4 and Opus 4.1, GPT-5 and GPT-5 mini (low), o3, Gemini 2.5 Pro, Grok 4, Kimi K2 (0905), Qwen3 Max | 396 | 6,531 |
| DTap-Bench (benign runs) | `dtap_to_tau2.py` | customer service, CRM, telecom, travel, an operating system's files, medical | the agent's own training runs; its harness's other agents; the other harnesses' agents | 7: Claude Opus 4.6 and Sonnet 4.5 (Claude Agent SDK), Gemini 3 Pro (Google ADK), GPT-5.1, 5.2, 5.4 and gpt-oss-120b (OpenAI Agents SDK) | 3,990 | 40,357 |

**Reads and writes.** Which tools write was checked against each benchmark's source.

- AgentDojo's `get_unread_emails` marks what it returns as read, so it is a write; `get_webpage` only logs its request, so it is a read.
- BFCL's `cd` moves the working directory, and its logins change what later calls may do; both are writes.
- In WorkBench, every send, reply, forward, create, update and delete is a write.
- DTap-Bench publishes trajectories, not its servers, so its tools are marked by the first verb in their names, `getJiraIssue` and `meetings_get` alike. `get`, `list`, `search`, `find`, `query` and `view` read; `create`, `update`, `add`, `send`, `book`, `cancel`, `set` and the rest write, among them `transfer_to_human`, and so does a name with no verb the converter knows. A name reads by its first verb, not a noun after it: the medical domain's `request_complete_blood_count` orders a test, as its siblings `request_vital_signs` and `request_chest_xray` do, so `request` writes; `check_file_exists` reads. The Claude Agent SDK's own tools (`Bash`, `Read`, `TodoWrite`) are neither.
- MCPMark's tools are marked the same way. PostgreSQL's `execute_sql`, which runs whatever the agent writes, is a write, so a flow never calls it. Notion's searches and database queries are POST requests that read, so a `post` is a write only when no read verb follows it, and a name's hyphens become underscores (`API-post-search` is `API_post_search`), since τ²-bench's tools are Python functions.

**The runs as recorded.** BFCL-Result, the leaderboard's own archive, gives each model's logged calls, their results, and the checker's verdict on each task. WorkBench's harness logs an agent's reply as an action named `Final Answer`; it is the reply, not a call. Some models call tools that do not exist; those calls are neither reads nor writes. DTap-Bench's harnesses log the calls that list or load tools (`List MCP Tools`, the Claude Agent SDK's `ToolSearch`). Those belong to the harness, so they are left out with their results. The Claude Agent SDK names some results' tools wrongly, so results are paired with calls in the order the calls were made. The OpenAI Agents SDK logs some domains' results as Python reprs of MCP text blocks; the converter unwraps them to the JSON inside, and so does MCPMark's. MCPMark's tasks are hard (a quarter of the runs pass its checks) and `learn` fits the habit on successful runs, so its flows learn from every run's reads (`--learn-from-all`, which keeps the verdict as `verified`). OpenClaw, the fourth harness, logs only the reply, so its runs are left out.

## What decides an agent's turns

Test episodes of the agents each flow is replayed on. The ceiling counts every turn whose calls are all reads that a lookup made earlier could have answered: after a tool response since the last write, with every argument already in an earlier result, or with no arguments at all.

| Benchmark (domain) | Turns | Replies | Writes | Reads | Ceiling | With the user's words |
|---|---|---|---|---|---|---|
| τ²-bench (the paper's Table 1) | 39,298 | 46.8% | 12.2% | 41.0% | 29.0% | 29.7% |
| τ-bench | 6,244 | 47.0% | 13.2% | 39.7% | 28.4% | 29.8% |
| — retail | 4,340 | 47.4% | 11.6% | 40.9% | 30.1% | 31.4% |
| — airline | 1,904 | 46.1% | 17.0% | 36.9% | 24.3% | 26.3% |
| BFCL | 8,510 | 34.9% | 33.7% | 31.5% | 13.2% | 18.6% |
| AgentDojo | 1,523 | 26.9% | 16.5% | 56.5% | 22.1% | 27.1% |
| — travel | 412 | 19.4% | 7.3% | 73.3% | 47.1% | 52.2% |
| — Slack | 435 | 20.7% | 28.3% | 51.0% | 21.6% | 25.1% |
| — banking | 206 | 38.8% | 18.9% | 42.2% | 10.2% | 10.2% |
| — workspace | 470 | 34.0% | 12.8% | 53.2% | 6.0% | 14.3% |
| WorkBench | 13,869 | 27.4% | 26.6% | 46.1% | 3.5% | 10.9% |
| — project management | 1,810 | 24.5% | 22.8% | 52.7% | 6.4% | 19.4% |
| — multi-domain | 4,872 | 23.1% | 25.4% | 51.5% | 5.8% | 13.9% |
| — CRM | 1,673 | 26.7% | 23.0% | 50.3% | 4.2% | 18.8% |
| — email | 1,670 | 30.2% | 31.6% | 38.2% | 0.5% | 6.9% |
| — calendar | 2,046 | 29.8% | 27.7% | 42.5% | 0.1% | 2.3% |
| — analytics | 1,798 | 36.9% | 31.0% | 32.1% | 0.0% | 0.2% |
| MCPMark | 6,531 | 5.1% | 50.9% | 43.9% | 7.0% | 10.6% |
| — filesystem | 1,791 | 5.4% | 37.2% | 57.4% | 10.9% | 11.8% |
| — PostgreSQL | 1,426 | 5.0% | 73.9% | 21.0% | 4.9% | 11.6% |
| — GitHub | 1,273 | 5.9% | 30.0% | 64.1% | 2.5% | 9.7% |
| — Notion | 2,041 | 4.6% | 59.9% | 35.6% | 7.9% | 9.4% |
| DTap-Bench | 40,357 | 12.0% | 59.9% | 28.1% | 10.7% | 13.7% |
| — telecom | 1,229 | 27.3% | 7.6% | 65.0% | 34.3% | 35.0% |
| — customer service | 3,949 | 15.2% | 24.1% | 60.7% | 24.2% | 33.6% |
| — travel | 5,672 | 7.9% | 53.2% | 38.9% | 19.8% | 27.2% |
| — CRM | 4,533 | 17.8% | 32.8% | 49.4% | 11.5% | 17.6% |
| — OS files | 4,912 | 17.3% | 41.2% | 41.5% | 11.2% | 13.9% |
| — medical | 20,062 | 9.0% | 82.8% | 8.2% | 3.8% | 3.8% |
| — Claude Agent SDK (2 agents) | 11,059 | 18.2% | 60.9% | 21.0% | 7.5% | 10.1% |
| — OpenAI Agents SDK (4) | 26,196 | 8.7% | 59.9% | 31.5% | 12.5% | 15.7% |
| — Google ADK (1) | 3,102 | 18.4% | 57.0% | 24.6% | 7.4% | 9.9% |

τ-bench's turns divide as τ²-bench's do, to within a point or two, though its agents are a year older and ran in another harness: the domain sets the anatomy. Reads are 32–73% of turns everywhere; what varies is where their arguments come from.

- **τ²-bench and τ-bench.** A customer names themselves and the agent walks the records: user, then orders, then items.
- **AgentDojo's travel suite.** A city's listing names the hotels, and every later lookup takes those names.
- **WorkBench.** Each request names its customer, task or date; the agent searches for them, then writes. So nearly every read's arguments come from the request, which only reading language can bind.
- **DTap-Bench.** In customer service each request names an order or a customer's email, and the agent walks the order, its shipment, its cases and the guidelines; in telecom it walks the customer's account, bills and tickets. How an agent groups its calls matters as much as the harness: in customer service and travel, Gemini on Google's ADK batches its reads into a few parallel turns (3.5 an episode in customer service, where the others make 8–11). A turn of parallel reads is saved only when every read in it is answered, so its ceiling there is 7.6% and 0.5%, against 13–34% for the other agents. In CRM and telecom it batches less, and its ceiling is 13.5% and 21.7%. In the operating system's files, each request names a path, and the agents list, read and check the files there: 11.2% of turns are in the ceiling, and the path the user wrote adds 2.7 points. In medical, the agents' calls order tests and question a simulated patient, and a test ordered is a write. Six agents make 0–8 reads in 642 runs, and 6 of their 14,657 test turns are in the ceiling. gpt-oss-120b makes 4,056: it lists the patients 824 times and asks for a patient's status 3,231 times, often by an id it made up (`patient_1`, `PLACEHOLDER`), and a quarter of its reads repeat one it made before. 14% of its turns are in the ceiling, nearly all of medical's 3.8%: the agent is part of the ceiling.

The column with the user's words is that bound: 3–7 points more in BFCL, AgentDojo, WorkBench and DTap-Bench (6 without medical, half its turns, where nothing is read), and under 2 in τ²-bench and τ-bench, where the user's details arrive over the conversation and the agent looks the rest up.

**A model reading the request.** Binding the user's words takes a speculator that reads them. `ceiling.py`'s fifth count shows the System-One model what the user wrote so far and the call about to be made, its tool and the argument's name, and asks it to pick the argument's value among the spans of the user's words, one to six words long (`scripts/model_questions.py`, through `stretto ask`: 8,532 questions, about $0.41). A turn counts when every value in it is picked right. Of the points the user's words add, a pattern and the model take:

| Benchmark | Words add (points) | A pattern takes | The model takes | Values picked right |
|---|---|---|---|---|
| WorkBench | 7.4 | 0% | 39% | 34–81% by domain |
| BFCL | 5.3 | 2% | 53% | 61% |
| AgentDojo | 4.9 | 4% | 48% | 58–100% by suite |
| MCPMark | 3.6 | 1% | 2% | 3–16% by server |
| DTap-Bench | 3.0 | 17% | 30% | 28–97% by domain |
| τ-bench | 1.5 | 16% | 53% | 74% |
| τ²-bench | 0.7 | 31% | 50% | 83% |

- **About half of what the words add.** The model takes about half of it in BFCL, AgentDojo and both τ-benches, where a pattern takes 2–31%, and two fifths in WorkBench, where a pattern takes none. The instruction was revised once, on WorkBench multi-domain's 185 questions, where the model had picked spans with words around the value (59% right; told to pick exactly the value, 70%), and then fixed for every set. The picks are in [benchmarks-2026-09-27-model-picks.json](benchmarks-2026-09-27-model-picks.json), and every earlier count is unchanged.
- **MCPMark's values are composed.** They are SQL, paths and titles the agent writes; 18–41% of them are a span of the request, and neither a pattern nor the model takes them.
- **A request that names several values for one call.** In DTap-Bench's travel a request names several cities, and the agent looks each up with the same tool: one pick per call finds one of them, as a binding that picks one record of a listing would. 82% of travel's questions are picked right, and 1% of its turns. Which city comes next is the agent's choice, as Notion's next block is.

**The format is part of the ceiling.** Read as its tools print them — YAML records, Python dicts, and listings of a label with one name a line — AgentDojo's results hold few values a parser can find. Its ceiling was 10.0% of turns (Slack 0.5%, travel 26.0%). Read as JSON, with the same values, it is 22.1%. A deployment serves MCP tools, whose results are usually JSON, and the converter writes them so (`as_json`).

## Speculation in replay

Both deciders at θ = 0.3, the retail-live threshold of the paper. Saved turns are a share of all turns. Detours are per episode, and lower bounds (see above). The intervals are 95%, from a bootstrap over tasks, a task's episodes of every agent drawn together.

| Benchmark | Saved, reach | Saved, next-step | Reach minus next-step | Share of the ceiling, reach | Detours per episode, reach / next-step |
|---|---|---|---|---|---|
| τ-bench retail | 22.9% | 21.5% | +1.43 points (0.84 – 2.07) | 76% | 0.92 / 0.85 |
| τ-bench airline | 17.1% | 17.1% | 0 | 70% | 0.41 / 0.41 |
| BFCL | 1.57% | 0.73% | +0.85 points (0.35 – 1.45) | 12% | 0.085 / 0.049 |
| AgentDojo | 7.2% | 7.7% | −0.53 points (−1.74 – 0.00) | 32% | 0.10 / 0.11 |
| — travel | 8.7% | 10.7% | | 19% | 0.39 / 0.41 |
| — Slack | 16.8% | 16.8% | | 78% | 0.11 / 0.11 |
| — banking, workspace | 0 | 0 | | 0% | 0 / 0 |
| WorkBench | 0 | 0 | 0 | 0% | 0 / 0 |
| MCPMark | 0.20% | 0.20% | 0 | 3% | 0.040 / 0.033 |
| MCPMark, flows from each model's own runs | 0.44% | 0.43% | +0.02 points (−0.05 – +0.08) | 6% | 0.72 / 0.65 |
| DTap-Bench (6 domains), flows from the other harnesses | 2.2% | 1.4% | +0.86 points (0.68 – 1.06) | 21% | 0.16 / 0.08 |
| DTap-Bench (6 domains), flows from the agent's own runs | 3.4% | 2.2% | +1.27 points (1.06 – 1.48) | 34% | 0.23 / 0.06 |

In τ-bench retail the flows answer 42.4% of GPT-4o's API calls (471 of 1,110) and 44.8% of Claude 3.5 Sonnet's (525 of 1,171) with each call's exact arguments and result. They make 651 and 638 lookups, 72% and 82% of them used; next-step answers 40.5% and 41.4%. Speculative Actions (Ye et al., ICLR 2026) reports that its model speculators (GPT-5 family, Gemini 2.5 Flash; one to three guesses a step) predict 22–38% of τ-bench retail's API calls. Its agent differs and it guesses the next step, writes included, so the comparison is indicative, not controlled.

Swept over thresholds, reach saves more than next-step at every threshold in BFCL: +0.94 points at 0.1, +0.86 at 0.2, +0.85 at 0.3, +0.36 at 0.5. Its extra detours shrink faster, from +0.16 per episode at 0.1 to +0.01 at 0.5. A flow learned from BFCL's ground-truth trajectories of the training tasks, replayed on those of the test tasks, saves 2.2% of turns against 0.7% at 0.3 (+1.59 points, 0.80–2.50).

On AgentDojo, the flows take 78% of Slack's ceiling and 19–23% of travel's, and nothing in banking or workspace. The deciders tie at 0.1 too (−0.46 points, −2.06 to +0.57). In banking, the most-read lookup takes a number, how many transactions (`n`), and the binding passes only strings; the reads that take no argument each scored under 0.15. In workspace, every search takes a query, a day or a file name that the user's request gave. The two deciders make the same lookups in Slack. In travel, where the only write is the final booking, "before the next write" means "in the episode", and every tool of a listing's chain scores high. After a listing of car rental companies, the reach decider looked up every company's fuel options and prices, 37 lookups that no agent made with the whole listing. The next-step decider asked for the companies' car types, which the agents did ask for, 8 times of 10, and saved 8 more turns of 412. The travel flow learned from 38 successful sessions; the reach counts are the sparser for it.

BFCL's flow takes only 12% of its ceiling. BFCL's 200 base tasks are 200 different requests over eight APIs and 128 tools, and eight models' runs of 120 of them are few sessions to learn what comes next: the flow's scores are low. At θ = 0.1 it takes 17% of the ceiling, for three times the detours.

On MCPMark's Notion server, the agents walk a page's blocks by the ids each listing returns, and whether that is in the ceiling depends on the agent: 22–34% of the Claude models' and Gemini 2.5 Pro's turns are, and at most 4% of the other six models'. GPT-5 and o3 pass a page size with every read of a block, a constant that a flow binding from results does not pass; counted in, constants would raise Notion's ceiling from 7.9% to 13.2%. The flows learn the walk: after a block's children, the agents' next call is for another block's children 55% of the time. They cannot tell which block: a binding picks the agent's block 38% of the time, so at 0.3 they hand back. Nor does a simple rule. Of the 229 block reads the newer models took from a listing, the binder's rule, the first unopened block of the most recent listing, picks the one read 38% of the time, as its learned chance says, and 46% if it skips blocks without children. Even told which listing the walk goes on from, the first unopened block is the one read 59% of the time (63% with children). At 0.1 they take 43% of the Claude models' and Gemini's ceiling there, at 14 detours an episode. Learned from each model's own 16 training runs, they save 0.6% of Notion's turns at 2.6 detours an episode; o3's own flow makes 173 detours, since its lookups leave out the page size its calls pass. Over MCPMark's four servers, each model's own flows save 0.44% of turns at 0.3, twice the pooled flows' 0.20%, and 6% of the ceiling.

Learned with `learn --constants`, each own flow also passes the arguments its agent passed with one value every time. At 0.3, own flows before and after:

| Own flows, use before write | Turns saved, without / with constants | Detours per episode, without / with |
|---|---|---|
| MCPMark Notion (9 models) | 13 / 21 | 2.58 / 1.60 |
| DTap-Bench CRM (7 agents) | 62 / 82 | 0.02 / 0.03 |
| DTap-Bench OS files (7 agents) | 155 / 262 | 0.09 / 0.11 |
| DTap-Bench customer service; MCPMark filesystem, PostgreSQL, GitHub | unchanged | unchanged |

o3's lookups now pass the page size it always passes, and its detours fall from 173 to 23. GPT-5's page size was a required argument the flow could not bind, so it made no block lookups; now it makes them, and saves 6 turns at 43 detours. In CRM, GPT-5.4's own flow saves 18 turns instead of 4. In the operating system's files, the agents always ask for a message's body as text (`prefer`), and the Claude models always sort a listing by size: with those, the own flows save 5.3% of the domain's turns instead of 3.2%, the 2.2 points constants add to its ceiling. Over MCPMark's four servers, the own flows save 0.57% of turns with constants, against 0.44% without. Most of what constants add to Notion's ceiling is still the walk's: which block comes next. A string a user wrote with a digit or an @, an id or an email, is never learned as a constant, however many sessions shared it: of these flows, that drops only the repository EasyR1 from Kimi K2's GitHub flow, which its users named, and GitHub's own flows save no turns and make no detours either way.

### Across harnesses

DTap-Bench runs the same tasks under three agent SDKs, so a flow can be learned under one and served under another. In each of six domains (customer service, CRM, telecom, travel, an operating system's files and medical), each agent's test episodes are replayed with four flows: learned from its own training runs, from its harness's other agents, from the other harnesses' agents, and from all seven. Both deciders at θ = 0.3; intervals are 95%, from a bootstrap over tasks, a task's episodes of every agent drawn together.

| Domain | Source | Agents | Turns | Saved, reach | Saved, next-step | Reach minus next-step | Share of the ceiling, reach | Detours per episode, reach / next-step |
|---|---|---|---|---|---|---|---|---|
| All six | own runs | 41 | 39,252 | 3.4% | 2.2% | +1.27 (+1.06 – +1.48) | 34% | 0.23 / 0.06 |
|  | harness-mates | 36 | 37,255 | 3.1% | 2.0% | +1.14 (+0.94 – +1.34) | 28% | 0.18 / 0.05 |
|  | other harnesses | 42 | 40,357 | 2.2% | 1.4% | +0.86 (+0.68 – +1.06) | 21% | 0.16 / 0.08 |
|  | all seven | 42 | 40,357 | 3.4% | 2.3% | +1.09 (+0.90 – +1.29) | 32% | 0.22 / 0.08 |
| Customer service | own runs | 7 | 3,949 | 7.0% | 2.3% | +4.68 (+4.16 – +5.20) | 29% | 1.48 / 0.25 |
|  | harness-mates | 6 | 3,725 | 5.1% | 2.0% | +3.06 (+2.59 – +3.52) | 20% | 0.89 / 0.23 |
|  | other harnesses | 7 | 3,949 | 4.0% | 1.8% | +2.18 (+1.65 – +2.70) | 16% | 0.52 / 0.12 |
|  | all seven | 7 | 3,949 | 6.3% | 3.0% | +3.32 (+2.42 – +4.07) | 26% | 0.89 / 0.13 |
| CRM | own runs | 7 | 4,533 | 1.4% | 0.6% | +0.75 (+0.53 – +0.98) | 12% | 0.02 / 0.01 |
|  | harness-mates | 6 | 4,036 | 1.1% | 0.5% | +0.64 (+0.41 – +0.88) | 10% | 0.02 / 0.01 |
|  | other harnesses | 7 | 4,533 | 0.8% | 0.8% | +0.09 (+0.00 – +0.21) | 7% | 0.09 / 0.07 |
|  | all seven | 7 | 4,533 | 1.3% | 1.1% | +0.11 (+0.02 – +0.21) | 11% | 0.15 / 0.09 |
| Telecom | own runs | 7 | 1,229 | 13.0% | 7.6% | +5.37 (+3.04 – +7.79) | 38% | 0.01 / 0.00 |
|  | harness-mates | 6 | 1,086 | 11.5% | 7.6% | +3.87 (+2.59 – +5.09) | 32% | 0.06 / 0.02 |
|  | other harnesses | 7 | 1,229 | 4.9% | 1.1% | +3.82 (+2.30 – +5.52) | 14% | 0.04 / 0.01 |
|  | all seven | 7 | 1,229 | 13.2% | 8.2% | +4.96 (+2.38 – +7.71) | 38% | 0.04 / 0.00 |
| Travel | own runs | 6 | 4,567 | 14.1% | 10.6% | +3.50 (+2.61 – +4.50) | 86% | 0.41 / 0.07 |
|  | harness-mates | 6 | 5,463 | 13.2% | 9.5% | +3.77 (+3.00 – +4.53) | 64% | 0.57 / 0.21 |
|  | other harnesses | 7 | 5,672 | 10.3% | 6.9% | +3.39 (+2.53 – +4.31) | 52% | 0.46 / 0.23 |
|  | all seven | 7 | 5,672 | 13.3% | 9.8% | +3.49 (+2.93 – +4.05) | 67% | 0.52 / 0.18 |
| OS files | own runs | 7 | 4,912 | 3.2% | 2.1% | +1.08 (+0.76 – +1.41) | 28% | 0.09 / 0.02 |
|  | harness-mates | 6 | 4,269 | 1.8% | 0.9% | +0.82 (+0.49 – +1.21) | 17% | 0.14 / 0.01 |
|  | other harnesses | 7 | 4,912 | 1.3% | 0.9% | +0.41 (+0.18 – +0.65) | 12% | 0.32 / 0.25 |
|  | all seven | 7 | 4,912 | 2.7% | 1.8% | +0.94 (+0.62 – +1.27) | 24% | 0.33 / 0.26 |
| Medical | own runs | 7 | 20,062 | 0.2% | 0.2% | +0.00 (+0.00 – +0.00) | 6% | 0.04 / 0.04 |
|  | harness-mates | 6 | 18,676 | 0.0% | 0.0% | +0.00 (+0.00 – +0.00) | 0% | 0.00 / 0.00 |
|  | other harnesses | 7 | 20,062 | 0.0% | 0.0% | +0.00 (+0.00 – +0.00) | 0% | 0.00 / 0.00 |
|  | all seven | 7 | 20,062 | 0.1% | 0.1% | +0.00 (+0.00 – +0.00) | 3% | 0.01 / 0.01 |

gpt-oss-120b succeeded in none of its travel runs, so it has no flow of its own there (`learn` fits the habit on successful runs). Gemini is its harness's only agent.

Turns saved by reach over the six domains, by the flow's source, with the share of what the agent's own flow saves:

| Agent (harness) | Own runs | Harness-mates | Other harnesses |
|---|---|---|---|
| GPT-5.1, 5.2 and 5.4 (OpenAI Agents SDK) | 826 | 743 (90%) | 540 (65%) |
| Claude Opus 4.6 (Claude Agent SDK) | 235 | 143 (61%) | 145 (62%) |
| Claude Sonnet 4.5 (Claude Agent SDK) | 108 | 87 (81%) | 60 (56%) |
| Gemini 3 Pro (Google ADK) | 60 | — | 27 (45%) |
| gpt-oss-120b (OpenAI Agents SDK), no travel | 116 | 186 | 133 |

- **A flow carries across harnesses at about three fifths of the agent's own.** Over the 41 agent-domain pairs with a flow of their own, a flow from the other harnesses keeps 59% of its turns, with 64% of its detours; one from the agent's harness-mates keeps 79%. Each SDK runs one vendor's models here, so harness and model family go together: the GPT-5 models keep 90% from each other, and Opus 4.6 keeps as much from the other harnesses' agents as from Sonnet 4.5 in its own.
- **Own flows overreach where sessions are few.** In customer service, learned from 96 runs of one agent, the flows score high and miss: of the reach flows' lookups scored 0.8–0.9, a quarter were used. Learned from all seven agents, they keep 90% of the own flows' turns with 60% of their detours (over the six domains, 92% with 91%).
- **Reach leads in every domain with reads to take, least in CRM and the operating system's files**, whose ceilings are 11.5% and 11.2% (+0.1 to +1.1 points). With the agents' own runs it takes 86% of travel's ceiling and 38% of telecom's. In medical both deciders make the same lookups, gpt-oss-120b's, and save 49 of the domain's 20,062 turns.

**Do agents skip what they already have?** The replay assumes that an agent that sees a lookup's result does not make the call itself. The record shows how often an agent makes a read again that it already made, with no write between, as an agent that ignored a lookup's result would (`scripts/remade.py --results`). Six of the seven agents did so for 22 of their 21,524 reads over the six domains, in all three harnesses. gpt-oss-120b did so for 1,919 of its 8,471 (23%; 7 of 766 in the operating system's files, 995 of 4,056 in medical), so its replayed savings are upper bounds. Counted the same way for every replayed agent of every benchmark:

| Benchmark | Agents | Reads | Made again, no write between | Most by one agent |
|---|---|---|---|---|
| τ²-bench | 9 (27 runs) | 58,129 | 0.8% | 4.1% (one run) |
| τ-bench | 2 | 10,608 | 1.0% | 1.1% |
| BFCL | 10 | 7,880 | 0.9% | 2.0% (DeepSeek V3.2) |
| AgentDojo | 10 | 2,390 | 1.5% | 6.2% (Llama 3.3 70B) |
| WorkBench | 14 | 21,060 | 2.6% | 17.5% (Qwen3.5 Flash) |
| DTap-Bench | 7 | 29,995 | 6.5% | 23% (gpt-oss-120b) |

The median agent of each benchmark repeats at most 1.1% of its reads; a few models repeat many, and their replayed savings are upper bounds. On MCPMark's four real servers, the nine replayed models made 3.4% of their reads again after the same read had succeeded, and 0.8% more after it had failed: the Claude and GPT-5 models under 1%, Kimi K2 and Gemini 2.5 Pro under 2%, and Qwen3 Max, o3 and Grok 4 5.5%, 8.3% and 9.3%.

### Calibration

With `--explore 0` each replay logs every lookup the flow weighed, with its score and whether the agent made the call before its next write (`scripts/calibration.py`). Reach minus next-step, with a paired bootstrap over tasks:

| Benchmark | Lookups weighed | ECE | Brier | AUC |
|---|---|---|---|---|
| τ-bench retail | 4,577 | −0.080 (−0.095 – −0.041) | −0.027 (−0.039 – −0.014) | +0.009 (−0.004 – +0.025) |
| τ-bench airline | 2,588 | +0.003 (−0.004 – +0.017) | −0.003 (−0.006 – +0.001) | −0.009 (−0.041 – +0.015) |
| BFCL | 3,601 | −0.021 (−0.035 – +0.001) | −0.004 (−0.017 – +0.009) | +0.054 (−0.002 – +0.118) |
| AgentDojo travel | 772 | +0.068 (+0.008 – +0.142) | +0.028 (−0.004 – +0.091) | −0.006 (−0.120 – +0.044) |
| AgentDojo Slack | 395 | +0.006 (−0.019 – +0.030) | −0.005 (−0.018 – +0.011) | +0.027 (0.000 – +0.064) |
| DTap-Bench customer service, own runs | 9,625 | −0.071 (−0.081 – −0.050) | −0.028 (−0.035 – −0.022) | +0.017 (−0.001 – +0.039) |
| — other harnesses | 16,280 | −0.033 (−0.045 – −0.016) | −0.011 (−0.016 – −0.007) | −0.016 (−0.032 – +0.000) |
| DTap-Bench CRM, own runs | 4,151 | −0.014 (−0.016 – −0.006) | −0.006 (−0.007 – −0.004) | +0.018 (+0.011 – +0.027) |
| — other harnesses | 10,840 | +0.001 (−0.001 – +0.007) | −0.000 (−0.001 – +0.000) | +0.001 (−0.006 – +0.008) |
| DTap-Bench telecom, own runs | 1,189 | −0.139 (−0.173 – −0.027) | −0.067 (−0.108 – −0.025) | +0.117 (+0.035 – +0.193) |
| — other harnesses | 1,626 | −0.060 (−0.100 – −0.004) | −0.036 (−0.064 – −0.005) | +0.052 (+0.011 – +0.093) |
| DTap-Bench travel, own runs | 1,514 | +0.017 (−0.035 – +0.058) | +0.011 (−0.047 – +0.058) | −0.391 (−0.488 – −0.269) |
| — other harnesses | 2,237 | −0.004 (−0.043 – +0.027) | −0.031 (−0.067 – −0.001) | −0.181 (−0.239 – −0.106) |
| DTap-Bench OS files, own runs | 4,533 | −0.034 (−0.044 – −0.009) | −0.013 (−0.017 – −0.009) | +0.051 (+0.033 – +0.068) |
| — other harnesses | 7,978 | −0.014 (−0.029 – −0.002) | −0.006 (−0.011 – −0.002) | +0.038 (+0.019 – +0.059) |
| DTap-Bench medical, own runs | 2,579 | −0.008 (−0.021 – +0.005) | −0.002 (−0.003 – −0.000) | +0.056 (+0.037 – +0.074) |
| — other harnesses | 9,815 | +0.002 (+0.001 – +0.002) | −0.000 (−0.000 – +0.000) | −0.024 (−0.072 – +0.005) |

WorkBench's flows weighed 4,154 lookups, all scored near zero, and the agents used none: there is nothing to calibrate. In DTap-Bench reach is better calibrated in customer service, telecom and the operating system's files, and alike in CRM and medical; in travel, as in AgentDojo's, it ranks lookups worse while saving more turns. As on τ²-bench, counting the right event helps most where agents read ahead of their next write in varied orders, as in τ-bench retail. In BFCL the differences have the same signs, with intervals that reach zero. Where the next call is nearly always the call that is used, as in airline, the two coincide.

## A threshold per decision

The paper's θ* prices a domain's average detour against its average saved turn. Proposition 3 holds per read, so each decision can be priced where it stands: a detour costs the looked-up tool's mean result tokens times the LLM turns left, and a saved turn is worth the next turn's input tokens plus the call's tokens over the turns after it. `scripts/per_decision.py` evaluates that rule off-policy, over the logged decisions of reach replays at θ = 0.1 on τ²-bench, for the eight agents whose episodes report tokens (`pilot/check_flow.py` now logs each decision's place in the episode). A rule is credited along each logged chain of lookups until it departs from the logged one. At flat thresholds this reproduces the actual sweep's turns saved to within 5.2% on average over 36 replays (+1.9% on balance).

| Domain | θ* | Utility at θ* (M tokens) | Per decision | Only lowered below θ* | Only raised above θ* | Detours, θ* / per decision |
|---|---|---|---|---|---|---|
| Retail | 0.298 | 17.1 | −15.3% (every agent, −11 to −19%) | −0.5% | −14.7% | 572 / 494 |
| Airline | 0.126 | 4.9 | +6.7% (−5 to +13%) | +13.0% | −6.3% | 506 / 2,440 |
| Telecom | 0.117 | 15.5 | +10.0% (0 to +102%) | −0.5% | +10.5% | 2,786 / 4,012 |

The rule raises the threshold early, where a detour rides in the most turns, and lowers it late. Which of the two pays depends on the scores where they move:

- **Retail loses by raising.** Early in an episode it drops the lookups of an order's products, scored 0.33 (a product lookup's chance, 0.43, times the binding's, 0.76), which the agents used 64% of the time.
- **Telecom gains by raising.** The agents that use fewer lookups than scored gain most: GPT-5.2 +57% and +102%, GLM-5 +30%.
- **Airline gains by lowering**, with five times the detours, each late and cheap.

A threshold per decision is only as good as each score where it is priced, and the scores are calibrated on average, not site by site. The average threshold forgives a score that is off where the costs are extreme.

**Calibrating each site first** (`--recalibrate WEIGHT`). Each lookup is scored instead by the rate at which lookups at its site (site, tool and binding's chance) were used, counted in the other half of the agent's test tasks, with the model's score as a Beta prior of WEIGHT lookups. The two halves are scored in turn, so no lookup is scored on its own outcome.

| Domain | θ*, recalibrated (40) | Per decision, recalibrated (40) | θ*, recalibrated (10) | Per decision, recalibrated (10) |
|---|---|---|---|---|
| Retail | −1.0% | −3.5% | +0.5% | −3.2% |
| Airline | +1.0% | +4.6% | −14.1% | −1.2% |
| Telecom | +2.5% | +2.7% | −0.4% | +0.6% |

Changes are against θ* with the model's scores. Calibrating the sites removes most of retail's loss, which was the miscalibrated sites', and leaves the threshold per decision within 5% of θ* everywhere. With a prior of 10 lookups it overfits airline's twenty test tasks. The served threshold stays the domain's: pricing each decision gains little once the scores are right on average, and costs much where they are not.

## In seconds and dollars

τ²-bench's recorded episodes report each LLM turn's generation time and cost for six of the paper's agents: Claude Opus and Sonnet 4.5, Gemini Pro and Flash, and GPT-5.2 at high and no reasoning. `pilot/check_flow.py` now names the turns a replay saved (`saved_at`), and `scripts/priced.py` prices them. It charges each detour the domain's counted detour tokens at the agent's input price, fitted on the agent's own turns. The replays are the paper's, at θ = 0.3.

| Domain | Turns saved, reach / next-step | Generation time saved per episode | Share of generation time | Cost saved per episode, net of detours | Share of cost |
|---|---|---|---|---|---|
| Retail | 28.3% / 24.9% | 35.6 s / 30.7 s | 23.6% / 20.4% | $0.026 / $0.022 | 18.1% / 15.2% |
| Airline | 10.9% / 10.8% | 17.0 s / 16.7 s | 7.2% / 7.1% | $0.012 / $0.012 | 6.5% / 6.4% |
| Telecom | 12.4% / 11.4% | † | † | $0.029 / $0.028 | 12.1% / 11.7% |

† Some of telecom's turns report no generation time for four of the six agents.

A turn that only reads generates less than one that replies, so a saved turn is worth less than the average turn, in seconds and in dollars. Per agent in retail, the saved turns are 17–26% of the generation time and 9–22% of the cost. GPT-5.2's share of the cost is lowest, since its reasoning makes its other turns dear. The costs are the benchmark's own records.

### Priced as billed

The paper's θ* = δ/(β+δ) counts both costs in input tokens (its Appendix B). Providers bill differently: an output token costs 4–8 times an input token, and with prompt caching a context's prefix is reread at about a tenth of the price. `scripts/costs.py --price OUT,READ,WRITE` counts both costs that way. A saved turn is worth its prompt, read from the cache up to the previous turn's prompt and written after it, plus its output tokens. A detour's result is written once and read from the cache in every later context. The sweep's replays are then priced at each domain's pooled costs (`--sweep`), for the six agent and domain pairs swept.

| Pricing, per uncached input token | θ*, retail | θ*, airline | θ*, telecom | Utility kept at the served 0.3 / 0.15 / 0.1, worst pair | The utility's scale |
|---|---|---|---|---|---|
| Input tokens alone (the paper's) | 0.30 | 0.13 | 0.12 | 91% | 1 |
| Output 5×, nothing cached | 0.27 | 0.11 | 0.11 | 94% | ×1.1–1.3 |
| Anthropic's: output 5×, cache reads 0.1×, writes 1.25× | 0.29 | 0.07 | 0.11 | 97% | ÷2.2–3.9 |
| OpenAI's: output 8×, cache reads 0.1×, writes 1× | 0.23 | 0.05 | 0.08 | 98% | ÷1.7–3.1 |

Caching discounts a detour more than a saved turn. A detour is carried almost entirely as cache reads, while a saved turn's output and new input are never cached. θ* therefore falls, most in airline (from 0.13 to 0.05–0.07). The thresholds counted in input tokens stay near the best: in each of the six pairs they keep at least 94% of the use-before-write speculator's best swept utility, and at least 97% with caching. Caching shrinks the utility two- to fourfold in input-token terms, while a saved turn still spares the time its output takes to generate.

## What the binder learned

### Lists

AgentDojo's travel suite showed a gap no τ²-bench domain had. After `get_all_hotels_in_city`, the habit knew that `get_hotels_prices` comes next 79% of the time. But its `hotel_names` argument is a list: every name the listing gave. The binding traced only string arguments, so the flow never made that lookup, and the suite's 47% ceiling was out of reach.

A binding now also traces list arguments. A list is found at a path of an earlier output when every value of it is a value there: `$.items[*]` of the listing. The binding then passes every value at that path of the most recent such output, in order, unless the lookup was already given that list. Its chance is scored as a string's is, at the agent's own calls in training. In AgentDojo that is 0.75 for the hotels' prices after a listing: sometimes the agent had already narrowed the hotels down.

On AgentDojo's travel suite, the same learned flow with its lists removed makes no lookup at all, with either decider. With them it saves 36 turns with the reach decider and 44 with next-step, 8.7% and 10.7% of the suite's turns, for 31 and 33 detours. Of the reach flow's lookups, those of every hotel or restaurant in a listing were used 44 times of 83; its lookups of every car rental company's fuel options and prices (37) never were.

A flow with lists is written as format 2. A build that reads only format 1 must refuse it rather than make fewer lookups than the flow was learned to (docs/formats.md). Flows without lists behave exactly as before: GLM-5's retail replay with the paper's flow is identical, decision log and all.

### Searches the agent always narrows

A lookup's required arguments are those the agent passed in at least 90% of its calls. WorkBench's searches take several optional filters: a task's name, its assignee, its board, its due date. Each agent narrows by one or another, so none is required. The flow made those lookups with no argument at all, and gave them a binding chance of 1, as for a read that takes none. Every lookup the flows made in WorkBench was of this kind, `search_tasks()` or `search_customers()`, which no agent ever calls; the first answers "No search parameters provided". Over the first 52 target-domain pairs the reach flow saved 1 turn and made 289 detours.

The chance that bound arguments are the agent's own must hold for no arguments too. For each lookup without a required argument that the agent sometimes called with one, `learn` now counts the calls that passed none (`bindings.bare`). Such a lookup, made with none, has the chance (bare + 1) / (calls + 2), Laplace-smoothed as a string binding's agreement is. A search the agent always narrows gets a chance near 0, and is not made. A read the agent always calls with no argument, such as the day or the user's balance, keeps the chance 1, as before.

With the count, the WorkBench flows make no lookup at all in the 13,869 turns of the fourteen agents' test episodes, at θ = 0.3 or 0.1: nothing saved where the ceiling held 3.5%, and no detour, where they had made 289 in the first 52 agent-domain pairs. The flows still weigh lookups (4,154 of them, all at scores near zero). On AgentDojo the count removed the travel flow's bare lookups of hotel addresses, whose argument the agents had passed under two names, and 21 of its detours (0.65 to 0.39 per episode with the reach decider), for the same saved turns. BFCL's flows changed little: `ls` without its flag.

## What is next

- **Reading the request.** A model that picks the value among the user's words takes 30–53% of what the words add in WorkBench, BFCL, AgentDojo and DTap-Bench, where a pattern takes 0–17% (above). A speculator that reads the request needs the conversation, which an MCP proxy does not see, and a model to read it; where a request names several values for one call, it would also need to know which the agent takes next.
- **More domains.** DTap-Bench's other domains with benign runs (browser, finance, legal, macOS and research) have 4–34 tasks each, too few to hold out 40% of them and learn from the rest. BFCL v4's agentic categories, web search and memory, are published in the same archive and not yet converted; a search whose results name the pages the agent then fetches is a listing of the kind flows bind from.
- **Compiling once beyond τ²-bench's solo telecom.** A procedure compiled once must bind every value of every call, writes included. `ceiling.py` counts the test episodes in which every value came from an earlier result, a constant or what the user wrote, with none the agent composed: 37% of AgentDojo's, 20% of BFCL's, 18% of WorkBench's, 10% of DTap-Bench's (84% of its telecom's) and 4% of MCPMark's. With none from what the user wrote either, at most 8%. There a procedure compiled once would need a model to read the request, and in most episodes one to write.
- **Calibration per site.** A threshold per decision needs each lookup's score right where it is priced. Retail's lookups of an order's products are scored as the tool's chance (0.43) times the argument's (0.76) and used 64% of the time; counting each lookup's own use at its site would score them directly.
- **Live, with other agents.** The replays assume an agent skips a call a lookup already answered, which GLM-5.3 did live. The DTap-Bench replays count what agents in three other harnesses would have skipped, not whether they would have. That six of the seven seldom repeat their own reads says they would; it remains to be checked live.

## Caveats

- **Counterfactual turns.** A replay counts what the flow would have spared an agent that otherwise acted as recorded. Live, GLM-5.3 made none of the flow's 101 lookups again (the paper, §6). The 72 agents here were never run with a flow. Six of DTap-Bench's seven agents repeated 22 of their 21,524 reads with no write between; gpt-oss-120b repeated 23%, and its savings are upper bounds. On the other benchmarks agents repeat 0.8–2.6% of their reads, most of them by a few models (Qwen3.5 Flash 17.5% in WorkBench).
- **Detours are lower bounds.** They are 55–72% of the environment's on τ²-bench, and the reach decider's lead in savings comes with more detours than these tables show. The paper's costs make one saved turn worth about 2.4 detours in retail and about 7 in airline and telecom.
- **One trial each.** BFCL, AgentDojo, WorkBench and DTap-Bench publish one run per model and task (DTap-Bench's latest, where a task was run again), so their intervals are over tasks, with every agent's episode of a task drawn together.
- **Constants.** Counted in the ceiling (`scripts/ceiling.py`'s third count), arguments an agent passes with one value every time would add 5.2 points of Notion's turns, 2.2 of DTap-Bench's OS files', 2.0 of its CRM's, 0.8 of its customer service's and 0.1 of BFCL's, and nothing in the other domains. `learn --constants` learns them; AgentDojo banking's agents pass a different number of transactions from task to task.
- **Benchmarks left out.** Gaia2's published results (facebook/omnilingual-gaia2-results: seven models in the OpenClaw and OpenCode harnesses) cut 68–70% of read results to 200 characters. They also record the apps' calls, not the model's turns. A replay could neither bind a lookup's arguments nor count turns, so Gaia2 is not among these benchmarks. Toolathlon's trajectories need a login.
- **The converters are choices.** Results are rendered as JSON, a listing becomes a label and its items, and every write is labelled by hand from the source, or in DTap-Bench by its verb. Each converter's docstring says what it does, and the checkout's `tools.py` lists every label.

## Reproduce

The benchmarks' runs are public, and nothing here needs an API key. From the repository root, with the benchmarks' data in `$DATA` and the converted runs written to `$WORK` (`scripts/bench/README.md`):

```bash
export WORK=work; mkdir -p $WORK/v1
# τ-bench: github.com/sierra-research/tau-bench, historical_trajectories/
python3 scripts/taubench_v1_to_tau2.py $DATA/tau-bench/historical_trajectories/gpt-4o-retail.json \
    --domain retail --agent gpt-4o --out $WORK/v1/gpt-4o-retail.json      # and the other three files

# BFCL: the bfcl-eval wheel unpacked in $DATA/bfcl (with mpmath), and a BFCL-Result snapshot
python3 scripts/bfcl_to_tau2.py --bfcl $DATA/bfcl --out $WORK/bfcl --runs $DATA/BFCL-Result/2025-12-16 \
    --models gpt-4.1-2025-04-14-FC claude-opus-4-5-20251101-FC ...     # the 18 models above

# AgentDojo: github.com/ethz-spylab/agentdojo, runs/
python3 scripts/agentdojo_to_tau2.py --runs $DATA/agentdojo/runs --out $WORK/dojo

# WorkBench: github.com/olly-styles/WorkBench, data/results/
python3 scripts/workbench_to_tau2.py --results $DATA/WorkBench/data/results --out $WORK/wb

# MCPMark: huggingface.co/datasets/Jakumetsu/mcpmark-trajectory-log, mcpmark-v1-0905/<model>__<service>/run-1/<task>/,
#   messages.json and meta.json in mcpm-runs/<model>__<service>/<task>/
for s in filesystem postgres github notion; do python3 scripts/mcpmark_to_tau2.py --runs mcpm-runs --service $s --out $WORK/mcpm --learn-from-all; done

# DTap-Bench: huggingface.co/datasets/AI-Secure/DTap-Bench-Agent-Trajectories, each config's
#   <harness>/<model>/<domain>/benign/<task>/: the latest <time>.json as traj.json, and judge_result.json as judge.json,
#   in dtap-runs/<domain>/<harness>__<model>/<task>/
python3 scripts/dtap_to_tau2.py --runs dtap-runs --domain customer-service --out $WORK/dtap
for d in crm telecom travel os-filesystem medical; do
  python3 scripts/dtap_to_tau2.py --runs dtap-runs --domain $d --name "dtap_${d//-/_}" --out $WORK/dtap
done
python3 scripts/remade.py --results $WORK/dtap/dtap_telecom/*.json --tau2 $WORK/dtap/checkout      # repeated reads

# The user's words read by a model: each set's questions, the model's picks (Jev, through `stretto ask`), the fifth count
python3 scripts/ceiling.py $WORK/wb/multi_domain/*.json --tau2 $WORK/wb/checkout --questions q-wb-multi_domain.jsonl
python3 scripts/model_questions.py q-*.jsonl --out picks.json --oracle jev --oracle-budget 1
python3 scripts/ceiling.py $WORK/wb/multi_domain/*.json --tau2 $WORK/wb/checkout \
    --model-answers docs/results/benchmarks-2026-09-27-model-picks.json --json ceiling-wb-multi_domain.json

# Learn from the older agents, replay the newer ones from the record (here AgentDojo's travel suite)
target/release/stretto learn --results $WORK/dojo/travel/gpt-4-0125-preview.json ... \
    --tau2 $WORK/dojo/checkout --domain travel --habit-only --out travel.flow.json
python3 pilot/check_flow.py --domain travel --trials 0 --results $WORK/dojo/travel/claude-3-5-sonnet-20241022.json \
    --flow travel.flow.json --flow-decider reach --flow-threshold 0.3 --flow-oracle replay --explore 0 \
    --trace --tau2 $WORK/dojo/checkout --out replays/c-travel-claude-3-5-sonnet-20241022-reach-0.3
python3 scripts/ceiling.py $WORK/dojo/travel/claude-3-5-sonnet-20241022.json --tau2 $WORK/dojo/checkout
python3 scripts/calibration.py replays/c-travel-*-habit-0.3 --versus replays/c-travel-*-reach-0.3

# Every replay behind this page, then its tables (scripts/bench/README.md): converted runs in $WORK as above
scripts/bench/replay.sh all
python3 scripts/bench/tables.py --json rows.json
python3 scripts/per_decision.py --replays $WORK/replays/perdec --results .data/tau2-targets --sweep $WORK/replays/sweep \
    --tau2 ../tau2-bench --recalibrate 40                              # a threshold per decision
python3 scripts/priced.py --replays $WORK/replays/tau2-env --results .data/tau2-targets   # seconds and dollars
for price in 0,1,1 5,1,1 5,0.1,1.25 8,0.1,1; do                     # θ* priced as billed, and the sweep at those prices
  python3 scripts/costs.py .data/tau2-targets/*.json --replays $WORK/replays/tau2-env --tau2 ../tau2-bench \
      --price $price --sweep docs/results/reach-2026-09-26.json; done
```

The rows behind every table are in [benchmarks-2026-09-27.json](benchmarks-2026-09-27.json): the trace-versus-environment comparison, each agent's ceiling, every replay's totals and episodes, the calibration summaries, the list ablation, the WorkBench runs before bare calls were counted, DTap-Bench by the flow's source, the per-decision evaluation, the saved turns priced in seconds and dollars, and θ* priced as billed (`costs_priced`). The flows every replay served are in [benchmarks-2026-09-27-flows.tar.gz](benchmarks-2026-09-27-flows.tar.gz).
