//! Live read-only flows (RFC-001 §3.5, §3.13).
//!
//! A [`Flow`] keeps what Phase 0b v2 learned for one domain, goal free (see
//! [`crate::phase0::compile_flow`]): the habit, the sites and the lookups
//! seen at each, the System-One questions, the arbiter of each fold, and
//! where each lookup's arguments came from in training. [`Flow::next`] takes
//! a live episode whose last step is a tool call that has returned and says
//! what the flow does next: a lookup, with its arguments, or hand back to
//! the LLM.
//!
//! The flow asks the questions Phase 0b asked offline and takes the
//! arbiter's most likely lookup when its probability clears a threshold
//! (the "lookup first" rule). An episode is judged by the arbiter of its
//! task's fold, which never saw the task, and the habit, trained on the
//! train split, never saw a test task.
//!
//! Offline, a lookup whose arguments were all seen earlier in the episode
//! counted as one a flow could make. Live, the flow has to pick them:
//! [`Bindings`] learns, for each lookup argument, which earlier outputs its
//! values came from in training (a tool and a path in its result, such as
//! `get_user_details` at `$.orders[*]`), and binds the first such value the
//! episode has not looked up yet, preferring one the customer mentioned.

use crate::arbitrate::Fitted;
use crate::phase0::{case_of, task_group};
use crate::shadow::{self, Asked, Decision, Kind, Predicate, Sites, RESPOND};
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use stretto_model::features::{step_outputs, FeatureMap, FOLDS};
use stretto_model::world::Predictor;
use stretto_model::{steps, EncodedEpisode, GroupedModel, Vocab};
use stretto_oracle::{request_key, Oracle};
use stretto_trace::{Episode, Event, ToolKind, ToolManifest};

/// A share of a lookup argument's training values a source must account for
/// before the flow binds from it.
const MIN_SOURCE_SHARE: f64 = 0.1;
/// An argument the lookup took in at least this share of its training calls
/// is required.
const REQUIRED_SHARE: f64 = 0.9;
/// Sibling values shorter than this are not matched against what the
/// customer said.
const MIN_MENTION: usize = 4;

/// A compiled live flow for one domain.
#[derive(Clone, Debug)]
pub struct Flow {
    pub(crate) vocab: Vocab,
    pub(crate) manifest: ToolManifest,
    pub(crate) map: FeatureMap,
    /// The habit's group for a goal-free episode.
    pub(crate) group: u32,
    pub(crate) habit: GroupedModel,
    pub(crate) sites: Sites,
    /// The predicates asked with each next-step question.
    pub(crate) predicates: Vec<Predicate>,
    /// The predicates the arbiter weighs.
    pub(crate) weighed: Vec<Predicate>,
    /// The arbiter of each fold.
    pub(crate) folds: Vec<Fitted>,
    pub(crate) bindings: Bindings,
    /// The System-One model id to request.
    pub(crate) model: String,
}

/// What the flow does next.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Proposal {
    /// Look something up.
    Lookup {
        /// The lookup.
        tool: String,
        /// Its arguments.
        arguments: Value,
    },
    /// Hand back to the LLM.
    HandBack {
        /// Why.
        reason: String,
    },
}

/// The flow's answer after one tool call, with what it rests on.
#[derive(Clone, Debug, Serialize)]
pub struct Next {
    /// What to do.
    #[serde(flatten)]
    pub proposal: Proposal,
    /// The site: the tool that just returned, marked when it failed.
    pub site: Option<String>,
    /// The arbiter's probability of each option; the rest is the chance
    /// that the agent would do none of them.
    pub probs: BTreeMap<String, f64>,
    /// The probability of the most likely lookup.
    pub prob: Option<f64>,
    /// The System-One model's answer to the one question.
    pub oracle: BTreeMap<String, f64>,
    /// The predicates' answers (the probability of "yes").
    pub predicates: BTreeMap<String, f64>,
    /// The request's key in the replay cache.
    pub key: Option<String>,
}

