---
description: Run an MCP server behind stretto-proxy in Cursor (mcp.json) and in VS Code (.vscode/mcp.json).
---

# Cursor and VS Code

Both editors start stdio MCP servers from a JSON file. Put `stretto-proxy` in place of the server's command, and the server's command after `--`. The two files differ in their top-level key.

::: code-group

```json [Cursor: .cursor/mcp.json]
{
  "mcpServers": {
    "notes": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
               "/home/me/notes"]
    }
  }
}
```

```json [VS Code: .vscode/mcp.json]
{
  "servers": {
    "notes": {
      "type": "stdio",
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
               "/home/me/notes"]
    }
  }
}
```

:::

- **Cursor** reads `.cursor/mcp.json` in a project, and `~/.cursor/mcp.json` for every project. Its MCP settings show each server's status.
- **VS Code** reads `.vscode/mcp.json` in a workspace; the server's tools are available to its agent mode. The **MCP: List Servers** command shows each server and can start, stop and restart it.

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), add it to the proxy's arguments in either file, and restart the server:

```json
"args": ["--record", "~/.stretto/notes", "--domain", "notes",
         "--flow", "~/.stretto/notes.flow.json", "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
         "/home/me/notes"]
```

## Notes

- **Checked-in configuration.** A project's `mcp.json` is shared with everyone who opens the project. Keep `--record` under each person's home (`~/...`), not in the repository: logs hold whatever the tools read and return.
- **Finding the program.** If the editor cannot start `stretto-proxy`, give its absolute path, from `which stretto-proxy`.

[Any MCP host](./) has what applies to every host: paths, credentials and the three stages of a deployment.
