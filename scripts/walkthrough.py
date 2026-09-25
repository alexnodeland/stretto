"""docs/walkthrough.md as a script: record, learn, review, audit and serve a
flow on the official MCP filesystem server, with a scripted agent standing in
for an LLM host.

    python3 scripts/walkthrough.py [--bin target/debug] [--work DIR] [--server CMD]

It builds a small notes folder, records sessions through stretto-proxy,
learns a flow with no key (`stretto learn --habit-only`), shows it for review
(`stretto flow-show`), audits it on sessions it never saw, serves it, then
learns it again from every session and reviews the change (`stretto
flow-diff`). It prints what each step shows and exits non-zero if a step
does not do what the walkthrough says. CI runs it, so the walkthrough cannot
drift from the code.
"""

import argparse
import json
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SERVER = "npx -y @modelcontextprotocol/server-filesystem@2026.8.31"
APPENDIX = "--- Also looked up automatically (current results; no need to repeat these calls) ---"

NOTES = {
    "projects/alpha.md": "# Alpha\nStatus: shipped on 2026-09-02.\n",
    "projects/beta.md": "# Beta\nStatus: waiting on the vendor's API keys.\n",
    "projects/gamma.md": "# Gamma\nStatus: design review on 2026-10-09.\n",
    "projects/delta.md": "# Delta\nStatus: scoping; owner to be named.\n",
    "meetings/2026-08-18.md": "# 2026-08-18\nAgreed the Q4 budget.\n",
    "meetings/2026-08-25.md": "# 2026-08-25\nAlpha launch checklist.\n",
    "meetings/2026-09-01.md": "# 2026-09-01\nAlpha go/no-go: go.\n",
    "meetings/2026-09-08.md": "# 2026-09-08\nBeta blocked on vendor keys.\n",
    "meetings/2026-09-15.md": "# 2026-09-15\nHiring plan for Delta.\n",
    "meetings/2026-10-06.md": "# 2026-10-06\nGamma design review moved to 10-09.\n",
    "meetings/2026-10-13.md": "# 2026-10-13\nDelta owner: Priya.\n",
    "todo.md": "- Renew the domain\n- Book the offsite\n",
    "inbox/receipt.txt": "Receipt 4471: 3 keyboards.\n",
}

# What the customer asks, what the agent searches for, and which of the hits
# it reads (all, or those whose name the customer used).
RECORDED = [
    ("What's the status of project alpha?", "**/alpha.md", None),
    ("Summarize the September meetings.", "**/meetings/2026-09-*.md", None),
    ("What's left on my todo list?", "**/todo.md", None),
    ("Compare projects beta and gamma.", "**/projects/*.md", ["beta", "gamma"]),
    ("What did we decide on September 8?", "**/2026-09-08.md", None),
    ("Read me all my project notes.", "**/projects/*.md", None),
    ("Summarize the August meetings.", "**/meetings/2026-08-*.md", None),
    ("What's in my inbox?", None, None),
]
NEW = [
    ("What's the status of project gamma?", "**/gamma.md", None),
    ("Summarize the August and September meetings.", "**/meetings/2026-0[89]-*.md", None),
    ("What's in my inbox?", None, None),
]
SERVED = [
    ("Summarize the October meetings.", "**/meetings/2026-10-*.md", None),
    ("What's the status of project delta?", "**/delta.md", None),
]


def fail(msg: str) -> None:
    print(f"walkthrough: FAILED: {msg}", file=sys.stderr)
    sys.exit(1)


