#!/usr/bin/env bash
# SessionStart hook: prepares a fresh Claude Code on the web container, so
# that `make check`, `make lint`, `make test` and `make site` work from the
# first prompt:
#
#   1. rustfmt and clippy for the toolchain rust-toolchain.toml pins;
#   2. the workspace's crates, for this machine's target (cargo fetch --locked);
#   3. the node modules of website/, and of console/ once it exists, when they
#      are missing or older than the package-lock.json.
#
# It runs only in cloud sessions (CLAUDE_CODE_REMOTE=true) and leaves a local
# machine alone. Each step is skipped when it is already done, so a prepared
# container costs well under a second. A step that fails, for example when
# the network is restricted, is reported and skipped: the hook always exits 0,
# so the session starts either way. Progress goes to stderr; stdout, which
# Claude reads as context, gets one line on what is ready and what is not.
#
# Run it by hand: CLAUDE_CODE_REMOTE=true .claude/hooks/session-start.sh

set -u

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
    exit 0
fi

root=$(cd "$(dirname "$0")/../.." && pwd) || exit 0
cd "$root" || exit 0

ready=
missing=
add_ready() { ready="${ready:+$ready, }$1"; }
add_missing() { missing="${missing:+$missing; }$1"; }

# A command with a time limit, so a stalled download cannot hold the session.
limit() { # limit SECONDS COMMAND...
    seconds=$1
    shift
    if command -v timeout >/dev/null 2>&1; then
        timeout "$seconds" "$@"
    else
        "$@"
    fi
}

# 1. rustfmt and clippy.
if cargo fmt --version >/dev/null 2>&1 && cargo clippy --version >/dev/null 2>&1; then
    add_ready "rustfmt and clippy"
elif command -v rustup >/dev/null 2>&1 && limit 180 rustup component add rustfmt clippy >&2; then
    add_ready "rustfmt and clippy (installed)"
else
    add_missing "rustfmt or clippy is not installed (rustup component add rustfmt clippy)"
fi

# 2. The crates, for this machine's target only: a build needs no others.
host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
if [ -n "$host" ]; then
    set -- --target "$host"
else
    set --
fi
if ! command -v cargo >/dev/null 2>&1; then
    add_missing "cargo is not installed"
elif cargo fetch --locked --offline "$@" >/dev/null 2>&1; then
    add_ready "crates"
elif CARGO_NET_RETRY=1 limit 240 cargo fetch --locked "$@" >&2; then
    add_ready "crates (fetched)"
else
    add_missing "cargo fetch failed, so the first build needs the network"
fi

# 3. Node modules. npm ci never rewrites package-lock.json; npm writes
# node_modules/.package-lock.json last, so it marks a finished install.
# One retry each, here and for cargo, so a blocked network fails in seconds.
for dir in website console; do
    [ -f "$dir/package-lock.json" ] || continue
    stamp=$dir/node_modules/.package-lock.json
    if [ -f "$stamp" ] && ! [ "$dir/package-lock.json" -nt "$stamp" ]; then
        add_ready "$dir/node_modules"
    elif ! command -v npm >/dev/null 2>&1; then
        add_missing "$dir/node_modules: npm is not installed"
    elif (cd "$dir" && limit 240 npm ci --prefer-offline --no-audit --no-fund \
        --fetch-retries=1 --fetch-retry-maxtimeout=10000 >&2); then
        add_ready "$dir/node_modules (installed)"
    else
        add_missing "$dir/node_modules: npm ci failed"
    fi
done

echo "Session setup (.claude/hooks/session-start.sh): ready: ${ready:-nothing}.${missing:+ Not ready: $missing.}"
exit 0
