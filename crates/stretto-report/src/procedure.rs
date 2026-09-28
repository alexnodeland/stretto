//! A procedure compiled once from agents' traces, run with no model.
//!
//! Where no user speaks, every branch of an agent's work follows what a tool
//! returned, so the whole procedure can be compiled, writes included
//! (`scripts/telecom_workflow.py --export` writes one from τ²-bench telecom's
//! solo runs). Per *site*, the tool that just returned (`!` after a failed
//! call) or `start`, a decision tree over the run's state picks the next
//! call. The state is a set of features: each tool's last result (its fields
//! and, for a phone's checks, each `Key: value` line), which tools were
//! called, which writes were made, and the ticket's words. A call's
//! identifiers are held as where they came from, the path of an earlier
//! result (`@tool$.path[*]`, with the fields that set its record apart) or
//! the shape they had in the ticket (`@ticket` and a pattern), and are bound
//! again from this run's results. A guard skips a read made since the last
//! write, or a write already made. The run stops when the tree says so, then
//! checks the outcome its ticket states with a read of its own, and hands
//! the ticket back when that check fails.
//!
//! [`Procedure::run`] takes the tools as a [`Tools`], so the same run serves
//! an MCP server (`stretto-procedure`) or a test double.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The procedure format this build reads.
pub const PROCEDURE_VERSION: u32 = 1;

/// A compiled procedure (the procedure IR).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Procedure {
    /// The format version; see [`PROCEDURE_VERSION`].
    pub stretto_procedure: u32,
    /// The domain it was compiled for.
    pub domain: String,
    /// Where it came from.
    #[serde(default)]
    pub provenance: Value,
    /// Whether its identifiers are held as where they came from, and bound
    /// again at run time; otherwise they are constants of the traces.
    pub symbolic: bool,
    /// The most calls a run makes.
    pub max_calls: usize,
    /// The action that ends a run.
    pub stop: String,
    /// The tool that hands the customer to a person, which also ends a run.
    pub handoff: String,
    /// The arguments the hand-off is called with.
    #[serde(default)]
    pub handoff_arguments: Map<String, Value>,
    /// The tools that only read.
    pub reads: BTreeSet<String>,
    /// The ticket's words and word pairs the trees may ask about.
    pub vocab: BTreeSet<String>,
    /// Per site, its tree.
    pub trees: BTreeMap<String, Node>,
    /// The node for a site training never saw.
    pub fallback: Node,
    /// The outcomes a ticket may state, each with the read that checks it.
    pub checks: Vec<Check>,
}

/// A node of a site's tree: a question about the state, or a leaf with the
/// calls made there in training, by count, in the order the run tries them.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Node {
    /// Go to `yes` if the state has `feature`, else to `no`.
    Split {
        feature: String,
        yes: Box<Node>,
        no: Box<Node>,
    },
    /// The calls, each an action (`tool{"arg": value}`, or the stop) and its
    /// count in training.
    Leaf { calls: Vec<(String, u64)> },
}

impl Node {
    /// The leaf the state reaches.
    fn leaf(&self, state: &BTreeSet<String>) -> &[(String, u64)] {
        let mut node = self;
        loop {
            match node {
                Node::Split { feature, yes, no } => {
                    node = if state.contains(feature) { yes } else { no };
                }
                Node::Leaf { calls } => return calls,
            }
        }
    }
}

/// An outcome a ticket may state ("they will consider the issue resolved
/// when ..."), and the read that checks it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    /// The words of the ticket that state it.
    pub phrase: String,
    /// The read that checks it, called with no arguments.
    pub probe: String,
    /// What the read's result, lowercased, contains when the outcome holds.
    pub contains: Vec<String>,
    /// And what it lacks.
    pub lacks: Vec<String>,
}

/// The tools a run calls.
pub trait Tools {
    /// Call `name` with `arguments`: the result's text, and whether the
    /// call failed.
    fn call(&mut self, name: &str, arguments: &Map<String, Value>) -> Result<(String, bool)>;
}

/// One call of a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub tool: String,
    pub arguments: Map<String, Value>,
    /// Whether it failed.
    pub failed: bool,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Its check says the outcome the ticket states holds.
    Resolved,
    /// It handed the customer to a person, the policy's own ending for what
    /// an agent may not fix.
    Transferred,
    /// Its check says the outcome does not hold, or the ticket states none
    /// it can check: the ticket goes to a model.
    HandBack,
}

