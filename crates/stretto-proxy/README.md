# stretto-proxy

A stdio [MCP](https://modelcontextprotocol.io) proxy that records what an agent does with a server's tools, and can act on it: run a stretto flow behind the agent's calls, check its writes against policy guards, and add a commit tool.

`stretto-proxy` starts the real MCP server as a child process and sits between it and the MCP host (a desktop app, an IDE, an agent framework). It forwards every line in both directions byte for byte and, with `--record`, also writes each line to a session log. `stretto_trace::mcp` turns logs into stretto's canonical `Episode`, so recorded sessions feed the same models as τ²-bench trajectories.

## Usage

```text
stretto-proxy [--record <DIR>] [--domain <NAME>] [--agent-model <MODEL>]
              [--flow <FILE> [--oracle jev|replay|mock] [--oracle-cache <DIR>] [--flow-decider arbiter|habit] ...]
              [--guards [--confirm-judge log|enforce [--confirm-second proposed] ...]]
              [--commit] [--context <FILE>] -- <SERVER_COMMAND>...
```

Install it with `cargo install --path crates/stretto-proxy`, which also installs `stretto-mcp-demo`. In the host's configuration, replace the server's command with `stretto-proxy` and put the original command after `--`:

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
- Without `--record`, the proxy only forwards.

The server inherits the proxy's environment and stderr. The proxy's stdout carries only the protocol. Its own messages go to stderr, prefixed `stretto-proxy:`; with `--record`, the first one says where the log is.

When the host closes the proxy's stdin, the proxy closes the server's stdin, forwards whatever the server still writes, waits for it to exit, and exits with its status (`128 + n` if signal `n` killed it). If the server exits first, the proxy forwards the rest of its output and exits the same way. If the proxy itself fails (the log cannot be created, the server cannot be started), it exits with 125 and leaves no log behind. Usage errors exit with 2.

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

`stretto-mcp-demo --world retail` serves a tiny shop instead, with four of τ²-bench retail's tool names and canned data (`cN@example.com` is `user_N`, whose orders `#WNa` and `#WNb` are pending), for trying active mode.

## Active mode

With any of `--flow`, `--guards`, `--commit` or `--context`, the proxy reads what crosses it, on one thread, and acts on it. Everything it does not act on is still forwarded byte for byte.

- **`--flow FILE`** runs a flow after each of the agent's calls (RFC-001's arm D0). The file is a flow IR from `stretto compile` (τ²-bench results) or `stretto learn` (sessions this proxy recorded). When the server answers one of the agent's `tools/call`s, the proxy holds the response and asks the flow what comes next. While the flow proposes lookups, the proxy makes them itself, as requests with ids `stretto-<n>`. When the flow hands back, the agent gets its own result with one more text item, in the pilot's format:

  ```text
  --- Also looked up automatically (current results; no need to repeat these calls) ---

  get_user_details {"user_id":"user_7"}:
  {"user_id":"user_7","orders":["#W7a","#W7b"],…}
  ```

  A flow only calls tools it reads as lookups and that the server, once it has listed its tools, does not mark `readOnlyHint: false`. One flow runs at a time; a response that arrives meanwhile is forwarded as it is.
  - `--oracle` says who answers the flow's questions: `jev` (the default; needs `TYPESAFE_API_KEY`), `replay` (the cache only) or `mock`. Answers are cached in `--oracle-cache` (default `~/.stretto/oracle-cache`), keyed by the request's hash.
  - `--flow-threshold` (0.3) is the probability the lookup must reach: the tool's, times how often its arguments' binding matched the agent in training.
  - `--flow-decider` says where the tool's probability comes from: `arbiter` (the default: the habit, the System-One model's answers and the predicates, combined) or `habit` (the habit alone). The habit alone asks no one, so it needs no key and adds no latency. Replayed on GLM-5's and Claude 3.7 Sonnet's recorded test episodes, it saved as many turns as the arbiter, with more detours, mostly in airline ([results](../../docs/results/arms-2026-09-24.md)).
  - `--flow-per-call` (8), `--flow-per-session` (40) and `--flow-questions` (300) cap lookups per result, lookups per session and questions per session.
  - Each decision is appended to `--flow-log`, by default `<session>.flow.jsonl` next to the session log.
  - `--task-id` picks the flow's fold; by default it is the session.
