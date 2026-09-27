#!/usr/bin/env python3
"""What any read-only speculator could save: the ceiling a flow is bounded by.

Each LLM turn of τ²-bench's recorded test episodes is classed by what decided
it (`anatomy.py`'s walk): a reply to the customer, or calls, triggered by the
customer's last message or by a tool's result. A turn of calls can be saved
by a speculator that only reads when every call in it is a read and a
lookup made earlier could have answered each: after some tool response since
the last write (so the result is still current), with every argument already
in the state. Two ceilings follow:

- *tool state*: every argument was in an earlier result (`copy`, as
  TraceCompiler classes arguments), or the call takes none;
- *with the customer's words*: an argument may also be a value the customer
  wrote, which a speculator that reads language could bind.

A flow binds from results only, so its savings are bounded by the first. A
third count, *with constants*, also admits an argument that takes one value
in every call of its tool the agent made, at least five (`anatomy.py`'s
constant, such as a page size an agent always passes), which a binding that
learned constants could pass. A fourth, *by shape*, admits instead a value
of the customer's that is the only one of its shape in what they wrote so far
(the one email, id, number or date), which a speculator could take from the
request with no model; names and free text need one. A fifth, *by model*,
admits a value of the customer's that a model, shown what they wrote so far
and the call (its tool and the argument's name), picks among the spans of
what they wrote (`spans`): `--questions` writes each such argument's question
as JSON lines, `model_questions.py` asks them, and `--model-answers` reads
the picks back.

    ceiling.py RESULTS.json... [--tau2 DIR] [--json OUT] [--questions OUT] [--model-answers FILE]

Only the test tasks' episodes, trials 0-3, are counted, as `pilot/check_flow.py`
replays them.
"""

import argparse
import collections
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import anatomy  # noqa: E402

COPY = {"copy, unique", "copy, select"}
TOKEN = re.compile(r"[\w.@+#-]+")


def shape(v: str):
    """A value's shape, if it has one a pattern can find: an email, a date, a number or an id (letters and digits)."""
    if re.fullmatch(r"[\w.+-]+@[\w-]+(\.[\w-]+)+", v):
        return "email"
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", v):
        return "date"
    if re.fullmatch(r"\d+(\.\d+)?", v):
        return "number"
    if re.fullmatch(r"[#\w-]+", v) and re.search(r"\d", v) and re.search(r"[a-z]", v):
        return "id"
    return None


def by_shape(value: str, said: str) -> bool:
    """Whether `value` is the only value of its shape in what the customer wrote (`said`, lowercased)."""
    kind = shape(value)
    if kind is None:
        return False
    found = {t.strip(".,;:!?") for t in TOKEN.findall(said)}
    return {t for t in found if shape(t) == kind} == {value}


STOP = set("""a an the and or but if of to in on at by for from with about as into onto than then so
is are was were be been am do does did have has had i me my we our you your it its this that these those he she
they them his her their what which who whom when where why how all any some no not please can could would should
will just also there here up out over after before again once too very""".split())
SPAN = re.compile(r"[\w.@+#/:-]+")
MAX_OPTIONS = 255  # a Choice question's limit


def words_of(text: str) -> list:
    """The words of a text, trimmed of the punctuation around them."""
    return [w for w in (t.strip(".,;:!?-") for t in SPAN.findall(text)) if w]


def spans(said: str, longest: int = 6) -> list:
    """What a model may pick as a value the customer wrote: every run of one to `longest` words of `said` that
    neither starts nor ends with a common word, the latest first, at most MAX_OPTIONS."""
    words = words_of(said)
    ends = {}
    for n in range(1, longest + 1):
        for i in range(len(words) - n + 1):
            run = words[i:i + n]
            if run[0] in STOP or run[-1] in STOP:
                continue
            ends[" ".join(run)] = max(ends.get(" ".join(run), -1), i + n)
    return sorted(ends, key=lambda s: (-ends[s], len(s)))[:MAX_OPTIONS]


def question_key(said: str, tool: str, arg: str) -> str:
    return hashlib.sha256(json.dumps([said, tool, arg]).encode()).hexdigest()[:24]


def domain_of(sims):
    tools = {c["name"] for s in sims for m in s["messages"] for c in m.get("tool_calls") or []}
    for d in ("telecom", "airline", "retail"):
        if tools & anatomy.READ[d] - anatomy.READ["retail" if d != "retail" else "airline"]:
            return d
    return "retail"


def constants(sims, domain, writes) -> set:
    """The (tool, argument) pairs whose every call over the episodes, at least five, took one value."""
    values, counts = collections.defaultdict(set), collections.Counter()
    for s in sims:
        for t in anatomy.walk(s, domain, writes)[0]:
            for c in t.get("calls") or []:
                for a in c["args"]:
                    values[(c["tool"], a["arg"])].add(json.dumps(a["value"], sort_keys=True, default=str))
                    counts[(c["tool"], a["arg"])] += 1
    return {k for k, v in values.items() if len(v) == 1 and counts[k] >= 5}


