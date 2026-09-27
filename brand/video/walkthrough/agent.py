#!/usr/bin/env python3
"""A scripted agent for the walkthrough video, standing in for an LLM host.

    agent.py [--flow FILE] [--logs DIR] [--quiet] [--from FILE] [REQUEST...]

Each REQUEST is one session: the agent starts `stretto-proxy` in front of the
official MCP filesystem server, as a host would, writes the conversation to the
proxy's --context file, searches the notes for what the request asks about,
reads what it needs and replies. With --flow, the proxy serves that flow (with
`reach`, its default for a flow without an arbiter), and the agent skips the
reads the flow already made:
their results arrive inside the result of the call before, after a line that
says so.

It prints each call and the exact text of each result, or with --quiet one line
per session. It mirrors scripts/walkthrough.py and docs/walkthrough.md: the
same notes, the same requests, the same behavior. It needs stretto-proxy on
PATH, Node 18+ (npx) and no key.
"""

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

SERVER = "npx -y @modelcontextprotocol/server-filesystem@2026.8.31"
APPENDIX = "--- Also looked up automatically (current results; no need to repeat these calls) ---"

# What each request makes the agent search for, and which hits it reads:
# all of them, or those whose name the customer used.
REQUESTS = {
    "What's the status of project alpha?": ("**/alpha.md", None),
    "Summarize the September meetings.": ("**/meetings/2026-09-*.md", None),
    "What's left on my todo list?": ("**/todo.md", None),
    "Compare projects beta and gamma.": ("**/projects/*.md", ["beta", "gamma"]),
    "What did we decide on September 8?": ("**/2026-09-08.md", None),
    "Read me all my project notes.": ("**/projects/*.md", None),
    "Summarize the August meetings.": ("**/meetings/2026-08-*.md", None),
    "What's in my inbox?": (None, None),
    "What's the status of project gamma?": ("**/gamma.md", None),
    "Summarize the August and September meetings.": ("**/meetings/2026-0[89]-*.md", None),
    "Summarize the October meetings.": ("**/meetings/2026-10-*.md", None),
    "What's the status of project delta?": ("**/delta.md", None),
}


