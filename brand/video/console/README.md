# The console film

A narrated tour of `stretto-console`, made from the real console: `capture.sh` serves the console's test fixtures (`crates/stretto-console/tests/fixtures/home`, re-dated so their sessions are from yesterday) from `/home/me/.stretto`, with the binaries in `/home/me/.cargo/bin`, and `capture.mjs` drives it in headless Chromium as a user does. It opens each page, tests the shop server's connection, opens a session and its decisions, moves a flow's threshold, opens its review and its comparison, runs an audit from the page, and opens the command palette. A second console started with `--read-only` gives the last shot. Each shot is saved at 2× in the dark theme (`shots/*.png`), with the boxes of what the film points at (`shots/shots.json`, and `shots/shots.js` for the page).

The film shows the shots in a browser window on the shared stage (`../stage/`), at the explainer's pace and in its voice ([`../PACING.md`](../PACING.md)). Its camera moves in on what each line names, a ring marks it, and a cursor goes to each control the capture clicked.

```sh
npm --prefix console ci && npm --prefix console run build
cargo build -p stretto-console -p stretto-report -p stretto-proxy
sh brand/video/console/capture.sh                       # writes brand/video/console/shots/; needs to write /home/me
cd brand
.venv-voice/bin/python video/narrate.py console-video
PYTHON=.venv-voice/bin/python node video/film.mjs console
```

Capture again whenever the console's pages change: the film reads the boxes from `shots/shots.js`, so it follows a page that moves, as long as the same controls are there.
