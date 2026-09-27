#!/usr/bin/env python3
"""Ask a model which of the user's words an agent passes: `ceiling.py`'s fifth count.

`ceiling.py --questions` writes, for each argument of a read whose value the
user wrote, what the user wrote so far, the call's tool and the argument's
name, and the spans of the user's words a model may pick from (`spans`), with
the agent's value, which is never shown. This asks the System-One model to
pick among the spans, as a speculator that reads the request would, through
`stretto ask` and its replay cache, and writes the picks for
`ceiling.py --model-answers`.

    model_questions.py QUESTIONS.jsonl... --out PICKS.json [--stretto BIN] [--oracle jev|replay]
        [--oracle-cache DIR] [--oracle-budget DOLLARS] [--oracle-concurrency N]

A question whose value is not among its spans counts as missed, as it would
for a speculator that picks from them. The report gives, per file, the
questions, the share whose value is among the spans, and the share the model
picked right.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

MODEL = "jev-latest"
STATE_CHARS = 4000  # the latest of what the user wrote, where the spans come from first


def request(q: dict) -> dict:
    """One Choice question: which span the agent passes as the argument."""
    return {
        "model": MODEL,
        "state": {"user": q["said"][-STATE_CHARS:].strip(), "call": {"tool": q["tool"], "argument": q["arg"]}},
        "questions": {
            "value": {
                "type": "choice",
                "instructions": (
                    f"An assistant helping the user is calling the tool `{q['tool']}`, and passes as its argument "
                    f"`{q['arg']}` a value the user wrote (the state's `user`): a name, an id, a date, a number, a "
                    f"title or a phrase, as the tool takes it. Which option is exactly that value, with no other words?"
                ),
                "criteria": {f"o{i}": span for i, span in enumerate(q["spans"])},
            }
        },
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("questions", nargs="+", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--stretto", default="target/release/stretto")
    ap.add_argument("--oracle", default="replay", choices=["jev", "replay"])
    ap.add_argument("--oracle-cache", default=".oracle-cache")
    ap.add_argument("--oracle-budget", default="1")
    ap.add_argument("--oracle-concurrency", default="8")
    args = ap.parse_args()
    by_file = {f: [json.loads(line) for line in f.read_text().splitlines() if line.strip()] for f in args.questions}
    asked = {q["key"]: q for qs in by_file.values() for q in qs if len(q["spans"]) >= 2}
    with tempfile.TemporaryDirectory() as tmp:
        reqs, outs = Path(tmp) / "requests.jsonl", Path(tmp) / "answers.jsonl"
        keys = sorted(asked)
        reqs.write_text("".join(json.dumps({"key": k, "request": request(asked[k])}) + "\n" for k in keys))
        subprocess.run([args.stretto, "ask", "--requests", str(reqs), "--out", str(outs), "--oracle", args.oracle,
                        "--oracle-cache", args.oracle_cache, "--oracle-budget", args.oracle_budget,
                        "--oracle-concurrency", args.oracle_concurrency], check=True)
        picks = {}
        # `stretto ask` answers in the order it was asked, each line keyed by its request's hash.
        for k, line in zip(keys, outs.read_text().splitlines()):
            answer = (json.loads(line).get("response") or {}).get("answers", {}).get("value")
            if answer:
                picks[k] = asked[k]["spans"][int(answer["choice"][1:])]
    # A question with one span has no choice to make.
    for k, q in ((q["key"], q) for qs in by_file.values() for q in qs if len(q["spans"]) == 1):
        picks[k] = q["spans"][0]
    args.out.write_text(json.dumps(picks, indent=0, sort_keys=True) + "\n")
    for f, qs in by_file.items():
        if not qs:
            continue
        inside = sum(q["value"] in q["spans"] for q in qs)
        right = sum(picks.get(q["key"]) == q["value"] for q in qs)
        print(f"{f.stem:40s} {len(qs):5d} questions, value among the spans {inside / len(qs):5.1%}, "
              f"picked right {right / len(qs):5.1%}", file=sys.stderr)


if __name__ == "__main__":
    main()