def classify(sim, domain, writes, fixed=frozenset(), picks=None, asked=None):
    """Per LLM turn: (kind, trigger, speculable from the tool state, from the
    tool state and the customer's words, from the tool state and the `fixed`
    constants, by shape, by model). Kinds: reply, reads, writes. `picks` maps
    a question's key to the model's pick; `asked`, if given, collects the
    questions of the customer's values in read turns."""
    turns, _, _ = anatomy.walk(sim, domain, writes)
    out = []
    # A lookup is current from the first tool response after the last write.
    responded_since_write = False
    for t in turns:
        if t["kind"] == "reply":
            out.append(("reply", t["trigger"], False, False, False, False, False))
            continue
        calls = t["calls"]
        reads = all(c["kind"] == "read" for c in calls)
        tool_ok = reads and responded_since_write and all(a["class"] in COPY for c in calls for a in c["args"])
        words_ok = reads and responded_since_write and all(a["class"] in COPY | {"customer"} for c in calls for a in c["args"])
        const_ok = reads and responded_since_write and all(
            a["class"] in COPY or (a["class"] == "generated" and (c["tool"], a["arg"]) in fixed) for c in calls for a in c["args"])
        shape_ok = reads and responded_since_write and all(
            a["class"] in COPY or (a["class"] == "customer" and by_shape(a["value"], c.get("said", ""))) for c in calls for a in c["args"])
        picked = lambda c, a: (picks or {}).get(question_key(c.get("said", ""), c["tool"], a["arg"])) == \
            " ".join(words_of(a["value"]))  # noqa: E731
        model_ok = reads and responded_since_write and all(
            a["class"] in COPY or (a["class"] == "customer" and picked(c, a)) for c in calls for a in c["args"])
        if asked is not None and reads:
            for c in calls:
                for a in c["args"]:
                    if a["class"] == "customer":
                        key = question_key(c.get("said", ""), c["tool"], a["arg"])
                        asked.setdefault(key, {"key": key, "said": c.get("said", ""), "tool": c["tool"], "arg": a["arg"],
                                               "value": " ".join(words_of(a["value"])), "spans": spans(c.get("said", ""))})
        out.append(("reads" if reads else "writes", t["trigger"], tool_ok, words_ok, const_ok, shape_ok, model_ok))
        # This turn's results come back before the next turn; a write in it
        # makes earlier lookups stale, and its own response is fresh.
        responded_since_write = True
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", nargs="+")
    ap.add_argument("--tau2", default="../tau2-bench")
    ap.add_argument("--json", help="write the table here")
    ap.add_argument("--questions", help="write the model's questions here (JSON lines)")
    ap.add_argument("--model-answers", help="the model's picks (JSON: question key -> the span it picked)")
    args = ap.parse_args()
    picks = json.loads(Path(args.model_answers).read_text()) if args.model_answers else None
    asked = {} if args.questions else None
    report = {}
    for path in args.results:
        data = json.loads(Path(path).read_text())
        sims = data["simulations"]
        named = data.get("info", {}).get("environment_info", {}).get("domain_name")
        if named not in ("retail", "airline", "telecom") and (Path(args.tau2) / f"src/tau2/domains/{named}/tools.py").exists():
            # Another benchmark in τ²-bench's layout: its tools say which read.
            domain = named
            anatomy.READ.setdefault(domain, anatomy.stub_tools(args.tau2, domain)[0])
        else:
            domain = domain_of(sims)
        test = set(json.loads((Path(args.tau2) / f"data/tau2/domains/{domain}/split_tasks.json").read_text())["test"])
        sims = [s for s in sims if str(s["task_id"]) in test and s.get("trial", 0) in (0, 1, 2, 3)]
        writes = {c["name"] for s in sims for m in s["messages"] for c in m.get("tool_calls") or []
                  if c["name"] not in anatomy.READ[domain] | anatomy.PURE | anatomy.HANDOFF}
        tally = collections.Counter()
        per_episode = {}
        fixed = constants(sims, domain, writes)
        for s in sims:
            rows = classify(s, domain, writes, fixed, picks, asked)
            for kind, trigger, tool_ok, words_ok, const_ok, shape_ok, model_ok in rows:
                tally["turns"] += 1
                tally[f"{kind}, after {trigger}"] += 1
                tally["ceiling, tool state"] += tool_ok
                tally["ceiling, with words"] += words_ok
                tally["ceiling, with constants"] += const_ok
                tally["ceiling, words by shape"] += shape_ok
                if picks is not None:
                    tally["ceiling, words by model"] += model_ok
            # Named as the replays name episodes.
            per_episode[f"task-{s['task_id']}-{s.get('trial', 0)}"] = {
                "turns": len(rows), "ceiling": sum(r[2] for r in rows), "ceiling_words": sum(r[3] for r in rows),
            }
        name = Path(path).name.removesuffix(".json")
        report[name] = {"domain": domain, "episodes": len(sims), **tally, "per_episode": per_episode}
        n = tally["turns"]
        print(f"{domain:8s} {name[:44]:44s} turns {n:5d}  ceiling: tool state {tally['ceiling, tool state']:5d} "
              f"({tally['ceiling, tool state'] / n:.1%}), with words {tally['ceiling, with words']:5d} "
              f"({tally['ceiling, with words'] / n:.1%})")
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=1) + "\n")
    if args.questions:
        Path(args.questions).write_text("".join(json.dumps(q) + "\n" for q in asked.values()))


if __name__ == "__main__":
    main()
