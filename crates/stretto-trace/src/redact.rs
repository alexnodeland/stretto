//! Pseudonymizing recorded sessions (`stretto redact`).
//!
//! A session log holds every value the tools returned and every word the
//! customer typed. [`redact`] rewrites a set of logs so that a value fewer
//! than `keep_shared` sessions share is replaced by a salted hash, and a
//! value that many sessions share (a status, a reason, a product type, a
//! field name) is kept. A value hashes the same wherever it appears, in any
//! of the logs, so what `stretto learn` needs still holds: which lookup
//! followed which call, where each argument's value was found in an earlier
//! result, and whether the customer had mentioned it.
//!
//! What is rewritten:
//! - the arguments of `tools/call` requests (the agent's and the proxy's),
//!   and their results: JSON values and keys, or each line of a result that
//!   is not JSON;
//! - the conversation (`context` entries), word by word;
//! - the server's notifications and the text of error responses.
//!
//! What is kept: tool names, JSON structure, `initialize` and `tools/list`,
//! timings and the header, whose server command the proxy already
//! redacted. The names that the tools' schemas in `tools/list` declare (the
//! tools, their properties, their enum values) are kept wherever they
//! appear, however few sessions use them, so that a rarely called tool's
//! argument names survive for the bindings. Values are compared lowercased
//! and trimmed, so `C7@x.com` and `c7@x.com` get the same hash.
//!
//! A value many sessions share is not always shared by many people: a
//! customer with several sessions shares their own id among them. The
//! values of the fields named in `hash_fields` (JSON keys, at any depth of
//! the arguments and results), and each word of them, are therefore hashed
//! however many sessions share them.
//!
//! This is pseudonymization, not anonymization. The hashes of one value
//! match across sessions, which is the point; a value shared by enough
//! sessions is kept even when it is personal, such as a common surname,
//! unless its field is named; and a value split differently in two places,
//! such as an address typed in the conversation and stored whole in a
//! record, no longer matches.

use crate::mcp::{LogEntry, McpLog, Peer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Characters trimmed from a word of the conversation before it is
/// compared or hashed; they are kept around the replacement.
const PUNCTUATION: &[char] = &[
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '"', '\'', '`',
];

/// Rewrite `logs`, hashing with `salt` every value fewer than `keep_shared`
/// of them share, and every value of the fields in `hash_fields`.
pub fn redact(
    logs: &[McpLog],
    salt: &str,
    keep_shared: usize,
    hash_fields: &[String],
) -> Vec<McpLog> {
    let fields: HashSet<String> = hash_fields.iter().map(|f| normalize(f)).collect();
    let mut census = Census::default();
    for log in logs {
        census.session = log.header.session.clone();
        for entry in walk(log) {
            visit(&entry.kind, entry.value, &mut |v| {
                census.add(v);
                if v.split_whitespace().nth(1).is_some() {
                    census.phrases.insert(normalize(v));
                }
            });
            if let (Kind::Tools, Some(m)) = (&entry.kind, entry.value) {
                declared(m, &mut |v| {
                    census.declared.insert(normalize(v));
                });
            }
            for doc in documents(&entry.kind, entry.value) {
                named(&doc, &fields, &mut |v| {
                    census.forced.insert(normalize(v));
                    for word in v.split_whitespace() {
                        census
                            .forced
                            .insert(normalize(word.trim_matches(PUNCTUATION)));
                    }
                });
            }
        }
    }
    let mut pseudonyms = Pseudonyms {
        census: &census,
        salt,
        keep_shared,
        phrases: HashMap::new(),
    };
    let mut phrases: HashMap<String, Vec<(Vec<String>, String)>> = HashMap::new();
    for value in &census.phrases {
        let Some(hash) = pseudonyms.of(value) else {
            continue;
        };
        let words: Vec<String> = value
            .split_whitespace()
            .map(|w| normalize(w.trim_matches(PUNCTUATION)))
            .filter(|w| !w.is_empty())
            .collect();
        if words.len() >= 2 {
            phrases
                .entry(words[0].clone())
                .or_default()
                .push((words, hash));
        }
    }
    for list in phrases.values_mut() {
        list.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
    }
    pseudonyms.phrases = phrases;
    logs.iter()
        .map(|log| {
            let mut out = log.clone();
            let kinds: Vec<Kind> = walk(log).into_iter().map(|e| e.kind).collect();
            for (entry, kind) in out.entries.iter_mut().zip(kinds) {
                rewrite(entry, &kind, &pseudonyms);
            }
            out
        })
        .collect()
}

/// Which values get which hash.
struct Pseudonyms<'a> {
    census: &'a Census,
    salt: &'a str,
    keep_shared: usize,
    /// The hashed values of several words, by their first word, longest
    /// first. Where one is typed in running text (the conversation, an
    /// error message) it is replaced whole, by the hash it has in the
    /// records, so "the desk lamp" the customer typed still contains what
    /// became of "Desk Lamp" in a result.
    phrases: HashMap<String, Vec<(Vec<String>, String)>>,
}

