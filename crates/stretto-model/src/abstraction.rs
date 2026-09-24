//! From episodes to abstract steps.
//!
//! A [`Step`] pairs an abstract [`Action`] with its [`Outcome`]. An action is
//! either a tool (by name; arguments are abstracted away here) or a reply to
//! the user. A tool call's outcome is whether it errored; a reply's outcome is
//! whether the user answered before the episode ended.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use stretto_trace::{Episode, Event};

/// An abstract agent action.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    /// A message to the user with no tool call.
    Respond,
    /// A call to the named tool.
    Tool(String),
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Respond => write!(f, "respond"),
            Action::Tool(name) => write!(f, "{name}"),
        }
    }
}

/// What followed an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Outcome {
    /// The tool call succeeded.
    Ok,
    /// The tool call returned an error (or no result was recorded).
    Err,
    /// The user replied to the agent's message.
    Reply,
    /// The episode ended after the agent's message.
    End,
}

impl Outcome {
    /// Dense index, for encoding.
    pub fn index(self) -> u32 {
        match self {
            Outcome::Ok => 0,
            Outcome::Err => 1,
            Outcome::Reply => 2,
            Outcome::End => 3,
        }
    }

    /// The outcome with dense index `i`.
    pub fn from_index(i: u32) -> Option<Self> {
        [Outcome::Ok, Outcome::Err, Outcome::Reply, Outcome::End]
            .get(i as usize)
            .copied()
    }
}

/// An abstract action and what came back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Step {
    /// What the agent did.
    pub action: Action,
    /// What followed.
    pub outcome: Outcome,
}

/// Abstract an episode into steps, in order.
///
/// An assistant turn with several tool calls becomes several steps.
pub fn steps(ep: &Episode) -> Vec<Step> {
    let errors: HashMap<&str, bool> = ep
        .events
        .iter()
        .filter_map(|e| match e {
            Event::ToolResult { call_id, error, .. } => Some((call_id.as_str(), *error)),
            _ => None,
        })
        .collect();

    let mut out = Vec::new();
    let mut awaiting_reply: Option<usize> = None;
    for e in &ep.events {
        match e {
            Event::Assistant { calls, .. } if calls.is_empty() => {
                awaiting_reply = Some(out.len());
                out.push(Step {
                    action: Action::Respond,
                    outcome: Outcome::End,
                });
            }
            Event::Assistant { calls, .. } => {
                awaiting_reply = None;
                for c in calls {
                    let outcome = match errors.get(c.id.as_str()) {
                        Some(false) => Outcome::Ok,
                        _ => Outcome::Err,
                    };
                    out.push(Step {
                        action: Action::Tool(c.name.clone()),
                        outcome,
                    });
                }
            }
            Event::User { .. } => {
                if let Some(i) = awaiting_reply.take() {
                    out[i].outcome = Outcome::Reply;
                }
            }
            Event::ToolResult { .. } => {}
        }
    }
    out
}

/// The assistant turn (LLM call) each step belongs to, aligned with
/// [`steps`]. Several tool calls in one turn share its index.
pub fn step_turns(ep: &Episode) -> Vec<usize> {
    let mut out = Vec::new();
    let mut turn = 0;
    for e in &ep.events {
        if let Event::Assistant { calls, .. } = e {
            if calls.is_empty() {
                out.push(turn);
            }
            for _ in calls {
                out.push(turn);
            }
            turn += 1;
        }
    }
    out
}

/// Dense ids for actions. `Respond` is always id 0; the last id is reserved
/// for actions never seen when the vocabulary was built.
///
/// It serializes as its list of actions; the index is rebuilt on load.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "Vec<Action>", into = "Vec<Action>")]
pub struct Vocab {
    actions: Vec<Action>,
    index: HashMap<Action, u32>,
}

impl From<Vec<Action>> for Vocab {
    fn from(actions: Vec<Action>) -> Self {
        let index = actions
            .iter()
            .enumerate()
            .map(|(i, a)| (a.clone(), i as u32))
            .collect();
        Self { actions, index }
    }
}

