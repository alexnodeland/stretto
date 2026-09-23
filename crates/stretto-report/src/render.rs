//! Markdown rendering of a Phase 0 report.

use crate::phase0::{DomainReport, FeaturedReport, Report, Settings, VariantStats};
use std::fmt::Write as _;
use stretto_model::features::FOLDS;
use stretto_model::projection::{GateKind, Projection, Scenario};
use stretto_model::provenance::Source;
use stretto_model::world::Position;
use stretto_trace::ToolKind;

fn pct(x: f64) -> String {
    format!("{:.0}%", 100.0 * x)
}

fn short_model(m: &str) -> String {
    // Drop a provider prefix such as `openai/`, and date suffixes such as
    // `-20250219` or `-2025-04-14`.
    let m = m.rsplit('/').next().unwrap_or(m);
    let mut parts: Vec<&str> = m.split('-').collect();
    while parts
        .last()
        .is_some_and(|p| p.len() >= 2 && p.chars().all(|c| c.is_ascii_digit()) && parts.len() > 2)
    {
        parts.pop();
    }
    parts.join("-")
}

/// A model's name, marked when it is a transfer target.
fn label(model: &str, target: bool) -> String {
    if target {
        format!("{} *(target)*", short_model(model))
    } else {
        short_model(model)
    }
}

/// What the habit was trained on, in words.
fn sources(st: &Settings) -> String {
    let extra = st
        .sources
        .iter()
        .map(|s| short_model(s))
        .collect::<Vec<_>>()
        .join(", ");
    match (st.baselines, extra.is_empty()) {
        (true, true) => "τ²-bench's published baseline trajectories".to_string(),
        (true, false) => {
            format!("τ²-bench's published baseline trajectories, plus leaderboard runs of {extra}")
        }
        (false, _) => format!("leaderboard runs of {extra}"),
    }
}

/// Render the report.
pub fn markdown(report: &Report) -> String {
    let mut s = String::new();
    let st = &report.settings;
    let _ = writeln!(s, "# stretto Phase 0 report\n");
    let _ = writeln!(
        s,
        "Source: {} (4 trials per task). The habit is a hierarchical Dirichlet back-off model \
         of the agent's next action, learned from the **successful episodes of the official \
         training tasks** and evaluated on the **held-out test tasks**. Coverage uses context \
         length k = {} and needs at least {} training observations of a context before the \
         habit may act on it.\n",
        sources(st),
        st.order,
        st.min_evidence
    );
    if report
        .domains
        .iter()
        .any(|d| d.models.iter().any(|m| m.target))
    {
        let _ = writeln!(
            s,
            "Models marked *(target)* come from τ²-bench leaderboard submissions and are transfer \
             targets: nothing is learned from them, except their own row of each transfer matrix \
             and their own-habit numbers in the model tables.\n"
        );
    }
    for d in &report.domains {
        domain(&mut s, d, report);
    }
    let _ = writeln!(s, "## Reading the numbers\n");
    let _ = writeln!(
        s,
        "- **Bits/step** is the cross-entropy of the agent's actual next action under the \
         habit: 0 means perfectly predictable. **Top-1** is how often the habit's first choice \
         is what the agent did. Actions are tools plus `respond` (a message to the user).\n\
         - **Coverage @ τ** is the share of held-out decisions where the habit's top option \
         has probability ≥ τ; **agree** is how often that option matched the agent there. \
         Agreeing with the agent is not the same as being right: the agent fails some tasks.\n\
         - **Macro-tool headroom** is Σ(n − 1) over runs of n consecutive tool-calling LLM \
         turns, as a share of all LLM turns: an upper bound on the turns `plan_*` macro-tools \
         could remove.\n\
         - **Argument provenance** looks each argument value up, verbatim and lower-cased, in \
         what the user had said and in earlier tool outputs. *Generated* values appear in \
         neither, so the agent had to produce them. *Short* values (< 3 characters) are not \
         attributed.\n\
         - **Code features** are enum-like fields read from JSON tool outputs by code, kept \
         only if they raise the likelihood of *held-out tasks*: a field that merely identifies \
         the task (a user's city) looks predictive on repeated trials and is rejected.\n\
         - **Writes after \"yes\" / any assent** look at the user's most recent message \
         before each write call: the strict proxy needs the word \"yes\", the lenient one also \
         accepts phrases like \"go ahead\" or \"proceed\". The true confirmation rate lies \
         between them; the System-One question \"did the user explicitly confirm?\" will \
         measure it.\n\
         - **Turns saved** (projection) is the share of all LLM turns a `plan_*` flow would \
         remove on held-out episodes: a run of t LLM turns costs min(t, 1 + pauses). The \
         **ceiling** is every run collapsing to one call. A **pause** is a decision inside a run \
         that nobody in the flow may take, or an argument only the LLM can produce.\n\
         - **Validated contexts** are those where the habit's top option matched the agent in at \
         least the stated share of at least the stated number of decisions, drawn from at least \
         the stated number of distinct tasks (a task's trials and models are near-copies), under \
         cross-validation grouped by task.\n\
         - A **risky decision** is one taken inside the flow that differs from the agent: \
         another tool, carrying on when the agent stopped, or another argument value. Handing \
         back early is counted separately, as safe: it costs one LLM turn. **Episodes with a \
         risky decision** stand in, conservatively, for the pass^1 a flow could lose.\n\
         - **Tokens and dollars saved** come from the usage the benchmark recorded for each \
         call: removed turns, plus intermediate tool outputs a collapsed run no longer carries \
         in later prompts, priced at the model's effective input price.\n\
         - **Phase 0b agreement** is how often the System-One model's pick matched the agent's \
         next step (or argument value) on held-out decisions. **Stop vs. go on** scores only \
         whether it handed back when the agent did. **Brier** is the squared error of its whole \
         distribution (0 is perfect, 2 the worst); **ECE** is the gap between the probability it \
         put on its pick and how often the pick was right, averaged over ten bins."
    );
    s
}

