#!/bin/sh
# Test install.sh against a local release: stand-in binaries, packed as
# .github/workflows/release.yml packs the real ones, served over HTTP from
# a temporary directory.
#
#   sh packaging/test-install.sh
#
# It checks that install.sh installs the four binaries into --prefix and
# that they run, that a second install replaces the first, that it says
# when the directory is not on PATH, and that it refuses an archive whose
# checksum does not match, an archive SHA256SUMS does not list, and an
# unknown argument, leaving the installed binaries as they were. It needs
# python3, for the HTTP server, and curl or wget.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/stretto-test-install.XXXXXX")
server=
cleanup() {
    if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; fi
    rm -rf "${work:?}"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

fail() {
    echo "test-install: FAILED: $*" >&2
    exit 1
}

# The target install.sh picks on this machine.
case $(uname -s)/$(uname -m) in
Linux/x86_64 | Linux/amd64) target=x86_64-unknown-linux-gnu ;;
Linux/aarch64 | Linux/arm64) target=aarch64-unknown-linux-gnu ;;
Darwin/arm64) target=aarch64-apple-darwin ;;
Darwin/x86_64)
    target=x86_64-apple-darwin
    if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
        target=aarch64-apple-darwin
    fi
    ;;
*)
    echo "test-install: no release target for $(uname -s)/$(uname -m); skipped"
    exit 0
    ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
    sums() { sha256sum "$@"; }
else
    sums() { shasum -a 256 "$@"; }
fi

# A release of VERSION: an archive of stand-ins that answer --version, and
# its SHA256SUMS.
release() {
    rm -rf "${work:?}/release" "${work:?}/pack"
    mkdir -p "$work/release" "$work/pack/stretto-$target"
    for bin in stretto stretto-proxy stretto-procedure stretto-mcp-demo; do
        printf '#!/bin/sh\necho "%s %s"\n' "$bin" "$1" >"$work/pack/stretto-$target/$bin"
        chmod 755 "$work/pack/stretto-$target/$bin"
    done
    cp "$root/LICENSE" "$root/README.md" "$work/pack/stretto-$target/"
    (cd "$work/pack" && tar -czf "$work/release/stretto-$target.tar.gz" "stretto-$target")
    (cd "$work/release" && sums "stretto-$target.tar.gz" >SHA256SUMS)
}

release 0.9.0
python3 -u -m http.server 0 --bind 127.0.0.1 --directory "$work/release" \
    >"$work/server.log" 2>&1 &
server=$!
port=
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    port=$(sed -n 's/.* port \([0-9][0-9]*\).*/\1/p' "$work/server.log" | head -n 1)
    [ -z "$port" ] || break
    sleep 0.25
done
[ -n "$port" ] || fail "the HTTP server did not start: $(cat "$work/server.log")"
base=http://127.0.0.1:$port
prefix=$work/prefix

install() {
    sh "$root/install.sh" --base-url "$base" --prefix "$prefix" "$@" >"$work/out" 2>&1
}

install || fail "install.sh: $(cat "$work/out")"
for bin in stretto stretto-proxy stretto-procedure stretto-mcp-demo; do
    [ "$("$prefix/bin/$bin" --version)" = "$bin 0.9.0" ] || fail "$bin is not installed"
done
grep -q "stretto 0.9.0 is installed in $prefix/bin" "$work/out" || fail "no summary: $(cat "$work/out")"
grep -q "is not on your PATH" "$work/out" || fail "no word about PATH: $(cat "$work/out")"
echo "test-install: installs the four binaries into --prefix, and says how to put them on PATH"

release 0.9.1
PATH="$prefix/bin:$PATH" install || fail "install.sh again: $(cat "$work/out")"
[ "$("$prefix/bin/stretto" --version)" = "stretto 0.9.1" ] || fail "the second install did not replace the first"
if grep -q "not on your PATH" "$work/out"; then fail "it asked to add a directory already on PATH"; fi
echo "test-install: a second install replaces the first"

cp "$work/release/SHA256SUMS" "$work/sums"
sed 's/^[0-9a-f]/0/' "$work/sums" >"$work/release/SHA256SUMS"
if install; then fail "it installed an archive whose checksum does not match"; fi
grep -q "does not match its checksum" "$work/out" || fail "wrong error: $(cat "$work/out")"
printf '%s  stretto-some-other-target.tar.gz\n' "$(cut -d ' ' -f 1 "$work/sums")" >"$work/release/SHA256SUMS"
if install; then fail "it installed an archive SHA256SUMS does not list"; fi
grep -q "SHA256SUMS has no checksum" "$work/out" || fail "wrong error: $(cat "$work/out")"
if install --frobnicate; then fail "it took an unknown argument"; fi
[ "$("$prefix/bin/stretto" --version)" = "stretto 0.9.1" ] || fail "a refused install changed the binaries"
echo "test-install: refuses a bad checksum, an unlisted archive and an unknown argument"
echo "test-install: ok"
