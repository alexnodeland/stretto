//! Drift alarms (RFC-001 §3.3 and §4): whether a flow still fits the agent
//! it serves, session by session.
//!
//! A flow is learned from one agent, one prompt, one harness and one set of
//! tools, and any of them can change. [`drift`] scores recorded sessions in
//! the order they ran, as `stretto audit` scores them ([`crate::audit`]),
//! into three series:
//!
//! - **surprise**: the mean negative log-probability, under the flow, of the
//!   agent's steps at the flow's decisions (nats);
//! - **disagreement**: the share of those decisions where the flow's
//!   likeliest option was not the agent's step;
//! - **unknown**: the share of the agent's steps that follow a tool training
//!   never saw it call, at sites the flow does not know.
//!
//! Bayesian online change-point detection (Adams & MacKay, 2007) runs on the
//! three together. Within a run of sessions, each series is Gaussian with a
//! mean and a variance of its own, under a normal-gamma prior, so a session
//! is predicted by a Student-t, save that any session may be an outlier
//! drawn from the prior; a run ends after any session with the same small
//! chance, the hazard. After each session, the posterior over how long
//! the current run has lasted says how likely it is that the agent changed a
//! few sessions ago. The alarm sounds when a change within the last `window`
//! sessions, after at least [`MIN_BEFORE`] sessions and followed by at least
//! [`MIN_RUN`] sessions of the new run, is likelier than `threshold`, and it
//! names the sites whose surprise rose most across the change.

use crate::audit::decisions_with;
use crate::flow::{Decider, Flow};
use fugue::{Distribution, StudentT};
use serde::Serialize;
use std::collections::BTreeMap;
use stretto_oracle::Oracle;
use stretto_trace::{Episode, Event};

/// The sessions a new run needs before the alarm counts it. One session
/// unlike the rest is an outlier, and two alike can be one task's retries;
/// three in a row are a change. On τ²-bench, requiring three rather than
/// two cut false alarms on an agent's own shuffled sessions from 18 runs in
/// 160 to 5, and found 60 of 480 spliced agents rather than 82, losing most
/// where the fit moved least (`docs/results/drift-2026-09-28.md`).
pub const MIN_RUN: usize = 3;

/// The sessions a change needs before it: until then, the sessions say
/// too little about how the agent usually scores to tell a change.
pub const MIN_BEFORE: usize = 10;

/// Below this, a series' spread is taken to be this: sessions that all score
/// alike would otherwise make any difference look like a change.
const MIN_SCALE: f64 = 0.05;

/// The chance that a session is unlike any run, drawn from the prior: with
/// it, one odd session costs a run little and does not end it.
const OUTLIER: f64 = 0.05;

/// How the alarm is set.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Settings {
    /// The prior chance that the agent changes after any one session.
    pub hazard: f64,
    /// How recent a change the alarm reports, in sessions.
    pub window: usize,
    /// The posterior probability of a recent change that sounds the alarm.
    pub threshold: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hazard: 0.01,
            window: 10,
            threshold: 0.5,
        }
    }
}

/// One session, scored under the flow.
#[derive(Clone, Debug, Serialize)]
pub struct Session {
    /// The episode's id: for a recorded session, its start time and process.
    pub id: String,
    /// Decisions scored.
    pub decisions: usize,
    /// Mean negative log-probability of the agent's steps (nats), if the
    /// flow decided anywhere in the session.
    pub surprise: Option<f64>,
    /// Share of decisions where the flow's likeliest option was not the
    /// agent's step.
    pub disagreement: Option<f64>,
    /// Share of the agent's steps after a tool training never saw called, if
    /// the agent called any tool.
    pub unknown: Option<f64>,
    /// After this session, the probability that the agent changed within the
    /// last `window` sessions.
    pub change: f64,
}

/// One site's decisions over a span of sessions.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SiteSpan {
    /// Decisions made there.
    pub decisions: usize,
    /// Share where the flow's likeliest option was the agent's step.
    pub agreement: f64,
    /// Mean negative log-probability of the agent's step (nats).
    pub surprise: f64,
}

/// A site on both sides of a change.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SiteShift {
    /// The site.
    pub site: String,
    /// Before the change, if the agent reached the site then.
    pub before: Option<SiteSpan>,
    /// From the change on.
    pub after: SiteSpan,
}

