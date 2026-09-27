#!/usr/bin/env python3
"""The working paper's figures, as SVG, from the round's published rows.

    paper_figures.py docs/results/reach-2026-09-26.json --out paper/

- `calibration.svg`: reliability diagrams, one panel per domain. Each bin of
  lookups the flows weighed (of at least 100) is placed at its mean score (x)
  and the share the agent used before its next write (y), for the habit's
  next-step score and for the reach score. The diagonal is perfect
  calibration.
- `thresholds.svg`: the token utility of the two speculators against the
  threshold, per agent and domain, priced at the domain's counted costs, with
  the domain's θ* = δ/(β+δ) marked.
- `learning.svg`: learning curves. For each domain, the share of LLM turns
  saved and the detours per episode against the sessions learned from (log
  scale), from the agent's own sessions and from other agents'. Each point is
  the mean over agents and seeds.
- `ceilings.svg` (with `--benchmarks`, the rows of
  docs/results/benchmarks-2026-09-27.json): the read-only ceiling per
  benchmark and domain, the turns a speculator binding from tool results
  could save, and on top what binding the user's words would add.

The SVGs carry their own light and dark palettes (`prefers-color-scheme`).
"""

import argparse
import collections
import json
import math
from pathlib import Path

# Validated as a categorical pair (light on #fcfcfb, dark on #1a1a19).
LIGHT = {"bg": "#fcfcfb", "ink": "#0b0b0b", "muted": "#52514e", "grid": "#e4e3df", "a": "#2a78d6", "b": "#eb6834", "c": "#1baf7a"}
DARK = {"bg": "#1a1a19", "ink": "#ffffff", "muted": "#c3c2b7", "grid": "#3a3936", "a": "#3987e5", "b": "#d95926", "c": "#199e70"}
DOMAINS = [("retail", "Retail"), ("airline", "Airline"), ("telecom", "Telecom"), ("solo", "Telecom, solo")]
MIN_BIN = 100  # lookups; smaller bins are not drawn


def nice(x: float) -> float:
    """The round step (1, 2, 2.5 or 5 times a power of ten) nearest above x."""
    e = 10 ** math.floor(math.log10(x))
    return next(m * e for m in (1, 2, 2.5, 5, 10) if m * e >= x)


def svg_open(w: int, h: int) -> list[str]:
    tokens = lambda p: "".join(f"--{k}:{v};" for k, v in p.items())  # noqa: E731
    return [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
        'font-family="system-ui, -apple-system, Segoe UI, sans-serif" font-size="12">',
        f"<style>svg{{{tokens(LIGHT)}}}@media (prefers-color-scheme: dark){{svg{{{tokens(DARK)}}}}}"
        ".t{fill:var(--ink)}.m{fill:var(--muted)}</style>",
        f'<rect width="{w}" height="{h}" fill="var(--bg)"/>',
    ]


def legend(out: list[str], x: float, y: float, items: list[tuple[str, str, str]]) -> None:
    for i, (key, label, dash) in enumerate(items):
        yy = y + 16 * i
        out.append(f'<line x1="{x}" x2="{x + 22}" y1="{yy}" y2="{yy}" stroke="var(--{key})" stroke-width="2"{dash}/>')
        out.append(f'<circle cx="{x + 11}" cy="{yy}" r="4" fill="var(--{key})" stroke="var(--bg)" stroke-width="2"/>')
        out.append(f'<text class="t" x="{x + 30}" y="{yy + 4}">{label}</text>')


