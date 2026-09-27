#!/bin/sh
# stretto's quickstart: record, learn, review and serve a flow on
# stretto-mcp-demo's tiny shop, with no key and no network.
#
#   examples/quickstart/run.sh [--bin DIR] [--work DIR]
#
# A scripted agent stands in for an LLM. For each customer it finds the
# user, reads their details and both their orders, and cancels one when
# asked. The script records six such sessions through stretto-proxy, learns
# a flow from them (`stretto learn --habit-only`), shows it for review
# (`stretto flow-show`), then serves it to two new customers and counts the
# calls the agent no longer makes. It exits non-zero if a step does not do
# what examples/quickstart/README.md says. It needs only a POSIX shell and
# the binaries, so it also runs in the container image.
#
# --bin DIR   where stretto, stretto-proxy and stretto-mcp-demo are
#             (default: PATH, else this checkout's target/release or
#             target/debug)
# --work DIR  a new or empty directory for the logs and the flow
#             (default: a new temporary directory)

set -eu

bin=
work=
while [ $# -gt 0 ]; do
    case $1 in
    --bin) bin=${2:?--bin needs a directory}; shift 2 ;;
    --work) work=${2:?--work needs a directory}; shift 2 ;;
    -h | --help) sed -n '2,20s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) echo "run.sh: unknown argument $1 (see --help)" >&2; exit 2 ;;
    esac
done

fail() {
    echo "quickstart: FAILED: $*" >&2
    exit 1
}

# The binaries: --bin, else PATH, else this checkout's build.
if [ -z "$bin" ]; then
    if command -v stretto >/dev/null 2>&1 && command -v stretto-proxy >/dev/null 2>&1 &&
        command -v stretto-mcp-demo >/dev/null 2>&1; then
        bin=$(dirname "$(command -v stretto)")
    else
        root=$(cd "$(dirname "$0")/../.." && pwd)
        for dir in "$root/target/release" "$root/target/debug"; do
            if [ -x "$dir/stretto" ] && [ -x "$dir/stretto-proxy" ]; then
                bin=$dir
                break
            fi
        done
    fi
fi
[ -n "$bin" ] || fail "no stretto binaries on PATH or in target/: install them (docs/install.md), or
    cargo build --release -p stretto-report -p stretto-proxy"
stretto=$bin/stretto
proxy=$bin/stretto-proxy
demo=$bin/stretto-mcp-demo
for b in "$stretto" "$proxy" "$demo"; do
    [ -x "$b" ] || fail "$b is missing (pass --bin DIR)"
done

if [ -z "$work" ]; then
    work=$(mktemp -d "${TMPDIR:-/tmp}/stretto-quickstart.XXXXXX")
elif [ -d "$work" ] && [ -n "$(ls -A "$work")" ]; then
    fail "--work $work is not empty"
fi
mkdir -p "$work/context"
work=$(cd "$work" && pwd)
echo "stretto quickstart: $("$stretto" --version) from $bin, working in $work"

# One MCP session: stretto-proxy in front of the demo shop, as an MCP host
# runs it, with the agent on the other end of two FIFOs.
start() { # start NAME [PROXY OPTIONS...]
    name=$1
    shift
    context=$work/context/$name.jsonl
    replies=$work/context/$name.replies
    : >"$context"
    : >"$replies"
    rm -f "$work/to-proxy" "$work/from-proxy"
    mkfifo "$work/to-proxy" "$work/from-proxy"
    "$proxy" --domain shop --context "$context" "$@" -- "$demo" --world retail \
        <"$work/to-proxy" >"$work/from-proxy" 2>>"$work/proxy.log" &
    pid=$!
    exec 3>"$work/to-proxy" 4<"$work/from-proxy"
    next=1
    calls=0
    rpc initialize '{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"quickstart","version":"1"}}'
    printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3
    rpc tools/list '{}'
}

# Send a request and wait for its response, in $reply.
rpc() { # rpc METHOD PARAMS
    id=$next
    next=$((next + 1))
    printf '{"jsonrpc":"2.0","id":%s,"method":"%s","params":%s}\n' "$id" "$1" "$2" >&3
    while IFS= read -r reply <&4; do
        case $reply in *"\"id\":$id,"* | *"\"id\":$id}"*)
            printf '%s\n' "$reply" >>"$replies"
            return 0
            ;;
        esac
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

# A line of the conversation, in the file the proxy reads it from.
say() { # say ROLE TEXT
    printf '{"role":"%s","content":"%s"}\n' "$1" "$2" >>"$context"
}

# Whether the flow already looked TOOL up with ARGUMENTS in this session:
# its lookups come appended to an earlier result, each headed by the call.
looked_up() { # looked_up TOOL ARGUMENTS
    heading=$(printf '%s %s:' "$1" "$2" | sed 's/"/\\"/g')
    grep -qF -- "$heading" "$replies"
}

