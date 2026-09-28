//! The probe against servers that do what a real one might: page their
//! tools, ask questions of their own, answer on event streams, or get
//! something wrong. Stdio servers are `sh` scripts; HTTP ones answer from a
//! thread on a local port.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use stretto_console::api::servers::ProbeResult;
use stretto_console::data::registry::{HeaderRef, Upstream};
use stretto_console::probe::probe_within;

const PATIENCE: Duration = Duration::from_secs(10);

fn no_variables(_: &str) -> Option<String> {
    None
}

async fn probe(upstream: Upstream) -> ProbeResult {
    probe_within(&upstream, PATIENCE, &no_variables).await
}

fn names(result: &ProbeResult) -> Vec<&str> {
    result.tools.iter().map(|t| t.name.as_str()).collect()
}

// ---- stdio ------------------------------------------------------------------

#[tokio::test]
async fn a_server_with_no_command_is_not_started() {
    let nothing = Upstream::Stdio {
        command: Vec::new(),
        env: Vec::new(),
    };
    let result = probe(nothing).await;
    assert_eq!(
        result.error.as_deref(),
        Some("the server's command is empty")
    );
}

#[cfg(unix)]
fn sh(script: &str) -> Upstream {
    Upstream::Stdio {
        command: vec!["sh".into(), "-c".into(), script.into()],
        env: Vec::new(),
    }
}

/// Before its answer to `initialize`, the server writes a line that is not
/// JSON, a request of its own (which the probe refuses) and an answer to
/// another request; then it lists its tools on two pages.
#[cfg(unix)]
#[tokio::test]
async fn a_stdio_server_that_asks_its_own_questions_and_pages_its_tools() {
    let result = probe(sh(r#"read -r l
echo 'not json'
echo '{"jsonrpc":"2.0","id":"s1","method":"roots/list"}'
echo '{"jsonrpc":"2.0","id":99,"result":{}}'
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"paged","version":"2"}}}'
read -r refusal
read -r l
read -r l
echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"b"}],"nextCursor":"more"}}'
read -r l
case "$l" in *'"cursor":"more"'*) ;; *) exit 1 ;; esac
case "$refusal" in *'"id":"s1"'*) ;; *) exit 1 ;; esac
case "$refusal" in *'Method not found'*) ;; *) exit 1 ;; esac
echo '{"jsonrpc":"2.0","id":3,"result":{"tools":[{"name":"a"}]}}'
read -r l"#))
    .await;
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(names(&result), ["a", "b"]);
    assert_eq!(result.server_name.as_deref(), Some("paged"));
}

#[cfg(unix)]
#[tokio::test]
async fn what_a_stdio_server_gets_wrong_is_said() {
    for (script, error) in [
        (
            "read -r l; exit 0",
            "the server closed its output before answering initialize",
        ),
        (
            "read -r l; exit 3",
            "the server closed its output before answering initialize (it exited with exit status: 3)",
        ),
        (
            r"read -r l; printf '\377\n'; read -r l",
            "reading from the server: stream did not contain valid UTF-8",
        ),
    ] {
        let result = probe(sh(script)).await;
        assert_eq!(result.error.as_deref(), Some(error), "{script}");
    }
    // One that never answers is given up on.
    let silent = sh("exec sleep 30");
    let result = probe_within(&silent, Duration::from_millis(300), &no_variables).await;
    assert_eq!(
        result.error.as_deref(),
        Some("the server did not answer within 0.3 s")
    );
}

// ---- Streamable HTTP --------------------------------------------------------

/// Read one request: its first line and its body.
fn read_request(stream: &mut TcpStream) -> (String, String) {
    let mut reader = BufReader::new(stream);
    let (mut first, mut length) = (String::new(), 0);
    reader.read_line(&mut first).unwrap();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
        }
        if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
            length = n.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (first, String::from_utf8(body).unwrap())
}

/// A server on a local port that answers each request as `reply` says,
/// given its first line and body; `None` holds the connection open with no
/// answer. Its URL.
fn server(reply: impl Fn(&str, &Value) -> Option<String> + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let reply = Arc::new(reply);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let (mut stream, reply) = (stream.unwrap(), reply.clone());
            std::thread::spawn(move || {
                let (first, body) = read_request(&mut stream);
                let message = serde_json::from_str(&body).unwrap_or(Value::Null);
                match reply(&first, &message) {
                    Some(answer) => stream.write_all(answer.as_bytes()).unwrap(),
                    None => std::thread::sleep(Duration::from_secs(5)),
                }
            });
        }
    });
    url
}

