#!/usr/bin/env python3
"""Write the LaTeX manuscript from the working paper.

    python3 build.py     # or `make`, which then runs pdflatex and bibtex

reads ../stretto.md and writes stretto.tex (from template.tex) and
figures/*.pdf (from ../*.svg). The Markdown is the source:

- its citation labels ([AWM], [ASI, SkillWeaver]) become natbib citations of
  refs.bib (CITES);
- each table becomes a float, captioned with the label the Markdown gives it
  (Table 1b), across both columns when it is too wide for one, with the note
  after it (a dagger, or "Brackets give ...") kept under it;
- each figure becomes a float, across both columns when its SVG is wider than
  600 px;
- each proposition is boxed, and each paragraph's bold lead-in is set as a
  run-in head;
- its Mermaid diagram becomes the TikZ picture in figure1.tex, and its
  Algorithm 1 algorithm1.tex;
- its section numbers are dropped for LaTeX's, which are the same, and its
  appendices follow the references.

Each SVG is drawn in its light colors, on white, in Latin Modern Sans. Needs
pandoc and rsvg-convert.
"""

import os
import re
import subprocess
import sys
import tempfile
from datetime import date
from pathlib import Path

HERE = Path(__file__).resolve().parent
PAPER = HERE.parent / "stretto.md"

# Each citation label of the Markdown, and its keys in refs.bib.
CITES = {
    "Barres et al. 2025": ["barres2025tau2"],
    "τ²-bench": ["barres2025tau2"],
    "τ-bench": ["yao2024taubench"],
    "BFCL": ["patil2025bfcl"],
    "AgentDojo": ["debenedetti2024agentdojo"],
    "WorkBench": ["styles2024workbench"],
    "WorkBench Revisited": ["styles2026workbenchrevisited"],
    "DTap": ["chen2026dtap"],
    "MCPMark": ["wu2025mcpmark"],
    "AWM": ["wang2025awm"],
    "ASI": ["wang2025asi"],
    "SkillWeaver": ["zheng2025skillweaver"],
    "WALT": ["prabhu2025walt"],
    "TraceCompiler": ["elyadouni2026tracecompiler"],
    "LOOP": ["wang2026loop"],
    "Compiled AI": ["trooskens2026compiledai"],
    "PreAct": ["li2026preact"],
    "Bosco et al.": ["bosco2019routines"],
    "Leno et al.": ["leno2021routines"],
    "Speculative Actions": ["ye2026speculativeactions"],
    "Speculative Macro Commit": ["liu2026macrocommit"],
    "AutoTool": ["jia2026autotool"],
    "AOSpec": ["chen2026aospec"],
    "PASTE": ["sui2026paste"],
    "Speculate with Memory": ["li2026specmemory"],
    "Speculate While You Reason": ["ji2026specreason"],
    "Speculative Interaction Agents": ["hooper2026speculativeinteraction"],
    "Ghost Tool Calls": ["mohammadi2026ghost"],
    "LLMCompiler": ["kim2024llmcompiler"],
    "APC": ["zhang2025apc"],
    "Leviathan et al.": ["leviathan2023speculative"],
    "EAGLE-2": ["li2024eagle2"],
    "SpecDec++": ["huang2024specdecpp"],
    "STAGE": ["luo2026stage"],
    "PolicyGuide": ["kang2026policyguide"],
    "MacKay & Peto": ["mackay1995hierarchical"],
    "MacKay & Peto 1995": ["mackay1995hierarchical"],
    "Decision mining": ["rozinat2006decision"],
    "Ibrahim & Chen": ["ibrahim2000power"],
    "DAgger": ["ross2011dagger"],
    "TRADE": ["wu2019trade"],
    "Cascades": ["chen2023frugalgpt", "madaan2024automix"],
    "fugue": ["nodeland2026fugue"],
    "D0": ["nodeland2026rfc"],
}
MARK = "TABLELABEL{}ENDLABEL"
WIDE_SVG = 600  # px: a figure wider than this spans both columns
COLUMN_CHARS = 54  # a table whose rows run longer than this, at \small, spans both columns,
WRAP_CHARS = 28  # its first column wrapped at this width


def pandoc(markdown: str) -> str:
    done = subprocess.run(
        ["pandoc", "-f", "markdown+tex_math_dollars+autolink_bare_uris+lists_without_preceding_blankline", "-t", "latex", "--natbib",
         "--wrap=none", "--columns=10000", "--top-level-division=section"],
        input=markdown, capture_output=True, text=True, check=True,
    )
    return done.stdout


