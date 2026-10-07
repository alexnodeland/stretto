# The math film

A narrated deep dive, for engineers, into the mathematics and the probabilistic programs inside stretto, drawn on the shared stage (`../stage/`) as the explainer is, at the same pace and in the same voice ([`../PACING.md`](../PACING.md)). It renders to `brand/media/math.mp4`, `math.vtt` and `math-poster.png`:

```sh
cd brand
npm install                                                # KaTeX, which typesets the formulas
PYTHON=.venv-voice/bin/python node video/film.mjs math     # after: .venv-voice/bin/python video/narrate.py math-video
node video/film.mjs math --info                            # the timeline the voice sets
node video/film.mjs math --stills 30,95 --width 960        # single frames, to video/out/stills/
```

## How it explains

The film works the way a blackboard does, not a slide deck:

- **Formulas are typeset** (KaTeX), and each symbol keeps one color through the whole film: a probability is petrol, a saved turn's value β periwinkle, a detour's cost δ amber, the prior's weight α lavender, a count ink. These two extra hues are the film's alone; the brand's palette has one accent.
- **Every term is named where it sits**: a bracket under or over it, a leader, and a few words, as the voice reaches it (`annotate`).
- **Derivations are worked in place.** One formula becomes the next, its shared terms travelling to their new places while the rest fades (`morph`): the inequality becomes the threshold, the threshold takes the live costs, the next-step back-off becomes the probability of use's, and q is carried out of its definition into the inequality.
- **Numbers follow what moves.** Where a value sweeps, the formula beside it is typeset again each frame with the value put in (`live`): the expected value as q crosses the threshold; the back-off's top level as α is turned up and down, with every level's bars recomputed from the counts; a binding's fraction and its Beta posterior as the bindings tried count up; a cursor on the threshold sweep reading both curves.
- **The charts are data, not drawing.** `data.js` (written by `data/results.py`) holds the reach round's published rows, the ones the paper's Figures 6, 7 and 9 and Table 5 are drawn from, and fugue's own run of alpha's posterior. `data/` is a small Rust program, its own workspace outside the repository's, that runs stretto-model's code on the console's test fixtures (the quickstart's six sessions): the chain `stretto learn` runs for α, the same posterior on a grid, and the back-off model's counts. Its median, 0.6974, is the alpha in the fixtures' `shop.flow.json`.

```sh
cd brand/video/math/data
cargo run --release -- ../../../../crates/stretto-console/tests/fixtures/home/logs/shop > fugue.json
python3 results.py ../../../../docs/results/reach-2026-09-26.json fugue.json > ../data.js
```

## The chapters

| Chapter | What the frame shows | Source |
|---|---|---|
| `title` | The lockup | |
| `event` | An episode's steps and the reads before its next write, U(x); q_c = Pr(c ∈ U(x) \| x), each term named; q carried into Pr(c next \| x) ≤ q_c, since a reply can come between; then the measured gaps: after a user's details, an order next at 0.64 under the habit and before the write 94% of the time; after an order, a product at 0.08 and 38% | paper §2.3, Proposition 4, §4.2 |
| `safety` | One episode with and without stretto: reads leave the state as it was, so if the agent makes the same calls, every write lands on the same state; Proposition 1 | paper §2.2, Appendix A |
| `rule` | E u = qβ − (1 − q)δ, each term named; the saving and the detour against q at the live costs, a probe sweeping q with the formula's numbers following, crossing at θ*; the threshold derived on the board, then given the live costs (1.56M tokens over 260 saved turns, 43,000 over 17 detours: θ* ≈ 0.30); the counted thresholds of airline (0.13) and telecom (0.12) | paper §2.3, Propositions 3 and 4, §3, Figure 2 |
| `habit` | A step abstracted; p_j, each term named; the back-off ladder for one real history of the quickstart's sessions, level by level (0.17, 0.31, 0.93, 0.99 for `get_order_details`); α turned up to 40 and down to 0.04, every level recomputed; p_j becoming r_j; after an email lookup, the next step summing to 1.00 and the probability of use to 2.99 | paper §2.4; `crates/stretto-model/src/world.rs`; `data/` |
| `fugue` | `alpha_posterior` as the code has it; the prior, the likelihood and their product on a log axis; the chain's 600 draws, in order, filling a histogram against the posterior; its median 0.70 and 5th–95th percentiles 0.26–1.65; the alpha in `shop.flow.json` | `crates/stretto-model/src/alpha.rs`; `data/` |
| `bindings` | ρ = (k + 1)/(n + 2), each term named; its Beta posterior and fraction as the bindings tried count up to the walkthrough's 12 of 14, at that rate; the decision, 1.00 × 0.81 = 0.81 ≥ 0.3 | `crates/stretto-report/src/flow.rs`; `docs/walkthrough.md` |
| `calibration` | What calibrated means; retail's reliability diagram, next step above the diagonal (one bin read out: 2,131 lookups scored 0.52, 93.5% used) and use before the write closer to it; the error per domain moving from the next step's to use before the write's | paper, Figure 6 and Table 2; the round's rows |
| `sweep` | Net saving against the threshold for GLM-5 in retail, both rules, the counted θ*, a cursor reading both; the share of retail's ceiling, 76.2% against 86.4%, with intervals and the +10.2-point difference | paper, Figure 7 and Table 3; the round's rows |
| `live` | Each of the 28 live tasks before and after, 23 of them fewer; −27.9% (19.1–35.9%); turns saved against the agent's own sessions in retail, ten giving 96% of what all do | paper, Tables 4 and 5, Figure 9; the round's rows |
| `audit` | The audit's fugue program, a `Categorical` at `addr!("decision", i)` scored with `ScoreGivenTrace`; the walkthrough's audit, 8 decisions, 0.563 nats per decision; surprise as a formula | `crates/stretto-report/src/audit.rs`; the walkthrough |

Every number on screen carries its scope beside it. The benchmark numbers and the proofs are the paper's; the worked examples are the walkthrough's (the binding's 0.81, the audit) and the quickstart's (the back-off, alpha's posterior). The formulas are the paper's, except where the frame names the code: the binding's chance is the code's, (k + 1)/(n + 2), a shrinkage toward ½, where paper §2.4 writes (k + 2ρ̄)/(n + 2), toward the site's rate. Where a value is turned or counted up to show how a formula responds (α, the bindings tried), only its end points are measured.