- **`--guards`** checks each of the agent's calls against the policy guards of `--domain` (`retail` or `airline`) before the server sees it. A call an enforced rule refuses never reaches the server. The agent gets an error result instead: `Refused by the policy check, so cancel_pending_order was not run and nothing changed: 'found it cheaper' is not an accepted reason (policy check retail.cancel_reason).` A rule that lacks the facts to decide does not refuse. `stretto guards` tests the rules against recorded trajectories.
- **`--confirm-judge log|enforce`** (with `--guards` and `--context`) puts each write the guards check for a confirmation (the rule `retail.confirmed` or `airline.confirmed`) to the System-One model too, with the question `stretto confirm` asks: did the customer's reply explicitly agree to this exact change? The judge sees what the agent said last before the customer's last message, that message and the call, built exactly as `stretto confirm` builds them, so the two share cached answers. The guards' word list for that rule is only logged, since it cannot tell a confirmation from a request; the judge is meant to replace it.
  - `log` records each judgment and refuses nothing the guards would not. `enforce` also refuses a write whose answer puts the probability of a yes below `--confirm-threshold` (0.5): `Refused by the policy check, so cancel_pending_order was not run and nothing changed: the customer has not explicitly agreed to this exact change (confirmation judge: p_yes 0.15); describe the change and ask the customer to confirm it first.`
  - A judge that cannot answer refuses nothing: no key, a cache miss under `--oracle replay`, an error, or the session's `--confirm-questions` (100) spent.
  - `--confirm-second proposed` also asks [the second question](../../docs/results/confirm-second-2026-09-24.md): had the agent proposed this change? A write then fails unless both answers are yes. `described` asks that page's first wording, which is too literal.
  - `--oracle` and `--oracle-cache` are the ones `--flow` uses.
  - Each judgment is appended to `--confirm-log`, by default `<session>.confirm.jsonl` next to the session log: the call, the word list's verdict, each answer, whether the write failed and whether that was enforced, the questions' cache keys, and how long the write waited for the judge.
  - Offline, enforcing the first question would refuse 5–11% of the writes accepted in τ²-bench's successful episodes, most of them real lapses ([results](../../docs/results/confirm-2026-09-24.md)). Whether that costs passes or turns live is [#6](https://github.com/alexnodeland/stretto/issues/6).
- **`--commit`** adds `stretto_commit` to the tools the server lists. It takes `{"calls": [{"name", "arguments"}, …]}` and makes the calls in order, each checked by the guards first. It stops at the first call that is refused or fails, and returns every call's result and the ones it did not run. It is meant for the writes the user has confirmed, in one LLM turn.
- **`--context FILE`** gives the proxy the conversation, which MCP never carries. The host appends one JSON line per message, `{"role": "user" | "assistant", "content": text}`. Before each tool call and each decision, the proxy logs the new lines as `context` entries, so flows see what the customer said and guards can read their confirmation.

The proxy's own messages (its requests to the server, and its answers to the agent: refusals, commit results, results with a flow's lookups) are logged as `proxy` entries. `stretto_trace::mcp::episode` counts the first response to each call as its result, so a flow's lookups appear as calls of their own, with ids starting `stretto-`.

## Log format

A log is JSONL. The first line is a header:

```json
{"stretto_mcp_log":2,"session":"20260923T212000.123Z-4242","started_unix_ms":1790198400123,"server_command":["npx","-y","some-mcp-server"],"domain":"orders","agent_model":null}
```

`session` is the UTC start time and the proxy's process id, and names the file. Every other line is one line from the wire:

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
- **LLM turns.** They are inferred from timing. A call sent while an earlier call of the current turn still awaits its response joins that turn (parallel calls); any other call starts a new turn. A host that runs one turn's calls one at a time shows one turn per call. A call that is never answered, nor cancelled, keeps its turn open, so later calls join it.
- **Outcomes, tokens and cost.** Whether the task succeeded is unknown: `reward` is 0.0 until you set it, and usage is empty.
- **The model.** Unless you pass `--agent-model`, episodes name the host application from `initialize` (`clientInfo.name`), not the LLM.
- **Other servers.** One proxy wraps one server; an agent with several servers leaves one log per wrapped server.

## Limits

- Only the stdio transport; Streamable HTTP servers are not supported.
- The proxy does not forward signals. If the host kills the proxy, the server's stdin closes, which is how MCP tells a stdio server to exit; a server that ignores that keeps running.

## Privacy

Logs hold every tool call and result verbatim: personal data, documents, whatever the tools read or return. Treat them like the data the tools touch, and never commit them.

The header's `server_command` is the command after `--`, with the values of credential-looking arguments replaced by `<redacted>` (`--api-key X`, `--token=X`, `GITHUB_TOKEN=X`, `Authorization: Bearer X`). That is best effort, so pass credentials to servers through `env`: the proxy never records its environment.
