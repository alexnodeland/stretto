//! Canonical agent episodes and trace ingest.
//!
//! An [`Episode`] is one conversation between an agent, a user and a set of
//! tools, flattened into [`Event`]s in the order they happened. Everything
//! downstream (abstraction, world model, reports) reads episodes, never a
//! benchmark's native format; the [`tau2`] module converts τ²-bench results,
//! and the [`mcp`] module converts logs recorded by `stretto-proxy`.

pub mod mcp;
pub mod redact;
pub mod tau2;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One tool call made by the agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Call id, used to match the call with its result.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Arguments as sent (a JSON object).
    pub arguments: serde_json::Value,
}

/// One thing that happened in an episode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A user turn.
    User { text: String },
    /// An assistant turn (one LLM call): tool calls, or a reply to the user
    /// when `calls` is empty.
    Assistant {
        text: Option<String>,
        calls: Vec<ToolCall>,
        /// Tokens and cost of this LLM call, when the source recorded them.
        #[serde(default)]
        usage: Option<TurnUsage>,
    },
    /// The result of an assistant tool call.
    ToolResult {
        call_id: String,
        name: String,
        error: bool,
        content: String,
    },
}

/// Tokens and cost of one LLM call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnUsage {
    /// Input tokens: the whole context the model read.
    pub prompt_tokens: u64,
    /// Output tokens, including any reasoning tokens.
    pub completion_tokens: u64,
    /// Cost in dollars, as the source computed it.
    pub cost: f64,
}

/// A complete episode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Episode {
    /// Unique id of this run of the task.
    pub id: String,
    /// Benchmark task id.
    pub task_id: String,
    /// Trial index when a task was run several times.
    pub trial: u32,
    /// Benchmark domain, e.g. `retail`.
    pub domain: String,
    /// Model that drove the agent.
    pub agent_model: String,
    /// Benchmark reward in `[0, 1]`; `1.0` means the task was solved.
    pub reward: f64,
    /// Events in order.
    pub events: Vec<Event>,
}

impl Episode {
    /// Whether the benchmark counted the task as solved.
    pub fn succeeded(&self) -> bool {
        self.reward >= 1.0 - 1e-9
    }

    /// Assistant turns, i.e. LLM calls made by the agent.
    pub fn assistant_turns(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, Event::Assistant { .. }))
            .count()
    }

    /// Usage of each assistant turn, in order (`None` where unrecorded).
    pub fn turn_usage(&self) -> Vec<Option<TurnUsage>> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Assistant { usage, .. } => Some(*usage),
                _ => None,
            })
            .collect()
    }

    /// All tool calls, in order.
    pub fn tool_calls(&self) -> impl Iterator<Item = &ToolCall> {
        self.events.iter().flat_map(|e| match e {
            Event::Assistant { calls, .. } => calls.as_slice(),
            _ => &[],
        })
    }

    /// The episode as it stands for a decision made when the call `call_id`
    /// returns, and the calls of its turn still awaiting a response.
    ///
    /// In the call's turn, the calls that have returned (a
    /// [`Event::ToolResult`] follows) are kept, with this one last, so the
    /// episode's last step is this call. The others are set apart: the agent
    /// has asked for them already, so a lookup of one would spare it
    /// nothing. An episode where no turn has the call is returned as it is.
    pub fn after_call(&self, call_id: &str) -> (Episode, Vec<ToolCall>) {
        let returned: std::collections::HashSet<&str> = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::ToolResult { call_id, .. } => Some(call_id.as_str()),
                _ => None,
            })
            .collect();
        let mut episode = self.clone();
        let mut asked = Vec::new();
        let turn = episode.events.iter_mut().find_map(|e| match e {
            Event::Assistant { calls, .. } if calls.iter().any(|c| c.id == call_id) => Some(calls),
            _ => None,
        });
        if let Some(calls) = turn {
            let (mut kept, mut this) = (Vec::new(), Vec::new());
            for c in std::mem::take(calls) {
                if c.id == call_id {
                    this.push(c);
                } else if returned.contains(c.id.as_str()) {
                    kept.push(c);
                } else {
                    asked.push(c);
                }
            }
            kept.append(&mut this);
            *calls = kept;
        }
        (episode, asked)
    }
}

/// Whether a tool reads state, writes it, or neither (τ²-bench's `ToolType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Reads the environment without changing it.
    Read,
    /// Changes the environment (database writes).
    Write,
    /// Neither, e.g. a calculator or a transfer to a human. In a manifest
    /// read from MCP annotations ([`mcp::manifest`]), a tool that gave no
    /// `readOnlyHint`, so its kind is unknown.
    Generic,
}

/// The tools a domain exposes, with their kinds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolManifest {
    /// Domain name.
    pub domain: String,
    /// Tool name to kind.
    pub tools: BTreeMap<String, ToolKind>,
    /// Tool name to its documentation, where the source has any.
    #[serde(default)]
    pub docs: BTreeMap<String, ToolDoc>,
}

/// What a tool's documentation says about it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolDoc {
    /// What the tool does, as one paragraph.
    pub summary: String,
    /// Argument name to its description.
    pub args: BTreeMap<String, String>,
}

impl ToolManifest {
    /// The kind of `name`, if the manifest lists it.
    pub fn kind(&self, name: &str) -> Option<ToolKind> {
        self.tools.get(name).copied()
    }

    /// Whether `name` is a write tool.
    pub fn is_write(&self, name: &str) -> bool {
        self.kind(name) == Some(ToolKind::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, order: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: "get_order".to_string(),
            arguments: json!({"order_id": order}),
        }
    }

    fn result(id: &str) -> Event {
        Event::ToolResult {
            call_id: id.to_string(),
            name: "get_order".to_string(),
            error: false,
            content: "{}".to_string(),
        }
    }

    /// The ids of the episode's calls, in order.
    fn ids(episode: &Episode) -> Vec<&str> {
        episode.tool_calls().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn after_a_call_its_turns_calls_still_in_flight_are_set_apart() {
        let episode = Episode {
            id: "e".to_string(),
            task_id: "t".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "agent".to_string(),
            reward: 1.0,
            events: vec![
                Event::User {
                    text: "My orders?".to_string(),
                },
                Event::Assistant {
                    text: None,
                    calls: vec![call("1", "#W1"), call("2", "#W2"), call("3", "#W3")],
                    usage: None,
                },
                // The second call answered first, then the third.
                result("2"),
                result("3"),
            ],
        };
        // When the third returns, the second has too; the first is still
        // on its way, so the agent has asked for it already.
        let (view, asked) = episode.after_call("3");
        assert_eq!(ids(&view), ["2", "3"]);
        assert_eq!(asked, [call("1", "#W1")]);
        assert_eq!(view.events[2..], episode.events[2..]);
        // When the second returned, the third was asked for too.
        let upto = Episode {
            events: episode.events[..3].to_vec(),
            ..episode.clone()
        };
        let (view, asked) = upto.after_call("2");
        assert_eq!(ids(&view), ["2"]);
        assert_eq!(asked.len(), 2);
        // A call no turn has leaves the episode as it is.
        let (view, asked) = episode.after_call("9");
        assert_eq!(view.events, episode.events);
        assert!(asked.is_empty());
    }
}
