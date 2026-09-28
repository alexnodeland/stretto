//! Policy guards on writes (RFC-001 §3.5; arms B and E).
//!
//! Each guard is one rule of a domain's written policy, compiled into a
//! check a proxy can run before a write reaches the server. A check reads
//! only what the episode has already shown: the user the agent
//! authenticated, the records it looked up, and what the customer said. So
//! a guard can pass, fail, or not know (a record it needs was never looked
//! up); only a known failure refuses the write.
//!
//! The rules come from τ²-bench's retail and airline policies. An LLM
//! (Claude, in drafting this module) compiled them from the policy text, and
//! [`audit`] tests each against recorded trajectories: a rule that fails
//! writes the tool accepted in successful episodes is wrong or too strict
//! (or the episode broke the policy and passed anyway), and a person should
//! review it before it is enforced. The audit of τ²-bench's published
//! baselines (`stretto guards`, docs/results/guards-2026-09-24.md) found the
//! confirmation rules no better than chance, so they are only logged; every
//! remaining failure it found in a successful episode breaks the written
//! policy. Rules that need judgment the tools cannot supply (whether a
//! reason for cancelling is covered by insurance, whether the user asked
//! for compensation) are left to the LLM.

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use stretto_trace::{Episode, Event, ToolCall};

/// The time τ²-bench's airline policy says it is.
const AIRLINE_NOW: &str = "2024-05-15T15:00:00";

/// What a guard says about a write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", content = "reason", rename_all = "snake_case")]
pub enum Verdict {
    /// The rule holds.
    Pass,
    /// The rule is broken, and why.
    Fail(String),
    /// The episode has not shown what the rule needs.
    Unknown(String),
}

/// One rule.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Short id, e.g. `retail.cancel.pending`.
    pub id: &'static str,
    /// The policy text it enforces, in brief.
    pub policy: &'static str,
    /// The writes it applies to.
    pub tools: &'static [&'static str],
    /// Whether a failure refuses the write; a rule the audit finds too
    /// noisy to enforce is only logged.
    pub enforce: bool,
    check: fn(&Facts, &ToolCall) -> Verdict,
}

/// The guards of one domain.
#[derive(Clone, Debug)]
pub struct Guards {
    domain: String,
    rules: Vec<Rule>,
}

impl Guards {
    /// The guards for `domain` (`retail` or `airline`), if there are any.
    pub fn for_domain(domain: &str) -> Option<Self> {
        let rules = match domain {
            "retail" => retail(),
            "airline" => airline(),
            _ => return None,
        };
        Some(Self {
            domain: domain.to_string(),
            rules,
        })
    }

    /// The domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// The rules.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Every rule's verdict on `call`, made after the last event of
    /// `episode` (which must not include `call` itself).
    pub fn check(&self, episode: &Episode, call: &ToolCall) -> Vec<(&'static str, Verdict)> {
        let facts = Facts::of(&episode.events);
        self.rules
            .iter()
            .filter(|r| r.tools.contains(&call.name.as_str()))
            .map(|r| (r.id, (r.check)(&facts, call)))
            .collect()
    }

    /// Why `call` should be refused, if an enforced rule knows it breaks
    /// the policy.
    pub fn refusal(&self, episode: &Episode, call: &ToolCall) -> Option<String> {
        let enforced = |id: &str| self.rules.iter().any(|r| r.id == id && r.enforce);
        let failed: Vec<String> = self
            .check(episode, call)
            .into_iter()
            .filter_map(|(id, v)| match v {
                Verdict::Fail(reason) if enforced(id) => {
                    Some(format!("{reason} (policy check {id})"))
                }
                _ => None,
            })
            .collect();
        (!failed.is_empty()).then(|| failed.join("; "))
    }
}

/// What an episode has shown so far.
#[derive(Debug, Default)]
struct Facts {
    /// Users found by email or by name and zip code (retail).
    authenticated: Vec<String>,
    /// Records by id, as last returned.
    users: HashMap<String, Value>,
    orders: HashMap<String, Value>,
    products: HashMap<String, Value>,
    reservations: HashMap<String, Value>,
    /// Flight statuses by (flight number, date).
    flights: HashMap<(String, String), String>,
    /// Successful writes so far, by tool, with their arguments.
    writes: Vec<(String, Value)>,
    /// What the customer said, in order.
    said: Vec<String>,
}

