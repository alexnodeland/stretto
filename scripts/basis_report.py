#!/usr/bin/env python3
"""The live episodes scored as τ²-bench does, from `pilot/rescore.py --scoring basis` output.

Each archive's episodes are rescored once with every check in their tasks'
reward basis (`--scoring basis --judge claude:MODEL`). The episodes whose
tasks have natural-language assertions to judge can be judged again, to see
how steady the judge is (`--extra`, one file per judging). An episode's
reward is then the majority of its judgings (ties go to the database check's
verdict).

    basis_report.py --archive ARCHIVE_ROWS.json [...] --extra JUDGING.json [...] \\
        [--rows OUT.json]

Prints, per archive and arm, the episodes, how many pass the database check,
how many pass every check in the basis, and the tasks where the two differ;
then how often the judgings agreed. `--rows` writes a row per episode: its
path in its archive, the recorded reward (`database`: the database check,
or every check for telecom's solo mode, which records that), each judging's
reward, the majority (`basis`), and each assertion with every judging's
verdict and, where one found it unmet, that judging's reason.
"""

import argparse
import collections
import json
from pathlib import Path


def key(episode: str) -> str:
    """An episode's path from its archive's top folder (`*-episodes`) on, the
    same wherever the archive was unpacked."""
    parts = Path(episode).parts
    tops = [i for i, p in enumerate(parts) if p.endswith("-episodes")]
    return "/".join(parts[tops[-1]:]) if tops else "/".join(p for p in parts if p)


def passed(reward: float) -> bool:
    return reward >= 1 - 1e-9


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--archive", type=Path, nargs="+", required=True, help="each archive's full rescore")
    ap.add_argument("--extra", type=Path, nargs="*", default=[], help="further judgings of the judged episodes")
    ap.add_argument("--rows", type=Path, help="write a row per episode here")
    args = ap.parse_args()

    episodes = {}
    for f in args.archive:
        for r in json.loads(f.read_text()):
            k = key(r["episode"])
            episodes[k] = {
                "episode": k, "domain": r["domain"], "task_id": r["task_id"], "arm": r["arm"],
                "database": r["recorded"], "judgings": [r["reward"]], "judge": r.get("judge"),
                "assertions": [
                    {"assertion": a["assertion"], "met": [a["met"]]}
                    | ({} if a["met"] else {"unmet_because": a["justification"]})
                    for a in r.get("nl_assertions") or []
                ],
            }
    for f in args.extra:
        for r in json.loads(f.read_text()):
            e = episodes[key(r["episode"])]
            e["judgings"].append(r["reward"])
            got = r.get("nl_assertions") or []
            if [a["assertion"] for a in got] != [a["assertion"] for a in e["assertions"]]:
                raise SystemExit(f"{f}: {e['episode']} has other assertions than its first judging")
            for a, b in zip(e["assertions"], got):
                a["met"].append(b["met"])
                if not b["met"]:
                    a.setdefault("unmet_because", b["justification"])
    for e in episodes.values():
        yes = sum(passed(x) for x in e["judgings"])
        no = len(e["judgings"]) - yes
        e["basis"] = e["database"] if yes == no else (1.0 if yes > no else 0.0)
        if not e["assertions"]:
            e.pop("judge")

    by = collections.defaultdict(list)
    for e in episodes.values():
        archive, *rest = e["episode"].split("/")
        by[(archive, e["domain"], "/".join(rest[:-1]))].append(e)
    print("| Archive | Domain | Arm | Episodes | Database check | Every check (majority of judgings) | Differ |")
    print("|---|---|---|---|---|---|---|")
    for (archive, domain, arm), es in sorted(by.items()):
        db = sum(passed(e["database"]) for e in es)
        basis = sum(passed(e["basis"]) for e in es)
        differ = sorted({e["task_id"] for e in es if passed(e["database"]) != passed(e["basis"])}, key=int)
        print(f"| {archive.removesuffix('-episodes')} | {domain} | {arm} | {len(es)} | {db} | {basis} | "
              f"{', '.join(differ) or '–'} |")

    judged = [e for e in episodes.values() if e["assertions"]]
    again = [e for e in judged if len(e["judgings"]) > 1]
    steady = sum(len(set(e["judgings"])) == 1 for e in again)
    verdicts = [a["met"] for e in again for a in e["assertions"]]
    agree = sum(len(set(m)) == 1 for m in verdicts)
    changed = sum(passed(e["database"]) != passed(e["basis"]) for e in episodes.values())
    print(f"\n{len(episodes)} episodes; {len(judged)} with assertions to judge; {changed} change under every check.")
    if again:
        times = sorted({len(e["judgings"]) for e in again})
        print(f"Judged {'/'.join(map(str, times))} times: the same reward every time in {steady} of "
              f"{len(again)} episodes, and the same verdict on {agree} of {len(verdicts)} assertions.")
    if args.rows:
        rows = sorted(episodes.values(), key=lambda e: e["episode"])
        args.rows.write_text(json.dumps(rows, indent=1, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
