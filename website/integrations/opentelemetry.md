---
description: Learn and measure a flow from the OpenTelemetry GenAI spans an agent framework exports, with no proxy recording its sessions.
---

# OpenTelemetry spans

Many agent frameworks report what their agents do as OpenTelemetry spans, under the [semantic conventions for generative AI](https://github.com/open-telemetry/semantic-conventions-genai): a span for each LLM call, and one for each tool the agent runs. If yours exports them, stretto can learn a flow from them, and measure it, with no proxy recording the sessions:

```sh
stretto learn --otel spans.otlp.jsonl --manifest shop.manifest.json --domain shop \
  --habit-only --out ~/.stretto/flows/shop.flow.json
stretto audit --flow ~/.stretto/flows/shop.flow.json --otel spans.otlp.jsonl
```

- **The export.** OTLP JSON, as the OpenTelemetry Collector's file exporter writes it: one `ExportTraceServiceRequest` per line, or one for the whole file. Each trace is one session.
- **LLM turns.** Each inference span (`gen_ai.operation.name` `chat`, `generate_content` or `text_completion`) is one LLM call, with its token usage. The `execute_tool` spans that follow it, before the next one, are that turn's tool calls; several at once are parallel calls of one turn.
- **Tool kinds.** Spans do not say which tools only read, so give them in a manifest, as for a server without `readOnlyHint` ([sessions](/guide/concepts/sessions#tool-kinds)):

  ```json
  {"domain": "shop", "tools": {"find_user_id_by_email": "read", "get_user_details": "read",
                               "get_order_details": "read", "cancel_pending_order": "write"}}
  ```

- **Content.** The conventions capture a tool call's arguments (`gen_ai.tool.call.arguments`) and result (`gen_ai.tool.call.result`), and the messages, only when the framework is asked to. Without them, a flow still learns which lookups follow which calls, but leaves every lookup to the agent, since it cannot say where the arguments come from. `learn` says so when the spans lack them. With them, the flow binds arguments from earlier results, as from recorded sessions.
- **The version.** The conventions are still in development. stretto reads the names of [semantic-conventions 1.38.0](https://github.com/open-telemetry/semantic-conventions/blob/v1.38.0/docs/gen-ai/gen-ai-spans.md), the first release to define a tool call's arguments and result, and a flow learned from spans records that in its provenance (`conventions`).

`promote` and `drift` take `--otel` in place of `--sessions` too. `phase0` does not: it is the τ²-bench study, and reads a checkout's tasks and tool files.

## Serving what you learned

A flow serves through `stretto-proxy`, in front of an MCP server. When the tools your framework reports are an MCP server's, put the proxy in front of it and serve the flow there ([any MCP host](./)); the flow names the tools as the spans do, which is as the server does. When they are functions of your own, a harness can ask `stretto serve` for the flow's next lookup over a local port, sending the conversation so far in τ²-bench's message format ([CLI](/reference/cli#stretto-flow-serve)).

## An example

[`docs/examples/pydantic-ai-shop.otlp.jsonl`](https://github.com/alexnodeland/stretto/blob/main/docs/examples/pydantic-ai-shop.otlp.jsonl) holds eight sessions of a [Pydantic AI](https://ai.pydantic.dev) agent with the demo shop's tools, exported by Pydantic AI's own instrumentation with content capture on. [`scripts/otel_fixture.py`](https://github.com/alexnodeland/stretto/blob/main/scripts/otel_fixture.py) records it, with a scripted model, so no LLM is called:

```sh
stretto learn --otel docs/examples/pydantic-ai-shop.otlp.jsonl \
  --manifest docs/examples/shop.manifest.json --domain shop --habit-only --out shop.flow.json
stretto flow-show shop.flow.json
```

The flow looks a customer's details up after finding them, with the id the lookup returned, and their orders after the details, as the agent did.