fn domain(s: &mut String, d: &DomainReport, report: &Report) {
    let reads = d.tools.values().filter(|k| **k == ToolKind::Read).count();
    let writes = d.tools.values().filter(|k| **k == ToolKind::Write).count();
    let _ = writeln!(s, "## {}\n", d.domain);
    let _ = writeln!(
        s,
        "{} training tasks, {} held-out tasks. {} tools: {} read, {} write, {} other.",
        d.train_tasks,
        d.test_tasks,
        d.tools.len(),
        reads,
        writes,
        d.tools.len() - reads - writes
    );
    match &d.alpha {
        Some(a) => {
            let _ = writeln!(
                s,
                "Back-off concentration α (fugue MH, {} draws): median {:.3}, 90% interval \
                 [{:.3}, {:.3}].\n",
                a.draws, a.median, a.lo, a.hi
            );
        }
        None => {
            let _ = writeln!(
                s,
                "Back-off concentration α fixed at {:.3}.\n",
                d.alpha_used
            );
        }
    }

    let _ = writeln!(s, "### Runs\n");
    let _ = writeln!(
        s,
        "| Agent model | User simulator | Episodes | Success | LLM turns/ep | Tool calls/ep | \
         Writes/ep | Macro-tool headroom | Writes after \"yes\" / any assent |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|");
    for m in &d.models {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {:.1} | {:.1} | {:.2} | {} | {} |",
            label(&m.model, m.target),
            m.user_model
                .as_deref()
                .map_or_else(|| "?".to_string(), short_model),
            m.episodes,
            pct(m.success_rate),
            m.assistant_turns_per_episode,
            m.tool_calls_per_episode,
            m.writes_per_episode,
            pct(m.runs.removable_share()),
            if m.writes == 0 {
                "–".to_string()
            } else {
                format!(
                    "{} / {}",
                    pct(m.writes_after_yes as f64 / m.writes as f64),
                    pct(m.writes_after_assent as f64 / m.writes as f64)
                )
            }
        );
    }
    let _ = writeln!(
        s,
        "| **all** | | | | | | | **{}** | |\n",
        pct(d.pooled.runs.removable_share())
    );

    let _ = writeln!(s, "### Next-action predictability (held-out)\n");
    let mut header = "| Agent model |".to_string();
    let mut rule = "|---|".to_string();
    for k in &report.settings.orders {
        let _ = write!(header, " k={k} bits / top-1 |");
        rule.push_str("---|");
    }
    let _ = writeln!(s, "{header}\n{rule}");
    let row = |name: &str, by_order: &[(usize, stretto_model::EvalStats)]| {
        let mut r = format!("| {name} |");
        for (_, e) in by_order {
            let _ = write!(r, " {:.2} / {} |", e.bits_per_step, pct(e.top1));
        }
        r
    };
    for m in &d.models {
        let _ = writeln!(s, "{}", row(&label(&m.model, m.target), &m.by_order));
    }
    let _ = writeln!(s, "{}\n", row("**all models pooled**", &d.pooled.by_order));

    let _ = writeln!(
        s,
        "### Coverage: decisions the habit could take (k = {})\n",
        report.settings.order
    );
    let mut header = "| Agent model |".to_string();
    let mut rule = "|---|".to_string();
    for t in &report.settings.thresholds {
        let _ = write!(header, " τ={t} cover / agree |");
        rule.push_str("---|");
    }
    let _ = writeln!(s, "{header}\n{rule}");
    let cov_row = |name: &str, points: &[stretto_model::CoveragePoint]| {
        let mut r = format!("| {name} |");
        for p in points {
            let _ = write!(r, " {} / {} |", pct(p.coverage), pct(p.accuracy));
        }
        r
    };
    for m in &d.models {
        let _ = writeln!(s, "{}", cov_row(&label(&m.model, m.target), &m.coverage));
    }
    let _ = writeln!(
        s,
        "{}\n",
        cov_row("**all models pooled**", &d.pooled.coverage)
    );

    let _ = writeln!(
        s,
        "### Where the habit can act (all models pooled, k = {}, τ = {})\n",
        report.settings.order, report.settings.position_threshold
    );
    let _ = writeln!(
        s,
        "| Decision made | Share of decisions | Bits/step | Top-1 | Cover / agree at τ |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|");
    for p in &d.pooled.positions {
        let name = match p.position {
            Position::AfterUser => "right after the user spoke",
            Position::AfterTool => "right after a tool returned",
        };
        let _ = writeln!(
            s,
            "| {} | {} | {:.2} | {} | {} / {} |",
            name,
            pct(p.share),
            p.eval.bits_per_step,
            pct(p.eval.top1),
            pct(p.coverage.coverage),
            pct(p.coverage.accuracy)
        );
    }
    let _ = writeln!(s);

    if let Some(f) = &d.featured {
        featured(s, d, f, report);
    }

    let _ = writeln!(
        s,
        "### Transfer: top-1 when the habit comes from another model (k = {})\n",
        report.settings.order
    );
    let names: Vec<String> = d.models.iter().map(|m| label(&m.model, m.target)).collect();
    let _ = writeln!(s, "| habit from ↓ / tested on → | {} |", names.join(" | "));
    let _ = writeln!(s, "|---|{}", "---|".repeat(names.len()));
    for a in &d.models {
        let cells: Vec<String> = d
            .models
            .iter()
            .map(|b| {
                d.transfer
                    .iter()
                    .find(|c| c.train == a.model && c.test == b.model)
                    .map(|c| pct(c.eval.top1))
                    .unwrap_or_default()
            })
            .collect();
        let _ = writeln!(
            s,
            "| {} | {} |",
            label(&a.model, a.target),
            cells.join(" | ")
        );
    }
    let _ = writeln!(s);

    let _ = writeln!(
        s,
        "### Most common tool runs (successful training episodes, all models)\n"
    );
    let _ = writeln!(s, "| Count | Run |");
    let _ = writeln!(s, "|---|---|");
    for r in &d.top_runs {
        let _ = writeln!(s, "| {} | `{}` |", r.count, r.tools.join(" → "));
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "### Argument provenance (all episodes, all models)\n");
    let _ = writeln!(
        s,
        "| Tool | Kind | Argument | Values | User | Tool output | Both | Generated | Literal | Short |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|---|");
    for p in &d.provenance {
        let share = |src: Source| {
            let c = p.counts.get(&src).copied().unwrap_or(0);
            if c == 0 {
                "".to_string()
            } else {
                pct(c as f64 / p.total as f64)
            }
        };
        let kind = match p.kind {
            Some(ToolKind::Read) => "read",
            Some(ToolKind::Write) => "**write**",
            Some(ToolKind::Generic) => "other",
            None => "?",
        };
        let _ = writeln!(
            s,
            "| `{}` | {} | `{}` | {} | {} | {} | {} | {} | {} | {} |",
            p.tool,
            kind,
            p.arg,
            p.total,
            share(Source::User),
            share(Source::ToolOutput),
            share(Source::Both),
            share(Source::Generated),
            share(Source::Literal),
            share(Source::Short),
        );
    }
    let _ = writeln!(s);
}

