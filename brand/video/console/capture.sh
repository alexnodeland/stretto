#!/bin/sh
# Capture the console film's shots from the real stretto-console.
#
#   sh brand/video/console/capture.sh
#
# Builds nothing: it needs the console's UI built (npm --prefix console ci &&
# npm --prefix console run build) and the binaries (cargo build -p
# stretto-console -p stretto-report -p stretto-proxy), in $STRETTO_BIN
# (default: target/debug). As the walkthrough is, it is shot in /home/me: the
# binaries are copied to /home/me/.cargo/bin, where `cargo install` puts them,
# and the console's test fixtures to /home/me/.stretto, with their sessions
# moved forward to yesterday as console/e2e/real/serve.sh does. So it needs to
# write /home/me (run it in a container or as root on a throwaway machine).
# Two consoles serve that copy: one as a user runs it, on 7891, and one with
# --read-only, on 7892. Then capture.mjs drives them and writes shots/.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
bin=${STRETTO_BIN:-$root/target/debug}
home=/home/me

for b in stretto-console stretto stretto-proxy stretto-mcp-demo; do
    [ -x "$bin/$b" ] || { echo "capture.sh: $bin/$b is missing" >&2; exit 1; }
done
[ -f "$root/console/dist/index.html" ] || { echo "capture.sh: build the UI first: npm --prefix console run build" >&2; exit 1; }

rm -rf "$home/.stretto" "$home/.cargo/bin"
mkdir -p "$home/.cargo/bin"
for b in stretto-console stretto stretto-proxy stretto-mcp-demo; do cp "$bin/$b" "$home/.cargo/bin/$b"; done
cp -R "$root/crates/stretto-console/tests/fixtures/home" "$home/.stretto"
python3 - "$home/.stretto" <<'EOF'
import re, sys, time
from pathlib import Path

DAY = 86_400_000
files = sorted(Path(sys.argv[1]).glob("*/*/*.jsonl"))
started = re.compile(r'"started_unix_ms":(\d+)')
latest = max(int(m.group(1)) for f in files for m in started.finditer(f.read_text()))
shift = max(0, (int(time.time() * 1000) - DAY - latest) // DAY * DAY)
for f in files:
    f.write_text(started.sub(lambda m: f'"started_unix_ms":{int(m.group(1)) + shift}', f.read_text()))
EOF

cd "$home"
PATH=$home/.cargo/bin:$PATH
export PATH
# No key: the console shows what stretto does without one.
unset TYPESAFE_API_KEY ZAI_API_KEY
# A debug build reads the UI from console/dist, where it was built.
stretto-console --no-auth --listen 127.0.0.1:7891 --data "$home/.stretto" >/dev/null 2>&1 &
one=$!
stretto-console --no-auth --read-only --listen 127.0.0.1:7892 --data "$home/.stretto" >/dev/null 2>&1 &
two=$!
trap 'kill $one $two 2>/dev/null || true' EXIT
for port in 7891 7892; do
    i=0
    until curl -sf "http://127.0.0.1:$port/api/health" >/dev/null; do
        i=$((i + 1)); [ $i -lt 60 ] || { echo "capture.sh: the console on $port did not start" >&2; exit 1; }
        sleep 0.5
    done
done
cd "$root/brand"
node video/console/capture.mjs
