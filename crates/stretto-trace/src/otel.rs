//! OpenTelemetry GenAI spans: what an agent framework reports of its LLM
//! calls and tool executions, read from an OTLP JSON export.
//!
//! Many agent frameworks emit spans under OpenTelemetry's semantic
//! conventions for generative AI. A deployment that exports them can learn
//! a flow from them, with no proxy in front of its servers. [`read_otel`]
//! reads an export, as the OpenTelemetry Collector's file exporter writes
//! it (one `ExportTraceServiceRequest` in OTLP JSON, or one a line), and
//! makes an [`Episode`] of each trace:
//!
//! - **LLM turns.** Each inference span (`gen_ai.operation.name` `chat`,
//!   `generate_content` or `text_completion`) is one LLM call: an
//!   [`Event::Assistant`] turn, with its token usage
//!   (`gen_ai.usage.input_tokens` and `gen_ai.usage.output_tokens`).
//! - **Tool calls.** Each `execute_tool` span is a call of
//!   `gen_ai.tool.name`, with id `gen_ai.tool.call.id` (else the span's),
//!   and its result. A turn's calls are the tool spans that start after its
//!   inference span and before the next one; several are parallel calls of
//!   that turn. Their results follow in the order they ended. A tool span
//!   with no inference span before it starts a turn of its own, which the
//!   tool spans that overlap it join.
//! - **Content is opt-in** in the conventions: a call's arguments
//!   (`gen_ai.tool.call.arguments`) and result (`gen_ai.tool.call.result`),
//!   and the messages (`gen_ai.input.messages`, `gen_ai.output.messages`),
//!   from which the user's messages and the assistant's text come. A call
//!   whose arguments were not captured has `null` arguments, which no one
//!   can bind, so a flow learns which lookups follow which calls but leaves
//!   those lookups to the agent ([`OtelRun::content`]).
//! - **Errors.** A tool span whose status is an error, or which has
//!   `error.type`, is an error result.
//! - **What spans do not say.** Whether a tool only reads: take the kinds
//!   from a manifest. Whether the task succeeded: `reward` is `0.0` unless
//!   the caller sets it. Other spans, such as the agent's run, are skipped.
//!
//! The conventions are still in development, so the names this module reads
//! are pinned to one version of them, [`CONVENTIONS`].

use crate::{Episode, Event, ToolCall, TurnUsage};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The OpenTelemetry semantic conventions for generative AI whose names this
/// module reads: those of semantic-conventions 1.38.0, the first to define
/// a tool call's arguments and result. A flow learned from spans records it.
pub const CONVENTIONS: &str = "OpenTelemetry GenAI semantic conventions 1.38.0";

/// The inference operations, each one LLM call.
const INFERENCE: [&str; 3] = ["chat", "generate_content", "text_completion"];

/// The spans of an export, as episodes.
#[derive(Clone, Debug, Default)]
pub struct OtelRun {
    /// One episode per trace, in the order the traces started. Each
    /// episode's id is its trace's id.
    pub episodes: Vec<Episode>,
    /// How many tool calls there were, and how many of them carried their
    /// arguments and their results: `[calls, with arguments, with results]`.
    pub content: [usize; 3],
}

/// One span, as far as this module reads it.
struct Span {
    trace: String,
    id: String,
    name: String,
    start: u64,
    end: u64,
    error: Option<String>,
    attributes: BTreeMap<String, Value>,
}

impl Span {
    fn operation(&self) -> Option<&str> {
        self.attributes
            .get("gen_ai.operation.name")
            .and_then(Value::as_str)
    }

    fn number(&self, key: &str) -> Option<u64> {
        self.attributes.get(key).and_then(Value::as_u64)
    }
}

