use super::*;
use tokio::sync::mpsc::error::TryRecvError;

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

fn app_with(config: Config, options: Listen) -> Arc<App> {
    Arc::new(App {
        config,
        active: None,
        options,
        port: 8931,
        sessions: Mutex::new(HashMap::new()),
        opened: AtomicU64::new(0),
        threads: Mutex::new(Vec::new()),
    })
}

fn app(token: Option<&str>) -> Arc<App> {
    let options = Listen {
        token: token.map(String::from),
        ..Listen::default()
    };
    app_with(Config::new(["cat"]), options)
}

/// A session, and the lines it passes on.
fn session() -> (Arc<Session>, mpsc::Receiver<Vec<u8>>) {
    let (tx, rx) = mpsc::channel();
    (Arc::new(Session::new("s1".into(), 1, tx)), rx)
}

/// The status a request was refused with.
fn refused<T>(result: Result<T, Refusal>) -> StatusCode {
    match result {
        Ok(_) => panic!("not refused"),
        Err(refusal) => refusal.status,
    }
}

fn request(id: &str) -> Request {
    (id.to_string(), None)
}

fn note(n: usize) -> String {
    format!(r#"{{"jsonrpc":"2.0","method":"notifications/message","params":{{"n":{n}}}}}"#)
}

async fn body(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn a_url_names_loopback_for_an_unspecified_address() {
    let at = |a: &str| url(a.parse().unwrap());
    assert_eq!(at("0.0.0.0:8931"), "http://127.0.0.1:8931/mcp");
    assert_eq!(at("[::]:8931"), "http://[::1]:8931/mcp");
    assert_eq!(at("192.168.1.5:80"), "http://192.168.1.5:80/mcp");
}

#[test]
fn only_loopback_is_served_without_a_token() {
    let open = Listen::default();
    assert!(admissible("127.0.0.1:1".parse().unwrap(), &open).is_ok());
    assert!(admissible("[::1]:1".parse().unwrap(), &open).is_ok());
    let e = admissible("0.0.0.0:8931".parse().unwrap(), &open).unwrap_err();
    assert!(e
        .to_string()
        .starts_with("listening on 0.0.0.0:8931 needs a token"));
    let closed = Listen {
        token: Some("t".repeat(16)),
        ..Listen::default()
    };
    assert!(admissible("0.0.0.0:8931".parse().unwrap(), &closed).is_ok());
}

#[test]
fn requests_are_checked_for_their_token_host_origin_and_version() {
    let open = app(None);
    let ok = |h: &[(&str, &str)]| open.check(&headers(h)).is_ok();
    assert!(ok(&[("host", "127.0.0.1:8931")]));
    assert!(ok(&[
        ("host", "localhost:8931"),
        ("origin", "http://localhost:6274")
    ]));
    assert!(ok(&[
        ("host", "[::1]:8931"),
        ("mcp-protocol-version", "2025-11-25")
    ]));
    let refused = |h: &HeaderMap| open.check(h).unwrap_err().status;
    assert_eq!(refused(&HeaderMap::new()), StatusCode::FORBIDDEN);
    assert_eq!(
        refused(&headers(&[("host", "evil.example:8931")])),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        refused(&headers(&[
            ("host", "127.0.0.1:8931"),
            ("origin", "https://evil.example")
        ])),
        StatusCode::FORBIDDEN
    );
    let mut garbled = headers(&[("host", "127.0.0.1:8931")]);
    garbled.insert(header::ORIGIN, HeaderValue::from_bytes(&[0xff]).unwrap());
    assert_eq!(refused(&garbled), StatusCode::FORBIDDEN);
    assert_eq!(
        refused(&headers(&[
            ("host", "127.0.0.1:8931"),
            ("mcp-protocol-version", "latest")
        ])),
        StatusCode::BAD_REQUEST
    );

    // With a token, the token is what counts, from any page.
    let closed = app(Some("s3cret-token-0123"));
    assert!(closed
        .check(&headers(&[
            ("authorization", "Bearer s3cret-token-0123"),
            ("origin", "https://app.example")
        ]))
        .is_ok());
    for given in [
        headers(&[("authorization", "Bearer wrong")]),
        headers(&[("host", "127.0.0.1:8931")]),
    ] {
        let r = closed.check(&given).unwrap_err().into_response();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(r.headers()[header::WWW_AUTHENTICATE], "Bearer");
    }
}

#[test]
fn names_and_values_are_read_as_the_protocol_says() {
    assert!(loopback_host("LOCALHOST:8931", 8931));
    assert!(loopback_host("localhost", 80));
    assert!(!loopback_host("localhost", 8931));
    assert!(!loopback_host("127.0.0.1:x", 8931));
    assert!(!loopback_host("[::1", 8931));
    assert!(loopback_origin("https://127.0.0.1"));
    assert!(loopback_origin("http://[::1]:8080"));
    assert!(!loopback_origin("null"));
    assert!(!loopback_origin("http://localhost.evil.example"));
    assert!(revision("2025-03-26"));
    assert!(!revision("2025-3-26") && !revision("2025/03/26"));
    assert!(host_session("task-7:a.b_c"));
    assert!(!host_session("") && !host_session("a b") && !host_session(&"x".repeat(129)));
    assert!(json_body(&headers(&[(
        "content-type",
        "application/json; charset=utf-8"
    )])));
    assert!(!json_body(&headers(&[("content-type", "text/plain")])));
    assert!(!json_body(&HeaderMap::new()));
    let both = headers(&[
        ("accept", "application/json"),
        ("accept", "Text/Event-Stream"),
    ]);
    assert!(accepts(&both, "text/event-stream"));
    assert!(!accepts(
        &headers(&[("accept", "application/json")]),
        "text/event-stream"
    ));
    assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"abc", b"ab"));
    let (a, b) = (new_id(), new_id());
    assert!(a.len() == 32 && a.bytes().all(|c| c.is_ascii_hexdigit()) && a != b);
}

