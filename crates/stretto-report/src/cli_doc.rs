//! The CLI reference in `docs/cli.md`, generated from the clap definitions.
//!
//! [`markdown`] writes one binary's section: what each command does, its
//! usage, and its options, grouped by help heading, each with whether it is
//! required or repeatable, its default, its values, and the options it
//! cannot be combined with. Each binary's tests compare its section of the
//! page with [`check_page`], so the page cannot drift from the code.

use clap::{Arg, ArgAction, Command};
use std::fmt::Write;
use std::path::Path;

/// The environment variable that makes [`check_page`] rewrite a stale
/// section instead of failing.
pub const BLESS: &str = "STRETTO_BLESS";

/// The reference for `cmd`: a table of its commands, then a section per
/// command, or a single section for a command without subcommands.
pub fn markdown(cmd: &Command) -> String {
    let mut cmd = cmd.clone();
    cmd.build();
    let name = cmd.get_name().to_string();
    let mut out = String::new();
    let _ = writeln!(out, "## `{name}`\n");
    if let Some(about) = cmd.get_long_about().or(cmd.get_about()) {
        let _ = writeln!(out, "{}\n", paragraphs(&about.to_string()));
    }
    let subs: Vec<&Command> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help" && !s.is_hide_set())
        .collect();
    if subs.is_empty() {
        options(&mut out, &cmd);
        return out;
    }
    let _ = writeln!(out, "| Command | What it does |\n|---|---|");
    for sub in &subs {
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        let _ = writeln!(
            out,
            "| [`{0}`](#{1}-{0}) | {2} |",
            sub.get_name(),
            name,
            first_sentence(&about).replace('|', "\\|")
        );
    }
    for sub in subs {
        let _ = writeln!(out, "\n### `{name} {}`\n", sub.get_name());
        if let Some(about) = sub.get_long_about().or(sub.get_about()) {
            let _ = writeln!(out, "{}\n", paragraphs(&about.to_string()));
        }
        options(&mut out, sub);
    }
    out
}

/// The command's usage, then its options under their headings.
fn options(out: &mut String, cmd: &Command) {
    let usage = cmd.clone().render_usage().to_string();
    let _ = writeln!(out, "```text\n{}\n```", usage.trim());
    let args: Vec<&Arg> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    let mut headings: Vec<&str> = Vec::new();
    for a in &args {
        if !headings.contains(&heading(a)) {
            headings.push(heading(a));
        }
    }
    for h in headings {
        let _ = writeln!(out, "\n**{h}**\n");
        for a in args.iter().filter(|a| heading(a) == h) {
            let _ = writeln!(out, "- {}", item(cmd, a));
        }
    }
}

/// The heading an argument is listed under, as in `--help`.
fn heading(a: &Arg) -> &str {
    a.get_help_heading().unwrap_or(if a.is_positional() {
        "Arguments"
    } else {
        "Options"
    })
}

/// One option: its form, what it takes, and its help.
fn item(cmd: &Command, a: &Arg) -> String {
    let value = || {
        let names: Vec<String> = a
            .get_value_names()
            .map(|n| n.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec![a.get_id().as_str().to_uppercase()]);
        let many = a.get_num_args().is_some_and(|n| n.max_values() > 1);
        let names: Vec<String> = names.iter().map(|n| format!("<{n}>")).collect();
        format!("{}{}", names.join(" "), if many { "..." } else { "" })
    };
    let takes_value = !matches!(
        a.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    );
    let form = if a.is_positional() {
        value()
    } else {
        let mut f = match (a.get_short(), a.get_long()) {
            (Some(s), Some(l)) => format!("-{s}, --{l}"),
            (Some(s), None) => format!("-{s}"),
            (None, Some(l)) => format!("--{l}"),
            (None, None) => a.get_id().to_string(),
        };
        if takes_value {
            f = format!("{f} {}", value());
        }
        f
    };
    let mut notes: Vec<String> = Vec::new();
    if a.is_required_set() {
        notes.push("required".to_string());
    }
    if matches!(a.get_action(), ArgAction::Append) && !a.is_positional() {
        notes.push("repeatable".to_string());
    }
    let values: Vec<String> = a
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| format!("`{}`", v.get_name()))
        .collect();
    if takes_value && !values.is_empty() {
        notes.push(format!("one of {}", values.join(", ")));
    }
    let defaults: Vec<String> = a
        .get_default_values()
        .iter()
        .map(|d| format!("`{}`", d.to_string_lossy()))
        .collect();
    if takes_value && !defaults.is_empty() {
        notes.push(format!("default {}", defaults.join(", ")));
    }
    // Either side may declare a conflict; clap refuses the pair both ways.
    let conflicts: Vec<String> = cmd
        .get_arguments()
        .filter(|b| {
            b.get_id() != a.get_id()
                && (cmd
                    .get_arg_conflicts_with(a)
                    .iter()
                    .any(|c| c.get_id() == b.get_id())
                    || cmd
                        .get_arg_conflicts_with(b)
                        .iter()
                        .any(|c| c.get_id() == a.get_id()))
        })
        .filter_map(|c| c.get_long().map(|l| format!("`--{l}`")))
        .collect();
    if !conflicts.is_empty() {
        notes.push(format!("not with {}", conflicts.join(", ")));
    }
    let help = a
        .get_long_help()
        .or(a.get_help())
        .map(|h| paragraphs(&h.to_string()))
        .unwrap_or_default();
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join("; "))
    };
    format!("`{form}`{notes}: {help}")
}

/// Help text on one line: its paragraphs joined, ending with a full stop
/// (clap drops it from one-line help).
fn paragraphs(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.chars().last() {
        Some('.' | '?' | '!' | ':') | None => text,
        Some(_) => format!("{text}."),
    }
}

/// The text up to its first full stop, or its first colon if that comes
/// sooner.
fn first_sentence(text: &str) -> String {
    let text = paragraphs(text);
    let stop = [". ", ": "]
        .iter()
        .filter_map(|p| text.find(p))
        .min()
        .unwrap_or(text.len());
    let sentence = text[..stop].trim_end_matches('.');
    format!("{sentence}.")
}

/// Compare `section` with the part of the page at `path` between
/// `<!-- begin NAME -->` and `<!-- end NAME -->`. With [`BLESS`] set, write
/// it there instead. `docs/review.md`'s examples are checked the same way.
pub fn check_page(path: &Path, name: &str, section: &str) -> Result<(), String> {
    let page = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (begin, end) = (
        format!("<!-- begin {name} -->\n"),
        format!("<!-- end {name} -->"),
    );
    let (Some(start), Some(stop)) = (page.find(&begin), page.find(&end)) else {
        return Err(format!("{} has no `{begin}` … `{end}`", path.display()));
    };
    let start = start + begin.len();
    let body = format!("\n{section}\n");
    if page[start..stop] == body {
        return Ok(());
    }
    if std::env::var_os(BLESS).is_some() {
        let page = format!("{}{body}{}", &page[..start], &page[stop..]);
        return std::fs::write(path, page).map_err(|e| format!("{}: {e}", path.display()));
    }
    Err(format!(
        "{} is out of date for `{name}`: run `{BLESS}=1 cargo test` and commit it",
        path.display()
    ))
}
