//! Staged learning (RFC-001 §3.3 and §3.7): a deployment's flow in two
//! parts. The **committed** flow is the one the proxy serves. The **staged**
//! flow learns from every session as it is recorded, and is scored against
//! the committed flow as it goes. Nothing reaches the proxy until a person
//! commits the staged flow, and every committed version is kept, with the
//! evidence it was committed on, for a rollback.
//!
//! Beside the committed flow `NAME.flow.json` ([`Paths`]):
//!
//! - `NAME.staged.flow.json`, the staged flow, which `stretto stage` learns
//!   from every session so far as `stretto learn --habit-only` would. The
//!   flow's structure (its vocabulary, the features it conditions on, the
//!   habit's concentration, its sites and its bindings' sources) is fitted
//!   to all the sessions at once, so it is learned again, not updated one
//!   session at a time;
//! - `NAME.stage.json`, the staged learner's [`State`]: the sessions learned
//!   from, and each session's [`Entry`], how each flow did on it;
//! - `NAME.history/`, every committed version: `N.flow.json`, and its
//!   [`Record`], `N.json`.
//!
//! **Scored before it is learned from.** Each new session is scored by the
//! committed flow, and by the staged flow as it was before the session
//! arrived, as the proxy serves each ([`promote::score_as_served`]): the
//! lookups it would have made, and which of them the agent made later. The
//! staged flow is never scored on a session it learned from, so the
//! [`Comparison`] of the last sessions is out of sample for both flows. In a
//! session the committed flow served, its lookups are there already, and
//! the agent had no reason to make them again: a lookup the proxy made is
//! served, neither used nor a detour, for either flow. So the comparison is
//! exact on sessions recorded in shadow, and, in served ones, wherever the
//! served flow handed back.

use crate::flow::Flow;
use crate::promote::{self, wilson_lower, Scored};
use crate::review;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The format version of `NAME.stage.json`.
pub const STATE_VERSION: u32 = 1;

/// The sessions whose [`Entry`] the state keeps: the most recent.
pub const LEDGER_MAX: usize = 1000;

/// The z of a 90% two-sided interval.
const Z90: f64 = 1.644_853_6;

/// The files of one deployment's flow, beside the committed flow.
#[derive(Clone, Debug, PartialEq)]
pub struct Paths {
    /// The flow the proxy serves: `NAME.flow.json`.
    pub committed: PathBuf,
    /// The flow that learns: `NAME.staged.flow.json`.
    pub staged: PathBuf,
    /// The staged learner's state: `NAME.stage.json`.
    pub state: PathBuf,
    /// Every committed version: `NAME.history/`.
    pub history: PathBuf,
}

impl Paths {
    /// The files beside the committed flow `committed`, which must be named
    /// `NAME.flow.json`, and not be a staged flow itself.
    pub fn of(committed: &Path) -> Result<Paths> {
        let name = committed
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(stem) = name.strip_suffix(".flow.json").filter(|s| !s.is_empty()) else {
            bail!(
                "{}: name the committed flow NAME.flow.json, as stretto init does",
                committed.display()
            );
        };
        if stem.ends_with(".staged") {
            bail!(
                "{} is a staged flow: name the committed one, {}.flow.json",
                committed.display(),
                stem.trim_end_matches(".staged")
            );
        }
        let beside = |suffix: &str| committed.with_file_name(format!("{stem}{suffix}"));
        Ok(Paths {
            committed: committed.to_path_buf(),
            staged: beside(".staged.flow.json"),
            state: beside(".stage.json"),
            history: beside(".history"),
        })
    }

    /// Version `n`'s flow in the history.
    pub fn version(&self, n: u32) -> PathBuf {
        self.history.join(format!("{n}.flow.json"))
    }

    fn record(&self, n: u32) -> PathBuf {
        self.history.join(format!("{n}.json"))
    }
}

/// What the staged learner keeps between runs (`NAME.stage.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// The format version: [`STATE_VERSION`].
    pub stretto_stage: u32,
    /// The domain of both flows.
    pub domain: String,
    /// The sessions the staged flow learned from, by id, in the order they
    /// were taken in.
    pub sessions: Vec<String>,
    /// How each flow did on each session, as it arrived: the most recent
    /// [`LEDGER_MAX`].
    pub ledger: Vec<Entry>,
    /// The last run's comparison, which `stretto flow-commit` records as
    /// the evidence a commit rested on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<Comparison>,
}

impl State {
    /// A state with nothing learned yet.
    pub fn new(domain: &str) -> State {
        State {
            stretto_stage: STATE_VERSION,
            domain: domain.to_string(),
            ..State::default()
        }
    }

    /// The state at `path`, if there is one.
    pub fn load(path: &Path) -> Result<Option<State>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let state: State =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if state.stretto_stage != STATE_VERSION {
            bail!(
                "{}: stage format {} is not supported (this build reads {STATE_VERSION})",
                path.display(),
                state.stretto_stage
            );
        }
        Ok(Some(state))
    }

    /// Write the state to `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &serde_json::to_vec(self)?)
    }

    /// Add a session's entry, keeping the most recent [`LEDGER_MAX`].
    pub fn record(&mut self, entry: Entry) {
        self.ledger.push(entry);
        let over = self.ledger.len().saturating_sub(LEDGER_MAX);
        self.ledger.drain(..over);
    }
}