impl Facts {
    fn of(events: &[Event]) -> Self {
        let mut f = Facts::default();
        let mut args: HashMap<&str, (&str, &Value)> = HashMap::new();
        for e in events {
            match e {
                Event::User { text } => f.said.push(text.to_lowercase()),
                Event::Assistant { calls, .. } => {
                    for c in calls {
                        args.insert(c.id.as_str(), (c.name.as_str(), &c.arguments));
                    }
                }
                Event::ToolResult {
                    call_id,
                    name,
                    error: false,
                    content,
                } => {
                    let parsed: Value = serde_json::from_str(content)
                        .unwrap_or_else(|_| Value::String(content.trim().to_string()));
                    let text =
                        |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
                    match name.as_str() {
                        "find_user_id_by_email" | "find_user_id_by_name_zip" => {
                            if let Value::String(id) = &parsed {
                                f.authenticated.push(id.trim_matches('"').to_string());
                            }
                        }
                        "get_user_details" => {
                            if let Some(id) = text(&parsed, "user_id") {
                                f.users.insert(id, parsed.clone());
                            }
                        }
                        "get_product_details" => {
                            if let Some(id) = text(&parsed, "product_id") {
                                f.products.insert(id, parsed.clone());
                            }
                        }
                        "get_flight_status" => {
                            if let Some((_, a)) = args.get(call_id.as_str()) {
                                if let (Some(n), Some(d), Value::String(s)) =
                                    (text(a, "flight_number"), text(a, "date"), &parsed)
                                {
                                    f.flights.insert((n, d), s.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                    // Orders and reservations come back from lookups and from
                    // the writes that change them.
                    if let Some(id) = text(&parsed, "order_id") {
                        f.orders.insert(id, parsed.clone());
                    }
                    if let Some(id) = text(&parsed, "reservation_id") {
                        f.reservations.insert(id, parsed.clone());
                    }
                    if let Some((tool, a)) = args.get(call_id.as_str()) {
                        if !tool.starts_with("get_")
                            && !tool.starts_with("find_")
                            && !tool.starts_with("list_")
                            && !tool.starts_with("search_")
                            && *tool != "calculate"
                        {
                            f.writes.push((tool.to_string(), (*a).clone()));
                        }
                    }
                }
                Event::ToolResult { .. } => {}
            }
        }
        f
    }

    /// The last thing the customer said, if anything.
    fn last_said(&self) -> Option<&str> {
        self.said.last().map(String::as_str)
    }
}

fn arg<'a>(call: &'a ToolCall, key: &str) -> Option<&'a str> {
    call.arguments.get(key).and_then(Value::as_str)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Words that confirm an action.
const CONFIRMATIONS: &[&str] = &[
    "yes",
    "yeah",
    "yep",
    "yup",
    "confirm",
    "go ahead",
    "proceed",
    "please do",
    "do it",
    "sure",
    "okay",
    "ok",
    "correct",
    "that's right",
    "sounds good",
    "absolutely",
    "please cancel",
    "please process",
    "please go",
    "let's do",
    "i agree",
    "fine",
];

fn confirmed(f: &Facts) -> Verdict {
    match f.last_said() {
        None => Verdict::Fail("the customer has not confirmed anything yet".to_string()),
        Some(said) => {
            let words: Vec<&str> = said
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|w| !w.is_empty())
                .collect();
            let joined = words.join(" ");
            let yes = CONFIRMATIONS.iter().any(|c| {
                if c.contains(' ') {
                    joined.contains(c)
                } else {
                    words.contains(c)
                }
            });
            if yes {
                Verdict::Pass
            } else {
                Verdict::Fail(
                    "the customer's last message is not an explicit confirmation".to_string(),
                )
            }
        }
    }
}

// ---- retail -------------------------------------------------------------------------------

const RETAIL_WRITES: &[&str] = &[
    "cancel_pending_order",
    "modify_pending_order_address",
    "modify_pending_order_items",
    "modify_pending_order_payment",
    "modify_user_address",
    "return_delivered_order_items",
    "exchange_delivered_order_items",
];
const ORDER_WRITES: &[&str] = &[
    "cancel_pending_order",
    "modify_pending_order_address",
    "modify_pending_order_items",
    "modify_pending_order_payment",
    "return_delivered_order_items",
    "exchange_delivered_order_items",
];

fn retail() -> Vec<Rule> {
    vec![
        Rule {
            id: "retail.authenticated",
            policy: "Authenticate the user (by email, or name and zip code) before any change.",
            tools: RETAIL_WRITES,
            enforce: true,
            check: |f, _| {
                if f.authenticated.is_empty() {
                    Verdict::Fail("the user has not been authenticated by email or by name and zip code".to_string())
                } else {
                    Verdict::Pass
                }
            },
        },
        Rule {
            id: "retail.own_order",
            policy: "Help only the authenticated user, with their own orders.",
            tools: ORDER_WRITES,
            enforce: true,
            check: |f, c| {
                let Some(order) = arg(c, "order_id").and_then(|id| f.orders.get(id)) else {
                    return Verdict::Unknown("the order was not looked up".to_string());
                };
                match order.get("user_id").and_then(Value::as_str) {
                    Some(owner) if f.authenticated.iter().any(|u| u == owner) => Verdict::Pass,
                    Some(owner) if f.authenticated.is_empty() => {
                        Verdict::Unknown(format!("order of {owner}; nobody authenticated"))
                    }
                    Some(owner) => Verdict::Fail(format!("the order belongs to {owner}, not the authenticated user")),
                    None => Verdict::Unknown("the order has no user".to_string()),
                }
            },
        },
        Rule {
            id: "retail.status_checked",
            policy: "Check the order's status before cancelling, modifying, returning or exchanging.",
            tools: ORDER_WRITES,
            enforce: true,
            check: |f, c| match arg(c, "order_id") {
                Some(id) if f.orders.contains_key(id) => Verdict::Pass,
                Some(_) => Verdict::Fail("the order's status was not checked first".to_string()),
                None => Verdict::Unknown("no order id".to_string()),
            },
        },
        Rule {
            id: "retail.pending",
            policy: "Only pending orders can be cancelled or modified; after an item change, only the address and payment.",
            tools: &[
                "cancel_pending_order",
                "modify_pending_order_address",
                "modify_pending_order_items",
                "modify_pending_order_payment",
            ],
            enforce: true,
            // The tools accept address and payment changes on a "pending
            // (item modified)" order, and τ²-bench's tasks expect them.
            check: |f, c| match c.name.as_str() {
                "modify_pending_order_address" | "modify_pending_order_payment" => {
                    let Some(order) = arg(c, "order_id").and_then(|id| f.orders.get(id)) else {
                        return Verdict::Unknown("the order was not looked up".to_string());
                    };
                    match order.get("status").and_then(Value::as_str) {
                        Some(s) if s.contains("pending") => Verdict::Pass,
                        Some(s) => Verdict::Fail(format!("the order is {s}, not pending")),
                        None => Verdict::Unknown("the order has no status".to_string()),
                    }
                }
                _ => status_is(f, c, "pending"),
            },
        },
        Rule {
            id: "retail.delivered",
            policy: "Only delivered orders can be returned or exchanged.",
            tools: &["return_delivered_order_items", "exchange_delivered_order_items"],
            enforce: true,
            check: |f, c| status_is(f, c, "delivered"),
        },
        Rule {
            id: "retail.cancel_reason",
            policy: "The reason for cancelling is 'no longer needed' or 'ordered by mistake'.",
            tools: &["cancel_pending_order"],
            enforce: true,
            check: |_, c| match arg(c, "reason") {
                Some("no longer needed" | "ordered by mistake") => Verdict::Pass,
                Some(other) => Verdict::Fail(format!("'{other}' is not an accepted reason")),
                None => Verdict::Fail("no reason given".to_string()),
            },
        },
        Rule {
            id: "retail.items_in_order",
            policy: "Only items of the order can be changed or returned.",
            tools: &[
                "modify_pending_order_items",
                "return_delivered_order_items",
                "exchange_delivered_order_items",
            ],
            enforce: true,
            check: |f, c| {
                let Some(order) = arg(c, "order_id").and_then(|id| f.orders.get(id)) else {
                    return Verdict::Unknown("the order was not looked up".to_string());
                };
                let held: Vec<String> = order
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|i| i.get("item_id").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut left = held.clone();
                for id in strings(c.arguments.get("item_ids")) {
                    match left.iter().position(|h| *h == id) {
                        Some(k) => {
                            left.remove(k);
                        }
                        None => return Verdict::Fail(format!("item {id} is not in the order")),
                    }
                }
                Verdict::Pass
            },
        },
        Rule {
            id: "retail.same_product",
            policy: "Each item can only change to an available variant of the same product.",
            tools: &["modify_pending_order_items", "exchange_delivered_order_items"],
            enforce: true,
            check: |f, c| {
                let Some(order) = arg(c, "order_id").and_then(|id| f.orders.get(id)) else {
                    return Verdict::Unknown("the order was not looked up".to_string());
                };
                let old = strings(c.arguments.get("item_ids"));
                let new = strings(c.arguments.get("new_item_ids"));
                if old.len() != new.len() {
                    return Verdict::Fail("item_ids and new_item_ids differ in length".to_string());
                }
                for (o, n) in old.iter().zip(&new) {
                    let product = order
                        .get("items")
                        .and_then(Value::as_array)
                        .and_then(|items| {
                            items.iter().find(|i| i.get("item_id").and_then(Value::as_str) == Some(o))
                        })
                        .and_then(|i| i.get("product_id").and_then(Value::as_str));
                    let Some(product) = product else {
                        return Verdict::Unknown(format!("item {o} is not in the order"));
                    };
                    let Some(details) = f.products.get(product) else {
                        return Verdict::Unknown(format!("product {product} was not looked up"));
                    };
                    match details.pointer(&format!("/variants/{n}")) {
                        None => {
                            return Verdict::Fail(format!(
                                "item {n} is not a variant of product {product}"
                            ))
                        }
                        Some(v) if v.get("available").and_then(Value::as_bool) == Some(false) => {
                            return Verdict::Fail(format!("item {n} is not available"))
                        }
                        Some(_) if o == n => {
                            return Verdict::Fail(format!("item {n} is the item it replaces"))
                        }
                        Some(_) => {}
                    }
                }
                Verdict::Pass
            },
        },
        Rule {
            id: "retail.once",
            policy: "Items of an order can be modified or exchanged only once.",
            tools: &["modify_pending_order_items", "exchange_delivered_order_items"],
            enforce: true,
            check: |f, c| {
                let id = arg(c, "order_id");
                let before = f.writes.iter().any(|(tool, a)| {
                    (tool == "modify_pending_order_items" || tool == "exchange_delivered_order_items")
                        && a.get("order_id").and_then(Value::as_str) == id
                });
                if before {
                    Verdict::Fail("the order's items were already changed once".to_string())
                } else {
                    Verdict::Pass
                }
            },
        },
        Rule {
            id: "retail.own_payment",
            policy: "Payment methods must be the user's own.",
            tools: &[
                "modify_pending_order_items",
                "modify_pending_order_payment",
                "return_delivered_order_items",
                "exchange_delivered_order_items",
            ],
            enforce: true,
            check: |f, c| {
                let Some(method) = arg(c, "payment_method_id") else {
                    return Verdict::Unknown("no payment method".to_string());
                };
                let owners: Vec<&Value> = f
                    .authenticated
                    .iter()
                    .filter_map(|u| f.users.get(u))
                    .collect();
                if owners.is_empty() {
                    return Verdict::Unknown("the user's details were not looked up".to_string());
                }
                if owners
                    .iter()
                    .any(|u| u.pointer(&format!("/payment_methods/{method}")).is_some())
                {
                    Verdict::Pass
                } else {
                    Verdict::Fail(format!("{method} is not one of the user's payment methods"))
                }
            },
        },
        Rule {
            id: "retail.refund_method",
            policy: "A return is refunded to the original payment method or an existing gift card.",
            tools: &["return_delivered_order_items"],
            enforce: true,
            check: |f, c| {
                let (Some(order), Some(method)) = (
                    arg(c, "order_id").and_then(|id| f.orders.get(id)),
                    arg(c, "payment_method_id"),
                ) else {
                    return Verdict::Unknown("the order or the method is unknown".to_string());
                };
                let original = order
                    .pointer("/payment_history/0/payment_method_id")
                    .and_then(Value::as_str);
                if original == Some(method) || method.starts_with("gift_card") {
                    Verdict::Pass
                } else {
                    Verdict::Fail(format!(
                        "{method} is neither the original payment method nor a gift card"
                    ))
                }
            },
        },
        Rule {
            id: "retail.confirmed",
            policy: "Obtain explicit confirmation (yes) before any change.",
            tools: RETAIL_WRITES,
            // A word list cannot tell a confirmation from a request: the
            // audit finds it misses as often in successful episodes as in
            // failed ones, so it is only logged.
            enforce: false,
            check: |f, _| confirmed(f),
        },
    ]
}

fn status_is(f: &Facts, c: &ToolCall, wanted: &str) -> Verdict {
    let Some(order) = arg(c, "order_id").and_then(|id| f.orders.get(id)) else {
        return Verdict::Unknown("the order was not looked up".to_string());
    };
    match order.get("status").and_then(Value::as_str) {
        Some(s) if s == wanted => Verdict::Pass,
        Some(s) => Verdict::Fail(format!("the order is {s}, not {wanted}")),
        None => Verdict::Unknown("the order has no status".to_string()),
    }
}

// ---- airline ------------------------------------------------------------------------------

const AIRLINE_WRITES: &[&str] = &[
    "book_reservation",
    "cancel_reservation",
    "update_reservation_baggages",
    "update_reservation_flights",
    "update_reservation_passengers",
];
const RESERVATION_WRITES: &[&str] = &[
    "cancel_reservation",
    "update_reservation_baggages",
    "update_reservation_flights",
    "update_reservation_passengers",
];

fn airline() -> Vec<Rule> {
    vec![
        Rule {
            id: "airline.own_reservation",
            policy: "Only the user's own reservations can be changed.",
            tools: RESERVATION_WRITES,
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                match r.get("user_id").and_then(Value::as_str) {
                    Some(owner) if f.users.contains_key(owner) => Verdict::Pass,
                    Some(owner) if f.users.is_empty() => {
                        Verdict::Unknown(format!("reservation of {owner}; no user looked up"))
                    }
                    Some(owner) => Verdict::Fail(format!("the reservation belongs to {owner}, not the user")),
                    None => Verdict::Unknown("the reservation has no user".to_string()),
                }
            },
        },
        Rule {
            id: "airline.cancel_allowed",
            policy: "Cancel only if booked within 24 hours, cancelled by the airline, business class, or insured; never after a flight was flown.",
            tools: &["cancel_reservation"],
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                if flown(r) {
                    return Verdict::Fail("a flight of the reservation was already flown".to_string());
                }
                let recent = r
                    .get("created_at")
                    .and_then(Value::as_str)
                    .is_some_and(|t| within_a_day(t, AIRLINE_NOW));
                let business = r.get("cabin").and_then(Value::as_str) == Some("business");
                let insured = r.get("insurance").and_then(Value::as_str) == Some("yes");
                let cancelled_by_airline = r
                    .get("flights")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|fl| {
                        let key = (
                            fl.get("flight_number").and_then(Value::as_str).unwrap_or_default().to_string(),
                            fl.get("date").and_then(Value::as_str).unwrap_or_default().to_string(),
                        );
                        f.flights.get(&key).is_some_and(|s| s == "cancelled")
                    });
                if recent || business || insured || cancelled_by_airline {
                    Verdict::Pass
                } else {
                    Verdict::Fail(
                        "not booked within 24 hours, not business, not insured, and no flight known to be \
                         cancelled by the airline (check get_flight_status if one was)"
                            .to_string(),
                    )
                }
            },
        },
        Rule {
            id: "airline.basic_economy_flights",
            policy: "Basic economy flights cannot be changed; the cabin can, and an upgraded reservation's flights then can too.",
            tools: &["update_reservation_flights"],
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                // The reservation as it stands: τ²-bench's own task 32
                // upgrades a basic-economy reservation, then changes its
                // flights.
                if r.get("cabin").and_then(Value::as_str) != Some("basic_economy") {
                    return Verdict::Pass;
                }
                let key = |fl: &Value| {
                    (
                        fl.get("flight_number").and_then(Value::as_str).map(str::to_string),
                        fl.get("date").and_then(Value::as_str).map(str::to_string),
                    )
                };
                let mut old: Vec<_> = r.get("flights").and_then(Value::as_array).into_iter().flatten().map(key).collect();
                let mut new: Vec<_> = c.arguments.get("flights").and_then(Value::as_array).into_iter().flatten().map(key).collect();
                old.sort();
                new.sort();
                if old == new {
                    Verdict::Pass
                } else {
                    Verdict::Fail(
                        "the reservation is basic economy, whose flights cannot change (its cabin can)"
                            .to_string(),
                    )
                }
            },
        },
        Rule {
            id: "airline.not_flown",
            policy: "The cabin cannot change once a flight was flown.",
            tools: &["update_reservation_flights"],
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                let changes_cabin = arg(c, "cabin").is_some_and(|cabin| {
                    r.get("cabin").and_then(Value::as_str) != Some(cabin)
                });
                if changes_cabin && flown(r) {
                    Verdict::Fail("a flight was already flown, so the cabin cannot change".to_string())
                } else {
                    Verdict::Pass
                }
            },
        },
        Rule {
            id: "airline.keep_bags",
            policy: "Checked bags can be added, not removed.",
            tools: &["update_reservation_baggages"],
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                let before = r.get("total_baggages").and_then(Value::as_i64);
                let after = c.arguments.get("total_baggages").and_then(Value::as_i64);
                match (before, after) {
                    (Some(b), Some(a)) if a < b => {
                        Verdict::Fail(format!("that removes bags ({b} to {a})"))
                    }
                    (Some(_), Some(_)) => Verdict::Pass,
                    _ => Verdict::Unknown("bag counts unknown".to_string()),
                }
            },
        },
        Rule {
            id: "airline.same_passenger_count",
            policy: "The number of passengers cannot change.",
            tools: &["update_reservation_passengers"],
            enforce: true,
            check: |f, c| {
                let Some(r) = arg(c, "reservation_id").and_then(|id| f.reservations.get(id)) else {
                    return Verdict::Unknown("the reservation was not looked up".to_string());
                };
                let before = r.get("passengers").and_then(Value::as_array).map(Vec::len);
                let after = c.arguments.get("passengers").and_then(Value::as_array).map(Vec::len);
                match (before, after) {
                    (Some(b), Some(a)) if a != b => {
                        Verdict::Fail(format!("that changes the number of passengers ({b} to {a})"))
                    }
                    (Some(_), Some(_)) => Verdict::Pass,
                    _ => Verdict::Unknown("passenger counts unknown".to_string()),
                }
            },
        },
        Rule {
            id: "airline.booking_limits",
            policy: "At most five passengers; at most one certificate, one credit card and three gift cards.",
            tools: &["book_reservation"],
            enforce: true,
            check: |_, c| {
                let passengers = c.arguments.get("passengers").and_then(Value::as_array).map_or(0, Vec::len);
                if passengers > 5 {
                    return Verdict::Fail(format!("{passengers} passengers is more than five"));
                }
                let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
                for p in c.arguments.get("payment_methods").and_then(Value::as_array).into_iter().flatten() {
                    let id = p.get("payment_id").and_then(Value::as_str).unwrap_or_default();
                    let kind = if id.starts_with("certificate") {
                        "certificate"
                    } else if id.starts_with("credit_card") {
                        "credit card"
                    } else if id.starts_with("gift_card") {
                        "gift card"
                    } else {
                        "other"
                    };
                    *kinds.entry(kind).or_default() += 1;
                }
                let over = [("certificate", 1), ("credit card", 1), ("gift card", 3)]
                    .into_iter()
                    .find(|(k, max)| kinds.get(k).copied().unwrap_or(0) > *max);
                match over {
                    Some((k, max)) => Verdict::Fail(format!("more than {max} {k} payment(s)")),
                    None => Verdict::Pass,
                }
            },
        },
        Rule {
            id: "airline.own_payment",
            policy: "Payment methods must already be in the user's profile.",
            tools: &["book_reservation", "update_reservation_baggages", "update_reservation_flights"],
            enforce: true,
            check: |f, c| {
                let mut ids: Vec<String> = c
                    .arguments
                    .get("payment_methods")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.get("payment_id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect();
                ids.extend(arg(c, "payment_id").map(str::to_string));
                if ids.is_empty() {
                    return Verdict::Unknown("no payment method".to_string());
                }
                if f.users.is_empty() {
                    return Verdict::Unknown("the user's details were not looked up".to_string());
                }
                for id in &ids {
                    let known = f
                        .users
                        .values()
                        .any(|u| u.pointer(&format!("/payment_methods/{id}")).is_some());
                    if !known {
                        return Verdict::Fail(format!("{id} is not in the user's profile"));
                    }
                }
                Verdict::Pass
            },
        },
        Rule {
            id: "airline.compensation",
            policy: "Compensate only silver or gold members, insured travelers, or business travelers.",
            tools: &["send_certificate"],
            enforce: true,
            check: |f, c| {
                let Some(user) = arg(c, "user_id").and_then(|id| f.users.get(id)) else {
                    return Verdict::Unknown("the user was not looked up".to_string());
                };
                let member = user.get("membership").and_then(Value::as_str);
                if matches!(member, Some("silver" | "gold")) {
                    return Verdict::Pass;
                }
                let eligible = f.reservations.values().any(|r| {
                    r.get("user_id").and_then(Value::as_str) == arg(c, "user_id")
                        && (r.get("insurance").and_then(Value::as_str) == Some("yes")
                            || r.get("cabin").and_then(Value::as_str) == Some("business"))
                });
                if eligible {
                    Verdict::Pass
                } else if f.reservations.is_empty() {
                    Verdict::Unknown("no reservation looked up".to_string())
                } else {
                    Verdict::Fail("a regular member without insurance or business class is not compensated".to_string())
                }
            },
        },
        Rule {
            id: "airline.confirmed",
            policy: "Obtain explicit confirmation (yes) before any change to a booking.",
            tools: AIRLINE_WRITES,
            // Logged only, as `retail.confirmed`.
            enforce: false,
            check: |f, _| confirmed(f),
        },
    ]
}

