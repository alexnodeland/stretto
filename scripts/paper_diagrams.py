#!/usr/bin/env python3
"""The working paper's diagrams, and the charts drawn from its own tables, as SVG.

    paper_diagrams.py --out paper/

- `lookup.svg`: how a lookup spares an LLM turn, and the event the
  speculator estimates (use before the next write, not the next step).
- `threshold.svg`: a read's expected value against its probability of use, at
  each domain's counted costs (Proposition 3, Appendix B).
- `turns.svg`: what decides each LLM turn, per domain and benchmark (Tables 1
  and 1b).
- `replay.svg`: the share of the read-only ceiling each speculator takes in
  replay (Table 3).
- `live.svg`: the change in LLM turns in every live comparison, with 95%
  intervals (Tables 5, 5b and 5c, and §4.4's smaller agent).
- `cascade.svg`: the compiled procedure and its hand-backs on solo telecom's
  held-out tasks (§4.5, Table 6).

Every number drawn is written here as the paper writes it, and the script
fails if paper/stretto.md does not hold it, so a figure cannot drift from the
text. The SVGs share paper_figures.py's palette, with its light and dark modes.
"""

import argparse
import re
from pathlib import Path

from paper_figures import DARK, LIGHT

PAPER = Path(__file__).resolve().parent.parent / "paper" / "stretto.md"

# Tints and neutrals for diagrams' fills, beside the categorical palette.
LIGHT_X = {"pa": "#dbe8f9", "pb": "#fde6da", "panel": "#f1f0ec", "g1": "#a3a29c", "g2": "#d6d5cf"}
DARK_X = {"pa": "#22385a", "pb": "#4a2a1b", "panel": "#262624", "g1": "#76756f", "g2": "#4b4a46"}
SANS = "system-ui, -apple-system, Segoe UI, sans-serif"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"


def svg_open(w: int, h: int) -> list[str]:
    tokens = lambda p: "".join(f"--{k}:{v};" for k, v in p.items())  # noqa: E731
    light, dark = {**LIGHT, **LIGHT_X}, {**DARK, **DARK_X}
    return [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
        f'font-family="{SANS}" font-size="12">',
        f"<style>svg{{{tokens(light)}}}@media (prefers-color-scheme: dark){{svg{{{tokens(dark)}}}}}"
        ".t{fill:var(--ink)}.m{fill:var(--muted)}.k{fill:var(--bg)}</style>",
        '<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" '
        'orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="var(--muted)"/></marker></defs>',
        f'<rect width="{w}" height="{h}" fill="var(--bg)"/>',
    ]


def text(out: list[str], x: float, y: float, s: str, cls: str = "t", anchor: str = "start",
         size: float | None = None, weight: str | None = None, mono: bool = False, italic: bool = False) -> None:
    attrs = [f'class="{cls}"', f'x="{x:.1f}"', f'y="{y:.1f}"']
    if anchor != "start":
        attrs.append(f'text-anchor="{anchor}"')
    if size:
        attrs.append(f'font-size="{size}"')
    if weight:
        attrs.append(f'font-weight="{weight}"')
    if mono:
        attrs.append(f'font-family="{MONO}"')
    if italic:
        attrs.append('font-style="italic"')
    s = s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    out.append(f"<text {' '.join(attrs)}>{s}</text>")


def width(s: str, size: float = 12, mono: bool = False) -> float:
    """A string's rough width in px, for laying out labels."""
    return len(s) * size * (0.61 if mono else 0.53)


def checked(numbers: list[str]) -> None:
    """Fail unless the paper writes every number a figure draws."""
    paper = PAPER.read_text().replace("**", "")
    missing = [n for n in numbers if n not in paper]
    if missing:
        raise SystemExit(f"paper_diagrams.py: paper/stretto.md does not hold {missing}")


def pct(s: str) -> float:
    return float(s.replace("−", "-").rstrip("%"))


# --- Figure: a lookup spares a turn; the event to estimate -----------------------------------