/// A run: its calls, its check of the outcome, and how it ended.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub calls: Vec<Call>,
    /// The check's read and what it returned, if the run made one.
    pub check: Option<(String, String)>,
    pub verdict: Verdict,
}

impl Procedure {
    /// Parse a procedure, refusing another format version.
    pub fn from_json(text: &str) -> Result<Self> {
        let v: Value = serde_json::from_str(text)?;
        match v.get("stretto_procedure").and_then(Value::as_u64) {
            Some(n) if n == u64::from(PROCEDURE_VERSION) => {}
            Some(n) => bail!(
                "procedure format {n} is not supported (this build reads {PROCEDURE_VERSION})"
            ),
            None => bail!("not a stretto procedure: no `stretto_procedure` version"),
        }
        Ok(serde_json::from_value(v)?)
    }

    /// Read a procedure file.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        Self::from_json(&text)
    }

    /// Run on `ticket` with `tools`, then check the outcome the ticket
    /// states.
    pub fn run(&self, ticket: &str, tools: &mut dyn Tools) -> Result<Run> {
        let mut state = State::new(self, ticket);
        let mut calls: Vec<Call> = Vec::new();
        let mut made: BTreeSet<String> = BTreeSet::new();
        // Reads made since the last write.
        let mut since_write: BTreeSet<String> = BTreeSet::new();
        // Per tool and argument, the values passed so far.
        let mut passed: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for _ in 0..self.max_calls {
            let features = state.features();
            let node = self.trees.get(&state.site).unwrap_or(&self.fallback);
            // The leaf's likeliest action that binds and is not a repeat.
            let mut chosen = None;
            for (action, _) in node.leaf(&features) {
                if *action == self.stop {
                    chosen = Some((action.clone(), None));
                    break;
                }
                let Some((tool, arguments)) = self.concrete(action, &state, &passed)? else {
                    continue;
                };
                let key = label(&tool, &arguments);
                let repeat = if self.reads.contains(&tool) {
                    since_write.contains(&key)
                } else {
                    made.contains(&key)
                };
                if !repeat {
                    chosen = Some((action.clone(), Some((tool, arguments, key))));
                    break;
                }
            }
            let Some((action, Some((tool, arguments, key)))) = chosen else {
                break;
            };
            let (content, failed) = tools.call(&tool, &arguments)?;
            for (k, v) in &arguments {
                if let Value::String(s) = v {
                    passed
                        .entry((tool.clone(), k.clone()))
                        .or_default()
                        .insert(s.clone());
                }
            }
            if self.reads.contains(&tool) {
                since_write.insert(key.clone());
            } else {
                since_write.clear();
                state.fixes.insert(action);
            }
            made.insert(key);
            state.called.insert(tool.clone());
            state.result(&tool, &content, failed);
            calls.push(Call {
                tool: tool.clone(),
                arguments,
                failed,
            });
            if tool == self.handoff {
                break;
            }
        }
        if calls.last().is_some_and(|c| c.tool == self.handoff) {
            return Ok(Run {
                calls,
                check: None,
                verdict: Verdict::Transferred,
            });
        }
        let lower = ticket.to_lowercase();
        let Some(check) = self.checks.iter().find(|c| lower.contains(&c.phrase)) else {
            return Ok(Run {
                calls,
                check: None,
                verdict: Verdict::HandBack,
            });
        };
        let (content, _) = tools.call(&check.probe, &Map::new())?;
        let result = content.to_lowercase();
        let holds = check.contains.iter().all(|c| result.contains(c.as_str()))
            && !check.lacks.iter().any(|x| result.contains(x.as_str()));
        Ok(Run {
            calls,
            check: Some((check.probe.clone(), content)),
            verdict: if holds {
                Verdict::Resolved
            } else {
                Verdict::HandBack
            },
        })
    }

    /// The call an action makes here: its identifiers bound again from this
    /// run's results and ticket, or `None` if one cannot be.
    fn concrete(
        &self,
        action: &str,
        state: &State,
        passed: &HashMap<(String, String), BTreeSet<String>>,
    ) -> Result<Option<(String, Map<String, Value>)>> {
        let (tool, arguments) = self.unlabel(action)?;
        if !self.symbolic || tool == self.handoff {
            return Ok(Some((tool, arguments)));
        }
        let none = BTreeSet::new();
        let mut bound = Map::new();
        for (k, v) in arguments {
            let passed = passed.get(&(tool.clone(), k.clone())).unwrap_or(&none);
            match resolve(&v, &state.outputs, &state.text, passed) {
                Some(b) => {
                    bound.insert(k, b);
                }
                None => return Ok(None),
            }
        }
        Ok(Some((tool, bound)))
    }

    /// An action's tool and arguments.
    fn unlabel(&self, action: &str) -> Result<(String, Map<String, Value>)> {
        match action.find('{') {
            None if action == self.handoff => {
                Ok((action.to_string(), self.handoff_arguments.clone()))
            }
            None => Ok((action.to_string(), Map::new())),
            Some(i) => {
                let arguments: Map<String, Value> = serde_json::from_str(&action[i..])
                    .map_err(|e| anyhow::anyhow!("action {action}: {e}"))?;
                Ok((action[..i].to_string(), arguments))
            }
        }
    }
}

