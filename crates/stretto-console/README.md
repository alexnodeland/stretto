# stretto-console

stretto's management plane: a local web app over the files stretto already writes to `~/.stretto`. It shows the MCP servers stretto fronts and the host configuration for each, the recorded sessions call by call with the flow's lookups and why it made them, the flows as a reviewer reads them, and the `stretto` CLI's jobs, and it runs those jobs. It is one binary, `stretto-console`, with the UI (`console/`, Vue) embedded. [docs/console.md](../../docs/console.md) is the guide to using it; this page documents how it works.

It reads through stretto's own library, so it cannot disagree with the CLI: sessions through `stretto_trace::mcp`, flows through `stretto_report::flow::Flow` and `stretto_report::review::view` (what `stretto flow-show` renders), host configuration through `stretto_report::init` (what `stretto init` prints), and the data directory as `stretto doctor` walks it. The one file it owns is the server registry, `servers.json`.

## Running it

```sh
cargo run --release -p stretto-console
# stretto-console: /home/me/.stretto on 127.0.0.1:7878
# stretto-console: open http://127.0.0.1:7878/?token=…
```

Open the URL it prints: the token in it signs the browser in (a cookie), and the page reloads without it.

| Option | What it does |
|---|---|
| `--data DIR` | The data directory. Default: `$STRETTO_HOME`, else `~/.stretto`, created if missing. |
| `--listen ADDR` | Where to listen. Default: `$STRETTO_CONSOLE_LISTEN`, else `127.0.0.1:7878`. The container image sets `STRETTO_CONSOLE_LISTEN=0.0.0.0:8080`. |
| `--token TOKEN` | The token the API requires. Default: `$STRETTO_CONSOLE_TOKEN`, else 32 random bytes as hex, new at each start and printed in the URL. A token you give is never printed. |
| `--no-auth` | Require no token. Refused unless `--listen` is a loopback address. |
| `--read-only` | Change nothing: every write and action is refused. |
| `--open` | Open the URL in the browser: `$BROWSER` when it is set, else the system's. |
| `--stretto PATH` | The `stretto` CLI that jobs run. Default: the one beside `stretto-console`, else the one on PATH. |

`stretto-console healthcheck [--listen ADDR]` asks `GET /api/health` on 127.0.0.1 at the port and exits with 0 when the console answers, 1 when not: a container's healthcheck. The console's own messages go to stderr, prefixed `stretto-console:`; Ctrl-C or SIGTERM stops it.

**The UI.** `console/` builds to `console/dist`, which the binary embeds: `cd console && npm ci && npm run build`, then `cargo build --release -p stretto-console`. A debug build reads `console/dist` from disk as it is. Without a build of the UI, `/` is a short page that says the API is up and how to build the UI. A release build made before `console/dist` existed does not notice it appear: `cargo clean -p stretto-console` and build again.

## The API

JSON under `/api`, `snake_case` fields. Times are milliseconds since the Unix epoch (`*_unix_ms`), sizes are bytes, probabilities are 0 to 1. An error is `{"error": "message"}` with its status: 400 for a request that is not as expected, 401 without the token, 403 for a refused change (no `X-Stretto-Console: 1`, `--read-only`, the Host check), 404 for what is not there, 409 for a name or file that exists or a commit or rollback that does not fit the files (nothing to commit, no such version), 422 for a file stretto cannot read (a flow of another format, a log that is not a session log), 500 otherwise.

