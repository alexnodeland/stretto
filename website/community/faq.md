---
description: Answers to common questions about stretto - keys, safety, hosts, models, latency, data, and the name.
---

# FAQ

## Does stretto change my agent's prompt or tools?

No. A flow's lookups arrive inside the results of calls the agent made, as one more text item. The agent's prompt and its tool list are unchanged. The one option that adds a tool, `--commit` (`stretto_commit`, for confirmed writes in one call), is off unless you ask for it.

## Do I need an API key?

No, unless you choose the arbiter. Recording, learning with `--habit-only`, and serving with `--flow-decider reach` or `habit` ask no model. The arbiter decider and the confirmation judge ask TypeSafe's System-One model, Jev, and need `TYPESAFE_API_KEY` ([environment variables](/reference/environment)).

## Can a flow change anything?

No. A flow only calls tools it learned as lookups and that the server does not mark as writes; a tool with no annotation counts as neither and is never called. Writes stay with the agent. `--flow-tools` narrows the flow to a list of tools you name, since a read can still be metered or recorded as an access ([lookups](/guide/concepts/lookups#which-tools-a-flow-may-call)).

## What does a wrong guess cost?

A *detour*: the lookup's result sits in the agent's context for the rest of the session, and the tool's own cost is paid. Reads leave the state unchanged, so a detour cannot change what the agent's writes act on. The threshold weighs exactly this cost against a saved turn's value ([deciders](/guide/concepts/deciders#the-threshold)).

## Does it add latency?

Deciding took under a millisecond per tool response in the live run (at most 4 ms). A lookup adds the tool's own latency to the call it rides on, where an LLM turn it saves takes seconds ([the paper, §4.4](/research/paper#44-live)).

## Which hosts and servers does it work with?

Any host that runs MCP servers over stdio, and any MCP server: one run as a command, or one reached over Streamable HTTP with `--upstream`. For Claude Code, Claude Desktop, Cursor and VS Code, `stretto init` prints the configuration ([integrations](/integrations/)). One proxy wraps one server; an agent with several servers needs one proxy for each.

## Is there a UI?

Yes: [the console](/guide/console), `stretto-console`, a web app on your machine over the files stretto writes. It shows each session as a timeline with the lookups the flow made, each flow's graph and review, and your servers with their host configuration and a connection test. It runs the CLI's jobs from the page. It runs from a checkout, or in a container.

## Which models does it work with?

The mechanism is independent of the model: it works through tool results, which every agent reads. The evidence: the `reach` decider ran live with GLM-5.3; an earlier flow also ran live with Claude Haiku 4.5 and Claude Sonnet 5; and replays cover nine frontier agents on τ²-bench and 89 more on six other benchmarks ([the claims](/research/claims)). How much a flow saves depends on the agent too: an agent that batches its reads into parallel calls leaves fewer turns to save.

## What if the agent calls the tool anyway?

Then that lookup saved nothing, and counts as a detour. Live, GLM-5.3 made none of the flow's 101 lookups again before its next write. In the published record, agents repeat 0.8–2.6% of their reads with no write between on five benchmarks, but a few models repeat many more, which a flow cannot help.

## How many sessions do I need?

Fewer than you might think, if they cover the kinds of request the agent sees. Replayed on τ²-bench, ten of an agent's own sessions gave 96% (retail) and 93% (airline) of what all of its sessions gave ([the claims](/research/claims)). A request type never recorded gets no help.

## Does any data leave my machine?

Only to the MCP server the proxy fronts, unless you ask a System-One model: serving with the arbiter, running the confirmation judge, or learning an arbiter (`stretto learn` without `--habit-only`) sends questions, with parts of the conversation and recent results, to Jev. Session logs hold everything the tools read and return, verbatim, so keep them where that data may live ([privacy](/guide/concepts/privacy)).

## Can I use it where there is no user?

Yes: where the agent works through a procedure against the tools with no one to talk to, a whole workflow can be compiled once and run with no model by `stretto-procedure`, which hands back what its outcome check cannot confirm ([procedures](/guide/concepts/procedures)).

## Why is it not on crates.io?

It depends on a feature of [fugue](https://github.com/alexnodeland/fugue) (`program`) that is not in a fugue release yet. The crate named `stretto` on crates.io is an unrelated cache library; this project's crates are named `stretto-*`. Install from source until the first release is tagged; from then on, each release also ships binaries, install scripts and a container image ([installation](/guide/installation)).

## What are fugue and Jev?

[fugue](https://github.com/alexnodeland/fugue) is a probabilistic programming library for Rust. A flow's run after each call is a fugue program, which is how the same flow is executed, simulated and used to score recorded sessions. Jev is TypeSafe's System-One model, which returns typed choices with calibrated probabilities; the arbiter decider asks it.

## Why the name?

In a fugue, a *stretto* is where entries of the subject overlap and compress. stretto does that to an agent's tool calls.
