#!/bin/sh
# Regenerate tests/fixtures/home: a small ~/.stretto made with the real tools
# on stretto-mcp-demo's retail world, for the console's tests.
#
#   crates/stretto-console/tests/fixtures/regenerate.sh [--bin DIR]
#
# 1. examples/quickstart/run.sh records six customers (logs/shop), learns
#    shop.flow.json from them, and serves it to two more (served/shop, with
#    their flow logs).
# 2. Three customers are served with the flow in shadow (shadow/shop:
#    stretto-proxy --flow-shadow), and `stretto promote --decider reach`
#    keeps the flow to the sites where its lookups were the agent's own
#    (shop.promoted.flow.json).
# 3. One customer of the `retail` domain goes through the policy guards with
#    the confirmation judge logging (the mock oracle, so no key and no
#    network), after a first call that fails (logs/retail).
# 4. servers.json registers the shop (stdio) and a Streamable HTTP server.
#
# Session ids and times are the run's. The headers name the demo server by
# its bare name, as if the binaries were on PATH, so the fixtures carry no
# path of the machine that made them.
#
# --bin DIR   where stretto, stretto-proxy and stretto-mcp-demo are (default:
#             $CARGO_TARGET_DIR/debug, else target/debug); build them with
#             cargo build -p stretto-report -p stretto-proxy

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../.." && pwd)
bin=
while [ $# -gt 0 ]; do
    case $1 in
    --bin) bin=${2:?--bin needs a directory}; shift 2 ;;
    -h | --help) sed -n '2,25s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) echo "regenerate.sh: unknown argument $1 (see --help)" >&2; exit 2 ;;
    esac
done
[ -n "$bin" ] || bin=${CARGO_TARGET_DIR:-$root/target}/debug
bin=$(cd "$bin" && pwd)
stretto=$bin/stretto
proxy=$bin/stretto-proxy
demo=$bin/stretto-mcp-demo
for b in "$stretto" "$proxy" "$demo"; do
    [ -x "$b" ] || { echo "regenerate.sh: $b is missing (pass --bin DIR)" >&2; exit 1; }
done

fail() {
    echo "regenerate.sh: FAILED: $*" >&2
    exit 1
}

work=$(mktemp -d "${TMPDIR:-/tmp}/stretto-console-fixtures.XXXXXX")
trap 'rm -rf "${work:?}"' EXIT
home=$work/home

# 1. The quickstart: six recorded customers, the flow, two served ones.
sh "$root/examples/quickstart/run.sh" --bin "$bin" --work "$work/qs" >"$work/quickstart.out" 2>&1 ||
    fail "the quickstart: $(tail -n 5 "$work/quickstart.out")"
