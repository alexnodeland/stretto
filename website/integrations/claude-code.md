---
description: Run an MCP server behind stretto-proxy in Claude Code, with a project's .mcp.json or claude mcp add-json.
---

# Claude Code

Claude Code starts stdio MCP servers from its configuration. Give it `stretto-proxy` as the server's command, and the server's command after `--`.

## In a project: `.mcp.json`

A `.mcp.json` file at the project's root configures servers for everyone working in it. Claude Code asks each person to approve a project's servers before it starts them.

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

## For yourself: `claude mcp add-json`

To add the server for your user, in every project:

```sh
claude mcp add-json --scope user notes '{"type":"stdio","command":"stretto-proxy","args":["--record","~/.stretto/notes","--domain","notes","--","npx","-y","@modelcontextprotocol/server-filesystem","/home/me/notes"]}'
```

`--scope local` (the default) adds it for you in the current project only, and `--scope project` writes it to `.mcp.json`.

## Check it

```sh
claude mcp list
```

In a session, `/mcp` shows each server's status. After the first session, the proxy's log is in `~/.stretto/notes/`.

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), add it to the proxy's arguments and restart Claude Code:

```json
"args": ["--record", "~/.stretto/notes", "--domain", "notes",
         "--flow", "~/.stretto/notes.flow.json", "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/me/notes"]
```

Claude Code then sees each flow lookup inside the result of the call it made, under `--- Also looked up automatically ... ---`. Run the flow with `--flow-shadow` first if you want to see what it would do before it acts ([shadow mode](/guide/concepts/shadow-and-promotion)).

## Notes

- **Naming the model.** The proxy cannot see which model drives Claude Code. Add `"--agent-model", "<model>"` before `--` to write it into the log's header.
- **The live research ran here.** The pilots ran GLM-5.3 in Claude Code, with τ²-bench's tools served over MCP behind `stretto-proxy` ([the paper, §3](/research/paper#3-setup)).
- **Several servers.** Wrap each in its own proxy, with its own `--record` directory and `--domain`.

[Any MCP host](./) has what applies to every host: paths, credentials and exit codes.
