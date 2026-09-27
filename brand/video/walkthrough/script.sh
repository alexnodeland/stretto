#!/usr/bin/env bash
# The walkthrough video's command list: docs/walkthrough.md, run for real.
#
#   bash brand/video/walkthrough/script.sh                 # run it in a new home directory
#   python3 brand/video/walkthrough/capture.py --bin DIR   # record it for the video
#
# Every command below runs: the stretto binaries, the official MCP filesystem
# server (through npx) and agent.py, a scripted agent standing in for an LLM
# host. No step needs a key. The working directory is also HOME, so ~/.stretto
# and ~/notes are inside it: capture.py sets that up, and so does a plain run.
#
# The format, which capture.py and render.mjs read:
#   step TITLE CAPTION   opens a step: a title card in the video, then a terminal
#   note CAPTION         the caption under the terminal from here on
#   run COMMAND          shows COMMAND at the prompt, runs it, records its output
# Setup outside a step is not shown. To add a step, such as `stretto init` or
# `stretto doctor`, add a `step` block where it belongs, with its `run` lines;
# nothing else needs to change. Then record and render again (brand/README.md).
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
STRETTO_BIN=${STRETTO_BIN:-}
[[ -n "$STRETTO_BIN" ]] && export PATH="$STRETTO_BIN:$PATH"
WORK=${STRETTO_WORK:-$(mktemp -d "${TMPDIR:-/tmp}/stretto-walkthrough.XXXXXX")}
cd "$WORK"
if [[ -z "${STRETTO_CAPTURE:-}" ]]; then
  # A plain run: a new, empty home, so doctor and ~/.stretto start fresh.
  # npx keeps its cache where it was.
  export npm_config_cache=${npm_config_cache:-$HOME/.npm}
  export HOME=$WORK
fi

# ---------------------------------------------------------------- helpers
_b64() { printf '%s' "$1" | base64 | tr -d '\n'; }
if [[ -n "${STRETTO_CAPTURE:-}" ]]; then
  _mark() { printf '\033]7777;%s;%s\007' "$1" "$(_b64 "$2")"; }
else
  _mark() { :; }
fi
step() {
  _mark step "$1"; _mark caption "$2"
  [[ -n "${STRETTO_CAPTURE:-}" ]] || printf '\n\033[1m== %s ==\033[0m\n%s\n' "$1" "$2"
}
note() {
  _mark caption "$1"
  [[ -n "${STRETTO_CAPTURE:-}" ]] || printf '\n# %s\n' "$1"
}
run() {
  local status=0
  _mark cmd "$1"
  [[ -n "${STRETTO_CAPTURE:-}" ]] || printf '\n\033[36m$\033[0m %s\n' "$1"
  eval "$1" || status=$?
  _mark exit "$status"
  if (( status != 0 )); then
    printf 'walkthrough: `%s` exited with %d\n' "$1" "$status" >&2
    exit "$status"
  fi
}

# ---------------------------------------------------------------- setup (not shown)
mkdir -p notes/projects notes/meetings notes/inbox context
printf '# Alpha\nStatus: shipped on 2026-09-02.\n' > notes/projects/alpha.md
printf "# Beta\nStatus: waiting on the vendor's API keys.\n" > notes/projects/beta.md
printf '# Gamma\nStatus: design review on 2026-10-09.\n' > notes/projects/gamma.md
printf '# Delta\nStatus: scoping; owner to be named.\n' > notes/projects/delta.md
printf '# 2026-08-18\nAgreed the Q4 budget.\n' > notes/meetings/2026-08-18.md
printf '# 2026-08-25\nAlpha launch checklist.\n' > notes/meetings/2026-08-25.md
printf '# 2026-09-01\nAlpha go/no-go: go.\n' > notes/meetings/2026-09-01.md
printf '# 2026-09-08\nBeta blocked on vendor keys.\n' > notes/meetings/2026-09-08.md
printf '# 2026-09-15\nHiring plan for Delta.\n' > notes/meetings/2026-09-15.md
printf '# 2026-10-06\nGamma design review moved to 10-09.\n' > notes/meetings/2026-10-06.md
printf '# 2026-10-13\nDelta owner: Priya.\n' > notes/meetings/2026-10-13.md
printf -- '- Renew the domain\n- Book the offsite\n' > notes/todo.md
printf 'Receipt 4471: 3 keyboards.\n' > notes/inbox/receipt.txt
cat > recorded.txt <<'EOF'
What's the status of project alpha?
Summarize the September meetings.
What's left on my todo list?
Compare projects beta and gamma.
What did we decide on September 8?
Read me all my project notes.
Summarize the August meetings.
What's in my inbox?
EOF
cat > new.txt <<'EOF'
What's the status of project gamma?
Summarize the August and September meetings.
What's in my inbox?
EOF
# `./agent` in the commands below is agent.py, the scripted agent.
ln -sf "$HERE/agent.py" agent
./agent --help > /dev/null

