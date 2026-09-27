//! `stretto doctor`: which of stretto's binaries are on PATH and at which
//! versions, whether stretto can keep its files in `~/.stretto`, whether a
//! System-One key is set (never its value), and the flows and recorded
//! sessions it finds there. Nothing here makes a network request.

use crate::flow::Flow;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The binaries that come with `stretto` (all from the `stretto-proxy`
/// package), what each is for, and whether stretto is of use without it.
pub const COMPANIONS: [(&str, &str, bool); 3] = [
    (
        "stretto-proxy",
        "MCP hosts run it to record sessions and serve flows",
        true,
    ),
    ("stretto-procedure", "it runs compiled procedures", false),
    (
        "stretto-mcp-demo",
        "a demo MCP server, which the quickstart uses",
        false,
    ),
];

/// The report, one line per finding, each marked `ok`, `note` or
/// `problem`, with lists under some of them.
#[derive(Debug, Default)]
pub struct Report {
    lines: Vec<String>,
    problems: usize,
}

impl Report {
    /// As it should be.
    pub fn ok(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("ok       {}", text.as_ref()));
    }

    /// Worth knowing; nothing to fix.
    pub fn note(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("note     {}", text.as_ref()));
    }

    /// Something to fix.
    pub fn problem(&mut self, text: impl AsRef<str>) {
        self.problems += 1;
        self.lines.push(format!("problem  {}", text.as_ref()));
    }

    /// An item of a list under the line before.
    pub fn item(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("           {}", text.as_ref()));
    }

    /// How many problems it found.
    pub fn problems(&self) -> usize {
        self.problems
    }

    /// The report as text.
    pub fn render(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }
}

/// The first file named `name` (with the platform's executable suffix) in
/// the directories of `path`, a value of PATH, as a shell would find it.
pub fn which(name: &str, path: &OsStr) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(path)
        .map(|dir| dir.join(&file))
        .find(|p| p.is_file())
}

/// The first line that `binary --version` prints, such as
/// `stretto-proxy 0.1.0`.
pub fn version_of(binary: &Path) -> Option<String> {
    let out = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Each companion binary: found on `path` (a value of PATH) at stretto's
/// own `version`, or where it is instead. `beside` is the directory of the
/// running `stretto`, where the installers put the others too.
pub fn check_binaries(report: &mut Report, version: &str, path: &OsStr, beside: Option<&Path>) {
    for (name, what, needed) in COMPANIONS {
        let install = format!(
            "It comes with stretto (docs/install.md), or `cargo install --locked --git \
             https://github.com/alexnodeland/stretto --tag v{version} stretto-proxy`."
        );
        let Some(found) = which(name, path) else {
            let near = beside
                .map(|dir| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
                .filter(|p| p.is_file());
            let text = match near {
                Some(near) => format!(
                    "{name} is at {}, which is not on PATH ({what}): add its directory to PATH",
                    near.display()
                ),
                None => format!("{name} is not on PATH ({what}). {install}"),
            };
            if needed {
                report.problem(text);
            } else {
                report.note(text);
            }
            continue;
        };
        match version_of(&found) {
            Some(v) if v == format!("{name} {version}") => {
                report.ok(format!("{v} ({})", found.display()))
            }
            Some(v) => report.problem(format!(
                "{v} ({}) is not stretto {version}'s: install them together. {install}",
                found.display()
            )),
            None => report.problem(format!(
                "{name} ({}) does not answer `--version` as stretto {version}'s does. {install}",
                found.display()
            )),
        }
    }
}

/// Whether stretto can keep its files in `dir`, `~/.stretto`: the proxy's
/// session logs and answer cache, and the flows the next steps write there.
pub fn check_dir(report: &mut Report, dir: &Path) {
    if !dir.exists() {
        report.note(format!(
            "{} does not exist yet; `stretto-proxy --record` creates it",
            dir.display()
        ));
        return;
    }
    if !dir.is_dir() {
        report.problem(format!("{} is not a directory", dir.display()));
        return;
    }
    let probe = dir.join(format!(".stretto-doctor-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            report.ok(format!("{} exists and is writable", dir.display()));
        }
        Err(e) => report.problem(format!("{} is not writable: {e}", dir.display())),
    }
}

/// Whether a key for TypeSafe's Jev is configured, from whether
/// `TYPESAFE_API_KEY` and `TYPESAFE_API_KEY_FILE` are set. Only their
/// presence is known here, never the key.
pub fn check_key(report: &mut Report, key: bool, key_file: bool) {
    if key {
        report.ok(
            "TYPESAFE_API_KEY is set (its value is not shown); `stretto doctor --network` \
             asks Jev one question with it",
        );
    } else if key_file {
        report.ok(
            "TYPESAFE_API_KEY_FILE is set, so stretto reads the key from that file; \
             `stretto doctor --network` asks Jev one question with it",
        );
    } else {
        report.note(
            "TYPESAFE_API_KEY is not set. It is optional: recording, `learn --habit-only`, \
             `flow-show`, `flow-diff`, `audit`, `promote` and serving with `--flow-decider \
             habit` or `reach` need no key. With one, stretto asks TypeSafe's Jev: `learn` \
             fits an arbiter, the proxy serves with `--flow-decider arbiter` and runs the \
             confirmation judge, and `phase0 --oracle jev` measures Jev on τ²-bench.",
        );
    }
}

/// What `scan` finds under `~/.stretto`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// Flow files (`*.flow.json`).
    pub flows: Vec<PathBuf>,
    /// Each directory of recorded sessions, with how many and their domains.
    pub sessions: BTreeMap<PathBuf, (usize, BTreeSet<String>)>,
}

/// The flows and the recorded sessions under `root`, down to three levels
/// (`~/.stretto/logs/<domain>/`), leaving the answer cache out.
pub fn scan(root: &Path) -> Found {
    fn walk(dir: &Path, depth: usize, found: &mut Found) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.is_dir() {
                if depth < 3 && name != "oracle-cache" {
                    walk(&path, depth + 1, found);
                }
            } else if name.ends_with(".flow.json") {
                found.flows.push(path);
            } else if stretto_trace::mcp::is_session_log(&path) {
                let entry = found.sessions.entry(dir.to_path_buf()).or_default();
                entry.0 += 1;
                entry
                    .1
                    .insert(domain_of(&path).unwrap_or_else(|| "none".to_string()));
            }
        }
    }
    let mut found = Found::default();
    walk(root, 1, &mut found);
    found
}