/// Whether a flight of reservation `r` departed before the policy's now.
fn flown(r: &Value) -> bool {
    let today = &AIRLINE_NOW[..10];
    r.get("flights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|fl| fl.get("date").and_then(Value::as_str))
        .any(|d| d < today)
}

/// Whether `then` (ISO date-time) is within 24 hours before `now`.
fn within_a_day(then: &str, now: &str) -> bool {
    let secs = |t: &str| -> Option<i64> {
        let (date, time) = t.split_once('T')?;
        let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
        let (y, m, day) = (d.next()??, d.next()??, d.next()??);
        let mut hms = time.split(':').map(|x| x.parse::<f64>().ok());
        let (h, mi, s) = (
            hms.next()??,
            hms.next()??,
            hms.next().flatten().unwrap_or(0.0),
        );
        // Days from civil (Howard Hinnant), for the difference only.
        let y2 = if m <= 2 { y - 1 } else { y };
        let era = y2.div_euclid(400);
        let yoe = y2 - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Some((era * 146_097 + doe) * 86_400 + (h * 3600.0 + mi * 60.0 + s) as i64)
    };
    match (secs(then), secs(now)) {
        (Some(a), Some(b)) => b - a <= 86_400 && b >= a,
        _ => false,
    }
}

/// How one rule fared on recorded episodes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RuleAudit {
    /// The rule.
    pub rule: String,
    /// Its policy text.
    pub policy: String,
    /// Whether a failure refuses the write (else it is only logged).
    pub enforced: bool,
    /// Verdicts on the writes the tool accepted in successful episodes:
    /// pass, fail, unknown.
    pub successful: [usize; 3],
    /// Verdicts on the writes the tool accepted in failed episodes.
    pub failed: [usize; 3],
    /// Verdicts on the writes the tool itself refused (a failure there
    /// agrees with the tool, and harms nothing).
    pub refused_by_tool: [usize; 3],
    /// Episodes in which it fails a write the tool accepted: successful,
    /// failed.
    pub episodes_failed: [usize; 2],
    /// A few of its failures on accepted writes of successful episodes
    /// (episode id, task id, reason): false alarms, or policy the episode
    /// broke and passed anyway.
    pub false_alarms: Vec<(String, String, String)>,
}

