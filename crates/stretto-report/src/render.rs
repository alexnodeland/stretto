//! Markdown rendering of a Phase 0 report.

use crate::phase0::{DomainReport, FeaturedReport, Report, VariantStats};
use std::fmt::Write as _;
use stretto_model::provenance::Source;
use stretto_model::world::Position;
use stretto_trace::ToolKind;

fn pct(x: f64) -> String {
    format!("{:.0}%", 100.0 * x)
}

fn short_model(m: &str) -> String {
    // Drop date suffixes such as `-20250219` or `-2025-04-14`.
    let mut parts: Vec<&str> = m.split('-').collect();
    while parts
        .last()
        .is_some_and(|p| p.len() >= 2 && p.chars().all(|c| c.is_ascii_digit()) && parts.len() > 2)
    {
        parts.pop();
    }
    parts.join("-")
}

/// Render the report.
pub fn markdown(report: &Report) -> String {
    let mut s = String::new();
    let st = &report.settings;
    let _ = writeln!(s, "# stretto Phase 0 report\n");
    let _ = writeln!(
        s,
        "Source: τ²-bench's published baseline trajectories (4 trials per task). The habit \
         is a hierarchical Dirichlet back-off model of the agent's next action, learned from \
         the **successful episodes of the official training tasks** and evaluated on the \
         **held-out test tasks**. Coverage uses context length k = {} and needs at least {} \
         training observations of a context before the habit may act on it.\n",
        st.order, st.min_evidence
    );
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
         measure it."
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
        "| Agent model | Episodes | Success | LLM turns/ep | Tool calls/ep | Writes/ep | \
         Macro-tool headroom | Writes after \"yes\" / any assent |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
    for m in &d.models {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {:.1} | {:.1} | {:.2} | {} | {} |",
            short_model(&m.model),
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
        "| **all** | | | | | | **{}** | |\n",
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
        let _ = writeln!(s, "{}", row(&short_model(&m.model), &m.by_order));
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
        let _ = writeln!(s, "{}", cov_row(&short_model(&m.model), &m.coverage));
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
    let names: Vec<String> = d.models.iter().map(|m| short_model(&m.model)).collect();
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
        let _ = writeln!(s, "| {} | {} |", short_model(&a.model), cells.join(" | "));
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
    }
}
