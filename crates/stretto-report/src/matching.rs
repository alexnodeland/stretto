//! Matching what a customer described to a record, with a System-One model.
//!
//! RFC-001 lists matching descriptions to records among the decisions a
//! System-One model could take from the LLM. At each write in τ²-bench's
//! published trajectories that picks records out of earlier tool results,
//! the model is shown what the customer said and the candidates, but not the
//! agent's pick, and asked which one the customer means:
//!
//! - **items**: which items of the order to return, exchange or modify.
//!   Several may be meant, so there is one yes/no question per item.
//! - **variant**: which of its product's variants each exchanged or modified
//!   item becomes.
//! - **payment**: which of the customer's payment methods to use.
//! - **reservation** (airline): which reservation to cancel or change, among
//!   those the agent looked up.
//!
//! A choice is asked only where there are at least two candidates. Its pick,
//! and the agent's, are scored against the task's expected actions: the same
//! write on the same order, or for a reservation, any reservation the task
//! expects that write on.

use crate::shadow::{self, Decision, Kind, ShadowConfig};
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use stretto_oracle::{request_key, Answer, NoulCriteria, Oracle, Question, Request};
use stretto_trace::tau2::GoldAction;
use stretto_trace::{Episode, Event, ToolCall, ToolManifest};

/// Customer messages kept, the latest last.
const MAX_MESSAGES: usize = 12;
/// Characters kept from the start of each customer message.
const MAX_TEXT: usize = 600;
/// Characters of a candidate's description in a choice question.
const MAX_OPTION: usize = 220;
/// Characters of the customer's words shown in an example.
const SHOWN: usize = 160;
/// The single-pick question's id.
const PICK: &str = "pick";

/// What a choice picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    /// The items of an order to return, exchange or modify.
    Items,
    /// The variant an exchanged or modified item becomes.
    Variant,
    /// The payment method.
    Payment,
    /// The reservation to cancel or change.
    Reservation,
}

impl Pick {
    fn name(self) -> &'static str {
        match self {
            Pick::Items => "Items of the order",
            Pick::Variant => "New variant",
            Pick::Payment => "Payment method",
            Pick::Reservation => "Reservation",
        }
    }
}

/// One record choice at one write.
#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    /// The τ²-bench task.
    pub task_id: String,
    /// The episode.
    pub episode: String,
    /// The agent model.
    pub agent_model: String,
    /// The write.
    pub tool: String,
    /// What it picks.
    pub kind: Pick,
    /// The candidates' ids.
    pub candidates: Vec<String>,
    /// The agent's pick (several items, or one of anything else).
    pub agent: BTreeSet<String>,
    /// The expected pick; for a reservation, any one of these.
    pub gold: BTreeSet<String>,
    /// The System-One model's pick, once asked.
    pub model: Option<BTreeSet<String>>,
    /// Its probability for its pick (for items, that of the least sure item).
    pub confidence: Option<f64>,
    /// The customer's last message before the write.
    pub customer: String,
    /// The question's key in the replay cache.
    pub key: String,
    #[serde(skip)]
    request: Request,
}

impl Choice {
    fn right(&self, pick: &BTreeSet<String>) -> bool {
        match self.kind {
            Pick::Reservation => pick.len() == 1 && pick.is_subset(&self.gold),
            _ => *pick == self.gold,
        }
    }

    /// Whether the agent picked what the task expects.
    pub fn agent_right(&self) -> bool {
        self.right(&self.agent)
    }

    /// Whether the model did, once asked.
    pub fn model_right(&self) -> Option<bool> {
        self.model.as_ref().map(|m| self.right(m))
    }
}

/// What the episode has shown so far.
#[derive(Default)]
struct Seen {
    customer: Vec<String>,
    orders: HashMap<String, Value>,
    products: HashMap<String, Value>,
    user: Option<Value>,
    reservations: BTreeMap<String, Value>,
}