/// How a domain's guards fared on recorded episodes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Audit {
    /// The domain.
    pub domain: String,
    /// Episodes read: successful, failed.
    pub episodes: [usize; 2],
    /// Writes checked: accepted by the tool, refused by it.
    pub writes: [usize; 2],
    /// Episodes in which an enforced rule fails a write the tool accepted:
    /// successful (false alarms, or policy broken and passed), failed.
    pub refused: [usize; 2],
    /// Every rule.
    pub rules: Vec<RuleAudit>,
    /// By task: episodes (successful, failed), and those in which an
    /// enforced rule would refuse an accepted write (successful, failed).
    pub tasks: BTreeMap<String, [usize; 4]>,
}

/// Test every guard against the writes of `episodes`: each write is checked
/// against what came before it, as a proxy would check it, and sorted by
/// whether the tool accepted it and whether the episode succeeded.
pub fn audit(guards: &Guards, episodes: &[&Episode]) -> Audit {
    let mut rules: BTreeMap<&str, RuleAudit> = guards
        .rules
        .iter()
        .map(|r| {
            (
                r.id,
                RuleAudit {
                    rule: r.id.to_string(),
                    policy: r.policy.to_string(),
                    enforced: r.enforce,
                    ..Default::default()
                },
            )
        })
        .collect();
    let mut out = Audit {
        domain: guards.domain.clone(),
        ..Default::default()
    };
    for ep in episodes {
        let good = ep.succeeded();
        let side = usize::from(!good);
        out.episodes[side] += 1;
        let tool_error: HashMap<&str, bool> = ep
            .events
            .iter()
            .filter_map(|e| match e {
                Event::ToolResult { call_id, error, .. } => Some((call_id.as_str(), *error)),
                _ => None,
            })
            .collect();
        let mut failing: HashSet<&str> = HashSet::new();
        for (i, e) in ep.events.iter().enumerate() {
            let Event::Assistant { calls, .. } = e else {
                continue;
            };
            let before = Episode {
                events: ep.events[..i].to_vec(),
                ..(*ep).clone()
            };
            for call in calls {
                let verdicts = guards.check(&before, call);
                if verdicts.is_empty() {
                    continue;
                }
                let refused = tool_error.get(call.id.as_str()).copied().unwrap_or(false);
                out.writes[usize::from(refused)] += 1;
                for (id, verdict) in verdicts {
                    let a = rules.get_mut(id).expect("every rule has an entry");
                    let k = match &verdict {
                        Verdict::Pass => 0,
                        Verdict::Fail(_) => 1,
                        Verdict::Unknown(_) => 2,
                    };
                    let counts = if refused {
                        &mut a.refused_by_tool
                    } else if good {
                        &mut a.successful
                    } else {
                        &mut a.failed
                    };
                    counts[k] += 1;
                    if let (Verdict::Fail(reason), false) = (verdict, refused) {
                        failing.insert(id);
                        if good && a.false_alarms.len() < 5 {
                            a.false_alarms
                                .push((ep.id.clone(), ep.task_id.clone(), reason));
                        }
                    }
                }
            }
        }
        for id in &failing {
            rules
                .get_mut(id)
                .expect("every rule has an entry")
                .episodes_failed[side] += 1;
        }
        let task = out.tasks.entry(ep.task_id.clone()).or_default();
        task[side] += 1;
        if failing.iter().any(|id| rules[id].enforced) {
            out.refused[side] += 1;
            task[2 + side] += 1;
        }
    }
    out.rules = rules.into_values().collect();
    out
}

