//! The demo server in this process: over stdio, over Streamable HTTP, and
//! its shop.

use super::*;
use std::io::{BufReader, Cursor, Read};
use std::net::{Shutdown, TcpStream};

/// Run the server with `args` on `input`: its exit code and output lines.
fn serve(args: &[&str], input: &str) -> (ExitCode, Vec<String>) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut output = Vec::new();
    let code = run(&args, &mut Cursor::new(input.as_bytes()), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    (code, text.lines().map(str::to_string).collect())
}

/// What a server with `args` answers `requests` with, one line each.
fn answers(args: &[&str], requests: &[Value]) -> Vec<Value> {
    let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
    let (code, lines) = serve(args, &input);
    assert_eq!(code, ExitCode::SUCCESS);
    lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
}

/// A tool result's text, and whether it is an error.
fn text(answer: &Value) -> (&str, bool) {
    let result = &answer["result"];
    (
        result["content"][0]["text"].as_str().unwrap_or_default(),
        result["isError"] == true,
    )
}

/// A tool result's text, as JSON.
fn parsed(answer: &Value) -> Value {
    serde_json::from_str(text(answer).0).unwrap()
}

#[test]
fn answers_each_kind_of_line_over_stdio() {
    let lines = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string(),
        json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
               "params": {"protocolVersion": "2025-03-26"}})
        .to_string(),
        "   ".to_string(),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        json!({"jsonrpc": "2.0", "id": 3, "result": {}}).to_string(),
        json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}).to_string(),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}).to_string(),
        call(6, "lookup", json!({"text": "hi", "delay_ms": 1})).to_string(),
        call(7, "update", json!({"fail": true})).to_string(),
        json!({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {"name": "lookup"}})
            .to_string(),
        call(9, "delete", json!({})).to_string(),
        json!({"jsonrpc": "2.0", "id": 10, "method": "resources/list"}).to_string(),
        "[1, 2]".to_string(),
        "not json".to_string(),
    ];
    let (code, out) = serve(&["--world", "echo"], &(lines.join("\n") + "\n"));
    assert_eq!(code, ExitCode::SUCCESS);
    let answers: Vec<Value> = out
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // The blank line, the notification and the response get no answer.
    assert_eq!(answers.len(), 11);
    assert_eq!(
        answers[0]["result"]["protocolVersion"],
        DEFAULT_PROTOCOL_VERSION
    );
    assert_eq!(answers[1]["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(answers[2], json!({"jsonrpc": "2.0", "id": 4, "result": {}}));
    let tools: Vec<(&str, bool)> = answers[3]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let hint = t["annotations"]["readOnlyHint"].as_bool().unwrap();
            (t["name"].as_str().unwrap(), hint)
        })
        .collect();
    assert_eq!(tools, [("lookup", true), ("update", false)]);
    assert_eq!(parsed(&answers[4]), json!({"text": "hi", "delay_ms": 1}));
    assert!(!text(&answers[4]).1);
    assert_eq!(
        text(&answers[5]),
        (r#"update failed, as asked: {"fail":true}"#, true)
    );
    assert_eq!(text(&answers[6]), ("{}", false));
    assert_eq!(answers[7]["error"]["message"], "Unknown tool: delete");
    assert_eq!(
        answers[8]["error"]["message"],
        "Method not found: resources/list"
    );
    assert_eq!(
        (&answers[9]["error"]["code"], &answers[9]["id"]),
        (&json!(-32600), &Value::Null)
    );
    assert_eq!(answers[10]["error"]["code"], -32700);
}

#[test]
fn says_its_usage_and_version_and_refuses_what_it_does_not_know() {
    let (code, lines) = serve(&["--help"], "");
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(lines.last().map(String::as_str), Some(USAGE));
    let (code, lines) = serve(&["-V"], "");
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        lines,
        [format!("stretto-mcp-demo {}", env!("CARGO_PKG_VERSION"))]
    );
    for args in [
        &["--world", "mars"][..],
        &["--hint", "lookup=maybe"],
        &["--hint", "lookup"],
        &["--port"],
        &["-h", "extra"],
    ] {
        let (code, lines) = serve(args, "");
        assert_eq!((code, lines.len()), (ExitCode::from(2), 0), "{args:?}");
    }
}

