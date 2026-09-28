"""Staged learning replayed on τ²-bench agents (issue #35).

    python3 scripts/staged_study.py --tau2 DIR --glm5 DIR [--stretto BIN]
        [--orders 10] [--batch 40] [--jobs 4] [--out FILE.json]

**In order.** For each domain, the committed flow C0 is learned from one
agent's episodes on the training tasks (`stretto learn --habit-only
--results`). A staged learner (`stretto stage --results`) starts from the
same episodes, then takes the agent's test episodes in a random order,
`--batch` at a time. Before it learns from a batch, each of its episodes is
scored by the committed flow and by the staged flow as it was, as each is
served (`reach`). Three ways to commit, compared on the test episodes:

- **never:** C0 serves every test episode;
- **every batch:** the staged flow is committed after each batch, and so
  serves the next. The staged flow a batch was scored by is that flow, so
  the same run's ledger holds its scores;
- **when it dominates:** after each batch, `stretto flow-commit` if, on
  that batch, the staged flow's lookups were used at least as often as the
  committed flow's with no more detours, one of the two strictly;
- **when it nets more:** after each batch, `stretto flow-commit` if, on
  that batch, the staged flow's used lookups less its detours were more
  than the committed flow's.

The agents are GLM-5 (`--glm5`, its results files) in retail and airline.

**A splice.** Claude 3.7 Sonnet's retail test episodes, then GPT-4.1-mini's,
with C0 learned from Claude 3.7's training episodes: never committing,
committing every batch, and committing every batch of a staged flow that
forgets (`--half-life 100`). `stretto drift` watches the same order.

A lookup counts as used when the agent made it in a later LLM turn, which
spares the agent that call, and as a detour when it never did (`stretto
promote`'s scoring). Prints Markdown tables and writes every run to
`--out`.
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


def split(tau2: Path, domain: str) -> tuple[set[str], set[str]]:
    s = json.loads((tau2 / f"data/tau2/domains/{domain}/split_tasks.json").read_text())
    return set(s["train"]), set(s["test"])


def run(stretto: str, *args: str) -> str:
    done = subprocess.run([stretto, *args], capture_output=True, text=True)
    if done.returncode != 0:
        raise SystemExit(f"stretto {' '.join(args)}: {done.stderr}")
    return done.stdout


def write(results: dict, sims: list, path: Path) -> Path:
    path.write_text(json.dumps({**results, "simulations": sims}))
    return path


def totals(ledger: list[dict], ids: set[str], side: str) -> dict:
    """The lookups of one side of the ledger over the sessions `ids`."""
    out = {"lookups": 0, "used": 0, "served": 0}
    for e in ledger:
        if e["session"] not in ids or not e.get(side):
            continue
        for c in e[side]["sites"].values():
            for k in out:
                out[k] += c[k]
    out["detours"] = out["lookups"] - out["used"] - out["served"]
    return out


def better(cmp: dict, rule: str) -> bool:
    """Whether the staged flow did better on the compared sessions, by
    `rule`: `dominates` (used at least as often, no more detours, one of
    them strictly) or `nets` (more used lookups less detours)."""
    c, s = cmp["total"]["committed"], cmp["total"]["staged"]
    cd = c["lookups"] - c["used"] - c["served"]
    sd = s["lookups"] - s["used"] - s["served"]
    if rule == "dominates":
        return s["used"] >= c["used"] and sd <= cd and (s["used"] > c["used"] or sd < cd)
    return s["used"] - sd > c["used"] - cd


def stream_run(a, tmp: Path, domain: str, name: str, results: dict, prefix: list,
               tests: list[list], c0: Path, seed: int, commit, half_life=None) -> dict:
    """One staged learner over `prefix` then each of `tests`' episodes in
    batches, committing by the rule `commit` if one is given; returns its
    ledger and its commits."""
    work = tmp / f"{name}-{seed}-{commit}-{half_life}"
    work.mkdir()
    flow = work / f"{domain}.flow.json"
    shutil.copy(c0, flow)
    stream = list(prefix)
    base = ["stage", "--flow", str(flow), "--tau2", str(a.tau2), "--decider", "reach",
            "--window", str(a.batch), "--out", str(work / "report.md")]
    if half_life:
        base += ["--half-life", str(half_life)]
    batches = [b for t in tests for b in (t[i:i + a.batch] for i in range(0, len(t), a.batch))]
    commits = []
    for k, batch in enumerate([[]] + batches):
        stream += batch
        f = write(results, stream, work / "stream.json")
        run(a.stretto, *base, "--results", str(f), "--json", str(work / "cmp.json"))
        if commit and k > 0 and better(json.loads((work / "cmp.json").read_text()), commit):
            run(a.stretto, "flow-commit", "--flow", str(flow))
            commits.append(k)
    ledger = json.loads((work / f"{domain}.stage.json").read_text())["ledger"]
    shutil.rmtree(work)
    return {"ledger": ledger, "commits": commits}


def in_order(a, tmp: Path, domain: str, results: dict) -> list[dict]:
    train, test = split(a.tau2, domain)
    sims = results["simulations"]
    prefix = [s for s in sims if s["task_id"] in train]
    c0 = tmp / f"glm5-{domain}-c0.flow.json"
    f = write(results, sims, tmp / f"glm5-{domain}.json")
    run(a.stretto, "learn", "--domain", domain, "--habit-only", "--tau2", str(a.tau2),
        "--results", str(f), "--out", str(c0))

    def one(seed: int) -> dict:
        tests = [s for s in sims if s["task_id"] in test]
        random.Random(seed).shuffle(tests)
        ids = {s["id"] for s in tests}
        name = f"glm5-{domain}"
        none = stream_run(a, tmp, domain, name, results, prefix, [tests], c0, seed, None)
        dom = stream_run(a, tmp, domain, name, results, prefix, [tests], c0, seed, "dominates")
        nets = stream_run(a, tmp, domain, name, results, prefix, [tests], c0, seed, "nets")
        return {
            "part": "in order", "domain": domain, "agent": "GLM-5", "order": seed,
            "episodes": len(ids), "batches": -(-len(ids) // a.batch),
            "never": totals(none["ledger"], ids, "committed"),
            "every batch": totals(none["ledger"], ids, "staged"),
            "when it dominates": totals(dom["ledger"], ids, "committed"),
            "when it nets more": totals(nets["ledger"], ids, "committed"),
            "commits": {"dominates": dom["commits"], "nets": nets["commits"]},
        }

    with ThreadPoolExecutor(a.jobs) as pool:
        return list(pool.map(one, range(a.orders)))


def splice(a, tmp: Path) -> list[dict]:
    domain = "retail"
    train, test = split(a.tau2, domain)
    final = a.tau2 / "data/tau2/results/final"
    first = json.loads(next(final.glob(f"claude-3-7-sonnet*_{domain}_*.json")).read_text())
    then = json.loads(next(final.glob(f"gpt-4.1-mini*_{domain}_*.json")).read_text())
    c0 = tmp / "splice-c0.flow.json"
    f = write(first, first["simulations"], tmp / "splice-first.json")
    run(a.stretto, "learn", "--domain", domain, "--habit-only", "--tau2", str(a.tau2),
        "--results", str(f), "--out", str(c0))
    prefix = [s for s in first["simulations"] if s["task_id"] in train]

    def one(seed: int) -> dict:
        rng = random.Random(seed)
        a_test = [s for s in first["simulations"] if s["task_id"] in test]
        b_test = [dict(s, id=f"b-{s['id']}") for s in then["simulations"] if s["task_id"] in test]
        rng.shuffle(a_test)
        rng.shuffle(b_test)
        ids_a, ids_b = {s["id"] for s in a_test}, {s["id"] for s in b_test}
        none = stream_run(a, tmp, domain, "splice", first, prefix, [a_test, b_test], c0, seed, None)
        forget = stream_run(a, tmp, domain, "splice", first, prefix, [a_test, b_test], c0, seed,
                            None, half_life=100)
        # The drift alarm on the same order, under C0.
        fa = write(first, a_test, tmp / f"splice-{seed}-a.json")
        fb = write(first, b_test, tmp / f"splice-{seed}-b.json")
        out = tmp / f"splice-{seed}.drift.json"
        subprocess.run([a.stretto, "drift", "--flow", str(c0), "--results", str(fa),
                        "--results", str(fb), "--json", str(out), "--out", "/dev/null"],
                       capture_output=True, text=True)
        d = json.loads(out.read_text())
        found = [x for x in d["alarms"] if x["at"] >= len(a_test)]
        for p in (fa, fb, out):
            p.unlink()
        row = {"part": "splice", "domain": domain, "agent": "Claude 3.7 Sonnet, then GPT-4.1-mini",
               "order": seed, "alarm_after": found[0]["at"] - len(a_test) + 1 if found else None,
               "false_alarms": sum(1 for x in d["alarms"] if x["at"] < len(a_test))}
        for seg, ids in (("before", ids_a), ("after", ids_b)):
            row[seg] = {
                "never": totals(none["ledger"], ids, "committed"),
                "every batch": totals(none["ledger"], ids, "staged"),
                "every batch, forgetting": totals(forget["ledger"], ids, "staged"),
            }
        return row

    with ThreadPoolExecutor(a.jobs) as pool:
        return list(pool.map(one, range(a.orders)))


def cell(rows: list[dict], key) -> str:
    """Mean used · detours over the rows, with the spread of used."""
    vals = [key(r) for r in rows]
    used = [v["used"] for v in vals]
    det = [v["detours"] for v in vals]
    spread = f" ({min(used)}–{max(used)})" if len(set(used)) > 1 else ""
    return f"{statistics.mean(used):.1f}{spread} · {statistics.mean(det):.1f}"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--tau2", type=Path, required=True)
    ap.add_argument("--glm5", type=Path, required=True, help="the directory of GLM-5's results files")
    ap.add_argument("--stretto", default="target/release/stretto")
    ap.add_argument("--orders", type=int, default=10)
    ap.add_argument("--batch", type=int, default=20)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", type=Path)
    a = ap.parse_args()
    tmp = Path(tempfile.mkdtemp(prefix="staged-study-"))
    rows = []
    for domain in ("retail", "airline"):
        results = json.loads(next(a.glm5.glob(f"glm-5_*_{domain}_*.json")).read_text())
        rows += in_order(a, tmp, domain, results)
    rows += splice(a, tmp)
    shutil.rmtree(tmp)

    print("| Domain | Test episodes | Never | Every batch | When it dominates | When it nets more |")
    print("|---|---|---|---|---|---|")
    for domain in ("retail", "airline"):
        rs = [r for r in rows if r["part"] == "in order" and r["domain"] == domain]
        commits = {k: statistics.mean(len(r["commits"][k]) for r in rs) for k in ("dominates", "nets")}
        print(f"| {domain} | {rs[0]['episodes']} | {cell(rs, lambda r: r['never'])} | "
              f"{cell(rs, lambda r: r['every batch'])} | "
              f"{cell(rs, lambda r: r['when it dominates'])}, {commits['dominates']:.1f} commits | "
              f"{cell(rs, lambda r: r['when it nets more'])}, {commits['nets']:.1f} commits |")
    rs = [r for r in rows if r["part"] == "splice"]
    print()
    print("| Segment | Never | Every batch | Every batch, forgetting |")
    print("|---|---|---|---|")
    for seg in ("before", "after"):
        print(f"| {seg} | {cell(rs, lambda r: r[seg]['never'])} | {cell(rs, lambda r: r[seg]['every batch'])} | "
              f"{cell(rs, lambda r: r[seg]['every batch, forgetting'])} |")
    delays = [r["alarm_after"] for r in rs if r["alarm_after"] is not None]
    print(f"\nDrift alarm after the splice in {len(delays)} of {len(rs)} orders"
          + (f", a median of {statistics.median(delays):g} sessions in" if delays else "")
          + f"; false alarms before it in {sum(1 for r in rs if r['false_alarms'])}.")
    if a.out:
        a.out.write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()
