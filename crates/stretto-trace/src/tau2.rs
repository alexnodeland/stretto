//! τ²-bench results files and domain metadata.
//!
//! A results file (`data/tau2/results/final/*.json`) holds `info`, `tasks` and
//! `simulations`. Each simulation's `messages` interleave assistant, user and
//! tool messages. Tool messages carry the call id but not the tool name, so the
//! name is recovered from the call that produced it.
//!
//! τ²-bench opens every conversation with a fixed assistant greeting that costs
//! nothing and is not an LLM decision; ingest drops it.

use crate::{Episode, Event, ToolCall, ToolDoc, ToolKind, ToolManifest, TurnUsage};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// One results file, converted.
#[derive(Clone, Debug)]
pub struct Tau2Run {
    /// Domain, from `environment_info.domain_name`.
    pub domain: String,
    /// Model that drove the agent.
    pub agent_model: String,
    /// Model that simulated the user.
    pub user_model: Option<String>,
    /// The domain policy the agent was given.
    pub policy: Option<String>,
    /// Gold assistant actions from each task's evaluation criteria, by task id.
    pub gold_actions: BTreeMap<String, Vec<GoldAction>>,
    /// Episodes, one per simulation.
    pub episodes: Vec<Episode>,
}

/// One expected action from a task's evaluation criteria.
#[derive(Clone, Debug, PartialEq)]
pub struct GoldAction {
    /// Tool name.
    pub name: String,
    /// Expected arguments.
    pub arguments: serde_json::Value,
}

/// Official task split (`data/tau2/domains/<domain>/split_tasks.json`).
#[derive(Clone, Debug, Deserialize)]
pub struct Split {
    /// Task ids used for compiling flows.
    pub train: Vec<String>,
    /// Held-out task ids.
    pub test: Vec<String>,
}

/// Load and convert a results file.
pub fn load_results(path: &Path) -> Result<Tau2Run> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_results(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Load a domain's official task split.
pub fn load_split(path: &Path) -> Result<Split> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Load a domain's tool manifest from its `tools.py`.
pub fn load_manifest(domain: &str, tools_py: &Path) -> Result<ToolManifest> {
    let text = std::fs::read_to_string(tools_py)
        .with_context(|| format!("reading {}", tools_py.display()))?;
    Ok(parse_tool_kinds(domain, &text))
}

/// Convert the JSON text of a results file.
pub fn parse_results(json: &str) -> Result<Tau2Run> {
    let raw: RawRun = serde_json::from_str(json)?;
    let domain = raw.info.environment_info.domain_name;
    let agent_model = raw
        .info
        .agent_info
        .llm
        .unwrap_or_else(|| "unknown".to_string());
    let user_model = raw.info.user_info.and_then(|u| u.llm);

    let gold_actions = raw
        .tasks
        .into_iter()
        .map(|t| {
            let actions = t
                .evaluation_criteria
                .and_then(|c| c.actions)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a.requestor.as_deref().unwrap_or("assistant") == "assistant")
                .map(|a| GoldAction {
                    name: a.name,
                    arguments: a.arguments,
                })
                .collect();
            (t.id, actions)
        })
        .collect();

    let episodes = raw
        .simulations
        .into_iter()
        .map(|s| convert_simulation(s, &domain, &agent_model))
        .collect();

    Ok(Tau2Run {
        domain,
        agent_model,
        user_model,
        policy: raw.info.environment_info.policy,
        gold_actions,
        episodes,
    })
}

fn convert_simulation(sim: RawSim, domain: &str, agent_model: &str) -> Episode {
    let mut events = Vec::with_capacity(sim.messages.len());
    let mut call_names: HashMap<String, String> = HashMap::new();

    for (i, m) in sim.messages.into_iter().enumerate() {
        match m.role.as_str() {
            "assistant" => {
                let calls: Vec<ToolCall> = m
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| c.requestor.as_deref().unwrap_or("assistant") == "assistant")
                    .map(|c| ToolCall {
                        id: c.id,
                        name: c.name,
                        arguments: c.arguments,
                    })
                    .collect();
                // The opening greeting is scripted, free, and not a decision.
                if i == 0 && calls.is_empty() && m.cost.unwrap_or(0.0) == 0.0 {
                    continue;
                }
                for c in &calls {
                    call_names.insert(c.id.clone(), c.name.clone());
                }
                let usage = m.usage.map(|u| TurnUsage {
                    prompt_tokens: u.prompt_tokens.unwrap_or(0),
                    completion_tokens: u.completion_tokens.unwrap_or(0),
                    cost: m.cost.unwrap_or(0.0),
                });
                events.push(Event::Assistant {
                    text: m.content.filter(|t| !t.is_empty()),
                    calls,
                    usage,
                });
            }
            "user" => events.push(Event::User {
                text: m.content.unwrap_or_default(),
            }),
            "tool" => {
                let call_id = m.id.unwrap_or_default();
                // Results of tools the *user* called (telecom's dual control)
                // have no assistant call; skip them.
                if let Some(name) = call_names.get(&call_id) {
                    events.push(Event::ToolResult {
                        name: name.clone(),
                        call_id,
                        error: m.error.unwrap_or(false),
                        content: m.content.unwrap_or_default(),
                    });
                }
            }
            _ => {}
        }
    }

    Episode {
        id: sim.id,
        task_id: sim.task_id,
        trial: sim.trial.unwrap_or(0),
        domain: domain.to_string(),
        agent_model: agent_model.to_string(),
        reward: sim.reward_info.map(|r| r.reward).unwrap_or(0.0),
        events,
    }
}

