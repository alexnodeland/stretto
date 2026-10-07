# The math film

A narrated deep dive, for engineers, into the mathematics and the probabilistic programs inside stretto, drawn on the shared stage (`../stage/`) as the explainer is, at the same pace and in the same voice ([`../PACING.md`](../PACING.md)). It renders to `brand/media/math.mp4`, `math.vtt` and `math-poster.png`:

```sh
cd brand
PYTHON=.venv-voice/bin/python node video/film.mjs math     # after: .venv-voice/bin/python video/narrate.py math-video
node video/film.mjs math --info                            # the timeline the voice sets
node video/film.mjs math --stills 30,95 --width 960        # single frames, to video/out/stills/
```

## The chapters

| Chapter | What the frame shows | Source |
|---|---|---|
| `cold` | No words: the rule, q ≥ δ/(β + δ), and a chance sliding across its line | paper, Proposition 3 |
| `event` | An episode's steps, the reads before its next write (U(x)), q_c = Pr(c ∈ U(x) \| x), and that the next-step probability is no larger; after a user's details, the habit put an order next at 0.64, and the agents read one before the next write 94% of the time | paper §2.3, Figure 1, §4.2 |
| `safety` | One episode with and without stretto: reads leave the state as it was, so if the agent makes the same calls, every write lands on the same state; Proposition 1, as the paper states it | paper §2.2, Appendix A |
| `rule` | A lookup's expected value, q·β − (1 − q)·δ, against q, crossing zero at θ* = δ/(β + δ): live (6,000 and 2,530 input tokens, θ* ≈ 0.30), and counted for airline (0.13) and telecom (0.12) | paper §2.3, Propositions 3 and 4, Appendix B |
| `habit` | A step abstracted to its tool, outcome and a feature of its result; the back-off over the last two steps; p_j and the reach decider's r_j | paper §2.4; `crates/stretto-model/src/world.rs` |
| `fugue` | `alpha_posterior`, as the code has it: a Normal(0, 2) prior on log α, the episodes' prequential likelihood as a `factor`, adaptive Metropolis–Hastings; the flow keeps the median | `crates/stretto-model/src/alpha.rs` |
| `bindings` | A binding's chance, (k + 1)/(n + 2), and flow-show's own line: 1.00 × 0.81 = 0.81 | `crates/stretto-report/src/flow.rs`; `docs/walkthrough.md` |
| `calibration` | Expected calibration error per τ²-bench domain, scored on the next step and on use before the next write | paper, Table 2; `docs/results/claims.md` |
| `audit` | The audit's fugue program, a `Categorical` at `addr!("decision", i)` scored with `ScoreGivenTrace`, and the walkthrough's audit: 8 decisions, 0.563 nats per decision | `crates/stretto-report/src/audit.rs`; the walkthrough |

Every number on screen carries its scope beside it. The benchmark numbers and the proofs are the paper's; the worked examples (the binding's 0.81, the audit) are the walkthrough's. The formulas are the paper's, except where the frame names the code: the binding's chance is the code's, (k + 1)/(n + 2), a shrinkage toward ½, where paper §2.4 writes (k + 2ρ̄)/(n + 2), toward the site's rate.