/// Read an OTLP JSON export of spans.
pub fn read_otel(path: &Path) -> Result<OtelRun> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_otel(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Parse the text of an OTLP JSON export: one export, or one a line.
pub fn parse_otel(text: &str) -> Result<OtelRun> {
    let exports: Vec<Value> = match serde_json::from_str(text) {
        Ok(one) => vec![one],
        Err(_) => text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(i, line)| {
                serde_json::from_str(line).with_context(|| format!("line {}: not JSON", i + 1))
            })
            .collect::<Result<_>>()?,
    };
    let mut traces: BTreeMap<String, Vec<Span>> = BTreeMap::new();
    for export in &exports {
        let resources = export
            .get("resourceSpans")
            .and_then(Value::as_array)
            .context("not an OTLP JSON export of spans: no resourceSpans")?;
        let spans = resources
            .iter()
            .flat_map(|r| items(r, "scopeSpans"))
            .flat_map(|s| items(s, "spans"));
        for span in spans {
            let span = span_of(span)?;
            traces.entry(span.trace.clone()).or_default().push(span);
        }
    }
    let mut run = OtelRun::default();
    let mut episodes: Vec<(u64, Episode)> = Vec::new();
    for (trace, mut spans) in traces {
        spans.sort_by_key(|s| (s.start, s.end));
        let started = spans[0].start;
        episodes.push((started, episode(&trace, &spans, &mut run.content)));
    }
    // Stable, so traces that start together keep their ids' order.
    episodes.sort_by_key(|(started, _)| *started);
    run.episodes = episodes.into_iter().map(|(_, ep)| ep).collect();
    Ok(run)
}

/// The array `value[key]`, or nothing.
fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// A span of the export.
fn span_of(span: &Value) -> Result<Span> {
    let text = |key: &str| span.get(key).and_then(Value::as_str).unwrap_or_default();
    let trace = text("traceId");
    anyhow::ensure!(!trace.is_empty(), "a span has no traceId");
    let attributes: BTreeMap<String, Value> = items(span, "attributes")
        .iter()
        .filter_map(|kv| {
            Some((
                kv.get("key")?.as_str()?.to_string(),
                any_value(&kv["value"]),
            ))
        })
        .collect();
    let status = &span["status"];
    let failed = matches!(&status["code"], Value::Number(n) if n.as_u64() == Some(2))
        || status["code"] == "STATUS_CODE_ERROR";
    let error = match (failed, attributes.get("error.type")) {
        (_, Some(kind)) => Some(status_message(status).unwrap_or_else(|| text_of(kind))),
        (true, None) => Some(status_message(status).unwrap_or_default()),
        (false, None) => None,
    };
    Ok(Span {
        trace: trace.to_string(),
        id: text("spanId").to_string(),
        name: text("name").to_string(),
        start: nanos(&span["startTimeUnixNano"]),
        end: nanos(&span["endTimeUnixNano"]),
        error,
        attributes,
    })
}

/// A status's message, when it has one.
fn status_message(status: &Value) -> Option<String> {
    status
        .get("message")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
}

/// A time in nanoseconds, written as a string or a number.
fn nanos(value: &Value) -> u64 {
    match value {
        Value::String(s) => s.parse().unwrap_or(0),
        other => other.as_u64().unwrap_or(0),
    }
}

/// An attribute's `AnyValue` as JSON.
fn any_value(value: &Value) -> Value {
    if let Some(s) = value.get("stringValue") {
        return s.clone();
    }
    if let Some(Value::String(i)) = value.get("intValue") {
        return i
            .parse::<i64>()
            .map_or_else(|_| Value::String(i.clone()), Value::from);
    }
    for key in ["boolValue", "intValue", "doubleValue", "bytesValue"] {
        if let Some(v) = value.get(key) {
            return v.clone();
        }
    }
    if let Some(array) = value.get("arrayValue") {
        return Value::Array(items(array, "values").iter().map(any_value).collect());
    }
    if let Some(list) = value.get("kvlistValue") {
        let fields = items(list, "values").iter().filter_map(|kv| {
            Some((
                kv.get("key")?.as_str()?.to_string(),
                any_value(&kv["value"]),
            ))
        });
        return Value::Object(fields.collect());
    }
    Value::Null
}