/// A call as the guard compares calls: its tool and canonical arguments.
fn label(tool: &str, arguments: &Map<String, Value>) -> String {
    if arguments.is_empty() {
        tool.to_string()
    } else {
        format!("{tool}{}", Value::Object(arguments.clone()))
    }
}

/// A feature's value: set (`true`), unset (`false`, as a failed call's
/// `error`), or a string (a list's length: `0`, `1` or `2+`).
#[derive(Clone, Debug, PartialEq)]
enum Feature {
    Yes,
    No,
    Is(String),
}

/// A run's state.
struct State {
    /// Per tool, the features of its last result.
    latest: BTreeMap<String, BTreeMap<String, Feature>>,
    /// The tools called.
    called: BTreeSet<String>,
    /// The writes made, as the actions that made them.
    fixes: BTreeSet<String>,
    /// The tool that just returned (`!` if it failed), or `start`.
    site: String,
    /// `ticket: WORD` for each of the ticket's words the trees know.
    words: BTreeSet<String>,
    /// The ticket.
    text: String,
    /// Each result so far, parsed, with its tool.
    outputs: Vec<(String, Value)>,
    /// Per tool, the record the ticket names (the line with the ticket's
    /// number), which another record of its kind read later does not
    /// replace in the state.
    named: HashMap<String, Map<String, Value>>,
    symbolic: bool,
}

impl State {
    fn new(p: &Procedure, ticket: &str) -> Self {
        State {
            latest: BTreeMap::new(),
            called: BTreeSet::new(),
            fixes: BTreeSet::new(),
            site: "start".to_string(),
            words: words(ticket)
                .into_iter()
                .filter(|w| p.vocab.contains(w))
                .map(|w| format!("ticket: {w}"))
                .collect(),
            text: ticket.to_string(),
            outputs: Vec::new(),
            named: HashMap::new(),
            symbolic: p.symbolic,
        }
    }

    /// Everything the trees ask about.
    fn features(&self) -> BTreeSet<String> {
        let mut out = self.words.clone();
        out.extend(self.called.iter().map(|t| format!("called {t}")));
        out.extend(self.fixes.iter().map(|a| format!("made {a}")));
        for (tool, fs) in &self.latest {
            for (k, v) in fs {
                out.insert(match v {
                    Feature::Yes => format!("{tool}:{k}"),
                    Feature::No => format!("{tool}:{k}=False"),
                    Feature::Is(s) => format!("{tool}:{k}={s}"),
                });
            }
        }
        out
    }

    /// Take in a call's result.
    fn result(&mut self, tool: &str, content: &str, failed: bool) {
        let value = if failed { Value::Null } else { parse(content) };
        self.site = if failed {
            format!("{tool}!")
        } else {
            tool.to_string()
        };
        if !failed {
            self.outputs.push((tool.to_string(), value.clone()));
        }
        let kept = self.named.get(tool).cloned();
        let same_kind = match (&value, &kept) {
            (Value::Object(v), Some(k)) => v.keys().eq(k.keys()),
            _ => false,
        };
        // Another record of the kind the ticket names, read after it, does
        // not replace it.
        if self.symbolic && !failed && same_kind && !self.names(&value) {
            return;
        }
        self.latest
            .insert(tool.to_string(), result_features(failed, &value));
        match &value {
            Value::Object(v) if self.names(&value) => {
                self.named.insert(tool.to_string(), v.clone());
            }
            Value::Object(_) if same_kind => {}
            _ => {
                self.named.remove(tool);
            }
        }
    }

    /// Whether a result holds a value the ticket gives (an id, a number).
    fn names(&self, value: &Value) -> bool {
        let given = given(&self.text);
        let mut leaves = Vec::new();
        scalar_leaves(value, &mut leaves);
        leaves.iter().any(|l| given.contains(l))
    }
}

