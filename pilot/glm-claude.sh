#!/usr/bin/env bash
# Run Claude Code against Z.ai's GLM Coding Plan endpoint (Z.ai's documented
# Claude Code setup), with a clean environment: only proxy and certificate
# settings pass through, so a Claude Code session running this script is not
# affected. Needs ZAI_API_KEY; GLM_CLAUDE_CONFIG_DIR keeps its state apart.
# Inside a session this script started (an MCP server, the simulated
# customer), the key and config directory arrive as the variables it set.
set -euo pipefail
key="${ZAI_API_KEY:-${ANTHROPIC_AUTH_TOKEN:-}}"
: "${key:?set ZAI_API_KEY}"
config="${GLM_CLAUDE_CONFIG_DIR:-${CLAUDE_CONFIG_DIR:-$HOME/.glm-claude}}"
mkdir -p "$config"
exec env -i \
  PATH="$PATH" HOME="$HOME" TERM=dumb \
  ${HTTPS_PROXY:+HTTPS_PROXY="$HTTPS_PROXY"} ${https_proxy:+https_proxy="$https_proxy"} \
  ${NO_PROXY:+NO_PROXY="$NO_PROXY"} ${no_proxy:+no_proxy="$no_proxy"} \
  ${NODE_EXTRA_CA_CERTS:+NODE_EXTRA_CA_CERTS="$NODE_EXTRA_CA_CERTS"} \
  ${SSL_CERT_FILE:+SSL_CERT_FILE="$SSL_CERT_FILE"} \
  ANTHROPIC_BASE_URL="https://api.z.ai/api/anthropic" \
  ANTHROPIC_AUTH_TOKEN="$key" \
  ANTHROPIC_DEFAULT_OPUS_MODEL="glm-5.3" \
  ANTHROPIC_DEFAULT_SONNET_MODEL="glm-5.3" \
  ANTHROPIC_DEFAULT_HAIKU_MODEL="glm-5.3-flash" \
  API_TIMEOUT_MS=3000000 \
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
  ENABLE_TOOL_SEARCH=false \
  CLAUDE_CONFIG_DIR="$config" \
  claude "$@"
