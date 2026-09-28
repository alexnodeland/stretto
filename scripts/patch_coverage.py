"""Every line a change adds to the workspace's Rust is run by a test.

    python3 scripts/patch_coverage.py lcov.info [REF]

Reads the LCOV report `make coverage` writes, and the lines the working tree
adds under crates/ since its merge base with REF (default origin/main; fetch
it first). Fails, naming each, when a line that llvm-cov instruments has not
run. Lines it does not instrument, such as comments and declarations, do not
count.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def git(*args: str) -> str:
    done = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True)
    if done.returncode:
        sys.exit(f"git {' '.join(args)}: {done.stderr.strip()}")
    return done.stdout


def hits(lcov: Path) -> dict[str, dict[int, int]]:
    """Each source file's instrumented lines and how often each ran, by path
    relative to the repository."""
    out: dict[str, dict[int, int]] = {}
    lines: dict[int, int] = {}
    for line in lcov.read_text().splitlines():
        if line.startswith("SF:"):
            path = Path(line[3:])
            if path.is_absolute() and path.is_relative_to(ROOT):
                path = path.relative_to(ROOT)
            lines = out.setdefault(path.as_posix(), {})
        elif line.startswith("DA:"):
            number, count = line[3:].split(",")[:2]
            lines[int(number)] = lines.get(int(number), 0) + int(count)
    return out


def added(base: str) -> dict[str, set[int]]:
    """The lines the working tree adds to Rust files under crates/ since
    `base`, by path."""
    out: dict[str, set[int]] = {}
    path = None
    for line in git("diff", "-U0", "--no-color", "--no-ext-diff", base, "--", "crates").splitlines():
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif line.startswith("@@") and path and path.endswith(".rs"):
            m = re.search(r"\+(\d+)(?:,(\d+))?", line)
            start, count = int(m.group(1)), int(m.group(2) if m.group(2) is not None else 1)
            out.setdefault(path, set()).update(range(start, start + count))
    return out


def main() -> None:
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__.split("\n\n")[1])
    lcov, ref = Path(sys.argv[1]), sys.argv[2] if len(sys.argv) == 3 else "origin/main"
    base = git("merge-base", "HEAD", ref).strip()
    covered = hits(lcov)
    instrumented = missed = 0
    for path, lines in sorted(added(base).items()):
        counts = covered.get(path, {})
        run = [n for n in sorted(lines) if n in counts]
        not_run = [n for n in run if counts[n] == 0]
        instrumented += len(run)
        missed += len(not_run)
        for n in not_run:
            print(f"{path}:{n}: added, and no test runs it")
    print(f"patch coverage: {instrumented - missed} of the {instrumented} lines added since "
          f"{ref} ({base[:7]}) that llvm-cov instruments are run by a test")
    if missed:
        sys.exit(1)


if __name__ == "__main__":
    main()
