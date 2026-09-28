#!/usr/bin/env bash
# PostToolUse hook for Edit and Write: formats the Rust file Claude has just
# changed with rustfmt, which reads the repository's rustfmt.toml as
# `cargo fmt` does. It is quiet and never fails the tool call: a file it
# cannot format (one that does not parse yet, or no rustfmt installed) is
# left as it is, and `make check` still catches it.
#
# Input: the hook's JSON on stdin, whose tool_input.file_path is absolute.

input=$(cat) || exit 0

if command -v jq >/dev/null 2>&1; then
    file=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)
elif command -v python3 >/dev/null 2>&1; then
    file=$(printf '%s' "$input" | python3 -c \
        'import json, sys; print((json.load(sys.stdin).get("tool_input") or {}).get("file_path") or "")' \
        2>/dev/null)
else
    exit 0
fi

case $file in
*.rs) ;;
*) exit 0 ;;
esac
[ -f "$file" ] || exit 0

# From the file's directory, so rustup picks the toolchain its checkout pins.
cd "$(dirname "$file")" 2>/dev/null || exit 0
rustfmt --quiet "$file" >/dev/null 2>&1
exit 0