class Session:
    """One MCP session through stretto-proxy, as a host would run it."""

    def __init__(self, proxy: list[str], server: list[str], root: Path, context: Path):
        self.context = context
        context.write_text("")  # One file per session: the proxy reads it from the start.
        self.proc = subprocess.Popen(
            [*proxy, "--context", str(context), "--", *server, str(root)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.next_id = 1
        self.calls = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "walkthrough", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        self.tools = self.rpc("tools/list", {})["tools"]

    def send(self, msg: dict) -> None:
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def rpc(self, method: str, params: dict) -> dict:
        i, self.next_id = self.next_id, self.next_id + 1
        self.send({"jsonrpc": "2.0", "id": i, "method": method, "params": params})
        while True:
            line = self.proc.stdout.readline()
            if not line:
                fail(f"the proxy closed during {method}: {self.proc.stderr.read()}")
            msg = json.loads(line)
            if msg.get("id") == i:
                if "error" in msg:
                    fail(f"{method}: {msg['error']}")
                return msg["result"]

    def say(self, role: str, text: str) -> None:
        with self.context.open("a") as f:
            f.write(json.dumps({"role": role, "content": text}) + "\n")

    def call(self, name: str, arguments: dict) -> tuple[str, dict]:
        """The tool's own text, and what the flow looked up after it, by path."""
        self.calls += 1
        result = self.rpc("tools/call", {"name": name, "arguments": arguments})
        texts = [c["text"] for c in result["content"] if c.get("type") == "text"]
        own = texts[0] if texts else ""
        flow: dict = {}
        for extra in texts[1:]:
            if not extra.startswith(APPENDIX):
                continue
            # Each lookup is a line `tool {arguments}:`, then its result.
            current = None
            for line in extra[len(APPENDIX):].splitlines():
                head = re.fullmatch(r"(\w+) (\{.*\}):", line)
                if head:
                    current = json.loads(head.group(2)).get("path") if head.group(1) == "read_text_file" else None
                    if current:
                        flow[current] = ""
                elif current:
                    flow[current] += line + "\n"
        return own, flow

    def close(self) -> str:
        self.proc.stdin.close()
        err = self.proc.stderr.read()
        if self.proc.wait() != 0:
            fail(f"the proxy exited with {self.proc.returncode}: {err}")
        return err


def agent(s: Session, root: Path, request: str, pattern, pick) -> dict:
    """A scripted agent: search, read what it needs, reply. Returns counts."""
    s.say("user", request)
    read_by_flow = 0
    if pattern is None:
        listing, _ = s.call("list_directory", {"path": str(root / "inbox")})
        s.say("assistant", f"Your inbox has: {listing}")
        return {"calls": s.calls, "flow": 0}
    hits, flow = s.call("search_files", {"path": str(root), "pattern": pattern})
    paths = [] if hits.startswith("No matches") else hits.splitlines()
    wanted = [p for p in paths if pick is None or any(w in Path(p).stem for w in pick)]
    read = dict(flow)
    read_by_flow += len(flow)
    for path in wanted:
        if path in read:
            continue
        text, more = s.call("read_text_file", {"path": path})
        read[path] = text
        read.update(more)
        read_by_flow += len(more)
    s.say("assistant", "Here is what I found: " + " ".join(read[p].split("\n")[1] for p in wanted))
    return {"calls": s.calls, "flow": read_by_flow}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default="target/debug", help="directory with stretto and stretto-proxy")
    ap.add_argument("--work", help="working directory (default: a temporary one)")
    ap.add_argument("--server", default=SERVER, help="the filesystem server's command")
    args = ap.parse_args()
    bin_dir = Path(args.bin).resolve()
    stretto, proxy = str(bin_dir / "stretto"), str(bin_dir / "stretto-proxy")
    work = Path(args.work or tempfile.mkdtemp(prefix="stretto-walkthrough-")).resolve()
    if work.exists():
        shutil.rmtree(work)
    root = work / "notes"
    for name, text in NOTES.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text)
    server = shlex.split(args.server)
    ctx = work / "context"
    ctx.mkdir(parents=True)

    def record(batch, logs: Path, extra=()):
        counts = []
        for n, (request, pattern, pick) in enumerate(batch):
            s = Session([proxy, "--record", str(logs), "--domain", "notes", *extra],
                        server, root, ctx / f"{logs.name}-{n}.jsonl")
            if n == 0 and logs.name == "logs":
                kinds = {t["name"]: t.get("annotations", {}).get("readOnlyHint") for t in s.tools}
                print(f"1. tools/list: {sum(k is True for k in kinds.values())} of {len(kinds)} tools "
                      f"say readOnlyHint: true")
                if kinds.get("read_text_file") is not True or kinds.get("write_file") is not False:
                    fail("the server does not mark its tools as the walkthrough expects")
            counts.append(agent(s, root, request, pattern, pick))
            s.close()
        return counts

    # 2. Record sessions.
    logs = work / "logs"
    record(RECORDED, logs)
    print(f"2. recorded {len(list(logs.glob('*.jsonl')))} sessions in {logs}")

    # 3. Learn a flow with no key.
    flow_path = work / "notes.flow.json"
    out = subprocess.run([stretto, "learn", "--sessions", str(logs), "--domain", "notes", "--habit-only",
                          "--out", str(flow_path)], capture_output=True, text=True)
    if out.returncode:
        fail(f"stretto learn: {out.stderr}")
    print("3. " + out.stderr.strip().splitlines()[-1])
    flow = json.loads(flow_path.read_text())
    reads = sorted(t for t, k in flow["manifest"]["tools"].items() if k == "read")
    print(f"   read-only tools: {', '.join(reads)}")
    for (site, failed), lookups in flow["sites"]["next"]:
        print(f"   after {site}{' (error)' if failed else ''}: {lookups}")
    sources = {tuple(k): v for k, v in flow["bindings"]["sources"]}
    found = sources.get(("read_text_file", "path"), {}).get("found", [])
    print(f"   read_text_file's path came from: {found}")
    if [["search_files", "$[*]"]] != [f[0] for f in found if f[0][1] == "$[*]"]:
        fail("the flow did not learn to read the files a search lists")
    out = subprocess.run([stretto, "flow-show", str(flow_path)], capture_output=True, text=True)
    if out.returncode:
        fail(f"stretto flow-show: {out.stderr}")
    shown = out.stdout
    (work / "notes.flow.md").write_text(shown)
    for line in shown.splitlines():
        if line.startswith("| `"):
            print(f"   flow-show: {line}")
    if "| `search_files` | `read_text_file` (7) |" not in shown or "looks up `read_text_file`" not in shown:
        fail("flow-show does not show the flow reading what a search found")

    # 4. Audit it on sessions it never saw.
    new_logs = work / "logs-new"
    record(NEW, new_logs)
    audit = work / "audit.json"
    out = subprocess.run([stretto, "audit", "--flow", str(flow_path), "--sessions", str(new_logs),
                          "--json", str(audit), "--out", str(work / "audit.md")],
                         capture_output=True, text=True)
    if out.returncode:
        fail(f"stretto audit: {out.stderr}")
    a = json.loads(audit.read_text())
    print(f"4. audit ({a['decider']}): {a['decisions']} decisions in {a['episodes']} new sessions, "
          f"agreement {a['agreement']:.0%}, {-a['mean_log_prob']:.2f} nats per decision")
    for site in a["sites"]:
        print(f"   {site['site']}: {site['decisions']} decisions, agreement {site['agreement']:.0%}")
    if a["unanswered"] or a["decisions"] == 0 or a["agreement"] < 0.5:
        fail("the audit did not score the new sessions as expected")

    # 5. Serve it.
    served = work / "logs-served"
    counts = record(SERVED, served, ["--flow", str(flow_path), "--flow-decider", "habit"])
    for (request, _, _), c in zip(SERVED, counts):
        print(f"5. \"{request}\": the agent made {c['calls']} calls; the flow read {c['flow']} files for it")
    decisions = [json.loads(l) for p in sorted(served.glob("*.flow.jsonl")) for l in p.read_text().splitlines()]
    for d in decisions[:4]:
        print(f"   flow log: {json.dumps(d)[:160]}")
    if counts[0]["flow"] < 2:
        fail("the flow did not read the October meetings")

    # 6. Learn again, and review what changed.
    both = work / "logs-all"
    both.mkdir()
    for log in [*logs.glob("*.jsonl"), *new_logs.glob("*.jsonl")]:
        shutil.copy(log, both / log.name)
    again = work / "notes-2.flow.json"
    out = subprocess.run([stretto, "learn", "--sessions", str(both), "--domain", "notes", "--habit-only",
                          "--out", str(again)], capture_output=True, text=True)
    if out.returncode:
        fail(f"stretto learn: {out.stderr}")
    out = subprocess.run([stretto, "flow-diff", str(flow_path), str(again)], capture_output=True, text=True)
    (work / "flow-diff.md").write_text(out.stdout)
    print(f"6. flow-diff exited with {out.returncode}:")
    for line in out.stdout.splitlines():
        if line.strip():
            print(f"   {line}")
    if out.returncode != 0 or "Nothing needs review" not in out.stdout:
        fail("learning from the new sessions changed what the flow may do")
    print(f"done: everything is in {work}")


if __name__ == "__main__":
    main()