/// One session, as each flow did on it when it arrived.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The session's id.
    pub session: String,
    /// The committed flow's lookups; absent when there was none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed: Option<Side>,
    /// The staged flow's, as it was before it learned from the session;
    /// absent before the staged flow was first learned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<Side>,
}

/// One flow's lookups in one session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Side {
    /// Per site.
    pub sites: BTreeMap<String, Count>,
    /// Decisions left out: the System-One model gave no answer.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unanswered: usize,
}

impl From<&Scored> for Side {
    fn from(scored: &Scored) -> Self {
        Side {
            sites: scored
                .sites
                .iter()
                .map(|(site, t)| {
                    let count = Count {
                        decisions: t.decisions,
                        lookups: t.lookups,
                        used: t.used,
                        served: t.served,
                    };
                    (site.clone(), count)
                })
                .collect(),
            unanswered: scored.unanswered,
        }
    }
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A flow's decisions and lookups at a site, or in all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Count {
    /// Decisions the flow made.
    pub decisions: usize,
    /// The lookups it would have made.
    pub lookups: usize,
    /// Of those, the ones the agent made in a later LLM turn.
    pub used: usize,
    /// Of the rest, the ones the proxy had made, serving a flow: neither
    /// used nor detours.
    pub served: usize,
}

impl Count {
    /// The lookups the agent never made: detours.
    pub fn detours(&self) -> usize {
        self.lookups - self.used - self.served
    }

    /// The share of the lookups whose use is known that the agent made,
    /// with its 90% interval (Wilson); none without such lookups.
    pub fn used_share(&self) -> Option<(f64, f64, f64)> {
        let known = self.lookups - self.served;
        (known > 0).then(|| {
            let lower = wilson_lower(self.used, known, Z90);
            let upper = 1.0 - wilson_lower(known - self.used, known, Z90);
            (self.used as f64 / known as f64, lower, upper)
        })
    }

    fn add(&mut self, other: &Count) {
        self.decisions += other.decisions;
        self.lookups += other.lookups;
        self.used += other.used;
        self.served += other.served;
    }
}

/// The committed and the staged flow's counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub committed: Count,
    pub staged: Count,
}

/// How the staged flow compares with the committed one: what `stretto
/// stage` reports, and a commit records.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The domain.
    pub domain: String,
    /// When the staged flow was learned, in milliseconds since the Unix
    /// epoch.
    pub learned_unix_ms: u64,
    /// The sessions the staged flow learned from.
    pub sessions: usize,
    /// Of those, the ones this run took in.
    pub new: usize,
    /// Whether there was a committed flow to compare with.
    pub committed: bool,
    /// The sessions compared: the last ones both flows were scored on as
    /// they arrived (the staged flow alone, without a committed one), at
    /// most the window asked for.
    pub compared: usize,
    /// Per site, both flows' counts on those sessions.
    pub sites: BTreeMap<String, Pair>,
    /// Over every site.
    pub total: Pair,
    /// Decisions left out, the committed flow's and the staged's: the
    /// System-One model gave no answer.
    pub unanswered: [usize; 2],
    /// What the staged flow changes, as `stretto flow-diff` lists it,
    /// heading by heading; empty without a committed flow.
    pub changes: Vec<review::DiffSection>,
    /// The changes a reviewer must look at (`flow-diff`'s).
    pub needs_review: Vec<String>,
    /// What the staged flow took from the committed one rather than learned.
    pub carried: Vec<String>,
}

/// Compare the flows on the ledger's last `window` sessions that both were
/// scored on (the staged flow alone when `committed` is false).
pub fn compare(state: &State, window: usize, committed: bool) -> Comparison {
    let mut cmp = Comparison {
        domain: state.domain.clone(),
        sessions: state.sessions.len(),
        committed,
        ..Comparison::default()
    };
    let entries = state
        .ledger
        .iter()
        .rev()
        .filter(|e| e.staged.is_some() && (!committed || e.committed.is_some()))
        .take(window);
    for entry in entries {
        cmp.compared += 1;
        let sides = [(&entry.committed, 0), (&entry.staged, 1)];
        for (side, i) in sides {
            let Some(side) = side else { continue };
            cmp.unanswered[i] += side.unanswered;
            for (site, count) in &side.sites {
                let pair = cmp.sites.entry(site.clone()).or_default();
                let (to, total) = match i {
                    0 => (&mut pair.committed, &mut cmp.total.committed),
                    _ => (&mut pair.staged, &mut cmp.total.staged),
                };
                to.add(count);
                total.add(count);
            }
        }
    }
    cmp
}

/// A share and its interval, as the report writes it.
fn share(count: &Count) -> String {
    match count.used_share() {
        Some((share, lower, upper)) => format!(
            "{} ({:.0}%, {:.0}–{:.0}%)",
            count.used,
            100.0 * share,
            100.0 * lower,
            100.0 * upper
        ),
        None => "—".to_string(),
    }
}

