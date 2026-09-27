#!/usr/bin/env bash
# The flows and replays behind docs/results/benchmarks-2026-09-27.md, from the runs the converters wrote to $WORK
# (scripts/bench/README.md says which converter writes which folder).
#
#   scripts/bench/replay.sh SET...
#
#   tau2      the paper batch, trace-only: nine agents in three domains and two solo agents, habit and reach at 0.3,
#             with the published pool flows (docs/results/reach-2026-09-26-*.flow.json)
#   tau2-env  the same replays against τ²-bench's environment; they also name the turns they saved (priced.py)
#   sweep     habit and reach at 0.1 to 0.8, two agents a domain, against the environment
#   perdec    reach at 0.1 against the environment, decisions logged (per_decision.py)
#   v1        τ-bench's four published runs, trace-only, with the τ²-bench pool flows
#   bfcl dojo wb mcpm dtap
#             flows learned from older agents' training runs, replayed trace-only on newer agents' test runs;
#             DTap-Bench's from each agent's own runs, its harness's other agents, the other harnesses and all
#   mcpm-own  MCPMark's newer models, each with a flow from its own training runs
#   ceilings  each set's read-only ceiling (ceiling.py)
#
# Environment: WORK (default work), TAU2 (τ²-bench's checkout, default ../tau2-bench), PY (a Python with τ²-bench
# installed, default python3), S (the stretto binary, default target/release/stretto), JOBS (episodes at once, 3).
# A replay writes $WORK/replays/SET/c-<domain>-<agent>[-<source>]-<decider>[-<θ>]/check.json and removes its
# episodes' folders. One whose check.json exists is skipped, so a batch picks up where it stopped.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$PWD
mkdir -p "${WORK:=work}"
WORK=$(cd "$WORK" && pwd)
TAU2=$(cd "${TAU2:-../tau2-bench}" && pwd)
PY=${PY:-python3}
S=${S:-$ROOT/target/release/stretto}
JOBS=${JOBS:-3}
FLOWS=$WORK/flows
POOLS=$ROOT/docs/results
mkdir -p "$FLOWS" "$WORK/cache" "$WORK/ceilings"

files() { # DIR NAME...: DIR/NAME.json for each name
  local dir=$1
  shift
  for n in "$@"; do echo "$dir/$n.json"; done
}

learn() { # OUT DOMAIN CHECKOUT RESULTS...: a habit-only flow from the runs' training tasks
  local out=$1 domain=$2 checkout=$3 args=()
  shift 3
  [ -f "$out" ] && return
  mkdir -p "$(dirname "$out")"
  for r in "$@"; do args+=(--results "$r"); done
  "$S" learn "${args[@]}" --tau2 "$checkout" --domain "$domain" --habit-only --out "$out" 2>&1 | tail -1
}

replay() { # OUT DOMAIN RESULTS FLOW DECIDER THRESHOLD CHECKOUT TRIALS [OPTION...]
  local out=$1 domain=$2 results=$3 flow=$4 decider=$5 threshold=$6 checkout=$7 trials=$8
  shift 8
  [ -f "$out/check.json" ] && return
  mkdir -p "$(dirname "$out")"
  # shellcheck disable=SC2086  # the trials are a list
  (cd "$ROOT/pilot" && timeout 2400 "$PY" check_flow.py --domain "$domain" --trials $trials --results "$results" \
    --flow "$flow" --flow-decider "$decider" --flow-threshold "$threshold" --flow-oracle replay \
    --oracle-cache "$WORK/cache" --explore 0 --jobs "$JOBS" --tau2 "$checkout" --out "$out" "$@") > "$out.log" 2>&1
  if [ -f "$out/check.json" ]; then
    "$PY" -c 'import json, sys; t = json.load(open(sys.argv[1]))["total"]; print(sys.argv[2], "saved", t["turns_saved"], "of", t["turns"], "turns,", t["detours"], "detours")' \
      "$out/check.json" "$(basename "$out")"
  else
    echo "$(basename "$out"): failed, see $out.log"
  fi
  rm -rf "${out:?}"/task-*
}

