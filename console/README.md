# stretto console: the UI

The web UI of `stretto-console`, stretto's management plane. It shows, in one place, the MCP servers stretto fronts and the configuration each host needs, the sessions stretto-proxy recorded with every call and the flow's lookups after it, the flows with what they may do and how they decide, the jobs that run the `stretto` CLI, and the console's settings. The server, `crates/stretto-console`, reads the data dir (`~/.stretto`) and serves this app from its own binary.

It is Vue 3, Vite and TypeScript, with vue-router, `@vue-flow/core` and `@dagrejs/dagre` for a flow's graph, `@lucide/vue` for icons and `markdown-it` for a flow's review. The components are the console's own, on the brand kit: `brand/tokens.css` and `brand/fonts` are imported from `../brand`, not copied.

## Running it

Node 22.12 or later.

```sh
cd console
npm ci
npm run dev:mock        # the UI on a mock API: no server needed
```

Open <http://127.0.0.1:5173>. The mock is described below.

Against a real console server, start `stretto-console` (it listens on `127.0.0.1:7878` and prints a link with its token), then:

```sh
npm run dev             # proxies /api to http://127.0.0.1:7878
```

Open the link the server printed, or <http://127.0.0.1:5173> and paste the token on the sign-in screen: the server sets its cookie from `?token=`, and the dev server passes such requests on to it. `STRETTO_CONSOLE_URL` points `npm run dev` at another address.

## The mock

`npm run dev:mock` runs Vite in mock mode, where a small plugin (`mock/plugin.ts`) answers every endpoint of the console's API from an in-memory data dir (`mock/fixtures`). It keeps the real server's rules: the `X-Stretto-Console: 1` header on writes, `{"error": …}` with the right status, read-only refusals, the token cookie, and Server-Sent Events on `/api/events`.

The data is the quickstart's shop two weeks into a deployment, recorded as `stretto-proxy` records it, with the demo server's own tool results and the proxy's own decision lines:

- **shop**, `stretto-mcp-demo --world retail`: six sessions recorded without a flow (the agent calls `find_user_id_by_email`, `get_user_details`, `get_order_details` twice, and `cancel_pending_order` as a write); a flow learned from them with no key; three sessions with the flow in shadow; the flow promoted on them (3 of 4 sites); and served since. After the agent's first call, the flow reads the customer's details and both orders, at 0.99, 0.99 and 0.95 times their bindings' chances, and hands back at 0.05, below 0.3.
- **notes**, the official filesystem server, recorded from Cursor, with no flow yet.
- **tickets**, a server over Streamable HTTP that was never registered: it shows under Discovered upstreams.
- The live cold start's **retail** flow from `docs/results` (with an arbiter), a flow file that does not load, a server whose flow is missing, and the jobs that did all this, one of them failed.

The review Markdown, `flow-diff`, `promote` and `audit` texts are the real tools' output (`mock/fixtures/data/texts.json`).

| Variable               | Effect                                                                  |
| ---------------------- | ----------------------------------------------------------------------- |
| `MOCK_AUTH=1`          | Ask for a token: `stretto-mock-token`                                   |
| `MOCK_READ_ONLY=1`     | Refuse every write, as `--read-only` does                               |
| `MOCK_EMPTY=1`         | Start from an empty data dir, to see the first-run guidance             |
| `MOCK_KEY_SET=1`       | Report TYPESAFE_API_KEY as set                                          |
| `MOCK_REDACT_SALT=1`   | Let redact jobs run (the server needs STRETTO_REDACT_SALT)              |
| `MOCK_LATENCY=140`     | Milliseconds each answer waits, to see the skeletons                    |
| `MOCK_TRAFFIC=0`       | Stop the new served session that arrives every 45 s (`MOCK_TRAFFIC_MS`) |
| `MOCK_JOB_STEP_MS=280` | How fast a job writes its output                                        |

The tests change the same settings at run time with `POST /__mock/state` (`{"reset": true}`, `{"auth": true}`, `{"empty": true}`, …).

## Scripts

| Script              | What it runs                                                                                                 |
| ------------------- | ------------------------------------------------------------------------------------------------------------ |
| `npm run dev`       | Vite, proxying `/api` to the console server                                                                  |
| `npm run dev:mock`  | Vite with the mock API                                                                                       |
| `npm run build`     | `vue-tsc --noEmit`, then `vite build` into `dist/`, which the server embeds                                  |
| `npm run typecheck` | `vue-tsc` on the app, then on the config, the mock and the end-to-end tests                                  |
| `npm run lint`      | ESLint (typescript-eslint, eslint-plugin-vue)                                                                |
| `npm run format`    | Prettier; `npm run format:check` only checks                                                                 |
| `npm test`          | Vitest with happy-dom: the API client, the formatters, the timeline and flow logic, and the key components   |
| `npm run e2e`       | Playwright against `dev:mock`                                                                                |
| `npm run shots`     | Playwright: every page in light and dark at 1440 and 390 px, into `SHOTS_DIR` (default `test-results/shots`) |

Playwright is pinned to 1.56.1. It uses the Chromium in `PLAYWRIGHT_BROWSERS_PATH` when that is set; elsewhere, `npx playwright install chromium` fetches it.

## Layout

```text
console/
  index.html            the page; public/theme-init.js applies a chosen theme before the first paint
  src/
    api/types.ts        the API's types, field for field as the spec names them
    api/client.ts       fetch with the cookie, X-Stretto-Console on writes, a 401 to sign-in, errors to toasts
    api/events.ts       GET /api/events: one EventSource, subscriptions by topic
    api/generated/      (to come) the same types, which `make types` generates from the server's DTOs
    stores/             auth and meta, theme, domain filter, toasts, running jobs
    composables/        useResource (load, refresh on events, keep the frame), hotkeys, copy, title
    lib/                formatting, the timeline and a flow's preview and graph, shell words, validation, Markdown
    styles/             the brand's tokens and fonts, and the console's base styles
    components/ui/      buttons, badges, cards, tabs, dialogs, fields, the JSON tree, the KPI tile, the meter
    components/…        the shell, the activity chart, the session timeline, the flow graph, the server form
    pages/              Overview, Servers, Sessions, Flows, Jobs, Settings, and each one's detail
  mock/                 the mock API: plugin.ts, api.ts, derive.ts, and fixtures/ (the world, the recorder, texts)
  tests/unit/           Vitest
  e2e/                  Playwright, and shots.spec.ts for the screenshots
```

## Notes

- **Types.** `src/api/types.ts` mirrors the spec until the server's DTOs are generated into `src/api/generated/` with ts-rs (`make types`, from the repository's root). Switching is a change of import path.
- **Content Security Policy.** The server sends `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self'`. So the build has no inline script (the theme's early script is a file), fonts are never inlined as `data:` URIs, and everything, fonts included, is served from the console itself.
- **Color.** Petrol marks what stretto does: the reads it made, a lookup that acts at a threshold, the primary action. The rest is ink and paper. Shadow, where the flow decides but looks nothing up, is a dashed petrol outline. The brand has no status colors, so errors and warnings always come with an icon and words; the console's own two (`--c-danger`, `--c-warn` in `src/styles/tokens.css`) are text-safe on every surface in both themes.
- **Charts.** The activity chart plots the agent's calls (the brand's baseline slate) with the reads stretto made on top (petrol), on one axis; sessions per day are in its tooltip and its table view, not on a second scale. Hover or focus a day to read it; the table has every value.
- **Scope.** Every number says what it counts: the KPI tiles name their period and that they cover every domain, a decision's value says the threshold it is weighed against and where that threshold came from.
