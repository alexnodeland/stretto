#!/usr/bin/env python3
"""Record the walkthrough for the video: run script.sh in a pseudo-terminal and
write what it prints, with timing, as an asciicast (v2) file.

    python3 brand/video/walkthrough/capture.py --bin DIR [--work DIR] [--out FILE]

--bin is the directory with the stretto binaries (stretto, stretto-proxy).
Every command in script.sh runs for real, in a fresh working directory, with
a small environment: PATH, HOME, locale, terminal size and proxy settings; no
other variable of yours reaches the commands, so no key does. The working
directory's path is written as /home/me, as docs/walkthrough.md shows it;
nothing else in the output is changed.

The cast's events are [seconds, "o", text] for output and [seconds, "m", json]
for the script's markers: {"step": title}, {"caption": text}, {"cmd": command}
and {"exit": status}. render.mjs turns it into the video.
"""

import argparse
import base64
import fcntl
import json
import os
import pty
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
COLS, ROWS = 128, 26
DISPLAY_HOME = "/home/me"
MARK = re.compile(rb"\x1b\]7777;([a-z]+);([A-Za-z0-9+/=]*)\x07")
# Variables the commands may see. Everything else stays out, keys included.
KEEP = {"HOME", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR", "USER", "LOGNAME", "SHELL",
        "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY", "https_proxy", "http_proxy", "no_proxy",
        "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS", "REQUESTS_CA_BUNDLE", "NPM_CONFIG_CACHE"}


def environment(bin_dir: Path, work: Path) -> dict:
    env = {k: v for k, v in os.environ.items() if k in KEEP}
    env.update({
        "PATH": f"{bin_dir}:{os.environ.get('PATH', '/usr/bin:/bin')}",
        "TERM": "xterm-256color", "COLUMNS": str(COLS), "LINES": str(ROWS),
        "LANG": env.get("LANG", "C.UTF-8"),
        "STRETTO_CAPTURE": "1", "STRETTO_WORK": str(work), "STRETTO_BIN": str(bin_dir),
        "PS1": "$ ",
    })
    return env


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True, type=Path, help="directory with stretto and stretto-proxy")
    ap.add_argument("--work", type=Path, help="working directory (default: a new temporary one)")
    ap.add_argument("--out", type=Path, default=HERE / "walkthrough.cast")
    ap.add_argument("--script", type=Path, default=HERE / "script.sh")
    args = ap.parse_args()

    bin_dir = args.bin.resolve()
    for name in ("stretto", "stretto-proxy"):
        if not (bin_dir / name).exists():
            sys.exit(f"capture: no {name} in {bin_dir}")
    work = (args.work or Path(tempfile.mkdtemp(prefix="stretto-walkthrough-"))).resolve()
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    env = environment(bin_dir, work)
    version = subprocess.run([str(bin_dir / "stretto"), "--version"], capture_output=True, text=True, env=env).stdout.strip()

    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(work)
        os.execve("/bin/bash", ["bash", "--noprofile", "--norc", str(args.script.resolve())], env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

    start = time.monotonic()
    events: list[list] = []
    pending = b""
    decoder_tail = b""

    def paths(text: str) -> str:
        return text.replace(str(work), DISPLAY_HOME)

    def emit_output(data: bytes, t: float) -> None:
        nonlocal decoder_tail
        data = decoder_tail + data
        # Keep an incomplete UTF-8 sequence for the next chunk.
        cut = len(data)
        for back in range(1, 4):
            if cut - back >= 0 and (data[cut - back] & 0xC0) == 0xC0:
                need = 2 if data[cut - back] < 0xE0 else 3 if data[cut - back] < 0xF0 else 4
                if back < need:
                    cut -= back
                break
        decoder_tail = data[cut:]
        text = data[:cut].decode("utf-8", errors="replace")
        if text:
            events.append([round(t, 4), "o", paths(text)])

    exit_status = None
    while True:
        try:
            ready, _, _ = select.select([fd], [], [], 0.05)
        except InterruptedError:
            continue
        if not ready:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                exit_status = status
                break
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        t = time.monotonic() - start
        pending += chunk
        while True:
            m = MARK.search(pending)
            if not m:
                # Hold back the start of a marker that the next read will complete.
                keep = pending.rfind(b"\x1b")
                tail = pending[keep:] if keep >= 0 else b""
                if tail and b"\x07" not in tail and b"\x1b]7777;".startswith(tail[:8]) and len(tail) < 8192:
                    emit_output(pending[:keep], t)
                    pending = pending[keep:]
                else:
                    emit_output(pending, t)
                    pending = b""
                break
            emit_output(pending[:m.start()], t)
            kind = m.group(1).decode()
            value = paths(base64.b64decode(m.group(2)).decode("utf-8"))
            events.append([round(t, 4), "m", json.dumps({kind: int(value) if kind == "exit" else value})])
            pending = pending[m.end():]
    emit_output(pending, time.monotonic() - start)
    if exit_status is None:
        _, exit_status = os.waitpid(pid, 0)
    code = os.waitstatus_to_exitcode(exit_status)

    header = {
        "version": 2, "width": COLS, "height": ROWS,
        "timestamp": int(time.time()),
        "title": "stretto: the walkthrough",
        "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color"},
        "stretto": {
            "version": version,
            "script": "brand/video/walkthrough/script.sh",
            "note": f"Paths in the capture's working directory are shown under {DISPLAY_HOME}.",
        },
    }
    with args.out.open("w") as f:
        f.write(json.dumps(header, ensure_ascii=False) + "\n")
        for e in events:
            f.write(json.dumps(e, ensure_ascii=False) + "\n")
    steps = sum(1 for e in events if e[1] == "m" and '"step"' in e[2])
    cmds = sum(1 for e in events if e[1] == "m" and '"cmd"' in e[2])
    print(f"capture: {steps} steps, {cmds} commands, {events[-1][0] if events else 0:.1f} s, exit {code}: {args.out}")
    if code != 0:
        sys.exit(code)


if __name__ == "__main__":
    main()
