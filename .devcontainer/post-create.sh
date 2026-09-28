#!/usr/bin/env bash
# Runs once when the dev container is created: the Rust components and tools
# the Makefile's targets use, the crates, and the documentation site's node
# modules. `make help` lists what to run next.
set -euo pipefail
cd "$(dirname "$0")/.."

make install-dev-tools
cargo fetch --locked
(cd website && npm ci --no-audit --no-fund)

echo "stretto dev container ready: make help lists the targets."
