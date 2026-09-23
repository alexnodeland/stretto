# stretto-proxy

A stdio [MCP](https://modelcontextprotocol.io) proxy that records what an agent does with a server's tools.

`stretto-proxy` starts the real MCP server as a child process and sits between it and the MCP host (a desktop app, an IDE, an agent framework). It forwards every line in both directions byte for byte and, with `--record`, also writes each line to a session log. `stretto_trace::mcp` turns logs into stretto's canonical `Episode`, so recorded sessions feed the same models as τ²-bench trajectories.

## Usage

```text
stretto-proxy [--record <DIR>] [--domain <NAME>] [--agent-model <MODEL>] -- <SERVER_COMMAND>...
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

## Log format

A log is JSONL. The first line is a header:

```json
{"stretto_mcp_log":1,"session":"20260923T212000.123Z-4242","started_unix_ms":1790198400123,"server_command":["npx","-y","some-mcp-server"],"domain":"orders","agent_model":null}
```

`session` is the UTC start time and the proxy's process id, and names the file. Every other line is one line from the wire:

```json
{"t_ms":12,"from":"client","message":{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"hello"}}}}
{"t_ms":14,"from":"server","message":{"id":3,"jsonrpc":"2.0","result":{"content":[{"text":"{\"text\":\"hello\"}","type":"text"}],"isError":false}}}
{"t_ms":15,"from":"server","raw":"Listening on stdio"}
```

- `t_ms`: when the proxy read the line, in milliseconds after it started. Times never decrease down the log.
- `from`: `client` (the host) or `server`.
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

- **The conversation.** User messages and the LLM's replies never reach an MCP server. Episodes from logs have no user events, and assistant turns have no text.
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