#[tokio::test]
async fn a_body_is_read_as_its_messages() {
    let (one, batch) = messages(
        br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"_meta":{"progressToken":"p1"}}}"#,
    )
    .unwrap();
    assert!(!batch && !one[0].initialize);
    assert_eq!(one[0].request, Some(("7".into(), Some("\"p1\"".into()))));
    // As sent, on one line.
    let (sent, _) =
        messages(b"{\"jsonrpc\": \"2.0\",\r\n \"method\": \"initialize\", \"id\": \"a\"}").unwrap();
    assert_eq!(
        sent[0].line,
        b"{\"jsonrpc\": \"2.0\",   \"method\": \"initialize\", \"id\": \"a\"}\n"
    );
    assert!(sent[0].initialize);
    let (told, _) = messages(br#"{"jsonrpc":"2.0","method":"initialize"}"#).unwrap();
    assert!(!told[0].initialize && told[0].request.is_none());
    let (all, batch) = messages(
        br#" [{"jsonrpc":"2.0","method":"notifications/initialized"},
              {"jsonrpc":"2.0","id":1,"result":{}},
              {"jsonrpc":"2.0","id":2,"error":{"code":1,"message":"no"}}]"#,
    )
    .unwrap();
    assert!(batch && all.len() == 3 && all.iter().all(|m| m.request.is_none()));

    for (bad, code) in [
        (&b"\xff"[..], -32700),
        (b"nope", -32700),
        (b"[]", -32600),
        (b"[1]", -32600),
        (br#"{"jsonrpc":"2.0","id":1}"#, -32600),
    ] {
        let refused = messages(bad).err().unwrap();
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        let error = body(refused.into_response()).await;
        assert_eq!(error["id"], Value::Null);
        assert_eq!(error["error"]["code"], code);
    }
}