impl Flow {
    /// The domain the flow was compiled for.
    pub fn domain(&self) -> &str {
        &self.manifest.domain
    }

    /// What the flow does after the last step of `episode`, a tool call that
    /// has returned: take the most likely lookup if the arbiter gives it at
    /// least `threshold`, else hand back (also if `oracle`, asked one
    /// request, fails).
    pub fn next(&self, episode: &Episode, oracle: &dyn Oracle, threshold: f64) -> Result<Next> {
        let mut next = Next {
            proposal: Proposal::HandBack {
                reason: String::new(),
            },
            site: None,
            probs: BTreeMap::new(),
            prob: None,
            oracle: BTreeMap::new(),
            predicates: BTreeMap::new(),
            key: None,
        };
        let hand_back = |mut next: Next, reason: String| {
            next.proposal = Proposal::HandBack { reason };
            Ok(next)
        };
        let Some(live) = shadow::live_request(
            episode,
            "",
            &self.manifest,
            &self.sites,
            &self.predicates,
            &self.model,
        ) else {
            return hand_back(next, "the last step is not a tool call".to_string());
        };
        next.site = Some(Sites::name(&live.prev, live.failed));
        let Some(request) = live.request else {
            return hand_back(next, "no lookups followed here in training".to_string());
        };
        let key = request_key(&request);
        next.key = Some(key.clone());
        let response = match oracle.ask(&request) {
            Ok(r) => r,
            Err(e) => return hand_back(next, format!("the System-One model failed: {e:#}")),
        };
        let st = steps(episode);
        let decision = Decision {
            episode: 0,
            step: st.len(),
            kind: Kind::Next,
            tool: None,
            actual: String::new(),
            agent: String::new(),
            request: Some(request),
            fixed: None,
        };
        let asked = Asked {
            responses: HashMap::from([(key, response)]),
            distinct: 1,
            attempted: 1,
            errors: 0,
            first_error: None,
        };
        let (Some(one), Some(two)) = (
            shadow::score(&decision, &asked),
            shadow::score_split(&decision, &asked),
        ) else {
            return hand_back(next, "the System-One answer was incomplete".to_string());
        };
        next.oracle = one.probs.clone();
        next.predicates = shadow::predicate_answers(&decision, &asked);
        let features = self.map.features(&step_outputs(episode));
        let encoded =
            EncodedEpisode::encode_with_features(&st, Some(&features), &self.vocab, false)
                .with_group(self.group);
        let predicted = self.habit.predict_at(&encoded, st.len());
        let group = task_group(&episode.task_id);
        let Some((case, options)) = case_of(
            &one,
            &two,
            &next.predicates,
            &self.weighed,
            &predicted,
            &live.prev,
            live.failed,
            group,
            &self.vocab,
        ) else {
            return hand_back(next, "handing back was not an option".to_string());
        };
        let judged = self.folds[(group % FOLDS) as usize].judge(&case);
        next.probs = options
            .iter()
            .cloned()
            .zip(judged.probs.iter().copied())
            .collect();
        // Lookup first: the most likely lookup (the first of equals, as
        // offline), if likely enough.
        let best = options
            .iter()
            .zip(&judged.probs)
            .filter(|(o, _)| o.as_str() != RESPOND)
            .fold(None::<(&String, f64)>, |best, (o, &p)| match best {
                Some((_, q)) if q >= p => best,
                _ => Some((o, p)),
            });
        let Some((tool, p)) = best else {
            return hand_back(next, "no lookup to make".to_string());
        };
        next.prob = Some(p);
        if p < threshold {
            return hand_back(next, format!("{tool} at {p:.2}, below {threshold}"));
        }
        match self.bindings.bind(tool, episode) {
            Ok(arguments) => {
                next.proposal = Proposal::Lookup {
                    tool: tool.clone(),
                    arguments,
                };
                Ok(next)
            }
            Err(why) => hand_back(next, format!("{tool}: {why}")),
        }
    }
}