/// The alarm, as it sounded.
#[derive(Clone, Debug, Serialize)]
pub struct Alarm {
    /// The session after which it sounded (an index into [`Drift::sessions`]).
    pub at: usize,
    /// The first session of the new run: the likeliest change point.
    pub change: usize,
    /// The probability of a change within the window.
    pub probability: f64,
    /// The sites the agent reached after the change, those it had not
    /// reached before first, then by how much the flow's surprise rose.
    pub moved: Vec<SiteShift>,
}

/// A flow's drift over recorded sessions.
#[derive(Clone, Debug, Serialize)]
pub struct Drift {
    /// The flow's domain.
    pub domain: String,
    /// How the flow decided: `arbiter`, `habit` or `reach`.
    pub decider: String,
    /// How the alarm was set.
    pub settings: Settings,
    /// The sessions, in the order they ran.
    pub sessions: Vec<Session>,
    /// Each time the alarm sounded.
    pub alarms: Vec<Alarm>,
    /// Whether it is sounding after the last session.
    pub sounding: bool,
    /// The tools the agent called that training never saw, each with the
    /// first session it was called in.
    pub unknown_tools: BTreeMap<String, usize>,
}

/// Per site: decisions, agreements, and the sum of the agent's steps'
/// log-probabilities.
type SiteTally = BTreeMap<String, (usize, usize, f64)>;

/// Score `episodes`, in the order they ran, under `flow` deciding by
/// `decider`, and watch for the agent changing. `oracle` answers the
/// arbiter's questions; the habit asks none.
pub fn drift(
    flow: &Flow,
    episodes: &[Episode],
    oracle: &dyn Oracle,
    decider: Decider,
    settings: Settings,
) -> Drift {
    let mut sessions = Vec::new();
    let mut tallies: Vec<SiteTally> = Vec::new();
    let mut series: Vec<[Option<f64>; 3]> = Vec::new();
    let mut unknown_tools = BTreeMap::new();
    for (i, ep) in episodes.iter().enumerate() {
        let (ds, _) = decisions_with(flow, ep, oracle, decider);
        let mut tally = SiteTally::new();
        for d in &ds {
            let t = tally.entry(d.site.clone()).or_default();
            t.0 += 1;
            t.1 += usize::from(d.top() == d.actual);
            t.2 += d.probs[d.actual].ln();
        }
        let n = ds.len() as f64;
        let surprise = (!ds.is_empty()).then(|| -tally.values().map(|t| t.2).sum::<f64>() / n);
        let disagreement =
            (!ds.is_empty()).then(|| 1.0 - tally.values().map(|t| t.1).sum::<usize>() as f64 / n);
        let after = steps_after(ep);
        let new: Vec<&str> = after
            .iter()
            .copied()
            .filter(|t| !flow.trained_on(t))
            .collect();
        for tool in &new {
            unknown_tools.entry(tool.to_string()).or_insert(i);
        }
        let unknown = (!after.is_empty()).then(|| new.len() as f64 / after.len() as f64);
        series.push([surprise, disagreement, unknown]);
        tallies.push(tally);
        sessions.push(Session {
            id: ep.id.clone(),
            decisions: ds.len(),
            surprise,
            disagreement,
            unknown,
            change: 0.0,
        });
    }
    let mut detector = Detector::new(&series, settings.hazard);
    let mut alarms: Vec<Alarm> = Vec::new();
    let mut since = 0;
    let mut was = false;
    for (t, x) in series.iter().enumerate() {
        let (p, run) = detector.observe(x, settings.window);
        sessions[t].change = p;
        let now = p >= settings.threshold;
        if now && !was {
            let change = t + 1 - run;
            alarms.push(Alarm {
                at: t,
                change,
                probability: p,
                moved: shifts(&tallies[since.min(change)..change], &tallies[change..=t]),
            });
            since = change;
        }
        was = now;
    }
    Drift {
        domain: flow.domain().to_string(),
        decider: decider.name().to_string(),
        settings,
        sessions,
        alarms,
        sounding: was,
        unknown_tools,
    }
}

