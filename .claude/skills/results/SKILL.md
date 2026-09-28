---
name: results
description: Write up an experiment's results in stretto, updating together the dated results page in docs/results/, the index (docs/results/README.md), the claims ledger (claims.md), the paper (paper/stretto.md), the RFC-001 amendment, the roadmap and every page that quotes a changed number. Use after a replay, live run or analysis produces new numbers, when pre-registering a run, or when a number anywhere must change.
---

# Results, claims and the pages that quote them

A number in stretto lives in one place, the results page that recomputes it, and is quoted, with its scope, everywhere else. A round is written up in one commit, so that no page quotes a number another page has changed. Read the latest round first (for example `git show --stat 5313707`, the frontier round) and follow its shape.

## Before the run: pre-register a live claim

A live claim for a new model family comes with a plan fixed in advance (RFC-001 §3.7): `docs/results/<topic>-YYYY-MM-DD-plan.md`, committed before the first episode, with the tasks, arms, trials, the comparison and the rule that decides whether a saving is established. `docs/results/frontier-2026-09-27-plan.md` is the model. Live runs spend credit: run them only when the owner asks (see CLAUDE.md, "Never").

## The round's page: `docs/results/<topic>-YYYY-MM-DD.md`

- A title that states the finding, an opening paragraph on the question, then `## Findings`: bullets that each give a number with its scope (the model, the benchmark, live, replay or record, and the 95% interval).
- `## Setup`, then sections per model, domain or analysis; `## Deviations from the plan` when there was one; `## Caveats` (or "What these runs cannot show"); `## Published`; and `## Reproduce`, with the commands that recompute every number on the page.
- Beside it: the rows (`<topic>-YYYY-MM-DD.json`), the episode archive (`-episodes.tar.gz` or `.tar.xz`), and any answer bundle (`answers-YYYY-MM-DD-<topic>.jsonl.gz` with its `.md`), so it replays without a key.
- Publish only benchmark episodes, whose customers are synthetic, and leave out account data such as a subscription's usage windows. A real user's sessions are shared only through `stretto redact` (docs/privacy.md).
- Every metric gets a definition, and a proxy says it is a proxy (AGENTS.md, "Honest reports").

## In the same commit

1. **`docs/results/README.md`:** a row under the round's date, `| [Title](page.md) ([rows](...), [episodes](...)) | What it found |`, in the order found.
2. **`docs/results/claims.md`:** when a headline number is new or changed, its row (claim, number with interval, kind of evidence, what recomputes it), and the "What the evidence does not show" bullets it affects.
3. **`paper/stretto.md`:** the abstract and contributions if a headline moved, the table and section in §4, and §6's limitations. Then rebuild the LaTeX manuscript in `paper/latex/` (`make`, which needs pandoc, rsvg-convert and latexmk). If those tools are missing, say that the PDF was not rebuilt.
4. **`docs/rfc/001-habit-compiler.md`:** a new `### 3.N Amendment K: <what the round found> (YYYY-MM-DD)` after the last amendment, with a **Details** line linking the plan, the page and the paper's sections, bullets of what was run and found, and "What this changes": the RFC sections it moves, and **Next**.
5. **`docs/roadmap.md`:** the evidence list, and what is still open.
6. **Every page that quotes a changed number:** `README.md` ("Measured, with its scope"), `brand/messaging.md` (the key messages and the 150-word description), and the site: `website/index.md`, `website/guide/why.md`, `website/guide/concepts/*.md`. Find them with `rg -n '<old number>'`.
7. **The site's research pages:** a page `website/research/<topic>.md` that includes the results page (`<!--@include: ../../docs/results/<page>.md-->`, with `title` and `description` front matter), its entry in the sidebar in `website/.vitepress/config.mts`, and a link in `website/research/index.md`. Then run `make site`.

## Numbers

- Copy numbers as the page computes them. 27.9% is not "almost 30%", and a replayed number never stands for a live one.
- Keep the kind of evidence with every number: *live* (agents run with the speculator), *replay* (against τ²-bench's environment), *record* (from published trajectories alone, where detours are lower bounds).
- An interval that includes no change means the saving is not established. Say so, as the frontier page says of airline.
- Write as the pages do: calm, exact, short sentences, no hype (brand/messaging.md).

## Check

`make ci`, and `make site` for the research pages. Then `rg -n` the old and new numbers across the repository, to find any page still quoting the old one.
