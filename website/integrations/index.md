---
description: Put stretto-proxy in front of any MCP server, in any MCP host - the pattern, the three stages of a deployment, and what to watch for.
---

# Any MCP host

stretto works with any MCP host that starts servers over stdio, which is how desktop apps, IDEs and agent frameworks run local servers. The change is always the same: in the host's configuration, **replace the server's command with `stretto-proxy`, and put the server's command after `--`**.

Before:

```json
{
  "mcpServers": {
    "orders": {
      "command": "npx",
      "args": ["-y", "some-mcp-server"],
      "env": { "SOME_API_KEY": "…" }
    }
  }
}
```

After:

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

The host still sees a stdio MCP server with the same tools. The server inherits the proxy's environment, so its `env` block works as before.

## Per host

| Host | Where the configuration lives | Page |
|---|---|---|
| Claude Code | `.mcp.json` in the project, or `claude mcp add-json` | [Claude Code](./claude-code) |
| Claude Desktop | `claude_desktop_config.json` | [Claude Desktop](./claude-desktop) |
| Cursor | `.cursor/mcp.json` in the project, or `~/.cursor/mcp.json` | [Cursor and VS Code](./cursor-vscode) |
| VS Code | `.vscode/mcp.json` in the workspace | [Cursor and VS Code](./cursor-vscode) |
| A server reached over HTTP | the same, with `--upstream URL` in place of a command | [Streamable HTTP servers](./streamable-http) |

## The three stages

A deployment moves through three sets of arguments. Only the proxy's arguments change; the server's command after `--` stays the same.

**1. Record.** Log sessions to learn from:

```json
"args": ["--record", "~/.stretto/logs", "--domain", "orders",
         "--", "npx", "-y", "some-mcp-server"]
```

**2. Shadow.** Serve a learned flow that decides and logs but makes no lookups ([shadow mode](/guide/concepts/shadow-and-promotion)):

```json
"args": ["--record", "~/.stretto/shadow", "--domain", "orders",
         "--flow", "~/.stretto/orders.flow.json", "--flow-decider", "reach", "--flow-shadow",
         "--", "npx", "-y", "some-mcp-server"]
```

**3. Serve.** Let the promoted flow act:

```json
"args": ["--record", "~/.stretto/logs", "--domain", "orders",
         "--flow", "~/.stretto/orders-promoted.flow.json", "--flow-decider", "reach",
         "--", "npx", "-y", "some-mcp-server"]
```

Between the stages, run `stretto learn`, `stretto flow-show` and `stretto promote` from your shell ([quick start](/guide/quick-start)). Add `--flow-tools` to name the only tools the flow may call on its own ([lookups](/guide/concepts/lookups#which-tools-a-flow-may-call)).

## What to watch for

- **Paths.** Hosts start servers without a shell. The proxy expands a leading `~` in its own path options (`--record`, `--flow`, `--context`, `--flow-log`, `--confirm-log`, `--oracle-cache`), but the server's arguments after `--` are passed as written: give them as absolute paths.
- **Finding the program.** Some hosts do not see your shell's `PATH`. If the host cannot start `stretto-proxy`, use its absolute path, from `which stretto-proxy`; the same goes for `npx` or the server's own command.
- **Credentials.** Put them in `env`, not in `args`. The proxy never records its environment; it replaces credential-looking arguments in the log's header with `<redacted>`, but only as a best effort.
- **One proxy per server.** Wrap each server you want to record in its own proxy. A flow learns from one server's calls.
- **Its messages.** The proxy writes to stderr, prefixed `stretto-proxy:`; the first message says where the log is. Hosts usually keep a server's stderr in their MCP logs.
- **Exit status.** The proxy exits with the server's status, or with 125 if the proxy itself fails, such as when the log cannot be created or the server cannot be started.

## The conversation

MCP does not carry the conversation, and none of the hosts above hand it to a server. `--context FILE` reads it from a file that something else appends to, one JSON line per message; without it, a flow still works, but cannot prefer a value the user mentioned ([the conversation](/guide/concepts/sessions#the-conversation)). Leave it out unless your harness writes that file.

## Related

- [`stretto-proxy` reference](/reference/proxy) and its [options](/reference/cli#stretto-proxy)
- [Quick start](/guide/quick-start)