impl Pseudonyms<'_> {
    /// The hash that replaces `v`, or `None` to keep it.
    fn of(&self, v: &str) -> Option<String> {
        let key = normalize(v);
        let census = self.census;
        let keep = census.declared.contains(&key) || census.sessions(&key) >= self.keep_shared;
        if key.is_empty() || (keep && !census.forced.contains(&key)) {
            return None;
        }
        let digest = Sha256::digest(format!("{}\0{key}", self.salt).as_bytes());
        let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
        Some(format!("h_{hex}"))
    }

    fn swap(&self, v: &str) -> String {
        self.of(v).unwrap_or_else(|| v.to_string())
    }

    /// The hashed value of several words that `words` starts with, if any:
    /// how many words it spans, and its hash.
    fn phrase_at(&self, words: &[String]) -> Option<(usize, &str)> {
        self.phrases
            .get(words.first()?)?
            .iter()
            .find_map(|(ws, hash)| {
                (ws.len() <= words.len() && ws.iter().zip(words).all(|(a, b)| a == b))
                    .then_some((ws.len(), hash.as_str()))
            })
    }
}

/// How many sessions each value appears in, the names the tools declare,
/// and the values of the named fields.
#[derive(Default)]
struct Census {
    session: String,
    seen: HashMap<String, (usize, String)>,
    declared: HashSet<String>,
    forced: HashSet<String>,
    /// Values of several words, from the arguments and results.
    phrases: HashSet<String>,
}

impl Census {
    fn add(&mut self, value: &str) {
        let key = normalize(value);
        if key.is_empty() {
            return;
        }
        let entry = self.seen.entry(key).or_insert((0, String::new()));
        if entry.1 != self.session {
            entry.0 += 1;
            entry.1 = self.session.clone();
        }
    }

    fn sessions(&self, key: &str) -> usize {
        self.seen.get(key).map_or(0, |e| e.0)
    }
}

/// A value as it is compared and hashed: trimmed, lower-cased, and without
/// a leading `#`, since an order a record writes `#W1` is often typed `W1`.
fn normalize(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .trim_start_matches('#')
        .to_string()
}

/// What an entry carries, and which of its parts are data.
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// Nothing to rewrite.
    Keep,
    /// A `tools/call` request: its arguments.
    Call,
    /// The response to a `tools/call`: its content and structured content,
    /// and its error's text.
    CallResult,
    /// A notification or request from the server, or another response's
    /// error: its params and error text.
    Other,
    /// A message of the conversation: its content, word by word.
    Conversation,
    /// A line that is not JSON: word by word.
    Raw,
    /// The response to `tools/list`: kept, and read for the names it
    /// declares.
    Tools,
}

struct Walked<'a> {
    kind: Kind,
    value: Option<&'a Value>,
}

/// Each entry of `log` with its kind. A response is a tool result when it
/// answers a `tools/call` the client or the proxy sent.
fn walk(log: &McpLog) -> Vec<Walked<'_>> {
    let mut calls: HashMap<String, String> = HashMap::new();
    log.entries
        .iter()
        .map(|e| {
            let Some(m) = &e.message else {
                return Walked {
                    kind: if e.raw.is_some() {
                        Kind::Raw
                    } else {
                        Kind::Keep
                    },
                    value: None,
                };
            };
            let method = m.get("method").and_then(Value::as_str);
            let id = m.get("id").filter(|id| !id.is_null()).map(Value::to_string);
            let kind = match (e.from, method) {
                (Peer::Context, _) => Kind::Conversation,
                (Peer::Client | Peer::Proxy, Some(method)) => {
                    if let Some(id) = &id {
                        calls.insert(id.clone(), method.to_string());
                    }
                    if method == "tools/call" {
                        Kind::Call
                    } else {
                        Kind::Keep
                    }
                }
                (Peer::Server, None) => {
                    match id.as_ref().and_then(|id| calls.get(id)).map(String::as_str) {
                        Some("tools/call") => Kind::CallResult,
                        _ if m.get("error").is_some() => Kind::Other,
                        Some("tools/list") => Kind::Tools,
                        _ => Kind::Keep,
                    }
                }
                (Peer::Server, Some(_)) => Kind::Other,
                _ => Kind::Keep,
            };
            Walked {
                kind,
                value: Some(m),
            }
        })
        .collect()
}

