//! Where tool-argument values came from.
//!
//! Each leaf value of each tool call's arguments is looked up, as lower-cased
//! text, in everything the user had said so far and in every tool output so
//! far. A value found in neither was produced by the agent. This is the
//! verbatim-match idea behind TraceCompiler's provenance classes, without its
//! uniqueness checks. Very short values match by coincidence, so values under
//! three characters get their own class instead of a guess.

use serde::Serialize;
use stretto_trace::{Episode, Event};

/// Where an argument value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Source {
    /// Said by the user, and not seen in any tool output.
    User,
    /// Seen in an earlier tool output, and not said by the user.
    ToolOutput,
    /// Both said by the user and seen in a tool output.
    Both,
    /// Found in neither: the agent produced it.
    Generated,
    /// A boolean or null: a closed set, nothing to trace.
    Literal,
    /// Too short to attribute reliably.
    Short,
}

impl Source {
    /// Every class, in report order.
    pub const ALL: [Source; 6] = [
        Source::User,
        Source::ToolOutput,
        Source::Both,
        Source::Generated,
        Source::Literal,
        Source::Short,
    ];
}

/// One argument value and its source.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArgUse {
    /// Tool name.
    pub tool: String,
    /// Top-level argument name.
    pub arg: String,
    /// Where the value came from.
    pub source: Source,
}

/// Every leaf argument value of every tool call in `ep`, grouped by call in
/// call order: `(argument, source, value)` per leaf.
pub fn call_sources(ep: &Episode) -> Vec<Vec<(String, Source, String)>> {
    let mut user_text = String::new();
    let mut tool_text = String::new();
    let mut out = Vec::new();
    for e in &ep.events {
        match e {
            Event::User { text } => {
                user_text.push_str(&text.to_lowercase());
                user_text.push('\n');
            }
            Event::ToolResult { content, .. } => {
                tool_text.push_str(&content.to_lowercase());
                tool_text.push('\n');
            }
            Event::Assistant { calls, .. } => {
                for c in calls {
                    let mut leaves = Vec::new();
                    collect_leaves(None, &c.arguments, &mut leaves);
                    out.push(
                        leaves
                            .into_iter()
                            .map(|(arg, value)| {
                                let source = classify(&value, &user_text, &tool_text);
                                let text = match &value {
                                    serde_json::Value::String(s) => s.trim().to_lowercase(),
                                    other => other.to_string(),
                                };
                                (arg, source, text)
                            })
                            .collect(),
                    );
                }
            }
        }
    }
    out
}

/// Classify every leaf argument value of every tool call in `ep`.
pub fn argument_provenance(ep: &Episode) -> Vec<ArgUse> {
    let mut user_text = String::new();
    let mut tool_text = String::new();
    let mut out = Vec::new();
    for e in &ep.events {
        match e {
            Event::User { text } => {
                user_text.push_str(&text.to_lowercase());
                user_text.push('\n');
            }
            Event::ToolResult { content, .. } => {
                tool_text.push_str(&content.to_lowercase());
                tool_text.push('\n');
            }
            Event::Assistant { calls, .. } => {
                for c in calls {
                    let mut leaves = Vec::new();
                    collect_leaves(None, &c.arguments, &mut leaves);
                    for (arg, value) in leaves {
                        out.push(ArgUse {
                            tool: c.name.clone(),
                            arg,
                            source: classify(&value, &user_text, &tool_text),
                        });
                    }
                }
            }
        }
    }
    out
}

fn collect_leaves(
    top: Option<&str>,
    v: &serde_json::Value,
    out: &mut Vec<(String, serde_json::Value)>,
) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, child) in map {
                collect_leaves(Some(top.unwrap_or(k)), child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_leaves(top, child, out);
            }
        }
        leaf => out.push((top.unwrap_or("").to_string(), leaf.clone())),
    }
}

fn classify(v: &serde_json::Value, user_text: &str, tool_text: &str) -> Source {
    let text = match v {
        serde_json::Value::String(s) => s.trim().to_lowercase(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return Source::Literal,
    };
    if text.chars().count() < 3 {
        return Source::Short;
    }
    match (user_text.contains(&text), tool_text.contains(&text)) {
        (true, true) => Source::Both,
        (true, false) => Source::User,
        (false, true) => Source::ToolOutput,
        (false, false) => Source::Generated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_trace::ToolCall;

    #[test]
    fn classifies_by_where_values_appeared() {
        let ep = Episode {
            id: "e".into(),
            task_id: "t".into(),
            trial: 0,
            domain: "retail".into(),
            agent_model: "m".into(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "Cancel order #W123, I no longer need it".into(),
                },
                Event::Assistant {
                    usage: None,
                    text: None,
                    calls: vec![ToolCall {
                        id: "1".into(),
                        name: "get_order_details".into(),
                        arguments: serde_json::json!({"order_id": "#W123"}),
                    }],
                },
                Event::ToolResult {
                    call_id: "1".into(),
                    name: "get_order_details".into(),
                    error: false,
                    content: r##"{"order_id": "#W123", "items": [{"item_id": "9876543"}]}"##.into(),
                },
                Event::Assistant {
                    usage: None,
                    text: None,
                    calls: vec![ToolCall {
                        id: "2".into(),
                        name: "cancel_pending_order".into(),
                        arguments: serde_json::json!({
                            "order_id": "#W123",
                            "reason": "no longer needed",
                            "item_ids": ["9876543"],
                            "flag": true,
                            "n": 1
                        }),
                    }],
                },
            ],
        };
        let uses = argument_provenance(&ep);
        let get = |arg: &str| {
            uses.iter()
                .filter(|u| u.tool == "cancel_pending_order" && u.arg == arg)
                .map(|u| u.source)
                .next()
                .unwrap()
        };
        assert_eq!(uses[0].source, Source::User);
        assert_eq!(get("order_id"), Source::Both);
        assert_eq!(get("reason"), Source::Generated);
        assert_eq!(get("item_ids"), Source::ToolOutput);
        assert_eq!(get("flag"), Source::Literal);
        assert_eq!(get("n"), Source::Short);
    }
}
