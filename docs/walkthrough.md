# Walkthrough: a flow for your own MCP server

Every result so far comes from τ²-bench's tools. This page runs the whole loop on a server stretto was never built around, [the official MCP filesystem server](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem), serving a folder of notes. The loop has five steps: record sessions, learn a flow with no key, audit it on sessions it never saw, serve it, and read what it did.

A scripted agent stands in for an LLM: it searches the notes for what the customer asks about and reads what it finds. [`scripts/walkthrough.py`](../scripts/walkthrough.py) runs every step below and checks the outcomes. It needs Node 18 or later, Python 3 and the two binaries, and no key. CI runs it, so this page cannot drift from the code:

```sh
cargo build -p stretto-report -p stretto-proxy
python3 scripts/walkthrough.py --bin target/debug
```

## 1. Wrap the server

In the MCP host's configuration, put `stretto-proxy` in place of the server's command and the server's command after `--`:

```json
{
  "mcpServers": {
    "notes": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/notes", "--domain", "notes",
               "--context", "~/.stretto/notes-context.jsonl",
               "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/me/notes"]
    }
  }
}
```

- `--record` writes one log per session, `~/.stretto/notes/<session>.jsonl`.
- `--domain` names the flow's domain; any name will do.
- **`--context`** is how the proxy sees the conversation, which MCP never carries. The host appends one JSON line per message: `{"role": "user" | "assistant", "content": text}`. The proxy reads the file from its start, so give each session its own file, or empty it when a session starts. Without it, a flow still works, but it cannot prefer what the customer mentioned, and the logs have no customer turns.

**Check the tools' kinds.** A flow only calls tools the server marks `readOnlyHint: true`. The filesystem server marks all 14 of its tools: 10 read, such as `search_files` and `read_text_file`, and 4 write, such as `write_file` and `move_file`. The first session's log holds the server's `tools/list` response; look for `annotations`. A tool with no hint counts as `generic`, and a flow never calls it. If your server gives no hints, write a manifest and pass it to `stretto learn --manifest`:

```json
{"domain": "notes", "tools": {"search_files": "read", "read_text_file": "read", "write_file": "write"}}
```

## 2. Record sessions

Use the agent as usual. The walkthrough records eight sessions, such as "What's the status of project alpha?" (one search, one file read) and "Summarize the September meetings." (one search, three reads). In each, the agent searches, reads what it needs and replies.

**How many sessions.** A flow only knows what the sessions showed: which lookup followed which call, and where each lookup's arguments came from. Replayed on τ²-bench's tasks, a flow learned from five of an agent's own sessions saved 12 to 19% of retail turns with the habit alone ([the cold start](results/cold-start-2026-09-24.md)). Which sessions matter more than how many. Record the kinds of request the agent will see; a request type never recorded gets no help.

**What the logs hold:** every message the proxy forwarded (`client` and `server`), the conversation (`context`), and the proxy's own requests (`proxy`), with timestamps. Here that is every file the agent read, verbatim ([the log format](../crates/stretto-proxy/README.md#log-format)). Treat the logs like the data the tools touch.

## 3. Learn a flow, with no key

```sh
stretto learn --sessions ~/.stretto/notes --domain notes --habit-only --out notes.flow.json
```

```text
stretto: learned the notes flow from 8 sessions (14 tools) and wrote notes.flow.json
```

`--habit-only` asks no System-One model. Every session trains the habit, and the flow has no arbiter, so it is served with `--flow-decider habit`. From a deployment's first sessions this is the steadier start: in the cold start results, the habit alone saved more than an arbiter fitted on those same sessions. `--arbiter-from data/arbiters/retail.json` instead adds [a shipped arbiter](../data/arbiters/README.md), fitted on four agents' τ²-bench decisions. It asks Jev at each decision when the flow serves, so it needs `TYPESAFE_API_KEY`.

The flow is a JSON file to review before serving it ([every field](formats.md)). Three parts say what it will do:

- **`manifest.tools`**: the 10 read tools, taken from the annotations. They are the only tools the flow will ever call.
- **`sites.next`**: after `search_files`, the agent read a file 7 times; after `read_text_file`, it read another 7 times. Those are the only lookups this flow can make.
- **`bindings.sources`**: `read_text_file`'s `path` came from `search_files`' result 14 times. Three came from the whole result (`$`), a search that found one file. Eleven came from one line of it (`$[*]`), a search that listed several. A result that is not JSON is read as its lines, so a lookup can bind one line at a time. The binding picked the agent's own path at 12 of its 14 reads. It missed twice, where the agent read the files the customer named ("beta and gamma") and the binding would have read the list in order. A value counts as mentioned only when the customer's words contain all of it, an id or an email, not part of a path.

