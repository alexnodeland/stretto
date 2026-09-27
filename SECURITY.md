# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: open a draft security advisory from the repository's **Security** tab (*Report a vulnerability*). Don't open a public issue for a vulnerability. You will get an acknowledgement, and a fix or a decision, as soon as it has been looked at; the advisory is published once a fix is released.

## Supported versions

stretto is pre-1.0. Fixes go into the latest release and `main`.

## What to report

stretto sits between an agent and its tools, so these are in scope:

- **Flows acting beyond reads.** A flow may call only the tools it learned as lookups, only those the server does not mark `readOnlyHint: false`, and, with `--flow-tools`, only those listed. A way to make `stretto-proxy` call any other tool, or to call a tool with arguments that did not come from where the flow's bindings say, is a vulnerability.
- **Guards and confirmation.** A way around `--guards` or `--confirm-judge enforce` for a write they should refuse.
- **Secrets.** `TYPESAFE_API_KEY` is read by the `stretto` binaries only, and `stretto-proxy` can take it from a file (`TYPESAFE_API_KEY_FILE`) so that the agent's process never holds it. A path by which it reaches a log, a flow, a session file or the agent is a vulnerability.
- **Recorded data.** Session logs hold tool arguments and results, which can be personal data. [docs/privacy.md](docs/privacy.md) says what each file keeps and what is sent to the System-One model; a way to make stretto keep or send more than it says is in scope.
- **Parsing.** Crashes or unbounded resource use from malformed MCP messages, flows or session logs.

Speculative reads can reveal what an agent is doing to whoever watches the server's traffic, as any prefetching does. The paper's Appendix D discusses it; reports that sharpen that analysis are welcome as issues.
