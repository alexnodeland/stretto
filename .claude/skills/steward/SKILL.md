---
name: steward
description: Drive a stretto pull request to green and ready to merge, covering which CI jobs run and what each checks, how to reproduce and fix a failing job, answering review comments, and the repository's conventions for branches, commits and PR descriptions. Use when opening a PR, when CI fails on one, when review comments arrive, or when asked to babysit or shepherd a PR.
---

# Taking a pull request to green

## The jobs

| Workflow, job | Runs on | What it checks | Reproduce |
|---|---|---|---|
| ci.yml `check` | every PR and push to main | `cargo fmt --check`, clippy with `-D warnings`, `cargo test --all-targets`, `cargo test --doc`, the API's TypeScript is current, rustdoc with `-D warnings` | `make ci` |
| ci.yml `ui` | every PR and push to main | the console's UI: Prettier, ESLint, vue-tsc, Vitest, the build, and Playwright on the mock API (with axe-core's WCAG 2.1 A and AA rules on every page) and on the console over the fixtures | `make ui-check`, `make e2e` |
| ci.yml `msrv` | every PR and push to main | `cargo +1.88.0 check --workspace --all-targets --locked`, and that Cargo.toml's `rust-version` is still 1.88 | `make msrv` |
| ci.yml `walkthrough` | every PR and push to main | the quickstart, `packaging/test-install.sh`, shellcheck, `scripts/walkthrough.py`, `scripts/http_check.py`, the scripts' doctests and the benchmark tables' fixture | the `check` skill, step 2 |
| ci.yml `platforms` | every PR and push to main | a `--locked` build on macOS 14 and Windows, and each binary's `--version` | not locally: read the log |
| coverage.yml | every PR and push to main | `make coverage`: line coverage at or above `COVERAGE_MIN`. The LCOV report is the run's artifact | `make coverage` |
| container.yml `build` | PRs and pushes to main that change the Dockerfile, `.dockerignore`, `crates/`, `console/`, the brand files the UI imports, the manifests, the quickstart or `compose.yaml` | both images build. In the CLI's, each binary, `stretto doctor` and the quickstart run. The console's starts healthy as the runner's user, refuses a request without its token, serves the built UI, and runs a `doctor` job | `make docker`, `make docker-console` |
| pages.yml | pushes to main only | the site builds with no dead link | `make site`, before merging |

## The loop

1. Read the failing job's log first and find the first error; later ones often follow from it. With the GitHub tools, list the PR's check runs and read the failed job's log.
2. Reproduce it locally with the command in the table. The `check` skill explains each kind of failure.
3. Fix the cause, not the check. Do not weaken a test, a lint or the coverage threshold to get green unless the owner agrees, and then say so in the PR.
4. Run `make ci` again (and the job's own command), commit, and push to the PR's branch. In a Claude Code on the web session, git can push only the session's branch.
5. Wait for the new run, and repeat until every job is green.

A job that fails without any change (a network error in `npx`, or a runner problem) can be re-run once. If it fails again, report it rather than retrying in a loop.

## Review comments

- Answer each comment where it was made. Say what changed, with the commit, or why not.
- When a comment asks for a change, make it in a new commit. Keep the history readable, and do not force-push a branch someone is reviewing unless the owner asks.
- Resolve a thread only when the reviewer's point is addressed.

## Conventions

- **Branches:** one purpose per branch, from main. Never push to main or force-push it; merging is the owner's call.
- **Commits:** the style of `git log`. A subject that says what changed in plain words, with no type prefix and no final period (for example "Rustdoc: no links from public docs to private items"). The body is prose saying why and how, with a list when there are several parts. One logical change per commit, formatted and passing `make ci`.
- **The PR description** follows `.github/pull_request_template.md`: *What this changes* (one or two sentences, and the issue it closes), *How it was checked* (tests run, and any replay or live run with its numbers and scope), and the checklist, ticked only for what was done.
- **In the same PR:** `docs/cli.md` regenerated (`make bless`) when a command or option changed; `docs/formats.md` for a file format; the guide pages in `website/`; CHANGELOG.md under *Unreleased*; and the claims ledger, if a quoted number changed (the `results` skill).
- **Writing:** calm and exact, with every number's scope (brand/messaging.md). stretto is lowercase.
