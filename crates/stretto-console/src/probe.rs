//! A live test of a registered server: `initialize`, then
//! `notifications/initialized`, then `tools/list`, as a host would start a
//! session.
//!
//! A stdio server is started from its command, spoken to in
//! newline-delimited JSON-RPC, and killed after. A Streamable HTTP server
//! (revision 2025-06-18) is POSTed each message, with the headers the
//! registry names filled from the console's environment, the session id it
//! assigns and the protocol version it agreed; its answer may be one JSON
//! body or an event stream. The session is ended with a DELETE. The whole
//! exchange has [`TIMEOUT`]. Header values are sent, never returned or
//! logged.

use crate::api::servers::ProbeResult;
use crate::api::sessions::ToolInfo;
use crate::data::registry::{HeaderRef, Upstream};
use crate::data::sessions::tool_info;
use crate::VERSION;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};

/// How long the whole exchange may take.
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// The protocol version the probe offers.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Pages of `tools/list` read at most.
const MAX_PAGES: usize = 20;

/// Ask the server behind `upstream` who it is and what tools it lists.
pub async fn probe(upstream: &Upstream) -> ProbeResult {
    let started = Instant::now();
    let answered = tokio::time::timeout(TIMEOUT, async {
        match upstream {
            Upstream::Stdio { command, .. } => stdio(command).await,
            Upstream::Http { url, headers } => http(url, headers).await,
        }
    })
    .await;
    let mut result = match answered {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => ProbeResult {
            error: Some(error),
            ..ProbeResult::default()
        },
        Err(_) => ProbeResult {
            error: Some(format!(
                "the server did not answer within {} s",
                TIMEOUT.as_secs()
            )),
            ..ProbeResult::default()
        },
    };
    result.ms = started.elapsed().as_millis() as u64;
    result
}

/// What `initialize` and the tools say.
fn result_of(init: &Value, tools: Vec<Value>) -> ProbeResult {
    let text = |p: &str| init.pointer(p).and_then(Value::as_str).map(str::to_string);
    ProbeResult {
        ok: true,
        ms: 0,
        error: None,
        server_name: text("/serverInfo/name"),
        server_version: text("/serverInfo/version"),
        protocol_version: text("/protocolVersion"),
        instructions: text("/instructions"),
        tools: {
            let mut tools: Vec<ToolInfo> = tools.iter().map(tool_info).collect();
            tools.sort_by(|a, b| a.name.cmp(&b.name));
            tools
        },
        flow_warnings: Vec::new(),
    }
}

fn initialize(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "initialize", "params": {
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {"name": "stretto-console", "version": VERSION}
    }})
}

fn initialized() -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
}

fn tools_list(id: u64, cursor: Option<&str>) -> Value {
    match cursor {
        Some(c) => {
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {"cursor": c}})
        }
        None => json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}}),
    }
}

