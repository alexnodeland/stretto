---
name: release
description: Cut a stretto release as docs/releasing.md describes, covering the version bump, the CHANGELOG section, the dry run, the vX.Y.Z tag, what release.yml and container.yml then publish, and the checks after. Use when the owner asks to prepare, dry-run or cut a release, or asks how releases work. Tagging and publishing wait for the owner's explicit go-ahead.
---

# Releasing stretto

`docs/releasing.md` is the procedure; read it first, since it is the source of truth and this skill is a summary. A version tag starts workflows that publish, so tagging, pushing a tag and running Release with `dry_run` unchecked are for the owner to decide. Prepare everything up to that point, and ask.

## What the workflows do

- **`release.yml`**, on a `v*` tag or by hand from the Actions tab:
  - `notes` checks that the tag is `v` plus the version in Cargo.toml, and takes the notes from CHANGELOG.md's `## X.Y.Z ` section, rewriting relative links to the tag. It fails before any build if either is wrong.
  - `build` makes a `--locked --release` build of the four binaries (`stretto`, `stretto-proxy`, `stretto-procedure`, `stretto-mcp-demo`) for five targets, runs each binary's `--version` where the runner can, and packs an archive with LICENSE and README.md.
  - `release` writes `SHA256SUMS`, copies `install.sh` and `install.ps1`, fills the Homebrew formula (`packaging/homebrew/fill.sh`), and creates the GitHub release once every build has passed. A tag with a hyphen (`v0.2.0-rc.1`) makes a pre-release.
  - Run by hand with `dry_run` checked (the default), it builds and packages everything, uploads it as the artifact `release`, and publishes nothing. Unchecked, it tags the commit `v<version>` itself and then starts `container.yml` on the tag.
- **`container.yml`**, on the same tag, publishes `ghcr.io/alexnodeland/stretto` for linux/amd64 and linux/arm64, tagged `X.Y.Z`, `X.Y` and `latest`. The arm64 image, built under QEMU, is the slowest step.
- Nothing goes to crates.io (docs/releasing.md, "crates.io", says why). Never run `cargo publish`.

## Prepare a release

1. **The version.** Before 1.0, bump the minor version for a change a user must act on, such as a flow format this version no longer reads, and the patch version otherwise.
2. **Bump it.** Set `version` in `[workspace.package]` in the root Cargo.toml; every crate inherits it. `cargo build` updates Cargo.lock. Then `make ci`.
3. **The changelog.** In CHANGELOG.md, turn `## Unreleased` into `## X.Y.Z (YYYY-MM-DD)` and start an empty `## Unreleased` above it. The notes are the lines under that heading, up to the next `## `.
4. **The lines that name a version** in README.md and docs/install.md: `grep -n 'v0\.\|0\.[0-9]\.[0-9]' README.md docs/install.md`.
5. **A dry run:** the owner runs Release by hand with `dry_run` checked. A macOS or Windows build failure shows up there, before any tag exists.
6. **Merge to main with CI green**, through a pull request.

## Cut it (the owner's go-ahead first)

```sh
git switch main && git pull
git tag -a vX.Y.Z -m "stretto X.Y.Z"
git push origin vX.Y.Z
```

The project settings ask before `git push` of a tag and before `gh release`.

## After

- The release page lists five archives, `SHA256SUMS`, `install.sh`, `install.ps1` and `stretto.rb`.
- `curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh -s -- --prefix /tmp/stretto-check`, then `/tmp/stretto-check/bin/stretto doctor`.
- `docker run --rm ghcr.io/alexnodeland/stretto:X.Y.Z --version`.
- The Homebrew tap, if it exists: copy the release's `stretto.rb` to `Formula/stretto.rb` there (docs/releasing.md, "Homebrew").

**When a build fails:** fix it on main, delete the tag (`git push --delete origin vX.Y.Z`, `git tag -d vX.Y.Z`) and tag the fixed commit. The next image publish replaces the tags an earlier one wrote.