/// A result's text as a value: its JSON (a JSON string's own JSON, if it
/// holds any), or the text.
fn parse(content: &str) -> Value {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::String(s)) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        Ok(v) => v,
        Err(_) => Value::String(content.to_string()),
    }
}

/// Digits, as τ²-bench's results write them (ASCII only).
fn digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn has_digit(s: &str) -> bool {
    s.chars().any(digit)
}

/// A value as the features compare it: lowercased and trimmed; booleans
/// `true` and `false`.
fn norm(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.trim().to_lowercase(),
        other => other.to_string().trim().to_lowercase(),
    }
}

/// The features of one result: whether the call failed; for a record, each
/// short field (ids and free text say nothing across episodes), each list's
/// or map's length, and the statuses in a map of records; for a list, its
/// length; for text, [`text_features`].
fn result_features(failed: bool, value: &Value) -> BTreeMap<String, Feature> {
    let mut f = BTreeMap::new();
    f.insert(
        "error".to_string(),
        if failed { Feature::Yes } else { Feature::No },
    );
    let size = |n: usize| {
        Feature::Is(match n {
            0 => "0".to_string(),
            1 => "1".to_string(),
            _ => "2+".to_string(),
        })
    };
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::String(s) => {
                        if s.chars().count() > 24 || has_run_of_digits(s, 3) {
                            continue;
                        }
                        f.insert(format!("{k}={}", norm(v)), Feature::Yes);
                    }
                    Value::Bool(_) => {
                        f.insert(format!("{k}={}", norm(v)), Feature::Yes);
                    }
                    Value::Number(n) if n.is_i64() || n.is_u64() => {
                        f.insert(format!("{k}={}", norm(v)), Feature::Yes);
                    }
                    Value::Array(items) => {
                        f.insert(format!("len({k})"), size(items.len()));
                    }
                    Value::Object(items) => {
                        f.insert(format!("len({k})"), size(items.len()));
                        for item in items.values() {
                            if let Some(Value::String(status)) = item.get("status") {
                                f.insert(
                                    format!("{k}.*.status={}", status.trim().to_lowercase()),
                                    Feature::Yes,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            f.insert("len($)".to_string(), size(items.len()));
        }
        Value::String(text) => f.extend(text_features(text)),
        _ => {}
    }
    f
}

fn has_run_of_digits(s: &str, n: usize) -> bool {
    let mut run = 0;
    for c in s.chars() {
        run = if digit(c) { run + 1 } else { 0 };
        if run >= n {
            return true;
        }
    }
    false
}

/// Features of a result in text, as a phone's checks print them: each
/// `Key: value` line's value, or each of its `|`-separated parts, and each
/// short line of its own. Parts with digits (a battery level, a speed) are
/// left out, as ids are from records.
fn text_features(text: &str) -> BTreeMap<String, Feature> {
    let mut f = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(": ") else {
            if line.chars().count() <= 60 && !has_digit(line) {
                f.insert(format!("says {}", line.to_lowercase()), Feature::Yes);
            }
            continue;
        };
        for part in value.split('|') {
            let part = part.trim();
            if !part.is_empty() && !has_digit(part) {
                f.insert(
                    format!("{}={}", key.trim().to_lowercase(), part.to_lowercase()),
                    Feature::Yes,
                );
            }
        }
    }
    f
}

/// The ticket's words and word pairs, digits left out.
fn words(ticket: &str) -> BTreeSet<String> {
    let lower = ticket.to_lowercase();
    let w: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out: BTreeSet<String> = w.iter().map(|s| s.to_string()).collect();
    out.extend(w.windows(2).map(|p| format!("{} {}", p[0], p[1])));
    out
}

/// Whether a value is an identifier: at least four characters, one a digit.
fn keyish(s: &str) -> bool {
    s.chars().count() >= 4 && has_digit(s)
}

/// The identifiers a ticket gives: runs of word characters, `.`, `@` and
/// `-` that hold a digit, at least four long.
fn given(ticket: &str) -> BTreeSet<String> {
    ticket
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '@' | '-')))
        .filter(|s| keyish(s))
        .map(str::to_string)
        .collect()
}

/// Every string and number under a value, as text.
fn scalar_leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => map.values().for_each(|v| scalar_leaves(v, out)),
        Value::Array(items) => items.iter().for_each(|v| scalar_leaves(v, out)),
        Value::String(s) => out.push(s.clone()),
        Value::Number(n) => out.push(python_number(n)),
        _ => {}
    }
}

