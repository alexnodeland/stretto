# The console

`stretto-console` is stretto's management plane: a web app on your own machine, over the files stretto writes to `~/.stretto`. In one place it shows:

- **Servers.** The MCP servers stretto fronts. Each has its upstream (a command, or a Streamable HTTP URL), its mode (recording, shadow or serving), its flow, and the configuration to paste into Claude Code, Claude Desktop, Cursor or VS Code. A connection test lists the server's tools as they are now, and says where the flow's view of them no longer holds.
- **Sessions.** Every recorded session, call by call:
  - the agent's calls and the flow's lookups, with their arguments and results;
  - why each lookup was made (its share × binding against the threshold), and why the flow handed back;
  - the conversation, and the confirmation judge's log.
- **Flows.** Each flow as a reviewer reads it:
  - the graph of calls and the lookups that follow them;
  - the sites and their bindings;
  - what would act at another threshold;
  - the difference from another flow.
- **Jobs.** `learn`, `promote`, `audit`, `redact` and `doctor`, run from the page, with their output as it comes and the files they wrote.

It reads everything through stretto's own code, so what it shows is what `stretto flow-show`, `stretto init` and `stretto doctor` say. The one file it owns is `servers.json`, the registry of servers. [The crate's README](../crates/stretto-console/README.md) documents its API.

## Start it

```sh
stretto-console
# stretto-console: /home/me/.stretto on 127.0.0.1:7878
# stretto-console: open http://127.0.0.1:7878/?token=3f9c…
```

Open the URL it prints: the token in it signs the browser in. `--open` opens it for you, and `--data DIR` serves another data directory than `~/.stretto` (or `$STRETTO_HOME`). Ctrl-C stops it.

The release archives, `install.sh` and `install.ps1` install `stretto-console` beside `stretto`, from the first release after 0.1.0 ([installing](install.md)). Until then, build it from a checkout.

### From a checkout

The UI is built with Node 22.12 or later, and the binary embeds it:

```sh
make console          # builds the UI, then runs the console on ~/.stretto
```

`make console ARGS="--read-only --open"` passes options through. To install the binary, build the UI first, since the release build embeds what `console/dist` holds when it compiles:

```sh
npm --prefix console ci && npm --prefix console run build
cargo install --locked --path crates/stretto-console
```

A build made without `console/dist` serves the API and, at `/`, a page that says how to build the UI.

### In a container

`ghcr.io/alexnodeland/stretto-console` is the console, with the `stretto` CLI beside it for its jobs. It serves on port 8080 in the container, and keeps its data in `/data/.stretto`. [`compose.yaml`](../compose.yaml) runs it over your `~/.stretto`:

```sh
STRETTO_UID=$(id -u) STRETTO_GID=$(id -g) docker compose up -d
docker compose logs console     # the URL to open, with its token
```

Or with `docker run`:

```sh
docker run -d --name stretto-console -p 127.0.0.1:8080:8080 \
  --user "$(id -u):$(id -g)" -v "$HOME/.stretto:/data/.stretto" \
  ghcr.io/alexnodeland/stretto-console
docker logs stretto-console
```

It runs as your user, so that the files the console writes (`servers.json`, the jobs, the trash) are yours; on macOS and Windows, Docker Desktop maps ownership and `--user` can be left out. The port is published on 127.0.0.1 only. The image's health check runs `stretto-console healthcheck`. `STRETTO_CONSOLE_TOKEN` fixes the token, so the URL stays the same across restarts, and `TYPESAFE_API_KEY`, when set, reaches the jobs that fit an arbiter; compose passes both through from your environment. Build the image from a checkout with `docker build --target console -t stretto-console .`.

## Sign in

Every request to the API needs the console's token, and every change also needs a header that a page on another site cannot send.