/// A value as text: a string as it is, anything else as JSON.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// An attribute that holds JSON, written as JSON text or structured.
fn json_attribute(span: &Span, key: &str) -> Option<Value> {
    match span.attributes.get(key)? {
        Value::String(s) => {
            Some(serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone())))
        }
        other => Some(other.clone()),
    }
}

/// The text parts of the messages in `messages` that have `role`, each
/// message's parts joined; messages with no text are left out.
fn texts(messages: Option<Value>, role: &str) -> Vec<String> {
    let messages = match messages {
        Some(Value::Array(messages)) => messages,
        _ => Vec::new(),
    };
    messages
        .iter()
        .filter(|m| m["role"] == role)
        .filter_map(|m| {
            let parts: Vec<&str> = items(m, "parts")
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["content"].as_str())
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        })
        .collect()
}

/// One LLM turn: the inference span, if there was one, and its tool spans.
struct Turn<'a> {
    inference: Option<&'a Span>,
    tools: Vec<&'a Span>,
}

/// The episode of one trace's `spans`, in the order they started; `content`
/// counts the tool calls and the content they carried.
fn episode(trace: &str, spans: &[Span], content: &mut [usize; 3]) -> Episode {
    let mut turns: Vec<Turn> = Vec::new();
    for span in spans {
        match span.operation() {
            Some(op) if INFERENCE.contains(&op) => turns.push(Turn {
                inference: Some(span),
                tools: Vec::new(),
            }),
            Some("execute_tool") => {
                // Without an inference span, overlapping tool spans are one
                // turn's parallel calls.
                let joins = turns.last().is_some_and(|t| {
                    t.inference.is_some() || t.tools.iter().any(|o| span.start < o.end)
                });
                if !joins {
                    turns.push(Turn {
                        inference: None,
                        tools: Vec::new(),
                    });
                }
                turns.last_mut().expect("a turn").tools.push(span);
            }
            _ => {}
        }
    }
    let mut events = Vec::new();
    let mut users = 0;
    let mut model = None;
    for turn in &turns {
        let (mut text, mut usage) = (None, None);
        if let Some(span) = turn.inference {
            let said = texts(json_attribute(span, "gen_ai.input.messages"), "user");
            for text in said.iter().skip(users) {
                events.push(Event::User { text: text.clone() });
            }
            users = users.max(said.len());
            let replied = texts(json_attribute(span, "gen_ai.output.messages"), "assistant");
            text = (!replied.is_empty()).then(|| replied.join("\n"));
            let tokens = (
                span.number("gen_ai.usage.input_tokens"),
                span.number("gen_ai.usage.output_tokens"),
            );
            if tokens != (None, None) {
                usage = Some(TurnUsage {
                    prompt_tokens: tokens.0.unwrap_or(0),
                    completion_tokens: tokens.1.unwrap_or(0),
                    cost: 0.0,
                });
            }
            model = model.or_else(|| {
                ["gen_ai.response.model", "gen_ai.request.model"]
                    .iter()
                    .find_map(|k| span.attributes.get(*k).and_then(Value::as_str))
            });
        }
        let calls: Vec<ToolCall> = turn
            .tools
            .iter()
            .map(|span| {
                let arguments = json_attribute(span, "gen_ai.tool.call.arguments");
                content[0] += 1;
                content[1] += usize::from(arguments.is_some());
                content[2] += usize::from(span.attributes.contains_key("gen_ai.tool.call.result"));
                ToolCall {
                    id: call_id(span),
                    name: tool_name(span),
                    arguments: arguments.unwrap_or(Value::Null),
                }
            })
            .collect();
        events.push(Event::Assistant { text, calls, usage });
        let mut ended = turn.tools.clone();
        ended.sort_by_key(|s| s.end);
        for span in ended {
            let result = span.attributes.get("gen_ai.tool.call.result").map(text_of);
            events.push(Event::ToolResult {
                call_id: call_id(span),
                name: tool_name(span),
                error: span.error.is_some(),
                content: result.or_else(|| span.error.clone()).unwrap_or_default(),
            });
        }
    }
    Episode {
        id: trace.to_string(),
        task_id: String::new(),
        trial: 0,
        domain: "otel".to_string(),
        agent_model: model.unwrap_or("unknown").to_string(),
        reward: 0.0,
        events,
    }
}

