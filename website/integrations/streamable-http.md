---
description: Put stretto-proxy in front of an MCP server reached over Streamable HTTP with --upstream, sending credentials from the environment.
---

# Streamable HTTP servers

A server you reach over the network, rather than start as a command, speaks MCP's Streamable HTTP transport. `stretto-proxy --upstream URL` proxies for it. The host still runs the proxy as a stdio server, as it would any local server, so the host's configuration looks the same as for a command, with `--upstream` where the command was:

```json
{
  "mcpServers": {
    "orders": {
      "command": "stretto-proxy",
      "args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
               "--upstream", "https://example.com/mcp", "--upstream-header", "Authorization=ORDERS_AUTH"],
      "env": { "ORDERS_AUTH": "Bearer …" }
    }
  }
}
```

- `--upstream URL` is the server's MCP endpoint (protocol revision 2025-06-18). It takes the place of the command after `--`; the two cannot be combined.
- `--upstream-header NAME=VAR` sends the header `NAME` with the value of the environment variable `VAR` on every request, such as a bearer token. Repeat it for more headers. Values are never logged, and the log's header records the URL without a user, a password, or the values of query parameters that look like credentials.

`stretto init` configures servers that are started by a command, so write this entry by hand. The proxy's other arguments are the ones `init` prints for a command: here `--record ~/.stretto/logs/orders`, as `init` would record a server named `orders` ([any MCP host](./)).

Recording, flows and everything else work as they do with a command:

```json
"args": ["--record", "~/.stretto/logs/orders", "--domain", "orders",
         "--flow", "~/.stretto/orders.flow.json", "--flow-decider", "reach",
         "--upstream", "https://example.com/mcp", "--upstream-header", "Authorization=ORDERS_AUTH"]
```

## What the proxy does over HTTP

- Each message the host sends is POSTed to the endpoint. The server's answer comes back to the host as lines, whether it is one JSON body or an event stream: progress notifications, log messages, then the result.
- The session id the server assigns (`Mcp-Session-Id`) and the agreed protocol version (`MCP-Protocol-Version`) go with every later request.
- After `notifications/initialized`, a GET stream carries the messages the server sends on its own, and is opened again if it drops.
- When the host closes the proxy's stdin, the proxy waits for the requests in flight and ends the session with a DELETE, then exits with 0.
- A request the server refuses, or cannot be reached for, is answered with a JSON-RPC error, so the host is not left waiting.

CI runs the proxy in front of the reference server, `@modelcontextprotocol/server-everything`, and the tests run it in front of `stretto-mcp-demo --http`, which insists on the session id and version.

## Hosts that connect by URL

The host's side can be HTTP too. `stretto-proxy --listen 127.0.0.1:8931` serves hosts at `http://127.0.0.1:8931/mcp` in place of stdio, and gives each MCP session a server, a log and a flow of its own; with `--upstream`, each gets its own session of the server. Start the proxy once, and give hosts its URL:

```bash
stretto-proxy --listen 127.0.0.1:8931 --record ~/.stretto/logs/orders --domain orders \
  --upstream https://example.com/mcp --upstream-header Authorization=ORDERS_AUTH
claude mcp add --transport http orders http://127.0.0.1:8931/mcp
```

Without `--listen-token-file`, the proxy serves loopback only. The [proxy's reference](/reference/proxy#hosts-over-http) has the rest.

## Limits

- **A session the server ends is not started again.** After the server answers 404 for the session's id, each request fails with a JSON-RPC error until the host restarts the proxy, or, over `--listen`, starts another session.

The [proxy's reference](/reference/proxy#usage) has the rest.
