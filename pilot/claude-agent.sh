#!/usr/bin/env bash
# Run Claude Code with a Claude model, on the endpoint and credentials of the
# environment it starts in: ANTHROPIC_BASE_URL, ANTHROPIC_API_KEY and
# CLAUDE_CODE_OAUTH_TOKEN pass through when set, with proxy and certificate
# settings, and nothing else, so no other key reaches the agent. Inside a
# Claude Code session that manages its own credentials (a cloud session), the
# endpoint alone is enough.
#
# Unlike glm-claude.sh it runs Claude Code in its normal mode: --bare takes
# only ANTHROPIC_API_KEY, so it is dropped from the arguments. The normal mode
# adds a short note to the model's context (the working directory, the
# model's name, the date), where --bare adds the date alone. It starts in an
# empty directory, so no CLAUDE.md is read. CLAUDE_AGENT_CONFIG_DIR keeps its
# state apart (default ~/.claude-agent).
set -euo pipefail
config="${CLAUDE_AGENT_CONFIG_DIR:-$HOME/.claude-agent}"
mkdir -p "$config/work"
args=()
for a in "$@"; do
  [ "$a" = --bare ] || args+=("$a")
done
cd "$config/work"
exec env -i \
  PATH="$PATH" HOME="$HOME" TERM=dumb \
  ${HTTPS_PROXY:+HTTPS_PROXY="$HTTPS_PROXY"} ${https_proxy:+https_proxy="$https_proxy"} \
  ${NO_PROXY:+NO_PROXY="$NO_PROXY"} ${no_proxy:+no_proxy="$no_proxy"} \
  ${NODE_EXTRA_CA_CERTS:+NODE_EXTRA_CA_CERTS="$NODE_EXTRA_CA_CERTS"} \
  ${SSL_CERT_FILE:+SSL_CERT_FILE="$SSL_CERT_FILE"} \
  ${ANTHROPIC_BASE_URL:+ANTHROPIC_BASE_URL="$ANTHROPIC_BASE_URL"} \
  ${ANTHROPIC_API_KEY:+ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY"} \
  ${CLAUDE_CODE_OAUTH_TOKEN:+CLAUDE_CODE_OAUTH_TOKEN="$CLAUDE_CODE_OAUTH_TOKEN"} \
  API_TIMEOUT_MS=3000000 \
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
  ENABLE_TOOL_SEARCH=false \
  CLAUDE_CONFIG_DIR="$config" \
  claude "${args[@]}"