leaderboard() { # the paper batch's runs from τ²-bench's leaderboard: retail and airline, then telecom
  { scripts/fetch-leaderboard.sh all; scripts/fetch-leaderboard.sh -t all | grep -i telecom; } | cut -d= -f2 | sed "s|^|$ROOT/|"
}

domain_of() {
  case $1 in *airline*) echo airline ;; *retail*) echo retail ;; *) echo telecom ;; esac
}

paper_batch() { # SET OPTION
  local set=$1 mode=$2 d
  for dec in reach habit; do
    for f in $(leaderboard); do
      d=$(domain_of "$f")
      replay "$WORK/replays/$set/c-$d-$(basename "$f" .json)-$dec" "$d" "$f" "$POOLS/reach-2026-09-26-$d.flow.json" \
        "$dec" 0.3 "$TAU2" "0 1 2 3" "$mode"
    done
    for m in gpt-4.1-2025-04-14 o4-mini-2025-04-16; do
      replay "$WORK/replays/$set/c-solo-$m-$dec" telecom \
        "$TAU2/data/tau2/results/final/${m}_telecom_no-user_gpt-4.1-2025-04-14_4trials.json" \
        "$POOLS/reach-2026-09-26-solo.flow.json" "$dec" 0.3 "$TAU2" "0 1 2 3" "$mode" --solo
    done
  done
}

sweep() {
  local d
  for f in glm-5_enabled_retail claude-sonnet-4-5_enabled_retail glm-5_enabled_airline claude-sonnet-4-5_enabled_airline \
    claude-sonnet-4-5_enabled_telecom gpt-5.2_none_telecom; do
    d=$(domain_of "$f")
    for dec in habit reach; do
      for th in 0.1 0.15 0.2 0.3 0.4 0.5 0.6 0.7 0.8; do
        replay "$WORK/replays/sweep/sw-$d-${f}_gpt-5.2_4trials-$dec-$th" "$d" "$ROOT/.data/tau2-targets/${f}_gpt-5.2_4trials.json" \
          "$POOLS/reach-2026-09-26-$d.flow.json" "$dec" "$th" "$TAU2" "0 1 2 3" --in-process
      done
    done
  done
}

perdec() { # the eight agents whose episodes count their tokens
  local d
  for f in $(leaderboard); do
    case $f in *llm_agent*) continue ;; esac
    d=$(domain_of "$f")
    replay "$WORK/replays/perdec/c-$d-$(basename "$f" .json)" "$d" "$f" "$POOLS/reach-2026-09-26-$d.flow.json" \
      reach 0.1 "$TAU2" "0 1 2 3" --in-process
  done
}

