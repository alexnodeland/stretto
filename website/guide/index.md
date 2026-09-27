---
description: stretto learns which reads an LLM agent makes next, and where their arguments come from, and serves those reads through an MCP proxy so the agent needs fewer LLM turns.
---

# What is stretto?

stretto makes tool-using LLM agents take fewer turns. It watches an agent's tool calls through an MCP proxy, learns from them which reads the agent makes next and where their arguments come from, and then makes those reads for it. After each of the agent's own calls, the results of the reads it was about to ask for ride in the same tool result. The agent sees them before it asks, and skips the turns it would have spent asking.

It needs no new tool and no change to the agent's prompt, and, deciding by counts alone, no model of its own. It only ever reads: a flow never calls a tool the server marks as a write, so a wrong guess costs one extra lookup and never an action.

::: tip The name
In a fugue, a *stretto* is where entries of the subject overlap and compress. stretto does that to an agent's tool calls: the reads the agent would make one turn at a time arrive together.
:::

## What the agent sees

The agent calls a tool as usual. The server's result comes back with one more text item, holding the lookups the flow made behind that call. From [the walkthrough](./walkthrough), after the agent searched a folder of notes for the October meetings:

```text
--- Also looked up automatically (current results; no need to repeat these calls) ---

read_text_file {"path":"/home/me/notes/meetings/2026-10-06.md"}:
# 2026-10-06
Gamma design review moved to 10-09.


read_text_file {"path":"/home/me/notes/meetings/2026-10-13.md"}:
# 2026-10-13
Delta owner: Priya.
```

The agent made 1 call where it had made 3.

## The loop

1. **Record.** `stretto-proxy` takes the place of an MCP server's command in your host's configuration. It starts the server, forwards every message byte for byte, and writes one log per session. See [sessions and recording](./concepts/sessions).
2. **Learn.** `stretto learn` reads the logs and writes a *flow*: a JSON file that says which lookups the agent made after each call, how often, and where each argument's value came from. Learning is counting. See [flows](./concepts/flows).
3. **Review.** `stretto flow-show` renders a flow for a person to read, and `stretto flow-diff` lists what changed between two flows. Run the flow in shadow mode first, then keep it to the places where its lookups were the agent's own. See [audit and review](./concepts/audit-and-review) and [shadow mode](./concepts/shadow-and-promotion).
4. **Serve.** Give the proxy the flow. After each of the agent's calls it makes the lookups that clear a threshold and appends their results. See [lookups and detours](./concepts/lookups) and [deciders](./concepts/deciders).
5. **Learn again.** New sessions are new counts. Learn from them, and let `flow-diff` tell you whether the new flow can do anything the old one could not.

## What you get

stretto is a Rust workspace. It installs four programs:

| Program | What it does |
|---|---|
| `stretto` | Learns flows from recorded sessions (`learn`), shows and compares them (`flow-show`, `flow-diff`), audits them on new sessions (`audit`), promotes them from shadow mode (`promote`), pseudonymizes sessions (`redact`), and measures how compressible an agent's behavior is on τ²-bench's published trajectories (`phase0`). |
| `stretto-proxy` | Wraps any MCP server, run as a command or reached over Streamable HTTP, and runs as a stdio server for the host. It records sessions and serves flows. |
| `stretto-procedure` | Runs a compiled procedure against an MCP server with no model, for work where no user speaks. See [procedures](./concepts/procedures). |
| `stretto-mcp-demo` | A tiny MCP server for trying the proxy. |

Every command and option is in the [CLI reference](/reference/cli).

## Where it helps

stretto takes the reads whose arguments an earlier tool result supplies: an order id listed in the customer's record, a file a search found, the hotels a city's listing named. How much of an agent's work that is, the *read-only ceiling*, depends on the domain. Across seven benchmarks it runs from 3.5% of LLM turns, where each request names what to read, to 47.1%, where one listing names what every later read takes ([the claims](/research/claims)). Replies to the user, writes, and reads that only the user's words can name stay with the model.

[Why stretto?](./why) says more about when it pays, with the evidence.

## Status

stretto is pre-alpha. 0.1.0 is the first release ([changelog](/community/changelog)), and these pages follow the `main` branch. It is not on crates.io yet, because it depends on a [fugue](https://github.com/alexnodeland/fugue) feature that is not in a fugue release yet. Install it from source: see [installation](./installation).

::: warning Not the `stretto` crate
The crate named `stretto` on crates.io is an unrelated cache library. This project's binary is `stretto`, and its crates are named `stretto-*`.
:::

## Next steps

- [Quick start](./quick-start): install, record, learn and serve a flow.
- [How it works](./how-it-works): what happens after each of the agent's calls.
- [Integrations](/integrations/): the configuration for Claude Code, Claude Desktop, Cursor, VS Code and Streamable HTTP servers.
- [The paper](/research/paper): the model and the results behind it.
