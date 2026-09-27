---
description: Run an MCP server behind stretto-proxy in Claude Desktop, with claude_desktop_config.json.
---

# Claude Desktop

Claude Desktop reads its MCP servers from `claude_desktop_config.json`. Open it from **Settings → Developer → Edit Config**, or at:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Put `stretto-proxy` in place of the server's command, and the server's command after `--`:

```json
{
  "mcpServers": {
    "notes": {
      "command": "/Users/me/.cargo/bin/stretto-proxy",
      "args": ["--record", "~/.stretto/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/me/notes"]
    }
  }
}
```

Quit Claude Desktop completely and open it again; it reads the file when it starts.

::: tip Absolute paths
Claude Desktop does not start servers from your shell, so it may not find programs on your shell's `PATH`. The example gives `stretto-proxy`'s absolute path, which `which stretto-proxy` prints. If the server's own command is not found either, give its absolute path too, such as `which npx`.
:::

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), add it to the proxy's arguments, and restart Claude Desktop:

```json
"args": ["--record", "~/.stretto/notes", "--domain", "notes",
         "--flow", "~/.stretto/notes.flow.json", "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/Users/me/notes"]
```

## When a server does not start

Claude Desktop keeps each server's stderr in its MCP logs: on macOS in `~/Library/Logs/Claude/`, as `mcp-server-notes.log` for this server. The proxy's own messages there start with `stretto-proxy:`, and the first one says where the session log is. An exit status of 125 means the proxy itself failed, such as when the log directory cannot be created or the server's command cannot be started.

[Any MCP host](./) has what applies to every host: paths, credentials and the three stages of a deployment.
