#!/usr/bin/env python3
"""Write the LaTeX manuscript from the working paper.

    python3 build.py     # or `make`, which then runs pdflatex and bibtex

reads ../stretto.md and writes stretto.tex (from template.tex) and
figures/*.pdf (from ../*.svg). The Markdown is the source:

- its citation labels ([AWM], [ASI, SkillWeaver]) become natbib citations of
  refs.bib (CITES);
- each table becomes a float, captioned with the label the Markdown gives it
  (Table 1b), and each figure a float;
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
        return ("```{=latex}\n\\begin{figure}[tbp]\n\\centering\n\\input{figure1}\n"
                f"\\caption{{{caption}}}\n\\label{{fig:regimes}}\n\\end{{figure}}\n```\n")

    text, n = re.subn(r"```mermaid\n.*?```\n\n\*Figure 1\. (.+?)\*\n", regimes, text, flags=re.S)
    assert n == body, "the Mermaid figure"
    text, n = re.subn(r"```text\nAlgorithm 1\..*?```\n", lambda m: "```{=latex}\n\\input{algorithm1}\n```\n", text, flags=re.S)
    assert n == body, "Algorithm 1"
    # Figures: the image, captioned by the paragraph after it.
    text, n = re.subn(
        r"!\[[^\]]*\]\(([\w-]+)\.svg\)\n\n\*Figure \d+\. (.+?)\*\n",
        lambda m: f"![{m.group(2)}](figures/{m.group(1)}.pdf)\n",
        text,
    )
    assert n == 4 * body, f"{n} figures"
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
        # A first column of long labels wraps, rather than shrinking the table.
        labels = [re.sub(r"\\[a-zA-Z]+|[{}]", "", row.split("&", 1)[0]).strip() for row in rows.split("\\\\")]
        first = ">{\\raggedright\\arraybackslash}p{4.2cm}" if columns > 3 and max(map(len, labels)) > 32 else "l"
        out += [tex[at:m.start()], "\n".join([
            "\\begin{table}[tbp]",
            "\\centering\\small",
            f"\\renewcommand{{\\thetable}}{{{label.group(1)}}}",
            f"\\caption{{{caption}}}",
            f"\\label{{tab:{label.group(1)}}}",
            "\\begin{adjustbox}{max width=\\linewidth}",
            "\\begin{tabular}{" + first + "c" * (columns - 1) + "}",
            "\\toprule",
            head.rstrip("\n"),
            "\\midrule",
            rows.rstrip("\n"),
            "\\bottomrule",
            "\\end{tabular}",
            "\\end{adjustbox}",
            "\\end{table}",
        ])]
        at = end
    return "".join(out) + tex[at:]


def figures(tex: str) -> str:
    tex = tex.replace("\\begin{figure}\n", "\\begin{figure}[tbp]\n")
    return re.sub(
        r"\\includegraphics(\[[^\]]*\])?\{figures/",
        r"\\includegraphics[width=\\linewidth,height=0.72\\textheight,keepaspectratio]{figures/",
        tex,
    )


def svg_to_pdf(svg: Path, pdf: Path) -> None:
    """The figure in its light colors, on white, in Latin Modern Sans."""
    s = svg.read_text()
    light = dict(re.findall(r"--([\w-]+):(#[0-9a-fA-F]{3,8})", s.split("@media", 1)[0]))
    light["bg"] = "#ffffff"
    s = re.sub(r"@media \(prefers-color-scheme: dark\)\{svg\{[^}]*\}\}", "", s)
    s = re.sub(r"var\(--([\w-]+)\)", lambda m: light[m.group(1)], s)
    s = re.sub(r'font-family="[^"]*"', 'font-family="Latin Modern Sans, DejaVu Sans, sans-serif"', s)
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
        "DATE": f"{day.day} {day:%B %Y}",
        "ABSTRACT": inline(cite(abstract)),
        "BODY": figures(tables(pandoc(prepare(body, True)))),
        "APPENDIX": figures(tables(pandoc(prepare(appendix, False)))),
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
