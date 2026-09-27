# Quickstart: the loop in under a minute, with no key

[`run.sh`](run.sh) runs stretto's whole loop on `stretto-mcp-demo`, a tiny MCP server that ships with stretto: it records sessions through `stretto-proxy`, learns a flow from them, shows the flow for review, serves it to new customers and counts the calls the agent no longer makes. It needs no API key and no network, only a POSIX shell and the binaries, and it takes about a second. CI runs it, so this page cannot drift from the code.

A scripted agent stands in for an LLM. The demo's shop (`stretto-mcp-demo --world retail`) has three read tools and one write, with canned data: `cN@example.com` is `user_N`, whose orders `#WNa` and `#WNb` are pending. For each customer the agent finds the user, reads their details and both orders, then cancels the first order or says how the orders stand, one call at a time.

## Run it

From a checkout:

```sh
cargo build --release -p stretto-report -p stretto-proxy
examples/quickstart/run.sh
```

With stretto installed ([docs/install.md](../../docs/install.md)), the binaries are on PATH, and the script is all you need from the repository:

```sh
curl -fsSLO https://raw.githubusercontent.com/alexnodeland/stretto/main/examples/quickstart/run.sh
sh run.sh
```

In the container image, which has the script:

```sh
docker run --rm --entrypoint /usr/local/share/stretto/quickstart/run.sh ghcr.io/alexnodeland/stretto
```

`--bin DIR` names the directory with `stretto`, `stretto-proxy` and `stretto-mcp-demo` (by default: PATH, else the checkout's `target/release` or `target/debug`). `--work DIR` keeps everything in a new or empty directory of your choosing (by default: a new temporary one). The script exits non-zero if a step does not do what this page says.

## What it shows

### 1. Record

```text
1. Record six customers of the demo shop, served by the scripted agent through stretto-proxy
   c1@example.com (cancel): 5 calls
   c2@example.com (status): 4 calls
   ...
   6 session logs in /tmp/stretto-quickstart.Xa9Qz2/logs
```

Each session runs the demo behind the proxy as an MCP host would: `stretto-proxy --record logs --domain shop --context FILE -- stretto-mcp-demo --world retail`. The proxy forwards every message and writes one log per session. The context file holds the conversation, which MCP never carries: the script appends each customer's and agent's line to it, as a host can.

### 2. Learn a flow, with no key

```text
2. Learn a flow from them, with no key
   $ stretto learn --sessions logs --domain shop --habit-only --out shop.flow.json
   stretto: learned the shop flow from 6 sessions (4 tools) and wrote /tmp/stretto-quickstart.Xa9Qz2/shop.flow.json
```

`--habit-only` asks no System-One model. The flow learns which lookup followed which call, and where each lookup's arguments came from.

### 3. Review it

`stretto flow-show` renders the flow for a reviewer. The script prints its tools, sites and bindings; `shop.flow.md` also has the program the flow runs:

```text
   | After | Lookups it may make next (times seen) | What the agent did next in training | With the habit alone |
   |---|---|---|---|
   | `find_user_id_by_email` | `get_user_details` (6) | get_user_details 100% (of 6) | looks up `get_user_details` 1.00 × 0.88 = 0.88 |
   | `get_order_details` | `get_order_details` (6) | get_order_details 50%, respond 50% (of 12) | looks up `get_order_details` 0.50 × 0.93 = 0.46 |
   | `get_user_details` | `get_order_details` (6) | get_order_details 100% (of 6) | looks up `get_order_details` 1.00 × 0.93 = 0.93 |
```

- **Tools**: the three reads, from the server's `readOnlyHint` annotations, are the only tools the flow may call. It never calls `cancel_pending_order`.
- **Sites**: after finding a user, the agent always read their details; after the details, an order; after an order, another order half the time.
- **Bindings**: `get_user_details`' `user_id` came from `find_user_id_by_email`'s whole result (`$`), and `get_order_details`' `order_id` from the list of orders in the user's details (`$.orders[*]`). The chance next to each is how often binding it that way gave the agent's own arguments in training.

### 4. Serve it

```text
4. Serve it to two new customers, and count the calls the agent makes with and without it
   c41@example.com (cancel): 5 calls without the flow, 2 with it
   c42@example.com (status): 4 calls without the flow, 1 with it
   The agent made 3 calls instead of 9. It makes one call per LLM turn, so that
   is 6 fewer LLM turns: the flow's lookups came back with its first call.
   What the flow decided after c41's first call, from its log (served/<session>.flow.jsonl):
   after find_user_id_by_email: looks up get_user_details {"user_id":"user_41"} (1.00 x 0.88)
   after get_user_details: looks up get_order_details {"order_id":"#W41a"} (1.00 x 0.93)
   after get_order_details: looks up get_order_details {"order_id":"#W41b"} (0.99 x 0.93)
   after get_order_details: hands back (get_order_details at 0.01, below 0.3)
```

The proxy serves the flow with `--flow shop.flow.json --flow-decider habit`. After the agent's first call, the flow looked up the customer's details and both orders, and the proxy appended them to that call's result under `--- Also looked up automatically ---`. The agent skipped the calls whose results it already had, and made the cancellation itself: a flow only calls reads. The customers are new, so every argument was bound from this session's results, not remembered.

The probabilities in the log differ from flow-show's pooled shares: served, the habit also conditions on the call before, so after one order it was sure of a second (0.99), and after the second sure of a stop.

## What it does not show

- **An LLM.** The agent follows a fixed plan and only checks which results the flow already brought. A real agent decides from the results, and what a flow saves depends on how it calls tools: the [results](../../docs/results/README.md) measure that on real agents.
- **A real server.** Every customer here is served the same way, so the flow is sure at every step. [The walkthrough](../../docs/walkthrough.md) runs the loop on the official MCP filesystem server, where the habit is unsure after a read, and adds the audit on new sessions and `flow-diff` after learning again.

## Next

- `stretto init --host claude-code|claude-desktop|cursor|vscode --domain NAME -- <server command>` prints the configuration that runs your own MCP server behind the proxy, and the steps from recording to a served flow ([the CLI reference](../../docs/cli.md#stretto-init)).
- `stretto doctor` checks the installation ([docs/install.md](../../docs/install.md)).
