# stretto-proxy

An [MCP](https://modelcontextprotocol.io) proxy that records what an agent does with a server's tools, and can act on it: run a stretto flow behind the agent's calls, check its writes against policy guards, and add a commit tool.

`stretto-proxy` starts the real MCP server as a child process, or connects to it over Streamable HTTP (`--upstream`), and sits between it and the MCP host (a desktop app, an IDE, an agent framework). The host runs the proxy as a stdio server, or connects to it over Streamable HTTP ([`--listen`](#hosts-over-http)). It forwards every line in both directions byte for byte and, with `--record`, also writes each line to a session log. `stretto_trace::mcp` turns logs into stretto's canonical `Episode`, so recorded sessions feed the same models as τ²-bench trajectories.

## Usage

```text
stretto-proxy [--record <DIR>] [--domain <NAME>] [--agent-model <MODEL>]
              [--flow <FILE> [--oracle jev|replay|mock] [--oracle-cache <DIR>] [--flow-decider arbiter|habit|reach] ...]
              [--guards [--confirm-judge log|enforce [--confirm-second proposed] ...]]
              [--commit] [--context <FILE>] -- <SERVER_COMMAND>...
stretto-proxy [the same options] --upstream <URL> [--upstream-header NAME=VAR]...
stretto-proxy [the same options] --listen <ADDR> [--listen-token-file <FILE>] [--listen-idle <MINUTES>] ...
```

Install it with `cargo install --path crates/stretto-proxy`, which also installs `stretto-mcp-demo` and `stretto-procedure`, which runs a compiled procedure against an MCP server with no model ([formats](../../docs/formats.md#the-procedure-ir-stretto_procedure-1), [CLI](../../docs/cli.md)). In the host's configuration, replace the server's command with `stretto-proxy` and put the original command after `--`:

```json
{
  "mcpServers": {
    "orders": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs", "--domain", "orders", "--", "npx", "-y", "some-mcp-server"],
      "env": { "SOME_API_KEY": "…" }
    }
  }
}
```

- `--record DIR` writes one log per session to `DIR/<session>.jsonl`, creating `DIR` if needed. A leading `~` is expanded, because hosts start servers without a shell. Hosts start servers in a directory of their choosing, so use an absolute path (or `~`).
- `--domain` and `--agent-model` are copied into the log header. The proxy cannot see which model drives the agent, so name it here if you know it.
- `--server-name` names the server in the log header, for an agent with several servers ([below](#several-servers)).
- Without `--record`, the proxy only forwards.

**Streamable HTTP servers.** `--upstream https://example.com/mcp` proxies a server that speaks MCP's HTTP transport (revision 2025-06-18), in place of a command:

```json
"args": ["--record", "~/.stretto/logs", "--domain", "orders",
         "--upstream", "https://example.com/mcp", "--upstream-header", "Authorization=ORDERS_AUTH"],
"env": { "ORDERS_AUTH": "Bearer …" }
```

- Each message the host sends is POSTed to the endpoint. The server's answer comes back to the host as lines, whether it is one JSON body or an event stream: progress notifications, log messages, then the result. The session id the server assigns (`Mcp-Session-Id`) and the agreed protocol version (`MCP-Protocol-Version`) go with every later request.
- After `notifications/initialized`, a GET stream carries the messages the server sends on its own; it is opened again if it drops. When the host closes the proxy's stdin, the proxy waits for the requests in flight and ends the session with a DELETE.
- `--upstream-header NAME=VAR` sends header `NAME` with the value of environment variable `VAR`, such as a bearer token, on every request. Values are never logged. The log header records the URL without a user, a password, or the values of query parameters that look like credentials.
- A request the server refuses, or cannot be reached for, is answered with a JSON-RPC error, so the host is not left waiting. The proxy exits with 0 once the session has ended.
- Recording, flows, guards and `stretto_commit` work as they do with a command. CI runs the proxy in front of the reference server, `@modelcontextprotocol/server-everything` ([`scripts/http_check.py`](../../scripts/http_check.py)), and the tests run it in front of `stretto-mcp-demo --http`, which insists on the session id and version.

The server inherits the proxy's environment and stderr. The proxy's stdout carries only the protocol. Its own messages go to stderr, prefixed `stretto-proxy:`; with `--record`, the first one says where the log is.

When the host closes the proxy's stdin, the proxy closes the server's stdin, forwards whatever the server still writes, waits for it to exit, and exits with its status (`128 + n` if signal `n` killed it). If the server exits first, the proxy forwards the rest of its output and exits the same way. If the proxy itself fails (the log cannot be created, the server cannot be started), it exits with 125 and leaves no log behind. Usage errors exit with 2.

## Several servers

One proxy wraps one server, so an agent with several servers leaves one log per server in each session. `stretto learn` makes one session of the logs one host session left, so that a flow learns that a lookup on one server follows a call on another, such as a docs search after a ticket is read.

- **Record them together.** Give every server's proxy the same `--record` directory: `stretto learn` reads one directory.
- **One host session.** Each proxy records the host session it ran in: `STRETTO_SESSION` when the host's environment sets it, else the host's process (`host-<pid>`, on Unix), which every server the host starts shares. Set `STRETTO_SESSION` on Windows, and when the host starts each server through a shell of its own. The logs of one host session that ran at the same time are one session.
- **Server names.** Each tool is named after its server, `docs::search`: the proxy's `--server-name`, else its `--domain`, else the name the server gives itself in `initialize`. Two servers of the same name at once cannot be told apart, so `learn` keeps such a host session's logs apart and says so; name the servers with `--server-name`.

```json
{
  "mcpServers": {
    "tickets": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/support", "--server-name", "tickets", "--", "npx", "-y", "some-tickets-server"]
    },
    "docs": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/support", "--server-name", "docs", "--", "npx", "-y", "some-docs-server"]
    }
  }
}
```

Then `stretto learn --sessions ~/.stretto/logs/support --domain support --out support.flow.json` learns one flow across both, and `promote`, `audit` and `drift` read the sessions the same way:

- The merged session's lines are the logs' lines in the order the proxies read them, timed to the millisecond, and its LLM turns are inferred across the servers. Calls to two servers less than a couple of milliseconds apart may be merged in either order; they are one LLM turn's anyway.
- The conversation, which each proxy reads from the host's `--context` file, is kept once.
- A call a proxy's log ended without an answer to, when its server died, say, is cancelled there, so that it does not hold the other servers' turns open.
- Once a directory holds a host session of several servers, the tools of every other log in it are named after their server too, so that a tool has one name across the sessions.

**Serving.** Give every proxy the same flow. Each proxy looks up only its own server's tools, calling them by their own names: the proxies share no connection, so a lookup on another server is left to the agent. Each also decides from its own server's calls alone, since it sees no other, while the flow learned from whole sessions. A proxy whose server's name is none of the flow's says so, and looks nothing up.

## Hosts over HTTP

With `--listen ADDR`, the proxy serves hosts over MCP's Streamable HTTP transport (revision 2025-11-25) instead of stdio: for hosts that connect to servers by URL, remote agents, and agents in another container. Start it once, and give hosts the URL it prints:

```bash
stretto-proxy --listen 127.0.0.1:8931 --record ~/.stretto/logs --domain orders -- npx -y some-mcp-server
# http://127.0.0.1:8931/mcp
claude mcp add --transport http orders http://127.0.0.1:8931/mcp
```

VS Code takes it as `{"type": "http", "url": "http://127.0.0.1:8931/mcp"}`, and Cursor as `{"url": "http://127.0.0.1:8931/mcp"}`. `stretto init --listen ADDR` prints each host's configuration by URL, with the command that starts the proxy ([one proxy for every host](https://stretto.alexnodeland.com/integrations/#one-proxy-for-every-host)); so does the console, for a server set to it.

- **Sessions.** Each MCP session (`Mcp-Session-Id`, assigned on `initialize`) gets a server of its own (the command, or its own session of `--upstream`), its own log (`<start>-<pid>-<n>.jsonl`), and its own run of the flow, the guards and the judge, as one stdio proxy has. A DELETE ends the session and closes its server's input, as a stdio host does when it exits. So does the server's exiting, and `--listen-idle` minutes (240 by default) with no request and no open stream; the host then gets 404 and starts another. SIGTERM or Ctrl-C ends every session and waits up to 10 seconds for their logs to close.
- **Streams.** A request is answered on an event stream, which carries what the server sends while the request waits, then the answer. A host that accepts only `application/json` gets the answer as one body. The server's own messages go on the host's GET stream when one is open, else on a waiting request's stream, else wait for one (1,000 at most). The proxy keeps no history of events, so a stream that drops is not resumed.
- **The host session.** The `Stretto-Session` header on `initialize` names it, as `STRETTO_SESSION` does over stdio, so that `stretto learn` merges the logs of one host session ([several servers](#several-servers)). `{session}` in `--context`, `--flow-log` and `--confirm-log` stands for it, else for the session's own id: a conversation file per session.
- **Who may connect.** Without `--listen-token-file`, the proxy listens only on a loopback address, answers only requests whose Host header names that address and port, and refuses pages on other sites by their Origin (403), which keeps DNS rebinding out. With it, every request needs `Authorization: Bearer <token>` (401 otherwise), and the proxy may listen on any address; put TLS in front of it on a network. The token is read from the file, so it is not in the process list.
- `--retain-days` prunes as each session starts, since the proxy outlives its sessions. At most 64 sessions are open at once (503 beyond that), and a message is at most 4 MiB.

## Try it

`stretto-mcp-demo` is a tiny MCP server for trying the proxy. It has two tools that store nothing and answer with their arguments: `lookup`, annotated read-only, and `update`, annotated as not read-only. `"fail": true` makes either return an error result, and `"delay_ms": n` makes it wait before answering, to try parallel calls.

```bash
cargo build -p stretto-proxy
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"hello"}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"update","arguments":{"text":"hi","fail":true}}}' \
  | target/debug/stretto-proxy --record /tmp/stretto-logs -- target/debug/stretto-mcp-demo
cat /tmp/stretto-logs/*.jsonl
```

To try it in a host, configure `stretto-proxy` as above with `stretto-mcp-demo` after `--`.

`stretto-mcp-demo --world retail` serves a tiny shop instead, with four of τ²-bench retail's tool names and canned data (`cN@example.com` is `user_N`, whose orders `#WNa` and `#WNb` are pending), for trying active mode. [The walkthrough](../../docs/walkthrough.md) runs the proxy on a real server, the official MCP filesystem server.

## Active mode

With any of `--flow`, `--guards`, `--commit` or `--context`, the proxy reads what crosses it, on one thread, and acts on it. Everything it does not act on is still forwarded byte for byte.

- **`--flow FILE`** runs a flow after each of the agent's calls (RFC-001's arm D0). The file is a flow IR from `stretto compile` (τ²-bench results) or `stretto learn` (sessions this proxy recorded). When the server answers one of the agent's `tools/call`s, the proxy holds the response and asks the flow what comes next. While the flow proposes lookups, the proxy makes them itself, as requests with ids `stretto-<n>`. When the flow hands back, the agent gets its own result with one more text item, in the pilot's format:

  ```text
  --- Also looked up automatically (current results; no need to repeat these calls) ---

  get_user_details {"user_id":"user_7"}:
  {"user_id":"user_7","orders":["#W7a","#W7b"],…}
  ```

  A flow only calls tools it reads as lookups and that the server, once it has listed its tools, does not mark `readOnlyHint: false`. A flow learned from recorded sessions pins each tool's input contract (its arguments, their types and which are required), and the proxy makes no lookup of a tool whose server now lists another: the flow learned to bind arguments that are not there any more, so the agent makes that call itself. `--flow-tools` names the only tools the flow may call on its own: a server's `readOnlyHint` says a call changes nothing, not that it is free, unlogged, or fine to make unasked, since a read can be metered, rate-limited, or recorded as an access. One flow runs at a time; a response that arrives meanwhile is forwarded as it is.
  - `--oracle` says who answers the flow's questions: `jev` (the default; needs `TYPESAFE_API_KEY`, or `TYPESAFE_API_KEY_FILE` naming a file that holds it), `replay` (the cache only) or `mock`. Answers are cached in `--oracle-cache` (default `~/.stretto/oracle-cache`), keyed by the request's hash.
  - `--flow-threshold` (0.3) is the probability the lookup must reach: the tool's, times how often its arguments' binding matched the agent in training.
  - `--flow-decider` says where the tool's probability comes from: `arbiter` (the habit, the System-One model's answers and the predicates, combined), `reach` (how often the agent made the lookup before its next write, counted in training) or `habit` (how often it made it next). By default a flow is served with its arbiter, and a flow without one, such as every flow `learn --habit-only` writes, with `reach`. `reach` and `habit` ask no one, so they need no key and add no latency. Replayed on GLM-5's and Claude 3.7 Sonnet's recorded test episodes, the habit alone saved as many turns as the arbiter, with more detours, mostly in airline ([results](../../docs/results/arms-2026-09-24.md)); `reach` is the better calibrated of the two ([results](../../docs/results/reach-2026-09-26.md)).
  - `--flow-per-call` (8), `--flow-per-session` (40) and `--flow-questions` (300) cap lookups per result, lookups per session and questions per session.
  - Each decision is appended to `--flow-log`, by default `<session>.flow.jsonl` next to the session log, with its site in the flow's run (`address`, such as `decide#0`). The run is the fugue program the flow holds ([formats](../../docs/formats.md#program)).
  - `--flow-explore EPSILON` explores (RFC-001 §3.7): with that probability, the flow takes a lookup other than the rule's choice, drawn by the decider's probabilities among the lookups that bind. Each decision then logs its `policy`: every lookup the site offered (the decider's and the habit's probabilities, the binding's chance and arguments), the rule's own choice (`greedy`), and the chance that the flow took what it took (`propensity`). `stretto evaluate` reads those, labelled, to estimate what another rule would have done. `--flow-explore 0` explores nothing and still logs them; `--flow-explore-seed` seeds the draws.
  - After each run's decisions comes one line for the run itself, `run`:
    - the program's data: `call`, `failed` and `max_lookups`;
    - each site as `[address, value, logp]`;
    - `surprise`: how unexpected the server's answers were under the flow's statistics, in nats.

    Given the data, the flow's program scores the run again with fugue's `ScoreGivenTrace`.
  - `--flow-shadow` runs the flow in shadow mode (RFC-001 §3.7): it decides after each call and logs what it would look up, with `"shadow": true`, but makes no lookups, so the agent gets the server's results unchanged. `stretto promote --sessions` then makes the same decisions again from the answers the proxy cached (pass its `--oracle-cache`), scores them against what the agent did, and writes a flow that acts only where its lookups were the agent's own ([promotion](../../docs/results/promotion-2026-09-25.md)).
  - A promoted flow hands back after a call whose site was not promoted, with the reason `the site is not promoted`.
  - `--task-id` picks the flow's fold; by default it is the session.
- **`--guards`** checks each of the agent's calls against the policy guards of `--domain` (`retail` or `airline`) before the server sees it. A call an enforced rule refuses never reaches the server. The agent gets an error result instead: `Refused by the policy check, so cancel_pending_order was not run and nothing changed: 'found it cheaper' is not an accepted reason (policy check retail.cancel_reason).` A rule that lacks the facts to decide does not refuse. The facts are what the session has shown the agent, the lookups a flow appended to a result included, whether the proxy's own flow made them or a flow the server runs. `stretto guards` tests the rules against recorded trajectories.
- **`--confirm-judge log|enforce`** (with `--guards` and `--context`) puts each write the guards check for a confirmation (the rule `retail.confirmed` or `airline.confirmed`) to the System-One model too, with the question `stretto confirm` asks: did the customer's reply explicitly agree to this exact change? The judge sees what the agent said last before the customer's last message, that message and the call, built exactly as `stretto confirm` builds them, so the two share cached answers. The guards' word list for that rule is only logged, since it cannot tell a confirmation from a request; the judge is meant to replace it.
  - `log` records each judgment and refuses nothing the guards would not. `enforce` also refuses a write whose answer puts the probability of a yes below `--confirm-threshold` (0.5): `Refused by the policy check, so cancel_pending_order was not run and nothing changed: the customer has not explicitly agreed to this exact change (confirmation judge: p_yes 0.15); describe the change and ask the customer to confirm it first.`
  - A judge that cannot answer refuses nothing: no key, a cache miss under `--oracle replay`, an error, or the session's `--confirm-questions` (100) spent.
  - `--confirm-second proposed` also asks [the second question](../../docs/results/confirm-second-2026-09-24.md): had the agent proposed this change? A write then fails unless both answers are yes. `described` asks that page's first wording, which is too literal. With `--confirm-second-shadow`, the second answer is only logged (`"second_shadow": true`), and a write fails on the first answer alone.
  - `--oracle` and `--oracle-cache` are the ones `--flow` uses.
  - Each judgment is appended to `--confirm-log`, by default `<session>.confirm.jsonl` next to the session log: the call, the word list's verdict, each answer, whether the write failed and whether that was enforced, the questions' cache keys, and how long the write waited for the judge. Beside the judge, and logged only, `proposal_check` lists the call's values whose record the confirmation chose another of, with no model: another order, item or card of the same list than the one the customer's reply, or the agent's proposal, named ([the check](../../docs/results/proposal-check-2026-09-26.md)).
  - Offline, enforcing the first question would refuse 5–11% of the writes accepted in τ²-bench's successful episodes, most of them real lapses ([results](../../docs/results/confirm-2026-09-24.md)). Live, enforced on 20 episodes, it refused nothing; logged on 20 more, it would have stopped 2 real lapses for 2 false alarms, and passes did not move. The second question's flags were 7 false in 8. So the recommended setting is `--confirm-judge enforce --confirm-second proposed --confirm-second-shadow`. The judge runs only when asked for, since it needs a Jev key ([results](../../docs/results/judge-live-2026-09-25.md)).
- **`--commit`** adds `stretto_commit` to the tools the server lists. It takes `{"calls": [{"name", "arguments"}, …]}` and makes the calls in order, each checked by the guards first. It stops at the first call that is refused or fails, and returns every call's result and the ones it did not run. It is meant for the writes the user has confirmed, in one LLM turn. A sentence saying so follows the server's own `instructions` in its answer to `initialize`, since hosts such as Claude Code put those in the system prompt, and a tool alone does not tell the agent when to use it.
- **`--context FILE`** gives the proxy the conversation, which MCP never carries. The host appends one JSON line per message, `{"role": "user" | "assistant", "content": text}`. Before each tool call and each decision, the proxy logs the new lines as `context` entries, so flows see what the customer said and guards can read their confirmation.

The proxy's own messages (its requests to the server, and its answers to the agent: refusals, commit results, results with a flow's lookups) are logged as `proxy` entries. `stretto_trace::mcp::episode` counts the first response to each call as its result, so a flow's lookups appear as calls of their own, with ids starting `stretto-`.

## Log format

A log is JSONL. The first line is a header:

```json
{"stretto_mcp_log":2,"session":"20260923T212000.123Z-4242","started_unix_ms":1790198400123,"server_command":["npx","-y","some-mcp-server"],"domain":"orders","agent_model":null,"host_session":"host-4211"}
```

`session` is the UTC start time and the proxy's process id, and names the file. `host_session` is the host session the proxy ran in, and `server_name`, when given, is `--server-name` ([several servers](#several-servers)). Every other line is one line from the wire:

```json
{"t_ms":12,"from":"client","message":{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"hello"}}}}
{"t_ms":14,"from":"server","message":{"id":3,"jsonrpc":"2.0","result":{"content":[{"text":"{\"text\":\"hello\"}","type":"text"}],"isError":false}}}
{"t_ms":15,"from":"server","raw":"Listening on stdio"}
```

- `t_ms`: when the proxy read the line, in milliseconds after it started. Times never decrease down the log.
- `from`: `client` (the host) or `server`; in active mode also `proxy` (a message the proxy sent on its own) and `context` (a message of the conversation from `--context`).
- `message`: the line exactly as it was sent, when it is one JSON value: a JSON-RPC message, or an array of them (a batch).
- `raw`: the line as text, when it is not valid JSON (invalid UTF-8 is replaced by U+FFFD).

Lines appear in the order the proxy read them. Each is recorded just before it is forwarded, so a request always comes before its response. A writer thread writes each line out as soon as it gets it, off the forwarding path, so forwarding never waits for the disk, and a crash loses at most the lines still in flight.

## Reading logs

```rust
use std::path::Path;
use stretto_trace::mcp::{episode, manifest, read_log};

let log = read_log(Path::new("/home/me/.stretto/logs/20260923T212000.123Z-4242.jsonl"))?;
let mut ep = episode(&log);
ep.reward = 1.0; // if you know the task was solved
let tools = manifest(&log, &ep.domain); // tool kinds from `readOnlyHint`
```

Each `tools/call` becomes a `ToolCall`, and each response to one becomes a `ToolResult`: an error for a JSON-RPC error or `isError: true`; its content is the result's text. `manifest` reads tool kinds from the `readOnlyHint` annotation. `true` is `Read`, `false` is `Write`, and no hint is `Generic`, which here means *unknown*. Annotations are the server's claims, not guarantees.

## What the proxy cannot see

- **The conversation.** User messages and the LLM's replies never reach an MCP server. Without `--context`, episodes from logs have no user events, and assistant turns have no text.
- **LLM turns.** They are inferred from timing. A call sent while an earlier call of the current turn still awaits its response joins that turn (parallel calls). So does a call sent within half a second of the turn's last response, with nothing said in between, unless it passes a value that first appeared in what the turn returned: Claude Code runs a turn's reads as the model streams them and its writes one after another, and a new LLM turn takes longer, as the model reads the results first ([the measurement](../../docs/results/turns-2026-09-28.md)). The proxy's own lookups join a turn only while a call of it awaits its response. Any other call starts a new turn. A call that is never answered, nor cancelled, keeps its turn open, so later calls join it.
- **Calls still on their way.** When one call of a turn returns, the flow decides at that call's site, and the turn's calls the host has sent but that have not returned count as asked for already: the flow does not look them up. Calls a streaming host sends later, the proxy cannot know of.
- **Outcomes, tokens and cost.** Whether the task succeeded is unknown: `reward` is 0.0 until you set it, and usage is empty.
- **The model.** Unless you pass `--agent-model`, episodes name the host application from `initialize` (`clientInfo.name`), not the LLM.
- **Other servers.** One proxy wraps one server, and sees only its calls. `stretto learn` merges the logs a host session left with several servers ([several servers](#several-servers)), but a proxy serving a flow learned so decides from its own server's calls alone.

## Limits

- With `--listen`, the proxy keeps no history of events: an event stream that drops is not resumed (`Last-Event-ID`), and what it would have carried is lost.
- With `--upstream`, a session the server ends (a 404 on its id) is not started again: each request after it fails with a JSON-RPC error until the host restarts the proxy, or, with `--listen`, starts another session.
- The proxy does not forward signals. If the host kills the proxy, the server's stdin closes, which is how MCP tells a stdio server to exit; a server that ignores that keeps running.

## Privacy

Logs hold every tool call and result verbatim: personal data, documents, whatever the tools read or return. Treat them like the data the tools touch, and never commit them. [docs/privacy.md](../../docs/privacy.md) lists what each file holds and what is sent to the System-One model.

`--retain-days N` deletes, when the proxy starts, what is older than N days in `--record` and `--oracle-cache`. `stretto redact` writes a pseudonymized copy of sessions to share, from which `stretto learn` learns the same flow.

The header's `server_command` is the command after `--`, with the values of credential-looking arguments replaced by `<redacted>` (`--api-key X`, `--token=X`, `GITHUB_TOKEN=X`, `Authorization: Bearer X`). That is best effort, so pass credentials to servers through `env`: the proxy never records its environment.
