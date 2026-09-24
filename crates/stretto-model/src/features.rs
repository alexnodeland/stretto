//! Typed features read straight from tool outputs.
//!
//! Tool outputs are usually JSON. Their low-cardinality fields (an order's
//! status, a reservation's cabin, whether a search came back empty) often
//! decide what the agent does next, and code can read them exactly and for
//! free.
//!
//! - [`discover`] lists candidate fields from training episodes.
//! - [`select`] adds them one at a time while each raises the likelihood of
//!   *held-out tasks* under the world model, by cross-validation grouped by
//!   task. That is predicate refinement with code predicates. Grouping
//!   matters: a benchmark runs each task several times, so a field that merely
//!   identifies the task (a user's city) looks predictive on the training set
//!   and would be picked by in-sample evidence, but it cannot help on a new
//!   task and grouped cross-validation rejects it.
//! - [`FeatureMap`] turns the chosen fields into a small feature id per step.

use crate::abstraction::{Step, Vocab};
use crate::world::{log_likelihood, BackoffModel, EncodedEpisode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use stretto_trace::{Episode, Event};

/// A field of a tool's JSON output.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Field {
    /// A scalar at a path (`status`, `address.state`).
    Scalar(String),
    /// Whether an array (at a path; `""` for a top-level array) is empty,
    /// has one element, or more.
    Len(String),
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Field::Scalar(p) => write!(f, "{p}"),
            Field::Len(p) if p.is_empty() => write!(f, "len(output)"),
            Field::Len(p) => write!(f, "len({p})"),
        }
    }
}

/// What a step produced: the tool and its parsed output, or nothing for a
/// reply to the user or an output that is not JSON.
#[derive(Clone, Debug)]
pub struct StepOutput {
    /// Tool called at this step, if any.
    pub tool: Option<String>,
    /// Parsed output, if the step was a tool call that returned JSON.
    pub output: Option<Value>,
}

/// The outputs of `ep`'s steps, aligned with [`crate::steps`].
pub fn step_outputs(ep: &Episode) -> Vec<StepOutput> {
    let contents: HashMap<&str, &str> = ep
        .events
        .iter()
        .filter_map(|e| match e {
            Event::ToolResult {
                call_id, content, ..
            } => Some((call_id.as_str(), content.as_str())),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for e in &ep.events {
        if let Event::Assistant { calls, .. } = e {
            if calls.is_empty() {
                out.push(StepOutput {
                    tool: None,
                    output: None,
                });
            }
            for c in calls {
                let output = contents
                    .get(c.id.as_str())
                    .and_then(|s| serde_json::from_str(s).ok());
                out.push(StepOutput {
                    tool: Some(c.name.clone()),
                    output,
                });
            }
        }
    }
    out
}

/// The value of `field` in `output`, as text. Missing fields read as `"∅"`.
pub fn read(output: &Value, field: &Field) -> String {
    let at = |path: &str| -> Option<&Value> {
        if path.is_empty() {
            return Some(output);
        }
        path.split('.').try_fold(output, |v, key| v.get(key))
    };
    match field {
        Field::Scalar(path) => match at(path) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Bool(b)) => b.to_string(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Null) | None => "∅".to_string(),
            Some(_) => "∅".to_string(),
        },
        Field::Len(path) => match at(path) {
            Some(Value::Array(a)) => match a.len() {
                0 => "0".to_string(),
                1 => "1".to_string(),
                _ => "2+".to_string(),
            },
            _ => "∅".to_string(),
        },
    }
}

fn fields_of(output: &Value) -> Vec<Field> {
    let mut out = Vec::new();
    match output {
        Value::Array(_) => out.push(Field::Len(String::new())),
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Array(_) => out.push(Field::Len(k.clone())),
                    Value::Object(inner) => {
                        for (k2, v2) in inner {
                            if !matches!(v2, Value::Array(_) | Value::Object(_)) {
                                out.push(Field::Scalar(format!("{k}.{k2}")));
                            }
                        }
                    }
                    _ => out.push(Field::Scalar(k.clone())),
                }
            }
        }
        _ => {}
    }
    out
}

/// A candidate field and the values it took in training.
#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    /// Tool whose output holds the field.
    pub tool: String,
    /// The field.
    pub field: Field,
    /// Values seen, with counts.
    pub values: BTreeMap<String, usize>,
}

/// Enum-like fields in the tool outputs of `episodes`: fields that take
/// between 2 and `max_values` distinct values, none of them in more than 98%
/// of that tool's outputs. Identifiers, prices and free text drop out
/// automatically.
pub fn discover(episodes: &[&[StepOutput]], max_values: usize) -> Vec<Candidate> {
    let mut seen: BTreeMap<(String, Field), BTreeMap<String, usize>> = BTreeMap::new();
    let mut calls: HashMap<String, usize> = HashMap::new();
    for outputs in episodes {
        for s in outputs.iter() {
            let (Some(tool), Some(output)) = (&s.tool, &s.output) else {
                continue;
            };
            *calls.entry(tool.clone()).or_insert(0) += 1;
            for field in fields_of(output) {
                let v = read(output, &field);
                *seen
                    .entry((tool.clone(), field))
                    .or_default()
                    .entry(v)
                    .or_insert(0) += 1;
            }
        }
    }
    seen.into_iter()
        .filter_map(|((tool, field), mut values)| {
            let n = calls[&tool];
            let present: usize = values.values().sum();
            if present < n {
                *values.entry("∅".to_string()).or_insert(0) += n - present;
            }
            let top = values.values().copied().max().unwrap_or(0);
            let ok =
                values.len() >= 2 && values.len() <= max_values && (top as f64) <= 0.98 * n as f64;
            ok.then_some(Candidate {
                tool,
                field,
                values,
            })
        })
        .collect()
}

