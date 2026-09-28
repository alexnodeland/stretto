#!/bin/sh
# The real stretto-console on a copy of its test fixtures, for npm run e2e:real
# (playwright.real.config.ts starts it, with `vite preview` in front of it
# serving the built UI and passing /api on).
#
# It needs stretto-console, stretto (the CLI the jobs run), stretto-proxy
# and stretto-mcp-demo, built with
#
#   cargo build -p stretto-console -p stretto-report -p stretto-proxy
#
# and found in $STRETTO_BIN (default: $CARGO_TARGET_DIR/debug, else the
# workspace's target/debug). They are linked into test-results/real/bin, so
# a later build does not swap them under the tests, and
# crates/stretto-console/tests/fixtures/home is copied to
# test-results/real/home, since jobs and deletes write into it.
#
#   E2E_API_PORT   the console's port (default 7891)

set -eu

here=$(cd "$(dirname "$0")" && pwd)
console=$(cd "$here/../.." && pwd)
root=$(cd "$console/.." && pwd)
port=${E2E_API_PORT:-7891}
bin=${STRETTO_BIN:-${CARGO_TARGET_DIR:-$root/target}/debug}
work=$console/test-results/real

for b in stretto-console stretto stretto-proxy stretto-mcp-demo; do
    [ -x "$bin/$b" ] || {
        echo "serve.sh: $bin/$b is missing: cargo build -p stretto-console -p stretto-report -p stretto-proxy" >&2
        exit 1
    }
done

rm -rf "${work:?}"
mkdir -p "$work/bin"
for b in stretto-console stretto stretto-proxy stretto-mcp-demo; do
    ln "$bin/$b" "$work/bin/$b" 2>/dev/null || cp "$bin/$b" "$work/bin/$b"
done
cp -R "$root/crates/stretto-console/tests/fixtures/home" "$work/home"

# The fixtures name the demo server by its bare name, as if it were on PATH.
PATH=$work/bin:$PATH
export PATH
exec "$work/bin/stretto-console" --no-auth --listen "127.0.0.1:$port" --data "$work/home" \
    --stretto "$work/bin/stretto"