| Method and path | What |
|---|---|
| `GET /api/health` | `{ok, version}`; needs no token |
| `GET /api/meta` | version, data directory, read-only, auth, whether a Jev key is set (never its value), the `stretto` and `stretto-proxy` binaries found |
| `GET /api/overview` | totals, domains, the last 14 UTC days, the 8 newest sessions, health checks (doctor's, the flows', the registry's), the 5 newest jobs |
| `GET /api/sessions?domain=&mode=&q=&limit=50&offset=0` | sessions, newest first; `mode` is `recorded`, `shadow` or `served`; `q` matches the session id, domain, agent and tools called |
| `GET /api/sessions/:key` | one session: the header, the tools listed, the LLM turns, every call (the agent's and the flow's, each lookup with the decision that made it), the conversation, the flow's decisions and runs, the confirmation judge's log, every line of the log in short |
| `GET /api/sessions/:key/raw` | the log, to download (`application/x-ndjson`) |
| `DELETE /api/sessions/:key` | moves the log, with its `.flow.jsonl` and `.confirm.jsonl`, to the trash |
| `GET /api/flows` | flows, most recently changed first; one that does not load has its `error` |
| `GET /api/flows/:key?threshold=0.3` | one flow as a reviewer reads it at that threshold: `review::view` and `flow-show`'s text, and warnings from the latest recorded `tools/list` of its domain |
| `GET /api/flows/:key/raw` | the file, to download |
| `GET /api/flows/diff?from=&to=&threshold=0.3&tolerance=0.05` | `flow-diff`: the changes to review, every change by heading, and the Markdown |
| `DELETE /api/flows/:key` | moves the file to the trash |
| `GET /api/flows/:key/stage` | the flow's staged learning (`stretto stage`), by the committed flow's key or the staged flow's: the staged flow, the last run's comparison site by site with the share's interval, its report, why a commit would be refused, what committing would change (`flow-diff` at its defaults), and every committed version, the latest first (`flow-log`) |
| `POST /api/flows/:key/commit` | `flow-commit`, with `{note}`: the staged flow becomes the committed flow, with the last comparison as its evidence |
| `POST /api/flows/:key/rollback` | `flow-rollback`, with `{to, note}`: version `to` (by default the one before) becomes the committed flow again, as a version of its own |
| `GET /api/servers` | the registry, each server with its `stretto-proxy` arguments, sessions, flow and issues; and the upstreams seen in session headers |
| `POST /api/servers`, `PUT /api/servers/:name`, `DELETE /api/servers/:name` | change the registry; a `PUT` with another name renames |
| `GET /api/servers/:name/config?host=claude-code\|claude-desktop\|cursor\|vscode` | what `stretto init` prints for the server: the snippet, where it goes, the next steps |
| `POST /api/servers/:name/probe` | a live test: `initialize` and `tools/list` now, the tools with their kinds, and what they say against the server's flow |
| `GET /api/jobs`, `GET /api/jobs/:id` | jobs, newest first, with the last 64 KiB of output; one job with all of it |
| `POST /api/jobs` | queue `learn`, `promote`, `audit`, `stage`, `redact` or `doctor` (202) |
| `POST /api/jobs/:id/cancel` | cancel a job: a queued one at once, so it never runs; a running one's `stretto` is killed, and the job ends `cancelled` (409 once it has ended) |
| `GET /api/jobs/:id/artifacts/:index` | a report or flow a job wrote, to show |
| `GET /api/settings` | the data directory's size by kind, the binaries, the key, the retention note |
| `GET /api/events` | Server-Sent Events: `changed` (`{what, keys}`, from a poll of the data directory every second), `job` (a job whose status or output moved on), and `: ping` every 15 s |
| `POST /api/logout` | clears the cookie |

A session or flow is named by its **key**: its file's stem (`20260928T020401.117Z-14662`, `shop`), with `~` and 8 hex digits of the SHA-256 of its path appended when two files share a stem. Keys stay the same across restarts while the files do. Every item also has its `path`, relative to the data directory.

**Types.** With the `ts` feature, every type the API answers with, and each request body, derives `ts_rs::TS`, and `make types` (`cargo test -p stretto-console --features ts --lib api::typescript`) writes them as TypeScript to `console/src/api/generated/` (one file per type, and `index.ts`). The test fails when the files it found there differed, so CI fails until the change to a type is committed with its TypeScript. `src/api/typescript.rs` lists the types. The feature is off by default, so a build, and coverage, leave the derives out.

**Jobs** run the `stretto` CLI as subprocesses, one at a time in the order they were queued (a cancelled one is skipped, and a running one's CLI killed), in the data directory, with the console's environment: the keys and the salt the console was started with reach the CLI, and the console itself only checks whether they are set. A request's paths are relative to the data directory, or start with `~/`; `learn` and `promote` never write over a file unless asked (`overwrite`). `learn` writes `<domain>.flow.json` by default, and fits no arbiter unless `habit_only` is false and a key is set; `promote` writes `<flow name>.promoted.flow.json` beside the flow; `audit` writes its report as JSON and Markdown; `stage` learns `<name>.staged.flow.json` beside the flow, and writes its comparison as JSON and Markdown; `redact` needs `STRETTO_REDACT_SALT`. `doctor` checks the data directory the console serves (`stretto doctor --data`).

## Security

- **The token** guards all of `/api` but `/api/health`: the cookie `stretto_console` (set by opening the UI with `?token=`, `HttpOnly; SameSite=Strict; Path=/`) or `Authorization: Bearer`. It is compared in constant time, through SHA-256 digests. The UI's files are served without it; they hold no data.
- **Changes** (POST, PUT, DELETE) also need `X-Stretto-Console: 1`, which a cross-site form cannot send. `--read-only` refuses every change and action, the connection test and jobs included.
- **`--no-auth`** is refused unless the console listens on a loopback address, and then answers only requests whose Host is `localhost`, `127.0.0.1` or `[::1]` with its port, so a page whose name resolves to 127.0.0.1 (DNS rebinding) gets nothing.
- **Paths.** A path a request names must resolve inside the data directory, or inside the home directory when it starts with `~/`; `..` cannot climb out, and no symbolic link may lead out. The scan follows a link only where it stays inside the data directory, and never enters `console/`.
- **Secrets.** The registry keeps environment variables and headers by name. It refuses a command argument, a URL user or a query parameter that looks like a credential. The console reads a variable's value only to send a header in the connection test, and never answers with one or logs it.
- **What runs.** The connection test starts a stdio server's command, with the console's environment, for 15 s at most, and kills it after. Jobs run only the `stretto` CLI.
- **Headers.** Every response carries `Content-Security-Policy: default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self'`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and `X-Frame-Options: DENY`; API responses are not cached.

## Where things are

The console reads the data directory as `stretto doctor` scans it, three levels down, leaving out `oracle-cache/` and its own `console/`: session logs (`*.jsonl`, such as `logs/<domain>/`, `shadow/<domain>/`), the flow and confirmation logs beside them, and flows (`*.flow.json`).

It writes only:

- `servers.json`, the registry: `{"stretto_servers": 1, "servers": [...]}`, written whole to a temporary file beside it and renamed over it;
- `console/jobs/<id>.json` and `<id>.log`, each job and its output, and the reports jobs write;
- `console/trash/<time>/`, where deleted sessions and flows go, at their paths. Nothing is unlinked; empty the trash when you choose;
- on a commit or a rollback, what `stretto flow-commit` and `flow-rollback` write: the committed flow, and its versions in `<name>.history/`, each written whole to a temporary file and renamed.

## Tests

The unit tests cover the keys, the scan, the session parser, flows, the registry, the auth checks and path safety. `tests/api.rs` drives every endpoint through the router (`tower::ServiceExt::oneshot`) over a copy of `tests/fixtures/home`, a small `~/.stretto` the real tools made on `stretto-mcp-demo`'s shop, whose customers are synthetic: the quickstart's six recorded sessions, its flow and two served sessions, three sessions with the flow in shadow and the promoted flow, a retail session through the guards with the confirmation judge logging, and a registry of two servers. `tests/fixtures/regenerate.sh` makes them again. The connection tests run `stretto-mcp-demo` over stdio and over HTTP, and the job tests run `stretto`, from the workspace's target directory: a flow is staged from the recorded sessions, then from those and five more, committed and rolled back.