`search_files`' own `path` and `pattern` were never found in an earlier output, since they come from the agent. So the flow never searches; it reads what a search found.

## 4. Audit it on sessions it never saw

Record a few more sessions without the flow, then score them under it:

```sh
stretto audit --flow notes.flow.json --sessions ~/.stretto/notes-new
```

```text
The flow decides with the habit alone. 3 episodes, 8 decisions scored (0 left out: no answer from the oracle).

| Site | Decisions | Agreement | Nats per decision |
|---|---|---|---|
| `read_text_file` | 6 | 33.3% | 0.747 |
| `search_files` | 2 | 100.0% | 0.009 |
```

At each point where the flow would decide, the audit compares its likeliest option with what the agent did next. It runs the flow as a fugue program and scores the agent's path under it ([the audit](results/audit-2026-09-24.md)).

- **After `search_files`** the habit is sure the agent reads a file (0.99), and it did both times.
- **After `read_text_file`** the habit is unsure: in training the agent read on 7 times and stopped 7 times. After a search and one read, it read on 4 times in 7; after two reads, 3 times in 7. So the habit expects a second read and then a stop. On a five-file request it disagrees with the agent's third, fourth and fifth reads, and on a one-file request it expected a second. The habit cannot count what is left in a list; the binding can. Serving, the flow reads on while the habit's probability times the binding's chance clears 0.3 (0.43 × 0.81 here) and the search listed a file it has not read. When nothing is left, it hands back. A site with low agreement is not a site the flow gets wrong, but a site to look at.
- **When to learn again:** a site whose agreement falls on new sessions, a request type the flow has not seen, or a server whose tools change. Learning is quick, so learn from the sessions recorded since.

A flow with an arbiter is audited with its arbiter unless `--decider habit` says otherwise; the arbiter asks the System-One model, from `--oracle-cache` or, with `--oracle jev`, live.

## 5. Serve it

Add `--flow` to the proxy's arguments:

```json
"args": ["--record", "~/.stretto/notes", "--domain", "notes", "--context", "~/.stretto/notes-context.jsonl",
         "--flow", "~/.stretto/notes.flow.json", "--flow-decider", "habit",
         "--", "npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/me/notes"]
```

Asked "Summarize the October meetings.", files the agent had never read, the agent searched once. The flow read both meetings behind that one call, so the agent made 1 call where it had made 3. The agent got its own result with one more text item:

```text
--- Also looked up automatically (current results; no need to repeat these calls) ---

read_text_file {"path":"/home/me/notes/meetings/2026-10-06.md"}:
# 2026-10-06
Gamma design review moved to 10-09.


read_text_file {"path":"/home/me/notes/meetings/2026-10-13.md"}:
# 2026-10-13
Delta owner: Priya.
```

**The flow log**, `<session>.flow.jsonl` next to the session log, says why. After the search, the flow looked up the first meeting (`"prob": 0.99`, `"binding": 0.81`). After that read, it looked up the second (0.57). After the second it handed back (abridged):

```json
{"action": "hand_back", "site": "read_text_file", "prob": 0.43, "probs": {"read_text_file": 0.43, "respond": 0.57},
 "reason": "read_text_file: nothing left to pass as `path`"}
```

Each entry has the site, each option's probability, the lookup's probability (`prob`) and its binding's chance (`binding`), and the lookup made (`tool`, `arguments`) or the reason for handing back. `--flow-per-call` (8) and `--flow-per-session` (40) cap the lookups.

## Privacy and limits

- **Logs** hold every tool call and result verbatim, here every file read. Keep them where the data may live, and never commit them. A flow holds no transcript: counts, tool and argument names, JSON paths and the tools' documentation. Code features (`map.ids`) can hold values from training outputs ([the formats page](formats.md#reviewing-a-flow)). [#24](https://github.com/alexnodeland/stretto/issues/24) tracks privacy for recorded sessions.
- **One stdio server per proxy.** Streamable HTTP servers are not supported ([#21](https://github.com/alexnodeland/stretto/issues/21)). An agent with several servers leaves one log per wrapped server, and a flow learns from one server's calls ([#22](https://github.com/alexnodeland/stretto/issues/22)).
- **What a flow can bind.** An argument's value must appear whole in an earlier result: as a JSON string, or as a line of a result that is not JSON. A value the agent composes, such as a path joined from a folder and a file name, has no source. A lookup that needs one hands back.
- **Only reads.** A flow never writes. Guards and the confirmation judge check writes, but only for τ²-bench's retail and airline domains (`--guards`).