class Session:
    """One MCP session through stretto-proxy, as a host runs it."""

    def __init__(self, proxy: list[str], server: list[str], root: Path, context: Path):
        self.context = context
        context.parent.mkdir(parents=True, exist_ok=True)
        context.write_text("")  # one file per session: the proxy reads it from the start
        self.proc = subprocess.Popen(
            [*proxy, "--context", str(context), "--", *server, str(root)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.next_id = 1
        self.calls = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "walkthrough-agent", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        self.rpc("tools/list", {})

    def send(self, msg: dict) -> None:
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def rpc(self, method: str, params: dict) -> dict:
        i, self.next_id = self.next_id, self.next_id + 1
        self.send({"jsonrpc": "2.0", "id": i, "method": method, "params": params})
        while True:
            line = self.proc.stdout.readline()
            if not line:
                sys.exit(f"agent: the proxy closed during {method}: {self.proc.stderr.read()}")
            msg = json.loads(line)
            if msg.get("id") == i:
                if "error" in msg:
                    sys.exit(f"agent: {method}: {msg['error']}")
                return msg["result"]

    def say(self, role: str, text: str) -> None:
        with self.context.open("a") as f:
            f.write(json.dumps({"role": role, "content": text}) + "\n")

    def call(self, name: str, arguments: dict) -> list[str]:
        self.calls += 1
        result = self.rpc("tools/call", {"name": name, "arguments": arguments})
        return [c["text"] for c in result["content"] if c.get("type") == "text"]

    def close(self) -> str:
        self.proc.stdin.close()
        err = self.proc.stderr.read()
        if self.proc.wait() != 0:
            sys.exit(f"agent: the proxy exited with {self.proc.returncode}: {err}")
        return err


def looked_up(texts: list[str]) -> dict[str, str]:
    """The files the flow read, from the extra text item after the appendix line."""
    found: dict[str, str] = {}
    for extra in texts[1:]:
        if not extra.startswith(APPENDIX):
            continue
        current = None
        for line in extra[len(APPENDIX):].splitlines():
            head = re.fullmatch(r"(\w+) (\{.*\}):", line)
            if head:
                current = json.loads(head.group(2)).get("path") if head.group(1) == "read_text_file" else None
                if current:
                    found[current] = ""
            elif current:
                found[current] += line + "\n"
    return found


def show(label: str, text: str, width: int = 0) -> None:
    lines = text.rstrip("\n").splitlines() or [""]
    for i, line in enumerate(lines):
        print(("  " + label + " " if i == 0 else " " * (len(label) + 3)) + line)


def run(args, request: str, n: int) -> int:
    if request not in REQUESTS:
        sys.exit(f"agent: no script for {request!r}; known requests: {', '.join(map(repr, REQUESTS))}")
    pattern, pick = REQUESTS[request]
    root = Path(args.notes).expanduser().resolve()
    logs = Path(args.logs).expanduser().resolve()
    proxy = [args.proxy, "--record", str(logs), "--domain", "notes"]
    if args.flow:
        proxy += ["--flow", str(Path(args.flow).expanduser().resolve())]
    context = Path(args.work).expanduser().resolve() / "context" / f"{logs.name}-{os.getpid()}-{n}.jsonl"
    s = Session(proxy, shlex.split(args.server), root, context)
    s.say("user", request)
    if not args.quiet:
        print(f"customer: {request}")

    def call(name: str, arguments: dict) -> list[str]:
        texts = s.call(name, arguments)
        if not args.quiet:
            show("→", f"{name} {json.dumps(arguments)}")
            for i, text in enumerate(texts):
                show("←" if i == 0 else " ", text)
        return texts

    read: dict[str, str] = {}
    flow_reads = 0
    if pattern is None:
        listing = call("list_directory", {"path": str(root / "inbox")})
        reply = "Your inbox has: " + listing[0].strip()
    else:
        texts = call("search_files", {"path": str(root), "pattern": pattern})
        more = looked_up(texts)
        read.update(more)
        flow_reads += len(more)
        hits = [] if texts[0].startswith("No matches") else texts[0].splitlines()
        wanted = [p for p in hits if pick is None or any(w in Path(p).stem for w in pick)]
        for path in wanted:
            if path in read:
                continue  # the flow already read it, in the result before
            texts = call("read_text_file", {"path": path})
            read[path] = texts[0]
            more = looked_up(texts)
            read.update(more)
            flow_reads += len(more)
        reply = " ".join(read[p].split("\n")[1] for p in wanted if p in read)
    s.say("assistant", reply)
    s.close()
    if args.quiet:
        print(f"{request:<46} {s.calls} call{'s' if s.calls != 1 else ''}")
    else:
        print(f"agent: {reply}")
        extra = f"; the flow read {flow_reads} file{'s' if flow_reads != 1 else ''} for it" if args.flow else ""
        print(f"{s.calls} call{'s' if s.calls != 1 else ''}{extra}")
    return s.calls


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("requests", nargs="*", help="one session per request")
    ap.add_argument("--from", dest="from_file", help="read the requests from this file, one per line")
    ap.add_argument("--flow", help="serve this flow (stretto-proxy --flow FILE)")
    ap.add_argument("--logs", default="~/.stretto/logs/notes",
                    help="where the proxy records sessions (default: ~/.stretto/logs/notes, where `stretto init` puts them)")
    ap.add_argument("--notes", default="notes", help="the folder the filesystem server serves (default: notes)")
    ap.add_argument("--work", default=".", help="where to keep the conversation files (default: .)")
    ap.add_argument("--proxy", default="stretto-proxy", help="the stretto-proxy binary")
    ap.add_argument("--server", default=os.environ.get("STRETTO_DEMO_SERVER", SERVER), help="the filesystem server's command")
    ap.add_argument("--quiet", action="store_true", help="one line per session")
    args = ap.parse_args()
    if args.from_file:
        args.requests += [l.strip() for l in Path(args.from_file).read_text().splitlines() if l.strip()]
    if not args.requests:
        ap.error("no request: give one or more, or --from FILE")
    for n, request in enumerate(args.requests):
        if n and not args.quiet:
            print()
        run(args, request, n)


if __name__ == "__main__":
    main()