def lookup(path: Path) -> None:
    checked(["0.64", "94% of the time"])
    w, h = 872, 262
    out = svg_open(w, h)
    mono = 11.5

    # (a) Two lanes: the same episode without and with the speculator.
    text(out, 0, 16, "(a) A lookup spares an LLM turn", weight="700", size=13)
    lanes = [(0, "Without stretto"), (240, "With stretto")]
    row = 27

    def turn(x: float, y: float, n: int, call: str, sans: bool = False) -> None:
        out.append(f'<rect x="{x}" y="{y - 13}" width="50" height="19" rx="9.5" fill="var(--muted)"/>')
        text(out, x + 25, y + 1, f"turn {n}", cls="k", anchor="middle", size=11, weight="700")
        text(out, x + 58, y + 1, call, size=mono if not sans else 12, mono=not sans)

    def result(x: float, y: float, s: str) -> None:
        text(out, x + 58, y + 1, s, cls="m", size=mono, mono=True)

    for x, title in lanes:
        text(out, x, 44, title, weight="700")
    y0 = 70
    x = lanes[0][0]
    # The turn the lookup will spare, outlined.
    out.append(f'<rect x="{x - 4}" y="{y0 + 2 * row - 18}" width="236" height="{row + 22}" rx="6" fill="none" '
               'stroke="var(--b)" stroke-width="1.5" stroke-dasharray="5 3"/>')
    turn(x, y0, 1, "get_user_details(u)")
    result(x, y0 + row, "↳ orders: [#W12]")
    turn(x, y0 + 2 * row, 2, "get_order_details(#W12)")
    result(x, y0 + 3 * row, "↳ status: delivered")
    turn(x, y0 + 4 * row, 3, "reply to the user", sans=True)
    text(out, x, y0 + 5 * row + 8, "3 LLM turns", weight="700")
    lw = width("the turn a lookup spares", 10.5) + 8
    out.append(f'<rect x="{x + 228 - lw:.1f}" y="{y0 + 2 * row - 25}" width="{lw:.1f}" height="14" fill="var(--bg)"/>')
    text(out, x + 224, y0 + 2 * row - 14, "the turn a lookup spares", cls="m", anchor="end", size=10.5)

    x = lanes[1][0]
    turn(x, y0, 1, "get_user_details(u)")
    result(x, y0 + row, "↳ orders: [#W12]")
    by = y0 + 2 * row - 16
    out.append(f'<rect x="{x + 50}" y="{by}" width="190" height="{row + 22}" rx="6" fill="var(--pb)" '
               'stroke="var(--b)" stroke-width="1.5"/>')
    text(out, x + 58, by + 16, "+ get_order_details(#W12)", size=mono, mono=True)
    text(out, x + 58, by + 16 + row - 4, "↳ status: delivered", cls="m", size=mono, mono=True)
    turn(x, y0 + 4 * row - 10, 2, "reply to the user", sans=True)
    text(out, x, y0 + 5 * row + 8, "2 LLM turns", weight="700")
    # The argument, bound from the result before it.
    end = x + 58 + width("↳ orders: [#W12]", mono, True)
    out.append(f'<path d="M{end + 4:.1f},{y0 + row - 3} C{end + 70:.1f},{y0 + row - 3} {x + 272:.1f},{by + 11} {x + 244:.1f},{by + 11}" '
               'fill="none" stroke="var(--muted)" stroke-width="1.2" marker-end="url(#arrow)"/>')
    text(out, end + 8, y0 + row - 8, "bound from this result", cls="m", size=10.5)

    # (b) The event the speculator estimates.
    x0 = 540
    text(out, x0, 16, "(b) Which reads to make", weight="700", size=13)
    text(out, x0, 44, "After the user's details: will the agent read the order?", cls="m", size=11.5)
    chips = [("user", "read"), ("reply", "reply"), ("order", "read"), ("reply", "reply"), ("product", "read"), ("cancel", "write")]
    cy, ch, gap = 74, 22, 7
    xs = []
    cx = x0
    for label, kind in chips:
        cw = width(label, mono, kind != "reply") + 12
        xs.append((cx, cw))
        if kind == "read":
            stroke = 'stroke="var(--b)" stroke-width="2"' if label == "order" else 'stroke="var(--muted)"'
            out.append(f'<rect x="{cx:.1f}" y="{cy}" width="{cw:.1f}" height="{ch}" rx="4" fill="var(--panel)" {stroke}/>')
            text(out, cx + cw / 2, cy + 15, label, anchor="middle", size=mono, mono=True)
        elif kind == "reply":
            out.append(f'<rect x="{cx:.1f}" y="{cy}" width="{cw:.1f}" height="{ch}" rx="4" fill="none" stroke="var(--g1)" stroke-dasharray="3 2"/>')
            text(out, cx + cw / 2, cy + 15, label, cls="m", anchor="middle", size=mono, italic=True)
        else:
            out.append(f'<rect x="{cx:.1f}" y="{cy}" width="{cw:.1f}" height="{ch}" rx="4" fill="var(--ink)"/>')
            text(out, cx + cw / 2, cy + 15, label, cls="k", anchor="middle", size=mono, mono=True)
        cx += cw + gap
    # The speculator decides after the tool response.
    dx = xs[0][0] + xs[0][1] + gap / 2
    out.append(f'<path d="M{dx - 5:.1f},{cy - 9} L{dx + 5:.1f},{cy - 9} L{dx:.1f},{cy - 2} z" fill="var(--ink)"/>')
    text(out, dx - 8, cy - 12, "the speculator decides", cls="m", size=11)
    text(out, xs[-1][0] + xs[-1][1] / 2, cy - 6, "write", cls="m", anchor="middle", size=11)

    def bracket(i: int, j: int, y: float, key: str, label: str) -> None:
        a, b = xs[i][0], xs[j][0] + xs[j][1]
        out.append(f'<path d="M{a:.1f},{y - 6} L{a:.1f},{y} L{b:.1f},{y} L{b:.1f},{y - 6}" fill="none" stroke="var(--{key})" stroke-width="2"/>')
        text(out, a, y + 15, label, size=11.5)

    bracket(1, 1, cy + ch + 12, "a", "the next step: a reply")
    bracket(1, 4, cy + ch + 46, "b", "before the next write: U(x), where the order is")
    # The two estimates, as the paper reports them (§4.2).
    by0, L = 188, 300
    for k, (key, label, v, shown) in enumerate((("a", "read next, under the habit", 0.64, "0.64"),
                                                ("b", "read before the next write, observed", 0.94, "94%"))):
        y = by0 + k * 36
        text(out, x0, y - 4, label, size=11.5)
        bx0 = x0
        out.append(f'<rect x="{bx0}" y="{y}" width="{L * v:.1f}" height="14" rx="2" fill="var(--{key})"/>')
        out.append(f'<rect x="{bx0}" y="{y}" width="{L}" height="14" rx="2" fill="none" stroke="var(--grid)"/>')
        text(out, bx0 + L * v - 5, y + 11, shown, cls="k", anchor="end", size=11, weight="700")
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