mkdir -p "$home/logs/shop" "$home/served/shop" "$home/shadow/shop" "$home/logs/retail"
cp "$work/qs"/logs/*.jsonl "$home/logs/shop/"
cp "$work/qs"/served/*.jsonl "$home/served/shop/"
cp "$work/qs/shop.flow.json" "$home/shop.flow.json"

# A scripted agent, as the quickstart's: stretto-proxy in front of the demo
# shop, with the agent on the other end of two FIFOs.
mkdir -p "$work/context"
start() { # start NAME [PROXY OPTIONS...]
    context=$work/context/$1.jsonl
    shift
    : >"$context"
    rm -f "$work/to-proxy" "$work/from-proxy"
    mkfifo "$work/to-proxy" "$work/from-proxy"
    "$proxy" --context "$context" "$@" -- "$demo" --world retail \
        <"$work/to-proxy" >"$work/from-proxy" 2>>"$work/proxy.log" &
    pid=$!
    exec 3>"$work/to-proxy" 4<"$work/from-proxy"
    next=1
    called=
    rpc initialize '{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixtures","version":"1"}}'
    printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3
    rpc tools/list '{}'
}

rpc() { # rpc METHOD PARAMS
    id=$next
    next=$((next + 1))
    printf '{"jsonrpc":"2.0","id":%s,"method":"%s","params":%s}\n' "$id" "$1" "$2" >&3
    while IFS= read -r reply <&4; do
        case $reply in *"\"id\":$id,"* | *"\"id\":$id}"*) return 0 ;; esac
    done
    fail "stretto-proxy closed during $1 (see $work/proxy.log)"
}

stop() {
    exec 3>&-
    cat <&4 >/dev/null
    exec 4<&-
    wait "$pid" || fail "stretto-proxy exited with $? (see $work/proxy.log)"
    rm -f "$work/to-proxy" "$work/from-proxy"
}

say() { # say ROLE TEXT
    printf '{"role":"%s","content":"%s"}\n' "$1" "$2" >>"$context"
}

# As the quickstart's agent, it reads each result before its next call.
call() { # call TOOL ARGUMENTS
    [ -z "$called" ] || sleep 0.6
    called=1
    rpc tools/call "{\"name\":\"$1\",\"arguments\":$2}"
}

customer() { # customer N cancel|status
    n=$1
    if [ "$2" = cancel ]; then
        say user "Hi, I'm c$n@example.com. I want to cancel an order I no longer need."
    else
        say user "Hello, c$n@example.com here. Where are my orders?"
    fi
    call find_user_id_by_email "{\"email\":\"c$n@example.com\"}"
    call get_user_details "{\"user_id\":\"user_$n\"}"
    call get_order_details "{\"order_id\":\"#W${n}a\"}"
    call get_order_details "{\"order_id\":\"#W${n}b\"}"
    if [ "$2" = cancel ]; then
        say assistant "You have two pending orders, #W${n}a and #W${n}b. Which one should I cancel?"
        say user "The first one, please."
        call cancel_pending_order "{\"order_id\":\"#W${n}a\",\"reason\":\"no longer needed\"}"
        say assistant "Order #W${n}a is cancelled."
    else
        say assistant "Your orders #W${n}a and #W${n}b are both pending."
    fi
}

# 2. Three customers with the flow in shadow, then the promoted flow.
for spec in 51:cancel 52:status 53:cancel; do
    start "shadow-${spec%%:*}" --record "$home/shadow/shop" --domain shop \
        --flow "$home/shop.flow.json" --flow-shadow
    customer "${spec%%:*}" "${spec#*:}"
    stop
done
grep -q '"shadow":true' "$home"/shadow/shop/*.flow.jsonl || fail "no shadow decisions"
"$stretto" promote --flow "$home/shop.flow.json" --sessions "$home/shadow/shop" \
    --decider reach --oracle-cache "$work/oracle-cache" \
    --out "$home/shop.promoted.flow.json" --report "$work/promote.md" 2>"$work/promote.log" ||
    fail "stretto promote: $(cat "$work/promote.log")"

# 3. A retail customer through the guards, the judge logging; the first
# call fails (a mistyped email).
start retail-61 --record "$home/logs/retail" --domain retail --guards \
    --confirm-judge log --oracle mock --oracle-cache "$work/oracle-cache"
say user "Hi, I'm c61@example.con, no, c61@example.com. Please cancel my order, I ordered it by mistake."
call find_user_id_by_email '{"email":"c61@example.con"}'
call find_user_id_by_email '{"email":"c61@example.com"}'
call get_user_details '{"user_id":"user_61"}'
call get_order_details '{"order_id":"#W61a"}'
say assistant "Order #W61a is pending. Shall I cancel it, as ordered by mistake?"
say user "Yes, cancel it."
call cancel_pending_order '{"order_id":"#W61a","reason":"ordered by mistake"}'
say assistant "Order #W61a is cancelled."
stop
ls "$home"/logs/retail/*.confirm.jsonl >/dev/null 2>&1 || fail "no confirmation log"

# 4. The registry: the shop behind the proxy with its flow, and an HTTP
# server whose credentials come from an environment variable.
cat >"$home/servers.json" <<'EOF'
{
  "stretto_servers": 1,
  "servers": [
    {
      "name": "shop",
      "description": "stretto-mcp-demo's tiny retail shop, served with its flow",
      "upstream": {"kind": "stdio", "command": ["stretto-mcp-demo", "--world", "retail"], "env": []},
      "mode": "serve",
      "flow": "shop.flow.json",
      "record_dir": "served/shop",
      "decider": null,
      "threshold": null,
      "created_unix_ms": 1790553600000,
      "updated_unix_ms": 1790553600000
    },
    {
      "name": "orders",
      "description": "An order service over Streamable HTTP, recording",
      "upstream": {"kind": "http", "url": "https://mcp.example.com/orders/mcp", "headers": [{"name": "Authorization", "env": "ORDERS_AUTH"}]},
      "mode": "record",
      "flow": null,
      "record_dir": null,
      "decider": null,
      "threshold": null,
      "created_unix_ms": 1790553600000,
      "updated_unix_ms": 1790553600000
    }
  ]
}
EOF

# The headers name the demo by its bare name.
for log in "$home"/*/*/*.jsonl; do
    case $log in *.flow.jsonl | *.confirm.jsonl) continue ;; esac
    sed "1s|\"$bin/stretto-mcp-demo\"|\"stretto-mcp-demo\"|" "$log" >"$log.tmp"
    mv "$log.tmp" "$log"
done
if grep -rqe "$bin" -e "$work" "$home"; then
    fail "a path of this machine is left in the fixtures"
fi

out=$here/home
rm -rf "${out:?}"
cp -R "$home" "$out"
echo "regenerate.sh: wrote $out ($(du -sk "$out" | cut -f1) KiB, $(find "$out" -type f | wc -l | tr -d ' ') files)"
