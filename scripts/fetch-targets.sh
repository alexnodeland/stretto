#!/usr/bin/env bash
# Fetch published τ²-bench trajectories for agent models that Phase 0 uses as
# transfer targets: models the habit never trains on. Sierra hosts leaderboard
# submissions in a public bucket (see web/leaderboard in the τ²-bench repo).
#
# Usage: scripts/fetch-targets.sh [dest]   (default: .data/tau2-targets)
# Prints the downloaded paths, one per line.
set -euo pipefail

dest="${1:-.data/tau2-targets}"
base="https://sierra-tau-bench-public.s3.us-west-2.amazonaws.com/submissions"
mkdir -p "$dest"

fetch() { # <submission dir> <file>
  if [ ! -s "$dest/$2" ]; then
    curl -fsSL -o "$dest/$2.part" "$base/$1/trajectories/$2"
    mv "$dest/$2.part" "$dest/$2"
  fi
  echo "$dest/$2"
}

# GLM-5 (Z.ai), thinking enabled; GPT-5.2 simulates the user.
fetch glm-5-think_sierra_2026-03-02 glm-5_enabled_retail_gpt-5.2_4trials.json
fetch glm-5-think_sierra_2026-03-02 glm-5_enabled_airline_gpt-5.2_4trials.json