impl Comparison {
    /// The report `stretto stage` writes (Markdown). `flow` names the
    /// committed flow's file in the commands it suggests.
    pub fn markdown(&self, flow: &str) -> String {
        let mut md = String::new();
        let _ = writeln!(md, "# The staged {} flow\n", self.domain);
        let _ = writeln!(
            md,
            "It learned from {} sessions, {} of them new in this run.",
            self.sessions, self.new
        );
        if !self.carried.is_empty() {
            let _ = writeln!(
                md,
                "From the committed flow it keeps {}.",
                self.carried.join(", ")
            );
        }
        let _ = writeln!(md);
        let flows = if self.committed {
            "both flows"
        } else {
            "the staged flow"
        };
        if self.compared == 0 {
            let _ = writeln!(
                md,
                "No session has been scored by {flows} yet: each session is scored as it arrives, before the staged flow learns from it, from the second run on.\n"
            );
        } else {
            let _ = writeln!(
                md,
                "## The last {} sessions\n\nEach session was scored by {flows} as it arrived, before the staged flow learned from it: the lookups each would have made there as served, and how many of them the agent made in a later LLM turn (used), with a 90% interval on the share; the rest are detours.\n",
                self.compared
            );
            if self.committed {
                let _ = writeln!(
                    md,
                    "| After | Committed: lookups | used | detours | Staged: lookups | used | detours |\n|---|---|---|---|---|---|---|"
                );
            } else {
                let _ = writeln!(
                    md,
                    "| After | Staged: lookups | used | detours |\n|---|---|---|---|"
                );
            }
            let rows = self
                .sites
                .iter()
                .map(|(site, pair)| (format!("`{site}`"), pair))
                .chain([("All".to_string(), &self.total)]);
            for (site, pair) in rows {
                let (c, s) = (&pair.committed, &pair.staged);
                if self.committed {
                    let _ = writeln!(
                        md,
                        "| {site} | {} | {} | {} | {} | {} | {} |",
                        c.lookups,
                        share(c),
                        c.detours(),
                        s.lookups,
                        share(s),
                        s.detours()
                    );
                } else {
                    let _ = writeln!(
                        md,
                        "| {site} | {} | {} | {} |",
                        s.lookups,
                        share(s),
                        s.detours()
                    );
                }
            }
            let _ = writeln!(md);
            let served = [self.total.committed.served, self.total.staged.served];
            if served.iter().any(|&n| n > 0) {
                let _ = writeln!(
                    md,
                    "The proxy had already made {} of the committed flow's lookups and {} of the staged flow's, serving a flow: they count as neither used nor detours, since the agent had no reason to make them again.\n",
                    served[0], served[1]
                );
            }
            if self.unanswered.iter().any(|&n| n > 0) {
                let _ = writeln!(
                    md,
                    "The System-One model gave no answer at {} of the committed flow's decisions and {} of the staged flow's, which are left out.\n",
                    self.unanswered[0], self.unanswered[1]
                );
            }
        }
        if self.committed {
            let _ = writeln!(md, "## What committing it would change\n");
            if self.changes.is_empty() {
                let _ = writeln!(
                    md,
                    "Nothing `stretto flow-diff` lists: its lookups, bindings and where it acts are the committed flow's.\n"
                );
            } else {
                for section in &self.changes {
                    let _ = writeln!(md, "### {}\n", section.title);
                    for change in &section.changes {
                        let _ = writeln!(md, "- {change}");
                    }
                    let _ = writeln!(md);
                }
                if !self.needs_review.is_empty() {
                    let _ = writeln!(
                        md,
                        "{} of these need review: the flow may call a tool, make a lookup, bind an argument from a source, or ask a model or a question it did not before.\n",
                        self.needs_review.len()
                    );
                }
            }
        }
        let _ = writeln!(
            md,
            "Commit it with `stretto flow-commit --flow {flow}`; the committed flow's versions are kept for `stretto flow-rollback`."
        );
        md
    }
}

/// What a version of the committed flow was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The committed flow as it was found: before its first commit, or
    /// after it was changed by other means than a commit.
    Found,
    /// A staged flow, committed.
    Commit,
    /// An earlier version, restored.
    Rollback,
}

/// One version of the committed flow (`NAME.history/N.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// The version, from 1.
    pub version: u32,
    pub kind: Kind,
    /// When it became the committed flow, in milliseconds since the Unix
    /// epoch.
    pub unix_ms: u64,
    /// For a rollback, the version it restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored: Option<u32>,
    /// Why, in the committer's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// What changed from the version before, as `stretto flow-diff` lists
    /// it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    /// For a commit, the comparison it rested on: the staged learner's last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Comparison>,
}

