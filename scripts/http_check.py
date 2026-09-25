"""stretto-proxy in front of the reference Streamable HTTP server,
@modelcontextprotocol/server-everything, as CI runs it.

    python3 scripts/http_check.py [--bin target/debug]

It starts the server on a free port, runs the proxy with `--upstream` and
`--record`, and checks each part of the transport the proxy relies on:
initialization and the session, a JSON answer, a tool call whose event
stream carries progress notifications before its result, the server's own
log messages on the GET stream, the session log, and a clean exit. It
needs Node 18 or later, and no key.
"""

import argparse
import json
import os
import queue
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

SERVER = ["npx", "-y", "@modelcontextprotocol/server-everything@2026.8.31", "streamableHttp"]


def fail(msg: str) -> None:
    print(f"http_check: FAILED: {msg}", file=sys.stderr)
    sys.exit(1)


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default="target/debug", help="directory with stretto-proxy")
    args = ap.parse_args()
    proxy_bin = str(Path(args.bin).resolve() / "stretto-proxy")
    port = free_port()
    # Its own process group, so that the server npx starts goes with it.
    server = subprocess.Popen(SERVER, env={**os.environ, "PORT": str(port)}, start_new_session=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        started = time.time()
        for line in server.stdout:
            if "listening" in line.lower():
                break
            if time.time() - started > 120:
                fail("the server did not start")
        work = Path(tempfile.mkdtemp(prefix="stretto-http-"))
        url = f"http://127.0.0.1:{port}/mcp"
        proxy = subprocess.Popen([proxy_bin, "--record", str(work / "logs"), "--domain", "everything",
                                  "--upstream", url],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        lines: queue.Queue = queue.Queue()
        threading.Thread(target=lambda: [lines.put(json.loads(l)) for l in proxy.stdout], daemon=True).start()

        def send(message: dict) -> None:
            proxy.stdin.write(json.dumps(message) + "\n")
            proxy.stdin.flush()

        def answer(i: int, wait: float = 30) -> tuple[dict, list]:
            notes, end = [], time.time() + wait
            while time.time() < end:
                try:
                    m = lines.get(timeout=0.5)
                except queue.Empty:
                    continue
                if m.get("id") == i and "method" not in m:
                    return m, notes
                notes.append(m)
            fail(f"no answer to request {i}")

        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "http_check", "version": "1"}}})
        init, _ = answer(1)
        if "result" not in init:
            fail(f"initialize: {init}")
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        send({"jsonrpc": "2.0", "id": 2, "method": "logging/setLevel", "params": {"level": "debug"}})
        answer(2)
        send({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}})
        listed, _ = answer(3)
        names = [t["name"] for t in listed["result"]["tools"]]
        print(f"1. initialized; {len(names)} tools listed")
        send({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
            "name": "trigger-long-running-operation", "arguments": {"duration": 2, "steps": 4},
            "_meta": {"progressToken": "check"}}})
        done, notes = answer(4)
        progress = [n for n in notes if n.get("method") == "notifications/progress"]
        print(f"2. a long tool call: {len(progress)} progress notifications, then its result")
        if len(progress) < 2 or "result" not in done:
            fail(f"progress on the event stream: {notes} {done}")
        send({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
            "name": "toggle-simulated-logging", "arguments": {}}})
        answer(5)
        own, end = [], time.time() + 30
        while not own and time.time() < end:
            try:
                m = lines.get(timeout=0.5)
            except queue.Empty:
                continue
            if m.get("method") == "notifications/message":
                own.append(m)
        print(f"3. the server's own stream: {len(own)} log message(s)")
        if not own:
            fail("no message on the GET stream")
        proxy.stdin.close()
        if proxy.wait(timeout=30) != 0:
            fail(f"the proxy exited with {proxy.returncode}: {proxy.stderr.read()}")
        log = next((work / "logs").glob("*.jsonl"))
        entries = [json.loads(l) for l in log.read_text().splitlines()]
        header, rest = entries[0], entries[1:]
        print(f"4. session log: {len(rest)} messages from {header['server_command']}")
        if header["server_command"] != [url] or len(rest) < 10:
            fail(f"the session log: {header}")
        print("done")
    finally:
        os.killpg(server.pid, signal.SIGTERM)


if __name__ == "__main__":
    main()
