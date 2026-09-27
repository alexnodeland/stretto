---
description: Run an MCP server behind stretto-proxy in Cursor and VS Code - stretto init prints the entry for mcp.json in each editor's format.
---

# Cursor and VS Code

Both editors start stdio MCP servers from a JSON file, and `stretto init` prints the entry that runs your server behind `stretto-proxy`, in each editor's format:

::: code-group

```sh [Cursor]
stretto init --host cursor --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

```sh [VS Code]
stretto init --host vscode --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

:::

The two formats differ in their top-level key, and VS Code's names the transport:

::: code-group

```json [Cursor: mcp.json]
{
  "mcpServers": {
    "notes": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
               "/home/me/notes"]
    }
  }
}
```

```json [VS Code: mcp.json]
{
  "servers": {
    "notes": {
      "type": "stdio",
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
               "/home/me/notes"]
    }
  }
}
```

:::

- **Cursor** reads `~/.cursor/mcp.json` for every project, and `.cursor/mcp.json` in a project. Merge the entry into either. Cursor's MCP settings show each server's status.
- **VS Code** reads `.vscode/mcp.json` in a workspace, and your user `mcp.json`, which the command **MCP: Open User Configuration** opens. Merge the entry into either; the server's tools are then available to its agent mode. The command **MCP: List Servers** shows each server and can start, stop and restart it.

`--write PATH` writes the file instead, such as `--write .vscode/mcp.json`, and leaves an existing file alone unless you add `--force`, which replaces it with any other servers in it. After the entry, `init` lists the next steps, from learning a flow to serving it: [the quick start](/guide/quick-start) follows them.

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), run `init` again with it, with `--host cursor` or `--host vscode`:

```sh
stretto init --host vscode --domain notes \
  --flow ~/.stretto/notes.flow.json \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

It prints the same entry, with the flow added to the proxy's arguments:

```json
"args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
         "--flow", "/home/me/.stretto/notes.flow.json",
         "--flow-decider", "habit",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
         "/home/me/notes"]
```

Replace the server's entry with the new one, and restart the server.

- Add `--shadow` to `init` to see what the flow would do before it acts ([shadow mode](/guide/concepts/shadow-and-promotion)).
- A flow learned with `--habit-only` is served on its habit (`--flow-decider habit`). To serve it with `reach`, which needs no key either, change `habit` to `reach` ([deciders](/guide/concepts/deciders)).

## Notes

- **Checked-in configuration.** A project's `mcp.json` is shared with everyone who opens the project. Keep `--record` under each person's home, as `init` does (`~/.stretto/logs/NAME`), not in the repository: logs hold whatever the tools read and return.
- **Finding the program.** If the editor cannot start `stretto-proxy`, give its full path as `command`, from `which stretto-proxy`.

[Any MCP host](./) has what applies to every host: paths, credentials and the three stages of a deployment.