/// Call `f` with every value `kind` marks as data in `m`.
fn visit(kind: &Kind, m: Option<&Value>, f: &mut impl FnMut(&str)) {
    match (kind, m) {
        (Kind::Call, Some(m)) => {
            if let Some(a) = m.pointer("/params/arguments") {
                leaves(a, f);
            }
        }
        (Kind::CallResult, Some(m)) => {
            for item in m
                .pointer("/result/content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    text_values(text, f);
                }
            }
            if let Some(s) = m.pointer("/result/structuredContent") {
                leaves(s, f);
            }
            error_words(m, f);
        }
        (Kind::Other, Some(m)) => {
            if let Some(p) = m.get("params") {
                leaves(p, f);
            }
            error_words(m, f);
        }
        (Kind::Conversation, Some(m)) => {
            if let Some(text) = m.get("content").and_then(Value::as_str) {
                words(text, f);
            }
        }
        _ => {}
    }
}

fn error_words(m: &Value, f: &mut impl FnMut(&str)) {
    if let Some(text) = m.pointer("/error/message").and_then(Value::as_str) {
        words(text, f);
    }
    if let Some(data) = m.pointer("/error/data") {
        leaves(data, f);
    }
}

/// Every key, string and number in `v`.
fn leaves(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Number(n) => f(&n.to_string()),
        Value::Array(items) => items.iter().for_each(|i| leaves(i, f)),
        Value::Object(map) => {
            for (k, v) in map {
                f(k);
                leaves(v, f);
            }
        }
        _ => {}
    }
}

/// The JSON that `kind` marks as data in `m`: a call's arguments, a
/// result's JSON text and structured content, a message's params, an
/// error's data.
fn documents(kind: &Kind, m: Option<&Value>) -> Vec<Value> {
    let Some(m) = m else {
        return Vec::new();
    };
    let pointers: &[&str] = match kind {
        Kind::Call => &["/params/arguments"],
        Kind::CallResult => &["/result/structuredContent", "/error/data"],
        Kind::Other => &["/params", "/error/data"],
        _ => &[],
    };
    let mut docs: Vec<Value> = pointers
        .iter()
        .filter_map(|p| m.pointer(p).cloned())
        .collect();
    if *kind == Kind::CallResult {
        for item in m
            .pointer("/result/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                docs.extend(serde_json::from_str::<Value>(text).ok());
            }
        }
    }
    docs
}

/// Every string and number under a key named in `fields`, at any depth of
/// `v`. The keys under it are left alone: they are usually field names.
fn named(v: &Value, fields: &HashSet<String>, f: &mut impl FnMut(&str)) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| named(i, fields, f)),
        Value::Object(map) => {
            for (k, v) in map {
                if fields.contains(&normalize(k)) {
                    scalars(v, f);
                } else {
                    named(v, fields, f);
                }
            }
        }
        _ => {}
    }
}

/// Every string and number in `v`.
fn scalars(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Number(n) => f(&n.to_string()),
        Value::Array(items) => items.iter().for_each(|i| scalars(i, f)),
        Value::Object(map) => map.values().for_each(|v| scalars(v, f)),
        _ => {}
    }
}

/// The names a `tools/list` response declares: each tool's name, and the
/// property names and enum values of its input and output schemas.
fn declared(m: &Value, f: &mut impl FnMut(&str)) {
    fn schema(v: &Value, f: &mut impl FnMut(&str)) {
        match v {
            Value::Object(map) => {
                if let Some(Value::Object(properties)) = map.get("properties") {
                    properties.keys().for_each(|k| f(k));
                }
                if let Some(Value::Array(values)) = map.get("enum") {
                    values.iter().filter_map(Value::as_str).for_each(&mut *f);
                }
                map.values().for_each(|v| schema(v, f));
            }
            Value::Array(items) => items.iter().for_each(|i| schema(i, f)),
            _ => {}
        }
    }
    for tool in m
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = tool.get("name").and_then(Value::as_str) {
            f(name);
        }
        for key in ["inputSchema", "outputSchema"] {
            if let Some(s) = tool.get(key) {
                schema(s, f);
            }
        }
    }
}