/// Where each lookup's arguments came from in training.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    /// Per lookup: calls seen, and how many of them passed each argument.
    args: BTreeMap<String, (usize, BTreeMap<String, usize>)>,
    /// Per lookup and argument: see [`Traced`].
    sources: BTreeMap<(String, String), Traced>,
}

/// String values one lookup argument took, and how many of them were found
/// in an earlier output of each `(tool, path)`.
type Traced = (usize, BTreeMap<(String, String), usize>);

impl Bindings {
    /// Learn from training episodes: each string argument of each lookup is
    /// traced to the most recent successful output that holds it as a value.
    pub fn learn<'a>(
        episodes: impl IntoIterator<Item = &'a Episode>,
        manifest: &ToolManifest,
    ) -> Self {
        let mut b = Bindings::default();
        for ep in episodes {
            let mut outputs: Vec<(&str, Value)> = Vec::new();
            for e in &ep.events {
                match e {
                    Event::ToolResult {
                        name,
                        content,
                        error: false,
                        ..
                    } => outputs.push((name.as_str(), parse(content))),
                    Event::Assistant { calls, .. } => {
                        for c in calls {
                            if manifest.tools.get(&c.name) != Some(&ToolKind::Read) {
                                continue;
                            }
                            let Value::Object(args) = &c.arguments else {
                                continue;
                            };
                            let seen = b.args.entry(c.name.clone()).or_default();
                            seen.0 += 1;
                            for arg in args.keys() {
                                *seen.1.entry(arg.clone()).or_insert(0) += 1;
                            }
                            for (arg, value) in args {
                                let Value::String(value) = value else {
                                    continue;
                                };
                                let entry =
                                    b.sources.entry((c.name.clone(), arg.clone())).or_default();
                                entry.0 += 1;
                                let found = outputs.iter().rev().find_map(|(tool, out)| {
                                    let mut paths = Vec::new();
                                    find(out, value, "$", &mut paths);
                                    paths.into_iter().next().map(|p| (tool.to_string(), p))
                                });
                                if let Some(source) = found {
                                    *entry.1.entry(source).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        b
    }

    /// Arguments for a call to `tool` after the last step of `episode`, or
    /// why there are none. Each required argument takes the first value
    /// found at one of its sources (most recent output first, in document
    /// order) that the episode has not already passed it, preferring a value
    /// the customer mentioned (or one whose record they did, by another of
    /// its fields). A lookup without arguments is made once.
    pub fn bind(&self, tool: &str, episode: &Episode) -> std::result::Result<Value, String> {
        let Some((calls, args)) = self.args.get(tool) else {
            return Err("never called in training".to_string());
        };
        let required: Vec<&String> = args
            .iter()
            .filter(|(_, &n)| n as f64 >= REQUIRED_SHARE * *calls as f64)
            .map(|(a, _)| a)
            .collect();
        let mut outputs: Vec<(&str, Value)> = Vec::new();
        let mut customer = String::new();
        let mut used: BTreeSet<(&str, String)> = BTreeSet::new();
        let mut called = false;
        for e in &episode.events {
            match e {
                Event::User { text } => {
                    customer.push_str(&text.to_lowercase());
                    customer.push('\n');
                }
                Event::ToolResult {
                    name,
                    content,
                    error: false,
                    ..
                } => outputs.push((name.as_str(), parse(content))),
                Event::Assistant { calls, .. } => {
                    for c in calls.iter().filter(|c| c.name == tool) {
                        called = true;
                        if let Value::Object(a) = &c.arguments {
                            for (k, v) in a {
                                used.insert((k.as_str(), value_text(v)));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if required.is_empty() {
            return if called {
                Err("already looked up".to_string())
            } else {
                Ok(Value::Object(Default::default()))
            };
        }
        let mut bound = serde_json::Map::new();
        for arg in required {
            let Some((values, sources)) = self.sources.get(&(tool.to_string(), arg.clone())) else {
                return Err(format!("`{arg}` never took a string in training"));
            };
            let rules: Vec<&(String, String)> = sources
                .iter()
                .filter(|(_, &n)| n >= 2 && n as f64 >= MIN_SOURCE_SHARE * *values as f64)
                .map(|(s, _)| s)
                .collect();
            let mut candidates: Vec<(String, bool)> = Vec::new();
            for (source, out) in outputs.iter().rev() {
                for (_, path) in rules.iter().filter(|(t, _)| t == source) {
                    for (value, siblings) in at_path(out, path) {
                        if used.contains(&(arg.as_str(), value.clone()))
                            || candidates.iter().any(|(v, _)| *v == value)
                        {
                            continue;
                        }
                        let mentioned = mentions(&customer, &value)
                            || siblings.iter().any(|s| {
                                s.chars().count() >= MIN_MENTION && mentions(&customer, s)
                            });
                        candidates.push((value, mentioned));
                    }
                }
            }
            let pick = candidates
                .iter()
                .find(|(_, m)| *m)
                .or(candidates.first())
                .map(|(v, _)| v.clone());
            match pick {
                Some(v) => {
                    bound.insert(arg.clone(), Value::String(v));
                }
                None if rules.is_empty() => {
                    return Err(format!("no source for `{arg}` (the LLM supplies it)"));
                }
                None => return Err(format!("nothing left to pass as `{arg}`")),
            }
        }
        Ok(Value::Object(bound))
    }
}

/// A tool result as JSON, or as one string if it is not JSON.
fn parse(content: &str) -> Value {
    serde_json::from_str(content).unwrap_or_else(|_| Value::String(content.to_string()))
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether the customer's (lower-cased) text mentions `value`, with or
/// without a leading `#`.
fn mentions(customer: &str, value: &str) -> bool {
    let v = value.to_lowercase();
    let v = v.trim();
    let bare = v.trim_start_matches('#');
    !bare.is_empty() && (customer.contains(v) || customer.contains(bare))
}

/// The paths at which `target` is a string value of `v`, array indices
/// written `[*]`.
fn find(v: &Value, target: &str, path: &str, out: &mut Vec<String>) {
    match v {
        Value::String(s) if s == target => out.push(path.to_string()),
        Value::Object(m) => {
            for (k, child) in m {
                find(child, target, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(items) => {
            for child in items {
                find(child, target, &format!("{path}[*]"), out);
            }
        }
        _ => {}
    }
}

/// The string values of `v` at `path` (as [`find`] writes it), in document
/// order, each with the other string values of its parent object.
fn at_path(v: &Value, path: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    walk(v, path.strip_prefix('$').unwrap_or(path), None, &mut out);
    out
}

fn walk(v: &Value, rest: &str, parent: Option<&Value>, out: &mut Vec<(String, Vec<String>)>) {
    if rest.is_empty() {
        if let Value::String(s) = v {
            let siblings = match parent {
                Some(Value::Object(m)) => m
                    .values()
                    .filter_map(|x| x.as_str())
                    .filter(|x| *x != s)
                    .map(String::from)
                    .collect(),
                _ => Vec::new(),
            };
            out.push((s.clone(), siblings));
        }
        return;
    }
    if let Some(rest) = rest.strip_prefix("[*]") {
        if let Value::Array(items) = v {
            for item in items {
                walk(item, rest, Some(v), out);
            }
        }
        return;
    }
    if let Some(rest) = rest.strip_prefix('.') {
        let end = rest.find(['.', '[']).unwrap_or(rest.len());
        let (key, rest) = rest.split_at(end);
        if let Value::Object(m) = v {
            if let Some(child) = m.get(key) {
                walk(child, rest, Some(v), out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stretto_trace::ToolCall;

    fn manifest() -> ToolManifest {
        ToolManifest {
            domain: "retail".to_string(),
            tools: BTreeMap::from([
                ("get_user_details".to_string(), ToolKind::Read),
                ("get_order_details".to_string(), ToolKind::Read),
                ("cancel_pending_order".to_string(), ToolKind::Write),
            ]),
            docs: BTreeMap::new(),
        }
    }

    fn call(id: &str, name: &str, arguments: Value) -> Event {
        Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments,
            }],
            usage: None,
        }
    }

    fn result(id: &str, name: &str, content: Value) -> Event {
        Event::ToolResult {
            call_id: id.to_string(),
            name: name.to_string(),
            error: false,
            content: content.to_string(),
        }
    }

    fn episode(events: Vec<Event>) -> Episode {
        Episode {
            id: "e".to_string(),
            task_id: "1".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "m".to_string(),
            reward: 1.0,
            events,
        }
    }

    fn user(orders: &[&str]) -> Value {
        json!({"user_id": "ann_1", "orders": orders})
    }

    #[test]
    fn binds_the_next_order_not_yet_looked_up() {
        let training: Vec<Episode> = (0..3)
            .map(|_| {
                episode(vec![
                    Event::User {
                        text: "help with my orders".to_string(),
                    },
                    call("a", "get_user_details", json!({"user_id": "ann_1"})),
                    result("a", "get_user_details", user(&["#W1", "#W2"])),
                    call("b", "get_order_details", json!({"order_id": "#W1"})),
                    result("b", "get_order_details", json!({"order_id": "#W1"})),
                    call("c", "get_order_details", json!({"order_id": "#W2"})),
                    result("c", "get_order_details", json!({"order_id": "#W2"})),
                ])
            })
            .collect();
        let b = Bindings::learn(&training, &manifest());
        let live = |extra: Vec<Event>| {
            let mut events = vec![
                Event::User {
                    text: "I want to cancel something".to_string(),
                },
                call("a", "get_user_details", json!({"user_id": "bob_2"})),
                result("a", "get_user_details", user(&["#W7", "#W8", "#W9"])),
            ];
            events.extend(extra);
            episode(events)
        };
        assert_eq!(
            b.bind("get_order_details", &live(vec![])),
            Ok(json!({"order_id": "#W7"}))
        );
        let after_one = live(vec![
            call("b", "get_order_details", json!({"order_id": "#W7"})),
            result("b", "get_order_details", json!({"order_id": "#W7"})),
        ]);
        assert_eq!(
            b.bind("get_order_details", &after_one),
            Ok(json!({"order_id": "#W8"}))
        );
        // The customer names an order: it goes first.
        let named = live(vec![Event::User {
            text: "It's order W9".to_string(),
        }]);
        assert_eq!(
            b.bind("get_order_details", &named),
            Ok(json!({"order_id": "#W9"}))
        );
        // A user id comes from the customer, never from an output: no rule.
        assert!(b.bind("get_user_details", &live(vec![])).is_err());
    }

    #[test]
    fn paths_generalize_array_indices() {
        let v = json!({"items": [{"name": "Water Bottle", "product_id": "p1"},
                                 {"name": "Desk Lamp", "product_id": "p2"}]});
        let mut paths = Vec::new();
        find(&v, "p2", "$", &mut paths);
        assert_eq!(paths, vec!["$.items[*].product_id"]);
        let found = at_path(&v, "$.items[*].product_id");
        assert_eq!(found[0].0, "p1");
        assert_eq!(found[1], ("p2".to_string(), vec!["Desk Lamp".to_string()]));
        assert!(mentions("the desk lamp i bought\n", "Desk Lamp"));
        assert_eq!(at_path(&json!("u1"), "$"), vec![("u1".to_string(), vec![])]);
    }
}