fn featured(s: &mut String, d: &DomainReport, f: &FeaturedReport, report: &Report) {
    let st = &report.settings;
    let _ = writeln!(
        s,
        "### What the habit gains from code features and a named intent (all models pooled, k = {})\n",
        st.order
    );
    if f.selected.is_empty() {
        let _ = writeln!(
            s,
            "{} candidate fields in tool outputs; none improved the likelihood of held-out tasks.\n",
            f.candidates
        );
    } else {
        let _ = writeln!(
            s,
            "{} candidate fields in tool outputs. Kept, in order, because each raised the \
             log-likelihood of held-out tasks (5-fold cross-validation grouped by task):\n",
            f.candidates
        );
        let _ = writeln!(s, "| Tool | Field | Values | Gain (nats) |");
        let _ = writeln!(s, "|---|---|---|---|");
        for x in &f.selected {
            let _ = writeln!(
                s,
                "| `{}` | `{}` | {} | {:.1} |",
                x.tool, x.field, x.values, x.gain
            );
        }
        let _ = writeln!(s);
    }
    let _ = writeln!(
        s,
        "The named intent is the set of write tools the episode goes on to call ({} distinct), \
         standing in for what the LLM states when it calls a macro-tool. It is layered on top of \
         the shared model, so rare intents fall back to it.\n",
        f.intents
    );
    let at = |v: &[stretto_model::CoveragePoint], tau: f64| {
        v.iter()
            .find(|c| (c.threshold - tau).abs() < 1e-9)
            .map(|c| format!("{} / {}", pct(c.coverage), pct(c.accuracy)))
            .unwrap_or_default()
    };
    let pos = |v: &[stretto_model::world::PositionStats], p: Position| {
        v.iter()
            .find(|x| x.position == p)
            .map(|x| {
                format!(
                    "{} / {}",
                    pct(x.coverage.coverage),
                    pct(x.coverage.accuracy)
                )
            })
            .unwrap_or_default()
    };
    let k = |v: &[(usize, stretto_model::EvalStats)]| {
        v.iter()
            .find(|(o, _)| *o == st.order)
            .map(|(_, e)| format!("{:.2} | {}", e.bits_per_step, pct(e.top1)))
            .unwrap_or_default()
    };
    let _ = writeln!(
        s,
        "| Habit sees | Bits/step | Top-1 | After a tool: cover / agree at τ={t} | After the user: cover / agree at τ={t} | All: cover / agree at τ=0.8 | at τ=0.9 |",
        t = st.position_threshold
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|");
    let seq = VariantStats {
        alpha: None,
        alpha_used: d.alpha_used,
        by_order: d.pooled.by_order.clone(),
        coverage: d.pooled.coverage.clone(),
        positions: d.pooled.positions.clone(),
    };
    for (name, v) in [
        ("the action sequence", &seq),
        ("+ code features", &f.code),
        ("+ code features + named intent", &f.code_and_intent),
    ] {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} |",
            name,
            k(&v.by_order),
            pos(&v.positions, Position::AfterTool),
            pos(&v.positions, Position::AfterUser),
            at(&v.coverage, 0.8),
            at(&v.coverage, 0.9)
        );
    }
    let _ = writeln!(s);
    projection(s, f, &report.settings);
    shadow_section(s, f);
}