def calibration(rows: dict, path: Path) -> None:
    pw, ph, gap, left, top, bottom = 200, 200, 46, 56, 76, 48
    w = left + 4 * pw + 3 * gap + 20
    h = top + ph + bottom
    out = svg_open(w, h)
    legend(out, left, 16, [("a", "habit: chance the agent calls it next", ""),
                           ("b", "reach: chance it calls it before its next write", "")])
    for i, (dom, title) in enumerate(DOMAINS):
        x0 = left + i * (pw + gap)
        sx = lambda v: x0 + pw * v  # noqa: E731
        sy = lambda v: top + ph * (1 - v)  # noqa: E731
        habit, reach = rows["habit"].get(dom), rows["reach"].get(dom)
        if not habit or not reach:
            continue
        for v in (0, 0.25, 0.5, 0.75, 1):
            out.append(f'<line x1="{sx(0)}" x2="{sx(1)}" y1="{sy(v):.1f}" y2="{sy(v):.1f}" stroke="var(--grid)"/>')
            out.append(f'<text class="m" x="{sx(v):.1f}" y="{top + ph + 16}" text-anchor="middle">{v:g}</text>')
            if i == 0:
                out.append(f'<text class="m" x="{x0 - 8}" y="{sy(v) + 4:.1f}" text-anchor="end">{v:g}</text>')
        out.append(f'<line x1="{sx(0)}" y1="{sy(0)}" x2="{sx(1)}" y2="{sy(1)}" stroke="var(--muted)" stroke-dasharray="3 3"/>')
        out.append(f'<text class="t" x="{x0}" y="{top - 24}" font-weight="600">{title}</text>')
        out.append(f'<text class="m" x="{x0}" y="{top - 8}">ECE {habit["ece"]:.3f} → {reach["ece"]:.3f}</text>')
        for key, cal in (("a", habit), ("b", reach)):
            pts = [(b["score"], b["used"]) for b in cal["reliability"] if b["lookups"] >= MIN_BIN]
            line = " ".join(f"{sx(a):.1f},{sy(b):.1f}" for a, b in pts)
            out.append(f'<polyline points="{line}" fill="none" stroke="var(--{key})" stroke-width="2"/>')
            for a, b in pts:
                out.append(f'<circle cx="{sx(a):.1f}" cy="{sy(b):.1f}" r="4" fill="var(--{key})" stroke="var(--bg)" stroke-width="2"/>')
    out.append(f'<text class="m" x="{left + (w - left) / 2:.0f}" y="{h - 8}" text-anchor="middle">score of the lookup (bin mean)</text>')
    out.append(f'<text class="m" transform="translate(14 {top + ph / 2}) rotate(-90)" text-anchor="middle">share used before the next write</text>')
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


