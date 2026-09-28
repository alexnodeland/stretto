---
name: check
description: Run exactly what stretto's CI runs (formatting, clippy, tests and doctests, rustdoc, and the end-to-end walkthrough job when it applies) and interpret any failure. Use before committing or pushing, before opening or updating a pull request, or to reproduce a failed CI job locally.
---

# Check before pushing

CI is `.github/workflows/ci.yml` (jobs `check`, `msrv`, `walkthrough` and `platforms`) and `.github/workflows/coverage.yml`, on every pull request and push to main. `container.yml` runs when the image's inputs change; `pages.yml` builds the site only after a merge. Run what applies to the change, in this order, and fix the first failure before going on.

## 1. Always: the check job

```bash
make ci    # make check lint test doc
```

That is `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, `cargo test --doc` and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, as CI runs them.

## 2. When the proxy, the CLI, an example or a script changed: the walkthrough job

```bash
cargo build -p stretto-report -p stretto-proxy
BIN=${CARGO_TARGET_DIR:-target}/debug
sh examples/quickstart/run.sh --bin "$BIN"         # no key, no network
sh packaging/test-install.sh                        # install.sh against a local release
shellcheck install.sh examples/quickstart/run.sh packaging/test-install.sh packaging/homebrew/fill.sh
python3 scripts/walkthrough.py --bin "$BIN"         # needs Node: npx fetches the MCP filesystem server
python3 scripts/http_check.py --bin "$BIN"          # needs Node: the reference Streamable HTTP server
python3 -m doctest scripts/telecom_workflow.py
python3 -m doctest scripts/ceiling.py
bash -n scripts/bench/replay.sh
python3 scripts/bench/tables.py --work scripts/bench/fixture --json /tmp/rows.json
```

`make quickstart` and `make walkthrough` run the first and the fifth. CI also asserts one cell of the fixture's table; the command is in ci.yml. Skip shellcheck if it is not installed, and say so.

## 3. When it applies

- **Dependencies, `Cargo.lock` or a newer std API:** `make msrv` checks the workspace on Rust 1.88 with the lockfile (it installs that toolchain, about 550 MB).
- **Tests removed, or much new code:** `make coverage` fails under `COVERAGE_MIN` in the Makefile. The instrumented build needs about 4 GB of disk.
- **`website/`, `docs/`, `paper/`, `site/`, `brand/` or `CHANGELOG.md`:** `make site`, which fails on a dead link. CI does not build the site before the merge.
- **Platform-specific code:** the `platforms` job builds on macOS and Windows, which cannot be run here. Keep Unix-only APIs behind `#[cfg(unix)]`.
- **The Dockerfile or what goes into the image:** `make docker`, then the checks in container.yml's `build` job.

## Reading failures

| Failure | What it means | What to do |
|---|---|---|
| `cargo fmt` prints a diff | Code is not formatted | `make fmt` |
| A clippy lint | Warnings are errors | Fix the code. Add an `#[allow]` only with a comment saying why |
| `docs/cli.md is out of date for stretto` (or `stretto-proxy`, `stretto-procedure`) | A command, option or help text changed | `make bless`, read the diff of `docs/cli.md`, commit it |
| `docs/review.md is out of date` | `flow-show` or `flow-diff` output changed | `make bless`, read the diff, commit it |
| `docs/formats.md does not name [...]` | A flow or arbiter file gained a field | Document the field in `docs/formats.md` |
| A rustdoc error such as `links to private item` or an unresolved link | The public docs link to what they cannot reach | Fix the path, or make a private item a code span |
| MSRV: `rustc 1.88 is not supported by the following packages` | A locked dependency needs a newer Rust | Pin that dependency to a release that supports 1.88 (`cargo update -p NAME --precise VERSION`), or raise `rust-version` in Cargo.toml and the msrv job's toolchain together, with a CHANGELOG line |
| Coverage under the threshold | Tests cover less than `COVERAGE_MIN` | Add tests. Lower the threshold only with a reason in the commit |
| `quickstart: FAILED: ...` | The loop no longer does what `examples/quickstart/README.md` says | The message names the step. Fix the code, or the README and the script together |
| `scripts/walkthrough.py` fails | The code and `docs/walkthrough.md` disagree | Change them together: CI exists so they cannot drift |
| `npx` cannot fetch a server | No network here | Report it as unverified; CI runs it |

Report what ran, what passed, and anything not run, with the reason.