# --- Figure: the threshold -------------------------------------------------------------------

COSTS = [("retail", 5820, 2470, "5{,}820", "2{,}470", ""),
         ("airline", 7050, 1020, "7{,}050", "1{,}020", ' stroke-dasharray="7 4"'),
         ("telecom", 8930, 1180, "8{,}930", "1{,}180", ' stroke-dasharray="2 3"')]


def threshold(path: Path) -> None:
    checked([n for c in COSTS for n in c[3:5]] + ["0.298", "0.13", "0.12"])
    w, h = 420, 270
    left, right, top, bottom = 52, 70, 14, 40
    pw, ph = w - left - right, h - top - bottom
    ymin, ymax = -3, 9
    sx = lambda q: left + pw * q  # noqa: E731
    sy = lambda v: top + ph * (1 - (v - ymin) / (ymax - ymin))  # noqa: E731
    out = svg_open(w, h)
    # Below zero a read costs more in detours than it saves.
    out.append(f'<rect x="{sx(0)}" y="{sy(0):.1f}" width="{pw}" height="{sy(ymin) - sy(0):.1f}" fill="var(--panel)"/>')
    for v in range(ymin, ymax + 1, 3):
        out.append(f'<line x1="{sx(0)}" x2="{sx(1)}" y1="{sy(v):.1f}" y2="{sy(v):.1f}" stroke="var(--grid)"/>')
        text(out, left - 6, sy(v) + 4, f"{v:g}".replace("-", "−"), cls="m", anchor="end", size=11)
    for q in (0, 0.25, 0.5, 0.75, 1):
        text(out, sx(q), top + ph + 16, f"{q:g}", cls="m", anchor="middle", size=11)
    out.append(f'<line x1="{sx(0)}" x2="{sx(1)}" y1="{sy(0):.1f}" y2="{sy(0):.1f}" stroke="var(--ink)"/>')
    text(out, sx(1) - 4, sy(ymin) - 6, "a detour costs more than the read is worth", cls="m", anchor="end", size=10.5)
    for name, beta, delta, _, _, dash in COSTS:
        b, d = beta / 1000, delta / 1000
        out.append(f'<line x1="{sx(0)}" y1="{sy(-d):.1f}" x2="{sx(1)}" y2="{sy(b):.1f}" stroke="var(--ink)" stroke-width="1.75"{dash}/>')
        text(out, sx(1) + 6, sy(b) + 4, name, size=11.5)
        star = d / (b + d)
        out.append(f'<circle cx="{sx(star):.1f}" cy="{sy(0):.1f}" r="4" fill="var(--b)" stroke="var(--bg)" stroke-width="1.5"/>')
    text(out, sx(0.298) + 7, sy(0) + 16, "θ* 0.30, retail", size=11)
    lx, ly = sx(0.03), sy(5.2)
    out.append(f'<line x1="{lx + 30:.1f}" y1="{ly + 5:.1f}" x2="{sx(0.12):.1f}" y2="{sy(0) - 6:.1f}" stroke="var(--muted)"/>')
    text(out, lx, ly, "θ* 0.12–0.13, airline and telecom", size=11)
    text(out, sx(0.03), sy(8), "qβ − (1−q)δ", size=12, italic=True)
    text(out, left + pw / 2, h - 6, "q, probability the read is used before the next write", cls="m", anchor="middle", size=11)
    out.append(f'<text class="m" font-size="11" transform="translate(14 {top + ph / 2}) rotate(-90)" text-anchor="middle">'
               "expected value of one read, thousand input tokens</text>")
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