/// The tool before each of the agent's steps that follows a tool: each
/// tool result the agent answered with a step, as the audit's decisions are
/// placed ([`crate::audit::decisions`]).
fn steps_after(ep: &Episode) -> Vec<&str> {
    ep.events
        .windows(2)
        .filter_map(|w| match w {
            [Event::ToolResult { name, .. }, next] if !matches!(next, Event::ToolResult { .. }) => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect()
}

/// Each site the agent reached from `after`'s sessions on, with the same
/// site over `before`'s: new sites first, then by how much the flow's
/// surprise rose.
fn shifts(before: &[SiteTally], after: &[SiteTally]) -> Vec<SiteShift> {
    let sum = |tallies: &[SiteTally]| {
        let mut all = SiteTally::new();
        for (site, t) in tallies.iter().flatten() {
            let a = all.entry(site.clone()).or_default();
            a.0 += t.0;
            a.1 += t.1;
            a.2 += t.2;
        }
        all
    };
    let span = |&(n, hits, lp): &(usize, usize, f64)| SiteSpan {
        decisions: n,
        agreement: hits as f64 / n as f64,
        surprise: -lp / n as f64,
    };
    let before = sum(before);
    let mut out: Vec<SiteShift> = sum(after)
        .iter()
        .map(|(site, t)| SiteShift {
            site: site.clone(),
            before: before.get(site).map(span),
            after: span(t),
        })
        .collect();
    let rise = |s: &SiteShift| s.before.as_ref().map(|b| s.after.surprise - b.surprise);
    out.sort_by(|a, b| match (rise(a), rise(b)) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (x, y) => x.is_some().cmp(&y.is_some()),
    });
    out
}

/// A normal-gamma prior on one series' mean and precision.
#[derive(Clone, Copy, Debug)]
struct Prior {
    mean: f64,
    kappa: f64,
    alpha: f64,
    beta: f64,
}

impl Prior {
    /// A weak prior centred on the series' median, with the spread of its
    /// session-to-session steps, which a change moves little (the median
    /// absolute difference, scaled to a standard deviation), and at least
    /// [`MIN_SCALE`].
    fn of(values: impl Iterator<Item = f64>) -> Self {
        let values: Vec<f64> = values.collect();
        let steps: Vec<f64> = values.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        let scale = (median(&steps) * 1.4826 / 2f64.sqrt()).max(MIN_SCALE);
        Self {
            mean: median(&values),
            kappa: 0.1,
            alpha: 1.0,
            beta: scale * scale,
        }
    }

    /// The log-density of `x` once `run` has been seen: a Student-t.
    fn log_predictive(&self, run: &Moments, x: f64) -> f64 {
        let kappa = self.kappa + run.n;
        let mean = (self.kappa * self.mean + run.n * run.mean) / kappa;
        let alpha = self.alpha + run.n / 2.0;
        let beta = self.beta
            + run.m2 / 2.0
            + self.kappa * run.n * (run.mean - self.mean).powi(2) / (2.0 * kappa);
        let scale = (beta * (kappa + 1.0) / (alpha * kappa)).sqrt();
        StudentT::new(2.0 * alpha, mean, scale).map_or(f64::NEG_INFINITY, |t| t.log_prob(&x))
    }
}

/// The median of `values`, or 0 for none.
fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => 0.0,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

/// A run's observations of one series, each weighed by the chance that it
/// belongs to the run: their weight, mean, and weighted sum of squared
/// deviations (Welford's update).
#[derive(Clone, Copy, Debug, Default)]
struct Moments {
    n: f64,
    mean: f64,
    m2: f64,
}

impl Moments {
    fn push(self, x: f64, weight: f64) -> Self {
        let n = self.n + weight;
        let mean = self.mean + weight * (x - self.mean) / n;
        Self {
            n,
            mean,
            m2: self.m2 + weight * (x - self.mean) * (x - mean),
        }
    }
}

/// Adams and MacKay's recursion over the length of the current run.
struct Detector {
    priors: [Prior; 3],
    hazard: f64,
    /// By run length (the sessions in the current run): the log joint
    /// probability with the sessions so far, and each series' moments.
    runs: Vec<(f64, [Moments; 3])>,
    /// Sessions seen.
    seen: usize,
}

impl Detector {
    fn new(series: &[[Option<f64>; 3]], hazard: f64) -> Self {
        let prior = |k: usize| Prior::of(series.iter().filter_map(|x| x[k]));
        Self {
            priors: [prior(0), prior(1), prior(2)],
            hazard,
            runs: vec![(0.0, [Moments::default(); 3])],
            seen: 0,
        }
    }

    /// The log-density of `x` after a run with moments `m`: the run's
    /// predictive, or with chance [`OUTLIER`] the prior's; and the chance
    /// that `x` belongs to the run, not an outlier. A missing score says
    /// nothing.
    fn log_predictive(&self, m: &[Moments; 3], x: &[Option<f64>; 3]) -> (f64, f64) {
        let under = |m: &[Moments; 3]| {
            (0..3)
                .filter_map(|k| x[k].map(|v| self.priors[k].log_predictive(&m[k], v)))
                .sum::<f64>()
        };
        let belongs = (-OUTLIER).ln_1p() + under(m);
        let total = log_sum_exp(&[belongs, OUTLIER.ln() + under(&[Moments::default(); 3])]);
        (total, (belongs - total).exp())
    }

    /// Take the next session's scores, and return the posterior probability
    /// that the current run began within the last `window` sessions, after
    /// at least [`MIN_BEFORE`] sessions, and has lasted at least [`MIN_RUN`]
    /// sessions; and the likeliest such run's length.
    fn observe(&mut self, x: &[Option<f64>; 3], window: usize) -> (f64, usize) {
        let predicted: Vec<(f64, f64)> = self
            .runs
            .iter()
            .map(|(lp, m)| {
                let (density, belongs) = self.log_predictive(m, x);
                (lp + density, belongs)
            })
            .collect();
        let joint: Vec<f64> = predicted.iter().map(|p| p.0).collect();
        let changed = log_sum_exp(&joint) + self.hazard.ln();
        let mut runs = vec![(changed, [Moments::default(); 3])];
        // A run keeps each session as far as it belongs to it, so an outlier
        // leaves the run's mean and spread as they were.
        for ((_, m), &(lp, belongs)) in self.runs.iter().zip(&predicted) {
            let pushed = [0, 1, 2].map(|k| x[k].map_or(m[k], |v| m[k].push(v, belongs)));
            runs.push((lp + (-self.hazard).ln_1p(), pushed));
        }
        let total = log_sum_exp(&runs.iter().map(|r| r.0).collect::<Vec<_>>());
        for r in &mut runs {
            r.0 -= total;
        }
        self.runs = runs;
        self.seen += 1;
        // A run of length r began after the first seen - r sessions.
        let recent = MIN_RUN..=window.min(self.seen.saturating_sub(MIN_BEFORE));
        let p = recent.clone().fold(0.0, |p, r| p + self.runs[r].0.exp());
        let run = recent.max_by(|&a, &b| self.runs[a].0.total_cmp(&self.runs[b].0));
        (p, run.unwrap_or(0))
    }
}

/// `ln Σ exp(x)`, computed stably.
fn log_sum_exp(xs: &[f64]) -> f64 {
    let top = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    top + xs.iter().map(|x| (x - top).exp()).sum::<f64>().ln()
}

/// The drift as Markdown.
pub fn markdown(d: &Drift) -> String {
    let mut out = format!("# Drift: {}\n\n", d.domain);
    let (Some(first), Some(last)) = (d.sessions.first(), d.sessions.last()) else {
        out.push_str("No sessions to score.\n");
        return out;
    };
    out.push_str(&format!(
        "{} sessions, from `{}` to `{}`, scored under the flow's {}. The alarm sounds when a \
         change within the last {} sessions, after at least {MIN_BEFORE} sessions and followed \
         by at least {MIN_RUN} of the new run, has probability {} or more (hazard {}).\n\n",
        d.sessions.len(),
        first.id,
        last.id,
        d.decider,
        d.settings.window,
        d.settings.threshold,
        d.settings.hazard,
    ));
    let now = last.change;
    match d.alarms.last().filter(|_| d.sounding) {
        Some(a) => out.push_str(&format!(
            "**Alarm: the agent changed at session {} (`{}`), {} sessions ago** (probability \
             {now:.2}). Relearn the flow with the sessions since weighed more (`stretto learn \
             --half-life`), or from them alone.\n\n",
            a.change + 1,
            d.sessions[a.change].id,
            d.sessions.len() - a.change,
        )),
        None => out.push_str(&format!(
            "**No alarm now:** a change within the last {} sessions has probability {now:.2}.\n\n",
            d.settings.window
        )),
    }
    if !d.alarms.is_empty() {
        out.push_str("## Alarms\n\n| After session | Changed at | Probability |\n|---|---|---|\n");
        for a in &d.alarms {
            out.push_str(&format!(
                "| {} | {} (`{}`) | {:.2} |\n",
                a.at + 1,
                a.change + 1,
                d.sessions[a.change].id,
                a.probability
            ));
        }
        for a in &d.alarms {
            out.push_str(&format!(
                "\n### Sites across the change at session {}\n\n| Site | Decisions before | \
                 after | Agreement before | after | Surprise before | after |\n\
                 |---|---|---|---|---|---|---|\n",
                a.change + 1
            ));
            for s in a.moved.iter().take(5) {
                let (n, agree, surprise) = s.before.as_ref().map_or(
                    ("0".to_string(), "–".to_string(), "–".to_string()),
                    |b| {
                        (
                            b.decisions.to_string(),
                            format!("{:.0}%", 100.0 * b.agreement),
                            format!("{:.2}", b.surprise),
                        )
                    },
                );
                out.push_str(&format!(
                    "| `{}` | {n} | {} | {agree} | {:.0}% | {surprise} | {:.2} |\n",
                    s.site,
                    s.after.decisions,
                    100.0 * s.after.agreement,
                    s.after.surprise
                ));
            }
        }
        out.push('\n');
    }
    if !d.unknown_tools.is_empty() {
        out.push_str(
            "## Tools training never saw\n\n| Tool | First called in session |\n|---|---|\n",
        );
        for (tool, &i) in &d.unknown_tools {
            out.push_str(&format!(
                "| `{tool}` | {} (`{}`) |\n",
                i + 1,
                d.sessions[i].id
            ));
        }
        out.push('\n');
    }
    out.push_str(
        "## Sessions\n\n| # | Session | Decisions | Surprise (nats) | Disagreement | Unknown \
         sites | P(change) |\n|---|---|---|---|---|---|---|\n",
    );
    let share = |x: Option<f64>| x.map_or("–".to_string(), |v| format!("{:.0}%", 100.0 * v));
    for (i, s) in d.sessions.iter().enumerate() {
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {:.2} |\n",
            i + 1,
            s.id,
            s.decisions,
            s.surprise.map_or("–".to_string(), |v| format!("{v:.2}")),
            share(s.disagreement),
            share(s.unknown),
            s.change
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase0::{compile_habit_flow_from_episodes, Config};
    use serde_json::{json, Value};
    use stretto_oracle::MockOracle;
    use stretto_trace::{ToolCall, ToolKind, ToolManifest};

    const MOCK: MockOracle = MockOracle {
        confidence: 0.6,
        noul: 0.5,
    };

    fn manifest() -> ToolManifest {
        ToolManifest {
            domain: "shop".to_string(),
            tools: BTreeMap::from([
                ("find_account".to_string(), ToolKind::Read),
                ("get_account".to_string(), ToolKind::Read),
                ("get_order".to_string(), ToolKind::Read),
                ("close_order".to_string(), ToolKind::Write),
            ]),
            docs: BTreeMap::new(),
        }
    }

    fn call(id: usize, name: &str, arguments: Value) -> [Event; 2] {
        [
            Event::Assistant {
                text: None,
                calls: vec![ToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    arguments,
                }],
                usage: None,
            },
            Event::ToolResult {
                call_id: id.to_string(),
                name: name.to_string(),
                error: false,
                content: json!({"id": format!("{name}-{id}")}).to_string(),
            },
        ]
    }

    fn say(text: &str) -> Event {
        Event::Assistant {
            text: Some(text.to_string()),
            calls: Vec::new(),
            usage: None,
        }
    }

    /// Customer `i` of agent A: it finds their account, reads it and each
    /// of their one to three orders, and closes one once they agree.
    fn agent_a(i: usize) -> Episode {
        let mut events = vec![Event::User {
            text: format!("I'm c{i}@example.com; close an order, please."),
        }];
        events.extend(call(1, "find_account", json!({"email": i})));
        events.extend(call(2, "get_account", json!({"account": i})));
        for k in 0..1 + i % 3 {
            events.extend(call(3 + k, "get_order", json!({"order": k})));
        }
        events.push(say("Close the first?"));
        events.push(Event::User {
            text: "Yes.".to_string(),
        });
        events.extend(call(9, "close_order", json!({"order": 0})));
        events.push(say("Done."));
        episode(format!("a-{i:03}"), events)
    }

    /// Agent B lists the orders with a tool training never saw, skips the
    /// account, and closes an order without asking.
    fn agent_b(i: usize) -> Episode {
        let mut events = vec![Event::User {
            text: format!("I'm c{i}@example.com; close an order, please."),
        }];
        events.extend(call(1, "find_account", json!({"email": i})));
        events.extend(call(2, "list_orders", json!({"account": i})));
        events.extend(call(3, "get_order", json!({"order": 0})));
        events.extend(call(4, "close_order", json!({"order": 0})));
        events.push(say("Done."));
        episode(format!("b-{i:03}"), events)
    }

    fn episode(id: String, events: Vec<Event>) -> Episode {
        Episode {
            task_id: id.clone(),
            id,
            trial: 0,
            domain: "shop".to_string(),
            agent_model: "agent".to_string(),
            reward: 1.0,
            events,
        }
    }

    fn flow() -> Flow {
        let mut config = Config::new(std::path::PathBuf::new());
        config.alpha_samples = 0;
        let train: Vec<Episode> = (0..60).map(agent_a).collect();
        compile_habit_flow_from_episodes(&config, &train, &manifest()).unwrap()
    }

    fn watch(flow: &Flow, episodes: &[Episode]) -> Drift {
        drift(flow, episodes, &MOCK, Decider::Habit, Settings::default())
    }

    #[test]
    fn a_splice_sounds_the_alarm_at_the_change_and_names_what_moved() {
        let flow = flow();
        // Agent A for 30 sessions, then agent B for 8.
        let mut episodes: Vec<Episode> = (100..130).map(agent_a).collect();
        episodes.extend((0..8).map(agent_b));
        let d = watch(&flow, &episodes);
        assert!(d.sounding, "{}", markdown(&d));
        assert_eq!(d.alarms.len(), 1, "{}", markdown(&d));
        let alarm = &d.alarms[0];
        assert_eq!(alarm.change, 30, "{}", markdown(&d));
        assert!(alarm.at >= 32 && alarm.at <= 34, "{}", markdown(&d));
        // Before the splice, all quiet.
        assert!(d.sessions[..30].iter().all(|s| s.change < 0.5));
        // B's new tool, and the sites across the change: where B went first
        // that A never did, then where the flow's surprise rose most.
        assert_eq!(
            d.unknown_tools,
            BTreeMap::from([("list_orders".into(), 30)])
        );
        assert!(d.sessions[30].unknown.unwrap() > 0.0);
        assert_eq!(d.sessions[0].unknown, Some(0.0));
        // After finding the account, B lists orders where A read the
        // account: the flow's surprise rose most there.
        let first = &alarm.moved[0];
        assert_eq!(first.site, "find_account", "{:?}", alarm.moved);
        assert!(first.after.surprise > first.before.as_ref().unwrap().surprise + 5.0);
        let md = markdown(&d);
        assert!(
            md.contains("**Alarm: the agent changed at session 31 (`b-000`)"),
            "{md}"
        );
        assert!(md.contains("| `list_orders` | 31 (`b-000`) |"), "{md}");
        assert!(
            md.contains("### Sites across the change at session 31"),
            "{md}"
        );
    }

    #[test]
    fn one_agents_sessions_in_any_order_raise_no_alarm() {
        let flow = flow();
        // A's sessions vary with the number of orders; shuffled, they are no
        // change.
        let episodes: Vec<Episode> = (0..45).map(|k| agent_a(100 + (k * 17) % 45)).collect();
        let d = watch(&flow, &episodes);
        assert!(d.alarms.is_empty(), "{}", markdown(&d));
        assert!(!d.sounding && d.unknown_tools.is_empty());
        let md = markdown(&d);
        assert!(md.contains("**No alarm now:**"), "{md}");
        assert!(!md.contains("## Alarms") && !md.contains("## Tools training"));
    }

    #[test]
    fn a_change_long_past_no_longer_sounds() {
        let flow = flow();
        let mut episodes: Vec<Episode> = (100..125).map(agent_a).collect();
        episodes.extend((0..20).map(agent_b));
        let d = watch(&flow, &episodes);
        assert_eq!(d.alarms.len(), 1);
        assert!(!d.sounding, "{}", markdown(&d));
        assert!(markdown(&d).contains("## Alarms"));
    }

    #[test]
    fn sessions_without_decisions_or_tools_say_nothing() {
        let flow = flow();
        let quiet = episode("q".to_string(), vec![say("Hello.")]);
        let d = watch(&flow, &[quiet.clone(), quiet]);
        assert_eq!(d.sessions[0].surprise, None);
        assert_eq!(d.sessions[0].unknown, None);
        assert!(d.alarms.is_empty());
        let md = markdown(&d);
        assert!(md.contains("| 1 | `q` | 0 | – | – | – | 0.00 |"), "{md}");
        let empty = watch(&flow, &[]);
        assert!(markdown(&empty).ends_with("No sessions to score.\n"));
    }

    #[test]
    fn an_outlier_is_no_change_and_a_shift_is() {
        let steady = |t: usize| 1.0 + 0.1 * ((t * 7) % 5) as f64 / 4.0;
        let run = |xs: &[f64]| {
            let series: Vec<[Option<f64>; 3]> = xs.iter().map(|&x| [Some(x), None, None]).collect();
            let mut d = Detector::new(&series, 0.01);
            series.iter().map(|x| d.observe(x, 10)).collect::<Vec<_>>()
        };
        // One odd session among steady ones.
        let mut xs: Vec<f64> = (0..40).map(steady).collect();
        xs[25] = 3.0;
        assert!(run(&xs).iter().all(|(p, _)| *p < 0.5), "{:?}", run(&xs));
        // The level moves for good: the alarm sounds three sessions in, with
        // the change where it happened.
        let xs: Vec<f64> = (0..40)
            .map(|t| steady(t) + if t >= 30 { 2.0 } else { 0.0 })
            .collect();
        let seen = run(&xs);
        let tail = &seen[28..];
        assert!(seen[..32].iter().all(|(p, _)| *p < 0.5), "{tail:?}");
        assert!(seen[32].0 > 0.5 && seen[32].1 == 3, "{tail:?}");
        assert_eq!(seen[35].1, 6);
    }

    #[test]
    fn moments_priors_and_medians() {
        let m = [2.0, 4.0, 9.0]
            .iter()
            .fold(Moments::default(), |m, &x| m.push(x, 1.0));
        assert!((m.mean - 5.0).abs() < 1e-12 && (m.m2 - 26.0).abs() < 1e-12);
        // Half an observation moves the mean a third of the way there.
        let half = Moments::default().push(0.0, 1.0).push(3.0, 0.5);
        assert!((half.mean - 1.0).abs() < 1e-12 && (half.m2 - 3.0).abs() < 1e-12);
        assert_eq!(median(&[]), 0.0);
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        // A series that never moves gets the smallest spread.
        let p = Prior::of([1.0, 1.0, 1.0].into_iter());
        assert_eq!((p.mean, p.beta), (1.0, MIN_SCALE * MIN_SCALE));
        // The predictive is a proper density, centred on what it has seen.
        let seen = Moments::default().push(1.0, 1.0).push(1.2, 1.0);
        let near = p.log_predictive(&seen, 1.1);
        assert!(near > p.log_predictive(&seen, 3.0));
        let dx = 0.001;
        let mass: f64 = (-20000..20000)
            .map(|k| (p.log_predictive(&seen, 1.1 + k as f64 * dx)).exp() * dx)
            .sum();
        assert!((mass - 1.0).abs() < 0.01, "{mass}");
        assert!((log_sum_exp(&[0.0, 0.0]) - 2f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn sites_shift_new_ones_first_then_by_how_much_surprise_rose() {
        let tally = |xs: &[(&str, usize, usize, f64)]| -> SiteTally {
            xs.iter()
                .map(|&(s, n, h, lp)| (s.to_string(), (n, h, lp)))
                .collect()
        };
        let before = [tally(&[("a", 2, 2, -0.2), ("b", 4, 4, -0.4)])];
        let after = [
            tally(&[("a", 1, 0, -2.0), ("b", 2, 1, -0.4)]),
            tally(&[("c", 1, 0, -3.0), ("b", 2, 2, -0.4)]),
        ];
        let moved = shifts(&before, &after);
        let names: Vec<&str> = moved.iter().map(|s| s.site.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"]);
        assert_eq!(
            moved[1].after,
            SiteSpan {
                decisions: 1,
                agreement: 0.0,
                surprise: 2.0
            }
        );
        assert_eq!(moved[2].after.decisions, 4);
        assert!((moved[2].after.agreement - 0.75).abs() < 1e-12);
    }
}
