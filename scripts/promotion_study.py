"""Per-site promotion against the global threshold alone, by replay
(docs/results/promotion-2026-09-25.md; stretto #19).

    python3 scripts/promotion_study.py --tau2 ../tau2-bench --oracle-cache .oracle-cache \
        --out runs/promotion [--domains retail airline] [--deciders arbiter habit] [--bars a b c]

For each domain and decider, the flow is compiled as `flow-serve` compiles
it and replayed unpromoted on every test episode of GLM-5 (four trials).
Then, for each half of the test tasks (sorted by id, alternating), it is
promoted on the other half's episodes and replayed on this half's. Summed
over the halves, every episode is replayed once, by a flow promoted on
sessions of other tasks. Replays run through `pilot/check_flow.py`, in the
pilot's Python environment, with every answer from the replay cache (the
published bundles hold them all).
Finished steps are kept in --out and reused.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# min_used, min_lower, min_tasks; b is `stretto promote`'s default.
BARS = {"a": (0.5, 0.3, 3), "b": (0.7, 0.5, 3), "c": (0.8, 0.6, 5)}
KEYS = ("turns", "turns_saved", "flow_lookups", "calls_skipped", "detours", "episodes", "episodes_with_detour")


def run(cmd: list, log: Path, cwd: Path | None = None) -> None:
    with open(log, "w") as f:
        done = subprocess.run([str(c) for c in cmd], stdout=f, stderr=subprocess.STDOUT, cwd=cwd)
    if done.returncode:
        sys.exit(f"failed ({done.returncode}): {' '.join(map(str, cmd))}; see {log}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--tau2", type=Path, required=True, help="a τ²-bench checkout")
    ap.add_argument("--oracle-cache", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--stretto", type=Path, default=ROOT / "target/release/stretto")
    ap.add_argument("--results-dir", type=Path, default=ROOT / ".data/tau2-targets",
                    help="where scripts/fetch-leaderboard.sh put GLM-5's results")
    ap.add_argument("--python", default=sys.executable, help="the pilot's Python, with τ²-bench")
    ap.add_argument("--domains", nargs="*", default=["retail", "airline"])
    ap.add_argument("--deciders", nargs="*", default=["arbiter", "habit"])
    ap.add_argument("--bars", nargs="*", default=list(BARS))
    ap.add_argument("--flow-oracle", default="replay", choices=["replay", "jev"],
                    help="who answers the replays' questions the cache lacks (jev: TYPESAFE_API_KEY)")
    args = ap.parse_args()
    out, cache, tau2 = args.out.resolve(), args.oracle_cache.resolve(), args.tau2.resolve()
    (out / "runs").mkdir(parents=True, exist_ok=True)

    def compiled(domain: str) -> Path:
        flow = out / f"{domain}.flow.json"
        if not flow.exists():
            run([args.stretto, "compile", "--tau2", tau2, "--domain", domain, "--questions", "v2",
                 "--predicates", ROOT / "data/predicates-v2.json", "--oracle", "replay",
                 "--oracle-cache", cache, "--out", flow], out / f"compile-{domain}.log")
        return flow

    def results(domain: str) -> Path:
        return (args.results_dir / f"glm-5_enabled_{domain}_gpt-5.2_4trials.json").resolve()

    def promoted(domain: str, decider: str, bar: str, half: int, train: list[str]) -> Path:
        flow = out / f"{domain}-{decider}-{bar}-{half}.flow.json"
        if not flow.exists():
            used, lower, tasks = BARS[bar]
            run([args.stretto, "promote", "--flow", compiled(domain), "--results", results(domain),
                 "--task-ids", ",".join(train), "--oracle", "replay", "--oracle-cache", cache,
                 "--decider", decider, "--threshold", "0.3", "--min-used", used,
                 "--min-lower", lower, "--min-tasks", tasks, "--out", flow,
                 "--report", flow.with_suffix(".md")], flow.with_suffix(".log"))
        return flow

    def replay(domain: str, decider: str, flow: Path, tasks: list[str], name: str) -> dict:
        check = out / "runs" / name / "check.json"
        if not check.exists():
            run([args.python, "check_flow.py", "--domain", domain, "--tau2", tau2, "--flow", flow,
                 "--trials", "0", "1", "2", "3", "--results", results(domain), "--task-ids", *tasks,
                 "--oracle-cache", cache, "--flow-oracle", args.flow_oracle, "--flow-decider", decider,
                 "--flow-threshold", "0.3", "--in-process", "--jobs", "3", "--out", out / "runs" / name],
                out / f"{name}.log", cwd=ROOT / "pilot")
        return json.loads(check.read_text())

    summary = {"bars": BARS, "halves": {}, "results": {}}
    for domain in args.domains:
        split = json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())
        tasks = sorted((str(t) for t in split["test"]), key=lambda t: (len(t), t))
        a, b = tasks[0::2], tasks[1::2]
        summary["halves"][domain] = [a, b]
        for decider in args.deciders:
            base = replay(domain, decider, compiled(domain), a + b, f"{domain}-{decider}-none")
            rows = {"none": {k: base["total"][k] for k in KEYS}}
            for bar in args.bars:
                total = {k: 0 for k in KEYS}
                sites = []
                for half, (test, train) in enumerate(((a, b), (b, a))):
                    flow = promoted(domain, decider, bar, half, train)
                    got = replay(domain, decider, flow, test, f"{domain}-{decider}-{bar}-{half}")
                    for k in KEYS:
                        total[k] += got["total"][k]
                    sites.append(json.loads(flow.read_text())["promoted"]["sites"])
                total["sites"] = sites
                rows[bar] = total
            summary["results"][f"{domain} {decider}"] = rows
            for name, r in rows.items():
                share = r["turns_saved"] / max(r["turns"], 1)
                print(f"{domain:8} {decider:8} {name:5} saved {r['turns_saved']:4} ({share:.1%}) "
                      f"lookups {r['flow_lookups']:4} the agent's {r['calls_skipped']:4} "
                      f"detours {r['detours']:4} in {r['episodes_with_detour']} episodes", flush=True)
    (out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")


if __name__ == "__main__":
    main()