/// The audits as a Markdown report.
pub fn markdown(audits: &[Audit]) -> String {
    let mut md = String::from("# Policy guards against recorded trajectories\n\n");
    md.push_str(
        "Every write in every episode is checked against what came before it, as \
         `stretto-proxy --guards` checks it, and sorted by whether the tool accepted it. A \
         failure on a write the tool refused agrees with the tool. A failure on an accepted \
         write of a successful episode is a false alarm, or a policy the episode broke and \
         passed anyway (τ²-bench grades the database, not every rule). Only enforced rules \
         refuse a write; the others are logged.\n",
    );
    for a in audits {
        let [good, bad] = a.episodes;
        md.push_str(&format!(
            "\n## {}\n\n{} episodes ({good} successful, {bad} failed); {} writes the tool \
             accepted and {} it refused. The enforced rules would refuse an accepted write in \
             **{} of {good} successful** episodes and **{} of {bad} failed** ones.\n\n",
            a.domain,
            good + bad,
            a.writes[0],
            a.writes[1],
            a.refused[0],
            a.refused[1],
        ));
        md.push_str(
            "| Rule | Enforced | Accepted, successful: pass / fail / unknown \
             | Accepted, failed: pass / fail / unknown | Refused by the tool: pass / fail / unknown \
             | Episodes failed: successful / failed |\n|---|---|---|---|---|---|\n",
        );
        let cell = |[p, f, u]: [usize; 3]| format!("{p} / {f} / {u}");
        for r in &a.rules {
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} / {} |\n",
                r.rule,
                if r.enforced { "yes" } else { "logged" },
                cell(r.successful),
                cell(r.failed),
                cell(r.refused_by_tool),
                r.episodes_failed[0],
                r.episodes_failed[1],
            ));
        }
        let alarms: Vec<&RuleAudit> = a
            .rules
            .iter()
            .filter(|r| !r.false_alarms.is_empty())
            .collect();
        if !alarms.is_empty() {
            md.push_str(
                "\nFailures on accepted writes of successful episodes (up to five per rule):\n\n",
            );
            for r in alarms {
                for (episode, task, reason) in &r.false_alarms {
                    md.push_str(&format!(
                        "- `{}`, task {task} ({episode}): {reason}\n",
                        r.rule
                    ));
                }
            }
        }
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            content: match content {
                Value::String(s) => s,
                other => other.to_string(),
            },
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

    fn order(status: &str) -> Value {
        json!({"order_id": "#W1", "user_id": "ann_1", "status": status,
               "items": [{"item_id": "i1", "product_id": "p1"}],
               "payment_history": [{"payment_method_id": "credit_card_1"}]})
    }

    fn cancel(reason: &str) -> ToolCall {
        ToolCall {
            id: "x".to_string(),
            name: "cancel_pending_order".to_string(),
            arguments: json!({"order_id": "#W1", "reason": reason}),
        }
    }

    #[test]
    fn a_proper_cancellation_passes_and_bad_ones_are_refused() {
        let g = Guards::for_domain("retail").unwrap();
        let good = episode(vec![
            Event::User {
                text: "Cancel #W1, I'm ann@x.com".to_string(),
            },
            call("1", "find_user_id_by_email", json!({"email": "ann@x.com"})),
            result("1", "find_user_id_by_email", json!("ann_1")),
            call("2", "get_order_details", json!({"order_id": "#W1"})),
            result("2", "get_order_details", order("pending")),
            Event::User {
                text: "Yes, please cancel it.".to_string(),
            },
        ]);
        assert_eq!(g.refusal(&good, &cancel("no longer needed")), None);
        // An unaccepted reason.
        let why = g.refusal(&good, &cancel("found it cheaper")).unwrap();
        assert!(why.contains("retail.cancel_reason"), "{why}");
        // A delivered order cannot be cancelled.
        let mut delivered = good.clone();
        delivered.events[4] = result("2", "get_order_details", order("delivered"));
        let why = g.refusal(&delivered, &cancel("no longer needed")).unwrap();
        assert!(why.contains("the order is delivered, not pending"), "{why}");
        // Nobody authenticated, the status unchecked, no confirmation: the
        // confirmation rule is only logged, so it is checked but not cited.
        let bare = episode(vec![Event::User {
            text: "cancel #W1".to_string(),
        }]);
        let why = g.refusal(&bare, &cancel("no longer needed")).unwrap();
        for rule in ["retail.authenticated", "retail.status_checked"] {
            assert!(why.contains(rule), "{why}");
        }
        assert!(!why.contains("retail.confirmed"), "{why}");
        assert!(g.check(&bare, &cancel("no longer needed")).contains(&(
            "retail.confirmed",
            Verdict::Fail(
                "the customer's last message is not an explicit confirmation".to_string()
            )
        )));
    }

    #[test]
    fn address_changes_follow_the_tools_after_an_item_change() {
        let g = Guards::for_domain("retail").unwrap();
        let mut modified = order("pending (item modified)");
        modified["order_id"] = json!("#W1");
        let ep = episode(vec![
            call("1", "find_user_id_by_email", json!({"email": "a"})),
            result("1", "find_user_id_by_email", json!("ann_1")),
            call("2", "get_order_details", json!({"order_id": "#W1"})),
            result("2", "get_order_details", modified),
            Event::User {
                text: "yes".to_string(),
            },
        ]);
        let address = ToolCall {
            id: "3".to_string(),
            name: "modify_pending_order_address".to_string(),
            arguments: json!({"order_id": "#W1", "address1": "1 Main St"}),
        };
        assert_eq!(g.refusal(&ep, &address), None);
        let why = g.refusal(&ep, &cancel("no longer needed")).unwrap();
        assert!(
            why.contains("pending (item modified), not pending"),
            "{why}"
        );
    }

    #[test]
    fn exchanges_stay_within_the_product_and_happen_once() {
        let g = Guards::for_domain("retail").unwrap();
        let mut events = vec![
            call("1", "find_user_id_by_email", json!({"email": "a"})),
            result("1", "find_user_id_by_email", json!("ann_1")),
            call("2", "get_order_details", json!({"order_id": "#W1"})),
            result("2", "get_order_details", order("delivered")),
            call("3", "get_product_details", json!({"product_id": "p1"})),
            result(
                "3",
                "get_product_details",
                json!({"product_id": "p1", "variants": {
                "i1": {"available": true}, "i2": {"available": true}, "i3": {"available": false}}}),
            ),
            call("4", "get_user_details", json!({"user_id": "ann_1"})),
            result(
                "4",
                "get_user_details",
                json!({"user_id": "ann_1", "payment_methods": {"credit_card_1": {}}}),
            ),
            Event::User {
                text: "yes".to_string(),
            },
        ];
        let exchange = |new: &str| ToolCall {
            id: "5".to_string(),
            name: "exchange_delivered_order_items".to_string(),
            arguments: json!({"order_id": "#W1", "item_ids": ["i1"], "new_item_ids": [new],
                              "payment_method_id": "credit_card_1"}),
        };
        assert_eq!(g.refusal(&episode(events.clone()), &exchange("i2")), None);
        assert!(g
            .refusal(&episode(events.clone()), &exchange("i3"))
            .unwrap()
            .contains("not available"));
        assert!(g
            .refusal(&episode(events.clone()), &exchange("i9"))
            .unwrap()
            .contains("not a variant"));
        // Once done, not again.
        events.push(call(
            "5",
            "exchange_delivered_order_items",
            exchange("i2").arguments,
        ));
        events.push(result("5", "exchange_delivered_order_items", json!("ok")));
        events.push(Event::User {
            text: "yes".to_string(),
        });
        assert!(g
            .refusal(&episode(events), &exchange("i2"))
            .unwrap()
            .contains("retail.once"));
    }

    #[test]
    fn airline_cancellations_follow_the_eligibility_rules() {
        let g = Guards::for_domain("airline").unwrap();
        let reservation = |created: &str, cabin: &str, insurance: &str| {
            json!({"reservation_id": "R1", "user_id": "u1", "cabin": cabin, "insurance": insurance,
                   "created_at": created, "flights": [{"flight_number": "HAT1", "date": "2024-05-20"}]})
        };
        let with = |r: Value| {
            episode(vec![
                call("1", "get_user_details", json!({"user_id": "u1"})),
                result(
                    "1",
                    "get_user_details",
                    json!({"user_id": "u1", "membership": "regular"}),
                ),
                call(
                    "2",
                    "get_reservation_details",
                    json!({"reservation_id": "R1"}),
                ),
                result("2", "get_reservation_details", r),
                Event::User {
                    text: "Yes, cancel it".to_string(),
                },
            ])
        };
        let cancel = ToolCall {
            id: "3".to_string(),
            name: "cancel_reservation".to_string(),
            arguments: json!({"reservation_id": "R1"}),
        };
        assert_eq!(
            g.refusal(
                &with(reservation("2024-05-15T09:00:00", "economy", "no")),
                &cancel
            ),
            None
        );
        assert_eq!(
            g.refusal(
                &with(reservation("2024-05-01T09:00:00", "business", "no")),
                &cancel
            ),
            None
        );
        assert_eq!(
            g.refusal(
                &with(reservation("2024-05-01T09:00:00", "economy", "yes")),
                &cancel
            ),
            None
        );
        let why = g
            .refusal(
                &with(reservation("2024-05-01T09:00:00", "economy", "no")),
                &cancel,
            )
            .unwrap();
        assert!(why.contains("airline.cancel_allowed"), "{why}");
    }

    #[test]
    fn upgrading_a_basic_economy_cabin_frees_its_flights() {
        let g = Guards::for_domain("airline").unwrap();
        let reservation = |cabin: &str| {
            json!({"reservation_id": "R1", "user_id": "u1", "cabin": cabin,
                   "flights": [{"flight_number": "HAT1", "date": "2024-05-17"}]})
        };
        let update = |id: &str, cabin: &str, flight: &str| ToolCall {
            id: id.to_string(),
            name: "update_reservation_flights".to_string(),
            arguments: json!({"reservation_id": "R1", "cabin": cabin, "payment_id": "credit_card_1",
                              "flights": [{"flight_number": flight, "date": "2024-05-17"}]}),
        };
        let mut events = vec![
            call("1", "get_user_details", json!({"user_id": "u1"})),
            result(
                "1",
                "get_user_details",
                json!({"user_id": "u1", "payment_methods": {"credit_card_1": {}}}),
            ),
            call(
                "2",
                "get_reservation_details",
                json!({"reservation_id": "R1"}),
            ),
            result("2", "get_reservation_details", reservation("basic_economy")),
            Event::User {
                text: "Yes, go ahead.".to_string(),
            },
        ];
        // Changing the cabin alone is allowed.
        let upgrade = update("3", "economy", "HAT1");
        assert_eq!(g.refusal(&episode(events.clone()), &upgrade), None);
        events.push(call(
            "3",
            "update_reservation_flights",
            upgrade.arguments.clone(),
        ));
        events.push(result(
            "3",
            "update_reservation_flights",
            reservation("economy"),
        ));
        events.push(Event::User {
            text: "Yes, now change the flight.".to_string(),
        });
        // Now economy, its flights can change, as τ²-bench's task 32
        // expects of this very path.
        assert_eq!(
            g.refusal(&episode(events.clone()), &update("4", "economy", "HAT2")),
            None
        );
        // Without the upgrade they cannot.
        events.truncate(5);
        let why = g
            .refusal(&episode(events), &update("4", "basic_economy", "HAT2"))
            .unwrap();
        assert!(why.contains("airline.basic_economy_flights"), "{why}");
    }

    #[test]
    fn day_arithmetic_crosses_months() {
        assert!(within_a_day("2024-05-14T16:00:00", AIRLINE_NOW));
        assert!(!within_a_day("2024-05-14T14:00:00", AIRLINE_NOW));
        assert!(within_a_day("2024-04-30T23:00:00", "2024-05-01T10:00:00"));
    }

    /// Rule `rule`'s verdict on the write `name(arguments)` after `events`.
    fn verdict(
        domain: &str,
        events: Vec<Event>,
        name: &str,
        arguments: Value,
        rule: &str,
    ) -> Verdict {
        let call = ToolCall {
            id: "w".to_string(),
            name: name.to_string(),
            arguments,
        };
        let verdict = Guards::for_domain(domain)
            .unwrap()
            .check(&episode(events), &call)
            .into_iter()
            .find(|(id, _)| *id == rule);
        assert!(verdict.is_some(), "{rule} does not check {name}");
        verdict.unwrap().1
    }

    fn fails(v: &Verdict, why: &str) -> bool {
        matches!(v, Verdict::Fail(r) if r.contains(why))
    }

    fn unknown(v: &Verdict, why: &str) -> bool {
        matches!(v, Verdict::Unknown(r) if r.contains(why))
    }

    /// A lookup of `name` that returned `content`.
    fn looked_up(id: &str, name: &str, content: Value) -> Vec<Event> {
        vec![call(id, name, json!({})), result(id, name, content)]
    }

    #[test]
    fn only_retail_and_airline_have_guards() {
        let g = Guards::for_domain("airline").unwrap();
        assert_eq!(g.domain(), "airline");
        assert!(g.rules().iter().any(|r| r.id == "airline.keep_bags"));
        assert!(Guards::for_domain("telecom").is_none());
    }

    #[test]
    fn retail_rules_say_what_they_cannot_tell_and_what_fails() {
        let ann = || looked_up("a", "find_user_id_by_email", json!("ann_1"));
        let order_of = |owner: Option<&str>, status: Option<&str>| {
            let mut o = json!({"order_id": "#W1", "items": [{"item_id": "i1", "product_id": "p1"}],
                               "payment_history": [{"payment_method_id": "credit_card_1"}]});
            if let Some(owner) = owner {
                o["user_id"] = json!(owner);
            }
            if let Some(status) = status {
                o["status"] = json!(status);
            }
            looked_up("o", "get_order_details", o)
        };
        let with = |parts: Vec<Vec<Event>>| parts.concat();
        let cancel = json!({"order_id": "#W1", "reason": "no longer needed"});
        // Whose order it is.
        let v = verdict(
            "retail",
            order_of(Some("bob_2"), Some("pending")),
            "cancel_pending_order",
            cancel.clone(),
            "retail.own_order",
        );
        assert!(unknown(&v, "order of bob_2; nobody authenticated"), "{v:?}");
        let v = verdict(
            "retail",
            with(vec![ann(), order_of(Some("bob_2"), Some("pending"))]),
            "cancel_pending_order",
            cancel.clone(),
            "retail.own_order",
        );
        assert!(fails(&v, "belongs to bob_2"), "{v:?}");
        let v = verdict(
            "retail",
            with(vec![ann(), order_of(None, Some("pending"))]),
            "cancel_pending_order",
            cancel.clone(),
            "retail.own_order",
        );
        assert!(unknown(&v, "the order has no user"), "{v:?}");
        // Its status.
        let v = verdict(
            "retail",
            ann(),
            "cancel_pending_order",
            json!({"reason": "no longer needed"}),
            "retail.status_checked",
        );
        assert!(unknown(&v, "no order id"), "{v:?}");
        let v = verdict(
            "retail",
            with(vec![ann(), order_of(Some("ann_1"), None)]),
            "cancel_pending_order",
            cancel.clone(),
            "retail.pending",
        );
        assert!(unknown(&v, "the order has no status"), "{v:?}");
        let address = json!({"order_id": "#W1", "address1": "1 Main St"});
        let v = verdict(
            "retail",
            ann(),
            "modify_pending_order_address",
            address.clone(),
            "retail.pending",
        );
        assert!(unknown(&v, "the order was not looked up"), "{v:?}");
        let v = verdict(
            "retail",
            with(vec![ann(), order_of(Some("ann_1"), Some("delivered"))]),
            "modify_pending_order_address",
            address.clone(),
            "retail.pending",
        );
        assert!(fails(&v, "the order is delivered, not pending"), "{v:?}");
        let v = verdict(
            "retail",
            with(vec![ann(), order_of(Some("ann_1"), None)]),
            "modify_pending_order_address",
            address,
            "retail.pending",
        );
        assert!(unknown(&v, "the order has no status"), "{v:?}");
        // The reason.
        let v = verdict(
            "retail",
            ann(),
            "cancel_pending_order",
            json!({"order_id": "#W1"}),
            "retail.cancel_reason",
        );
        assert!(fails(&v, "no reason given"), "{v:?}");
        // The items.
        let exchange = |old: &[&str], new: &[&str]| json!({"order_id": "#W1", "item_ids": old, "new_item_ids": new, "payment_method_id": "credit_card_1"});
        let v = verdict(
            "retail",
            ann(),
            "exchange_delivered_order_items",
            exchange(&["i1"], &["i2"]),
            "retail.items_in_order",
        );
        assert!(unknown(&v, "the order was not looked up"), "{v:?}");
        let delivered = || with(vec![ann(), order_of(Some("ann_1"), Some("delivered"))]);
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            exchange(&["i9"], &["i2"]),
            "retail.items_in_order",
        );
        assert!(fails(&v, "item i9 is not in the order"), "{v:?}");
        let v = verdict(
            "retail",
            ann(),
            "exchange_delivered_order_items",
            exchange(&["i1"], &["i2"]),
            "retail.same_product",
        );
        assert!(unknown(&v, "the order was not looked up"), "{v:?}");
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            exchange(&["i1"], &[]),
            "retail.same_product",
        );
        assert!(fails(&v, "differ in length"), "{v:?}");
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            exchange(&["i9"], &["i2"]),
            "retail.same_product",
        );
        assert!(unknown(&v, "item i9 is not in the order"), "{v:?}");
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            exchange(&["i1"], &["i2"]),
            "retail.same_product",
        );
        assert!(unknown(&v, "product p1 was not looked up"), "{v:?}");
        let product = looked_up(
            "p",
            "get_product_details",
            json!({"product_id": "p1",
            "variants": {"i1": {"available": true}, "i2": {"available": true}}}),
        );
        let v = verdict(
            "retail",
            with(vec![delivered(), product]),
            "exchange_delivered_order_items",
            exchange(&["i1"], &["i1"]),
            "retail.same_product",
        );
        assert!(fails(&v, "item i1 is the item it replaces"), "{v:?}");
        // The payment method.
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            json!({"order_id": "#W1"}),
            "retail.own_payment",
        );
        assert!(unknown(&v, "no payment method"), "{v:?}");
        let v = verdict(
            "retail",
            delivered(),
            "exchange_delivered_order_items",
            exchange(&["i1"], &["i2"]),
            "retail.own_payment",
        );
        assert!(
            unknown(&v, "the user's details were not looked up"),
            "{v:?}"
        );
        let details = looked_up(
            "u",
            "get_user_details",
            json!({"user_id": "ann_1",
            "payment_methods": {"credit_card_1": {}}}),
        );
        let v = verdict(
            "retail",
            with(vec![delivered(), details]),
            "exchange_delivered_order_items",
            json!({"order_id": "#W1", "payment_method_id": "paypal_9"}),
            "retail.own_payment",
        );
        assert!(
            fails(&v, "paypal_9 is not one of the user's payment methods"),
            "{v:?}"
        );
        // Where a refund goes.
        let refund = |method: &str| json!({"order_id": "#W1", "item_ids": ["i1"], "payment_method_id": method});
        let v = verdict(
            "retail",
            ann(),
            "return_delivered_order_items",
            refund("credit_card_1"),
            "retail.refund_method",
        );
        assert!(unknown(&v, "the order or the method is unknown"), "{v:?}");
        for method in ["credit_card_1", "gift_card_7"] {
            let v = verdict(
                "retail",
                delivered(),
                "return_delivered_order_items",
                refund(method),
                "retail.refund_method",
            );
            assert_eq!(v, Verdict::Pass, "{method}");
        }
        let v = verdict(
            "retail",
            delivered(),
            "return_delivered_order_items",
            refund("paypal_9"),
            "retail.refund_method",
        );
        assert!(
            fails(&v, "neither the original payment method nor a gift card"),
            "{v:?}"
        );
        // Nothing said yet: nothing confirmed.
        let v = verdict(
            "retail",
            ann(),
            "cancel_pending_order",
            cancel,
            "retail.confirmed",
        );
        assert!(fails(&v, "has not confirmed anything yet"), "{v:?}");
    }

    #[test]
    fn airline_rules_say_what_they_cannot_tell_and_what_fails() {
        let reservation = |r: Value| looked_up("r", "get_reservation_details", r);
        let user = |u: Value| looked_up("u", "get_user_details", u);
        let r1 = |extra: Value| {
            let mut r = json!({"reservation_id": "R1", "user_id": "u1", "cabin": "economy",
                               "flights": [{"flight_number": "HAT1", "date": "2024-05-20"}],
                               "passengers": [{"first_name": "Ann"}], "total_baggages": 2});
            for (k, v) in extra.as_object().unwrap() {
                r[k] = v.clone();
            }
            r
        };
        let flown = || json!({"flights": [{"flight_number": "HAT1", "date": "2024-05-01"}]});
        let with = |parts: Vec<Vec<Event>>| parts.concat();
        let id = json!({"reservation_id": "R1"});
        // Whose reservation it is.
        for (rule, tool) in [
            ("airline.own_reservation", "cancel_reservation"),
            ("airline.cancel_allowed", "cancel_reservation"),
            (
                "airline.basic_economy_flights",
                "update_reservation_flights",
            ),
            ("airline.not_flown", "update_reservation_flights"),
            ("airline.keep_bags", "update_reservation_baggages"),
            (
                "airline.same_passenger_count",
                "update_reservation_passengers",
            ),
        ] {
            let v = verdict("airline", Vec::new(), tool, id.clone(), rule);
            assert!(
                unknown(&v, "the reservation was not looked up"),
                "{rule}: {v:?}"
            );
        }
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "cancel_reservation",
            id.clone(),
            "airline.own_reservation",
        );
        assert!(unknown(&v, "reservation of u1; no user looked up"), "{v:?}");
        let v = verdict(
            "airline",
            with(vec![
                user(json!({"user_id": "u2"})),
                reservation(r1(json!({}))),
            ]),
            "cancel_reservation",
            id.clone(),
            "airline.own_reservation",
        );
        assert!(fails(&v, "belongs to u1"), "{v:?}");
        let v = verdict(
            "airline",
            with(vec![
                user(json!({"user_id": "u2"})),
                reservation(json!({"reservation_id": "R1"})),
            ]),
            "cancel_reservation",
            id.clone(),
            "airline.own_reservation",
        );
        assert!(unknown(&v, "the reservation has no user"), "{v:?}");
        // Flown flights.
        let v = verdict(
            "airline",
            reservation(r1(flown())),
            "cancel_reservation",
            id.clone(),
            "airline.cancel_allowed",
        );
        assert!(fails(&v, "already flown"), "{v:?}");
        let v = verdict(
            "airline",
            reservation(r1(flown())),
            "update_reservation_flights",
            json!({"reservation_id": "R1", "cabin": "business"}),
            "airline.not_flown",
        );
        assert!(fails(&v, "the cabin cannot change"), "{v:?}");
        // A flight the airline cancelled lets the reservation go.
        let status = vec![
            call(
                "s",
                "get_flight_status",
                json!({"flight_number": "HAT1", "date": "2024-05-20"}),
            ),
            result("s", "get_flight_status", json!("cancelled")),
        ];
        let old = r1(json!({"created_at": "2024-05-01T09:00:00"}));
        let v = verdict(
            "airline",
            with(vec![reservation(old), status]),
            "cancel_reservation",
            id.clone(),
            "airline.cancel_allowed",
        );
        assert_eq!(v, Verdict::Pass);
        // Bags and passengers.
        let bags = |n: i64| json!({"reservation_id": "R1", "total_baggages": n});
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_baggages",
            bags(1),
            "airline.keep_bags",
        );
        assert!(fails(&v, "that removes bags (2 to 1)"), "{v:?}");
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_baggages",
            bags(3),
            "airline.keep_bags",
        );
        assert_eq!(v, Verdict::Pass);
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_baggages",
            id.clone(),
            "airline.keep_bags",
        );
        assert!(unknown(&v, "bag counts unknown"), "{v:?}");
        let people = |n: usize| json!({"reservation_id": "R1", "passengers": vec![json!({"first_name": "A"}); n]});
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_passengers",
            people(2),
            "airline.same_passenger_count",
        );
        assert!(fails(&v, "(1 to 2)"), "{v:?}");
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_passengers",
            people(1),
            "airline.same_passenger_count",
        );
        assert_eq!(v, Verdict::Pass);
        let v = verdict(
            "airline",
            reservation(r1(json!({}))),
            "update_reservation_passengers",
            id.clone(),
            "airline.same_passenger_count",
        );
        assert!(unknown(&v, "passenger counts unknown"), "{v:?}");
        // What one booking may hold.
        let booking = |passengers: usize, payments: &[&str]| {
            json!({"passengers": vec![json!({"first_name": "A"}); passengers],
                   "payment_methods": payments.iter().map(|p| json!({"payment_id": p})).collect::<Vec<_>>()})
        };
        let v = verdict(
            "airline",
            Vec::new(),
            "book_reservation",
            booking(6, &[]),
            "airline.booking_limits",
        );
        assert!(fails(&v, "6 passengers is more than five"), "{v:?}");
        let v = verdict(
            "airline",
            Vec::new(),
            "book_reservation",
            booking(1, &["credit_card_1", "credit_card_2"]),
            "airline.booking_limits",
        );
        assert!(fails(&v, "more than 1 credit card payment(s)"), "{v:?}");
        let v = verdict(
            "airline",
            Vec::new(),
            "book_reservation",
            booking(
                1,
                &["certificate_1", "gift_card_1", "gift_card_2", "paypal_1"],
            ),
            "airline.booking_limits",
        );
        assert_eq!(v, Verdict::Pass);
        // Paying with the user's own methods.
        let v = verdict(
            "airline",
            Vec::new(),
            "book_reservation",
            booking(1, &[]),
            "airline.own_payment",
        );
        assert!(unknown(&v, "no payment method"), "{v:?}");
        let v = verdict(
            "airline",
            Vec::new(),
            "book_reservation",
            booking(1, &["credit_card_1"]),
            "airline.own_payment",
        );
        assert!(
            unknown(&v, "the user's details were not looked up"),
            "{v:?}"
        );
        let v = verdict(
            "airline",
            user(json!({"user_id": "u1", "payment_methods": {"credit_card_1": {}}})),
            "update_reservation_baggages",
            json!({"reservation_id": "R1", "payment_id": "gift_card_9"}),
            "airline.own_payment",
        );
        assert!(
            fails(&v, "gift_card_9 is not in the user's profile"),
            "{v:?}"
        );
        // Compensation.
        let send = json!({"user_id": "u1", "amount": 100});
        let v = verdict(
            "airline",
            Vec::new(),
            "send_certificate",
            send.clone(),
            "airline.compensation",
        );
        assert!(unknown(&v, "the user was not looked up"), "{v:?}");
        let regular = || user(json!({"user_id": "u1", "membership": "regular"}));
        let v = verdict(
            "airline",
            user(json!({"user_id": "u1", "membership": "gold"})),
            "send_certificate",
            send.clone(),
            "airline.compensation",
        );
        assert_eq!(v, Verdict::Pass);
        let v = verdict(
            "airline",
            regular(),
            "send_certificate",
            send.clone(),
            "airline.compensation",
        );
        assert!(unknown(&v, "no reservation looked up"), "{v:?}");
        let v = verdict(
            "airline",
            with(vec![
                regular(),
                reservation(r1(json!({"insurance": "yes"}))),
            ]),
            "send_certificate",
            send.clone(),
            "airline.compensation",
        );
        assert_eq!(v, Verdict::Pass);
        let v = verdict(
            "airline",
            with(vec![regular(), reservation(r1(json!({"insurance": "no"})))]),
            "send_certificate",
            send,
            "airline.compensation",
        );
        assert!(fails(&v, "not compensated"), "{v:?}");
        // A time that is not one is not within a day.
        assert!(!within_a_day("soon", AIRLINE_NOW));
    }

    #[test]
    fn a_result_of_no_known_call_changes_nothing() {
        let f = Facts::of(&[
            result("9", "cancel_pending_order", json!({"order_id": "#W1"})),
            result("8", "get_flight_status", json!("available")),
            // A lookup that failed shows nothing.
            Event::ToolResult {
                call_id: "7".to_string(),
                name: "get_order_details".to_string(),
                error: true,
                content: json!({"order_id": "#W2"}).to_string(),
            },
        ]);
        assert!(f.writes.is_empty() && f.flights.is_empty());
        assert!(f.orders.contains_key("#W1") && !f.orders.contains_key("#W2"));
    }

    /// What a rule cannot tell is counted apart, and an audit without a
    /// failure lists none.
    #[test]
    fn the_audit_counts_what_the_rules_cannot_tell() {
        let g = Guards::for_domain("retail").unwrap();
        let blind = episode(vec![
            call(
                "1",
                "cancel_pending_order",
                json!({"order_id": "#W1", "reason": "no longer needed"}),
            ),
            result("1", "cancel_pending_order", json!({})),
        ]);
        let a = audit(&g, &[&blind]);
        let pending = a.rules.iter().find(|r| r.rule == "retail.pending").unwrap();
        assert_eq!(pending.successful, [0, 0, 1]);
        let md = markdown(&[audit(&g, &[])]);
        assert!(!md.contains("Failures on accepted writes"), "{md}");
    }

    #[test]
    fn the_audit_sorts_each_write_by_the_tool_and_by_the_episode() {
        let g = Guards::for_domain("retail").unwrap();
        let with_cancel = |id: &str, reason: &str, refused: bool, reward: f64| Episode {
            id: id.to_string(),
            reward,
            ..episode(vec![
                Event::User {
                    text: "Cancel #W1, I'm ann@x.com".to_string(),
                },
                call("1", "find_user_id_by_email", json!({"email": "ann@x.com"})),
                result("1", "find_user_id_by_email", json!("ann_1")),
                call("2", "get_order_details", json!({"order_id": "#W1"})),
                result("2", "get_order_details", order("pending")),
                Event::User {
                    text: "Yes, please cancel it.".to_string(),
                },
                call(
                    "3",
                    "cancel_pending_order",
                    json!({"order_id": "#W1", "reason": reason}),
                ),
                Event::ToolResult {
                    call_id: "3".to_string(),
                    name: "cancel_pending_order".to_string(),
                    error: refused,
                    content: "{}".to_string(),
                },
            ])
        };
        let passed = with_cancel("a", "no longer needed", false, 1.0);
        // A reason the policy refuses, in an episode that passed anyway.
        let alarm = with_cancel("b", "found it cheaper", false, 1.0);
        let failed = with_cancel("c", "found it cheaper", false, 0.0);
        // The tool refused it too.
        let refused = with_cancel("d", "found it cheaper", true, 0.0);
        let a = audit(&g, &[&passed, &alarm, &failed, &refused]);
        assert_eq!(a.domain, "retail");
        assert_eq!((a.episodes, a.writes, a.refused), ([2, 2], [3, 1], [1, 1]));
        let reason = a
            .rules
            .iter()
            .find(|r| r.rule == "retail.cancel_reason")
            .unwrap();
        assert!(reason.enforced);
        assert_eq!(reason.successful, [1, 1, 0]);
        assert_eq!(reason.failed, [0, 1, 0]);
        assert_eq!(reason.refused_by_tool, [0, 1, 0]);
        assert_eq!(reason.episodes_failed, [1, 1]);
        assert_eq!(reason.false_alarms.len(), 1);
        assert_eq!(a.tasks["1"], [2, 2, 1, 1]);
        let md = markdown(&[a]);
        for line in [
            "## retail",
            "4 episodes (2 successful, 2 failed); 3 writes the tool accepted and 1 it refused.",
            "| `retail.cancel_reason` | yes | 1 / 1 / 0 | 0 / 1 / 0 | 0 / 1 / 0 | 1 / 1 |",
            "| `retail.confirmed` | logged |",
            "- `retail.cancel_reason`, task 1 (b): 'found it cheaper' is not an accepted reason",
        ] {
            assert!(md.contains(line), "{line}\n{md}");
        }
    }
}
