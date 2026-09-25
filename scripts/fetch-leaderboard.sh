#!/usr/bin/env bash
# Fetch published τ²-bench trajectories from Sierra's leaderboard, for Phase 0
# to train on (--source) or to test transfer to (--target). Sierra hosts
# leaderboard submissions in a public bucket (see web/leaderboard in the
# τ²-bench repo).
#
# Usage: scripts/fetch-leaderboard.sh [-d dest] [-t] [model ...]
#   dest defaults to .data/tau2-targets; models default to glm-5.
#   -t also fetches each model's telecom run, named as its retail run is.
#   Models: glm-5 qwen3.5 qwen3-max gpt-5.2 gpt-5.2-none claude-opus-4.5
#           claude-sonnet-4.5 gemini-3-pro gemini-3-flash, or "all".
# Prints one label=path pair per file (retail, airline, then telecom), ready to pass
# to `stretto phase0 --source` or `--target`.
set -euo pipefail

dest=".data/tau2-targets"
if [ "${1:-}" = "-d" ]; then
  dest="$2"
  shift 2
fi
telecom=""
if [ "${1:-}" = "-t" ]; then
  telecom=1
  shift
fi
models=("$@")
if [ ${#models[@]} -eq 0 ]; then
  models=(glm-5)
fi
if [ "${models[0]}" = "all" ]; then
  models=(glm-5 qwen3.5 qwen3-max gpt-5.2 gpt-5.2-none claude-opus-4.5 claude-sonnet-4.5 gemini-3-pro gemini-3-flash)
fi

base="https://sierra-tau-bench-public.s3.us-west-2.amazonaws.com/submissions"
mkdir -p "$dest"

fetch() { # <label> <submission dir> <file>
  if [ ! -s "$dest/$3" ]; then
    curl -fsSL -o "$dest/$3.part" "$base/$2/trajectories/$3"
    mv "$dest/$3.part" "$dest/$3"
  fi
  echo "$1=$dest/$3"
}

for model in "${models[@]}"; do
  case "$model" in
    # GPT-5.2 simulates the user unless noted.
    glm-5) # Z.ai GLM-5, thinking enabled
      dir=glm-5-think_sierra_2026-03-02
      files=(glm-5_enabled_retail_gpt-5.2_4trials.json glm-5_enabled_airline_gpt-5.2_4trials.json) ;;
    qwen3.5) # Qwen3.5-397B-A17B, thinking enabled
      dir=qwen3.5-397b-a17b-think_sierra_2026-03-02
      files=(qwen3.5-397b-a17b_enabled_retail_gpt-5.2_4trials.json qwen3.5-397b-a17b_enabled_airline_gpt-5.2_4trials.json) ;;
    qwen3-max) # Qwen3-Max-Thinking-Preview; GPT-4.1 simulates the user
      dir=qwen3-max_qwen_2025-10-30
      files=(retail_llm_agent_qwen3-max-2025-10-30_user_simulator_gpt-4.1-2025-04-14.json airline_llm_agent_qwen3-max-2025-10-30_user_simulator_gpt-4.1-2025-04-14.json) ;;
    gpt-5.2) # reasoning effort high
      dir=gpt-5-2_sierra_2026-02-26
      files=(gpt-5.2_high_retail_gpt-5.2_4trials.json gpt-5.2_high_airline_gpt-5.2_4trials.json) ;;
    gpt-5.2-none) # reasoning off
      dir=gpt-5-2-none_sierra_2026-02-26
      files=(gpt-5.2_none_retail_gpt-5.2_4trials.json gpt-5.2_none_airline_gpt-5.2_4trials.json) ;;
    claude-opus-4.5) # effort high
      dir=claude-opus-4-5_sierra_2026-02-26
      files=(claude-opus-4-5_high_retail_gpt-5.2_4trials.json claude-opus-4-5_high_airline_gpt-5.2_4trials.json) ;;
    claude-sonnet-4.5) # thinking enabled
      dir=claude-sonnet-4-5_sierra_2026-02-26
      files=(claude-sonnet-4-5_enabled_retail_gpt-5.2_4trials.json claude-sonnet-4-5_enabled_airline_gpt-5.2_4trials.json) ;;
    gemini-3-pro)
      dir=gemini-3-pro_sierra_2026-03-02
      files=(geminipro-retail.json geminipro-airline.json) ;;
    gemini-3-flash)
      dir=gemini-3-flash_sierra_2026-03-02
      files=(geminiflash-retail.json geminiflash-airline.json) ;;
    *)
      echo "fetch-leaderboard: unknown model '$model'" >&2
      exit 2 ;;
  esac
  if [ -n "$telecom" ]; then
    files+=("${files[0]/retail/telecom}")
  fi
  for f in "${files[@]}"; do
    fetch "$model" "$dir" "$f"
  done
done
