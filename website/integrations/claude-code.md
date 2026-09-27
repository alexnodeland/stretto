---
description: Run an MCP server behind stretto-proxy in Claude Code - stretto init prints the claude mcp add command, or writes a project's .mcp.json.
---

# Claude Code

Claude Code starts stdio MCP servers from its configuration. `stretto init --host claude-code` prints the command that adds your server to it, behind `stretto-proxy`.

## Add the server

```sh
stretto init --host claude-code --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

It prints a `claude mcp add` command, on one line (wrapped here). Run it in the project where you use Claude Code:

```sh
claude mcp add notes -- stretto-proxy \
  --record ~/.stretto/logs/notes --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

Everything after the first `--` is the server's command: `stretto-proxy` and its arguments, then, after the proxy's own `--`, the real server's command. By default the server is added for you in the current project. With `claude mcp add --scope user`, it applies to every project; with `--scope project`, it is written to the project's `.mcp.json`, to share.

After the command, `init` prints the next steps, from learning a flow to serving it: [the quick start](/guide/quick-start) follows them.

## Or write the project's `.mcp.json`

```sh
stretto init --host claude-code --domain notes --write .mcp.json \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

It writes:

```json
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

A `.mcp.json` at the project's root configures servers for everyone working in it, and Claude Code asks each person to approve a project's servers before it starts them. `--write` leaves an existing `.mcp.json` alone unless you add `--force`, which replaces the whole file, with any other servers in it. To add the server to a file that has others, run the printed `claude mcp add` command with `--scope project` instead.

## Check it

```sh
claude mcp list
```

In a session, `/mcp` shows each server's status. After the first session, the proxy's log is in `~/.stretto/logs/notes/`.

## Serve a flow

Once you have [learned and reviewed a flow](/guide/quick-start#4-learn-a-flow), run `init` again with it:

```sh
stretto init --host claude-code --domain notes \
  --flow ~/.stretto/notes.flow.json \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

```sh
claude mcp add notes -- stretto-proxy \
  --record ~/.stretto/logs/notes --domain notes \
  --flow /home/me/.stretto/notes.flow.json --flow-decider habit \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

The server is already configured under that name, so remove it first (`claude mcp remove notes`), then run the new command and restart Claude Code. Claude Code then sees each of the flow's lookups inside the result of the call it made, under `--- Also looked up automatically ... ---`.

- Add `--shadow` to `init` to see what the flow would do before it acts ([shadow mode](/guide/concepts/shadow-and-promotion)).
- A flow learned with `--habit-only` is served on its habit (`--flow-decider habit`). To serve it with `reach`, which needs no key either, change `habit` to `reach` ([deciders](/guide/concepts/deciders)).

## Notes

- **Naming the model.** The proxy cannot see which model drives Claude Code. Add `--agent-model <model>` to the proxy's arguments, before its `--`, to write it into the log's header.
- **The live research ran here.** The pilots ran GLM-5.3 in Claude Code, with τ²-bench's tools served over MCP behind `stretto-proxy` ([the paper, §3](/research/paper#3-setup)).
- **Several servers.** Wrap each in its own proxy, with its own `--domain`; `init` gives each its own `--record` directory.

[Any MCP host](./) has what applies to every host: paths, credentials and exit codes.