# The agent calls TOOL, unless the flow has already looked it up.
call() { # call TOOL ARGUMENTS
    if looked_up "$1" "$2"; then
        return 0
    fi
    calls=$((calls + 1))
    rpc tools/call "{\"name\":\"$1\",\"arguments\":$2}"
    case $reply in *'"isError":true'*) fail "$1 $2: $reply" ;; esac
}

# One customer: the agent finds them, reads their details and both orders,
# then cancels the first order or says how the orders stand.
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

echo
echo "1. Record six customers of the demo shop, served by the scripted agent through stretto-proxy"
for spec in 1:cancel 2:status 3:cancel 4:status 5:cancel 6:cancel; do
    n=${spec%%:*}
    kind=${spec#*:}
    start "recorded-$n" --record "$work/logs"
    customer "$n" "$kind"
    stop
    echo "   c$n@example.com ($kind): $calls calls"
done
sessions=$(find "$work/logs" -name '*.jsonl' | wc -l | tr -d ' ')
[ "$sessions" -eq 6 ] || fail "expected 6 session logs in $work/logs, found $sessions"
echo "   $sessions session logs in $work/logs"

echo
echo "2. Learn a flow from them, with no key"
echo "   \$ stretto learn --sessions logs --domain shop --habit-only --out shop.flow.json"
"$stretto" learn --sessions "$work/logs" --domain shop --habit-only --out "$work/shop.flow.json" \
    2>"$work/learn.log" || fail "stretto learn: $(cat "$work/learn.log")"
echo "   $(tail -n 1 "$work/learn.log")"

echo
echo "3. Review it: the tools it may call, the lookups it may make and where their arguments come from"
echo "   \$ stretto flow-show shop.flow.json"
"$stretto" flow-show "$work/shop.flow.json" >"$work/shop.flow.md" || fail "stretto flow-show"
# Its tools, sites and bindings; shop.flow.md also has the program it runs.
awk '/^## / { part = $2 } part ~ /^(Tools|Sites|Bindings)$/ && /^(## |- |\|)/ { print "   " $0 }' \
    "$work/shop.flow.md"
# shellcheck disable=SC2016 # the backquotes are Markdown's, not the shell's
grep -q '^| `find_user_id_by_email` | `get_user_details`' "$work/shop.flow.md" ||
    fail "the flow did not learn to read the user's details after finding them"

echo
echo "4. Serve it to two new customers, and count the calls the agent makes with and without it"
without=0
with=0
for spec in 41:cancel 42:status; do
    n=${spec%%:*}
    kind=${spec#*:}
    start "baseline-$n"
    customer "$n" "$kind"
    stop
    before=$calls
    start "served-$n" --record "$work/served" --flow "$work/shop.flow.json"
    customer "$n" "$kind"
    stop
    echo "   c$n@example.com ($kind): $before calls without the flow, $calls with it"
    without=$((without + before))
    with=$((with + calls))
done
[ "$with" -lt "$without" ] || fail "the flow saved no calls"
echo "   The agent made $with calls instead of $without. It makes one call per LLM turn, so that"
echo "   is $((without - with)) fewer LLM turns: the flow's lookups came back with its first call."
echo "   What the flow decided after c41's first call, from its log (served/<session>.flow.jsonl):"
# A decision line has the site (the call just made), then the lookup with its
# probability times its binding's chance, or the reason to hand back. A `run`
# line ends the flow's run after one call.
awk 'function get(key, pattern) {
         if (!match($0, "\"" key "\":" pattern)) return ""
         return substr($0, RSTART + length(key) + 3, RLENGTH - length(key) - 3)
     }
     /"run":[{]/ { exit }
     {
         site = get("site", "\"[^\"]*\"")
         gsub(/"/, "", site)
         if (get("action", "\"[a-z_]*\"") == "\"lookup\"") {
             tool = get("tool", "\"[^\"]*\"")
             gsub(/"/, "", tool)
             printf "   after %s: looks up %s %s (%.2f x %.2f)\n", site, tool,
                 get("arguments", "[{][^}]*[}]"), get("prob", "[0-9.e-]*"), get("binding", "[0-9.e-]*")
         } else {
             reason = get("reason", "\"[^\"]*\"")
             gsub(/"/, "", reason)
             printf "   after %s: hands back (%s)\n", site, reason
         }
     }' "$(find "$work/served" -name '*.flow.jsonl' | sort | head -n 1)"

echo
echo "Done. Everything is in $work: the session logs, the flow and its review (shop.flow.md)."
echo "Next: wrap your own MCP server with \`stretto init\` (docs/install.md), and run the whole loop"
echo "on a real server with docs/walkthrough.md."