/// A response's result, or its error as text.
fn answer(message: &Value, method: &str) -> Result<Value, String> {
    if let Some(e) = message.get("error").filter(|e| !e.is_null()) {
        return Err(format!(
            "the server answered {method} with an error: {}",
            e.get("message")
                .and_then(Value::as_str)
                .unwrap_or("(no message)")
        ));
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// Whether `message` is the response to request `id`.
fn answers(message: &Value, id: u64) -> bool {
    message.get("id") == Some(&json!(id))
        && message.get("method").is_none()
        && (message.get("result").is_some() || message.get("error").is_some())
}

// ---- stdio ------------------------------------------------------------------

async fn stdio(command: &[String]) -> Result<ProbeResult, String> {
    let Some((program, args)) = command.split_first() else {
        return Err("the server's command is empty".to_string());
    };
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("starting {program}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("no stdout")?).lines();
    let exchange = async {
        send(&mut stdin, &initialize(1)).await?;
        let init = receive(&mut stdin, &mut lines, 1, "initialize").await?;
        send(&mut stdin, &initialized()).await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for page in 0..MAX_PAGES {
            let id = 2 + page as u64;
            send(&mut stdin, &tools_list(id, cursor.as_deref())).await?;
            let listed = receive(&mut stdin, &mut lines, id, "tools/list").await?;
            tools.extend(
                listed
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            cursor = listed
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok::<ProbeResult, String>(result_of(&init, tools))
    };
    let result = exchange.await;
    // Closing its input asks the server to exit; it is killed either way.
    drop(stdin);
    let _ = child.kill().await;
    match result {
        Err(e) if e.contains("closed its output") => match child.try_wait() {
            Ok(Some(status)) if !status.success() => Err(format!("{e} (it exited with {status})")),
            _ => Err(e),
        },
        other => other,
    }
}

async fn send(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    let mut line = message.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("writing to the server: {e}"))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("writing to the server: {e}"))
}

/// The server's answer to request `id`. Lines that are not JSON are
/// skipped, and requests of the server's own are refused, as a client that
/// offers no capabilities would.
async fn receive(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
    id: u64,
    method: &str,
) -> Result<Value, String> {
    loop {
        let line = lines
            .next_line()
            .await
            .map_err(|e| format!("reading from the server: {e}"))?
            .ok_or_else(|| format!("the server closed its output before answering {method}"))?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if answers(&message, id) {
            return answer(&message, method);
        }
        if let (Some(_), Some(their)) = (message.get("method"), message.get("id")) {
            let refusal = json!({"jsonrpc": "2.0", "id": their,
                "error": {"code": -32601, "message": "Method not found"}});
            send(stdin, &refusal).await?;
        }
    }
}

// ---- Streamable HTTP --------------------------------------------------------

const SESSION: &str = "mcp-session-id";
const PROTOCOL: &str = "mcp-protocol-version";

async fn http(url: &str, headers: &[HeaderRef]) -> Result<ProbeResult, String> {
    let mut extra = reqwest::header::HeaderMap::new();
    for h in headers {
        let name = reqwest::header::HeaderName::from_bytes(h.name.as_bytes())
            .map_err(|_| format!("{:?} is not a header name", h.name))?;
        // The value is read to be sent, and goes nowhere else.
        let value = std::env::var(&h.env).map_err(|_| {
            format!(
                "{} is not set in the console's environment, so the {} header cannot be sent",
                h.env, h.name
            )
        })?;
        let mut value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|_| format!("{} does not hold a valid header value", h.env))?;
        value.set_sensitive(true);
        extra.insert(name, value);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| format!("building the HTTP client: {e}"))?;
    let mut session: Option<String> = None;
    let mut protocol: Option<String> = None;
    let exchange = async {
        let response = post(
            &client,
            url,
            &extra,
            &session,
            &protocol,
            &initialize(1),
            "initialize",
        )
        .await?;
        session = response
            .headers()
            .get(SESSION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let init = read_answer(response, 1, "initialize").await?;
        protocol = init
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string);
        post(
            &client,
            url,
            &extra,
            &session,
            &protocol,
            &initialized(),
            "notifications/initialized",
        )
        .await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for page in 0..MAX_PAGES {
            let id = 2 + page as u64;
            let response = post(
                &client,
                url,
                &extra,
                &session,
                &protocol,
                &tools_list(id, cursor.as_deref()),
                "tools/list",
            )
            .await?;
            let listed = read_answer(response, id, "tools/list").await?;
            tools.extend(
                listed
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            cursor = listed
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok::<ProbeResult, String>(result_of(&init, tools))
    };
    let result = exchange.await;
    if let Some(id) = &session {
        let mut end = client
            .delete(url)
            .headers(extra.clone())
            .header(SESSION, id);
        if let Some(p) = &protocol {
            end = end.header(PROTOCOL, p);
        }
        let _ = end.send().await;
    }
    result
}

async fn post(
    client: &reqwest::Client,
    url: &str,
    extra: &reqwest::header::HeaderMap,
    session: &Option<String>,
    protocol: &Option<String>,
    message: &Value,
    method: &str,
) -> Result<reqwest::Response, String> {
    let mut request = client
        .post(url)
        .headers(extra.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .body(message.to_string());
    if let Some(id) = session {
        request = request.header(SESSION, id);
    }
    if let Some(p) = protocol {
        request = request.header(PROTOCOL, p);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("sending {method}: {}", e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the server answered {method} with HTTP {status}"));
    }
    Ok(response)
}

/// The answer to request `id`: the JSON body, or the event of the stream
/// that carries it.
async fn read_answer(
    mut response: reqwest::Response,
    id: u64,
    method: &str,
) -> Result<Value, String> {
    let events = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("text/event-stream"));
    if !events {
        let message: Value = response
            .json()
            .await
            .map_err(|e| format!("reading the answer to {method}: {}", e.without_url()))?;
        let found = match &message {
            Value::Array(batch) => batch.iter().find(|m| answers(m, id)).cloned(),
            m if answers(m, id) => Some(m.clone()),
            _ => None,
        };
        return match found {
            Some(m) => answer(&m, method),
            None => Err(format!(
                "the server's answer to {method} is not its response"
            )),
        };
    }
    let mut buffer = String::new();
    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|e| format!("reading the answer to {method}: {}", e.without_url()))?
            .ok_or_else(|| {
                format!("the server's event stream ended before it answered {method}")
            })?;
        buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
        while let Some(end) = buffer.find("\n\n") {
            let event: String = buffer.drain(..end + 2).collect();
            let data: Vec<&str> = event
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            if let Ok(message) = serde_json::from_str::<Value>(&data.join("\n")) {
                if answers(&message, id) {
                    return answer(&message, method);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_command_that_does_not_start_or_does_not_speak_mcp() {
        let r = probe(&Upstream::Stdio {
            command: vec!["/nonexistent/stretto-no-such-server".into()],
            env: vec![],
        })
        .await;
        assert!(!r.ok);
        assert!(r.error.unwrap().starts_with("starting /nonexistent/"));
        let r = probe(&Upstream::Stdio {
            command: vec![],
            env: vec![],
        })
        .await;
        assert_eq!(r.error.as_deref(), Some("the server's command is empty"));
        #[cfg(unix)]
        {
            let r = probe(&Upstream::Stdio {
                command: vec!["sh".into(), "-c".into(), "echo not json; exit 3".into()],
                env: vec![],
            })
            .await;
            let e = r.error.unwrap();
            assert!(
                e.contains("closed its output before answering initialize"),
                "{e}"
            );
        }
    }

    #[tokio::test]
    async fn a_missing_header_variable_is_named_and_nothing_is_sent() {
        let r = probe(&Upstream::Http {
            url: "http://127.0.0.1:9/mcp".into(),
            headers: vec![HeaderRef {
                name: "Authorization".into(),
                env: "STRETTO_CONSOLE_TEST_UNSET_VARIABLE".into(),
            }],
        })
        .await;
        assert_eq!(
            r.error.as_deref(),
            Some(
                "STRETTO_CONSOLE_TEST_UNSET_VARIABLE is not set in the console's environment, \
                 so the Authorization header cannot be sent"
            )
        );
    }

    #[test]
    fn a_response_is_matched_by_id() {
        assert!(answers(
            &json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
            1
        ));
        assert!(!answers(
            &json!({"jsonrpc": "2.0", "id": 2, "result": {}}),
            1
        ));
        assert!(!answers(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
            1
        ));
        let e = answer(
            &json!({"id": 1, "error": {"code": 1, "message": "no"}}),
            "tools/list",
        );
        assert_eq!(
            e.unwrap_err(),
            "the server answered tools/list with an error: no"
        );
    }
}