def cite(text: str) -> str:
    """Citation labels to pandoc citations, where every label in the brackets is known."""
    def one(m: re.Match) -> str:
        labels = re.split(r",\s*", m.group(1))
        if not all(label in CITES for label in labels):
            return m.group(0)
        return "[" + "; ".join(f"@{key}" for label in labels for key in CITES[label]) + "]"

    return re.sub(r"\[([^\[\]\n]+)\](?![(\[])", one, text)


def prepare(text: str, body: bool) -> str:
    """The Markdown of the body (or the appendices), ready for pandoc."""
    # The Mermaid diagram and its caption: the TikZ figure.
    def regimes(m: re.Match) -> str:
        caption = inline(cite(m.group(1)))
        return ("```{=latex}\n\\begin{figure*}[tp]\n\\centering\n\\adjustbox{max width=\\textwidth}{\\input{figure1}}\n"
                f"\\caption{{{caption}}}\n\\label{{fig:regimes}}\n\\end{{figure*}}\n```\n")

    text, n = re.subn(r"```mermaid\n.*?```\n\n\*Figure \d+\. (.+?)\*\n", regimes, text, flags=re.S)
    assert n == body, "the Mermaid figure"
    text, n = re.subn(r"```text\nAlgorithm 1\..*?```\n", lambda m: "```{=latex}\n\\input{algorithm1}\n```\n", text, flags=re.S)
    assert n == body, "Algorithm 1"
    # Figures: the image, captioned by the paragraph after it.
    text, n = re.subn(r"!\[[^\]]*\]\(([\w-]+)\.svg\)\n\n\*Figure \d+\. (.+?)\*\n", figure, text)
    assert n == 10 * body, f"{n} figures"
    # Tables: the caption paragraph before each, with its label kept.
    text = re.sub(
        r"^\*Table (\w+)\. (.+?)\*\n\n(?=\|)",
        lambda m: f"Table: {MARK.format(m.group(1))} {m.group(2)}\n\n",
        text, flags=re.M,
    )
    # Sections: LaTeX numbers them, in the same order.
    text = re.sub(r"^## (?:Appendix [A-Z]\.|\d+) (.+)$", r"# \1", text, flags=re.M)
    text = re.sub(r"^### \d+\.\d+ (.+)$", r"## \1", text, flags=re.M)
    return cite(text)


def figure(m: re.Match) -> str:
    """A figure float, across both columns when the SVG is wide."""
    # A caption that opens with "(a)" is not a list.
    stem, caption = m.group(1), inline(cite(re.sub(r"^\(", r"\\(", m.group(2))))
    width = float(re.search(r'viewBox="0 0 ([\d.]+)', (HERE.parent / f"{stem}.svg").read_text()).group(1))
    env, size, where = ("figure*", "\\textwidth", "tp") if width > WIDE_SVG else ("figure", "\\columnwidth", "tbp")
    return ("```{=latex}\n"
            f"\\begin{{{env}}}[{where}]\n\\centering\n"
            f"\\includegraphics[width={size},height=0.72\\textheight,keepaspectratio]{{figures/{stem}.pdf}}\n"
            f"\\caption{{{caption}}}\n\\label{{fig:{stem}}}\n\\end{{{env}}}\n```\n")


def inline(markdown: str) -> str:
    return pandoc(markdown).strip()


def braced(s: str, i: int) -> tuple[str, int]:
    """The contents of the braces opening at s[i], and the index after them."""
    assert s[i] == "{"
    depth = 0
    for j in range(i, len(s)):
        depth += {"{": 1, "}": -1}.get(s[j], 0)
        if depth == 0:
            return s[i + 1:j], j + 1
    raise ValueError("unbalanced braces")


def two_lines(cell: str) -> str:
    """A long header cell on two lines, split at the space nearest its middle
    (after a comma if one is near), outside braces and math."""
    cell = cell.strip()
    plain = re.sub(r"\\[a-zA-Z]+|[{}$]", "", cell)
    if len(plain) <= 16:
        return cell
    depth, math, spaces = 0, False, []
    for i, c in enumerate(cell):
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif c == "$":
            math = not math
        elif c == " " and depth == 0 and not math:
            spaces.append(i)
    if not spaces:
        return cell
    middle = len(cell) / 2
    best = min(spaces, key=lambda i: abs(i - middle) - (len(cell) / 6 if cell[i - 1] == "," else 0))
    return f"\\begin{{tabular}}[b]{{@{{}}c@{{}}}}{cell[:best]}\\\\ {cell[best + 1:]}\\end{{tabular}}"