/// Feature ids per tool, `1..MAX_IDS`; 0 means "no feature".
pub const MAX_IDS: u32 = 64;

/// Maps each tool output to a feature id from a chosen set of fields.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeatureMap {
    fields: BTreeMap<String, Vec<Field>>,
    #[serde(with = "crate::pairs")]
    ids: HashMap<(String, Vec<String>), u32>,
}

impl FeatureMap {
    /// Build ids for `fields` from the value combinations seen in `episodes`.
    /// Each tool gets its own id space, `1..MAX_IDS`; unseen and overflow
    /// combinations share id 0 with "no feature".
    pub fn fit(fields: &[(String, Field)], episodes: &[&[StepOutput]]) -> Self {
        let mut by_tool: BTreeMap<String, Vec<Field>> = BTreeMap::new();
        for (tool, field) in fields {
            by_tool.entry(tool.clone()).or_default().push(field.clone());
        }
        let mut map = FeatureMap {
            fields: by_tool,
            ids: HashMap::new(),
        };
        let mut next: HashMap<String, u32> = HashMap::new();
        let mut combos: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
        for outputs in episodes {
            for s in outputs.iter() {
                if let Some(key) = map.key(s) {
                    combos.insert(key);
                }
            }
        }
        for key in combos {
            let n = next.entry(key.0.clone()).or_insert(1);
            if *n < MAX_IDS {
                map.ids.insert(key, *n);
                *n += 1;
            }
        }
        map
    }

    fn key(&self, s: &StepOutput) -> Option<(String, Vec<String>)> {
        let tool = s.tool.as_ref()?;
        let fields = self.fields.get(tool)?;
        let output = s.output.as_ref()?;
        Some((
            tool.clone(),
            fields.iter().map(|f| read(output, f)).collect(),
        ))
    }

