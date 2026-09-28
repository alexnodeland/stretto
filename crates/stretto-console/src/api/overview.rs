//! The overview: totals, domains, the last 14 days, recent sessions, health
//! checks and recent jobs.

use super::jobs::Job;
use super::sessions::{SessionMode, SessionSummary};
use super::{blocking, ApiResult};
use crate::data::{flows, registry, sessions};
use crate::time::{day, DAY_MS};
use crate::{Shared, State, VERSION};
use axum::extract::State as AxumState;
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;
use stretto_report::doctor::{self, Report};
use ts_rs::TS;

/// Counts over everything in the data directory.
#[derive(Clone, Debug, Default, Serialize, TS)]
pub struct Totals {
    pub sessions: usize,
    /// Sessions started in the last 7 days.
    pub sessions_7d: usize,
    pub domains: usize,
    pub flows: usize,
    /// Servers in the registry.
    pub servers: usize,
    /// The agents' own calls.
    pub tool_calls: usize,
    /// Lookups the proxy made for flows.
    pub flow_lookups: usize,
    /// Served decisions to hand back.
    pub hand_backs: usize,
    /// Decisions logged in shadow, lookups and hand-backs.
    pub shadow_decisions: usize,
    /// The agents' calls that failed.
    pub errors: usize,
}

/// Sessions by mode.
#[derive(Clone, Debug, Default, Serialize, TS)]
pub struct ModeCounts {
    pub recorded: usize,
    pub shadow: usize,
    pub served: usize,
}

/// A domain: a server's sessions and flows go by its name.
#[derive(Clone, Debug, Default, Serialize, TS)]
pub struct DomainSummary {
    pub name: String,
    pub sessions: usize,
    pub last_session_unix_ms: Option<u64>,
    pub modes: ModeCounts,
    /// Its flows, by key.
    pub flows: Vec<String>,
    /// Registry servers of this name.
    pub servers: Vec<String>,
}

/// One UTC day.
#[derive(Clone, Debug, Serialize, TS)]
pub struct DayActivity {
    /// `YYYY-MM-DD`
    pub day: String,
    /// Sessions started that day.
    pub sessions: usize,
    pub tool_calls: usize,
    pub flow_lookups: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Ok,
    Note,
    Warn,
    Error,
}

/// A check, as `stretto doctor` words its own.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HealthItem {
    pub level: HealthLevel,
    pub message: String,
}

/// `GET /api/overview`
#[derive(Clone, Debug, Serialize, TS)]
pub struct Overview {
    pub totals: Totals,
    pub domains: Vec<DomainSummary>,
    /// The last 14 UTC days, oldest first.
    pub activity: Vec<DayActivity>,
    /// The 8 newest sessions.
    pub recent_sessions: Vec<SessionSummary>,
    pub health: Vec<HealthItem>,
    /// The 5 newest jobs.
    pub jobs: Vec<Job>,
}

/// Days the activity chart covers.
pub const ACTIVITY_DAYS: u64 = 14;

pub async fn overview(AxumState(state): AxumState<Shared>) -> ApiResult<Json<Overview>> {
    let jobs: Vec<Job> = state.jobs.list().into_iter().take(5).collect();
    blocking(&state, move |state| Ok(Json(build(state, jobs)))).await
}

fn build(state: &State, jobs: Vec<Job>) -> Overview {
    let now = state.now();
    let all = sessions::all(state);
    let flow_list = flows::all(state);
    let (registry, registry_error) = match registry::load(state.data_dir()) {
        Ok(r) => (r, None),
        Err(e) => (registry::Registry::default(), Some(e)),
    };

    let mut totals = Totals {
        sessions: all.len(),
        flows: flow_list.len(),
        servers: registry.servers.len(),
        ..Totals::default()
    };
    let week = now.saturating_sub(7 * DAY_MS);
    let today = now / DAY_MS;
    let first = today + 1 - ACTIVITY_DAYS.min(today + 1);
    let mut activity: Vec<DayActivity> = (first..=today)
        .map(|d| DayActivity {
            day: day(d * DAY_MS),
            sessions: 0,
            tool_calls: 0,
            flow_lookups: 0,
        })
        .collect();
    let mut domains: BTreeMap<String, DomainSummary> = BTreeMap::new();
    fn domain<'a>(
        domains: &'a mut BTreeMap<String, DomainSummary>,
        name: &str,
    ) -> &'a mut DomainSummary {
        domains
            .entry(name.to_string())
            .or_insert_with(|| DomainSummary {
                name: name.to_string(),
                ..DomainSummary::default()
            })
    }
    for (s, loaded) in &all {
        totals.sessions_7d += usize::from(s.started_unix_ms >= week);
        totals.tool_calls += s.tool_calls;
        totals.flow_lookups += s.flow_lookups;
        totals.hand_backs += s.hand_backs;
        totals.shadow_decisions += loaded.shadow_decisions;
        totals.errors += s.errors;
        let d = s.started_unix_ms / DAY_MS;
        if (first..=today).contains(&d) {
            let a = &mut activity[(d - first) as usize];
            a.sessions += 1;
            a.tool_calls += s.tool_calls;
            a.flow_lookups += s.flow_lookups;
        }
        if let Some(name) = &s.domain {
            let d = domain(&mut domains, name);
            d.sessions += 1;
            d.last_session_unix_ms = d.last_session_unix_ms.max(Some(s.started_unix_ms));
            match s.mode {
                SessionMode::Recorded => d.modes.recorded += 1,
                SessionMode::Shadow => d.modes.shadow += 1,
                SessionMode::Served => d.modes.served += 1,
            }
        }
    }
    for (file, loaded) in &flow_list {
        if let Ok(flow) = &loaded.flow {
            domain(&mut domains, flow.domain())
                .flows
                .push(file.key.clone());
        }
    }
    for server in &registry.servers {
        domain(&mut domains, &server.name)
            .servers
            .push(server.name.clone());
    }
    totals.domains = domains.len();

    let mut health = Vec::new();
    health_of_setup(state, &mut health);
    if let Some(e) = registry_error {
        health.push(item(HealthLevel::Error, e));
    }
    let newest_tools: BTreeMap<&str, (&str, &Vec<super::sessions::ToolInfo>)> = {
        let mut m = BTreeMap::new();
        for (s, loaded) in &all {
            if let Some(d) = &s.domain {
                if !loaded.tools.is_empty() {
                    m.entry(d.as_str())
                        .or_insert((s.session_id.as_str(), &loaded.tools));
                }
            }
        }
        m
    };
    for (file, loaded) in &flow_list {
        match &loaded.flow {
            Err(e) => health.push(item(
                HealthLevel::Error,
                format!("{}: this stretto cannot read it ({e})", file.rel),
            )),
            Ok(flow) => {
                if let Some((session, tools)) = newest_tools.get(flow.domain()) {
                    let source = format!("the latest recorded tools/list (session {session})");
                    for w in flows::tool_warnings(flow, tools, &source) {
                        health.push(item(HealthLevel::Warn, format!("{}: {w}", file.rel)));
                    }
                }
            }
        }
    }
    for (rel, e) in sessions::unreadable(state).into_iter().take(5) {
        health.push(item(
            HealthLevel::Warn,
            format!("{rel} is not a session log this stretto can read: {e}"),
        ));
    }
    for server in &registry.servers {
        for issue in super::servers::view(state, server, &all).issues {
            health.push(item(
                HealthLevel::Warn,
                format!("server {}: {issue}", server.name),
            ));
        }
    }

    Overview {
        totals,
        domains: domains.into_values().collect(),
        activity,
        recent_sessions: all.iter().take(8).map(|(s, _)| s.clone()).collect(),
        health,
        jobs,
    }
}