/// A number as Python writes it (`15.0` for a whole float).
fn python_number(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if !(n.is_i64() || n.is_u64()) && f.fract() == 0.0 && f.abs() < 1e16 => {
            format!("{f:.1}")
        }
        _ => n.to_string(),
    }
}

/// A path's steps: keys, and `*` for every element of a list.
fn steps(path: &str) -> Vec<Option<&str>> {
    let mut out = Vec::new();
    let mut rest = path.strip_prefix('$').unwrap_or(path);
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("[*]") {
            out.push(None);
            rest = r;
        } else if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            out.push(Some(&r[..end]));
            rest = &r[end..];
        } else {
            break;
        }
    }
    out
}

/// Everything in `value` at `path`, in document order.
fn select<'a>(value: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut here = vec![value];
    for step in steps(path) {
        here = here
            .into_iter()
            .flat_map(|v| match (step, v) {
                (None, Value::Array(items)) => items.iter().collect(),
                (Some(k), Value::Object(map)) => map.get(k).into_iter().collect(),
                _ => Vec::new(),
            })
            .collect();
    }
    here
}

/// The strings of `value` at `path`, in document order.
fn at(value: &Value, path: &str) -> Vec<String> {
    select(value, path)
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// For a path through a list of records, each string at the path with its
/// record.
fn members<'a>(value: &'a Value, path: &str) -> Vec<(String, &'a Map<String, Value>)> {
    let Some(i) = path.rfind("[*]") else {
        return Vec::new();
    };
    let (head, tail) = (&path[..i + 3], &path[i + 3..]);
    select(value, head)
        .into_iter()
        .filter_map(Value::as_object)
        .flat_map(|record| {
            at(&Value::Object(record.clone()), &format!("${tail}"))
                .into_iter()
                .map(move |v| (v, record))
        })
        .collect()
}

/// Bind a symbol again: the ticket's first match of its shape, or the most
/// recent result of its tool with a value at its path, the first in a list
/// not yet passed as this argument, preferring the record like the one the
/// demonstrator chose; for a field of one record, the record that holds a
/// value the ticket gives before the most recent. Anything else is itself.
fn resolve(
    v: &Value,
    outputs: &[(String, Value)],
    ticket: &str,
    passed: &BTreeSet<String>,
) -> Option<Value> {
    let Some(sym) = v.as_str().and_then(|s| s.strip_prefix('@')) else {
        return Some(v.clone());
    };
    if let Some(pattern) = sym.strip_prefix("ticket") {
        return find_shape(pattern, ticket).map(Value::String);
    }
    let dollar = sym.find('$')?;
    let (tool, path) = (&sym[..dollar], &sym[dollar..]);
    let (path, apart) = match path.split_once('?') {
        Some((p, a)) => (p, serde_json::from_str::<Map<String, Value>>(a).ok()?),
        None => (path, Map::new()),
    };
    let listed = path.contains("[*]");
    let mut found: Vec<(&Value, String)> = Vec::new();
    for (name, value) in outputs.iter().rev() {
        if name != tool {
            continue;
        }
        let mut values: Vec<String> = at(value, path)
            .into_iter()
            .filter(|x| !(listed && passed.contains(x)))
            .collect();
        if !apart.is_empty() {
            let like: Vec<String> = members(value, path)
                .into_iter()
                .filter(|(x, record)| {
                    !passed.contains(x) && apart.iter().all(|(k, want)| record.get(k) == Some(want))
                })
                .map(|(x, _)| x)
                .collect();
            values.retain(|x| !like.contains(x));
            values = like.into_iter().chain(values).collect();
        }
        if let Some(first) = values.into_iter().next() {
            found.push((value, first));
        }
    }
    if !listed {
        let given = given(ticket);
        for (value, v) in &found {
            let mut leaves = Vec::new();
            scalar_leaves(value, &mut leaves);
            if leaves.iter().any(|l| given.contains(l)) {
                return Some(Value::String(v.clone()));
            }
        }
    }
    found.into_iter().next().map(|(_, v)| Value::String(v))
}

