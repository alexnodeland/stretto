# The manuscript, in LaTeX

`stretto.tex` is the working paper, [`../stretto.md`](../stretto.md), as an arXiv-ready LaTeX manuscript in two columns, and `stretto.pdf` is it built. The Markdown is the source: edit it, then run `make` here, which

1. writes `stretto.tex` from the Markdown and [`template.tex`](template.tex) ([`build.py`](build.py), with pandoc), and `figures/*.pdf` from the paper's SVGs, drawn in their light colors on white, in Latin Modern Sans (`rsvg-convert`);
2. builds `stretto.pdf` with `latexmk` (pdfLaTeX and BibTeX), dated by the Markdown's last commit, so the same source builds the same PDF.

`make arxiv` packs what arXiv needs into `arxiv.tar.gz`: `stretto.tex`, the compiled bibliography (`stretto.bbl`; arXiv runs no BibTeX), [`refs.bib`](refs.bib), [`figure1.tex`](figure1.tex), [`algorithm1.tex`](algorithm1.tex) and `figures/`. Nothing here posts it.

What `build.py` changes on the way:

- **Citations.** The Markdown's labels (`[AWM]`, `[ASI, SkillWeaver]`) become natbib citations of `refs.bib`; `CITES` maps each label to its keys. A bracket whose labels are not all in `CITES` stays as written, and the build fails if anything that looks like a pandoc citation is left.
- **Tables** become floats captioned with the Markdown's own labels (Table 1b), with long headers on two lines and a first column of long labels wrapped. A table fits a column when its rows do, and otherwise spans both; the note under it (a dagger, or "Brackets give ...") travels with it. Columns of words are set left, columns of numbers centered.
- **Figures.** The Mermaid diagram is the TikZ picture in `figure1.tex` (Figure 3), Algorithm 1 is `algorithm1.tex`, and each SVG figure a float, which spans both columns when the SVG is wider than 600 px.
- **Propositions** are boxed, and a paragraph's bold lead-in is set as a run-in head.
- **Sections** lose the Markdown's numbers for LaTeX's, which are the same, and the appendices follow the references.

The SVGs come from two scripts. [`scripts/paper_figures.py`](../../scripts/paper_figures.py) draws Figures 5–7 and 9 from a round's published rows; [`scripts/paper_diagrams.py`](../../scripts/paper_diagrams.py) draws the diagrams (Figures 1 and 11) and the charts drawn from the paper's own tables (Figures 2, 4, 8 and 10), and fails if the text of `../stretto.md` (its figures' captions aside) does not hold a number it draws, in its sentence or table row:

```bash
python3 scripts/paper_figures.py docs/results/reach-2026-09-26.json --benchmarks docs/results/benchmarks-2026-09-27.json --out paper/
python3 scripts/paper_diagrams.py --out paper/
```

It needs pandoc (3.1 or later), `rsvg-convert` (librsvg) and a TeX distribution with `latexmk`, `adjustbox`, `algorithmicx`, `newunicodechar`, `xurl`, `tcolorbox`, `titlesec`, `fancyhdr`, `enumitem` and `dblfloatfix`, with the Latin Modern fonts installed where `rsvg-convert` finds them, so the figures are set in Latin Modern Sans (on Debian and Ubuntu: `pandoc librsvg2-bin latexmk lmodern fonts-lmodern texlive-latex-extra texlive-science texlive-fonts-recommended texlive-pictures`).

If you edit `stretto.tex` by hand, `make` overwrites it the next time the Markdown changes: port the edit to the Markdown, or stop running `build.py`.