/// Every record choice in `ep`, whose task expects `gold`, with the question
/// to ask `model` about it. `manifest` describes the writes.
pub fn choices(
    ep: &Episode,
    gold: &[GoldAction],
    manifest: &ToolManifest,
    model: &str,
) -> Vec<Choice> {
    let mut seen = Seen::default();
    let mut out = Vec::new();
    for e in &ep.events {
        match e {
            Event::User { text } => seen.customer.push(text.clone()),
            Event::Assistant { calls, .. } => {
                for call in calls {
                    at_write(ep, call, gold, manifest, model, &seen, &mut out);
                }
            }
            Event::ToolResult {
                name,
                content,
                error: false,
                ..
            } => {
                let Ok(v) = serde_json::from_str::<Value>(content) else {
                    continue;
                };
                let id = |field: &str| v.get(field).and_then(Value::as_str).map(String::from);
                match name.as_str() {
                    "get_order_details" => {
                        if let Some(id) = id("order_id") {
                            seen.orders.insert(id, v);
                        }
                    }
                    "get_product_details" => {
                        if let Some(id) = id("product_id") {
                            seen.products.insert(id, v);
                        }
                    }
                    "get_user_details" => seen.user = Some(v),
                    "get_reservation_details" => {
                        if let Some(id) = id("reservation_id") {
                            seen.reservations.insert(id, v);
                        }
                    }
                    _ => {}
                }
            }
            Event::ToolResult { .. } => {}
        }
    }
    out
}

