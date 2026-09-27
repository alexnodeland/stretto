#!/bin/sh
# Fill in the Homebrew formula template (stretto.rb, next to this script)
# with a release's version and its archives' checksums, and print it.
#
#   packaging/homebrew/fill.sh VERSION SHA256SUMS > stretto.rb
#
# VERSION has no `v` (0.2.0). SHA256SUMS is the release's, as sha256sum
# writes it. The release workflow runs this and attaches the result.

set -eu

version=${1:?usage: fill.sh VERSION SHA256SUMS}
sums=${2:?usage: fill.sh VERSION SHA256SUMS}
version=${version#v}

sum() {
    s=$(awk -v f="stretto-$1.tar.gz" '$2 == f || $2 == "*" f { print $1; exit }' "$sums")
    case $s in
    [0-9a-f]*) printf '%s' "$s" ;;
    *) echo "fill.sh: $sums has no checksum for stretto-$1.tar.gz" >&2; return 1 ;;
    esac
}

mac_arm=$(sum aarch64-apple-darwin)
mac_intel=$(sum x86_64-apple-darwin)
linux_arm=$(sum aarch64-unknown-linux-gnu)
linux_intel=$(sum x86_64-unknown-linux-gnu)

formula=$(sed -e "s/@VERSION@/$version/" \
    -e "s/@SHA256_AARCH64_APPLE_DARWIN@/$mac_arm/" \
    -e "s/@SHA256_X86_64_APPLE_DARWIN@/$mac_intel/" \
    -e "s/@SHA256_AARCH64_UNKNOWN_LINUX_GNU@/$linux_arm/" \
    -e "s/@SHA256_X86_64_UNKNOWN_LINUX_GNU@/$linux_intel/" \
    "$(dirname "$0")/stretto.rb")
if printf '%s\n' "$formula" | grep -q '@[A-Z0-9_]*@'; then
    echo "fill.sh: the template has a placeholder this script does not fill" >&2
    exit 1
fi
printf '%s\n' "$formula"