impl From<Vocab> for Vec<Action> {
    fn from(v: Vocab) -> Self {
        v.actions
    }
}

impl Vocab {
    /// Build from the actions in `steps`, plus any extra tool names (e.g. the
    /// domain manifest, so unseen-but-exposed tools get their own id).
    pub fn build<'a>(
        steps: impl IntoIterator<Item = &'a Step>,
        extra_tools: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut names: Vec<String> = steps
            .into_iter()
            .filter_map(|s| match &s.action {
                Action::Tool(n) => Some(n.clone()),
                Action::Respond => None,
            })
            .chain(extra_tools.into_iter().map(str::to_string))
            .collect();
        names.sort();
        names.dedup();
        let mut actions = vec![Action::Respond];
        actions.extend(names.into_iter().map(Action::Tool));
        Self::from(actions)
    }

    /// Number of ids, including the unseen-action id.
    pub fn len(&self) -> usize {
        self.actions.len() + 1
    }

    /// Always false: the unseen-action id is always present.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The id for unseen actions.
    pub fn unseen(&self) -> u32 {
        self.actions.len() as u32
    }

    /// Id of `a` (the unseen id if `a` is not in the vocabulary).
    pub fn id(&self, a: &Action) -> u32 {
        self.index.get(a).copied().unwrap_or(self.unseen())
    }

    /// The action behind an id, if it is not the unseen id.
    pub fn action(&self, id: u32) -> Option<&Action> {
        self.actions.get(id as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_trace::ToolCall;

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    fn episode(events: Vec<Event>) -> Episode {
        Episode {
            id: "e".into(),
            task_id: "t".into(),
            trial: 0,
            domain: "d".into(),
            agent_model: "m".into(),
            reward: 1.0,
            events,
        }
    }

    #[test]
    fn abstracts_calls_replies_and_errors() {
        let ep = episode(vec![
            Event::User { text: "hi".into() },
            Event::Assistant {
                usage: None,
                text: None,
                calls: vec![call("1", "find_user")],
            },
            Event::ToolResult {
                call_id: "1".into(),
                name: "find_user".into(),
                error: true,
                content: "not found".into(),
            },
            Event::Assistant {
                usage: None,
                text: Some("which email?".into()),
                calls: vec![],
            },
            Event::User {
                text: "a@b.c".into(),
            },
            Event::Assistant {
                usage: None,
                text: None,
                calls: vec![call("2", "find_user")],
            },
            Event::ToolResult {
                call_id: "2".into(),
                name: "find_user".into(),
                error: false,
                content: "u1".into(),
            },
            Event::Assistant {
                usage: None,
                text: Some("done".into()),
                calls: vec![],
            },
        ]);
        let s = steps(&ep);
        let got: Vec<(String, Outcome)> = s
            .iter()
            .map(|s| (s.action.to_string(), s.outcome))
            .collect();
        assert_eq!(
            got,
            vec![
                ("find_user".into(), Outcome::Err),
                ("respond".into(), Outcome::Reply),
                ("find_user".into(), Outcome::Ok),
                ("respond".into(), Outcome::End),
            ]
        );
    }

    #[test]
    fn vocab_reserves_respond_and_unseen() {
        let s = [Step {
            action: Action::Tool("b".into()),
            outcome: Outcome::Ok,
        }];
        let v = Vocab::build(s.iter(), ["a"]);
        assert_eq!(v.id(&Action::Respond), 0);
        assert_eq!(v.id(&Action::Tool("a".into())), 1);
        assert_eq!(v.id(&Action::Tool("b".into())), 2);
        assert_eq!(v.id(&Action::Tool("zzz".into())), v.unseen());
        assert_eq!(v.len(), 4);
        assert_eq!(v.action(1), Some(&Action::Tool("a".into())));
        assert_eq!(v.action(v.unseen()), None);
    }
}
