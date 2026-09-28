# Releasing stretto

A version tag, `vX.Y.Z`, pushed to the commit on `main` to release, starts two workflows:

- [`release.yml`](../.github/workflows/release.yml) makes the GitHub release. Its notes are the version's section of [CHANGELOG.md](../CHANGELOG.md). It builds the console's UI once, then carries an archive of the five binaries (`stretto-console` with the UI built in), with `LICENSE` and `README.md`, for each of five targets, their `SHA256SUMS`, [`install.sh`](../install.sh), [`install.ps1`](../install.ps1), and `stretto.rb`, the Homebrew formula with the checksums filled in.
- [`container.yml`](../.github/workflows/container.yml) publishes `ghcr.io/alexnodeland/stretto` and `ghcr.io/alexnodeland/stretto-console` for linux/amd64 and linux/arm64, tagged `X.Y.Z`, `X.Y` and `latest`.

Nothing is published to crates.io ([below](#cratesio)). [docs/install.md](install.md) is what users read.

## Check a release first

Run the Release workflow by hand from the Actions tab with *dry_run* checked, the default. It builds and packages the five targets and runs each binary's `--version` where the runner can. It also writes `SHA256SUMS`, the installers and the formula. It uploads all of that as the run's artifact `release`, and tags and publishes nothing, so a build that fails on macOS or Windows shows up before any tag does.

## Cut a release

1. **Choose the version.** Before 1.0, bump the minor version for a change that a user must act on, such as a flow format this version no longer reads, and the patch version otherwise.
2. **Bump it.** Set `version` in `[workspace.package]` in the root `Cargo.toml`; every crate inherits it. Run `cargo build`, which updates the crates' versions in `Cargo.lock`, and the checks CI runs: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`.
3. **Write the changelog section.** In `CHANGELOG.md`, turn `## Unreleased` into `## X.Y.Z (YYYY-MM-DD)` and start an empty `## Unreleased` above it. The release notes are the lines under the heading that starts with `## X.Y.Z `, up to the next `## `; relative links in them are rewritten to point at the tag.
4. **Update the lines that name a version** in README.md and docs/install.md (`grep -n 'v0\.\|0\.[0-9]\.[0-9]' README.md docs/install.md`).
5. **Merge to `main`** with CI green.
6. **Tag it and push the tag:**

   ```sh
   git switch main && git pull
   git tag -a vX.Y.Z -m "stretto X.Y.Z"
   git push origin vX.Y.Z
   ```

   `release.yml` checks that the tag is `v` plus Cargo.toml's version and that CHANGELOG.md has the section, before it builds anything. It creates the release only once all five builds have passed, so a failed build publishes no release. `container.yml` runs on the same tag on its own; the arm64 image is built under emulation and is the slowest step.
7. **Check the release:**
   - the release page lists five archives, `SHA256SUMS`, `install.sh`, `install.ps1` and `stretto.rb`;
   - `curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh -s -- --prefix /tmp/stretto-check`, then `/tmp/stretto-check/bin/stretto doctor`;
   - `docker run --rm ghcr.io/alexnodeland/stretto:X.Y.Z --version`, and the same for `stretto-console`;
   - the first time only: the packages `stretto` and `stretto-console` appear under the account's packages on GitHub. If one is private, make it public in its settings (Package settings, Change visibility), and link it to the repository if it is not linked.
8. **Update the Homebrew tap** ([below](#homebrew)).

**By hand, instead of pushing a tag:** in the Actions tab, run *Release* on `main`. It tags the commit it runs on `v` plus Cargo.toml's version, makes the release, and then starts *Container* on the new tag, since a tag made with the workflow's own token starts no workflow by itself.

**Pre-releases.** A tag with a hyphen, such as `v0.2.0-rc.1`, makes a GitHub pre-release. `releases/latest`, and so `install.sh` and `install.ps1` by default, stay on the last full release; `install.sh --version v0.2.0-rc.1` installs it. Its image is tagged `0.2.0-rc.1` only.

**When a build fails.** Fix it on `main`, delete the tag (`git push --delete origin vX.Y.Z`, `git tag -d vX.Y.Z`) and tag the fixed commit. If `container.yml` had already published the image, the next publish replaces its tags.

## Why the release workflow is written by hand

cargo-dist can generate a release workflow with archives, checksums, shell and PowerShell installers and a Homebrew tap. It was not used, because the hand-written workflow is simpler to maintain here:

- cargo-dist owns the workflow it generates: it writes it again on each cargo-dist upgrade, and its check fails when the file differs from what it would write, unless edits are allowed and then carried by hand. This release has steps of its own that would have to fit its model or be patched again after each regeneration: notes from CHANGELOG.md with links rewritten to the tag, the run by hand that tags the commit, starting the container workflow, and the check that the tag is Cargo.toml's version.
- The hand-written one is one workflow file, `install.sh`, `install.ps1` and `packaging/homebrew/fill.sh`, which run no tool beyond rustup, cargo, tar, 7z, sha256sum and gh, and CI tests `install.sh` against a local release (`packaging/test-install.sh`).
- What cargo-dist would add is not needed yet: MSI installers, publishing to the tap with a token, and an updater. If it is, cargo-dist can take over: it writes its own `release.yml`.

## Targets

| Target | Runner | Notes |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | glibc 2.35 or later |
| `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` | GitHub's arm64 runner, free for public repositories; glibc 2.35 or later |
| `aarch64-apple-darwin` | `macos-14` | |
| `x86_64-apple-darwin` | `macos-14` | built on Apple silicon, for Intel: GitHub is retiring its Intel macOS runners |
| `x86_64-pc-windows-msvc` | `windows-latest` | tested only with `--version` |

Each build runs `cargo build --locked --release`, then each binary's `--version` where the runner can run it (all but the Intel macOS build).

The Linux builds run on Ubuntu 22.04 for its glibc: a binary needs the glibc it was linked against, or a later one. When GitHub retires `ubuntu-22.04`, moving to `ubuntu-24.04` raises the floor to glibc 2.39, which leaves out Debian 12 and Ubuntu 22.04; building in an older container, or with cargo-zigbuild and a glibc version, keeps it lower.

## The container image

The [Dockerfile](../Dockerfile) builds with `rust:RUST_VERSION-slim-trixie` and runs on `debian:trixie-slim`; both are Debian 13, so the binaries find the glibc they were linked against. Bump `RUST_VERSION` when the release should build with a newer Rust. The image runs as the user 10001 with `HOME=/data`, and has no CA certificates: reqwest uses rustls with the Mozilla roots of `webpki-roots` built in, and the Jev client also trusts `SSL_CERT_FILE`.

`.dockerignore` lets in only the manifests, `crates/`, the quickstart, `LICENSE` and `README.md`. It leaves out `rust-toolchain.toml`, whose `stable` channel would make rustup in the builder fetch another toolchain than the pinned one.

Pull requests that change the image's inputs build it for linux/amd64 and run each binary, `stretto doctor` and the quickstart in it, with nothing published. Only the tag job can write packages. If the arm64 build under QEMU grows too slow, build each platform on its own runner (`ubuntu-24.04-arm` for arm64) and join them with `docker buildx imagetools create`.

## Homebrew

Each release carries `stretto.rb`, filled from [`packaging/homebrew/stretto.rb`](../packaging/homebrew/stretto.rb) by [`fill.sh`](../packaging/homebrew/fill.sh). It installs the release binaries for macOS and Linux and the completions for bash, zsh and fish. For `brew install alexnodeland/tap/stretto` to work:

1. Create the public repository `github.com/alexnodeland/homebrew-tap`. It does not exist yet; creating it is the repository owner's decision.
2. For each release, copy the release's `stretto.rb` to `Formula/stretto.rb` in the tap and push. `brew install alexnodeland/tap/stretto`, `brew test alexnodeland/tap/stretto` and `brew audit --strict --online alexnodeland/tap/stretto` check it.
3. To do this in `release.yml` instead, add a step that pushes the formula to the tap with a token that can write to it, kept as a repository secret (a fine-grained token limited to the tap). It is not set up.

## crates.io

stretto is not on crates.io, for three reasons.

1. **fugue comes from git.** crates.io takes a crate only if every dependency comes from a registry, and `fugue-ppl` is taken from a pinned git revision for its `program` feature. `cargo package -p stretto-model` fails today with *all dependencies must have a version requirement specified when packaging. dependency `fugue-ppl` does not specify a version*. crates.io has fugue-ppl 0.2.2 and earlier, none with `program`; the pinned revision is 0.2.3. A fugue-ppl release with `program` unblocks it; the workspace then takes it by version, `fugue = { package = "fugue-ppl", version = "…", features = ["program"] }`, in place of `git` and `rev`.
2. **The crates depend on each other by path.** Each of those dependencies in `[workspace.dependencies]` also needs a version, such as `stretto-model = { path = "crates/stretto-model", version = "0.1.0" }`. They are published in order: `stretto-trace`, `stretto-oracle`, `stretto-model`, then the CLI's package, then `stretto-proxy`.
3. **The name `stretto` is taken** on crates.io by an unrelated crate, a cache library. The binary keeps its name either way; the question is the CLI's package, now `stretto-report`. The proposal: publish it as `stretto-cli`, with `[[bin]] name = "stretto"` as now, so that `cargo install stretto-cli` installs `stretto`, and keep `stretto-proxy`, `stretto-trace`, `stretto-model` and `stretto-oracle` as they are. On 2026-09-27 the crates.io index had none of `stretto-cli`, `stretto-report`, `stretto-proxy`, `stretto-procedure`, `stretto-trace`, `stretto-model` and `stretto-oracle`. Renaming is a decision for the maintainer; nothing has been renamed.

The metadata crates.io shows is in the manifests already: each crate's description, keywords, categories, readme, homepage and documentation.