def tables(tex: str) -> str:
    """pandoc's longtables as table floats, captioned with their own labels."""
    out, at = [], 0
    for m in re.finditer(r"\\begin\{longtable\}", tex):
        if m.start() < at:
            continue
        end = tex.index("\\end{longtable}", m.start()) + len("\\end{longtable}")
        block = tex[m.start():end]
        i = block.index("\\caption{")
        caption, _ = braced(block, i + len("\\caption"))
        label = re.search(r"TABLELABEL(\w+)ENDLABEL\s*", caption)
        caption = caption.replace(label.group(0), "")
        head = block.split("\\toprule\\noalign{}\n", 1)[1].split("\\midrule\\noalign{}\n", 1)[0]
        cells = head.rstrip().removesuffix("\\\\").split("&")
        head = " & ".join([cells[0].strip()] + [two_lines(c) for c in cells[1:]]) + " \\\\\n"
        rows = block.split("\\endlastfoot\n", 1)[1].rsplit("\\end{longtable}", 1)[0]
        columns = head.count("&") + 1
        # The note under a table travels with it.
        note = re.match(r"\s*\n((?:†|Brackets give)[^\n]*)\n", tex[end:])
        if note:
            end += note.end()
        widths = longest(head, rows)
        wrap = widths[0] > WRAP_CHARS
        wide = sum(widths) - (widths[0] - WRAP_CHARS if wrap else 0) > COLUMN_CHARS
        # A first column of long labels wraps, rather than shrinking the table.
        labels = [re.sub(r"\\[a-zA-Z]+|[{}]", "", row.split("&", 1)[0]).strip() for row in rows.split("\\\\")]
        if wide:
            first = ">{\\raggedright\\arraybackslash}p{4.2cm}" if columns > 3 and max(map(len, labels)) > 32 else "l"
        else:
            first = ">{\\raggedright\\arraybackslash}p{3.7cm}" if wrap else "l"
        env = "table*" if wide else "table"
        # A column of words is set left, a column of numbers centred.
        body = [row.split("&") for row in rows.split("\\\\") if row.strip()]
        align = "".join("l" if words(row[i] for row in body if i < len(row)) else "c" for i in range(1, columns))
        out += [tex[at:m.start()], "\n".join([
            f"\\begin{{{env}}}[{'tp' if wide else 'tbp'}]",
            "\\centering\\small",
            f"\\renewcommand{{\\thetable}}{{{label.group(1)}}}",
            f"\\caption{{{caption}}}",
            f"\\label{{tab:{label.group(1)}}}",
            "\\begin{adjustbox}{max width=\\linewidth}",
            "\\begin{tabular}{" + first + align + "}",
            "\\toprule",
            head.rstrip("\n"),
            "\\midrule",
            rows.rstrip("\n"),
            "\\bottomrule",
            "\\end{tabular}",
            "\\end{adjustbox}",
            *([f"\\par\\smallskip{{\\raggedright\\footnotesize\\color{{muted}}{note.group(1)}\\par}}"] if note else []),
            f"\\end{{{env}}}",
        ])]
        at = end
    return "".join(out) + tex[at:]


def words(cells) -> bool:
    """Whether most of a column's cells are words, not numbers."""
    plain = [re.sub(r"\\[a-zA-Z]+|[{}$\\]", "", c).strip() for c in cells]
    plain = [c for c in plain if c]
    return sum(c[0].isalpha() and not c.startswith(("n =",)) for c in plain) > len(plain) / 2


def longest(head: str, rows: str) -> list[int]:
    """The characters of each column's longest cell, as printed (a split header cell by
    its longer line), plus two for the space between columns."""
    def plain(cell: str) -> str:
        cell = re.sub(r"\\begin\{tabular\}\[b\]\{@\{\}c@\{\}\}|\\end\{tabular\}", "", cell)
        lines = cell.split("\\\\")
        return max((re.sub(r"\\[a-zA-Z]+|[{}$\\]", "", line).strip() for line in lines), key=len)

    widths: list[int] = []
    for row in [head] + rows.split("\\\\"):
        for i, cell in enumerate(row.split("&")):
            n = len(plain(cell))
            widths += [0] * (i + 1 - len(widths))
            widths[i] = max(widths[i], n)
    return [w + 2 for w in widths]