fn json(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
}

fn events(stream: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{stream}"
    )
}

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"remote","version":"3"},"instructions":"Ask away."}}"#;

fn http(url: String, headers: Vec<HeaderRef>) -> Upstream {
    Upstream::Http { url, headers }
}

/// `initialize` on an event stream, the first page of tools in a batch,
/// the second as one JSON body; then the session is ended.
#[tokio::test]
async fn an_http_server_that_answers_on_streams_and_in_batches() {
    let (ended, ended_rx) = std::sync::mpsc::channel();
    let url = server(move |first, message| {
        if first.starts_with("DELETE") {
            ended.send(first.to_string()).unwrap();
            return Some(json("200 OK", "", ""));
        }
        let id = &message["id"];
        Some(match message["method"].as_str().unwrap_or_default() {
            "initialize" => events(&format!(
                "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}}\n\n\
                 data: not json\n\nevent: message\ndata: {INIT}\n\n"
            ))
            .replace(
                "Connection: close",
                "Mcp-Session-Id: s-1\r\nConnection: close",
            ),
            "notifications/initialized" => json("202 Accepted", "", ""),
            "tools/list" if message["params"]["cursor"] == "p2" => json(
                "200 OK",
                "",
                &format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":"z"}}]}}}}"#),
            ),
            _ => json(
                "200 OK",
                "",
                &format!(
                    r#"[{{"jsonrpc":"2.0","method":"notifications/message"}},{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":"y"}}],"nextCursor":"p2"}}}}]"#
                ),
            ),
        })
    });
    let auth = |name: &str| (name == "REMOTE_AUTH").then(|| "Bearer t".to_string());
    let upstream = http(
        url,
        vec![HeaderRef {
            name: "Authorization".into(),
            env: "REMOTE_AUTH".into(),
        }],
    );
    let result = probe_within(&upstream, PATIENCE, &auth).await;
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(names(&result), ["y", "z"]);
    assert_eq!(result.instructions.as_deref(), Some("Ask away."));
    assert!(ended_rx.recv_timeout(PATIENCE).is_ok());
}

#[tokio::test]
async fn what_an_http_server_gets_wrong_is_said() {
    let header = |name: &str| HeaderRef {
        name: name.into(),
        env: "REMOTE_AUTH".into(),
    };
    let bad_value = |_: &str| Some("a\nb".to_string());
    let nothing = "http://127.0.0.1:9/mcp".to_string();
    let result = probe_within(
        &http(nothing.clone(), vec![header("Bad Header")]),
        PATIENCE,
        &no_variables,
    )
    .await;
    assert_eq!(
        result.error.as_deref(),
        Some("\"Bad Header\" is not a header name")
    );
    let result = probe_within(
        &http(nothing.clone(), vec![header("Authorization")]),
        PATIENCE,
        &bad_value,
    )
    .await;
    assert_eq!(
        result.error.as_deref(),
        Some("REMOTE_AUTH does not hold a valid header value")
    );
    let result = probe(http(nothing.clone(), vec![header("Authorization")])).await;
    assert_eq!(
        result.error.as_deref(),
        Some(
            "REMOTE_AUTH is not set in the console's environment, so the Authorization header \
             cannot be sent"
        )
    );
    // Nothing listens on port 9.
    let result = probe(http(nothing, Vec::new())).await;
    assert!(result
        .error
        .unwrap()
        .starts_with("sending initialize: error sending request"));

    for (answer, error) in [
        (
            json("200 OK", "", "not json"),
            "reading the answer to initialize: error decoding response body",
        ),
        (
            json("200 OK", "", r#"{"jsonrpc":"2.0","id":7,"result":{}}"#),
            "the server's answer to initialize is not its response",
        ),
        (
            events("data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}\n\n"),
            "the server's event stream ended before it answered initialize",
        ),
        // A stream cut short of the length it gave.
        (
            events("data: {}\n\n").replace(
                "Connection: close",
                "Content-Length: 1000\r\nConnection: close",
            ),
            "reading the answer to initialize: error decoding response body",
        ),
    ] {
        let url = server(move |_, _| Some(answer.clone()));
        let result = probe(http(url, Vec::new())).await;
        assert_eq!(result.error.as_deref(), Some(error));
    }
    // One that never answers is given up on.
    let url = server(|_, _| None);
    let result = probe_within(
        &http(url, Vec::new()),
        Duration::from_millis(300),
        &no_variables,
    )
    .await;
    assert_eq!(
        result.error.as_deref(),
        Some("the server did not answer within 0.3 s")
    );
}