# --- Figure: what decides each turn (Tables 1 and 1b) ---------------------------------------

TURNS = [
    ("τ²-bench, nine agents", [
        ("retail", "39.6%", "14.0%", "46.3%", "34.2%"),
        ("airline", "40.3%", "12.7%", "46.9%", "29.5%"),
        ("telecom", "55.2%", "10.6%", "34.3%", "24.7%"),
        ("all", "46.8%", "12.2%", "41.0%", "29.0%"),
    ]),
    ("Six more benchmarks", [
        ("τ-bench", "47.0%", "13.2%", "39.7%", "28.4%"),
        ("BFCL", "34.9%", "33.7%", "31.5%", "13.2%"),
        ("AgentDojo", "26.9%", "16.5%", "56.5%", "22.1%"),
        ("WorkBench", "27.4%", "26.6%", "46.1%", "3.5%"),
        ("DTap-Bench", "12.0%", "59.9%", "28.1%", "10.7%"),
        ("MCPMark", "5.1%", "50.9%", "43.9%", "7.0%"),
    ]),
]


def turns(path: Path) -> None:
    rows_md = PAPER.read_text()
    for _, rows in TURNS:
        for r in rows:
            if not any(all(v in line for v in r[1:]) for line in rows_md.splitlines() if line.startswith("|")):
                raise SystemExit(f"paper_diagrams.py: no table row of paper/stretto.md holds {r}")
    w, left, right, bar, gap, head = 420, 84, 16, 17, 5, 24
    top = 50
    h = top + sum(head + len(rows) * (bar + gap) for _, rows in TURNS) + 26
    pw = w - left - right
    out = svg_open(w, h)
    segs = [("a", "reads in the ceiling", "k"), ("pa", "other reads", "t"), ("g1", "writes", "t"), ("g2", "replies to the user", "t")]
    for i, (key, label, _) in enumerate(segs):
        lx, ly = (0, 10) if i < 2 else (0, 28)
        lx = left + (i % 2) * 160
        out.append(f'<rect x="{lx}" y="{ly}" width="12" height="12" rx="2" fill="var(--{key})"/>')
        text(out, lx + 18, ly + 10, label, size=11.5)
    y = top
    for title, rows in TURNS:
        text(out, 0, y + 14, title, weight="700", size=12)
        y += head
        for name, replies, writes, reads, ceiling in rows:
            vals = [pct(ceiling), pct(reads) - pct(ceiling), pct(writes), pct(replies)]
            total = sum(vals)
            text(out, left - 8, y + 12.5, name, cls="t" if name == "all" else "m", anchor="end", size=11.5,
                 weight="700" if name == "all" else None)
            x = left
            for (key, _, cls), v in zip(segs, vals):
                sw = pw * v / total
                out.append(f'<rect x="{x:.1f}" y="{y}" width="{max(sw - 2, 0.5):.1f}" height="{bar}" fill="var(--{key})"/>')
                if sw >= 28:
                    text(out, x + 4, y + 12.5, f"{v:.1f}", cls=cls, size=10.5)
                x += sw
            y += bar + gap
    for v in (0, 25, 50, 75, 100):
        text(out, left + pw * v / 100, h - 8, f"{v}%", cls="m", anchor="middle", size=10.5)
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


