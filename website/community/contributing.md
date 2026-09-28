---
description: How to build, test and change stretto - the workspace, the checks CI runs, the conventions, the generated documentation, and this site.
---

# Contributing

Issues and pull requests are welcome on [GitHub](https://github.com/alexnodeland/stretto). What is left to do is tracked in issues, all sub-issues of [#1](https://github.com/alexnodeland/stretto/issues/1), and grouped with context on [the roadmap](./roadmap).

## The workspace

```text
crates/
  stretto-trace/   the episode schema; τ²-bench and MCP proxy log ingest; tool manifests; redaction
  stretto-model/   the action abstraction, the back-off habit, the α posterior (with fugue), argument provenance
  stretto-oracle/  the Oracle trait, the Jev client, the replay cache, a mock
  stretto-report/  the stretto CLI: phase0, learn, compile, serve, audit, flow-show, flow-diff, promote, redact, ...
  stretto-proxy/   stretto-proxy, stretto-procedure and stretto-mcp-demo
pilot/             live τ²-bench episodes behind the proxy, and replays
scripts/           the walkthrough, the benchmarks round, and each results page's analysis
docs/              the design, formats, CLI reference, privacy, review, roadmap, and every result
paper/             the working paper
site/              the research notebook
website/           this site
examples/          the no-key quickstart that CI runs
packaging/         the Homebrew formula and the installers' test
brand/             the brand kit, the explainer and the videos
```

## Build and test

```sh
make help   # the everyday commands
make fmt    # cargo fmt --all
make ci     # the formatting check, clippy, the tests and doctests, and the API docs
```

`make test` runs the unit tests, and Phase 0 on a miniature fixture checkout. CI runs what `make ci` runs, checks that the workspace builds with Rust 1.88, the oldest it supports (`make msrv`), and measures line coverage, failing under a threshold (`make coverage`). Then it runs:

- [the quick start's demo](/guide/quick-start#2-see-the-whole-loop-with-no-key), with no key and no network (`sh examples/quickstart/run.sh --bin target/debug`);
- `install.sh` against a local release of stand-in binaries (`sh packaging/test-install.sh`), and shellcheck on the shell scripts;
- [the walkthrough](/guide/walkthrough), end to end on the official MCP filesystem server (`python3 scripts/walkthrough.py --bin target/debug`);
- the proxy in front of the reference Streamable HTTP server (`python3 scripts/http_check.py --bin target/debug`);
- the doctests of `scripts/telecom_workflow.py` and `scripts/ceiling.py`, and the benchmarks round's scripts on a fixture.

On macOS and Windows, CI builds the five programs, runs each one's `--version`, and runs the console's tests.

## Conventions

- **One schema.** Everything downstream reads `stretto_trace::Episode`, never a benchmark's native format. Add a new source as an ingest module.
- **Posteriors, not frequencies.** Anything that drives a decision is a posterior predictive, with its evidence counts next to it. Use closed forms where they exist, and fugue where they do not.
- **Replay by default.** Oracle calls go through the replay cache, so experiments pay for each distinct question once and re-run for free. Never commit cache contents that contain third-party data.
- **Honest reports.** Every metric in a report gets a definition. A proxy must say it is a proxy.
- **Secrets** come from the environment. Never log or commit them.

## Documentation that is generated or checked

- **The CLI reference**, [`docs/cli.md`](../../docs/cli.md), is generated from the code. After changing an option, run `STRETTO_BLESS=1 cargo test --bins` to rewrite it; CI fails while it is out of date.
- **The examples in [reviewing flows](/reference/review)** are what `flow-show` and `flow-diff` print for real flows. `STRETTO_BLESS=1 cargo test` rewrites them.
- **The file formats**: a test fails when a serialized field of a flow or an arbiter is missing from [`docs/formats.md`](../../docs/formats.md).
- **The walkthrough** runs in CI, so it cannot drift from the code.

## This site

The site lives in `website/` and is built with [VitePress](https://vitepress.dev). Its reference, research and community pages include the repository's own Markdown, so edit those files where they are (the **Edit this page on GitHub** link goes there), not the pages that include them.

```sh
cd website
npm ci
npm run dev       # a local server with hot reload
npm run build     # the static site, in website/.vitepress/dist
npm run preview   # serve the build
```

- A page includes a repository file with VitePress's include directive: an HTML comment holding `@include:` and the file's path relative to the page, as in each page of `website/reference/`.
- Links in the repository's Markdown stay relative to where the file lives. The build rewrites them: to the page that renders the file, when the site has one, and otherwise to the file on GitHub. A link to a file that does not exist fails the build, as does a link to a page that does not exist.
- Headings get GitHub's anchors, so `formats.md#program` works on GitHub and here alike.
- Math in `$…$` and `$$…$$` renders with MathJax, and a ```` ```mermaid ```` block renders as a diagram.

The GitHub Pages workflow builds and deploys the site on every push to `main` that touches it or the files it includes.

## License

stretto is released under the [MIT License](./license).