# ---------------------------------------------------------------- the steps
step "Check the install" \
  "stretto doctor checks the binaries on PATH and ~/.stretto, and says what a key would add. Nothing here needs one."
run 'stretto doctor'

step "Wrap the server" \
  "A folder of notes, served by the official MCP filesystem server. stretto init prints the host's configuration: for Claude Code, a claude mcp add command that puts stretto-proxy in the server's place."
run 'find notes -type f | sort'
note "Its next steps go to stderr; the rest of this walkthrough runs them. A scripted agent starts the proxy with the same arguments."
run 'stretto init --host claude-code --domain notes -- npx -y @modelcontextprotocol/server-filesystem ~/notes 2>/dev/null'

step "Record sessions" \
  "Use the agent as usual. Here a scripted agent stands in for the LLM: it searches the notes and reads what it finds, through the proxy, which records each session in ~/.stretto/logs/notes."
run './agent --quiet --from recorded.txt'
run 'ls ~/.stretto/logs/notes'

step "Learn a flow, with no key" \
  "--habit-only asks no model: stretto counts which reads followed which calls, and where their arguments came from."
run 'stretto learn --sessions ~/.stretto/logs/notes --domain notes --habit-only --out ~/.stretto/notes.flow.json'

step "Review it" \
  "A flow is a JSON file to review before serving it. flow-show renders it for a reviewer: what it may look up after each call, and where each argument comes from."
note "After a search, the flow reads a file: the agent did so after every search in training."
run "stretto flow-show ~/.stretto/notes.flow.json | awk '/^## Sites/{on=1} /^## Bindings/{on=0} on'"
note "The file's path comes from the search's own result: one line of it, or all of it when it found one file."
run "stretto flow-show ~/.stretto/notes.flow.json | awk '/^## Bindings/{on=1} /^## Sources/{on=0} on'"

step "Audit it on sessions it never saw" \
  "Record three more sessions without the flow, then score the agent's steps under it, site by site."
run './agent --quiet --logs ~/.stretto/logs/notes-new --from new.txt'
run 'stretto audit --flow ~/.stretto/notes.flow.json --sessions ~/.stretto/logs/notes-new | head -n 14'

step "Serve it" \
  "The same request, without the flow and with it. With it, the flow's reads ride in the search's result, and the agent skips the calls it would have made."
run './agent --logs ~/.stretto/logs/served "Summarize the October meetings."'
note "With the flow, served by the proxy behind the agent's calls: --flow ~/.stretto/notes.flow.json. It has no arbiter, so the proxy decides with reach, asking no model."
run './agent --logs ~/.stretto/logs/served --flow ~/.stretto/notes.flow.json "Summarize the October meetings."'
note "The flow log says why: each lookup's probability and its binding's chance, then why it handed back."
run "jq -c 'select(.action) | {action, site, prob: (.prob * 100 | round / 100), binding, tool, reason} | del(..|nulls)' ~/.stretto/logs/served/*.flow.jsonl"

step "Learn again, and review what changed" \
  "The audit's sessions are new training data. Learn from all eleven, and diff the new flow against the one being served."
run 'cp ~/.stretto/logs/notes-new/*.jsonl ~/.stretto/logs/notes/'
run 'stretto learn --sessions ~/.stretto/logs/notes --domain notes --habit-only --out ~/.stretto/notes-2.flow.json'
run 'stretto flow-diff ~/.stretto/notes.flow.json ~/.stretto/notes-2.flow.json'