# --- Figure: share of the ceiling in replay (Table 3) ---------------------------------------

REPLAY = [  # domain, next step [lo, hi], use before write [lo, hi], difference
    ("retail", ("76.2%", "71.7", "81.6"), ("86.4%", "80.2", "93.1"), "+10.2"),
    ("airline", ("46.2%", "38.3", "55.1"), ("46.6%", "38.5", "55.4"), "+0.5"),
    ("telecom", ("47.9%", "45.5", "50.3"), ("51.7%", "49.0", "54.4"), "+3.8"),
    ("telecom, solo", ("40.0%", "36.9", "43.9"), ("42.8%", "39.0", "47.0"), "+2.8"),
]


def replay(path: Path) -> None:
    checked([f"{v} [{lo}, {hi}]" for r in REPLAY for v, lo, hi in r[1:3]] + [r[3] for r in REPLAY])
    w, left, right, top, rowh = 420, 92, 58, 44, 40
    h = top + len(REPLAY) * rowh + 34
    pw = w - left - right
    lo_x, hi_x = 30, 100
    sx = lambda v: left + pw * (v - lo_x) / (hi_x - lo_x)  # noqa: E731
    out = svg_open(w, h)
    for i, (key, label) in enumerate((("a", "next step"), ("b", "use before write"))):
        lx = left + i * 120
        out.append(f'<line x1="{lx}" x2="{lx + 18}" y1="16" y2="16" stroke="var(--{key})" stroke-width="2"/>')
        out.append(f'<circle cx="{lx + 9}" cy="16" r="4.5" fill="var(--{key})" stroke="var(--bg)" stroke-width="1.5"/>')
        text(out, lx + 24, 20, label, size=11.5)
    text(out, w - 2, 20, "difference, points", cls="m", anchor="end", size=11)
    body = top + len(REPLAY) * rowh - 10
    for v in range(lo_x, hi_x + 1, 10):
        out.append(f'<line x1="{sx(v):.1f}" x2="{sx(v):.1f}" y1="{top - 8}" y2="{body}" stroke="var(--grid)"/>')
        text(out, sx(v), body + 15, f"{v}", cls="m", anchor="middle", size=10.5)
    text(out, left + pw / 2, h - 4, "share of the read-only ceiling taken (%), 95% intervals", cls="m", anchor="middle", size=11)
    for i, (name, nxt, use, diff) in enumerate(REPLAY):
        y = top + i * rowh + 6
        text(out, left - 8, y + 9, name, cls="m", anchor="end", size=11.5)
        for k, (key, (v, lo, hi)) in enumerate((("a", nxt), ("b", use))):
            yy = y + k * 12
            out.append(f'<line x1="{sx(float(lo)):.1f}" x2="{sx(float(hi)):.1f}" y1="{yy}" y2="{yy}" stroke="var(--{key})" stroke-width="2"/>')
            out.append(f'<circle cx="{sx(pct(v)):.1f}" cy="{yy}" r="4.5" fill="var(--{key})" stroke="var(--bg)" stroke-width="1.5"/>')
        text(out, w - 2, y + 10, diff, anchor="end", size=11.5, weight="700" if i == 0 else None)
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


# --- Figure: every live comparison ----------------------------------------------------------