    /// What feature `id` of `tool` stands for, as `field=value` pairs.
    pub fn describe(&self, tool: &str, id: u32) -> Option<String> {
        let (_, values) = self.ids.iter().find(|((t, _), &i)| t == tool && i == id)?.0;
        Some(
            self.fields[tool]
                .iter()
                .zip(values)
                .map(|(f, v)| format!("{f}={v}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }

    /// Feature id of every step.
    pub fn features(&self, outputs: &[StepOutput]) -> Vec<u32> {
        outputs
            .iter()
            .map(|s| {
                self.key(s)
                    .and_then(|k| self.ids.get(&k).copied())
                    .unwrap_or(0)
            })
            .collect()
    }
}

/// A field chosen by [`select`].
#[derive(Clone, Debug, Serialize)]
pub struct Selected {
    /// Tool whose output holds the field.
    pub tool: String,
    /// The field.
    pub field: Field,
    /// Distinct values it took in training.
    pub values: usize,
    /// Increase in the cross-validated log-likelihood of held-out tasks
    /// (nats) when the field was added.
    pub gain: f64,
}

/// One training episode for [`select`].
pub struct TrainEpisode<'a> {
    /// Abstract steps.
    pub steps: &'a [Step],
    /// Outputs aligned with `steps`.
    pub outputs: &'a [StepOutput],
    /// Group for cross-validation: episodes of the same task share it.
    pub group: u64,
}

/// Folds used by [`select`]'s grouped cross-validation.
pub const FOLDS: u64 = 5;

/// Greedy forward selection of candidate fields by grouped cross-validated
/// log-likelihood, at context length `order` and concentration `alpha`.
/// Stops when no candidate adds at least `min_gain` nats, or after
/// `max_fields` fields.
pub fn select(
    train: &[TrainEpisode<'_>],
    candidates: &[Candidate],
    vocab: &Vocab,
    order: usize,
    alpha: f64,
    min_gain: f64,
    max_fields: usize,
) -> Vec<Selected> {
    let outputs: Vec<&[StepOutput]> = train.iter().map(|e| e.outputs).collect();
    let evidence = |fields: &[(String, Field)]| {
        let map = FeatureMap::fit(fields, &outputs);
        let encoded: Vec<(u64, EncodedEpisode)> = train
            .iter()
            .map(|e| {
                let f = map.features(e.outputs);
                let enc = EncodedEpisode::encode_with_features(e.steps, Some(&f), vocab, true);
                (e.group % FOLDS, enc)
            })
            .collect();
        (0..FOLDS)
            .map(|fold| {
                let (held, fit): (Vec<_>, Vec<_>) = encoded.iter().partition(|(g, _)| *g == fold);
                let fit: Vec<EncodedEpisode> = fit.into_iter().map(|(_, e)| e.clone()).collect();
                let held: Vec<EncodedEpisode> = held.into_iter().map(|(_, e)| e.clone()).collect();
                let model = BackoffModel::fit(order, alpha, vocab.len(), &fit);
                log_likelihood(&model, &held)
            })
            .sum::<f64>()
    };

    let mut chosen: Vec<(String, Field)> = Vec::new();
    let mut picked: Vec<Selected> = Vec::new();
    let mut current = evidence(&chosen);
    while picked.len() < max_fields {
        let best = candidates
            .iter()
            .filter(|c| !chosen.iter().any(|(t, f)| *t == c.tool && *f == c.field))
            .map(|c| {
                let mut trial = chosen.clone();
                trial.push((c.tool.clone(), c.field.clone()));
                (c, evidence(&trial))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let Some((c, ll)) = best else { break };
        if ll - current < min_gain {
            break;
        }
        chosen.push((c.tool.clone(), c.field.clone()));
        picked.push(Selected {
            tool: c.tool.clone(),
            field: c.field.clone(),
            values: c.values.len(),
            gain: ll - current,
        });
        current = ll;
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstraction::{Action, Outcome};
    use serde_json::json;

    fn out(tool: &str, v: Value) -> StepOutput {
        StepOutput {
            tool: Some(tool.into()),
            output: Some(v),
        }
    }

    #[test]
    fn reads_scalars_nested_fields_and_lengths() {
        let v = json!({"status": "pending", "user": {"tier": "gold"}, "items": [1, 2, 3]});
        assert_eq!(read(&v, &Field::Scalar("status".into())), "pending");
        assert_eq!(read(&v, &Field::Scalar("user.tier".into())), "gold");
        assert_eq!(read(&v, &Field::Len("items".into())), "2+");
        assert_eq!(read(&v, &Field::Scalar("missing".into())), "∅");
        assert_eq!(read(&json!([]), &Field::Len(String::new())), "0");
    }

    #[test]
    fn discovers_enum_like_fields_only() {
        let eps: Vec<Vec<StepOutput>> = (0..40)
            .map(|i| {
                vec![out(
                    "get_order",
                    json!({
                        "order_id": format!("#W{i}"),
                        "status": if i % 2 == 0 { "pending" } else { "delivered" },
                        "items": vec![0; i % 3],
                        "currency": "usd"
                    }),
                )]
            })
            .collect();
        let refs: Vec<&[StepOutput]> = eps.iter().map(|e| e.as_slice()).collect();
        let found: Vec<String> = discover(&refs, 8)
            .into_iter()
            .map(|c| c.field.to_string())
            .collect();
        assert_eq!(found, vec!["status", "len(items)"]);
    }

    #[test]
    fn selection_keeps_the_field_that_decides_the_branch() {
        // The agent cancels pending orders and returns delivered ones; only
        // the status field tells the two apart.
        let tools = ["get_order", "cancel", "return"];
        let mut all_steps = Vec::new();
        let mut all_outputs = Vec::new();
        for i in 0..60 {
            let pending = i % 2 == 0;
            let noise = if i % 3 == 0 { "a" } else { "b" };
            all_steps.push(vec![
                Step {
                    action: Action::Tool("get_order".into()),
                    outcome: Outcome::Ok,
                },
                Step {
                    action: Action::Tool(if pending { "cancel" } else { "return" }.into()),
                    outcome: Outcome::Ok,
                },
            ]);
            all_outputs.push(vec![
                out(
                    "get_order",
                    json!({"status": if pending { "pending" } else { "delivered" }, "noise": noise}),
                ),
                out("x", json!({})),
            ]);
        }
        let vocab = Vocab::build(all_steps.iter().flatten(), tools);
        let refs: Vec<&[StepOutput]> = all_outputs.iter().map(|o| o.as_slice()).collect();
        let candidates = discover(&refs, 8);
        let train: Vec<TrainEpisode> = all_steps
            .iter()
            .zip(&all_outputs)
            .enumerate()
            .map(|(i, (s, o))| TrainEpisode {
                steps: s,
                outputs: o,
                group: i as u64,
            })
            .collect();
        let picked = select(&train, &candidates, &vocab, 1, 1.0, 1.0, 4);
        assert_eq!(picked.len(), 1, "{picked:?}");
        assert_eq!(picked[0].field, Field::Scalar("status".into()));
        assert!(picked[0].gain > 10.0);

        let map = FeatureMap::fit(&[("get_order".into(), picked[0].field.clone())], &refs);
        let f = map.features(&all_outputs[0]);
        assert!(f[0] > 0 && f[1] == 0);
        assert_ne!(map.features(&all_outputs[1])[0], f[0]);
        let described = map.describe("get_order", f[0]).expect("a fitted id");
        assert!(described.starts_with("status="), "{described}");
        assert_eq!(map.describe("get_order", 999), None);
    }
}
