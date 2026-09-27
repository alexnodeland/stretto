# The LaTeX manuscript of ../stretto.md: `make` writes stretto.tex and figures/
# (build.py) and builds stretto.pdf; `make arxiv` packs the sources arXiv
# needs, the bibliography compiled (arXiv runs no BibTeX), into arxiv.tar.gz.
# Needs pandoc, rsvg-convert and a TeX distribution with latexmk.

# The PDF carries the date of the Markdown's last commit, so it rebuilds the same.
SOURCE_DATE_EPOCH ?= $(shell git log -1 --format=%ct -- ../stretto.md 2>/dev/null || date +%s)
export SOURCE_DATE_EPOCH
export FORCE_SOURCE_DATE = 1

SOURCES = stretto.tex refs.bib figure1.tex algorithm1.tex

all: stretto.pdf

stretto.tex: ../stretto.md build.py template.tex $(wildcard ../*.svg)
	python3 build.py

stretto.pdf: $(SOURCES)
	latexmk -pdf -interaction=nonstopmode -halt-on-error -silent stretto.tex

arxiv.tar.gz: stretto.pdf
	tar -czf $@ $(SOURCES) stretto.bbl figures/*.pdf

arxiv: arxiv.tar.gz

clean:
	latexmk -c stretto.tex
	rm -f arxiv.tar.gz

.PHONY: all arxiv clean
