# The manuscript, in LaTeX

`stretto.tex` is the working paper, [`../stretto.md`](../stretto.md), as an arXiv-ready LaTeX manuscript, and `stretto.pdf` is it built. The Markdown is the source: edit it, then run `make` here, which

1. writes `stretto.tex` from the Markdown and [`template.tex`](template.tex) ([`build.py`](build.py), with pandoc), and `figures/*.pdf` from the paper's SVGs, drawn in their light colors on white, in Latin Modern Sans (`rsvg-convert`);
2. builds `stretto.pdf` with `latexmk` (pdfLaTeX and BibTeX), dated by the Markdown's last commit, so the same source builds the same PDF.

`make arxiv` packs what arXiv needs into `arxiv.tar.gz`: `stretto.tex`, the compiled bibliography (`stretto.bbl`; arXiv runs no BibTeX), [`refs.bib`](refs.bib), [`figure1.tex`](figure1.tex), [`algorithm1.tex`](algorithm1.tex) and `figures/`. Nothing here posts it.

What `build.py` changes on the way:

- **Citations.** The Markdown's labels (`[AWM]`, `[ASI, SkillWeaver]`) become natbib citations of `refs.bib`; `CITES` maps each label to its keys. A bracket whose labels are not all in `CITES` stays as written, and the build fails if anything that looks like a pandoc citation is left.
- **Tables** become floats captioned with the Markdown's own labels (Table 1b), with long headers on two lines and, in wide tables, a first column of long labels wrapped.
- **Figures.** The Mermaid diagram is the TikZ picture in `figure1.tex`, Algorithm 1 is `algorithm1.tex`, and each SVG figure a float.
- **Sections** lose the Markdown's numbers for LaTeX's, which are the same, and the appendices follow the references.

It needs pandoc (3.1 or later), `rsvg-convert` (librsvg) and a TeX distribution with `latexmk`, `adjustbox`, `algorithmicx`, `newunicodechar` and `xurl` (on Debian and Ubuntu: `pandoc librsvg2-bin latexmk texlive-latex-extra texlive-science texlive-fonts-recommended`).

If you edit `stretto.tex` by hand, `make` overwrites it the next time the Markdown changes: port the edit to the Markdown, or stop running `build.py`.