LIVE = [  # group, [(label, change, lo, hi, kind)], kind: "s" the speculator, "p" the prompt alone
    ("τ²-bench: GLM-5.3, agent and user", [
        ("retail, 20 tasks", "−32.2%", "−40.0", "−23.5", "s"),
        ("airline, 8 tasks", "−16.2%", "−37.2", "+10.6", "s"),
        ("both, 28 tasks", "−27.9%", "−35.9", "−19.1", "s"),
    ]),
    ("τ²-bench: Claude Sonnet 5, 84 pairs", [
        ("the speculator", "−20.5%", "−24.4", "−16.5", "s"),
        ("the prompt alone", "−3.4%", "−7.8", "+1.2", "p"),
        ("speculator, prompt in both", "−22.9%", "−26.1", "−19.5", "s"),
    ]),
    ("τ²-bench: Claude Haiku 4.5, 84 pairs", [
        ("the speculator", "−22.4%", "−29.4", "−15.8", "s"),
        ("the prompt alone", "−5.9%", "−10.9", "−0.9", "p"),
        ("speculator, prompt in both", "−17.2%", "−24.0", "−10.2", "s"),
    ]),
    ("τ²-bench: glm-5.3-flash, with guards", [
        ("20 tasks", "−20.8%", "−29.9", "−11.0", "s"),
    ]),
    ("AgentDojo and BFCL: GLM-5.3, Claude Haiku 4.5", [
        ("AgentDojo, Slack and travel", "−10.1%", "−14.0", "−5.8", "s"),
        ("AgentDojo, banking, workspace", "−2.1%", "−8.7", "+3.6", "s"),
        ("AgentDojo, all", "−6.0%", "−9.9", "−2.1", "s"),
        ("BFCL, 20 held-out tasks", "+1.6%", "−4.2", "+6.3", "s"),
    ]),
]


def live(path: Path) -> None:
    md = PAPER.read_text().replace("**", "")
    for _, rows in LIVE:
        for label, v, lo, hi, _ in rows:
            if v == "−20.8%":
                checked(["20.8% fewer LLM turns (11.0–29.9%)"])
            elif not (f"{v} [{lo}, {hi}]" in md or f"{v} [{hi}, {lo}]" in md):
                raise SystemExit(f"paper_diagrams.py: paper/stretto.md does not hold {v} [{lo}, {hi}]")
    w, left, right, top, rowh, head = 436, 178, 48, 30, 17, 22
    h = top + sum(head + len(r) * rowh for _, r in LIVE) + 36
    pw = w - left - right
    lo_x, hi_x = -45, 15
    sx = lambda v: left + pw * (v - lo_x) / (hi_x - lo_x)  # noqa: E731
    out = svg_open(w, h)
    for i, (key, label) in enumerate((("b", "the speculator"), ("g1", "a prompt for parallel calls, alone"))):
        lx = 0 if i == 0 else 120
        out.append(f'<rect x="{lx + 4}" y="8" width="9" height="9" fill="var(--{key})"/>')
        text(out, lx + 18, 17, label, size=11.5)
    body = h - 36
    for v in range(lo_x + 5, hi_x + 1, 10):
        out.append(f'<line x1="{sx(v):.1f}" x2="{sx(v):.1f}" y1="{top}" y2="{body}" stroke="var(--grid)"/>')
        text(out, sx(v), body + 15, f"{v:+d}".replace("-", "−") if v else "0", cls="m", anchor="middle", size=10.5)
    out.append(f'<line x1="{sx(0):.1f}" x2="{sx(0):.1f}" y1="{top}" y2="{body}" stroke="var(--muted)" stroke-width="1.2"/>')
    text(out, left + pw / 2, h - 4, "change in LLM turns (%), 95% intervals", cls="m", anchor="middle", size=11)
    y = top
    for title, rows in LIVE:
        text(out, 2, y + 14, title, weight="700", size=11.5)
        y += head
        for label, v, lo, hi, kind in rows:
            cy = y + rowh / 2 - 2
            key = "b" if kind == "s" else "g1"
            text(out, left - 8, cy + 4, label, cls="m", anchor="end", size=11)
            out.append(f'<line x1="{sx(pct(lo)):.1f}" x2="{sx(pct(hi)):.1f}" y1="{cy:.1f}" y2="{cy:.1f}" stroke="var(--{key})" stroke-width="2"/>')
            out.append(f'<rect x="{sx(pct(v)) - 4.5:.1f}" y="{cy - 4.5:.1f}" width="9" height="9" fill="var(--{key})" stroke="var(--bg)" stroke-width="1.5"/>')
            text(out, w - 2, cy + 4, v, anchor="end", size=11)
            y += rowh
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


