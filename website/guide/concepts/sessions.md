---
description: How stretto-proxy records an agent's sessions with an MCP server, what a session log holds, and what the proxy cannot see.
---

# Sessions and recording

Everything stretto learns comes from sessions: what an agent did with a server's tools, recorded by `stretto-proxy` as it happened.

## The proxy

`stretto-proxy` takes the place of an MCP server in the host's configuration. The host starts it as a stdio server; it starts the real server as a child process, from the command after `--`, or connects to one over Streamable HTTP ([`--upstream`](/integrations/streamable-http)). It forwards every line in both directions byte for byte, except what you ask it to act on, such as a result a flow adds its lookups to.

```sh
stretto-proxy --record ~/.stretto/logs/orders --domain orders \
  -- npx -y some-mcp-server
```

`stretto init` prints this for your host, in the host's own format ([integrations](/integrations/)).

- `--record DIR` writes one log per session, `DIR/<session>.jsonl`, and creates `DIR` if it is missing. Keep one directory per server, since `stretto learn` reads every session in a directory; `stretto init` uses `~/.stretto/logs/NAME`. Without `--record`, the proxy only forwards.
- `--domain NAME` names the domain in the log's header, and later the flow's.
- `--agent-model MODEL` names the model that drives the agent, which the proxy cannot see. Without it, the log names the host application from `initialize`.

Its stdout carries only the protocol. Its own messages go to stderr, prefixed `stretto-proxy:`, and the first says where the log is. It exits with the server's status, or with 125 if the proxy itself fails. When nothing but recording is asked of it, it parses nothing it forwards. A writer thread writes the log off the forwarding path, so forwarding never waits for the disk.

## What a session log holds

A log is JSON lines. The first line is a header:

```json
{"stretto_mcp_log":2,"session":"20260923T212000.123Z-4242","started_unix_ms":1790198400123,"server_command":["npx","-y","some-mcp-server"],"domain":"orders","agent_model":null}
```

Every other line is one line from the wire, with when the proxy read it (`t_ms`, milliseconds since it started) and who sent it (`from`): `client` (the host), `server`, and, when the proxy acts, `proxy` (its own requests) and `context` (the conversation):

```json
{"t_ms":12,"from":"client","message":{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"hello"}}}}
{"t_ms":14,"from":"server","message":{"id":3,"jsonrpc":"2.0","result":{"content":[{"text":"{\"text\":\"hello\"}","type":"text"}],"isError":false}}}
```

So a log holds every tool call and result verbatim: whatever the tools read or return. Keep logs where that data may live, and never commit them. [Privacy and redaction](./privacy) covers retention and pseudonymized copies. The [log format](/reference/proxy#log-format) documents every field.

## Tool kinds

A flow only ever calls tools that read. It learns which ones do from the server's own `tools/list` response, recorded in the first session: a tool annotated `readOnlyHint: true` reads, one annotated `false` writes, and one with no hint is *generic*, which a flow never calls. Annotations are the server's claims, so check them against what the tools do ([reviewing a flow](/reference/formats#reviewing-a-flow)).

If a server gives no hints, write a manifest and pass it to `stretto learn --manifest`:

```json
{"domain": "notes", "tools": {"search_files": "read", "read_text_file": "read", "write_file": "write"}}
```

## The conversation

MCP carries tool calls, not the conversation. `--context FILE` gives the proxy the conversation from a file the host appends to, one JSON line per message:

```json
{"role": "user", "content": "Summarize the October meetings."}
```

With it, a flow can prefer a value the user mentioned when it binds a lookup's arguments, and the logs hold the user's turns. Without it, a flow still works. The proxy reads the file from its start, so give each session its own file, or empty it when a session starts. The MCP hosts on the [integrations](/integrations/) pages do not write such a file themselves; a harness that drives the agent can, as [the pilots'](../../../pilot/run_episode.py) does.

## What the proxy cannot see

- **LLM turns.** They are inferred from timing: a call sent while an earlier call of the turn still awaits its response joins that turn; any other call starts a new one.
- **Outcomes, tokens and cost.** Whether a session succeeded is unknown. `stretto learn --rewards FILE` takes rewards by session id; a session without one counts as successful.
- **Other servers.** One proxy wraps one server. An agent with several servers leaves one log per wrapped server, and a flow learns from one server's calls.

## How many sessions

A flow only knows what the sessions showed: which lookup followed which call, and where each lookup's arguments came from. Which sessions matter more than how many, so record the kinds of request the agent will see; a request type never recorded gets no help. Replayed on τ²-bench, ten of an agent's own sessions gave 96% (retail) and 93% (airline) of what all of them did, and telecom needed about thirty ([the paper, §4.3](/research/paper#43-learning-from-few-sessions)).

## Try it

`stretto-mcp-demo` is a tiny server for trying the proxy: `lookup` (read-only) and `update` (a write) answer with their arguments. `stretto-mcp-demo --world retail` serves a tiny shop with four of τ²-bench retail's tool names and canned data instead. [The quick start's demo](../quick-start#2-see-the-whole-loop-with-no-key) records six sessions on that shop and learns a flow from them, and [the proxy's reference](/reference/proxy#try-it) records one from the shell.

[The console](../console) lists every recorded session, by domain and mode, and shows one as a timeline: the conversation, each call with its arguments and result, and the lookups the flow made after it.

## Related

- [`stretto-proxy` reference](/reference/proxy) and its [options](/reference/cli#stretto-proxy)
- [Integrations](/integrations/): the configuration for each host
- [Flows](./flows): what `stretto learn` makes of the sessions
- [The console](../console): every session, as a timeline