v1() {
  local name d
  for spec in gpt-4o-retail:retail gpt-4o-airline:airline sonnet-35-new-retail:retail sonnet-35-new-airline:airline; do
    name=${spec%%:*} d=${spec##*:}
    for dec in reach habit; do
      replay "$WORK/replays/v1/c-$d-$name-$dec" "$d" "$WORK/v1/$name.json" "$POOLS/reach-2026-09-26-$d.flow.json" \
        "$dec" 0.3 "$TAU2" "0 1 2 3" --trace
    done
  done
}

bfcl() {
  local b=$WORK/bfcl
  local pool=(gpt-4.1-2025-04-14-FC gpt-4.1-mini-2025-04-14-FC o4-mini-2025-04-16-FC o3-2025-04-16-FC mistral-large-2411-FC
    meta-llama_Llama-3.3-70B-Instruct-FC qwen3-235b-a22b-instruct-2507-FC gemini-2.5-flash-FC)
  local targets=(claude-opus-4-5-20251101-FC claude-sonnet-4-5-20250929-FC claude-haiku-4-5-20251001-FC
    gpt-5.2-2025-12-11-FC gpt-5-mini-2025-08-07-FC gemini-3-pro-preview-FC grok-4-1-fast-reasoning-FC
    kimi-k2-0905-preview-FC glm-4.6-FC DeepSeek-V3.2-Exp-FC)
  # shellcheck disable=SC2046
  learn "$FLOWS/bfcl-pool.flow.json" bfcl "$b/checkout" $(files "$b/runs" "${pool[@]}")
  learn "$FLOWS/bfcl-ground-truth.flow.json" bfcl "$b/checkout" "$b/bfcl_ground_truth.json"
  for th in 0.3 0.1 0.2 0.5; do
    for dec in reach habit; do
      replay "$WORK/replays/bfcl-ground-truth/c-bfcl-$dec-$th" bfcl "$b/bfcl_ground_truth.json" \
        "$FLOWS/bfcl-ground-truth.flow.json" "$dec" "$th" "$b/checkout" 0 --trace
      for t in "${targets[@]}"; do
        replay "$WORK/replays/bfcl/c-bfcl-$t-$dec-$th" bfcl "$b/runs/$t.json" "$FLOWS/bfcl-pool.flow.json" \
          "$dec" "$th" "$b/checkout" 0 --trace
      done
    done
  done
}

dojo() {
  local x=$WORK/dojo
  local pool=(gpt-3.5-turbo-0125 gpt-4-0125-preview gpt-4-turbo-2024-04-09 claude-3-opus-20240229 claude-3-sonnet-20240229
    claude-3-haiku-20240307 command-r command-r-plus meta-llama_Llama-3-70b-chat-hf gemini-1.5-pro-001 gemini-1.5-flash-001)
  local targets=(gpt-4o-2024-05-13 gpt-4o-mini-2024-07-18 claude-3-5-sonnet-20240620 claude-3-5-sonnet-20241022
    claude-3-7-sonnet-20250219 gemini-1.5-pro-002 gemini-1.5-flash-002 gemini-2.0-flash-exp gemini-2.0-flash-001
    meta-llama_Llama-3.3-70B-Instruct)
  for s in travel slack banking workspace; do
    # shellcheck disable=SC2046
    learn "$FLOWS/dojo-$s.flow.json" "$s" "$x/checkout" $(files "$x/$s" "${pool[@]}")
  done
  # The list ablation: travel's flow without its list bindings.
  "$PY" -c 'import json, sys; f = json.load(open(sys.argv[1])); f["bindings"].pop("lists", None); json.dump(f, open(sys.argv[2], "w"))' \
    "$FLOWS/dojo-travel.flow.json" "$FLOWS/dojo-travel-nolists.flow.json"
  for th in 0.3 0.1; do
    for s in travel slack banking workspace; do
      for t in "${targets[@]}"; do
        for dec in reach habit; do
          replay "$WORK/replays/dojo/c-$s-$t-$dec-$th" "$s" "$x/$s/$t.json" "$FLOWS/dojo-$s.flow.json" "$dec" "$th" \
            "$x/checkout" 0 --trace
        done
      done
    done
  done
  for t in "${targets[@]}"; do
    for dec in reach habit; do
      replay "$WORK/replays/dojo-nolists/c-travel-$t-$dec-0.3" travel "$x/travel/$t.json" \
        "$FLOWS/dojo-travel-nolists.flow.json" "$dec" 0.3 "$x/checkout" 0 --trace
    done
  done
}

wb() {
  local x=$WORK/wb
  local pool=(gpt-3.5 gpt-4-turbo gpt-4o gpt-4.1 o3 gpt-5 gpt-5.1 gpt-5.2 glm-4.6 claude-haiku-4.5)
  local targets=(claude-fable-5 claude-opus-4.8 claude-sonnet-4.6 gemini-3.1-pro gemini-3.5-flash gpt-5.4 gpt-5.4-mini
    gpt-5.4-nano gpt-5.5 kimi-k2.6 deepseek-v4-pro qwen-3.5-flash mistral-medium-3-5 mistral-small-2603)
  local domains=(multi_domain customer_relationship_manager project_management email calendar analytics)
  for d in "${domains[@]}"; do
    # shellcheck disable=SC2046
    learn "$FLOWS/wb-$d.flow.json" "$d" "$x/checkout" $(files "$x/$d" "${pool[@]}")
  done
  for th in 0.3 0.1; do
    for d in "${domains[@]}"; do
      for t in "${targets[@]}"; do
        for dec in reach habit; do
          replay "$WORK/replays/wb/c-$d-$t-$dec-$th" "$d" "$x/$d/$t.json" "$FLOWS/wb-$d.flow.json" "$dec" "$th" \
            "$x/checkout" 0 --trace
        done
      done
    done
  done
}

MCPM_TARGETS=(claude-sonnet-4 claude-opus-4-1 gpt-5-low gpt-5-mini-low o3 gemini-2-5-pro grok-4 kimi-k2-0905 qwen-3-max)

mcpm() {
  local x=$WORK/mcpm d
  local pool=(gpt-4-1 gpt-4-1-mini o4-mini gemini-2-5-flash deepseek-chat glm-4-5 kimi-k2-0711 qwen-3-coder-plus)
  local targets=("${MCPM_TARGETS[@]}")
  for svc in filesystem postgres github notion; do
    # shellcheck disable=SC2046
    learn "$FLOWS/mcpmark_$svc.flow.json" "mcpmark_$svc" "$x/checkout" $(files "$x/mcpmark_$svc" "${pool[@]}")
  done
  for th in 0.3 0.1; do
    for svc in filesystem postgres github notion; do
      d=mcpmark_$svc
      for t in "${targets[@]}"; do
        for dec in reach habit; do
          replay "$WORK/replays/mcpm/c-$d-$t-$dec-$th" "$d" "$x/$d/$t.json" "$FLOWS/$d.flow.json" "$dec" "$th" \
            "$x/checkout" 0 --trace
        done
      done
    done
  done
}

mcpm_own() { # each newer model's flow from its own training runs, replayed on its test runs
  local x=$WORK/mcpm d f
  for svc in filesystem postgres github notion; do
    d=mcpmark_$svc
    for t in "${MCPM_TARGETS[@]}"; do
      f=$FLOWS/mcpm-own/$d-$t.flow.json
      learn "$f" "$d" "$x/checkout" "$x/$d/$t.json"
      for dec in reach habit; do
        [ -f "$f" ] && replay "$WORK/replays/mcpm-own/c-$d-$t-$dec-0.3" "$d" "$x/$d/$t.json" "$f" "$dec" 0.3 "$x/checkout" 0 --trace
      done
    done
  done
}

dtap() { # per domain: flows from the agent's own runs, its harness's other agents, the other harnesses, all
  local x=$WORK/dtap d h f agents same other
  for d in customer_service dtap_crm dtap_telecom dtap_travel; do
    mkdir -p "$FLOWS/dtap/$d"
    agents=$(cd "$x/$d" && ls -- *.json | sed 's/\.json$//')
    # shellcheck disable=SC2046,SC2086
    learn "$FLOWS/dtap/$d/all.flow.json" "$d" "$x/checkout" $(files "$x/$d" $agents)
    for a in $agents; do
      h=${a%%-*} same=() other=()
      for b in $agents; do
        [ "$b" = "$a" ] && continue
        if [ "${b%%-*}" = "$h" ]; then same+=("$b"); else other+=("$b"); fi
      done
      learn "$FLOWS/dtap/$d/own-$a.flow.json" "$d" "$x/checkout" "$x/$d/$a.json"
      # shellcheck disable=SC2046
      [ ${#same[@]} -gt 0 ] && learn "$FLOWS/dtap/$d/same-$a.flow.json" "$d" "$x/checkout" $(files "$x/$d" "${same[@]}")
      # shellcheck disable=SC2046
      learn "$FLOWS/dtap/$d/other-$h.flow.json" "$d" "$x/checkout" $(files "$x/$d" "${other[@]}")
    done
    for dec in reach habit; do
      for a in $agents; do
        h=${a%%-*}
        for src in own same other all; do
          case $src in
            own | same) f=$FLOWS/dtap/$d/$src-$a.flow.json ;;
            other) f=$FLOWS/dtap/$d/other-$h.flow.json ;;
            all) f=$FLOWS/dtap/$d/all.flow.json ;;
          esac
          # A flow that failed to learn (no successful training runs) has no file; its replay is left out.
          [ -f "$f" ] && replay "$WORK/replays/dtap/c-$d-$a-$src-$dec-0.3" "$d" "$x/$d/$a.json" "$f" "$dec" 0.3 \
            "$x/checkout" 0 --trace
        done
      done
    done
  done
}