def styled(tex: str) -> str:
    """Propositions boxed; a paragraph's (or a list item's) bold lead-in as a run-in head."""
    tex = re.sub(r"(?m)^(\\textbf\{Proposition \d+ \([^)]*\)\.\}.*?)(?=\n\n)",
                 lambda m: f"\\begin{{prop}}\n{m.group(1)}\n\\end{{prop}}", tex, flags=re.S)
    # A display of two equations, side by side, is too wide for a column: one under the other.
    tex = re.sub(r"\\\[([^\]]*?),\s*\\qquad\s*([^\]]*?)\\\]",
                 lambda m: f"\\begin{{gather*}}{m.group(1)},\\\\ {m.group(2)}\\end{{gather*}}", tex)
    tex = re.sub(r"(?m)^\\textbf\{([^{}]+)\}", r"\\runin{\1}", tex)
    tex = re.sub(r"(\\item\s+)\\textbf\{([^{}]+)\}", r"\1\\runin{\2}", tex)
    # τ-bench's pass^k, with a caret rather than a circumflex accent.
    return tex.replace("pass\\^{}", "pass\\textasciicircum{}")


def svg_to_pdf(svg: Path, pdf: Path) -> None:
    """The figure in its light colors, on white, in Latin Modern Sans."""
    s = svg.read_text()
    light = dict(re.findall(r"--([\w-]+):(#[0-9a-fA-F]{3,8})", s.split("@media", 1)[0]))
    light["bg"] = "#ffffff"
    s = re.sub(r"@media \(prefers-color-scheme: dark\)\{svg\{[^}]*\}\}", "", s)
    s = re.sub(r"var\(--([\w-]+)\)", lambda m: light[m.group(1)], s)
    s = re.sub(r'font-family="([^"]*)"', lambda m: 'font-family="Latin Modern Mono, DejaVu Sans Mono, monospace"'
               if "monospace" in m.group(1) else 'font-family="Latin Modern Sans, DejaVu Sans, sans-serif"', s)
    # Dated by the SVG's last commit, so an unchanged figure converts to the same bytes.
    env = dict(os.environ)
    if "SOURCE_DATE_EPOCH" not in env:
        stamp = subprocess.run(["git", "log", "-1", "--format=%ct", "--", str(svg)], cwd=HERE,
                               capture_output=True, text=True).stdout.strip()
        env["SOURCE_DATE_EPOCH"] = stamp or "0"
    with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as f:
        f.write(s)
    try:
        subprocess.run(["rsvg-convert", "-f", "pdf", "-o", str(pdf), f.name], check=True, env=env)
    finally:
        os.unlink(f.name)


def main() -> None:
    md = PAPER.read_text()
    title = md.split("\n", 1)[0].removeprefix("# ").strip()
    when = re.search(r"^\*Draft, (\d{4})-(\d{2})-(\d{2})\.\*$", md, re.M)
    day = date(*map(int, when.groups()))
    abstract = md.split("## Abstract\n", 1)[1].split("\n## ", 1)[0].strip()
    body = md[md.index("## 1 Introduction"):md.index("## References")]
    appendix = md[md.index("## Appendix A."):]

    (HERE / "figures").mkdir(exist_ok=True)
    for svg in sorted(HERE.parent.glob("*.svg")):
        svg_to_pdf(svg, HERE / "figures" / f"{svg.stem}.pdf")

    tex = (HERE / "template.tex").read_text()
    parts = {
        "TITLE": inline(title),
        "PLAINTITLE": title,
        "SHORTTITLE": title.split(":")[0],
        "DATE": f"{day.day} {day:%B %Y}",
        "ABSTRACT": inline(cite(abstract)),
        "BODY": styled(tables(pandoc(prepare(body, True)))),
        "APPENDIX": styled(tables(pandoc(prepare(appendix, False)))),
    }
    for key, value in parts.items():
        tex = tex.replace(f"%%{key}%%", value)
    left = re.findall(r"TABLELABEL|```|\[@", tex)
    if left:
        sys.exit(f"build.py: left unconverted: {sorted(set(left))}")
    (HERE / "stretto.tex").write_text(tex)
    print(f"build.py: wrote stretto.tex and {len(list((HERE / 'figures').glob('*.pdf')))} figures")


if __name__ == "__main__":
    main()
