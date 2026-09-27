## What this changes

<!-- One or two sentences: what and why. Link the issue it closes. -->

## How it was checked

<!-- Tests added or run, and any replay or live run with its numbers and scope. -->

- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] `docs/cli.md` regenerated (`STRETTO_BLESS=1 cargo test`) if a command or option changed
- [ ] Docs and `CHANGELOG.md` (*Unreleased*) updated
- [ ] Any number quoted comes from a results page, and the claims ledger is updated if a headline changed