ceilings() { # the replayed agents' test runs, per set
  local c=$WORK/ceilings x
  # shellcheck disable=SC2046
  [ -f "$c/tau2.json" ] || "$PY" scripts/ceiling.py $(leaderboard) --tau2 "$TAU2" --json "$c/tau2.json" > /dev/null
  # shellcheck disable=SC2046
  [ -f "$c/v1.json" ] || "$PY" scripts/ceiling.py $(files "$WORK/v1" gpt-4o-retail sonnet-35-new-retail gpt-4o-airline sonnet-35-new-airline) \
    --tau2 "$TAU2" --json "$c/v1.json" > /dev/null
  x=$WORK/bfcl
  [ -f "$c/bfcl-ground-truth.json" ] || "$PY" scripts/ceiling.py "$x/bfcl_ground_truth.json" --tau2 "$x/checkout" \
    --json "$c/bfcl-ground-truth.json" > /dev/null
  # shellcheck disable=SC2046
  [ -f "$c/bfcl.json" ] || "$PY" scripts/ceiling.py $(ls "$WORK"/replays/bfcl | sed -n 's/^c-bfcl-\(.*\)-reach-0\.3$/\1/p' | \
    sed "s|.*|$x/runs/&.json|") --tau2 "$x/checkout" --json "$c/bfcl.json" > /dev/null
  for spec in dojo:travel dojo:slack dojo:banking dojo:workspace wb:multi_domain wb:customer_relationship_manager \
    wb:project_management wb:email wb:calendar wb:analytics mcpm:mcpmark_filesystem mcpm:mcpmark_postgres \
    mcpm:mcpmark_github mcpm:mcpmark_notion dtap:customer_service dtap:dtap_crm dtap:dtap_telecom dtap:dtap_travel; do
    local set=${spec%%:*} d=${spec##*:}
    [ -f "$c/$set-$d.json" ] && continue
    # The agents replayed on the set's test runs.
    # shellcheck disable=SC2046
    "$PY" scripts/ceiling.py $(ls "$WORK/replays/$set" | grep -v '\.log$' | sed -n "s/^c-$d-\(.*\)-reach-0\.3$/\1/p" | \
      sed -E 's/-(own|same|other|all)$//' | sort -u | sed "s|.*|$WORK/$set/$d/&.json|") --tau2 "$WORK/$set/checkout" \
      --json "$c/$set-$d.json" > /dev/null
  done
}

run_set() {
  case $1 in
    tau2) paper_batch tau2 --trace ;;
    tau2-env) paper_batch tau2-env --in-process ;;
    sweep | perdec | v1 | bfcl | dojo | wb | mcpm | dtap | ceilings) "$1" ;;
    mcpm-own) mcpm_own ;;
    all) for s in tau2 tau2-env sweep perdec v1 bfcl dojo wb mcpm mcpm-own dtap ceilings; do run_set "$s"; done ;;
    *) echo "unknown set: $1" >&2 && exit 2 ;;
  esac
}

[ $# -gt 0 ] || { awk 'NR > 1 && !/^#/ { exit } NR > 1' "$ROOT/scripts/bench/replay.sh"; exit 2; }
for set in "$@"; do run_set "$set"; done