/// The domain in a session log's header, its first line.
fn domain_of(log: &Path) -> Option<String> {
    let mut first = String::new();
    std::io::BufReader::new(std::fs::File::open(log).ok()?)
        .read_line(&mut first)
        .ok()?;
    let header: serde_json::Value = serde_json::from_str(&first).ok()?;
    header.get("domain")?.as_str().map(String::from)
}

/// The flows and sessions under `dir`, `~/.stretto`, with paths shown from
/// `home` as `~`.
pub fn check_files(report: &mut Report, dir: &Path, home: &Path) {
    let found = scan(dir);
    let short = |p: &Path| match p.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    };
    if found.flows.is_empty() {
        report.note(format!(
            "no flows in {} yet (`stretto learn` writes one)",
            short(dir)
        ));
    } else {
        report.ok(format!(
            "{} in {}:",
            count(found.flows.len(), "flow"),
            short(dir)
        ));
        for path in &found.flows {
            match Flow::load(path) {
                Ok(flow) => report.item(format!("{}: {}", short(path), describe(&flow))),
                Err(e) => report.item(format!(
                    "{}: this stretto cannot read it ({e:#})",
                    short(path)
                )),
            }
        }
    }
    if found.sessions.is_empty() {
        report.note(format!("no recorded sessions in {} yet", short(dir)));
    } else {
        report.ok(format!("recorded sessions in {}:", short(dir)));
        for (dir, (n, domains)) in &found.sessions {
            let domains: Vec<&str> = domains.iter().map(String::as_str).collect();
            report.item(format!(
                "{}: {}, domain {}",
                short(dir),
                count(*n, "session"),
                domains.join(", ")
            ));
        }
    }
}