#[test]
fn the_shop_answers_from_its_canned_data() {
    let answers = answers(
        &["--world", "retail"],
        &[
            call(
                1,
                "find_user_id_by_email",
                json!({"email": "c7@example.com"}),
            ),
            call(
                2,
                "find_user_id_by_email",
                json!({"email": "c@example.com"}),
            ),
            call(
                3,
                "get_user_details",
                json!({"user_id": "user_7", "delay_ms": 1}),
            ),
            call(4, "get_user_details", json!({"user_id": "user_"})),
            call(5, "get_order_details", json!({"order_id": "#W7b"})),
            call(6, "get_order_details", json!({"order_id": "#W7c"})),
            call(
                7,
                "cancel_pending_order",
                json!({"order_id": "#W7a", "reason": "no longer needed"}),
            ),
            call(
                8,
                "cancel_pending_order",
                json!({"order_id": "#W7a", "reason": "too slow"}),
            ),
            call(
                9,
                "cancel_pending_order",
                json!({"order_id": "W7a", "reason": "ordered by mistake"}),
            ),
            json!({"jsonrpc": "2.0", "id": 10, "method": "tools/call",
                   "params": {"name": "get_user_details"}}),
            call(11, "lookup", json!({})),
        ],
    );
    assert_eq!(text(&answers[0]), ("user_7", false));
    assert_eq!(text(&answers[1]), ("Error: User not found", true));
    let user = parsed(&answers[2]);
    assert_eq!(user["orders"], json!(["#W7a", "#W7b"]));
    assert_eq!(
        user["payment_methods"]["credit_card_7"]["source"],
        "credit_card"
    );
    assert_eq!(text(&answers[3]), ("Error: User not found", true));
    let order = parsed(&answers[4]);
    assert_eq!(
        (&order["user_id"], &order["status"]),
        (&json!("user_7"), &json!("pending"))
    );
    assert_eq!(text(&answers[5]), ("Error: Order not found", true));
    assert_eq!(parsed(&answers[6])["status"], "cancelled");
    assert_eq!(text(&answers[7]), ("Error: Invalid reason", true));
    assert_eq!(text(&answers[8]), ("Error: Order not found", true));
    assert_eq!(text(&answers[9]), ("Error: User not found", true));
    assert_eq!(answers[10]["error"]["message"], "Unknown tool: lookup");
    // `call` answers only the tools the shop lists.
    assert_eq!(
        retail::answer("lookup", &json!({})),
        Err("Unknown tool: lookup".to_string())
    );
}

#[test]
fn the_shop_lists_its_tools_as_told() {
    let answers = answers(
        &[
            "--world",
            "retail",
            "--hide",
            "get_order_details",
            "--hint",
            "find_user_id_by_email=false",
            "--hint",
            "get_user_details=none",
        ],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            call(2, "get_order_details", json!({"order_id": "#W1a"})),
        ],
    );
    let listed: Vec<(&str, &Value)> = answers[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), &t["annotations"]))
        .collect();
    assert_eq!(
        listed,
        [
            ("find_user_id_by_email", &json!({"readOnlyHint": false})),
            ("get_user_details", &json!({})),
            ("cancel_pending_order", &json!({"readOnlyHint": false})),
        ]
    );
    assert_eq!(
        answers[1]["error"]["message"],
        "Unknown tool: get_order_details"
    );
}

/// A server on a local port with `extra` arguments, over HTTP: its address.
fn http_server(extra: &[&str]) -> String {
    let mut args = vec!["--http".to_string(), "127.0.0.1:0".to_string()];
    args.extend(extra.iter().map(|a| a.to_string()));
    let (reader, mut writer) = io::pipe().unwrap();
    std::thread::spawn(move || run(&args, &mut io::empty(), &mut writer));
    let mut url = String::new();
    BufReader::new(reader).read_line(&mut url).unwrap();
    let addr = url.trim().strip_prefix("http://").unwrap();
    addr.strip_suffix("/mcp").unwrap().to_string()
}