#[test]
fn answers_go_on_the_stream_of_their_request_which_ends_with_the_last() {
    let (s, _core) = session();
    let mut answers = s.expect(&[request("1"), request("2")], false).unwrap();
    s.deliver(br#"{"jsonrpc":"2.0","id":2,"result":{}}"#);
    // A batch from the server, with a Windows line end.
    s.deliver(b"[{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"a\":1}}]\r\n");
    let first: Value = serde_json::from_str(&answers.try_recv().unwrap()).unwrap();
    let second: Value = serde_json::from_str(&answers.try_recv().unwrap()).unwrap();
    assert_eq!(first["id"], 2);
    assert_eq!(second["result"]["a"], 1);
    assert_eq!(answers.try_recv(), Err(TryRecvError::Disconnected));
    // An answer no request waits for, a line that is not JSON and an
    // empty line are dropped.
    s.deliver(br#"{"jsonrpc":"2.0","id":9,"result":{}}"#);
    s.deliver(br#"{"jsonrpc":"2.0","error":{"code":1,"message":"no id"}}"#);
    s.deliver(b"not json");
    s.deliver(b"  ");
    let routes = lock(&s.routes);
    assert!(routes.queue.is_empty() && routes.streams.is_empty());
}

#[test]
fn the_servers_own_messages_go_on_the_best_open_stream_or_wait_for_one() {
    let (s, _core) = session();
    // No stream: it waits.
    s.deliver(note(1).as_bytes());
    // A JSON body takes answers alone.
    let _json = s.expect(&[request("1")], true).unwrap();
    s.deliver(note(2).as_bytes());
    assert_eq!(lock(&s.routes).queue.len(), 2);
    // A request's event stream takes them, those waiting first.
    let mut post = s
        .expect(&[("2".into(), Some("\"tok\"".into()))], false)
        .unwrap();
    assert_eq!(post.try_recv().unwrap(), note(1));
    assert_eq!(post.try_recv().unwrap(), note(2));
    // With a GET stream open, it takes them, but for progress on the
    // request's own token.
    let mut get = s.listen().unwrap();
    s.deliver(note(3).as_bytes());
    s.deliver(
        br#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"tok","progress":1}}"#,
    );
    assert_eq!(get.try_recv().unwrap(), note(3));
    assert!(post.try_recv().unwrap().contains("\"progress\":1"));
    // A new GET stream replaces the old, which ends.
    let again = s.listen().unwrap();
    assert_eq!(get.try_recv(), Err(TryRecvError::Disconnected));
    // A stream whose host has gone is forgotten, and the next one takes
    // the message.
    drop(again);
    s.deliver(note(4).as_bytes());
    assert_eq!(post.try_recv().unwrap(), note(4));
    assert!(lock(&s.routes).get.is_none());
    // So are its requests; the JSON body left takes no message.
    drop(post);
    s.deliver(note(5).as_bytes());
    let routes = lock(&s.routes);
    assert_eq!(routes.queue, [note(5)]);
    assert_eq!(routes.pending.keys().collect::<Vec<_>>(), ["1"]);
}

#[test]
fn at_most_so_many_messages_wait_for_a_stream() {
    let (s, _core) = session();
    for n in 0..=MAX_QUEUED {
        s.deliver(note(n).as_bytes());
    }
    let routes = lock(&s.routes);
    assert_eq!(routes.queue.len(), MAX_QUEUED);
    assert_eq!(routes.queue[0], note(1));
}

#[test]
fn a_request_id_waits_for_one_answer_at_a_time() {
    let (s, _core) = session();
    let _waiting = s.expect(&[request("1")], false).unwrap();
    assert_eq!(
        s.expect(&[request("1")], false).unwrap_err().status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        s.expect(&[request("2"), request("2")], false)
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn an_ended_session_answers_what_waits_and_takes_nothing_more() {
    let (s, core) = session();
    assert_eq!(
        accepted(&s, [b"x\n".to_vec()]).status(),
        StatusCode::ACCEPTED
    );
    assert_eq!(core.recv().unwrap(), b"x\n");
    assert_eq!(events_for(&s).status(), StatusCode::OK);
    let mut waiting = s.expect(&[request("\"a\"")], false).unwrap();
    assert!(s.end("its server exited with status 1"));
    assert!(!s.end("again"));
    let error: Value = serde_json::from_str(&waiting.try_recv().unwrap()).unwrap();
    assert_eq!(error["id"], "a");
    assert_eq!(
        error["error"]["message"],
        "the session ended: its server exited with status 1"
    );
    assert_eq!(waiting.try_recv(), Err(TryRecvError::Disconnected));
    // The server's input has ended.
    assert!(core.recv().is_err());
    assert!(!s.send([b"y\n".to_vec()]));
    assert_eq!(
        s.expect(&[request("2")], false).unwrap_err().status,
        StatusCode::NOT_FOUND
    );
    assert!(s.listen().is_none());
    s.deliver(note(1).as_bytes());
    assert!(lock(&s.routes).queue.is_empty());
    assert_eq!(
        accepted(&s, [b"z\n".to_vec()]).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(events_for(&s).status(), StatusCode::NOT_FOUND);
}

#[test]
fn a_session_is_idle_with_no_request_and_no_open_stream() {
    let (s, _core) = session();
    let stream = s.listen().unwrap();
    assert_eq!(s.idle_for(), Duration::ZERO);
    drop(stream);
    thread::sleep(Duration::from_millis(20));
    assert!(s.idle_for() >= Duration::from_millis(20));
    s.touch();
    assert!(s.idle_for() < Duration::from_millis(20));
}

#[test]
fn a_sessions_input_and_output_are_lines() {
    let (tx, rx) = mpsc::channel();
    let mut input = Lines::new(rx);
    for part in [&b"ab"[..], b"", b"c\n"] {
        tx.send(part.to_vec()).unwrap();
    }
    drop(tx);
    let mut all = String::new();
    input.read_to_string(&mut all).unwrap();
    assert_eq!(all, "abc\n");

    let (s, _core) = session();
    let mut answers = s.expect(&[request("1"), request("2")], false).unwrap();
    let mut out = Out {
        session: s.clone(),
        line: Vec::new(),
    };
    out.write_all(br#"{"jsonrpc":"2.0","id":1,"#).unwrap();
    out.write_all(b"\"result\":{}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}")
        .unwrap();
    out.flush().unwrap();
    assert!(answers.try_recv().unwrap().contains("\"id\":1"));
    assert_eq!(answers.try_recv(), Err(TryRecvError::Empty));
    // A last line with no newline goes out when the output closes.
    drop(out);
    assert!(answers.try_recv().unwrap().contains("\"id\":2"));
}

#[test]
fn sessions_open_as_the_host_names_them_and_up_to_the_most_there_may_be() {
    let app = app(None);
    let bad = app.open(
        &headers(&[("stretto-session", "a b")]),
        &[request("1")],
        false,
    );
    assert_eq!(refused(bad), StatusCode::BAD_REQUEST);
    {
        let mut sessions = lock(&app.sessions);
        for n in 0..MAX_SESSIONS {
            let (tx, _) = mpsc::channel();
            let session = Session::new(n.to_string(), n as u64, tx);
            sessions.insert(n.to_string(), Arc::new(session));
        }
    }
    let full = app.open(&HeaderMap::new(), &[request("1")], false);
    assert_eq!(refused(full), StatusCode::SERVICE_UNAVAILABLE);
    let named = |id: &str| app.session(&headers(&[("mcp-session-id", id)]));
    assert_eq!(
        refused(app.session(&HeaderMap::new())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(refused(named("nope")), StatusCode::NOT_FOUND);
    assert!(named(" 7 ").is_ok_and(|s| s.n == 7));
    app.close_all("stretto-proxy is stopping");
    assert!(lock(&app.sessions).is_empty());
}

#[test]
fn stopping_waits_so_long_for_the_sessions_threads() {
    let options = Listen {
        grace: Duration::from_millis(30),
        ..Listen::default()
    };
    let app = app_with(Config::new(["cat"]), options);
    lock(&app.threads).push(thread::spawn(|| thread::sleep(Duration::from_millis(500))));
    let started = Instant::now();
    app.finish();
    assert!(started.elapsed() < Duration::from_millis(400));
}

/// With `cat` as each session's server, what the host sends comes back as
/// the server's own messages.
#[cfg(unix)]
#[test]
fn a_session_runs_its_own_proxy_until_it_ends() {
    let dir = std::env::temp_dir().join(format!("stretto-listen-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let logs = dir.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let old = logs.join("old.jsonl");
    std::fs::File::create(&old)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(30 * 86_400))
        .unwrap();
    let config = Config {
        record: Some(logs.clone()),
        host_session: Some("host-1".into()),
        ..Config::new(["cat"])
    };
    let options = Listen {
        idle: Duration::from_millis(1),
        retain: Some((vec![logs.clone()], 7)),
        ..Listen::default()
    };
    let app = app_with(config, options);
    // Its host gave up on the answer to initialize.
    let (s, _) = app
        .open(
            &headers(&[("stretto-session", "task-7")]),
            &[request("0")],
            false,
        )
        .unwrap();
    // Each session prunes as it starts.
    assert!(!old.exists());
    let mut own = s.listen().unwrap();
    assert!(s.send([b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec()]));
    assert!(own.blocking_recv().unwrap().contains("\"ping\""));
    // An open stream keeps it.
    thread::sleep(Duration::from_millis(5));
    app.reap();
    assert!(lock(&app.sessions).contains_key(&s.id));
    drop(own);
    app.reap();
    assert!(lock(&app.sessions).is_empty());
    app.finish();
    let names: Vec<PathBuf> = std::fs::read_dir(&logs)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let log = stretto_trace::mcp::read_log(&names[0]).unwrap();
    assert!(log.header.session.ends_with("-1"));
    assert_eq!(log.header.host_session.as_deref(), Some("task-7"));
    assert_eq!(log.messages().count(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A session whose server cannot start answers `initialize` with the
/// reason.
#[test]
fn a_server_that_cannot_start_ends_its_session_with_the_reason() {
    let app = app_with(
        Config::new(["/nonexistent/stretto-test-server"]),
        Listen::default(),
    );
    let (_, mut answer) = app.open(&HeaderMap::new(), &[request("1")], true).unwrap();
    let error: Value = serde_json::from_str(&answer.blocking_recv().unwrap()).unwrap();
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .starts_with("the session ended: starting /nonexistent/stretto-test-server"));
    app.finish();
}

#[test]
fn a_port_in_use_is_refused_and_a_poisoned_lock_still_opens() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap();
    let e = listen(addr, &Config::new(["cat"]), None, &Listen::default()).unwrap_err();
    assert_eq!(e.to_string(), format!("listening on {addr}"));

    // A thread that panicked holding a lock leaves the rest usable.
    let mutex = Mutex::new(1);
    let _ = std::panic::catch_unwind(|| {
        let _held = mutex.lock().unwrap();
        panic!("a session's thread fails");
    });
    assert!(mutex.is_poisoned());
    assert_eq!(*lock(&mutex), 1);
}