/// Every version of the committed flow, the oldest first.
pub fn history(paths: &Paths) -> Result<Vec<Record>> {
    let entries = match std::fs::read_dir(&paths.history) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("listing {}", paths.history.display())),
    };
    let mut records = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let Some(n) = name.strip_suffix(".json") else {
            continue;
        };
        if n.parse::<u32>().is_err() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let record: Record =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        records.push(record);
    }
    records.sort_by_key(|r| r.version);
    Ok(records)
}

/// Write `bytes` to `path` whole or not at all: a proxy that starts while
/// a flow is being committed reads the old flow or the new one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Keep `bytes` as the next version, with its record; the record's
/// version is set here.
fn keep(
    paths: &Paths,
    versions: &mut Vec<Record>,
    bytes: &[u8],
    mut record: Record,
) -> Result<Record> {
    record.version = versions.last().map_or(1, |r| r.version + 1);
    write_atomic(&paths.version(record.version), bytes)?;
    let text = serde_json::to_vec_pretty(&record)?;
    write_atomic(&paths.record(record.version), &text)?;
    versions.push(record.clone());
    Ok(record)
}

/// The committed flow's bytes, if there is one.
fn read_committed(paths: &Paths) -> Result<Option<Vec<u8>>> {
    match std::fs::read(&paths.committed) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", paths.committed.display())),
    }
}

/// Whether the committed flow is not the history's latest version: it was
/// never committed through `stretto flow-commit`, or was changed since by
/// other means. False without a committed flow.
pub fn unrecorded(paths: &Paths, versions: &[Record]) -> Result<bool> {
    let Some(current) = read_committed(paths)? else {
        return Ok(false);
    };
    differs(paths, versions, &current)
}

/// Whether `current` is not the history's latest version.
fn differs(paths: &Paths, versions: &[Record], current: &[u8]) -> Result<bool> {
    let Some(latest) = versions.last() else {
        return Ok(true);
    };
    let path = paths.version(latest.version);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(bytes != current)
}

/// Keep `current`, the committed flow as it was found, as a version of its
/// own.
fn keep_found(paths: &Paths, versions: &mut Vec<Record>, current: &[u8], now: u64) -> Result<()> {
    let changes = match versions.last() {
        Some(r) => changes(
            &Flow::load(&paths.version(r.version))?,
            &Flow::from_json(&String::from_utf8_lossy(current))?,
        ),
        None => Vec::new(),
    };
    let found = Record {
        version: 0,
        kind: Kind::Found,
        unix_ms: now,
        restored: None,
        note: None,
        changes,
        evidence: None,
    };
    keep(paths, versions, current, found)?;
    Ok(())
}

/// What changed from `old` to `new`, as `stretto flow-diff` lists it at its
/// defaults.
fn changes(old: &Flow, new: &Flow) -> Vec<String> {
    review::diff(old, new, 0.05, 0.3).changes()
}

/// Make the staged flow the committed one. The committed flow's versions
/// are kept, the new one with `evidence`, the staged learner's last
/// comparison, when it is the staged flow's (it was learned when the
/// comparison says).
pub fn commit(
    paths: &Paths,
    note: Option<String>,
    evidence: Option<Comparison>,
    now: u64,
) -> Result<Record> {
    let staged_bytes = std::fs::read(&paths.staged).with_context(|| {
        format!(
            "reading the staged flow {}: stretto stage learns it",
            paths.staged.display()
        )
    })?;
    let staged = Flow::from_json(&String::from_utf8_lossy(&staged_bytes))
        .with_context(|| format!("parsing {}", paths.staged.display()))?;
    let committed = read_committed(paths)?;
    let changes = match &committed {
        Some(bytes) => {
            let committed = Flow::from_json(&String::from_utf8_lossy(bytes))
                .with_context(|| format!("parsing {}", paths.committed.display()))?;
            if committed.domain() != staged.domain() {
                bail!(
                    "the staged flow is for {}, the committed one for {}",
                    staged.domain(),
                    committed.domain()
                );
            }
            if bytes == &staged_bytes {
                bail!("the staged flow is the committed one: nothing to commit");
            }
            changes(&committed, &staged)
        }
        None => Vec::new(),
    };
    let mut versions = history(paths)?;
    if let Some(bytes) = &committed {
        if differs(paths, &versions, bytes)? {
            keep_found(paths, &mut versions, bytes, now)?;
        }
    }
    let learned = staged.provenance().compiled_unix_ms;
    let record = Record {
        version: 0,
        kind: Kind::Commit,
        unix_ms: now,
        restored: None,
        note,
        changes,
        evidence: evidence.filter(|e| e.learned_unix_ms == learned),
    };
    let record = keep(paths, &mut versions, &staged_bytes, record)?;
    write_atomic(&paths.committed, &staged_bytes)?;
    Ok(record)
}

