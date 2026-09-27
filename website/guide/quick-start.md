---
description: Install stretto, put the proxy in front of an MCP server, learn a flow from recorded sessions with no key, review it, and serve it.
---

# Quick start

This page puts a flow behind an agent: install stretto, record a few sessions through the proxy, learn a flow from them, review it, and serve it. It uses [the official MCP filesystem server](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) on a folder of notes, as [the walkthrough](./walkthrough) does, but any MCP server works the same way.

You need:

- Rust 1.87 or later ([rustup](https://rustup.rs)), to install from source;
- an MCP host, such as Claude Code, Claude Desktop, Cursor or VS Code;
- Node 18 or later, for this page's example server (`npx`).

No API key: everything below learns and decides without a model.

<!-- BRAND SLOT: the recorded walkthrough (website/public/media/walkthrough.mp4, poster media/walkthrough-poster.png). Renders nothing until the file is there. -->
<BrandEmbed kind="walkthrough" caption="The walkthrough, recorded: the whole loop on the MCP filesystem server." />

## 1. Install

```sh
cargo install --git https://github.com/alexnodeland/stretto \
  stretto-proxy stretto-report
```

This builds from the `main` branch and installs `stretto`, `stretto-proxy`, `stretto-procedure` and `stretto-mcp-demo` into `~/.cargo/bin`. Check that your shell finds them:

```sh
stretto --version
stretto-proxy --version
```

[Installation](./installation) has the other ways to install, and what each package installs.

## 2. See the proxy record, with no host

`stretto-mcp-demo` is a tiny MCP server with two tools that answer with their arguments: `lookup`, marked read-only, and `update`, marked as a write. Send it a few JSON-RPC messages through the proxy:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lookup","arguments":{"text":"hello"}}}' \
  | stretto-proxy --record /tmp/stretto-demo -- stretto-mcp-demo
cat /tmp/stretto-demo/*.jsonl
```

The server's answers come out on stdout, unchanged, and the proxy says on stderr where it records (`stretto-proxy: recording to /tmp/stretto-demo/<session>.jsonl`). The log starts with a header, then holds every line that crossed the proxy, with when and from whom:

```json
{"stretto_mcp_log":2,"session":"20260927T132150.035Z-17076","started_unix_ms":1790515310035,"server_command":["stretto-mcp-demo"],"domain":null,"agent_model":null}
{"t_ms":2,"from":"client","message":{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"0"}}}}
```

[The log format](/reference/proxy#log-format) documents every field.

## 3. Put the proxy in front of your server

In your MCP host's configuration, put `stretto-proxy` where the server's command was, and the server's command after `--`. For the filesystem server, in the JSON that Claude Code (`.mcp.json`), Claude Desktop and Cursor read:

```json
{
  "mcpServers": {
    "notes": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/me/notes"]
    }
  }
}
```

- `--record` writes one log per session into `~/.stretto/notes/`. The proxy expands a leading `~` in its own paths, since hosts start servers without a shell. The server's arguments after `--` are passed as written, so give the notes folder as an absolute path.
- `--domain` names the flow's domain; any name will do.

[Integrations](/integrations/) has the exact file and format for each host, and the variant for servers you reach over HTTP.

Now use the agent as you normally would. Each session leaves a log. Record the kinds of request the agent will see: a flow only knows what the sessions showed, and a request type never recorded gets no help. Replayed on τ²-bench, ten of an agent's own sessions gave 96% (retail) and 93% (airline) of what all of its sessions did ([the claims](/research/claims)).

::: tip Tool kinds
A flow only calls tools the server marks `readOnlyHint: true`. The filesystem server marks all 14 of its tools: 10 read, such as `search_files` and `read_text_file`, and 4 write. If your server gives no hints, write [a manifest](./concepts/sessions#tool-kinds) and pass it to `stretto learn --manifest`.
:::

## 4. Learn a flow

```sh
stretto learn --sessions ~/.stretto/notes --domain notes --habit-only --out ~/.stretto/notes.flow.json
```

```text
stretto: learned the notes flow from 8 sessions (14 tools) and wrote notes.flow.json
```

`--habit-only` asks no model: the flow learns from counts alone. It records which lookup followed which call, how often, and where each lookup's arguments came from in an earlier result.

## 5. Review it

A flow makes calls on the agent's behalf, so read it before you serve it:

```sh
stretto flow-show ~/.stretto/notes.flow.json
```

It lists the tools the flow may call (only the read tools), the lookups it may make after each call, and where each argument comes from, with how often that matched the agent in training. [Audit and review](./concepts/audit-and-review) says what to check.

## 6. Serve it

Add the flow to the proxy's arguments:

```json
"args": ["--record", "~/.stretto/notes", "--domain", "notes",
         "--flow", "~/.stretto/notes.flow.json", "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/me/notes"]
```

Restart the server in your host. From now on, after each of the agent's calls, the flow makes the lookups whose chance of being used before the agent's next write, times the chance their arguments are the agent's, is at least 0.3 (`--flow-threshold`). Their results ride in the same tool result, under `--- Also looked up automatically ... ---`.

`--flow-decider reach` asks no model and needs no key. [Deciders](./concepts/deciders) explains it and the alternatives.

## 7. See what it did

- **The flow log.** Every decision is logged beside the session log, in `<session>.flow.jsonl`: the lookup made and its probability, or why the flow handed back.
- **An audit.** Score the flow on sessions it never saw, recorded without it:

  ```sh
  stretto audit --flow ~/.stretto/notes.flow.json --sessions ~/.stretto/notes-new --decider reach
  ```

  At each point where the flow would decide, the audit compares its likeliest option with what the agent did next, per site.

## See the whole loop in one command

From a checkout of the repository, [`scripts/walkthrough.py`](../../scripts/walkthrough.py) runs every step of [the walkthrough](./walkthrough) with a scripted agent in place of an LLM, and checks each outcome. It needs Python 3 and Node 18 or later:

```sh
git clone https://github.com/alexnodeland/stretto && cd stretto
python3 scripts/walkthrough.py --bin ~/.cargo/bin
```

## Next steps

- Run a new flow in [shadow mode](./concepts/shadow-and-promotion) first, and promote it where its lookups were the agent's own.
- Learn again as sessions arrive, and review each change with [`stretto flow-diff`](./concepts/audit-and-review#compare-two-flows).
- Read [how it works](./how-it-works), then the [core concepts](./concepts/sessions).
