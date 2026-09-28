---
description: Put stretto-proxy in front of any MCP server, in any MCP host - stretto init for Claude Code, Claude Desktop, Cursor and VS Code, the three stages of a deployment, and what to watch for.
---

# Any MCP host

stretto works with any MCP host that starts servers over stdio, which is how desktop apps, IDEs and agent frameworks run local servers. The change is always the same: in the host's configuration, **replace the server's command with `stretto-proxy`, and put the server's command after `--`**.

## `stretto init` prints it

For Claude Code, Claude Desktop, Cursor and VS Code, `stretto init` prints the configuration in the host's own format. Give it the host, a name for the server, and the server's command after `--`:

```sh
stretto init --host cursor --domain orders -- npx -y some-mcp-server
```

It prints (its arguments wrapped here):

```json
{
  "mcpServers": {
    "orders": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
               "--", "npx", "-y", "some-mcp-server"]
    }
  }
}
```

- `--host` is `claude-code`, `claude-desktop`, `cursor` or `vscode`. The configuration goes to stdout; where it goes, and the next steps, go to stderr.
- `--domain NAME` is the server's name in the host, and the domain of its sessions and flows. The proxy records sessions in `~/.stretto/logs/NAME`, or where `--record DIR` says.
- `--write PATH` writes the configuration file instead of printing it. It leaves an existing file alone unless you add `--force`, which replaces the whole file, with any other servers in it; to add a server to a file that has others, merge the printed configuration by hand.
- A server program named by a relative path is written with its absolute path, since the host starts servers in a directory of its own choosing.

`init` writes no `env` block. If the server needs environment variables, add them to its entry as before: the server inherits the proxy's environment. Every option is in [the CLI reference](/reference/cli#stretto-init).

::: tip In the console
[The console](/guide/console)'s page for a server shows the same configuration for each host, from a registry of your servers. It also tests the connection, listing the server's tools as they are now.
:::

## Per host

| Host | `--host` | What `init` prints | Page |
|---|---|---|---|
| Claude Code | `claude-code` | a `claude mcp add` command; with `--write .mcp.json`, the project's file | [Claude Code](./claude-code) |
| Claude Desktop | `claude-desktop` | JSON for `claude_desktop_config.json`, with the proxy's full path | [Claude Desktop](./claude-desktop) |
| Cursor | `cursor` | JSON for `~/.cursor/mcp.json` or `.cursor/mcp.json` | [Cursor and VS Code](./cursor-vscode) |
| VS Code | `vscode` | JSON for `.vscode/mcp.json`, or the user `mcp.json` | [Cursor and VS Code](./cursor-vscode) |
| A server reached over HTTP | none | write the entry by hand, with `--upstream URL` in place of the command | [Streamable HTTP servers](./streamable-http) |

In any other host, write the entry by hand. Before:

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
      "args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
               "--", "npx", "-y", "some-mcp-server"],
      "env": { "SOME_API_KEY": "…" }
    }
  }
}
```

The host still sees a stdio MCP server with the same tools.

## The three stages

A deployment moves through three configurations, and `stretto init` prints each. Only the proxy's arguments change; the server's command after `--` stays the same.

**1. Record.** Log sessions to learn from:

```sh
stretto init --host cursor --domain orders -- npx -y some-mcp-server
```

```json
"args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
         "--", "npx", "-y", "some-mcp-server"]
```

Then learn a flow and review it:

```sh
stretto learn --sessions ~/.stretto/logs/orders --domain orders \
  --habit-only --out ~/.stretto/orders.flow.json
stretto flow-show ~/.stretto/orders.flow.json
```

**2. Shadow.** Serve the flow so that it decides and logs, but makes no lookups ([shadow mode](/guide/concepts/shadow-and-promotion)):

```sh
stretto init --host cursor --domain orders \
  --flow ~/.stretto/orders.flow.json --shadow \
  -- npx -y some-mcp-server
```

```json
"args": ["--record", "~/.stretto/shadow/orders", "--domain", "orders",
         "--flow", "/home/me/.stretto/orders.flow.json",
         "--flow-decider", "reach", "--flow-shadow",
         "--", "npx", "-y", "some-mcp-server"]
```

Then keep the flow to the calls where its lookups were the agent's own:

```sh
stretto promote --flow ~/.stretto/orders.flow.json \
  --sessions ~/.stretto/shadow/orders \
  --out ~/.stretto/orders-promoted.flow.json
```

**3. Serve.** Let the promoted flow act:

```sh
stretto init --host cursor --domain orders \
  --flow ~/.stretto/orders-promoted.flow.json \
  -- npx -y some-mcp-server
```

```json
"args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
         "--flow", "/home/me/.stretto/orders-promoted.flow.json",
         "--flow-decider", "reach",
         "--", "npx", "-y", "some-mcp-server"]
```

A flow learned with `--habit-only` has no arbiter, so `init` serves it with the `reach` decider (`--flow-decider reach`), which asks no model and needs no key ([deciders](/guide/concepts/deciders)). A flow with an arbiter is served with it, and `init` reminds you that the server then needs `TYPESAFE_API_KEY` in its `env`. To name the only tools the flow may call on its own, add `--flow-tools` to `args` by hand ([lookups](/guide/concepts/lookups#which-tools-a-flow-may-call)).

## What to watch for

- **Paths.** Hosts start servers without a shell. The proxy expands a leading `~` in its own path options (`--record`, `--flow`, `--context`, `--flow-log`, `--confirm-log`, `--oracle-cache`), but the server's arguments after `--` are passed as written: give them as absolute paths.
- **Finding the program.** Some hosts do not see your shell's `PATH`. `init` writes `stretto-proxy`'s full path for Claude Desktop, which starts servers with a minimal `PATH`. If another host cannot start it, use its full path too, from `which stretto-proxy`; the same goes for `npx` or the server's own command.
- **Credentials.** Put them in `env`, not in `args`. The proxy never records its environment; it replaces credential-looking arguments in the log's header with `<redacted>`, but only as a best effort.
- **One proxy per server.** Wrap each server you want to record in its own proxy, with its own `--domain`. For a flow across several servers, record them into one directory ([several servers](/guide/concepts/sessions#several-servers)).
- **Its messages.** The proxy writes to stderr, prefixed `stretto-proxy:`; the first message says where the log is. Hosts usually keep a server's stderr in their MCP logs.
- **Exit status.** The proxy exits with the server's status, or with 125 if the proxy itself fails, such as when the log cannot be created or the server cannot be started.

## The conversation

MCP does not carry the conversation, and none of the hosts above hand it to a server. `--context FILE` reads it from a file that something else appends to, one JSON line per message; without it, a flow still works, but cannot prefer a value the user mentioned ([the conversation](/guide/concepts/sessions#the-conversation)). Leave it out unless your harness writes that file.

## Related

- [`stretto init`](/reference/cli#stretto-init) and [`stretto-proxy`'s options](/reference/cli#stretto-proxy)
- [`stretto-proxy` reference](/reference/proxy)
- [Quick start](/guide/quick-start)
- [The console](/guide/console): each server's host configuration and connection test