/// Make version `to` the committed flow again. Without `to`, the version
/// before the committed flow: the one before the latest, or the latest
/// itself when the committed flow was changed since by other means, which
/// is kept as a version first. The rollback is a version of its own, so it
/// can be rolled back too.
pub fn rollback(paths: &Paths, to: Option<u32>, note: Option<String>, now: u64) -> Result<Record> {
    let Some(current) = read_committed(paths)? else {
        bail!(
            "there is no committed flow at {}",
            paths.committed.display()
        );
    };
    let mut versions = history(paths)?;
    let changed = differs(paths, &versions, &current)?;
    let latest = versions.last().map_or(0, |r| r.version);
    let to = match to {
        Some(n) => n,
        None if changed && latest > 0 => latest,
        None if !changed && latest > 1 => latest - 1,
        None => bail!(
            "{} has no earlier version to roll back to",
            paths.committed.display()
        ),
    };
    if !versions.iter().any(|r| r.version == to) {
        bail!(
            "there is no version {to}: {} has versions 1 to {latest}",
            paths.committed.display()
        );
    }
    if to == latest && !changed {
        bail!("version {to} is the committed flow already");
    }
    let bytes = std::fs::read(paths.version(to))
        .with_context(|| format!("reading {}", paths.version(to).display()))?;
    let changes = changes(
        &Flow::from_json(&String::from_utf8_lossy(&current))?,
        &Flow::from_json(&String::from_utf8_lossy(&bytes))?,
    );
    if changed {
        keep_found(paths, &mut versions, &current, now)?;
    }
    let record = Record {
        version: 0,
        kind: Kind::Rollback,
        unix_ms: now,
        restored: Some(to),
        note,
        changes,
        evidence: None,
    };
    let record = keep(paths, &mut versions, &bytes, record)?;
    write_atomic(&paths.committed, &bytes)?;
    Ok(record)
}

/// `YYYY-MM-DD HH:MM UTC` of `ms` (Howard Hinnant's `civil_from_days`).
fn when(ms: u64) -> String {
    const DAY_MS: u64 = 86_400_000;
    let z = (ms / DAY_MS) as i64 + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let minutes = ms % DAY_MS / 60_000;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes / 60,
        minutes % 60
    )
}

/// The history as `stretto flow-log` lists it (Markdown), the latest first.
pub fn log_markdown(flow: &str, versions: &[Record]) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# {flow}\n");
    if versions.is_empty() {
        let _ = writeln!(
            md,
            "No version has been committed: `stretto flow-commit` keeps each."
        );
        return md;
    }
    let latest = versions.last().map_or(0, |r| r.version);
    for r in versions.iter().rev() {
        let what = match r.kind {
            Kind::Found => "found in place".to_string(),
            Kind::Commit => "committed".to_string(),
            Kind::Rollback => format!("rolled back to version {}", r.restored.unwrap_or_default()),
        };
        let current = if r.version == latest {
            " (committed now)"
        } else {
            ""
        };
        let _ = writeln!(
            md,
            "## Version {}{current}\n\n{what}, {}",
            r.version,
            when(r.unix_ms)
        );
        if let Some(note) = &r.note {
            let _ = writeln!(md, "\n> {note}");
        }
        if let Some(e) = &r.evidence {
            let (c, s) = (&e.total.committed, &e.total.staged);
            let _ = write!(
                md,
                "\nOn the last {} sessions before it, the staged flow's lookups: {}, {} used, {} detours",
                e.compared, s.lookups, s.used, s.detours()
            );
            if e.committed {
                let _ = write!(
                    md,
                    "; the committed flow's: {}, {} used, {} detours",
                    c.lookups,
                    c.used,
                    c.detours()
                );
            }
            let _ = writeln!(md, ".");
        }
        if !r.changes.is_empty() {
            let _ = writeln!(md);
            for change in &r.changes {
                let _ = writeln!(md, "- {change}");
            }
        }
        let _ = writeln!(md);
    }
    md
}