# --- Figure: the cascade where no user speaks -----------------------------------------------

def cascade(path: Path) -> None:
    checked(["40 held-out", "resolves 25 runs, transfers 11 to a person", "hands back 4, all four failures",
             "it misses one failure", "in 4.75 LLM turns each", "39 of 40", "0.48 LLM turns per ticket", "15.9"])
    w, h = 420, 368
    out = svg_open(w, h)

    def box(x: float, y: float, bw: float, bh: float, title: str, sub: str = "", fill: str = "panel", stroke: str = "muted", bold: bool = True) -> None:
        out.append(f'<rect x="{x}" y="{y}" width="{bw}" height="{bh}" rx="6" fill="var(--{fill})" stroke="var(--{stroke})" stroke-width="1.3"/>')
        text(out, x + bw / 2, y + (17 if sub else bh / 2 + 4.5), title, anchor="middle", size=12, weight="700" if bold else None)
        for k, line in enumerate(sub.split("\n") if sub else []):
            text(out, x + bw / 2, y + 32 + 13 * k, line, cls="m", anchor="middle", size=10.5)

    def arrow(x1: float, y1: float, x2: float, y2: float) -> None:
        out.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="var(--muted)" stroke-width="1.3" marker-end="url(#arrow)"/>')

    cx = w / 2
    box(cx - 120, 4, 240, 26, "40 held-out solo telecom tickets", bold=False)
    arrow(cx, 30, cx, 46)
    box(cx - 140, 47, 280, 40, "compiled procedure, no model", "trees, bindings and a guard, from traces")
    arrow(cx, 87, cx, 103)
    box(cx - 140, 104, 280, 40, "check the ticket's stated outcome", "one read")
    cols = [(4, "resolved 25", "one failure missed", "panel", "muted"),
            (144, "transferred 11", "to a person, by policy", "panel", "muted"),
            (284, "handed back 4", "all four failures", "pb", "b")]
    for x, t, s, fill, stroke in cols:
        arrow(cx, 144, x + 66, 166)
        box(x, 167, 132, 40, t, s, fill, stroke)
    arrow(284 + 66, 207, 284 + 66, 223)
    box(170, 224, 246, 40, "GLM-5.3 takes over", "with the procedure's calls and results", "pb", "b")
    text(out, 166, 238, "resolves all 4 in both trials,", cls="m", anchor="end", size=10.5)
    text(out, 166, 251, "4.75 LLM turns each", cls="m", anchor="end", size=10.5)
    # The outcome.
    out.append(f'<line x1="0" x2="{w}" y1="280" y2="280" stroke="var(--grid)"/>')
    text(out, 2, 298, "39 of 40 passed", weight="700", size=12.5)
    text(out, 108, 298, "LLM turns per ticket:", cls="m", size=11.5)
    L, x0 = 220, 150
    for k, (key, label, v, shown) in enumerate((("b", "the cascade", 0.48, "0.48"), ("g1", "GLM-5.3 alone", 15.9, "15.9"))):
        y = 310 + 24 * k
        text(out, x0 - 8, y + 11, label, anchor="end", size=11.5)
        out.append(f'<rect x="{x0}" y="{y}" width="{max(L * v / 16, 2):.1f}" height="14" rx="2" fill="var(--{key})"/>')
        text(out, x0 + L * v / 16 + 6, y + 11.5, shown, size=11.5, weight="700")
    out.append("</svg>")
    path.write_text("\n".join(out) + "\n")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", type=Path, default=Path("paper"))
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for name, draw in (("lookup", lookup), ("threshold", threshold), ("turns", turns),
                       ("replay", replay), ("live", live), ("cascade", cascade)):
        draw(args.out / f"{name}.svg")
    print("wrote lookup.svg, threshold.svg, turns.svg, replay.svg, live.svg, cascade.svg")


if __name__ == "__main__":
    main()
