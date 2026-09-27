#!/bin/sh
# Install stretto from a GitHub release: stretto, stretto-proxy,
# stretto-procedure and stretto-mcp-demo, on Linux (x86_64 or aarch64, with
# glibc 2.35 or later) or macOS (Apple silicon or Intel).
#
#   curl -fsSL https://github.com/alexnodeland/stretto/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version v0.2.0 --prefix /usr/local
#
# It downloads the release's archive for this system and its SHA256SUMS,
# checks the archive's checksum, and copies the four binaries into
# PREFIX/bin. It changes nothing else: if that directory is not on PATH, it
# says how to add it. docs/install.md has the other ways to install.

set -eu

repo=alexnodeland/stretto
binaries="stretto stretto-proxy stretto-procedure stretto-mcp-demo"

usage() {
    cat <<'EOF'
usage: install.sh [--version VERSION] [--prefix DIR] [--base-url URL]

  --version VERSION  the release to install, such as v0.2.0 (default: the latest)
  --prefix DIR       install into DIR/bin (default: ~/.local, so ~/.local/bin)
  --base-url URL     fetch the release's files from URL instead of GitHub:
                     a mirror, or a directory served over HTTP for testing
EOF
}

die() {
    echo "install.sh: $*" >&2
    exit 1
}

version=latest
prefix=
base=
while [ $# -gt 0 ]; do
    case $1 in
    --version) version=${2:?--version needs a value, such as v0.2.0}; shift 2 ;;
    --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
    --base-url) base=${2:?--base-url needs a URL}; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
    esac
done
if [ -z "$prefix" ]; then
    prefix=${HOME:?HOME is not set; pass --prefix}/.local
fi

# The release's target for this system.
os=$(uname -s)
arch=$(uname -m)
case $arch in
x86_64 | amd64) arch=x86_64 ;;
arm64 | aarch64) arch=aarch64 ;;
*) die "there are no release binaries for $arch: build from source (docs/install.md)" ;;
esac
case $os in
Linux)
    if ldd --version 2>&1 | grep -qi musl; then
        die "the Linux binaries need glibc, and this system uses musl: use the container \
image or build from source (docs/install.md)"
    fi
    glibc=$(getconf GNU_LIBC_VERSION 2>/dev/null | sed -n 's/^glibc //p')
    if [ -n "$glibc" ]; then
        major=${glibc%%.*}
        minor=${glibc#*.}
        minor=${minor%%.*}
        if [ "$major" -lt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -lt 35 ]; }; then
            die "the Linux binaries need glibc 2.35 or later, and this system has $glibc: \
use the container image or build from source (docs/install.md)"
        fi
    fi
    target=$arch-unknown-linux-gnu
    ;;
Darwin)
    # A shell under Rosetta says x86_64 on Apple silicon: take the native build.
    if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
        arch=aarch64
    fi
    target=$arch-apple-darwin
    ;;
MINGW* | MSYS* | CYGWIN*) die "on Windows, use install.ps1 (docs/install.md)" ;;
*) die "there are no release binaries for $os: build from source (docs/install.md)" ;;
esac

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q -O "$2" "$1"; }
else
    die "needs curl or wget"
fi
if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
    sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
elif command -v openssl >/dev/null 2>&1; then
    sha256() { openssl dgst -sha256 -r "$1" | cut -d ' ' -f 1; }
else
    die "needs sha256sum, shasum or openssl to check the download"
fi

case $version in
latest) tag= ;;
v*) tag=$version ;;
*) tag=v$version ;;
esac
if [ -n "$base" ]; then
    url=${base%/}
elif [ -z "$tag" ]; then
    url=https://github.com/$repo/releases/latest/download
else
    url=https://github.com/$repo/releases/download/$tag
fi

archive=stretto-$target.tar.gz
tmp=$(mktemp -d "${TMPDIR:-/tmp}/stretto-install.XXXXXX")
trap 'rm -rf "${tmp:?}"' EXIT
trap 'exit 1' HUP INT TERM

echo "install.sh: downloading $url/$archive"
fetch "$url/$archive" "$tmp/$archive" || die "could not download $url/$archive"
fetch "$url/SHA256SUMS" "$tmp/SHA256SUMS" || die "could not download $url/SHA256SUMS"
expected=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1; exit }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "SHA256SUMS has no checksum for $archive"
actual=$(sha256 "$tmp/$archive")
[ "$actual" = "$expected" ] ||
    die "$archive does not match its checksum (expected $expected, got $actual): not installed"

tar -xzf "$tmp/$archive" -C "$tmp"
bindir=${prefix%/}/bin
mkdir -p "$bindir" || die "cannot create $bindir: pass --prefix"
for bin in $binaries; do
    [ -f "$tmp/stretto-$target/$bin" ] || die "$archive has no $bin"
done
# Each binary is copied beside its final name, then renamed over it, so a
# running copy is replaced cleanly.
for bin in $binaries; do
    cp "$tmp/stretto-$target/$bin" "$bindir/.$bin.new"
    chmod 755 "$bindir/.$bin.new"
    mv -f "$bindir/.$bin.new" "$bindir/$bin"
done
installed=$("$bindir/stretto" --version) || die "$bindir/stretto does not run on this system"

echo
echo "$installed is installed in $bindir: $(echo "$binaries" | sed 's/ /, /g')."
case :$PATH: in
*:"$bindir":*) ;;
*)
    echo
    echo "$bindir is not on your PATH. Add it for new shells, for example with this"
    echo "line in ~/.profile (or your shell's startup file):"
    echo
    echo "    export PATH=\"$bindir:\$PATH\""
    ;;
esac
cat <<'EOF'

Next:
    stretto doctor
        checks the installation
    stretto init --host claude-code --domain NAME -- <server command>
        runs an MCP server behind stretto-proxy, recording its sessions
        (also claude-desktop, cursor, vscode), and says what comes next
    stretto completions bash|zsh|fish|powershell|elvish
        prints shell completions

The quickstart runs the whole loop with no key:
https://github.com/alexnodeland/stretto/tree/main/examples/quickstart
EOF