/// Score each flow in `flows` on `session` as it is served, for the ledger.
pub fn score(
    flows: [Option<&Flow>; 2],
    session: &promote::Recorded,
    oracle: &dyn stretto_oracle::Oracle,
    serving: impl Fn(&Flow) -> promote::Serving,
) -> Entry {
    let side = |flow: Option<&Flow>| {
        flow.map(|f| {
            let scored =
                promote::score_as_served(f, std::slice::from_ref(session), oracle, serving(f));
            Side::from(&scored)
        })
    };
    Entry {
        session: session.episode.id.clone(),
        committed: side(flows[0]),
        staged: side(flows[1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stretto-stage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn count(lookups: usize, used: usize, served: usize) -> Count {
        Count {
            decisions: lookups + 1,
            lookups,
            used,
            served,
        }
    }

    fn side(site: &str, c: Count, unanswered: usize) -> Side {
        Side {
            sites: BTreeMap::from([(site.to_string(), c)]),
            unanswered,
        }
    }

    #[test]
    fn the_files_sit_beside_the_committed_flow() {
        let p = Paths::of(Path::new("/d/shop.flow.json")).unwrap();
        assert_eq!(p.staged, Path::new("/d/shop.staged.flow.json"));
        assert_eq!(p.state, Path::new("/d/shop.stage.json"));
        assert_eq!(p.version(3), Path::new("/d/shop.history/3.flow.json"));
        for bad in ["/d/.flow.json", "/d/shop.json", "/"] {
            assert!(Paths::of(Path::new(bad)).is_err(), "{bad}");
        }
        let staged = Paths::of(Path::new("shop.staged.flow.json")).unwrap_err();
        assert!(format!("{staged}").contains("name the committed one, shop.flow.json"));
    }

    #[test]
    fn a_state_is_read_back_and_keeps_the_last_sessions() {
        let dir = temp("state");
        let path = dir.join("shop.stage.json");
        assert_eq!(State::load(&path).unwrap(), None);
        let mut state = State::new("shop");
        for i in 0..LEDGER_MAX + 2 {
            state.record(Entry {
                session: i.to_string(),
                ..Entry::default()
            });
        }
        assert_eq!(state.ledger.len(), LEDGER_MAX);
        assert_eq!(state.ledger[0].session, "2");
        state.save(&path).unwrap();
        assert_eq!(State::load(&path).unwrap(), Some(state));
        std::fs::write(
            &path,
            "{\"stretto_stage\": 9, \"domain\": \"shop\", \"sessions\": [], \"ledger\": []}",
        )
        .unwrap();
        let error = format!("{:#}", State::load(&path).unwrap_err());
        assert!(error.contains("stage format 9 is not supported"), "{error}");
        std::fs::write(&path, "not json").unwrap();
        assert!(format!("{:#}", State::load(&path).unwrap_err()).contains("parsing"));
        // A directory where the state should be.
        assert!(format!("{:#}", State::load(&dir).unwrap_err()).contains("reading"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_comparison_takes_the_last_sessions_both_flows_were_scored_on() {
        let mut state = State::new("shop");
        state.sessions = (0..4).map(|i| i.to_string()).collect();
        let entry = |session: &str, committed: Option<Side>, staged: Option<Side>| Entry {
            session: session.to_string(),
            committed,
            staged,
        };
        state.record(entry("0", None, None));
        state.record(entry("1", Some(side("a", count(2, 2, 0), 0)), None));
        state.record(entry(
            "2",
            Some(side("a", count(2, 1, 1), 0)),
            Some(side("a", count(3, 1, 0), 1)),
        ));
        state.record(entry(
            "3",
            Some(side("a", count(1, 1, 0), 2)),
            Some(side("b", count(1, 0, 0), 0)),
        ));
        let cmp = compare(&state, 50, true);
        assert_eq!(cmp.compared, 2);
        assert_eq!(cmp.sessions, 4);
        assert_eq!(cmp.total.committed, count(3, 2, 1).tap(|c| c.decisions = 5));
        assert_eq!(cmp.total.staged.detours(), 3);
        assert_eq!(cmp.unanswered, [2, 1]);
        assert_eq!(cmp.sites["b"].committed, Count::default());
        let md = cmp.markdown("shop.flow.json");
        assert!(
            md.contains("| After | Committed: lookups | used | detours | Staged"),
            "{md}"
        );
        assert!(
            md.contains("| `a` | 3 | 2 (100%, 43–100%) | 0 | 3 | 1 (33%, 8–75%) | 2 |"),
            "{md}"
        );
        assert!(
            md.contains("already made 1 of the committed flow's lookups and 0"),
            "{md}"
        );
        // The window, and the staged flow alone.
        assert_eq!(compare(&state, 1, true).compared, 1);
        let alone = compare(&state, 50, false);
        assert_eq!(alone.compared, 2);
        let md = alone.markdown("shop.flow.json");
        assert!(
            md.contains("| After | Staged: lookups | used | detours |"),
            "{md}"
        );
        assert!(md.contains("| `b` | 1 | 0 (0%, 0–73%) | 1 |"), "{md}");
        assert!(
            md.contains("no answer at 2 of the committed flow's decisions and 1"),
            "{md}"
        );
        assert!(!md.contains("What committing it would change"), "{md}");
        assert!(
            md.contains("stretto flow-commit --flow shop.flow.json"),
            "{md}"
        );
        // A site whose lookups were all served has no share.
        assert_eq!(count(2, 0, 2).used_share(), None);
        assert_eq!(share(&count(2, 0, 2)), "—");
    }

    trait Tap: Sized {
        fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
    impl Tap for Count {}

    #[test]
    fn the_report_lists_the_changes_and_those_that_need_review() {
        let mut cmp = Comparison {
            domain: "shop".to_string(),
            sessions: 3,
            new: 1,
            committed: true,
            carried: vec!["its arbiter".to_string()],
            ..Comparison::default()
        };
        let md = cmp.markdown("f.flow.json");
        assert!(
            md.contains("No session has been scored by both flows yet"),
            "{md}"
        );
        assert!(
            md.contains("From the committed flow it keeps its arbiter."),
            "{md}"
        );
        assert!(md.contains("Nothing `stretto flow-diff` lists"), "{md}");
        cmp.changes = vec![review::DiffSection {
            title: "Lookups".to_string(),
            changes: vec!["after `a`: may look up `b`".to_string()],
        }];
        cmp.needs_review = vec!["after `a`: may look up `b`".to_string()];
        let md = cmp.markdown("f.flow.json");
        assert!(
            md.contains("### Lookups\n\n- after `a`: may look up `b`"),
            "{md}"
        );
        assert!(md.contains("1 of these need review"), "{md}");
    }

    #[test]
    fn the_log_says_what_each_version_was() {
        let evidence = |committed| Comparison {
            compared: 4,
            committed,
            total: Pair {
                committed: count(4, 3, 0),
                staged: count(5, 5, 0),
            },
            ..Comparison::default()
        };
        let record = |version, kind, restored, evidence| Record {
            version,
            kind,
            unix_ms: 1_790_000_000_000,
            restored,
            note: None,
            changes: vec!["a change".to_string()],
            evidence,
        };
        let versions = [
            record(1, Kind::Found, None, None),
            record(2, Kind::Commit, None, Some(evidence(false))),
            record(3, Kind::Commit, None, Some(evidence(true))),
            record(4, Kind::Rollback, Some(2), None),
        ];
        let md = log_markdown("f.flow.json", &versions);
        assert!(
            md.contains(
                "## Version 4 (committed now)\n\nrolled back to version 2, 2026-09-21 14:13 UTC"
            ),
            "{md}"
        );
        assert!(md.contains("## Version 1\n\nfound in place"), "{md}");
        assert!(
            md.contains("staged flow's lookups: 5, 5 used, 0 detours.\n"),
            "{md}"
        );
        assert!(
            md.contains("; the committed flow's: 4, 3 used, 1 detours."),
            "{md}"
        );
        assert!(log_markdown("f.flow.json", &[]).contains("No version has been committed"));
        assert_eq!(when(0), "1970-01-01 00:00 UTC");
        assert_eq!(when(951_782_400_000 + 61_000), "2000-02-29 00:01 UTC");
    }

    /// The toy flow, and one whose model differs.
    fn flows() -> (Flow, Vec<u8>, Vec<u8>) {
        let flow = crate::flow::tests::toy_flow();
        let one = serde_json::to_vec(&flow).unwrap();
        let mut two: serde_json::Value = serde_json::to_value(&flow).unwrap();
        two["model"] = serde_json::json!("another-model");
        (flow, one, serde_json::to_vec(&two).unwrap())
    }

    fn error(e: anyhow::Error) -> String {
        format!("{e:#}")
    }

    #[test]
    fn a_committed_flow_keeps_every_version() {
        let dir = temp("versions");
        let paths = Paths::of(&dir.join("shop.flow.json")).unwrap();
        let (_, one, two) = flows();
        assert!(!unrecorded(&paths, &[]).unwrap());
        // The first commit, with no committed flow before it.
        std::fs::write(&paths.staged, &one).unwrap();
        let first = commit(&paths, Some("first".to_string()), None, 1).unwrap();
        assert_eq!(
            (first.version, first.kind, first.changes.len()),
            (1, Kind::Commit, 0)
        );
        assert!(error(commit(&paths, None, None, 2).unwrap_err()).contains("nothing to commit"));
        // A second, with the evidence that is the staged flow's.
        std::fs::write(&paths.staged, &two).unwrap();
        let learned = Flow::from_json(&String::from_utf8_lossy(&two))
            .unwrap()
            .provenance()
            .compiled_unix_ms;
        let evidence = |learned_unix_ms| Comparison {
            learned_unix_ms,
            ..Comparison::default()
        };
        let second = commit(&paths, None, Some(evidence(learned)), 3).unwrap();
        assert_eq!(second.version, 2);
        assert!(second.evidence.is_some() && !second.changes.is_empty());
        assert_eq!(std::fs::read(&paths.committed).unwrap(), two);
        // Changed by hand: kept as found before the rollback, which
        // restores the latest version by default.
        std::fs::write(&paths.committed, &one).unwrap();
        let versions = history(&paths).unwrap();
        assert!(unrecorded(&paths, &versions).unwrap());
        let back = rollback(&paths, None, None, 4).unwrap();
        assert_eq!((back.version, back.restored), (4, Some(2)));
        let versions = history(&paths).unwrap();
        assert_eq!(versions[2].kind, Kind::Found);
        assert!(!unrecorded(&paths, &versions).unwrap());
        // Then the version before, and a named one; an old evidence is
        // dropped.
        assert_eq!(rollback(&paths, None, None, 5).unwrap().restored, Some(3));
        assert_eq!(rollback(&paths, Some(1), None, 6).unwrap().version, 6);
        assert!(error(rollback(&paths, Some(6), None, 7).unwrap_err()).contains("already"));
        assert!(error(rollback(&paths, Some(9), None, 7).unwrap_err()).contains("no version 9"));
        std::fs::write(&paths.staged, &two).unwrap();
        let stale = commit(&paths, None, Some(evidence(learned + 1)), 7).unwrap();
        assert!(stale.evidence.is_none());
        // A version that is gone.
        std::fs::remove_file(paths.version(1)).unwrap();
        assert!(error(rollback(&paths, Some(1), None, 8).unwrap_err()).contains("reading"));
        let md = log_markdown("shop.flow.json", &history(&paths).unwrap());
        assert!(
            md.contains("## Version 7 (committed now)") && md.contains("> first"),
            "{md}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_flow_committed_by_other_means_is_kept_first() {
        let dir = temp("found");
        let paths = Paths::of(&dir.join("shop.flow.json")).unwrap();
        let (_, one, two) = flows();
        std::fs::write(&paths.committed, &one).unwrap();
        std::fs::write(&paths.staged, &two).unwrap();
        let record = commit(&paths, None, None, 1).unwrap();
        let versions = history(&paths).unwrap();
        let kinds: Vec<Kind> = versions.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [Kind::Found, Kind::Commit]);
        assert!(versions[0].changes.is_empty() && !record.changes.is_empty());
        assert_eq!(std::fs::read(paths.version(1)).unwrap(), one);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_cannot_be_committed_or_rolled_back_says_why() {
        let dir = temp("commit");
        let paths = Paths::of(&dir.join("shop.flow.json")).unwrap();
        let (flow, one, two) = flows();
        assert!(
            error(commit(&paths, None, None, 1).unwrap_err()).contains("stretto stage learns it")
        );
        assert!(error(rollback(&paths, None, None, 1).unwrap_err())
            .contains("there is no committed flow"));
        std::fs::write(&paths.staged, "not a flow").unwrap();
        assert!(error(commit(&paths, None, None, 1).unwrap_err()).contains("parsing"));
        // Nothing to roll back to, with the committed flow alone.
        std::fs::write(&paths.committed, &one).unwrap();
        assert!(error(rollback(&paths, None, None, 1).unwrap_err()).contains("no earlier version"));
        assert!(unrecorded(&paths, &[]).unwrap());
        // A committed flow that does not parse, or is for another domain.
        std::fs::write(&paths.staged, &two).unwrap();
        std::fs::write(&paths.committed, "not a flow").unwrap();
        assert!(error(commit(&paths, None, None, 1).unwrap_err()).contains("parsing"));
        let mut other: serde_json::Value = serde_json::to_value(&flow).unwrap();
        other["manifest"]["domain"] = serde_json::json!("airline");
        std::fs::write(&paths.committed, other.to_string()).unwrap();
        assert!(error(commit(&paths, None, None, 1).unwrap_err())
            .contains("the staged flow is for retail, the committed one for airline"));
        // A commit that fails keeps nothing.
        assert!(!paths.history.exists());
        // The history is not a directory, or holds what is not a record.
        std::fs::write(&paths.history, "").unwrap();
        assert!(error(history(&paths).unwrap_err()).contains("listing"));
        std::fs::remove_file(&paths.history).unwrap();
        std::fs::create_dir_all(paths.history.join("x.json")).unwrap();
        std::fs::write(paths.history.join("notes.txt"), "").unwrap();
        assert_eq!(history(&paths).unwrap(), []);
        std::fs::create_dir_all(paths.history.join("2.json")).unwrap();
        assert!(error(history(&paths).unwrap_err()).contains("reading"));
        std::fs::remove_dir_all(paths.history.join("2.json")).unwrap();
        std::fs::write(paths.history.join("1.json"), "{}").unwrap();
        assert!(error(history(&paths).unwrap_err()).contains("parsing"));
        // The committed flow is a directory.
        std::fs::remove_dir_all(&paths.history).unwrap();
        std::fs::remove_file(&paths.committed).unwrap();
        std::fs::create_dir_all(&paths.committed).unwrap();
        assert!(error(rollback(&paths, None, None, 1).unwrap_err()).contains("reading"));
        // A latest version that is gone.
        std::fs::remove_dir_all(&paths.committed).unwrap();
        std::fs::write(&paths.staged, &one).unwrap();
        commit(&paths, None, None, 1).unwrap();
        std::fs::remove_file(paths.version(1)).unwrap();
        std::fs::write(&paths.staged, &two).unwrap();
        assert!(error(commit(&paths, None, None, 1).unwrap_err()).contains("reading"));
        assert!(
            error(unrecorded(&paths, &history(&paths).unwrap()).unwrap_err()).contains("reading")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_is_written_whole_or_not_at_all() {
        let dir = temp("atomic");
        write_atomic(&dir.join("a/b.json"), b"x").unwrap();
        assert_eq!(std::fs::read(dir.join("a/b.json")).unwrap(), b"x");
        // Its directory cannot be made; the file cannot be written.
        std::fs::write(dir.join("file"), "").unwrap();
        assert!(
            error(write_atomic(&dir.join("file/c.json"), b"x").unwrap_err()).contains("creating")
        );
        let tmp = dir.join(format!(".d.json.{}.tmp", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(error(write_atomic(&dir.join("d.json"), b"x").unwrap_err()).contains("writing"));
        std::fs::remove_dir_all(&tmp).unwrap();
        std::fs::create_dir_all(dir.join("e.json/full")).unwrap();
        assert!(error(write_atomic(&dir.join("e.json"), b"x").unwrap_err()).contains("writing"));
        write_atomic(Path::new("stretto-stage-test.tmp"), b"x").unwrap();
        std::fs::remove_file("stretto-stage-test.tmp").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