/// Send `request` and read the whole response.
fn exchange(addr: &str, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

/// A request to `/mcp` with `headers` and `body`.
fn request(method: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut head = format!(
        "{method} /mcp HTTP/1.1\r\nHost: demo\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    format!("{head}\r\n{body}")
}

fn status(response: &str) -> &str {
    response.split("\r\n").next().unwrap()
}

fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    let (head, _) = response.split_once("\r\n\r\n")?;
    head.lines()
        .filter_map(|l| l.split_once(": "))
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

#[test]
fn serves_the_same_over_streamable_http() {
    let addr = http_server(&["--require-auth", "Bearer t"]);
    let auth = ("Authorization", "Bearer t");
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                      "params": {"protocolVersion": "2025-03-26"}})
    .to_string();

    // Only /mcp, and only with the key.
    let elsewhere = exchange(&addr, "GET /other HTTP/1.1\r\nHost: demo\r\n\r\n");
    assert_eq!(status(&elsewhere), "HTTP/1.1 404 Not Found");
    for headers in [&[][..], &[("Authorization", "Bearer x")]] {
        let refused = exchange(&addr, &request("POST", headers, &init));
        assert_eq!(status(&refused), "HTTP/1.1 401 Unauthorized");
    }
    // A header line with no colon is skipped, and a head that never ends is
    // read as far as it goes.
    let odd = exchange(
        &addr,
        "GET /mcp HTTP/1.1\r\nno colon here\r\nAuthorization: Bearer t\r\n\r\n",
    );
    assert_eq!(status(&odd), "HTTP/1.1 404 Not Found");
    let mut cut = TcpStream::connect(&addr).unwrap();
    cut.write_all(b"GET /mcp HTTP/1.1\r\nAuthorization: Bearer t\r\n")
        .unwrap();
    cut.shutdown(Shutdown::Write).unwrap();
    let mut response = String::new();
    cut.read_to_string(&mut response).unwrap();
    assert_eq!(status(&response), "HTTP/1.1 404 Not Found");

    // `initialize` assigns a session and agrees a version.
    let answer = exchange(&addr, &request("POST", &[auth], &init));
    assert_eq!(status(&answer), "HTTP/1.1 200 OK");
    let session = header(&answer, "Mcp-Session-Id").unwrap().to_string();
    let result: Value = serde_json::from_str(body(&answer)).unwrap();
    assert_eq!(result["result"]["protocolVersion"], "2025-03-26");
    // One with no id has no answer to give.
    let unanswerable = json!({"jsonrpc": "2.0", "method": "initialize"}).to_string();
    let refused = exchange(&addr, &request("POST", &[auth], &unanswerable));
    assert_eq!(status(&refused), "HTTP/1.1 400 Bad Request");

    // Everything later names a live session and its version.
    let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string();
    let live = [
        auth,
        ("Mcp-Session-Id", session.as_str()),
        ("MCP-Protocol-Version", "2025-03-26"),
    ];
    for (headers, expected) in [
        (&[auth][..], "HTTP/1.1 400 Bad Request"),
        (
            &[auth, ("Mcp-Session-Id", "demo-session-99")],
            "HTTP/1.1 404 Not Found",
        ),
        (
            &[
                auth,
                ("Mcp-Session-Id", session.as_str()),
                ("MCP-Protocol-Version", "2025-06-18"),
            ],
            "HTTP/1.1 400 Bad Request",
        ),
    ] {
        let response = exchange(&addr, &request("POST", headers, &list));
        assert_eq!(status(&response), expected, "{headers:?}");
    }
    let listed = exchange(&addr, &request("POST", &live, &list));
    assert_eq!(header(&listed, "Content-Type"), Some("application/json"));
    let tools: Value = serde_json::from_str(body(&listed)).unwrap();
    assert_eq!(tools["result"]["tools"][0]["name"], "lookup");
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let accepted = exchange(&addr, &request("POST", &live, &initialized.to_string()));
    assert_eq!(status(&accepted), "HTTP/1.1 202 Accepted");

    // A tool call is answered on an event stream, a log message first.
    let lookup = call(3, "lookup", json!({"text": "hi"})).to_string();
    let called = exchange(&addr, &request("POST", &live, &lookup));
    assert_eq!(header(&called, "Content-Type"), Some("text/event-stream"));
    let events: Vec<Value> = body(&called)
        .split("\n\n")
        .filter_map(|e| e.strip_prefix("event: message\ndata: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["params"]["data"], "calling lookup");
    assert_eq!(text(&events[1]), (r#"{"text":"hi"}"#, false));

    // The server's own stream needs a session, and stays open until the
    // session is deleted.
    let unknown = exchange(&addr, &request("GET", &[auth], ""));
    assert_eq!(status(&unknown), "HTTP/1.1 404 Not Found");
    let mut stream = TcpStream::connect(&addr).unwrap();
    stream
        .write_all(request("GET", &live, "").as_bytes())
        .unwrap();
    let mut seen = Vec::new();
    let mut chunk = [0; 512];
    while !String::from_utf8_lossy(&seen).contains("hello from the server's own stream") {
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0, "{}", String::from_utf8_lossy(&seen));
        seen.extend_from_slice(&chunk[..n]);
    }
    let deleted = exchange(&addr, &request("DELETE", &live, ""));
    assert_eq!(status(&deleted), "HTTP/1.1 200 OK");
    stream.read_to_end(&mut seen).unwrap();
    let gone = exchange(&addr, &request("POST", &live, &list));
    assert_eq!(status(&gone), "HTTP/1.1 404 Not Found");
    let nothing = exchange(&addr, &request("DELETE", &[auth], ""));
    assert_eq!(status(&nothing), "HTTP/1.1 200 OK");
    let put = exchange(&addr, &request("PUT", &[auth], ""));
    assert_eq!(
        (status(&put), header(&put, "Allow")),
        ("HTTP/1.1 405 Method Not Allowed", Some("GET, POST, DELETE"))
    );
}