/// Parse the `@is_tool(ToolType.X)` decorators of a τ²-bench `tools.py`,
/// and each tool's docstring.
///
/// Each decorator applies to the next `def name(` line. Commented-out tools
/// (`# @is_tool(...)`) are ignored. The docstring's opening paragraphs become
/// the summary, and its `Args:` section the argument descriptions.
pub fn parse_tool_kinds(domain: &str, tools_py: &str) -> ToolManifest {
    let mut tools = BTreeMap::new();
    let mut docs = BTreeMap::new();
    let mut pending: Option<ToolKind> = None;
    let lines: Vec<&str> = tools_py.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("@is_tool(ToolType.") {
            let kind = rest.split(')').next().unwrap_or("");
            pending = match kind {
                "READ" => Some(ToolKind::Read),
                "WRITE" => Some(ToolKind::Write),
                _ => Some(ToolKind::Generic),
            };
        } else if let Some(rest) = t.strip_prefix("def ") {
            if let Some(kind) = pending.take() {
                let name = rest.split('(').next().unwrap_or("").trim();
                if !name.is_empty() {
                    tools.insert(name.to_string(), kind);
                    if let Some(doc) = docstring(&lines[i..]) {
                        docs.insert(name.to_string(), doc);
                    }
                }
            }
        }
    }
    ToolManifest {
        domain: domain.to_string(),
        tools,
        docs,
    }
}

/// The docstring of the function whose `def` line starts `lines`, if any.
fn docstring(lines: &[&str]) -> Option<ToolDoc> {
    // The signature may span lines; it ends at the first line ending in ':'.
    let body = lines.iter().position(|l| l.trim_end().ends_with(':'))? + 1;
    let first = lines.get(body)?.trim();
    let open = first.strip_prefix("\"\"\"")?;
    let mut text: Vec<String> = Vec::new();
    if let Some(one_line) = open.strip_suffix("\"\"\"") {
        text.push(one_line.to_string());
    } else {
        text.push(open.to_string());
        for l in &lines[body + 1..] {
            let l = l.trim();
            if let Some(last) = l.strip_suffix("\"\"\"") {
                text.push(last.to_string());
                break;
            }
            text.push(l.to_string());
        }
    }
    let mut doc = ToolDoc::default();
    let mut summary: Vec<&str> = Vec::new();
    let mut section = "";
    let mut current: Option<String> = None;
    for l in &text {
        let l = l.as_str();
        if matches!(
            l,
            "Args:" | "Returns:" | "Raises:" | "Example:" | "Examples:"
        ) {
            section = l;
            continue;
        }
        match section {
            "" if !l.is_empty() => summary.push(l),
            "Args:" if !l.is_empty() => match l.split_once(':') {
                Some((name, desc)) if !name.contains(' ') => {
                    doc.args.insert(name.to_string(), desc.trim().to_string());
                    current = Some(name.to_string());
                }
                _ => {
                    if let Some(d) = current.as_ref().and_then(|n| doc.args.get_mut(n)) {
                        d.push(' ');
                        d.push_str(l);
                    }
                }
            },
            _ => {}
        }
    }
    doc.summary = summary.join(" ");
    (!doc.summary.is_empty() || !doc.args.is_empty()).then_some(doc)
}

#[derive(Deserialize)]
struct RawRun {
    info: RawInfo,
    #[serde(default)]
    tasks: Vec<RawTask>,
    simulations: Vec<RawSim>,
}

#[derive(Deserialize)]
struct RawInfo {
    agent_info: RawAgentInfo,
    user_info: Option<RawAgentInfo>,
    environment_info: RawEnvInfo,
}

#[derive(Deserialize)]
struct RawAgentInfo {
    llm: Option<String>,
}

#[derive(Deserialize)]
struct RawEnvInfo {
    domain_name: String,
    policy: Option<String>,
}

#[derive(Deserialize)]
struct RawTask {
    id: String,
    evaluation_criteria: Option<RawCriteria>,
}

#[derive(Deserialize)]
struct RawCriteria {
    actions: Option<Vec<RawAction>>,
}

#[derive(Deserialize)]
struct RawAction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
    requestor: Option<String>,
}