/// The choices `call` makes, if it is a write the task expects.
fn at_write(
    ep: &Episode,
    call: &ToolCall,
    gold: &[GoldAction],
    manifest: &ToolManifest,
    model: &str,
    seen: &Seen,
    out: &mut Vec<Choice>,
) {
    let arg = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let list = |v: &Value, k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let does = manifest
        .docs
        .get(&call.name)
        .map(|d| head(&d.summary, 300))
        .unwrap_or_else(|| call.name.clone());
    let base = |kind: Pick, candidates: &[(String, Value)], agent, gold, request: Request| Choice {
        task_id: ep.task_id.clone(),
        episode: ep.id.clone(),
        agent_model: ep.agent_model.clone(),
        tool: call.name.clone(),
        kind,
        candidates: candidates.iter().map(|(id, _)| id.clone()).collect(),
        agent,
        gold,
        model: None,
        confidence: None,
        customer: seen.customer.last().cloned().unwrap_or_default(),
        key: request_key(&request),
        request,
    };
    let payments = || -> Vec<(String, Value)> {
        seen.user
            .as_ref()
            .and_then(|u| u.get("payment_methods"))
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(id, r)| (id.clone(), r.clone())).collect())
            .unwrap_or_default()
    };
    let one = |s: Option<String>| s.into_iter().collect::<BTreeSet<String>>();
    match call.name.as_str() {
        "return_delivered_order_items"
        | "exchange_delivered_order_items"
        | "modify_pending_order_items"
        | "modify_pending_order_payment" => {
            let Some(order_id) = arg(&call.arguments, "order_id") else {
                return;
            };
            let Some(g) = gold.iter().find(|g| {
                g.name == call.name && arg(&g.arguments, "order_id").as_ref() == Some(&order_id)
            }) else {
                return;
            };
            let about = json!({"write": does, "order_id": order_id});
            let order = seen.orders.get(&order_id);
            let items: Vec<&Value> = order
                .and_then(|o| o.get("items"))
                .and_then(Value::as_array)
                .map(|a| a.iter().collect())
                .unwrap_or_default();
            if call.name != "modify_pending_order_payment" {
                // The items of the order.
                let mut candidates: Vec<(String, Value)> = Vec::new();
                for item in &items {
                    if let Some(id) = arg(item, "item_id") {
                        if !candidates.iter().any(|(c, _)| *c == id) {
                            candidates.push((id, strip(item, &["product_id"])));
                        }
                    }
                }
                if candidates.len() >= 2 {
                    let questions = candidates
                        .iter()
                        .map(|(id, r)| (format!("item_{id}"), item_question(id, r)))
                        .collect();
                    let request = request(model, seen, about.clone(), &candidates, None, questions);
                    out.push(base(
                        Pick::Items,
                        &candidates,
                        list(&call.arguments, "item_ids").into_iter().collect(),
                        list(&g.arguments, "item_ids").into_iter().collect(),
                        request,
                    ));
                }
                // What each exchanged or modified item becomes.
                let expected: HashMap<String, String> = list(&g.arguments, "item_ids")
                    .into_iter()
                    .zip(list(&g.arguments, "new_item_ids"))
                    .collect();
                for (old, new) in list(&call.arguments, "item_ids")
                    .into_iter()
                    .zip(list(&call.arguments, "new_item_ids"))
                {
                    let (Some(want), Some(item)) = (
                        expected.get(&old),
                        items
                            .iter()
                            .find(|i| arg(i, "item_id").as_ref() == Some(&old)),
                    ) else {
                        continue;
                    };
                    let Some(product) = arg(item, "product_id").and_then(|p| seen.products.get(&p))
                    else {
                        continue;
                    };
                    let candidates: Vec<(String, Value)> = product
                        .get("variants")
                        .and_then(Value::as_object)
                        .map(|m| m.iter().map(|(id, r)| (id.clone(), r.clone())).collect())
                        .unwrap_or_default();
                    if candidates.len() < 2 {
                        continue;
                    }
                    let question = pick_question(
                        "From what the customer said, which variant in candidates should \
                         item_being_replaced become?",
                        &candidates,
                    );
                    let request = request(
                        model,
                        seen,
                        about.clone(),
                        &candidates,
                        Some(("item_being_replaced", strip(item, &["product_id"]))),
                        BTreeMap::from([(PICK.to_string(), question)]),
                    );
                    out.push(base(
                        Pick::Variant,
                        &candidates,
                        one(Some(new)),
                        one(Some(want.clone())),
                        request,
                    ));
                }
            }
            // The payment method, with the order's payments for "the original
            // one".
            let history = order.and_then(|o| o.get("payment_history")).cloned();
            payment_choice(
                call,
                g,
                "payment_method_id",
                &payments(),
                about,
                history,
                model,
                seen,
                &base,
                out,
            );
        }
        "cancel_reservation"
        | "update_reservation_flights"
        | "update_reservation_baggages"
        | "update_reservation_passengers" => {
            let expected: Vec<&GoldAction> = gold.iter().filter(|g| g.name == call.name).collect();
            let Some(reservation) = arg(&call.arguments, "reservation_id") else {
                return;
            };
            if expected.is_empty() {
                return;
            }
            // The reservation, among those the agent looked up.
            let candidates: Vec<(String, Value)> = seen
                .reservations
                .iter()
                .map(|(id, r)| (id.clone(), strip(r, &["payment_history", "user_id"])))
                .collect();
            if candidates.len() >= 2 {
                let question = pick_question(
                    "From what the customer said, which reservation in candidates do they \
                     want this done to?",
                    &candidates,
                );
                let request = request(
                    model,
                    seen,
                    json!({"write": does}),
                    &candidates,
                    None,
                    BTreeMap::from([(PICK.to_string(), question)]),
                );
                out.push(base(
                    Pick::Reservation,
                    &candidates,
                    one(Some(reservation.clone())),
                    expected
                        .iter()
                        .filter_map(|g| arg(&g.arguments, "reservation_id"))
                        .collect(),
                    request,
                ));
            }
            // The payment method, for the expected write on this reservation.
            if let Some(g) = expected
                .iter()
                .find(|g| arg(&g.arguments, "reservation_id").as_ref() == Some(&reservation))
            {
                let history = seen
                    .reservations
                    .get(&reservation)
                    .and_then(|r| r.get("payment_history"))
                    .cloned();
                payment_choice(
                    call,
                    g,
                    "payment_id",
                    &payments(),
                    json!({"write": does, "reservation_id": reservation}),
                    history,
                    model,
                    seen,
                    &base,
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Builds a [`Choice`] at the current write from what it picks, the
/// candidates, the agent's pick, the expected pick and the question.
type MakeChoice<'a> =
    dyn Fn(Pick, &[(String, Value)], BTreeSet<String>, BTreeSet<String>, Request) -> Choice + 'a;

/// The payment-method choice of `call` (its argument `field`), if the task
/// expects one and the customer has at least two methods.
#[allow(clippy::too_many_arguments)]
fn payment_choice(
    call: &ToolCall,
    g: &GoldAction,
    field: &str,
    candidates: &[(String, Value)],
    about: Value,
    history: Option<Value>,
    model: &str,
    seen: &Seen,
    base: &MakeChoice,
    out: &mut Vec<Choice>,
) {
    let arg = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let (Some(agent), Some(want)) = (arg(&call.arguments, field), arg(&g.arguments, field)) else {
        return;
    };
    if candidates.len() < 2 {
        return;
    }
    let question = pick_question(
        "From what the customer said, which of their payment methods in candidates should \
         the agent use for this change?",
        candidates,
    );
    let request = request(
        model,
        seen,
        about,
        candidates,
        history.map(|h| ("payments_so_far", h)),
        BTreeMap::from([(PICK.to_string(), question)]),
    );
    out.push(base(
        Pick::Payment,
        candidates,
        BTreeSet::from([agent]),
        BTreeSet::from([want]),
        request,
    ));
}

/// The request for one choice: what the customer said, what the agent is
/// about to do, the candidates, and anything else needed to read them.
fn request(
    model: &str,
    seen: &Seen,
    about: Value,
    candidates: &[(String, Value)],
    context: Option<(&str, Value)>,
    questions: BTreeMap<String, Question>,
) -> Request {
    let said: Vec<String> = seen
        .customer
        .iter()
        .skip(seen.customer.len().saturating_sub(MAX_MESSAGES))
        .map(|t| head(t, MAX_TEXT))
        .collect();
    let mut state = json!({
        "customer_said": said,
        "the_agent_is_about_to": about,
        "candidates": candidates.iter().map(|(_, r)| r).collect::<Vec<_>>(),
    });
    if let Some((k, v)) = context {
        state[k] = v;
    }
    Request {
        model: model.to_string(),
        state,
        questions,
    }
}

/// Whether the customer wants the write done to item `id`.
fn item_question(id: &str, record: &Value) -> Question {
    Question::Noul {
        instructions: format!(
            "An agent in a customer-service chat is about to make the change in \
             the_agent_is_about_to. From what the customer said, do they want it done to item \
             {id} in candidates ({})?",
            describe(record)
        ),
        criteria: Some(NoulCriteria {
            yes: "The customer asked for this item: by its name or description, or as part of \
                  a group they named, such as 'everything in the order'."
                .to_string(),
            no: "The customer did not ask for this item, or asked to keep it.".to_string(),
        }),
    }
}

/// One pick among `candidates`, each described by its record.
fn pick_question(instructions: &str, candidates: &[(String, Value)]) -> Question {
    Question::Choice {
        instructions: format!(
            "An agent in a customer-service chat is about to make the change in \
             the_agent_is_about_to. {instructions}"
        ),
        criteria: candidates
            .iter()
            .map(|(id, r)| (id.clone(), head(&describe(r), MAX_OPTION)))
            .collect(),
    }
}

/// A record in one line: its scalar fields, and those one level down.
fn describe(record: &Value) -> String {
    fn scalar(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
    let Value::Object(m) = record else {
        return scalar(record).unwrap_or_default();
    };
    let mut parts = Vec::new();
    for (k, v) in m {
        match v {
            Value::Object(sub) => {
                let inner: Vec<String> = sub
                    .iter()
                    .filter_map(|(k2, v2)| scalar(v2).map(|s| format!("{k2} {s}")))
                    .collect();
                parts.push(format!("{k}: {}", inner.join(", ")));
            }
            Value::Array(items) => {
                let inner: Vec<String> = items
                    .iter()
                    .take(4)
                    .map(|i| match i {
                        Value::Object(o) => {
                            o.values().filter_map(scalar).collect::<Vec<_>>().join(" ")
                        }
                        other => scalar(other).unwrap_or_default(),
                    })
                    .collect();
                parts.push(format!("{k}: {}", inner.join("; ")));
            }
            other => {
                if let Some(s) = scalar(other) {
                    parts.push(format!("{k}: {s}"));
                }
            }
        }
    }
    parts.join("; ")
}

/// `record` without `fields`.
fn strip(record: &Value, fields: &[&str]) -> Value {
    let mut r = record.clone();
    if let Value::Object(m) = &mut r {
        for f in fields {
            m.remove(*f);
        }
    }
    r
}

fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Ask `oracle` about every choice in `items` (each distinct question once)
/// and fill in its picks.
pub fn judge(
    oracle: &(dyn Oracle + Sync),
    items: &mut [Choice],
    config: &ShadowConfig,
    domain: &str,
) -> Result<()> {
    let decisions: Vec<Decision> = items
        .iter()
        .enumerate()
        .map(|(i, c)| Decision {
            episode: i,
            step: 0,
            kind: Kind::Next,
            tool: None,
            actual: String::new(),
            agent: String::new(),
            request: Some(c.request.clone()),
            fixed: None,
        })
        .collect();
    let asked = shadow::ask(oracle, &decisions, config, domain)?;
    for c in items.iter_mut() {
        let Some(response) = asked.responses.get(&c.key) else {
            continue;
        };
        match c.kind {
            Pick::Items => {
                let answers: Option<Vec<(String, f64)>> = c
                    .candidates
                    .iter()
                    .map(|id| match response.answers.get(&format!("item_{id}")) {
                        Some(Answer::Noul { noul }) => Some((id.clone(), *noul)),
                        _ => None,
                    })
                    .collect();
                if let Some(answers) = answers {
                    c.model = Some(
                        answers
                            .iter()
                            .filter(|(_, p)| *p >= 0.5)
                            .map(|(id, _)| id.clone())
                            .collect(),
                    );
                    c.confidence = answers
                        .iter()
                        .map(|(_, p)| p.max(1.0 - p))
                        .min_by(f64::total_cmp);
                }
            }
            _ => {
                if let Some(Answer::Choice {
                    choice,
                    probabilities,
                    ..
                }) = response.answers.get(PICK)
                {
                    c.model = Some(BTreeSet::from([choice.clone()]));
                    c.confidence = probabilities.get(choice).copied();
                }
            }
        }
    }
    Ok(())
}

/// Counts for one kind of choice.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Row {
    /// What is picked.
    pub kind: Option<Pick>,
    /// Choices.
    pub choices: usize,
    /// The agent picked what the task expects.
    pub agent_right: usize,
    /// The model did.
    pub model_right: usize,
    /// The model's pick differs from the agent's.
    pub differs: usize,
    /// ... and the agent's pick was wrong.
    pub differs_agent_wrong: usize,
    /// The agent was wrong and the model right.
    pub catches: usize,
    /// The model did not answer.
    pub unanswered: usize,
}

/// A choice where the model and the agent differ.
#[derive(Clone, Debug, Serialize)]
pub struct Example {
    /// The τ²-bench task.
    pub task_id: String,
    /// What is picked.
    pub kind: Pick,
    /// The agent's pick.
    pub agent: Vec<String>,
    /// The model's pick.
    pub model: Vec<String>,
    /// The expected pick.
    pub gold: Vec<String>,
    /// The start of the customer's last message.
    pub customer: String,
}

/// The matching audit of one domain.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MatchAudit {
    /// The domain.
    pub domain: String,
    /// One row per kind of choice, then all of them.
    pub rows: Vec<Row>,
    /// Choices the model got right and the agent wrong.
    pub catches: Vec<Example>,
    /// Choices the agent got right and the model wrong.
    pub misses: Vec<Example>,
}

/// Count `items`, all of `domain`, by kind; show up to `examples` of each
/// disagreement.
pub fn audit(domain: &str, items: &[Choice], examples: usize) -> MatchAudit {
    let mut by: BTreeMap<Pick, Row> = BTreeMap::new();
    let mut all = Row::default();
    let mut a = MatchAudit {
        domain: domain.to_string(),
        ..Default::default()
    };
    for c in items {
        let row = by.entry(c.kind).or_insert_with(|| Row {
            kind: Some(c.kind),
            ..Default::default()
        });
        for r in [row, &mut all] {
            r.choices += 1;
            r.agent_right += usize::from(c.agent_right());
            match (&c.model, c.model_right()) {
                (Some(m), Some(right)) => {
                    r.model_right += usize::from(right);
                    let differs = *m != c.agent;
                    r.differs += usize::from(differs);
                    r.differs_agent_wrong += usize::from(differs && !c.agent_right());
                    r.catches += usize::from(right && !c.agent_right());
                }
                _ => r.unanswered += 1,
            }
        }
        let example = |m: &BTreeSet<String>| Example {
            task_id: c.task_id.clone(),
            kind: c.kind,
            agent: c.agent.iter().cloned().collect(),
            model: m.iter().cloned().collect(),
            gold: c.gold.iter().cloned().collect(),
            customer: head(&c.customer.replace('\n', " "), SHOWN),
        };
        if let (Some(m), Some(right)) = (&c.model, c.model_right()) {
            if right && !c.agent_right() && a.catches.len() < examples {
                a.catches.push(example(m));
            }
            if !right && c.agent_right() && a.misses.len() < examples {
                a.misses.push(example(m));
            }
        }
    }
    a.rows = by.into_values().chain([all]).collect();
    a
}

/// The audits as a report.
pub fn markdown(audits: &[MatchAudit]) -> String {
    let mut s = String::from("# Matching descriptions to records\n\n");
    let _ = writeln!(
        s,
        "At each write that picks records out of earlier tool results (the items of an order, \
         the variant an item becomes, a payment method, a reservation) and has at least two \
         candidates, a System-One model is shown what the customer said and the candidates, \
         but not the agent's pick, and asked which one the customer means. Both picks are \
         scored against the task's expected actions. Used as a check, the model flags a write \
         where its pick differs from the agent's.\n"
    );
    let pct = |n: usize, d: usize| {
        if d == 0 {
            "–".to_string()
        } else {
            format!("{n} ({:.1}%)", 100.0 * n as f64 / d as f64)
        }
    };
    for a in audits {
        let _ = writeln!(s, "## {}\n", a.domain);
        let _ = writeln!(
            s,
            "| Choice | Choices | Agent right | Model right | Model differs | ... agent wrong there | \
             Agent wrong, model right | Unanswered |"
        );
        let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
        for r in &a.rows {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                r.kind.map_or("All", Pick::name),
                r.choices,
                pct(r.agent_right, r.choices),
                pct(r.model_right, r.choices),
                pct(r.differs, r.choices),
                pct(r.differs_agent_wrong, r.differs),
                pct(r.catches, r.choices - r.agent_right),
                r.unanswered
            );
        }
        s.push('\n');
        for (title, list) in [
            ("The model right, the agent wrong", &a.catches),
            ("The agent right, the model wrong", &a.misses),
        ] {
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(s, "{title} (up to {}):\n", list.len());
            for e in list {
                let _ = writeln!(
                    s,
                    "- task {}, {}: agent {:?}, model {:?}, expected {:?}; the customer last said \"{}\"",
                    e.task_id,
                    e.kind.name().to_lowercase(),
                    e.agent,
                    e.model,
                    e.gold,
                    e.customer.replace('"', "'")
                );
            }
            s.push('\n');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use stretto_trace::ToolCall;

    fn call(id: &str, name: &str, args: Value) -> Event {
        Event::Assistant {
            text: None,
            calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: args,
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

    /// The customer returns the lamp of a two-item order to their gift card;
    /// the agent returns the chair to their credit card.
    fn episode() -> Episode {
        Episode {
            id: "e1".to_string(),
            task_id: "7".to_string(),
            trial: 0,
            domain: "retail".to_string(),
            agent_model: "m".to_string(),
            reward: 0.0,
            events: vec![
                Event::User {
                    text: "Return the lamp from order #W1 to my gift card, please.".to_string(),
                },
                call("a", "get_user_details", json!({"user_id": "u"})),
                result(
                    "a",
                    "get_user_details",
                    json!({"payment_methods": {
                        "gift_card_1": {"source": "gift_card", "id": "gift_card_1", "balance": 10},
                        "credit_card_2": {"source": "credit_card", "id": "credit_card_2"},
                    }}),
                ),
                call("b", "get_order_details", json!({"order_id": "#W1"})),
                result(
                    "b",
                    "get_order_details",
                    json!({"order_id": "#W1", "items": [
                        {"item_id": "1", "name": "Chair", "product_id": "p1"},
                        {"item_id": "2", "name": "Lamp", "product_id": "p2"},
                    ]}),
                ),
                call(
                    "c",
                    "return_delivered_order_items",
                    json!({"order_id": "#W1", "item_ids": ["1"], "payment_method_id": "credit_card_2"}),
                ),
            ],
        }
    }

    #[test]
    fn choices_are_scored_against_the_expected_write() {
        let gold = vec![GoldAction {
            name: "return_delivered_order_items".to_string(),
            arguments: json!({"order_id": "#W1", "item_ids": ["2"], "payment_method_id": "gift_card_1"}),
        }];
        let manifest = ToolManifest {
            domain: "retail".to_string(),
            tools: BTreeMap::new(),
            docs: BTreeMap::new(),
        };
        let mut items = choices(&episode(), &gold, &manifest, "jev-test");
        assert_eq!(
            items.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [Pick::Items, Pick::Payment]
        );
        // The model is not shown the agent's pick.
        let state = items[0].request.state.to_string();
        assert!(!state.contains("credit_card_2\"]") && state.contains("Lamp"));
        assert_eq!(items[0].request.questions.len(), 2);
        assert!(!items[0].agent_right() && !items[1].agent_right());

        // The model picks the lamp, and the credit card.
        items[0].model = Some(BTreeSet::from(["2".to_string()]));
        items[1].model = Some(BTreeSet::from(["credit_card_2".to_string()]));
        let a = audit("retail", &items, 3);
        let all = a.rows.last().unwrap();
        assert_eq!((all.choices, all.agent_right, all.model_right), (2, 0, 1));
        assert_eq!(
            (all.differs, all.differs_agent_wrong, all.catches),
            (1, 1, 1)
        );
        assert_eq!(a.catches.len(), 1);
        assert!(markdown(&[a]).contains("| Items of the order | 1 | 0 (0.0%) | 1 (100.0%) |"));
    }

    #[test]
    fn a_reservation_is_right_if_the_task_expects_the_write_on_it() {
        let c = Choice {
            task_id: "1".to_string(),
            episode: "e".to_string(),
            agent_model: "m".to_string(),
            tool: "cancel_reservation".to_string(),
            kind: Pick::Reservation,
            candidates: vec!["A".into(), "B".into(), "C".into()],
            agent: BTreeSet::from(["B".to_string()]),
            gold: BTreeSet::from(["A".to_string(), "B".to_string()]),
            model: Some(BTreeSet::from(["C".to_string()])),
            confidence: Some(0.6),
            customer: String::new(),
            key: String::new(),
            request: Request {
                model: String::new(),
                state: Value::Null,
                questions: BTreeMap::new(),
            },
        };
        assert!(c.agent_right());
        assert_eq!(c.model_right(), Some(false));
    }
}