/// A tool's text: its JSON values, or each line.
fn text_values(text: &str, f: &mut impl FnMut(&str)) {
    match serde_json::from_str::<Value>(text) {
        Ok(v) if v.is_object() || v.is_array() => leaves(&v, f),
        _ => {
            for line in text.lines() {
                f(line)
            }
        }
    }
}

/// Each word of `text`, without its surrounding punctuation.
fn words(text: &str, f: &mut impl FnMut(&str)) {
    for word in text.split_whitespace() {
        let core = word.trim_matches(PUNCTUATION);
        if !core.is_empty() {
            f(core);
        }
    }
}

/// `entry` with the values `kind` marks replaced by their pseudonyms.
fn rewrite(entry: &mut LogEntry, kind: &Kind, p: &Pseudonyms) {
    if let (Kind::Raw, Some(raw)) = (kind, entry.raw.as_mut()) {
        *raw = rewrite_words(raw, p);
        return;
    }
    let Some(m) = entry.message.as_mut() else {
        return;
    };
    match kind {
        Kind::Call => {
            if let Some(a) = m.pointer_mut("/params/arguments") {
                *a = rewrite_leaves(a, p);
            }
        }
        Kind::CallResult => {
            if let Some(items) = m
                .pointer_mut("/result/content")
                .and_then(Value::as_array_mut)
            {
                for item in items {
                    if let Some(Value::String(text)) = item.get_mut("text") {
                        *text = rewrite_text(text, p);
                    }
                }
            }
            if let Some(s) = m.pointer_mut("/result/structuredContent") {
                *s = rewrite_leaves(s, p);
            }
            rewrite_error(m, p);
        }
        Kind::Other => {
            if let Some(params) = m.get_mut("params") {
                *params = rewrite_leaves(params, p);
            }
            rewrite_error(m, p);
        }
        Kind::Conversation => {
            if let Some(Value::String(text)) = m.get_mut("content") {
                *text = rewrite_words(text, p);
            }
        }
        Kind::Keep | Kind::Raw | Kind::Tools => {}
    }
}

fn rewrite_error(m: &mut Value, p: &Pseudonyms) {
    if let Some(Value::String(text)) = m.pointer_mut("/error/message") {
        *text = rewrite_words(text, p);
    }
    if let Some(data) = m.pointer_mut("/error/data") {
        *data = rewrite_leaves(data, p);
    }
}

