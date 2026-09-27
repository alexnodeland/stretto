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
      "args": ["--record", "~/.stretto/logs", "--domain", "orders",
               "--upstream", "https://example.com/mcp", "--upstream-header", "Authorization=ORDERS_AUTH"],
      "env": { "ORDERS_AUTH": "Bearer …" }
    }
  }
}
```

- `--upstream URL` is the server's MCP endpoint (protocol revision 2025-06-18). It takes the place of the command after `--`; the two cannot be combined.
- `--upstream-header NAME=VAR` sends the header `NAME` with the value of the environment variable `VAR` on every request, such as a bearer token. Repeat it for more headers. Values are never logged, and the log's header records the URL without a user, a password, or the values of query parameters that look like credentials.

Recording, flows and everything else work as they do with a command:

```json
"args": ["--record", "~/.stretto/logs", "--domain", "orders",
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

## Limits

- **The host side is stdio only.** The proxy does not serve Streamable HTTP to a host; only the server side can be HTTP.
- **A session the server ends is not started again.** After the server answers 404 for the session's id, each request fails with a JSON-RPC error until the host restarts the proxy.

The [proxy's reference](/reference/proxy#usage) has the rest.
