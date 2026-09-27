---
description: Install stretto, see the whole loop with no key, put the proxy in front of your MCP server with stretto init, learn a flow, review it, and serve it.
---

# Quick start

This page puts a flow behind an agent. You install stretto, watch the whole loop run on a demo, then record a few sessions of your own agent through the proxy, learn a flow from them, review it, and serve it. It uses [the official MCP filesystem server](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) on a folder of notes, as [the walkthrough](./walkthrough) does, but any MCP server works the same way.

You need:

- Rust 1.88 or later ([rustup](https://rustup.rs)), to install from source until the first release is tagged ([other ways to install](./installation));
- an MCP host, such as Claude Code, Claude Desktop, Cursor or VS Code;
- Node 18 or later, for this page's example server (`npx`).

No API key: everything below learns and decides without a model.

<!-- BRAND SLOT: the recorded walkthrough (website/public/media/walkthrough.mp4, poster media/walkthrough-poster.png). Renders nothing until the file is there. -->
<BrandEmbed kind="walkthrough" caption="The walkthrough, recorded: the whole loop on the MCP filesystem server." />

## 1. Install

```sh
curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh
stretto doctor
```

The install script checks the latest release's archive for your system against its `SHA256SUMS` and puts `stretto`, `stretto-proxy`, `stretto-procedure` and `stretto-mcp-demo` in `~/.local/bin`. On Windows, with Docker, or to build from source, see [installation](./installation). `stretto doctor` then checks the installation: that the other three programs are on your `PATH` at the same version as `stretto`, that `~/.stretto` is writable, whether a TypeSafe key is set (never its value), and which flows and sessions you have. It exits with 1 when something needs fixing. On a first run it prints:

```text
stretto 0.1.0 (/home/me/.local/bin/stretto)

ok       stretto-proxy 0.1.0 (/home/me/.local/bin/stretto-proxy)
ok       stretto-procedure 0.1.0 (/home/me/.local/bin/stretto-procedure)
ok       stretto-mcp-demo 0.1.0 (/home/me/.local/bin/stretto-mcp-demo)
note     /home/me/.stretto does not exist yet; `stretto-proxy --record` creates it
note     TYPESAFE_API_KEY is not set. It is optional: …
note     no flows in ~/.stretto yet (`stretto learn` writes one)
note     no recorded sessions in ~/.stretto yet
```

For shell completions, `stretto completions bash` prints the script for bash, and likewise for `zsh`, `fish`, `powershell` and `elvish` ([where each shell reads it](../../docs/install.md#shell-completions)).

## 2. See the whole loop with no key

```sh
curl -fsSLO https://raw.githubusercontent.com/alexnodeland/stretto/main/examples/quickstart/run.sh \
  && sh run.sh
```

The script runs the loop on `stretto-mcp-demo`'s tiny shop, with a scripted agent in place of an LLM. It records six customers' sessions through `stretto-proxy`, learns a flow from them with no key, prints the flow for review, and serves it to two new customers. It needs only a POSIX shell and the programs you just installed, makes no network request, and takes about a second. Its last step serves the flow to the two new customers:

```text
c41@example.com (cancel): 5 calls without the flow, 2 with it
c42@example.com (status): 4 calls without the flow, 1 with it
The agent made 3 calls instead of 9. It makes one call per LLM turn, so that
is 6 fewer LLM turns: the flow's lookups came back with its first call.
```

[The quickstart's README](../../examples/quickstart/README.md) shows and explains everything it prints.

## 3. Put the proxy in front of your server

In your MCP host's configuration, `stretto-proxy` takes the place of the server's command, and the server's command goes after `--`. `stretto init` prints that configuration. Give it the host (`claude-code`, `claude-desktop`, `cursor` or `vscode`), a name for the server, which is also the domain of its sessions and flows, and the server's command after `--`:

```sh
stretto init --host cursor --domain notes \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

For Cursor, as for Claude Desktop and VS Code, it prints the JSON for the host's configuration file (its arguments wrapped here):

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

For Claude Code it prints the `claude mcp add` command to run instead. Then it says where the configuration goes, and lists the next steps, which this page follows. It writes nothing unless you pass `--write PATH`, and even then it leaves an existing file alone unless you add `--force`. [Integrations](/integrations/) shows what it prints for each host.

- `--record ~/.stretto/logs/notes` writes one log per session into that folder. The proxy expands a leading `~` in its own paths, since hosts start servers without a shell. The server's arguments after `--` are passed as written, so give the notes folder as an absolute path.
- `--domain notes` names the server in the host, and the domain of its sessions and flows.

Now use the agent as you normally would. Each session leaves a log. Record the kinds of request the agent will see: a flow only knows what the sessions showed, and a request type never recorded gets no help. Replayed on τ²-bench, ten of an agent's own sessions gave 96% (retail) and 93% (airline) of what all of its sessions did ([the claims](/research/claims)).

::: tip Tool kinds
A flow only calls tools the server marks `readOnlyHint: true`. The filesystem server marks all 14 of its tools: 10 read, such as `search_files` and `read_text_file`, and 4 write. If your server gives no hints, write [a manifest](./concepts/sessions#tool-kinds) and pass it to `stretto learn --manifest`.
:::

## 4. Learn a flow

```sh
stretto learn --sessions ~/.stretto/logs/notes --domain notes \
  --habit-only --out ~/.stretto/notes.flow.json
```

With [the walkthrough](./walkthrough)'s eight sessions, it prints:

```text
stretto: learned the notes flow from 8 sessions (14 tools) and wrote /home/me/.stretto/notes.flow.json
```

`--habit-only` asks no model: the flow learns from counts alone. It records which lookup followed which call, how often, and where each lookup's arguments came from in an earlier result.

## 5. Review it

A flow makes calls on the agent's behalf, so read it before you serve it:

```sh
stretto flow-show ~/.stretto/notes.flow.json
```

It lists the tools the flow may call (only the read tools), the lookups it may make after each call, and where each argument comes from, with how often that matched the agent in training. [Audit and review](./concepts/audit-and-review) says what to check.

## 6. Serve it

Run `stretto init` again, with the flow:

```sh
stretto init --host cursor --domain notes \
  --flow ~/.stretto/notes.flow.json \
  -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes
```

It prints the same configuration, with the flow added to the proxy's arguments:

```json
"args": ["--record", "~/.stretto/logs/notes", "--domain", "notes",
         "--flow", "/home/me/.stretto/notes.flow.json",
         "--flow-decider", "reach",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem",
         "/home/me/notes"]
```

Replace the server's entry in the host's configuration with it, and restart the server. From then on, after each of the agent's calls, the flow makes each lookup whose probability, times the chance that its arguments are the agent's own, is at least 0.3 (`--flow-threshold`). Their results ride in the same tool result, under `--- Also looked up automatically ... ---`.

A flow learned with `--habit-only` has no arbiter, so `init` serves it with the `reach` decider (`--flow-decider reach`), which asks no model and needs no key. It decides on each lookup's chance of use before the agent's next write, which the research found better calibrated than the chance that the lookup comes next ([deciders](./concepts/deciders)).

::: tip Shadow first
Add `--shadow` to `stretto init` to try the flow on real traffic before it acts. The proxy then records to `~/.stretto/shadow/notes`, and the flow decides and logs what it would look up, but makes no lookups. `stretto promote` then keeps the flow to the calls where its lookups were the agent's own:

```sh
stretto promote --flow ~/.stretto/notes.flow.json \
  --sessions ~/.stretto/shadow/notes \
  --out ~/.stretto/notes-promoted.flow.json
```

Serve the promoted flow the same way. See [shadow mode and promotion](./concepts/shadow-and-promotion).
:::

## 7. See what it did, and learn again

- **The flow log.** Every decision is logged beside its session's log, in `~/.stretto/logs/notes/<session>.flow.jsonl`: the lookup made and its probability, or why the flow handed back.
- **Learn again** when the agent, its prompts or the server change, and review what changed. `flow-diff` exits with 1 when the new flow can do something the old one could not:

  ```sh
  stretto learn --sessions ~/.stretto/logs/notes --domain notes \
    --habit-only --out ~/.stretto/notes-new.flow.json
  stretto flow-diff ~/.stretto/notes.flow.json ~/.stretto/notes-new.flow.json
  ```

- **An audit** scores a flow on sessions it never saw ([audit and review](./concepts/audit-and-review#audit-a-flow)).

## The walkthrough, as a script

From a checkout of the repository, [`scripts/walkthrough.py`](../../scripts/walkthrough.py) runs every step of [the walkthrough](./walkthrough) on the official MCP filesystem server, with a scripted agent in place of an LLM, and checks each outcome. It needs Python 3 and Node 18 or later:

```sh
git clone https://github.com/alexnodeland/stretto && cd stretto
python3 scripts/walkthrough.py --bin ~/.cargo/bin
```

## Next steps

- Read [how it works](./how-it-works), then the [core concepts](./concepts/sessions).
- Set up your host from [its integration page](/integrations/).
- Review each change to a flow with [`stretto flow-diff`](./concepts/audit-and-review#compare-two-flows), as you would code.