/// `v` with each key, string and number that has a pseudonym replaced by it.
fn rewrite_leaves(v: &Value, p: &Pseudonyms) -> Value {
    match v {
        Value::String(s) => p.of(s).map_or_else(|| v.clone(), Value::String),
        Value::Number(n) => p
            .of(&n.to_string())
            .map_or_else(|| v.clone(), Value::String),
        Value::Array(items) => Value::Array(items.iter().map(|i| rewrite_leaves(i, p)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (p.swap(k), rewrite_leaves(v, p)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// A tool's text, rewritten as JSON (compact) or line by line.
fn rewrite_text(text: &str, p: &Pseudonyms) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(v) if v.is_object() || v.is_array() => rewrite_leaves(&v, p).to_string(),
        _ => text
            .lines()
            .map(|l| p.swap(l))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// `text` with each hashed value of several words replaced whole, and each
/// other word swapped, its punctuation and line breaks kept.
fn rewrite_words(text: &str, p: &Pseudonyms) -> String {
    text.lines()
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let cores: Vec<String> = words
                .iter()
                .map(|w| normalize(w.trim_matches(PUNCTUATION)))
                .collect();
            let mut out = Vec::with_capacity(words.len());
            let mut i = 0;
            while i < words.len() {
                if let Some((n, hash)) = p.phrase_at(&cores[i..]) {
                    let (first, last) = (words[i], words[i + n - 1]);
                    let lead = &first[..first.len() - first.trim_start_matches(PUNCTUATION).len()];
                    let trail = &last[last.trim_end_matches(PUNCTUATION).len()..];
                    out.push(format!("{lead}{hash}{trail}"));
                    i += n;
                    continue;
                }
                let word = words[i];
                let core = word.trim_matches(PUNCTUATION);
                out.push(if core.is_empty() {
                    word.to_string()
                } else {
                    let start = word.find(core).unwrap_or(0);
                    format!(
                        "{}{}{}",
                        &word[..start],
                        p.swap(core),
                        &word[start + core.len()..]
                    )
                });
                i += 1;
            }
            out.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::parse_log;
    use serde_json::json;

    fn log(session: usize, email: &str, user: &str) -> McpLog {
        let lines = [
            json!({"stretto_mcp_log": 2, "session": format!("s{session}"), "started_unix_ms": 0,
                   "server_command": ["demo"], "domain": "shop", "agent_model": null}),
            json!({"t_ms": 0, "from": "context", "message": {"role": "user",
                   "content": format!("Hi, I'm {email}. Cancel my order, please!")}}),
            json!({"t_ms": 1, "from": "client", "message": {"jsonrpc": "2.0", "id": 1,
                   "method": "tools/call", "params": {"name": "find", "arguments": {"email": email}}}}),
            json!({"t_ms": 2, "from": "server", "message": {"jsonrpc": "2.0", "id": 1, "result":
                   {"content": [{"type": "text", "text": user}]}}}),
            json!({"t_ms": 3, "from": "client", "message": {"jsonrpc": "2.0", "id": 2,
                   "method": "tools/call", "params": {"name": "get", "arguments": {"user_id": user}}}}),
            json!({"t_ms": 4, "from": "server", "message": {"jsonrpc": "2.0", "id": 2, "result":
                   {"content": [{"type": "text", "text":
                     json!({"user_id": user, "status": "pending", "cards": {format!("card_{user}"): 1}}).to_string()}]}}}),
            json!({"t_ms": 5, "from": "client", "message": {"jsonrpc": "2.0", "id": 3, "method": "tools/list"}}),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        parse_log(&text).unwrap()
    }

    #[test]
    fn hashes_what_few_sessions_share_and_keeps_the_rest() {
        let logs: Vec<McpLog> = (0..4)
            .map(|i| log(i, &format!("c{i}@x.com"), &format!("user_{i}")))
            .collect();
        let out = redact(&logs, "salt", 3, &[]);
        let text: String = out[0]
            .entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect();
        // What identifies the customer is gone...
        assert!(
            !text.contains("c0@x.com") && !text.contains("user_0"),
            "{text}"
        );
        // ...what every session shares is kept...
        for kept in ["pending", "status", "Cancel", "user_id", "tools/list"] {
            assert!(text.contains(kept), "{kept} in {text}");
        }
        // ...and a value hashes the same wherever it appears: the email the
        // customer typed and the one the agent passed; the user id returned
        // and the one passed on.
        let m = |i: usize| out[0].entries[i].message.clone().unwrap();
        let email = m(1)["params"]["arguments"]["email"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(email.starts_with("h_"), "{email}");
        assert!(m(0)["content"]
            .as_str()
            .unwrap()
            .contains(&format!("{email}. Cancel")));
        let user = m(2)["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(m(3)["params"]["arguments"]["user_id"], user.as_str());
        let record: Value =
            serde_json::from_str(m(4)["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(record["user_id"], user.as_str());
        assert!(record["cards"]
            .as_object()
            .unwrap()
            .keys()
            .all(|k| k.starts_with("h_")));
        // Another salt, other hashes.
        let other = redact(&logs, "pepper", 3, &[]);
        assert_ne!(other[0].entries[1].message, out[0].entries[1].message);
    }

    #[test]
    fn a_value_of_several_words_typed_in_the_conversation_gets_its_records_hash() {
        // The customer types the item's name and the order without its `#`;
        // the record holds both as values. Four sessions share the words
        // "desk" and "lamp", but only this one the item and the order.
        let logs: Vec<McpLog> = (0..4)
            .map(|i| {
                let said = if i == 0 {
                    "Return the Desk Lamp, order W1 please.".to_string()
                } else {
                    format!("Return the desk and the lamp, order W{}0 please.", i + 1)
                };
                let record = if i == 0 {
                    json!({"order_id": "#W1", "items": [{"name": "Desk Lamp"}]})
                } else {
                    json!({"order_id": format!("#W{}0", i + 1), "items": [{"name": "Chair"}]})
                };
                let lines = [
                    json!({"stretto_mcp_log": 2, "session": format!("s{i}"), "started_unix_ms": 0,
                           "server_command": ["demo"], "domain": "shop", "agent_model": null}),
                    json!({"t_ms": 0, "from": "context", "message": {"role": "user", "content": said}}),
                    json!({"t_ms": 1, "from": "client", "message": {"jsonrpc": "2.0", "id": 1,
                           "method": "tools/call", "params": {"name": "get", "arguments": {}}}}),
                    json!({"t_ms": 2, "from": "server", "message": {"jsonrpc": "2.0", "id": 1,
                           "result": {"content": [{"type": "text", "text": record.to_string()}]}}}),
                ];
                parse_log(&lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap()
            })
            .collect();
        let out = redact(&logs, "salt", 3, &[]);
        let m = |i: usize| out[0].entries[i].message.clone().unwrap();
        let record: Value =
            serde_json::from_str(m(2)["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let (item, order) = (
            record["items"][0]["name"].as_str().unwrap(),
            record["order_id"].as_str().unwrap(),
        );
        assert!(
            item.starts_with("h_") && order.starts_with("h_"),
            "{record}"
        );
        assert_eq!(
            m(0)["content"],
            format!("Return the {item}, order {order} please.")
        );
        // Where the words stand alone, they are kept: four sessions share them.
        assert_eq!(
            out[1].entries[0].message.as_ref().unwrap()["content"]
                .as_str()
                .unwrap()
                .split(", order")
                .next(),
            Some("Return the desk and the lamp")
        );
    }

    #[test]
    fn hashes_named_fields_however_shared_and_keeps_declared_names() {
        // One customer, four sessions, so their id and name are shared by
        // all four. Only the first session passes the rare argument.
        let logs: Vec<McpLog> = (0..4)
            .map(|i| {
                let mut arguments = json!({"full_name": "Ada Lovelace"});
                if i == 0 {
                    arguments["rare_flag"] = json!("seldom");
                }
                let lines = [
                    json!({"stretto_mcp_log": 2, "session": format!("s{i}"), "started_unix_ms": 0,
                           "server_command": ["demo"], "domain": "shop", "agent_model": null}),
                    json!({"t_ms": 0, "from": "context", "message": {"role": "user",
                           "content": "Hi, I'm Ada Lovelace."}}),
                    json!({"t_ms": 1, "from": "client", "message": {"jsonrpc": "2.0", "id": 1,
                           "method": "tools/list"}}),
                    json!({"t_ms": 2, "from": "server", "message": {"jsonrpc": "2.0", "id": 1,
                           "result": {"tools": [{"name": "find", "inputSchema": {"type": "object",
                             "properties": {"full_name": {"type": "string"},
                                            "rare_flag": {"type": "string", "enum": ["seldom"]}}}}]}}}),
                    json!({"t_ms": 3, "from": "client", "message": {"jsonrpc": "2.0", "id": 2,
                           "method": "tools/call", "params": {"name": "find", "arguments": arguments}}}),
                    json!({"t_ms": 4, "from": "server", "message": {"jsonrpc": "2.0", "id": 2, "result":
                           {"content": [{"type": "text", "text": json!({"user_id": "ada_1",
                             "name": {"first": "Ada", "last": "Lovelace"}}).to_string()}]}}}),
                ];
                parse_log(&lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap()
            })
            .collect();
        let text = |out: &[McpLog]| out[0].to_jsonl();
        // Shared by four sessions, the customer's values are kept; the rare
        // argument's name and value are kept because the schema declares
        // them.
        let shared = text(&redact(&logs, "salt", 3, &[]));
        for kept in ["ada_1", "Ada Lovelace", "rare_flag", "seldom"] {
            assert!(shared.contains(kept), "{kept} in {shared}");
        }
        // Named, their values are hashed wherever they appear, word by word
        // in the conversation; the keys under a named field are kept.
        let named: Vec<String> = ["user_id", "name", "full_name"].map(String::from).into();
        let out = redact(&logs, "salt", 3, &named);
        let hashed = text(&out);
        for gone in ["ada_1", "Ada", "Lovelace"] {
            assert!(!hashed.contains(gone), "{gone} in {hashed}");
        }
        for kept in ["user_id", "first", "last", "full_name", "rare_flag"] {
            assert!(hashed.contains(kept), "{kept} in {hashed}");
        }
        let said = out[0].entries[0].message.as_ref().unwrap()["content"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            said.starts_with("Hi, I'm h_") && said.ends_with('.'),
            "{said}"
        );
    }
}