/// A tool span's call id: `gen_ai.tool.call.id`, else the span's own.
fn call_id(span: &Span) -> String {
    span.attributes
        .get("gen_ai.tool.call.id")
        .map_or_else(|| span.id.clone(), text_of)
}

/// A tool span's tool: `gen_ai.tool.name`, else the span's name after
/// `execute_tool `.
fn tool_name(span: &Span) -> String {
    match span.attributes.get("gen_ai.tool.name") {
        Some(name) => text_of(name),
        None => span
            .name
            .strip_prefix("execute_tool ")
            .unwrap_or(&span.name)
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Pydantic AI's spans for eight customers of the demo shop
    /// (`scripts/otel_fixture.py`).
    const PYDANTIC_AI: &str = include_str!("../../../docs/examples/pydantic-ai-shop.otlp.jsonl");

    /// Each event as a short string, as `mcp`'s tests write them.
    fn outline(ep: &Episode) -> Vec<String> {
        ep.events
            .iter()
            .map(|e| match e {
                Event::Assistant { calls, text, .. } => {
                    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
                    let said = if text.is_some() { " said" } else { "" };
                    format!("turn({}){said}", names.join(","))
                }
                Event::ToolResult { name, error, .. } => {
                    format!("result:{name}{}", if *error { " (error)" } else { "" })
                }
                Event::User { .. } => "user".to_string(),
            })
            .collect()
    }

    #[test]
    fn reads_a_frameworks_export() {
        let run = parse_otel(PYDANTIC_AI).unwrap();
        assert_eq!(run.episodes.len(), 8);
        assert_eq!(run.content, [40, 40, 40]);
        let ep = &run.episodes[0];
        assert_eq!(
            outline(ep),
            [
                "user",
                "turn(find_user_id_by_email)",
                "result:find_user_id_by_email",
                "turn(get_user_details)",
                "result:get_user_details",
                "turn(get_order_details,get_order_details)",
                "result:get_order_details",
                "result:get_order_details",
                "turn(cancel_pending_order) said",
                "result:cancel_pending_order",
                "turn() said"
            ]
        );
        assert_eq!(ep.agent_model, "scripted-shop-model");
        assert!(ep
            .turn_usage()
            .iter()
            .all(|u| u.is_some_and(|u| u.prompt_tokens > 0)));
        assert!(matches!(&ep.events[0],
            Event::User { text } if text.starts_with("Hi, I'm c")));
        // The first customer found, and their details read with the id.
        let calls: Vec<&ToolCall> = ep.tool_calls().collect();
        let user = calls[1].arguments["user_id"].as_str().unwrap();
        assert!(matches!(&ep.events[2],
            Event::ToolResult { content, .. } if content == user));
        // The traces in the order they ran, each its own session.
        let ids: std::collections::HashSet<&str> =
            run.episodes.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), 8);
    }

    fn string(s: &str) -> Value {
        json!({"stringValue": s})
    }

    fn attribute(key: &str, value: Value) -> Value {
        json!({"key": key, "value": value})
    }

    /// A span of trace `trace` from `start` to `end`, with `attributes`.
    fn span(
        trace: &str,
        id: &str,
        name: &str,
        (start, end): (u64, u64),
        attributes: Vec<Value>,
    ) -> Value {
        json!({"traceId": trace, "spanId": id, "name": name,
               "startTimeUnixNano": start.to_string(), "endTimeUnixNano": end,
               "attributes": attributes})
    }

    fn tool(trace: &str, id: &str, name: &str, times: (u64, u64)) -> Value {
        span(
            trace,
            id,
            &format!("execute_tool {name}"),
            times,
            vec![
                attribute("gen_ai.operation.name", string("execute_tool")),
                attribute("gen_ai.tool.name", string(name)),
            ],
        )
    }

    fn export(spans: Vec<Value>) -> String {
        json!({"resourceSpans": [{"resource": {}},
               {"scopeSpans": [{"scope": {"name": "test"}}, {"spans": spans}]}]})
        .to_string()
    }

    #[test]
    fn every_value_of_an_attribute_is_read() {
        let values = [
            (json!({"intValue": "12"}), json!(12)),
            (json!({"intValue": "twelve"}), json!("twelve")),
            (json!({"intValue": 7}), json!(7)),
            (json!({"boolValue": true}), json!(true)),
            (json!({"doubleValue": 0.5}), json!(0.5)),
            (json!({"bytesValue": "AAE="}), json!("AAE=")),
            (
                json!({"arrayValue": {"values": [{"stringValue": "a"}]}}),
                json!(["a"]),
            ),
            (json!({"arrayValue": {}}), json!([])),
            (
                json!({"kvlistValue": {"values": [{"key": "k", "value": {"intValue": "1"}},
                                                   {"value": {}}]}}),
                json!({"k": 1}),
            ),
            (json!({}), Value::Null),
        ];
        for (any, read) in values {
            assert_eq!(any_value(&any), read, "{any}");
        }
        assert_eq!(nanos(&json!("x")), 0);
        assert_eq!(text_of(&json!(3)), "3");
    }

    #[test]
    fn what_a_trace_leaves_out_is_left_out() {
        let chat = |id: &str, times: (u64, u64), mut more: Vec<Value>| {
            more.push(attribute("gen_ai.operation.name", string("chat")));
            span("t2", id, "chat", times, more)
        };
        let messages = |said: &[&str]| {
            let m: Vec<Value> = said
                .iter()
                .map(|s| json!({"role": "user", "parts": [{"type": "text", "content": s}]}))
                .chain([json!({"role": "user", "parts": [{"type": "tool_call_response"}]})])
                .collect();
            string(&Value::Array(m).to_string())
        };
        let text = export(vec![
            // A trace of tool spans alone: two at once, then a third after.
            tool("t1", "a", "lookup", (100, 200)),
            tool("t1", "b", "lookup", (150, 250)),
            {
                let mut s = tool("t1", "c", "lookup", (300, 400));
                s["attributes"] =
                    json!([attribute("gen_ai.operation.name", string("execute_tool"))]);
                s["status"] = json!({"code": 2, "message": "boom"});
                s
            },
            // A conversation: two user messages, turns with and without usage.
            span(
                "t2",
                "agent",
                "invoke_agent shop",
                (0, 999),
                vec![attribute("gen_ai.operation.name", string("invoke_agent"))],
            ),
            span("t2", "plain", "no operation", (1, 2), Vec::new()),
            chat(
                "c1",
                (10, 20),
                vec![
                    attribute("gen_ai.input.messages", messages(&["one"])),
                    attribute("gen_ai.usage.input_tokens", json!({"intValue": "5"})),
                    attribute("gen_ai.request.model", string("m-requested")),
                ],
            ),
            {
                let mut s = tool("t2", "d", "search", (30, 40));
                s["attributes"].as_array_mut().unwrap().extend([
                    attribute("gen_ai.tool.call.id", string("call-d")),
                    attribute(
                        "gen_ai.tool.call.arguments",
                        json!({"kvlistValue": {"values": [
                                  {"key": "q", "value": {"stringValue": "x"}}]}}),
                    ),
                    attribute("error.type", string("ToolError")),
                ]);
                s["name"] = json!("search");
                s
            },
            {
                let mut s = tool("t2", "e", "fetch", (31, 41));
                s["attributes"].as_array_mut().unwrap().extend([
                    attribute("gen_ai.tool.call.arguments", string("not json")),
                    attribute("gen_ai.tool.call.result", json!({"intValue": "3"})),
                    attribute("error.type", string("Timeout")),
                ]);
                s["status"] = json!({"code": "STATUS_CODE_ERROR", "message": "timed out"});
                s
            },
            chat(
                "c2",
                (50, 60),
                vec![
                attribute("gen_ai.input.messages", messages(&["one", "two"])),
                attribute("gen_ai.output.messages", string(&json!([
                    {"role": "assistant", "parts": [{"type": "text", "content": "Hello"},
                                                     {"type": "text", "content": "again"}]}
                ]).to_string())),
            ],
            ),
            chat(
                "c3",
                (70, 80),
                vec![
                    attribute("gen_ai.output.messages", json!({"stringValue": "{}"})),
                    attribute("gen_ai.usage.output_tokens", json!({"intValue": 9})),
                ],
            ),
        ]);
        let run = parse_otel(&text).unwrap();
        assert_eq!(run.content, [5, 2, 1]);
        assert_eq!(run.episodes.len(), 2);
        // The conversation began first.
        let (second, first) = (&run.episodes[0], &run.episodes[1]);
        assert_eq!(second.id, "t2");
        assert_eq!(
            outline(first),
            [
                "turn(lookup,lookup)",
                "result:lookup",
                "result:lookup",
                "turn(lookup)",
                "result:lookup (error)"
            ]
        );
        assert_eq!(first.agent_model, "unknown");
        assert_eq!(first.tool_calls().next().unwrap().arguments, Value::Null);
        assert_eq!(first.tool_calls().nth(2).unwrap().id, "c");
        assert!(matches!(&first.events[4], Event::ToolResult { content, .. } if content == "boom"));
        assert_eq!(
            outline(second),
            [
                "user",
                "turn(search,fetch)",
                "result:search (error)",
                "result:fetch (error)",
                "user",
                "turn() said",
                "turn()"
            ]
        );
        assert_eq!(second.agent_model, "m-requested");
        let calls: Vec<&ToolCall> = second.tool_calls().collect();
        assert_eq!(
            (calls[0].id.as_str(), &calls[0].arguments),
            ("call-d", &json!({"q": "x"}))
        );
        assert_eq!(calls[1].arguments, json!("not json"));
        let results: Vec<&str> = second
            .events
            .iter()
            .filter_map(|e| match e {
                Event::ToolResult { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(results, ["ToolError", "3"]);
        let usage = second.turn_usage();
        assert_eq!(
            usage
                .iter()
                .map(|u| u.map(|u| (u.prompt_tokens, u.completion_tokens)))
                .collect::<Vec<_>>(),
            [Some((5, 0)), None, Some((0, 9))]
        );
        assert!(
            matches!(&second.events[5], Event::Assistant { text: Some(t), .. } if t == "Hello\nagain")
        );
    }

    #[test]
    fn what_is_not_an_export_says_so() {
        let error = |text: &str| format!("{:#}", parse_otel(text).unwrap_err());
        assert!(error("{\"x\": 1}").contains("no resourceSpans"));
        assert!(error("{}\nnot json\n").contains("line 2: not JSON"));
        let untraced = export(vec![json!({"spanId": "a"})]);
        assert!(error(&untraced).contains("no traceId"));
        let missing = std::env::temp_dir().join("stretto-otel-missing.jsonl");
        assert!(format!("{:#}", read_otel(&missing).unwrap_err()).starts_with("reading"));
        let path = std::env::temp_dir().join(format!("stretto-otel-{}.jsonl", std::process::id()));
        std::fs::write(&path, "not json\n").unwrap();
        let unread = format!("{:#}", read_otel(&path).unwrap_err());
        std::fs::remove_file(&path).unwrap();
        assert!(
            unread.starts_with("parsing") && unread.contains("line 1: not JSON"),
            "{unread}"
        );
        // One export a line, with blank lines between.
        let two = format!(
            "{}\n\n{}\n",
            export(Vec::new()),
            export(vec![tool("t", "a", "x", (1, 2))])
        );
        assert_eq!(parse_otel(&two).unwrap().episodes.len(), 1);
    }
}