fn projection(s: &mut String, f: &FeaturedReport, settings: &Settings) {
    let Some(first) = f.projection.first() else {
        return;
    };
    let _ = writeln!(
        s,
        "### Projection: what macro-tools would save on held-out tasks\n"
    );
    let _ = writeln!(
        s,
        "Each run of consecutive tool calls is replayed as one `plan_*` call driven by the habit \
         above (code features + named intent). Where the habit may not act, the decision goes \
         either to the LLM (a pause, one turn) or to a System-One model assumed to agree with \
         the agent (the upper bound Phase 0b will test). Generated arguments always pause. \
         Collapsing every run to one call would save {} of LLM turns (the ceiling).\n",
        pct1(first.ceiling_share()),
    );
    let _ = writeln!(
        s,
        "The habit may act either wherever its top option clears a threshold (with at least {} \
         training observations), or only in *validated* contexts: those where its top option \
         matched the agent in at least {} of at least {} decisions from at least {} distinct \
         tasks, under {}-fold cross-validation grouped by task. A habit that hands back too \
         early is safe: the LLM \
         takes the step, at the cost of one turn. A habit that picks another tool, or carries \
         on when the agent stopped, is the risk.\n",
        settings.min_evidence,
        pct0(settings.validated_min_agreement),
        settings.validated_min_n,
        settings.validated_min_tasks,
        FOLDS,
    );
    let _ = writeln!(
        s,
        "| Where the habit may not act | The habit acts | Turns saved | Pauses/ep | Habit decisions/ep | System-One decisions/ep | Habit handed back early (/100 ep) | Habit chose another step (/100 ep) | Episodes where it did |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|");
    for p in &f.projection {
        let n = p.episodes.max(1) as f64;
        let _ = writeln!(
            s,
            "| {} | {} | **{}** | {:.2} | {:.2} | {:.2} | {:.1} | {:.1} | {} |",
            who(p.scenario),
            gate(&p.gate),
            pct1(p.saved_share()),
            p.pauses as f64 / n,
            p.habit_decisions as f64 / n,
            p.oracle_decisions as f64 / n,
            100.0 * p.early_stops as f64 / n,
            100.0 * p.disagreements as f64 / n,
            pct1(p.risky_share())
        );
    }
    let _ = writeln!(s);

    if !f.validated.is_empty() {
        let _ = writeln!(
            s,
            "Validated contexts, and how the habit did in them on held-out tasks:\n"
        );
        let _ = writeln!(
            s,
            "| Last steps (oldest first) | The agent's usual next step | Agreement, cross-validated on training tasks | Agreement on held-out tasks |"
        );
        let _ = writeln!(s, "|---|---|---|---|");
        for v in &f.validated {
            let _ = writeln!(
                s,
                "| {} | {} | {} of {} ({} tasks) | {} |",
                v.context
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(" → "),
                v.action,
                pct1(v.cv_agreed as f64 / v.cv_n.max(1) as f64),
                v.cv_n,
                v.cv_tasks,
                if v.test_n == 0 {
                    "not reached".to_string()
                } else {
                    format!(
                        "{} of {}",
                        pct1(v.test_agreed as f64 / v.test_n as f64),
                        v.test_n
                    )
                },
            );
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(
        s,
        "Tokens and dollars come from the usage the benchmark recorded for every LLM call. A \
         removed turn saves its whole prompt and completion. A run that collapses to one call \
         also stops carrying its intermediate tool outputs in every later prompt; their size is \
         read off the recorded prompt growth, and priced at each model's effective input price, \
         fitted to its recorded costs by least squares (so it absorbs any prompt-caching \
         discount): {}.\n",
        f.prices
            .iter()
            .filter(|m| m.input_per_mtok > 0.0 || m.output_per_mtok > 0.0)
            .map(|m| format!(
                "{} ${:.2} in / ${:.2} out per MTok",
                short_model(&m.model),
                m.input_per_mtok,
                m.output_per_mtok
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let _ = writeln!(
        s,
        "| Where the habit may not act | The habit acts | Turns saved | Input tokens saved | Output tokens saved | $ saved | $ per episode |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|");
    for p in &f.projection {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | **{}** | {} |",
            who(p.scenario),
            gate(&p.gate),
            pct1(p.saved_share()),
            pct1(p.input_saved_share()),
            pct1(p.output_saved_share()),
            pct1(p.cost_saved_share()),
            dollars_per_episode(p),
        );
    }
    let _ = writeln!(s);

    if !f.by_model.is_empty() {
        let _ = writeln!(
            s,
            "By agent model, with the habit acting only in validated contexts and a perfect \
             System-One model deciding the rest. Pooled dollars weigh each model by its spend.{}\n",
            if f.by_model.iter().any(|m| m.target) {
                " Models marked *(target)* were never trained on: the habit, its features, the \
                 closed argument sets and the validated contexts all come from the other models, \
                 and the pooled tables above leave them out."
            } else {
                ""
            }
        );
        let _ = writeln!(
            s,
            "| Agent model | Tool turns with parallel calls | Turns saved | Ceiling | Input tokens saved | Output tokens saved | $ saved | $ per episode |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
        for m in &f.by_model {
            let p = &m.projection;
            let _ = writeln!(
                s,
                "| {} | {} | **{}** | {} | {} | {} | {} | {} |",
                label(&m.model, m.target),
                pct0(m.parallel_share),
                pct1(p.saved_share()),
                pct1(p.ceiling_share()),
                if p.input_tokens > 0.0 {
                    pct1(p.input_saved_share())
                } else {
                    "–".to_string()
                },
                if p.output_tokens > 0.0 {
                    pct1(p.output_saved_share())
                } else {
                    "–".to_string()
                },
                if p.cost > 0.0 {
                    pct1(p.cost_saved_share())
                } else {
                    "–".to_string()
                },
                if p.cost > 0.0 {
                    dollars_per_episode(p)
                } else if p.input_tokens > 0.0 {
                    "no cost recorded".to_string()
                } else {
                    "no usage recorded".to_string()
                },
            );
        }
        let _ = writeln!(s);
    }
}

fn dollars_per_episode(p: &Projection) -> String {
    let n = p.episodes.max(1) as f64;
    format!("{:.4} → {:.4}", p.cost / n, (p.cost - p.cost_saved) / n)
}

fn shadow_section(s: &mut String, f: &FeaturedReport) {
    let Some(sh) = &f.shadow else {
        return;
    };
    let _ = writeln!(s, "### Phase 0b: the System-One model in shadow mode\n");
    if sh.oracle == "mock" {
        let _ = writeln!(
            s,
            "**Mock oracle.** It always picks the first option, so these numbers only check the \
             pipeline; they say nothing about Jev.\n"
        );
    }
    let versions = if sh.versions.is_empty() {
        "none".to_string()
    } else {
        sh.versions.join(", ")
    };
    let _ = writeln!(
        s,
        "At every held-out decision a flow would hand to a System-One model, it was asked a typed \
         question about the state the flow would have there: right after a tool returns, which \
         tool the agent calls next or whether it hands back; and, for tool calls inside a run, \
         the value of each closed-set argument. Oracle: `{}` (answering model: {versions}). \
         {} decisions, {} distinct questions, {} answered, {} failed; {} input tokens \
         (${:.2} at ${}/MTok).\n",
        sh.oracle,
        sh.decisions,
        sh.distinct,
        sh.answered,
        sh.errors,
        sh.input_tokens,
        sh.input_tokens as f64 / 1e6 * crate::shadow::PRICE_PER_MTOK,
        crate::shadow::PRICE_PER_MTOK,
    );
    if let Some(e) = &sh.first_error {
        let _ = writeln!(s, "First failure: `{}`\n", e.replace('`', "'"));
    }
    let t = sh.thresholds.last().copied().unwrap_or(0.9);
    let _ = writeln!(s, "Next step (which tool next, or hand back):\n");
    let _ = writeln!(
        s,
        "| Agent model | Decisions | Agreed | Stop vs. go on | Brier | ECE | p ≥ {t}: share / agreed |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|");
    for row in &sh.rows {
        let a = &row.next;
        let at = a.curve.iter().find(|c| (c.0 - t).abs() < 1e-9);
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {:.3} | {:.3} | {} |",
            shadow_label(row),
            a.n,
            pct1(a.rate()),
            pct1(a.stop_agreed as f64 / a.n.max(1) as f64),
            a.brier,
            a.ece,
            at.map_or("–".to_string(), |c| format!(
                "{} / {}",
                pct1(c.1),
                pct1(c.2)
            )),
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "Closed-set arguments:\n");
    let _ = writeln!(
        s,
        "| Agent model | Decisions | Agreed | p ≥ {t}: share / agreed |"
    );
    let _ = writeln!(s, "|---|---|---|---|");
    for row in &sh.rows {
        let a = &row.args;
        let at = a.curve.iter().find(|c| (c.0 - t).abs() < 1e-9);
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} |",
            shadow_label(row),
            a.n,
            pct1(a.rate()),
            at.map_or("–".to_string(), |c| format!(
                "{} / {}",
                pct1(c.1),
                pct1(c.2)
            )),
        );
    }
    let _ = writeln!(s);
    if !sh.by_arg.is_empty() {
        let _ = writeln!(s, "Closed-set arguments by argument (source models):\n");
        let _ = writeln!(
            s,
            "| Tool | Argument | Decisions | Agreed | p ≥ {t}: share / agreed |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|");
        for row in &sh.by_arg {
            let a = &row.agreement;
            let at = a.curve.iter().find(|c| (c.0 - t).abs() < 1e-9);
            let _ = writeln!(
                s,
                "| `{}` | `{}` | {} | {} | {} |",
                row.tool,
                row.arg,
                a.n,
                pct1(a.rate()),
                at.map_or("–".to_string(), |c| format!(
                    "{} / {}",
                    pct1(c.1),
                    pct1(c.2)
                )),
            );
        }
        let _ = writeln!(s);
    }

    if !sh.projection.is_empty() {
        let _ = writeln!(
            s,
            "Projection with the System-One model, pooled over the source models: the habit acts \
             in validated contexts, the System-One pick is trusted at or above the threshold (with \
             *two keys*, only when it is also the habit's top option), and anything else pauses.\n"
        );
        let _ = writeln!(
            s,
            "| Pick trusted at | Turns saved | Pauses/ep | System-One decisions/ep | Handed back early (/100 ep) | Risky decisions (/100 ep) | Episodes with one |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|");
        for p in &sh.projection {
            let n = p.episodes.max(1) as f64;
            let t = column(p.scenario);
            let _ = writeln!(
                s,
                "| {} | **{}** | {:.2} | {:.2} | {:.1} | {:.1} | {} |",
                t,
                pct1(p.saved_share()),
                p.pauses as f64 / n,
                p.oracle_decisions as f64 / n,
                100.0 * (p.early_stops + p.oracle_early_stops) as f64 / n,
                100.0 * (p.disagreements + p.oracle_disagreements) as f64 / n,
                pct1(p.risky_share()),
            );
        }
        let _ = writeln!(s);
    }

    if f.by_model.iter().any(|m| !m.with_oracle.is_empty()) {
        let _ = writeln!(
            s,
            "The gate, per agent model: at least 20% fewer LLM turns, with at most 1% of episodes \
             holding a risky decision (a conservative stand-in for losing at most one point of \
             pass^1). Each cell is turns saved · episodes with a risky decision.\n"
        );
        let mut header = "| Agent model | Perfect System-One |".to_string();
        let mut rule = "|---|---|".to_string();
        for p in f.by_model.first().map_or(&[][..], |m| &m.with_oracle[..]) {
            let _ = write!(header, " {} |", column(p.scenario));
            rule.push_str("---|");
        }
        let _ = writeln!(s, "{header} Gate |\n{rule}---|");
        for m in &f.by_model {
            let cells: Vec<String> = m
                .with_oracle
                .iter()
                .map(|p| format!("{} · {}", pct1(p.saved_share()), pct1(p.risky_share())))
                .collect();
            let passing = m
                .with_oracle
                .iter()
                .find(|p| p.saved_share() >= 0.2 && p.risky_share() <= 0.01)
                .map(|p| column(p.scenario));
            let gate = match passing {
                Some(rule) => format!("**passes** ({rule})"),
                None if m.projection.saved_share() < 0.2 => {
                    "fails: below 20% even with a perfect System-One model".to_string()
                }
                None => "fails".to_string(),
            };
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} |",
                label(&m.model, m.target),
                pct1(m.projection.saved_share()),
                cells.join(" | "),
                gate
            );
        }
        let _ = writeln!(s);
    }
}

/// A short name for an oracle scenario, for table columns.
fn column(scenario: Scenario) -> String {
    match scenario {
        Scenario::HabitThenOracle(t) => format!("p ≥ {t}"),
        Scenario::TwoKeys(t) => format!("two keys, p ≥ {t}"),
        _ => "–".to_string(),
    }
}

fn shadow_label(row: &crate::phase0::ShadowRow) -> String {
    if row.model == "all source models" {
        "**all source models**".to_string()
    } else {
        label(&row.model, row.target)
    }
}

fn who(scenario: Scenario) -> String {
    match scenario {
        Scenario::HabitOnly => "pause for the LLM".to_string(),
        Scenario::HabitThenPerfectOracle => "ask a perfect System-One model".to_string(),
        Scenario::HabitThenOracle(t) => format!("ask the System-One model, trusted at p ≥ {t}"),
        Scenario::TwoKeys(t) => {
            format!("ask the System-One model, trusted at p ≥ {t} when the habit agrees")
        }
    }
}

fn gate(g: &GateKind) -> String {
    match g {
        GateKind::Threshold(t) => format!("top option ≥ {t}"),
        GateKind::Validated(1) => "in 1 validated context".to_string(),
        GateKind::Validated(k) => format!("in {k} validated contexts"),
    }
}

fn pct0(x: f64) -> String {
    format!("{:.0}%", 100.0 * x)
}

fn pct1(x: f64) -> String {
    format!("{:.1}%", 100.0 * x)
}

#[cfg(test)]
mod tests {
    use super::short_model;

    #[test]
    fn strips_date_suffixes() {
        assert_eq!(
            short_model("claude-3-7-sonnet-20250219"),
            "claude-3-7-sonnet"
        );
        assert_eq!(short_model("gpt-4.1-2025-04-14"), "gpt-4.1");
        assert_eq!(short_model("gpt-4.1-mini-2025-04-14"), "gpt-4.1-mini");
        assert_eq!(short_model("o4-mini-2025-04-16"), "o4-mini");
        assert_eq!(short_model("openai/glm-5-fp8"), "glm-5-fp8");
    }
}
