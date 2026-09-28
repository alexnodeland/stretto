"""Drift alarms on τ²-bench agents spliced together (issue #18).

    python3 scripts/drift_splices.py --tau2 DIR [--stretto BIN] [--orders 20]
        [--domains retail airline] [--out FILE.json]

For each domain, and each agent A with results in the checkout, a flow of
the habit alone is learned from A's episodes on the training tasks
(`stretto learn --habit-only`). Then, on the test tasks, in `--orders`
random orders:

- A alone: A's test episodes, shuffled. Any alarm is a false one.
- A, then B, for every other agent B: A's test episodes, then B's, each
  shuffled. An alarm that sounds before the splice is a false one; the
  first to sound within two windows after it, with its change no earlier
  than two sessions before the splice, detects it, and the sessions from
  the splice to the alarm are its delay.

Each run is `stretto drift --json` with its default settings. The mean
surprise of A's and B's test episodes under A's flow says how far B moved.
Prints a Markdown table and writes every run to `--out`.
"""

import argparse
import json
import random
import shutil
import statistics
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def agents(tau2: Path, domain: str) -> dict[str, Path]:
    """Each agent's results file for `domain`, by model name."""
    out = {}
    for f in sorted((tau2 / "data/tau2/results/final").glob(f"*_{domain}_*.json")):
        out[f.name.split(f"_{domain}_")[0]] = f
    return out


def test_tasks(tau2: Path, domain: str) -> set[str]:
    split = json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())
    return set(split["test"])


def shuffled(results: dict, tasks: set[str], seed: int, path: Path) -> Path:
    """A copy of `results` with only the test episodes, in a random order."""
    sims = [s for s in results["simulations"] if s["task_id"] in tasks]
    random.Random(seed).shuffle(sims)
    path.write_text(json.dumps({**results, "simulations": sims}))
    return path


def drift(stretto: str, flow: Path, files: list[Path], out: Path) -> dict:
    args = [stretto, "drift", "--flow", str(flow), "--json", str(out), "--out", "/dev/null"]
    for f in files:
        args += ["--results", str(f)]
    done = subprocess.run(args, capture_output=True, text=True)
    if done.returncode not in (0, 1):
        raise SystemExit(f"{' '.join(args)}: {done.stderr}")
    return json.loads(out.read_text())


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--tau2", type=Path, required=True)
    ap.add_argument("--stretto", default="target/debug/stretto")
    ap.add_argument("--orders", type=int, default=20)
    ap.add_argument("--domains", nargs="+", default=["retail", "airline"])
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", type=Path)
    a = ap.parse_args()
    tmp = Path(tempfile.mkdtemp(prefix="drift-splices-"))
    rows = []
    for domain in a.domains:
        files = agents(a.tau2, domain)
        tasks = test_tasks(a.tau2, domain)
        loaded = {m: json.loads(f.read_text()) for m, f in files.items()}
        for ref, ref_file in files.items():
            flow = tmp / f"{domain}-{ref}.flow.json"
            subprocess.run(
                [a.stretto, "learn", "--domain", domain, "--habit-only", "--tau2", str(a.tau2),
                 "--results", str(ref_file), "--out", str(flow)],
                check=True, capture_output=True,
            )

            def run(job):
                other, seed = job
                first = shuffled(loaded[ref], tasks, seed, tmp / f"{domain}-{ref}-{other}-{seed}-a.json")
                inputs = [first]
                if other != ref:
                    inputs.append(shuffled(loaded[other], tasks, seed + 1000, tmp / f"{domain}-{ref}-{other}-{seed}-b.json"))
                out = tmp / f"{domain}-{ref}-{other}-{seed}.drift.json"
                d = drift(a.stretto, flow, inputs, out)
                # Each copy is megabytes: keep only the scores.
                for f in [*inputs, out]:
                    f.unlink()
                splice = sum(1 for s in loaded[ref]["simulations"] if s["task_id"] in tasks)
                if other == ref:
                    splice = len(d["sessions"])
                window = d["settings"]["window"]
                false = [x for x in d["alarms"] if x["at"] < splice]
                found = [x for x in d["alarms"]
                         if splice <= x["at"] < splice + 2 * window and x["change"] >= splice - 2]
                surprise = [s["surprise"] for s in d["sessions"] if s["surprise"] is not None]
                return {
                    "domain": domain, "flow_of": ref, "then": other, "order": seed,
                    "sessions": len(d["sessions"]), "splice": splice,
                    "false_alarms": len(false),
                    "detected": bool(found),
                    "delay": found[0]["at"] - splice + 1 if found else None,
                    "change_at": found[0]["change"] if found else None,
                    "surprise_before": statistics.mean(surprise[:splice]) if surprise[:splice] else None,
                    "surprise_after": statistics.mean(surprise[splice:]) if surprise[splice:] else None,
                }

            jobs = [(other, seed) for other in files for seed in range(a.orders)]
            with ThreadPoolExecutor(a.jobs) as pool:
                rows.extend(pool.map(run, jobs))
    print("| Domain | Flow of | Then | Surprise before → after | Runs with a false alarm | Splices found | Median delay |")
    print("|---|---|---|---|---|---|---|")
    groups = {}
    for r in rows:
        groups.setdefault((r["domain"], r["flow_of"], r["then"]), []).append(r)
    for (domain, ref, other), rs in groups.items():
        before = statistics.mean(r["surprise_before"] for r in rs if r["surprise_before"] is not None)
        after = [r["surprise_after"] for r in rs if r["surprise_after"] is not None]
        moved = f"{before:.2f} → {statistics.mean(after):.2f}" if after else f"{before:.2f}"
        false = sum(1 for r in rs if r["false_alarms"])
        if other == ref:
            print(f"| {domain} | {ref} | (alone) | {moved} | {false} of {len(rs)} | – | – |")
        else:
            found = [r for r in rs if r["detected"]]
            delay = statistics.median(r["delay"] for r in found) if found else None
            print(f"| {domain} | {ref} | {other} | {moved} | {false} of {len(rs)} | {len(found)} of {len(rs)} | "
                  f"{'–' if delay is None else f'{delay:g}'} |")
    if a.out:
        a.out.write_text(json.dumps(rows, indent=1) + "\n")
    shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