def thresholds(sweep: list[dict], costs: dict, path: Path) -> None:
    names = {"glm-5_enabled": "GLM-5", "claude-sonnet-4-5_enabled": "Sonnet 4.5", "gpt-5.2_none": "GPT-5.2 (none)"}
    by = collections.defaultdict(dict)  # (domain, agent) -> decider -> {theta: utility}
    for r in sweep:
        b, d = costs[r["domain"]]
        by[(r["domain"], r["agent"])].setdefault(r["decider"], {})[r["threshold"]] = (b * r["turns_saved"] - d * r["detours"]) / 1e6
    order = {"retail": 0, "airline": 1, "telecom": 2}
    keys = sorted(by, key=lambda k: (order[k[0]], k[1]))
    cols = 3
    pw, ph, gx, gy, left, top, bottom = 200, 130, 56, 70, 64, 64, 44
    rows_n = math.ceil(len(keys) / cols)
    w = left + cols * pw + (cols - 1) * gx + 20
    h = top + rows_n * ph + (rows_n - 1) * gy + bottom
    out = svg_open(w, h)
    legend(out, left, 16, [("a", "next step", ""), ("b", "use before write", "")])
    for i, key in enumerate(keys):
        dom, agent = key
        x0 = left + (i % cols) * (pw + gx)
        y0 = top + (i // cols) * (ph + gy)
        vals = [u for dec in by[key].values() for u in dec.values()]
        step = nice(max(max(vals), 0.01) / 3)
        ymax = step * math.ceil(max(vals) / step) if max(vals) > 0 else 1
        ymin = min(0.0, step * math.floor(min(vals) / step))
        sx = lambda t: x0 + pw * (t - 0.1) / 0.7  # noqa: E731
        sy = lambda v: y0 + ph * (1 - (v - ymin) / (ymax - ymin))  # noqa: E731
        short = next((n for k, n in names.items() if agent.startswith(k)), agent)
        b, d = costs[dom]
        star = d / (b + d)
        out.append(f'<text class="t" x="{x0}" y="{y0 - 10}" font-weight="600">{short}, {dom}</text>')
        v = ymin
        while v <= ymax + 1e-9:
            out.append(f'<line x1="{x0}" x2="{x0 + pw}" y1="{sy(v):.1f}" y2="{sy(v):.1f}" stroke="var(--grid)"/>')
            out.append(f'<text class="m" x="{x0 - 6}" y="{sy(v) + 4:.1f}" text-anchor="end">{v:g}</text>')
            v = round(v + step, 6)
        for t in (0.1, 0.3, 0.5, 0.8):
            out.append(f'<text class="m" x="{sx(t):.1f}" y="{y0 + ph + 16}" text-anchor="middle">{t:g}</text>')
        if 0.1 <= star <= 0.8:
            out.append(f'<line x1="{sx(star):.1f}" x2="{sx(star):.1f}" y1="{y0}" y2="{y0 + ph}" stroke="var(--muted)" stroke-dasharray="3 3"/>')
            out.append(f'<text class="m" x="{sx(star) + 4:.1f}" y="{y0 + ph - 6}">θ* {star:.2f}</text>')
        for kk, dec in (("a", "habit"), ("b", "reach")):
            pts = sorted(by[key].get(dec, {}).items())
            if not pts:
                continue
            xy = [(sx(t), sy(u)) for t, u in pts]
            out.append(f'<polyline points="{" ".join(f"{a:.1f},{c:.1f}" for a, c in xy)}" fill="none" stroke="var(--{kk})" stroke-width="2"/>')
            for a, c in xy:
                out.append(f'<circle cx="{a:.1f}" cy="{c:.1f}" r="3.5" fill="var(--{kk})" stroke="var(--bg)" stroke-width="1.5"/>')
    out.append(f'<text class="m" x="{left + (w - left) / 2:.0f}" y="{h - 8}" text-anchor="middle">threshold θ</text>')
    out.append(f'<text class="m" transform="translate(16 {top + (h - top - bottom) / 2:.0f}) rotate(-90)" text-anchor="middle">utility, millions of input tokens</text>')
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


def learning(rows: list[dict], turns: dict, path: Path) -> None:
    by = collections.defaultdict(lambda: collections.defaultdict(list))  # (domain, protocol) -> n -> [(saved, det/ep)]
    for r in rows:
        # n = 0 learns nothing and saves nothing, so its row carries no turns.
        t = r.get("turns") or turns.get((r["domain"], r["agent"])) or (1 if r["turns_saved"] == 0 else None)
        if not t:
            continue
        by[(r["domain"], r["protocol"])][r["n"]].append((100 * r["turns_saved"] / t, r["detours"] / r.get("episodes", 160)))
    doms = [d for d, _ in DOMAINS if any(k[0] == d for k in by)]
    series = [("a", "own", "learned from the agent's own sessions", ""),
              ("b", "pool", "learned from four other agents' sessions", ' stroke-dasharray="5 4"'),
              ("c", "sample+own", "from 100 of the other agents' sessions, then the agent's own", ' stroke-dasharray="1.5 3"')]
    series = [x for x in series if any(k[1] == x[1] for k in by)]
    pw, ph, gap, left, bottom, mid = 220, 150, 40, 60, 44, 44
    top = 26 + 16 * len(series)
    w = left + len(doms) * pw + (len(doms) - 1) * gap + 20
    h = top + 2 * ph + mid + bottom
    out = svg_open(w, h)
    legend(out, left, 16, [(key, label, dash) for key, _, label, dash in series])
    nmax = max((n for v in by.values() for n in v), default=1000)
    top_s = max((sum(x[0] for x in p) / len(p) for v in by.values() for p in v.values()), default=30)
    top_d = max((sum(x[1] for x in p) / len(p) for v in by.values() for p in v.values()), default=1)
    step_s, step_d = nice(top_s / 4), nice(top_d / 3)
    ymax_s, ymax_d = step_s * math.ceil(top_s / step_s), step_d * math.ceil(top_d / step_d)
    ticks = [n for n in (0, 10, 30, 100, 300, 1000) if n <= nmax]
    for i, dom in enumerate(doms):
        x0 = left + i * (pw + gap)
        sx = lambda n: x0 + pw * math.log10(n + 1) / math.log10(nmax + 1)  # noqa: E731
        for row, (ymax, label, k) in enumerate(((ymax_s, "LLM turns saved (%)", 0), (ymax_d, "detours per episode", 1))):
            y0 = top + row * (ph + mid)
            sy = lambda v: y0 + ph * (1 - v / ymax)  # noqa: E731
            step = step_s if k == 0 else step_d
            for j in range(round(ymax / step) + 1):
                v = round(step * j, 6)
                out.append(f'<line x1="{x0}" x2="{x0 + pw}" y1="{sy(v):.1f}" y2="{sy(v):.1f}" stroke="var(--grid)"/>')
                if i == 0:
                    out.append(f'<text class="m" x="{x0 - 8}" y="{sy(v) + 4:.1f}" text-anchor="end">{v:g}</text>')
            for n in ticks:
                out.append(f'<text class="m" x="{sx(n):.1f}" y="{y0 + ph + 16}" text-anchor="middle">{n}</text>')
            if i == 0:
                out.append(f'<text class="m" transform="translate(16 {y0 + ph / 2}) rotate(-90)" text-anchor="middle">{label}</text>')
            if row == 0:
                out.append(f'<text class="t" x="{x0}" y="{top - 8}" font-weight="600">{dict(DOMAINS)[dom]}</text>')
            for key, proto, _, dash in series:
                pts = sorted(by.get((dom, proto), {}).items())
                if not pts:
                    continue
                xy = [(sx(n), sy(sum(x[k] for x in v) / len(v))) for n, v in pts]
                out.append(f'<polyline points="{" ".join(f"{a:.1f},{b:.1f}" for a, b in xy)}" fill="none" stroke="var(--{key})" stroke-width="2"{dash}/>')
                for a, b in xy:
                    out.append(f'<circle cx="{a:.1f}" cy="{b:.1f}" r="4" fill="var(--{key})" stroke="var(--bg)" stroke-width="2"/>')
    out.append(f'<text class="m" x="{left + (w - left) / 2:.0f}" y="{h - 8}" text-anchor="middle">sessions learned from (log scale)</text>')
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


BENCHMARKS = [("tau2-bench", "τ²-bench"), ("tau-bench", "τ-bench"), ("bfcl", "BFCL"), ("agentdojo", "AgentDojo"),
              ("workbench", "WorkBench")]
NAMES = {"customer_relationship_manager": "CRM", "project_management": "project mgmt", "multi_domain": "multi-domain",
         "bfcl": "multi-turn", "slack": "Slack"}


def ceilings(rows: list[dict], path: Path) -> None:
    by = collections.defaultdict(lambda: [0, 0, 0])  # (benchmark, domain) -> turns, tool state, with words
    for r in rows:
        k = (r["benchmark"], r.get("domain") or r["benchmark"])
        by[k][0] += r["turns"]
        by[k][1] += r["ceiling, tool state"]
        by[k][2] += r["ceiling, with words"]
    groups = []
    for bench, label in BENCHMARKS:
        doms = sorted((d for b, d in by if b == bench), key=lambda d: -by[(bench, d)][1] / by[(bench, d)][0])
        if doms:
            groups.append((label, [(d, *[100 * v / by[(bench, d)][0] for v in by[(bench, d)][1:]]) for d in doms]))
    left, right, bar, gap, head, top = 150, 60, 14, 6, 22, 44
    pw = 420
    h = top + sum(head + len(ds) * (bar + gap) for _, ds in groups) + 36
    w = left + pw + right
    out = svg_open(w, h)
    out.append(f'<rect x="{left}" y="10" width="14" height="10" fill="var(--a)"/>')
    out.append(f'<text class="t" x="{left + 20}" y="19">bound from tool results</text>')
    out.append(f'<rect x="{left + 190}" y="10" width="14" height="10" fill="var(--b)"/>')
    out.append(f'<text class="t" x="{left + 210}" y="19">also from the user\'s words</text>')
    xmax = 60
    sx = lambda v: left + pw * v / xmax  # noqa: E731
    body = top + sum(head + len(ds) * (bar + gap) for _, ds in groups)
    for v in range(0, xmax + 1, 10):
        out.append(f'<line x1="{sx(v):.1f}" x2="{sx(v):.1f}" y1="{top - 6}" y2="{body}" stroke="var(--grid)"/>')
        out.append(f'<text class="m" x="{sx(v):.1f}" y="{body + 16}" text-anchor="middle">{v}</text>')
    out.append(f'<text class="m" x="{left + pw / 2:.0f}" y="{h - 4}" text-anchor="middle">LLM turns a read-only speculator could save (%)</text>')
    y = top
    for label, doms in groups:
        out.append(f'<text class="t" x="8" y="{y + 14}" font-weight="600">{label}</text>')
        y += head
        for d, tool, words in doms:
            out.append(f'<text class="m" x="{left - 8}" y="{y + bar - 3}" text-anchor="end">{NAMES.get(d, d)}</text>')
            if tool > 0:
                out.append(f'<rect x="{left}" y="{y}" width="{max(sx(tool) - left - 1, 0.5):.1f}" height="{bar}" rx="2" fill="var(--a)"/>')
            if words - tool > 0.05:
                out.append(f'<rect x="{sx(tool) + 1:.1f}" y="{y}" width="{max(sx(words) - sx(tool) - 1, 0.5):.1f}" height="{bar}" rx="2" fill="var(--b)"/>')
            value = f"{tool:.1f} + {words - tool:.1f}" if words - tool > 0.05 else f"{tool:.1f}"
            out.append(f'<text class="t" x="{sx(words) + 6:.1f}" y="{y + bar - 3}">{value}</text>')
            y += bar + gap
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("rows", type=Path, help="docs/results/reach-2026-09-26.json")
    ap.add_argument("--out", type=Path, default=Path("paper"))
    ap.add_argument("--benchmarks", type=Path, help="docs/results/benchmarks-2026-09-27.json, for ceilings.svg")
    args = ap.parse_args()
    data = json.loads(args.rows.read_text())
    args.out.mkdir(parents=True, exist_ok=True)
    if args.benchmarks:
        ceilings(json.loads(args.benchmarks.read_text())["ceilings"], args.out / "ceilings.svg")
    calibration(data["calibration"], args.out / "calibration.svg")
    counted = data["costs"].get("counted", {})
    costs = {k.split("/")[0]: (v["beta"], v["delta"]) for k, v in counted.items() if k.endswith("/pooled")}
    if data.get("sweep") and costs:
        thresholds(data["sweep"], costs, args.out / "thresholds.svg")
    turns = {(r["domain"], r["agent"]): r["reach"]["total"]["turns"] for r in data["replays"] if "reach" in r}
    if data.get("learning_curves"):
        learning(data["learning_curves"], turns, args.out / "learning.svg")
    print("wrote", ", ".join(p.name for p in sorted(args.out.glob("*.svg"))))


if __name__ == "__main__":
    main()