- **The token** is new at each start and printed in the URL. `--token` or `STRETTO_CONSOLE_TOKEN` fixes it, and a token you give is never printed: open `http://127.0.0.1:7878/?token=<your token>`. Opening the URL sets a cookie (HttpOnly, SameSite=Strict), and the page reloads without the token in it. *Sign out* in Settings clears the cookie.
- **Scripts** send `Authorization: Bearer <token>`, and `X-Stretto-Console: 1` on POST, PUT and DELETE:

  ```sh
  curl -H "Authorization: Bearer $STRETTO_CONSOLE_TOKEN" http://127.0.0.1:7878/api/overview
  ```

- **`--no-auth`** turns the token off, and only on a loopback address (`127.0.0.1`, `[::1]`). It then answers only requests addressed to `localhost`, `127.0.0.1` or `[::1]`, so another site that resolves its name to your machine gets nothing.
- **`--read-only`** refuses every change: no edits to the registry, no connection tests, no jobs, nothing moved to the trash.

## From another machine

The console serves plain HTTP. To use one that runs on a server, forward its port over SSH rather than listening on a public address:

```sh
ssh -L 7878:127.0.0.1:7878 me@server stretto-console
```

Then open the printed URL on your own machine. On a shared machine, `--read-only` gives a view that cannot change anything. To serve it more widely, put it behind a reverse proxy that terminates TLS, and keep the token.

## The pages

| Page | What it shows, and what you can do there |
|---|---|
| Overview | This week's sessions, calls, lookups served and shadow lookups; the last 14 days; each domain with its mode, flow, server and last session; the newest sessions and jobs; health checks. With no data yet, the three steps to get some: add a server, use your agent, learn a flow. |
| Servers | The registry. Add or edit a server: a command or a URL, the environment variables and headers it needs (by name), its mode, flow, decider and threshold. Copy the host configuration for your MCP host. Run the connection test. Upstreams found in recorded sessions but not in the registry can be added in one click. |
| Sessions | Every session, filtered by domain, mode or text. A session's page shows its timeline: the conversation, each call with its arguments and result, the lookups the flow made after it ("read ahead by stretto"), and each decision with its probability against the threshold. Tabs hold every decision in a table and the raw log. |
| Flows | Every flow. A flow's page shows its graph and a threshold slider that shows which lookups would act. It has the sites with their bindings, the tools (writes flagged), the review as `flow-show` prints it, and the raw JSON. From there you can compare it with another flow, download it, promote it or audit it. |
| Jobs | New jobs, the queue, and each job's live output and the files it wrote, such as a new flow or an audit's report. |
| Settings | The data directory and its size by kind, the binaries found, whether a Jev key is set (never its value), the read-only mode, the theme, and sign out. |

The console follows the data directory as it changes: a new session, flow or job shows up within a second, without a reload.

## What it changes

It writes three things, all in the data directory:

- `servers.json`, the registry, each time you save a server;
- `console/jobs/`, each job with its output, and the reports jobs write;
- `console/trash/`, where a deleted session or flow is moved, never unlinked. Empty it when you choose.

It never edits an MCP host's configuration: it shows the snippet to paste, as `stretto init` prints it.

## Jobs

Jobs run the `stretto` CLI in the data directory, one at a time, in the order they were queued, with the console's environment. So a job that fits an arbiter needs `TYPESAFE_API_KEY` (or `TYPESAFE_API_KEY_FILE`) where the console runs, and `redact` needs `STRETTO_REDACT_SALT`. The console only checks whether they are set. `learn` and `promote` never write over an existing file unless you ask. `doctor` checks the data directory the console serves. A job cannot be stopped from the page: stopping the console stops it, and the job is marked as failed at the next start.

## Limits

- **The connection test** starts a stdio server's command on the machine the console runs on, with the console's environment, for 15 seconds at most. In the container, which has no Node or Python, it can test a Streamable HTTP server, but not a stdio server that needs them; run the console on your own machine for those.
- **One user.** Anyone with the token can do anything the console does. `--read-only` is the only role.
- **Plain HTTP.** See [from another machine](#from-another-machine).