/// The first text in `ticket` of a shape: literal characters (`\` before
/// one escapes it) and `\d{N}`, N digits.
fn find_shape(pattern: &str, ticket: &str) -> Option<String> {
    enum Token {
        Digits(usize),
        Char(char),
    }
    let mut tokens = Vec::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            tokens.push(Token::Char(c));
            continue;
        }
        match chars.next() {
            Some('d') => {
                let n = if chars.peek() == Some(&'{') {
                    chars.next();
                    let digits: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    digits.parse().ok()?
                } else {
                    1
                };
                tokens.push(Token::Digits(n));
            }
            Some(other) => tokens.push(Token::Char(other)),
            None => return None,
        }
    }
    let text: Vec<char> = ticket.chars().collect();
    'start: for start in 0..=text.len() {
        let mut i = start;
        for t in &tokens {
            match t {
                Token::Char(c) => {
                    if text.get(i) != Some(c) {
                        continue 'start;
                    }
                    i += 1;
                }
                Token::Digits(n) => {
                    for _ in 0..*n {
                        if !text.get(i).is_some_and(|&c| digit(c)) {
                            continue 'start;
                        }
                        i += 1;
                    }
                }
            }
        }
        return Some(text[start..i].iter().collect());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn outs(items: &[(&str, Value)]) -> Vec<(String, Value)> {
        items
            .iter()
            .map(|(t, v)| (t.to_string(), v.clone()))
            .collect()
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn symbols_bind_again_as_the_workflow_binds_them() {
        // The cases of `scripts/telecom_workflow.py`'s own `resolve`.
        let o = outs(&[
            (
                "get_customer_by_phone",
                json!({"customer_id": "C7", "line_ids": ["L7", "L8"]}),
            ),
            (
                "get_details_by_id",
                json!({"line_id": "L7", "phone_number": "555-0107"}),
            ),
            (
                "get_details_by_id",
                json!({"line_id": "L8", "phone_number": "555-0108"}),
            ),
            (
                "get_bills_for_customer",
                json!([{"bill_id": "B1", "status": "Paid"}, {"bill_id": "B2", "status": "Overdue"}]),
            ),
        ]);
        let r = |s: &str, ticket: &str, passed: &[&str]| {
            resolve(&json!(s), &o, ticket, &set(passed)).map(|v| v.as_str().unwrap().to_string())
        };
        assert_eq!(
            r("@get_customer_by_phone$.line_ids[*]", "", &["L7"]).unwrap(),
            "L8"
        );
        assert_eq!(r("@get_details_by_id$.line_id", "", &[]).unwrap(), "L8");
        assert_eq!(
            r("@get_details_by_id$.line_id", "my number is 555-0107", &[]).unwrap(),
            "L7"
        );
        assert_eq!(
            r(
                r#"@get_bills_for_customer$[*].bill_id?{"status": "Overdue"}"#,
                "",
                &[]
            )
            .unwrap(),
            "B2"
        );
        assert_eq!(
            r(r"@ticket\d{3}\-\d{4}", "call me at 555-0199", &[]).unwrap(),
            "555-0199"
        );
        assert_eq!(r("@get_payment_methods$.id", "", &[]), None);
        // Anything that is not a symbol is itself.
        assert_eq!(resolve(&json!(4), &o, "", &set(&[])), Some(json!(4)));
    }

    #[test]
    fn features_read_records_and_a_phones_checks() {
        let rec = result_features(
            false,
            &json!({"status": "Active", "line_id": "L1002", "roaming": false, "data_gb": 8.7,
                    "items": [], "devices": {"a": {"status": "On"}}, "phone": "555-123-2002",
                    "count": 3}),
        );
        let names: BTreeSet<&str> = rec.keys().map(String::as_str).collect();
        assert!(names.contains("status=active") && names.contains("roaming=false"));
        assert!(names.contains("count=3"));
        // Ids (three digits in a row) say nothing across episodes.
        assert!(!names.iter().any(|n| n.starts_with("line_id")));
        assert!(names.contains("devices.*.status=on"));
        assert!(!names
            .iter()
            .any(|n| n.starts_with("phone=") || n.starts_with("data_gb")));
        assert_eq!(rec["len(items)"], Feature::Is("0".into()));
        assert_eq!(rec["error"], Feature::No);
        let text = text_features(
            "Airplane Mode: OFF | SIM Card Status: active\nStatus Bar: 📶⁴ Excellent | 5G | 🔋 80%\nNo SIM card detected",
        );
        let names: BTreeSet<&str> = text.keys().map(String::as_str).collect();
        assert!(names.contains("airplane mode=off"), "{names:?}");
        // A superscript is not a digit to the workflow, as to Python's `\d`.
        assert!(names.contains("status bar=📶⁴ excellent"), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("5g") || n.contains("80%")));
        assert!(names.contains("says no sim card detected"));
        assert_eq!(
            given("Customer: John, phone number: 555-123-2002."),
            set(&["555-123-2002."])
        );
        assert!(words("Abroad in France").contains("abroad in"));
    }

    struct Phone {
        airplane: bool,
        log: Vec<String>,
    }

    impl Tools for Phone {
        fn call(&mut self, name: &str, arguments: &Map<String, Value>) -> Result<(String, bool)> {
            self.log.push(label(name, arguments));
            Ok(match name {
                "get_customer_by_phone" => (
                    json!({"customer_id": "C9", "line_ids": ["L9"]}).to_string(),
                    false,
                ),
                "check_status_bar" if self.airplane => {
                    ("Status Bar: ✈️ Airplane Mode".into(), false)
                }
                "check_status_bar" => ("Status Bar: 📶⁴ Excellent".into(), false),
                "toggle_airplane_mode" => {
                    self.airplane = !self.airplane;
                    ("Airplane Mode is now OFF.".into(), false)
                }
                "resume_line" => ("ok".into(), false),
                _ => ("no such tool".into(), true),
            })
        }
    }

    fn procedure() -> Procedure {
        let leaf = |calls: &[(&str, u64)]| Node::Leaf {
            calls: calls.iter().map(|(a, k)| (a.to_string(), *k)).collect(),
        };
        Procedure {
            stretto_procedure: PROCEDURE_VERSION,
            domain: "telecom".into(),
            provenance: Value::Null,
            symbolic: true,
            max_calls: 10,
            stop: "stop".into(),
            handoff: "transfer_to_human_agents".into(),
            handoff_arguments: Map::new(),
            reads: set(&["get_customer_by_phone", "check_status_bar"]),
            vocab: set(&["no service"]),
            trees: BTreeMap::from([
                (
                    "start".to_string(),
                    leaf(&[(
                        r#"get_customer_by_phone{"phone_number": "@ticket\\d{3}\\-\\d{4}"}"#,
                        9,
                    )]),
                ),
                (
                    "get_customer_by_phone".to_string(),
                    leaf(&[("check_status_bar", 9)]),
                ),
                (
                    "check_status_bar".to_string(),
                    Node::Split {
                        feature: "check_status_bar:status bar=✈️ airplane mode".into(),
                        yes: Box::new(leaf(&[("toggle_airplane_mode", 8), ("stop", 1)])),
                        no: Box::new(leaf(&[
                            // A lookup made since the last write is skipped.
                            ("check_status_bar", 5),
                            (
                                r#"resume_line{"line_id": "@get_customer_by_phone$.line_ids[*]"}"#,
                                4,
                            ),
                            ("stop", 3),
                        ])),
                    },
                ),
                (
                    "toggle_airplane_mode".to_string(),
                    leaf(&[("check_status_bar", 9)]),
                ),
                ("resume_line".to_string(), leaf(&[("stop", 9)])),
            ]),
            fallback: leaf(&[("stop", 1)]),
            checks: vec![Check {
                phrase: "status bar shows that they have signal".into(),
                probe: "check_status_bar".into(),
                contains: vec!["📶".into()],
                lacks: vec!["no signal".into(), "airplane mode".into()],
            }],
        }
    }

    #[test]
    fn a_run_binds_guards_and_checks_its_outcome() {
        let p = procedure();
        let ticket = "No service; my number is 555-0199. They will consider the issue resolved \
                      when the status bar shows that they have signal.";
        let mut phone = Phone {
            airplane: true,
            log: Vec::new(),
        };
        let run = p.run(ticket, &mut phone).unwrap();
        let tools: Vec<&str> = run.calls.iter().map(|c| c.tool.as_str()).collect();
        assert_eq!(
            tools,
            [
                "get_customer_by_phone",
                "check_status_bar",
                "toggle_airplane_mode",
                "check_status_bar",
                "resume_line"
            ]
        );
        assert_eq!(run.calls[0].arguments["phone_number"], json!("555-0199"));
        assert_eq!(run.calls[4].arguments["line_id"], json!("L9"));
        assert_eq!(run.verdict, Verdict::Resolved);
        assert_eq!(phone.log.last().unwrap(), "check_status_bar");
        // With the phone left in airplane mode, the check fails and the
        // ticket goes back.
        let mut stuck = p.clone();
        stuck.trees.insert(
            "check_status_bar".into(),
            Node::Leaf {
                calls: vec![("stop".into(), 1)],
            },
        );
        let run = stuck
            .run(
                ticket,
                &mut Phone {
                    airplane: true,
                    log: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(run.verdict, Verdict::HandBack);
        // An identifier the ticket does not give cannot bind: the run stops.
        let run = p
            .run(
                "No service.",
                &mut Phone {
                    airplane: true,
                    log: Vec::new(),
                },
            )
            .unwrap();
        assert!(run.calls.is_empty() && run.verdict == Verdict::HandBack);
    }

    #[test]
    fn a_hand_off_ends_the_run_and_a_bad_action_stops_it() {
        let mut p = procedure();
        let start = |action: &str| Node::Leaf {
            calls: vec![(action.to_string(), 1)],
        };
        p.trees
            .insert("start".into(), start("transfer_to_human_agents"));
        let mut phone = Phone {
            airplane: false,
            log: Vec::new(),
        };
        let run = p.run("No service.", &mut phone).unwrap();
        assert_eq!(run.verdict, Verdict::Transferred);
        assert!(run.check.is_none() && run.calls[0].failed);
        p.trees
            .insert("start".into(), start("check_status_bar{oops"));
        let e = p.run("No service.", &mut phone).unwrap_err().to_string();
        assert!(e.starts_with("action check_status_bar{oops: "), "{e}");
    }

    /// The record the ticket names stays in the state when another of its
    /// kind is read after it, unless the procedure holds no symbols.
    #[test]
    fn the_record_the_ticket_names_is_kept() {
        let mut p = procedure();
        let line = |id: &str, status: &str| json!({"line_id": id, "status": status}).to_string();
        for symbolic in [true, false] {
            p.symbolic = symbolic;
            let mut state = State::new(&p, "Line L7001 has no service.");
            state.result("get_details_by_id", &line("L7001", "Active"), false);
            state.result("get_details_by_id", &line("L7002", "Suspended"), false);
            let kept = state.features().contains("get_details_by_id:status=active");
            assert_eq!(kept, symbolic);
            state.result("get_details_by_id", "", true);
            assert_eq!(state.site, "get_details_by_id!");
        }
    }

    #[test]
    fn results_are_read_whatever_their_shape() {
        assert_eq!(parse(r#""{\"a\": 1}""#), json!({"a": 1}));
        assert_eq!(parse(r#""plain""#), json!("plain"));
        assert_eq!(parse("not json"), json!("not json"));
        let two = result_features(false, &json!({"items": [1, 2]}));
        assert_eq!(two["len(items)"], Feature::Is("2+".into()));
        let list = result_features(false, &json!([]));
        assert_eq!(list["len($)"], Feature::Is("0".into()));
        assert_eq!(result_features(false, &json!(7)).len(), 1);
        let text = text_features("Signal: good\n\n  \nNo SIM");
        assert_eq!(text.len(), 2);
        let mut leaves = Vec::new();
        scalar_leaves(&json!([15.0, 3, 2.5, true, null, "x"]), &mut leaves);
        assert_eq!(leaves, ["15.0", "3", "2.5", "x"]);
    }

    #[test]
    fn paths_and_shapes_find_only_what_is_there() {
        assert!(steps("$x").is_empty());
        assert!(select(&json!({"a": 1}), "$[*]").is_empty());
        assert!(select(&json!([1]), "$.a").is_empty());
        assert!(members(&json!({"a": "b"}), "$.a").is_empty());
        assert_eq!(find_shape(r"A\d", "xA1").as_deref(), Some("A1"));
        assert_eq!(find_shape("\\", "x"), None);
    }

    #[test]
    fn a_procedure_is_read_only_from_a_procedure_file() {
        let e = Procedure::from_json("{}").unwrap_err().to_string();
        assert!(e.starts_with("not a stretto procedure"), "{e}");
        let missing = std::path::Path::new("/nonexistent/p.json");
        let e = Procedure::load(missing).unwrap_err().to_string();
        assert!(e.starts_with("reading /nonexistent/p.json"), "{e}");
    }

    #[test]
    fn a_procedure_survives_its_ir_and_names_its_version() {
        let p = procedure();
        let text = serde_json::to_string(&p).unwrap();
        let back = Procedure::from_json(&text).unwrap();
        assert_eq!(back.trees.len(), p.trees.len());
        let mut v: Value = serde_json::from_str(&text).unwrap();
        v["stretto_procedure"] = json!(2);
        let e = Procedure::from_json(&v.to_string())
            .unwrap_err()
            .to_string();
        assert!(e.contains("format 2"), "{e}");
    }
}
