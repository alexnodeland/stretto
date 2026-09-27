---
description: Run an MCP server behind stretto-proxy in Claude Desktop - stretto init prints the entry for claude_desktop_config.json, with the proxy's full path.
---

# Claude Desktop

Claude Desktop reads its MCP servers from `claude_desktop_config.json`. `stretto init --host claude-desktop` prints the entry that runs your server behind `stretto-proxy`:

```sh
stretto init --host claude-desktop --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /Users/me/notes
```

```json
{
  "mcpServers": {
    "notes": {
      "command": "/Users/me/.cargo/bin/stretto-proxy",
      "args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
               "/Users/me/notes"]
    }
  }
}
```

Claude Desktop starts servers with a minimal `PATH`, so `init` writes the full path of the `stretto-proxy` it finds on yours. Merge the entry into `claude_desktop_config.json`, which **Settings → Developer → Edit Config** opens, or find it at:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Then quit Claude Desktop completely and open it again; it reads the file when it starts. After it prints the entry, `init` lists the next steps, from learning a flow to serving it: [the quick start](/guide/quick-start) follows them.

::: tip The server's command
`init` resolves the proxy's path, not the server's. If Claude Desktop cannot find the server's own command either, such as `npx`, give its full path in `args`: `which npx` prints it.
:::

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), run `init` again with it:

```sh
stretto init --host claude-desktop --domain notes \
  --flow ~/.stretto/notes.flow.json \
  -- npx -y @modelcontextprotocol/server-filesystem /Users/me/notes
```

```json
"args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
         "--flow", "/Users/me/.stretto/notes.flow.json",
         "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
         "/Users/me/notes"]
```

Replace the server's entry with the new one, and restart Claude Desktop.

- Add `--shadow` to `init` to see what the flow would do before it acts ([shadow mode](/guide/concepts/shadow-and-promotion)).
- A flow learned with `--habit-only` has no arbiter, so `init` serves it with `reach`, which asks no model and needs no key ([deciders](/guide/concepts/deciders)).

## When a server does not start

Claude Desktop keeps each server's stderr in its MCP logs: on macOS in `~/Library/Logs/Claude/`, as `mcp-server-notes.log` for this server. The proxy's own messages there start with `stretto-proxy:`, and the first one says where the session log is. An exit status of 125 means the proxy itself failed, such as when the log directory cannot be created or the server's command cannot be started.

[Any MCP host](./) has what applies to every host: paths, credentials and the three stages of a deployment.