#[derive(Deserialize)]
struct RawSim {
    id: String,
    task_id: String,
    trial: Option<u32>,
    reward_info: Option<RawReward>,
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawReward {
    reward: f64,
}

#[derive(Deserialize)]
struct RawMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<RawToolCall>>,
    id: Option<String>,
    error: Option<bool>,
    cost: Option<f64>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct RawToolCall {
    id: String,
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
    requestor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = r##"{
      "info": {
        "agent_info": {"llm": "agent-x"},
        "user_info": {"llm": "user-y"},
        "environment_info": {"domain_name": "retail", "policy": "be nice", "tool_defs": null}
      },
      "tasks": [{"id": "7", "evaluation_criteria": {"actions": [
        {"name": "get_order_details", "arguments": {"order_id": "#W1"}, "requestor": "assistant"}
      ]}}],
      "simulations": [{
        "id": "s1", "task_id": "7", "trial": 2, "reward_info": {"reward": 1.0},
        "messages": [
          {"role": "assistant", "content": "Hi! How can I help you today?", "cost": 0.0},
          {"role": "user", "content": "Where is order #W1?"},
          {"role": "assistant", "content": "Checking.", "cost": 0.01,
           "usage": {"prompt_tokens": 1200, "completion_tokens": 30},
           "tool_calls": [{"id": "c1", "name": "get_order_details",
                           "arguments": {"order_id": "#W1"}, "requestor": "assistant"}]},
          {"role": "tool", "id": "c1", "content": "{\"status\": \"pending\"}", "error": false},
          {"role": "assistant", "content": "It is pending.", "cost": 0.01}
        ]
      }]
    }"##;

    #[test]
    fn converts_a_simulation() {
        let run = parse_results(RUN).unwrap();
        assert_eq!(run.domain, "retail");
        assert_eq!(run.agent_model, "agent-x");
        assert_eq!(run.user_model.as_deref(), Some("user-y"));
        assert_eq!(run.gold_actions["7"][0].name, "get_order_details");

        let ep = &run.episodes[0];
        assert!(ep.succeeded());
        assert_eq!(ep.trial, 2);
        // The free greeting is dropped: user, assistant(call), result, assistant(reply).
        assert_eq!(ep.events.len(), 4);
        assert_eq!(ep.assistant_turns(), 2);
        let usage = ep.turn_usage();
        assert_eq!(usage[0].map(|u| u.prompt_tokens), Some(1200));
        assert_eq!(usage[0].map(|u| u.cost), Some(0.01));
        assert_eq!(usage[1], None);
        match &ep.events[2] {
            Event::ToolResult {
                name,
                error,
                content,
                ..
            } => {
                assert_eq!(name, "get_order_details");
                assert!(!error);
                assert!(content.contains("pending"));
            }
            other => panic!("expected a tool result, got {other:?}"),
        }
    }

    #[test]
    fn parses_tool_kinds() {
        let py = r#"
    @is_tool(ToolType.READ)
    def get_order_details(self, order_id: str) -> Order:
        ...
    @is_tool(ToolType.WRITE)
    def cancel_pending_order(
        self, order_id: str, reason: str
    ) -> Order:
        ...
    # @is_tool(ToolType.THINK)
    # def think(self, thought: str) -> str:
    @is_tool(ToolType.GENERIC)
    def transfer_to_human_agents(self, summary: str) -> str:
        ...
    def helper(self):
        ...
"#;
        let m = parse_tool_kinds("retail", py);
        assert_eq!(m.tools.len(), 3);
        assert_eq!(m.kind("get_order_details"), Some(ToolKind::Read));
        assert!(m.is_write("cancel_pending_order"));
        assert_eq!(m.kind("transfer_to_human_agents"), Some(ToolKind::Generic));
        assert_eq!(m.kind("think"), None);
        assert_eq!(m.kind("helper"), None);
    }

    #[test]
    fn parses_docstrings() {
        let py = r#"
    @is_tool(ToolType.WRITE)
    def cancel_pending_order(
        self, order_id: str, reason: str
    ) -> Order:
        """Cancel a pending order. If the order is already processed or delivered,
        it cannot be cancelled.

        Args:
            order_id: The order id, such as '#W0000000'. Be careful there is a '#'
                symbol at the beginning of the order id.
            reason: The reason for cancellation, which should be either
                'no longer needed' or 'ordered by mistake'.

        Returns:
            Order: The order details after the cancellation.
        """
        order = self._get_order(order_id)
    @is_tool(ToolType.GENERIC)
    def think(self, thought: str) -> str:
        """Use the tool to think about something."""
        return ""
    @is_tool(ToolType.READ)
    def undocumented(self) -> str:
        return ""
"#;
        let m = parse_tool_kinds("retail", py);
        let doc = &m.docs["cancel_pending_order"];
        assert_eq!(
            doc.summary,
            "Cancel a pending order. If the order is already processed or delivered, it cannot be cancelled."
        );
        assert_eq!(
            doc.args["reason"],
            "The reason for cancellation, which should be either 'no longer needed' or 'ordered by mistake'."
        );
        assert!(doc.args["order_id"].ends_with("beginning of the order id."));
        assert_eq!(
            m.docs["think"].summary,
            "Use the tool to think about something."
        );
        assert!(!m.docs.contains_key("undocumented"));
    }
}