/// `n` and the noun, plural unless `n` is 1.
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// A flow in a few words: its domain, what it learned from, how it decides.
pub fn describe(flow: &Flow) -> String {
    let decides = if flow.has_arbiter() {
        "with an arbiter, which asks Jev when served (`--flow-decider habit` needs no key)"
    } else {
        "on its habit alone (serve it with `--flow-decider habit`)"
    };
    let promoted = if flow.promotion().is_some() {
        ", promoted"
    } else {
        ""
    };
    format!(
        "domain {}, from {}, decides {decides}{promoted}",
        flow.domain(),
        count(flow.provenance().habit_episodes, "session")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stretto-doctor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_key_is_reported_by_presence_only() {
        let mut report = Report::default();
        check_key(&mut report, false, false);
        check_key(&mut report, true, false);
        check_key(&mut report, false, true);
        let text = report.render();
        assert!(
            text.contains("TYPESAFE_API_KEY is not set. It is optional"),
            "{text}"
        );
        assert!(text.contains("its value is not shown"), "{text}");
        assert!(text.contains("TYPESAFE_API_KEY_FILE is set"), "{text}");
        assert_eq!(report.problems(), 0);
    }

    #[test]
    fn a_missing_proxy_is_a_problem_and_a_missing_demo_a_note() {
        let empty = temp("path");
        let mut report = Report::default();
        check_binaries(&mut report, "0.1.0", empty.as_os_str(), None);
        let text = report.render();
        assert!(
            text.contains("problem  stretto-proxy is not on PATH"),
            "{text}"
        );
        assert!(
            text.contains("note     stretto-mcp-demo is not on PATH"),
            "{text}"
        );
        assert!(text.contains("--tag v0.1.0 stretto-proxy"), "{text}");
        assert_eq!(report.problems(), 1);
        std::fs::remove_dir_all(&empty).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn binaries_on_path_are_found_and_their_versions_compared() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("bin");
        for (name, version) in [
            ("stretto-proxy", "0.2.0"),
            ("stretto-procedure", "0.1.0"),
            ("stretto-mcp-demo", "0.2.0"),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\necho '{name} {version}'\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([Path::new("/nonexistent"), &dir]).unwrap();
        assert_eq!(
            which("stretto-proxy", &path),
            Some(dir.join("stretto-proxy"))
        );
        assert_eq!(which("stretto-nothing", &path), None);
        let mut report = Report::default();
        check_binaries(&mut report, "0.2.0", &path, None);
        let text = report.render();
        assert!(text.contains("ok       stretto-proxy 0.2.0"), "{text}");
        assert!(
            text.contains("problem  stretto-procedure 0.1.0")
                && text.contains("install them together"),
            "{text}"
        );
        assert_eq!(report.problems(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_scan_finds_flows_and_sessions_but_not_flow_logs_or_the_cache() {
        let root = temp("scan");
        let logs = root.join("logs").join("notes");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::create_dir_all(root.join("oracle-cache")).unwrap();
        let header = |domain: &str| {
            format!("{{\"stretto_mcp_log\":2,\"session\":\"s\",\"domain\":\"{domain}\"}}\n")
        };
        std::fs::write(logs.join("a.jsonl"), header("notes")).unwrap();
        std::fs::write(logs.join("b.jsonl"), header("notes")).unwrap();
        std::fs::write(logs.join("b.flow.jsonl"), "{}\n").unwrap();
        std::fs::write(logs.join("b.confirm.jsonl"), "{}\n").unwrap();
        std::fs::write(root.join("oracle-cache").join("x.jsonl"), header("x")).unwrap();
        std::fs::write(root.join("notes.flow.json"), "{}").unwrap();
        let found = scan(&root);
        assert_eq!(found.flows, [root.join("notes.flow.json")]);
        assert_eq!(
            found.sessions,
            BTreeMap::from([(logs.clone(), (2, BTreeSet::from(["notes".to_string()])))])
        );
        // A file that is not a flow is listed as one this stretto cannot read.
        let mut report = Report::default();
        check_files(&mut report, &root, &root);
        let text = report.render();
        assert!(
            text.contains("~/notes.flow.json: this stretto cannot read it"),
            "{text}"
        );
        assert!(
            text.contains("~/logs/notes: 2 sessions, domain notes"),
            "{text}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_directory_must_be_writable_once_it_exists() {
        let root = temp("dir");
        let mut report = Report::default();
        check_dir(&mut report, &root.join(".stretto"));
        std::fs::create_dir_all(root.join(".stretto")).unwrap();
        check_dir(&mut report, &root.join(".stretto"));
        let text = report.render();
        assert!(text.contains("does not exist yet"), "{text}");
        assert!(text.contains("exists and is writable"), "{text}");
        assert_eq!(std::fs::read_dir(root.join(".stretto")).unwrap().count(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