fn item(level: HealthLevel, message: impl Into<String>) -> HealthItem {
    HealthItem {
        level,
        message: message.into(),
    }
}

/// The checks `stretto doctor` makes of an installation, as far as the
/// console sees it: the data directory, the binaries it found, the key.
fn health_of_setup(state: &State, out: &mut Vec<HealthItem>) {
    let dir = state.data_dir();
    if state.config.read_only {
        out.push(item(
            HealthLevel::Note,
            format!(
                "read-only: the console changes nothing in {}",
                dir.display()
            ),
        ));
    } else {
        let mut report = Report::default();
        doctor::check_dir(&mut report, dir);
        out.extend(from_report(&report, HealthLevel::Error));
    }
    let binaries = &state.config.binaries;
    for (name, found, what) in [
        ("stretto", &binaries.stretto, "jobs cannot run"),
        (
            "stretto-proxy",
            &binaries.proxy,
            "MCP hosts run it to record sessions and serve flows",
        ),
    ] {
        match found {
            Some(b) => match &b.version {
                Some(v) if *v == format!("{name} {VERSION}") => {
                    out.push(item(HealthLevel::Ok, format!("{v} ({})", b.path)))
                }
                Some(v) => out.push(item(
                    HealthLevel::Warn,
                    format!(
                        "{v} ({}) is not stretto {VERSION}'s, as the console is: install them \
                         together",
                        b.path
                    ),
                )),
                None => out.push(item(
                    HealthLevel::Warn,
                    format!("{name} ({}) does not answer --version", b.path),
                )),
            },
            None => out.push(item(
                HealthLevel::Warn,
                format!("{name} is not beside the console or on PATH: {what} (docs/install.md)"),
            )),
        }
    }
    let mut report = Report::default();
    doctor::check_key(
        &mut report,
        crate::env_set("TYPESAFE_API_KEY"),
        crate::env_set("TYPESAFE_API_KEY_FILE"),
    );
    out.extend(from_report(&report, HealthLevel::Warn));
}

/// A doctor's report as health items: `ok` and `note` as they are, a
/// `problem` at `problem`, each list item as its own item.
fn from_report(report: &Report, problem: HealthLevel) -> Vec<HealthItem> {
    let mut out: Vec<HealthItem> = Vec::new();
    for line in report.render().lines() {
        let (level, text) = if let Some(t) = line.strip_prefix("ok ") {
            (HealthLevel::Ok, t)
        } else if let Some(t) = line.strip_prefix("note ") {
            (HealthLevel::Note, t)
        } else if let Some(t) = line.strip_prefix("problem ") {
            (problem, t)
        } else {
            match out.last() {
                Some(last) => (last.level, line),
                None => (HealthLevel::Note, line),
            }
        };
        out.push(item(level, text.trim()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctors_lines_become_health_items() {
        let mut report = Report::default();
        report.ok("fine");
        report.note("worth knowing");
        report.problem("broken");
        report.item("a detail");
        let items = from_report(&report, HealthLevel::Error);
        assert_eq!(
            items,
            [
                item(HealthLevel::Ok, "fine"),
                item(HealthLevel::Note, "worth knowing"),
                item(HealthLevel::Error, "broken"),
                item(HealthLevel::Error, "a detail"),
            ]
        );
    }
}
