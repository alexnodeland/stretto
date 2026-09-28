//! The engine in this process: the host's and the server's lines handed to
//! it one at a time, and what it sends each side read back.

use super::*;
use fugue::{addr, factor, observe, pure, sample, Bernoulli, Model, ModelExt, Normal, Poisson};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Mutex;
use stretto_oracle::{MockOracle, Request, Response};
use stretto_report::flow::PER_CALL;
use stretto_trace::mcp::LOG_VERSION;

/// A flow with an arbiter, learned in the retail domain.
const ARBITER: &str = "docs/examples/retail-5-sessions-shipped-arbiter.flow.json";
/// A flow with reach counts, learned on the demo's shop.
const SHOP: &str = "crates/stretto-console/tests/fixtures/home/shop.flow.json";

fn repo(path: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// One side of the wire: what the engine wrote to it, and whether writing
/// to it fails.
#[derive(Clone, Default)]
struct Wire {
    written: Rc<RefCell<Vec<u8>>>,
    broken: Rc<Cell<bool>>,
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.broken.get() {
            return Err(std::io::Error::other("broken pipe"));
        }
        self.written.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Wire {
    /// The lines written since the last take, as JSON where they are.
    fn take(&self) -> Vec<Value> {
        let bytes = std::mem::take(&mut *self.written.borrow_mut());
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|_| json!(l)))
            .collect()
    }
}

fn header() -> LogHeader {
    LogHeader {
        stretto_mcp_log: LOG_VERSION,
        session: "session".to_string(),
        started_unix_ms: 0,
        server_command: Vec::new(),
        domain: Some("retail".to_string()),
        agent_model: None,
        host_session: None,
        server_name: None,
    }
}

fn flow_config(path: &str, decider: Decider) -> FlowConfig {
    FlowConfig {
        flow: Flow::load(&repo(path)).unwrap(),
        oracle: Box::new(MockOracle {
            confidence: 0.6,
            noul: 0.5,
        }),
        threshold: 0.3,
        decider,
        per_call: PER_CALL,
        per_session: 40,
        max_questions: 300,
        log: None,
        shadow: false,
        tools: None,
        explore: None,
    }
}

/// The engine, with the two sides of its wire.
struct Harness<'a> {
    engine: Engine<'a, Wire>,
    server: Wire,
    host: Wire,
}

fn harness(active: &Active) -> Harness<'_> {
    harness_with(active, None, (None, None))
}

/// The engine with `tap` and its flow and confirmation logs.
fn harness_with(
    active: &Active,
    tap: Option<Tap>,
    logs: (Option<PathBuf>, Option<PathBuf>),
) -> Harness<'_> {
    harness_of(active, header(), tap, logs)
}

/// The engine, its log started with `header`.
fn harness_of(
    active: &Active,
    header: LogHeader,
    tap: Option<Tap>,
    logs: (Option<PathBuf>, Option<PathBuf>),
) -> Harness<'_> {
    let (server, host) = (Wire::default(), Wire::default());
    let engine = Engine::new(
        active,
        header,
        tap,
        Instant::now(),
        (Box::new(server.clone()), host.clone()),
        logs,
    );
    Harness {
        engine,
        server,
        host,
    }
}

impl Harness<'_> {
    /// The host sends `message`.
    fn client(&mut self, message: Value) {
        self.engine.on_client(line_of(&message));
    }

    /// The server sends `message`.
    fn server_says(&mut self, message: Value) {
        self.engine.on_server(line_of(&message));
    }

    /// Answer the proxy's own requests as the shop would, until the host
    /// has something: what it has.
    fn serve_lookups(&mut self) -> Vec<Value> {
        loop {
            let host = self.host.take();
            if !host.is_empty() {
                return host;
            }
            let asked = self.server.take();
            assert!(!asked.is_empty(), "the engine waits on nothing");
            for m in asked {
                let text = shop(&m["params"]["name"], &m["params"]["arguments"]);
                self.server_says(result(m["id"].clone(), &text));
            }
        }
    }

    /// The session so far, up to the agent's call 3 and its result.
    fn found_user(&mut self) {
        self.client(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        self.server_says(json!({"jsonrpc": "2.0", "id": 2, "result": {"tools": shop_tools()}}));
        self.client(call(
            3,
            "find_user_id_by_email",
            json!({"email": "c7@example.com"}),
        ));
        self.server.take();
        self.host.take();
    }
}

/// What the demo shop answers.
fn shop(tool: &Value, arguments: &Value) -> String {
    match tool.as_str().unwrap_or_default() {
        "find_user_id_by_email" => "user_7".to_string(),
        "get_user_details" => json!({"user_id": arguments["user_id"],
                                     "orders": ["#W7a", "#W7b"]})
        .to_string(),
        "get_order_details" => json!({"order_id": arguments["order_id"], "user_id": "user_7",
                                      "status": "pending"})
        .to_string(),
        "cancel_pending_order" => json!({"order_id": arguments["order_id"],
                                         "status": "cancelled"})
        .to_string(),
        other => panic!("the shop has no {other}"),
    }
}

/// The text of each item of a tool result.
fn texts(message: &Value) -> Vec<&str> {
    message["result"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["text"].as_str().unwrap())
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-engine-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Append a line of the conversation to the host's file.
fn say(context: &std::path::Path, role: &str, text: &str) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(context)
        .unwrap();
    writeln!(file, "{}", json!({"role": role, "content": text})).unwrap();
}

fn call(id: impl Into<Value>, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id.into(), "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
}

fn result(id: impl Into<Value>, text: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id.into(),
           "result": {"content": [{"type": "text", "text": text}], "isError": false}})
}

/// The demo shop's tools, as it lists them.
fn shop_tools() -> Value {
    let tool = |name: &str, read_only: bool| {
        json!({"name": name, "inputSchema": {"type": "object"},
               "annotations": {"readOnlyHint": read_only}})
    };
    json!([
        tool("find_user_id_by_email", true),
        tool("get_user_details", true),
        tool("get_order_details", true),
        tool("cancel_pending_order", false),
    ])
}

// ---- the flow ---------------------------------------------------------------------------

#[test]
fn a_flow_looks_up_what_the_agent_will_need_next() {
    let dir = scratch("flow");
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        ..Default::default()
    };
    let flow_log = dir.join("flow.jsonl");
    let mut h = harness_with(&active, None, (Some(flow_log.clone()), None));
    h.found_user();
    h.server_says(result(3, "user_7"));
    // The flow asks for the user's details in the agent's place.
    let asked = h.server.take();
    assert_eq!(asked[0]["id"], "stretto-1");
    assert_eq!(
        asked[0]["params"],
        json!({"name": "get_user_details", "arguments": {"user_id": "user_7"}})
    );
    h.server_says(result(
        "stretto-1",
        &shop(&json!("get_user_details"), &json!({"user_id": "user_7"})),
    ));
    let answered = h.serve_lookups();
    assert_eq!(answered.len(), 1);
    let parts = texts(&answered[0]);
    assert_eq!(parts[0], "user_7");
    assert!(parts[1].starts_with(APPENDIX), "{}", parts[1]);
    assert!(parts[1].contains("get_user_details {\"user_id\":\"user_7\"}:\n"));
    // Each decision is logged, then the run.
    let logged = std::fs::read_to_string(&flow_log).unwrap();
    let entries: Vec<Value> = logged
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(entries.len() >= 2);
    assert_eq!(
        entries.last().unwrap()["run"]["call"],
        "find_user_id_by_email"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_flow_stops_at_its_limits_and_in_shadow() {
    // One lookup per session: the run's second decision hands back.
    let mut fc = flow_config(ARBITER, Decider::Arbiter);
    fc.per_session = 1;
    let active = Active {
        flow: Some(fc),
        ..Default::default()
    };
    let mut h = harness(&active);
    h.found_user();
    h.server_says(result(3, "user_7"));
    let answered = h.serve_lookups();
    assert_eq!(texts(&answered[0]).len(), 2);
    assert_eq!(h.engine.lookups, 1);

    // In shadow, and where a tool is not granted, the decision is logged
    // and the agent gets its own result.
    let mut shadow = flow_config(ARBITER, Decider::Arbiter);
    shadow.shadow = true;
    let mut withheld = flow_config(ARBITER, Decider::Arbiter);
    withheld.tools = Some(["get_order_details".to_string()].into());
    for fc in [shadow, withheld] {
        let active = Active {
            flow: Some(fc),
            ..Default::default()
        };
        let mut h = harness(&active);
        h.found_user();
        h.server_says(result(3, "user_7"));
        assert!(h.server.take().is_empty());
        assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
    }
}

/// An oracle that cannot answer.
struct Unanswering;

impl Oracle for Unanswering {
    fn ask(&self, _: &Request) -> anyhow::Result<Response> {
        anyhow::bail!("no answer")
    }
}

#[test]
fn a_flow_that_fails_hands_back() {
    // The System-One model cannot answer; the flow cannot decide as it is
    // served (it has no reach counts).
    let mut unanswered = flow_config(ARBITER, Decider::Arbiter);
    unanswered.oracle = Box::new(Unanswering);
    for fc in [unanswered, flow_config(ARBITER, Decider::Reach)] {
        let active = Active {
            flow: Some(fc),
            ..Default::default()
        };
        let mut h = harness(&active);
        h.found_user();
        h.server_says(result(3, "user_7"));
        assert!(h.server.take().is_empty());
        assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
    }
}

#[test]
fn a_flow_whose_program_does_not_build_never_runs() {
    // The shop's program, with a distribution no program knows.
    let read = |path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(repo(path)).unwrap()).unwrap()
    };
    let program = read(SHOP)["program"]
        .to_string()
        .replace("\"Decide\"", "\"Nowhere\"");
    let mut flow = read(ARBITER);
    flow["program"] = serde_json::from_str(&program).unwrap();
    let mut fc = flow_config(ARBITER, Decider::Arbiter);
    fc.flow = serde_json::from_value(flow).unwrap();
    let active = Active {
        flow: Some(fc),
        ..Default::default()
    };
    let mut h = harness(&active);
    assert!(h.engine.program.is_none());
    h.found_user();
    h.server_says(result(3, "user_7"));
    assert!(h.server.take().is_empty());
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
}

#[test]
fn a_flow_starts_only_after_a_call() {
    let dir = scratch("after");
    let context = dir.join("context.jsonl");
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        context: Some(context.clone()),
        ..Default::default()
    };
    let mut h = harness(&active);
    h.found_user();
    // The agent spoke after the call returned: the session's last step is
    // no call.
    say(&context, "assistant", "Found you.");
    h.engine.read_context();
    assert!(h.engine.flow_run("3").is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A run of `model` for the agent's call 3, as the engine would drive it.
fn custom(model: Model<RunValue>, response: Value) -> FlowWork {
    let mailbox = Rc::new(RefCell::new(Mailbox::default()));
    let handler = LiveRun {
        mailbox: mailbox.clone(),
        trace: Trace::default(),
    };
    FlowWork {
        original: line_of(&response),
        response,
        looked: Vec::new(),
        after: "3".to_string(),
        run: Run {
            data: RunData {
                call: "find_user_id_by_email".to_string(),
                failed: false,
                max_lookups: 1,
            },
            future: Box::pin(run_async(handler, model)),
            mailbox,
            pending: None,
        },
    }
}

fn decide_site(options: &[&str]) -> fugue::WithMeta<Categorical, DecideSite> {
    let n = options.len();
    fugue::WithMeta::new(
        Categorical::new(vec![1.0 / n as f64; n]).unwrap(),
        DecideSite {
            site: "find_user_id_by_email".to_string(),
            options: options.iter().map(|o| o.to_string()).collect(),
        },
    )
}

#[test]
fn runs_that_ask_what_the_flow_cannot_give_hand_back() {
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        ..Default::default()
    };
    let mut h = harness(&active);
    h.found_user();
    // The call returns with no flow after it.
    h.engine.calls.clear();
    h.server_says(result(3, "user_7"));
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);

    // The flow chooses a lookup its program does not offer at the site.
    let model = sample(addr!("decide", 0), decide_site(&["respond"])).map(RunValue::Usize);
    h.engine.drive(json!(3), custom(model, result(3, "user_7")));
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
    // An outcome with no lookup before it fails.
    let model = sample(addr!("outcome", 0), Bernoulli::new(0.5).unwrap()).map(RunValue::Bool);
    h.engine.drive(json!(3), custom(model, result(3, "user_7")));
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
    // A run that waits on nothing cannot go on.
    let mut work = custom(pure(RunValue::Int(0)), result(3, "user_7"));
    work.run.future = Box::pin(std::future::pending());
    h.engine.drive(json!(3), work);
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
}

// ---- the handler that runs a flow's program ----------------------------------------------

/// Run `model` under a [`LiveRun`], answering each site with `answer`.
fn live(model: Model<RunValue>, answer: impl Fn(&Ask) -> Answer) -> (RunValue, Trace) {
    let mailbox = Rc::new(RefCell::new(Mailbox::default()));
    let handler = LiveRun {
        mailbox: mailbox.clone(),
        trace: Trace::default(),
    };
    let mut future = Box::pin(run_async(handler, model));
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
        let ask = mailbox.borrow_mut().ask.take().expect("a waiting run asks");
        mailbox.borrow_mut().answer = Some(answer(&ask));
    }
}

#[test]
fn the_handler_takes_decisions_and_outcomes_from_the_engine() {
    let decide = || {
        sample(
            addr!("decide", 0),
            decide_site(&["respond", "get_user_details"]),
        )
    };
    let (value, trace) = live(decide().map(RunValue::Usize), |ask| match ask {
        Ask::Decide { address, site } => {
            assert_eq!((address.as_str(), site.options.len()), ("decide#0", 2));
            Answer::Option(1)
        }
        Ask::LookUp => unreachable!("no lookup here"),
    });
    assert_eq!(value, RunValue::Usize(1));
    assert_eq!(trace.choices[&addr!("decide", 0)].logp, 0.0);
    // An answer that is no option hands back; so does a site with no
    // options of the flow's.
    let (value, _) = live(decide().map(RunValue::Usize), |_| Answer::Failed(true));
    assert_eq!(value, RunValue::Usize(HAND_BACK));
    let plain = sample(
        addr!("decide", 0),
        Categorical::new(vec![0.5, 0.5]).unwrap(),
    );
    let (value, _) = live(plain.map(RunValue::Usize), |_| {
        unreachable!("nothing to ask")
    });
    assert_eq!(value, RunValue::Usize(HAND_BACK));

    // An outcome is the lookup's success, scored in the likelihood.
    let outcome = || sample(addr!("outcome", 0), Bernoulli::new(0.8).unwrap()).map(RunValue::Bool);
    let (value, trace) = live(outcome(), |_| Answer::Failed(false));
    assert_eq!(value, RunValue::Bool(true));
    assert!((trace.log_likelihood - 0.8f64.ln()).abs() < 1e-12);
    let (value, _) = live(outcome(), |_| Answer::Option(1));
    assert_eq!(value, RunValue::Bool(false));
}

#[test]
fn the_handler_scores_observations_and_factors() {
    let model = observe(addr!("a"), Normal::new(0.0, 1.0).unwrap(), 0.0)
        .bind(|()| observe(addr!("b"), Bernoulli::new(0.5).unwrap(), true))
        .bind(|()| observe(addr!("c"), Poisson::new(1.0).unwrap(), 0u64))
        .bind(|()| {
            observe(
                addr!("d"),
                Categorical::new(vec![0.25, 0.75]).unwrap(),
                1usize,
            )
        })
        .bind(|()| factor(-1.5))
        .map(|()| RunValue::Int(0));
    let (_, trace) = live(model, |_| unreachable!("nothing to ask"));
    let expected = -0.5 * (2.0 * std::f64::consts::PI).ln() + 0.5f64.ln() - 1.0 + 0.75f64.ln();
    assert!((trace.log_likelihood - expected).abs() < 1e-12);
    assert_eq!(trace.log_factors, -1.5);
}

#[test]
#[should_panic(expected = "a flow's program has no f64 site")]
fn a_flow_program_has_no_continuous_site() {
    let model = sample(addr!("x"), Normal::new(0.0, 1.0).unwrap()).map(RunValue::F64);
    live(model, |_| unreachable!("nothing to ask"));
}

#[test]
#[should_panic(expected = "a flow's program has no u64 site")]
fn a_flow_program_has_no_count_site() {
    let model = sample(addr!("x"), Poisson::new(1.0).unwrap()).map(RunValue::U64);
    live(model, |_| unreachable!("nothing to ask"));
}

// ---- commits -----------------------------------------------------------------------------

fn commit(id: u64, calls: Value) -> Value {
    call(id, COMMIT_TOOL, json!({"calls": calls}))
}

#[test]
fn a_commit_makes_its_calls_in_order_and_reports_each() {
    let active = Active {
        commit: true,
        ..Default::default()
    };
    let mut h = harness(&active);
    // The commit tool is listed after the server's own.
    h.client(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    h.server_says(json!({"jsonrpc": "2.0", "id": 2, "result": {"tools": shop_tools()}}));
    let listed = h.host.take();
    assert_eq!(listed[0]["result"]["tools"][4]["name"], COMMIT_TOOL);
    h.server.take();

    // A call with no arguments takes none.
    h.client(commit(
        3,
        json!([{"name": "get_order_details", "arguments": {"order_id": "#W7a"}},
               {"name": "find_user_id_by_email"}]),
    ));
    let answered = h.serve_lookups();
    assert_eq!(answered[0]["result"]["isError"], false);
    let report = texts(&answered[0])[0];
    assert!(
        report.starts_with("get_order_details {\"order_id\":\"#W7a\"}:\n"),
        "{report}"
    );
    assert!(
        report.ends_with("find_user_id_by_email {}:\nuser_7"),
        "{report}"
    );

    // A call that fails stops the rest.
    h.client(commit(
        4,
        json!([{"name": "get_order_details", "arguments": {"order_id": "#W9"}},
               {"name": "find_user_id_by_email", "arguments": {}}]),
    ));
    let asked = h.server.take();
    h.server_says(json!({"jsonrpc": "2.0", "id": asked[0]["id"],
                         "result": {"content": [{"type": "text", "text": "Order not found"}],
                                    "isError": true}}));
    let answered = h.host.take();
    assert_eq!(answered[0]["result"]["isError"], true);
    let report = texts(&answered[0])[0];
    assert!(report.contains("(error):\nOrder not found"), "{report}");
    assert!(
        report.ends_with("find_user_id_by_email {} was not run"),
        "{report}"
    );

    // What is not a list of calls of the server's tools runs nothing.
    for calls in [
        json!([]),
        json!("get_order_details"),
        json!([{"arguments": {}}]),
        json!([{"name": COMMIT_TOOL, "arguments": {}}]),
        json!([{"name": "get_order_details", "arguments": "#W7a"}]),
    ] {
        h.client(commit(5, calls));
        let answered = h.host.take();
        assert_eq!(answered[0]["result"]["isError"], true);
        assert!(texts(&answered[0])[0].ends_with("nothing was run."));
    }
    assert!(h.server.take().is_empty());
}

#[test]
fn a_commit_the_guards_refuse_stops_there() {
    let active = Active {
        commit: true,
        guards: Guards::for_domain("retail"),
        ..Default::default()
    };
    let mut h = harness(&active);
    h.client(commit(
        3,
        json!([{"name": "cancel_pending_order",
                "arguments": {"order_id": "#W7a", "reason": "found it cheaper"}}]),
    ));
    let answered = h.host.take();
    let report = texts(&answered[0])[0];
    assert!(
        report.contains("was refused by the policy check"),
        "{report}"
    );
    assert!(h.server.take().is_empty());
}

#[test]
fn work_the_server_never_answers_is_finished_without_it() {
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        commit: true,
        ..Default::default()
    };
    // The server ends while a lookup and a commit wait on it.
    let mut h = harness(&active);
    h.found_user();
    h.server_says(result(3, "user_7"));
    h.client(commit(
        4,
        json!([{"name": "get_order_details", "arguments": {"order_id": "#W7a"}}]),
    ));
    assert_eq!(h.engine.jobs.len(), 2);
    h.engine.server_ended();
    let answered = h.host.take();
    assert_eq!(texts(&answered[0]), ["user_7"]);
    assert!(texts(&answered[1])[0].ends_with("got no answer from the server"));

    // Each waits past its patience; a late answer is dropped.
    let mut h = harness(&active);
    h.found_user();
    h.server_says(result(3, "user_7"));
    h.client(commit(
        4,
        json!([{"name": "get_order_details", "arguments": {"order_id": "#W7a"}}]),
    ));
    let asked = h.server.take();
    let long_ago = Instant::now()
        .checked_sub(PATIENCE + Duration::from_secs(1))
        .unwrap();
    for job in &mut h.engine.jobs {
        job.waiting.as_mut().unwrap().since = long_ago;
    }
    h.engine.expire();
    assert!(h.engine.jobs.is_empty());
    assert_eq!(h.host.take().len(), 2);
    h.server_says(result(asked[0]["id"].clone(), "late"));
    assert!(h.host.take().is_empty());

    // With the server's input closed, a lookup or a commit's next call is
    // not made.
    let mut h = harness(&active);
    h.found_user();
    h.client(commit(
        4,
        json!([{"name": "get_order_details", "arguments": {"order_id": "#W7a"}},
               {"name": "get_order_details", "arguments": {"order_id": "#W7b"}}]),
    ));
    let asked = h.server.take();
    h.engine.to_server = None;
    // The agent's call returns while the commit waits: the flow may still
    // follow it, but cannot make its lookup.
    h.server_says(result(3, "user_7"));
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
    let order = shop(&json!("get_order_details"), &json!({"order_id": "#W7a"}));
    h.server_says(result(asked[0]["id"].clone(), &order));
    let answered = h.host.take();
    let report = texts(&answered[0])[0];
    assert!(
        report.ends_with("{\"order_id\":\"#W7b\"} got no answer from the server"),
        "{report}"
    );
}

#[test]
fn only_one_flow_runs_at_a_time() {
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        ..Default::default()
    };
    let mut h = harness(&active);
    h.found_user();
    h.client(call(
        4,
        "find_user_id_by_email",
        json!({"email": "c7@example.com"}),
    ));
    h.server_says(result(3, "user_7"));
    assert_eq!(h.engine.jobs.len(), 1);
    // The second call returns while the first's flow waits: it is
    // forwarded as it is.
    h.server_says(result(4, "user_7"));
    let answered = h.host.take();
    assert_eq!((answered.len(), &answered[0]["id"]), (1, &json!(4)));
}

// ---- the wire and the log ----------------------------------------------------------------

#[test]
fn what_is_not_a_response_to_the_proxy_goes_to_the_host_as_it_is() {
    let dir = scratch("wire");
    let recorder = crate::record::Recorder::start(&dir, &header(), Instant::now()).unwrap();
    let active = Active {
        commit: true,
        ..Default::default()
    };
    let mut h = harness_with(&active, Some(recorder.tap()), (None, None));
    // Lines that are not one JSON message, and messages that are no
    // response, are forwarded unchanged.
    h.engine.on_server(b"not JSON\n".to_vec());
    h.engine.on_server(b"[1, 2]\n".to_vec());
    let note = json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {}});
    h.server_says(note.clone());
    assert_eq!(h.host.take(), [json!("not JSON"), json!([1, 2]), note]);
    // A tool with no name is not learned.
    h.client(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    h.server_says(json!({"jsonrpc": "2.0", "id": 2,
                         "result": {"tools": [{"description": "nameless"},
                                              {"name": "lookup", "annotations": {"readOnlyHint": true}}]}}));
    assert_eq!(h.engine.server_tools.len(), 1);
    // A call with no arguments takes none.
    h.client(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "lookup"}}),
    );
    let path = recorder.path().to_path_buf();
    recorder.finish();
    let log = stretto_trace::mcp::read_log(&path).unwrap();
    assert_eq!(log.entries[0].raw.as_deref(), Some("not JSON"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_side_that_stops_taking_lines_gets_no_more() {
    let active = Active {
        commit: true,
        ..Default::default()
    };
    let mut h = harness(&active);
    h.server.broken.set(true);
    h.client(call(3, "lookup", json!({})));
    assert!(h.engine.to_server.is_none());
    h.client(call(4, "lookup", json!({})));
    h.host.broken.set(true);
    h.server_says(result(3, "ok"));
    h.server_says(result(4, "ok"));
    assert!(!h.engine.host_ok);
    h.host.broken.set(false);
    h.server_says(result(5, "ok"));
    assert!(h.host.take().is_empty());
}

#[test]
fn the_conversation_is_read_a_whole_line_at_a_time() {
    let dir = scratch("context");
    let context = dir.join("context.jsonl");
    let active = Active {
        context: Some(context.clone()),
        ..Default::default()
    };
    let mut h = harness(&active);
    // No file yet: nothing said yet.
    h.engine.read_context();
    // Half a line waits for the rest; blank lines are skipped.
    std::fs::write(
        &context,
        "\n{\"role\":\"user\",\"content\":\"Hi\"}\n{\"role\":",
    )
    .unwrap();
    h.engine.read_context();
    assert_eq!(h.engine.log.entries.len(), 1);
    // A directory is no conversation.
    let directory = Active {
        context: Some(dir.clone()),
        ..Default::default()
    };
    let mut h = harness(&directory);
    h.engine.read_context();
    assert!(h.engine.log.entries.is_empty());
    // A partial line alone is not read.
    std::fs::write(&context, "{\"role\":").unwrap();
    let mut h = harness(&active);
    h.engine.read_context();
    assert!(h.engine.log.entries.is_empty());
    // Without a conversation file there is nothing to read.
    let none = Active::default();
    let mut h = harness(&none);
    h.engine.read_context();
    assert!(h.engine.log.entries.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_flow_log_that_cannot_be_opened_is_skipped() {
    let active = Active {
        flow: Some(flow_config(ARBITER, Decider::Arbiter)),
        ..Default::default()
    };
    let nowhere = PathBuf::from("/nonexistent/stretto/flow.jsonl");
    let h = harness_with(&active, None, (Some(nowhere.clone()), Some(nowhere)));
    assert!(h.engine.flow_log.is_none() && h.engine.confirm_log.is_none());
}

#[test]
fn reads_lines_until_the_input_ends_fails_or_nobody_listens() {
    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("gone"))
        }
    }
    let lines = |input: Box<dyn Read>| {
        let (tx, rx) = std::sync::mpsc::channel();
        read_lines(input, tx, Input::Client, Input::ClientEnd);
        rx.into_iter()
            .map(|i| match i {
                Input::Client(line) => String::from_utf8(line).unwrap(),
                _ => "end".to_string(),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(Box::new(&b"a\nb"[..])), ["a\n", "b", "end"]);
    assert_eq!(
        lines(Box::new((&b"a\n"[..]).chain(Failing))),
        ["a\n", "end"]
    );
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    read_lines(
        Box::new(&b"a\n"[..]) as Box<dyn Read>,
        tx,
        Input::Server,
        Input::ServerEnd,
    );
}

// ---- guards and the confirmation judge ---------------------------------------------------

/// Answers each request with the next of its answers, whatever it asks.
struct Scripted(Mutex<VecDeque<anyhow::Result<Response>>>);

impl Scripted {
    fn new(answers: Vec<anyhow::Result<Response>>) -> Box<Self> {
        Box::new(Scripted(Mutex::new(answers.into())))
    }
}

impl Oracle for Scripted {
    fn ask(&self, _: &Request) -> anyhow::Result<Response> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .expect("an answer is scripted")
    }
}

fn answer(id: &str, answer: stretto_oracle::Answer) -> anyhow::Result<Response> {
    Ok(Response {
        model: "jev-test".to_string(),
        answers: BTreeMap::from([(id.to_string(), answer)]),
        usage: Default::default(),
    })
}

fn noul(id: &str, p: f64) -> anyhow::Result<Response> {
    answer(id, stretto_oracle::Answer::Noul { noul: p })
}

fn judge(oracle: Box<Scripted>, enforce: bool) -> ConfirmConfig {
    ConfirmConfig {
        oracle,
        model: "jev-latest".to_string(),
        second: None,
        second_shadow: false,
        threshold: 0.5,
        enforce,
        max_questions: 10,
        log: None,
    }
}

/// A session up to the customer's yes to cancelling `order`, with the
/// user's orders listed as records: then the agent's cancellation of
/// `cancels`. What the host and the server got, and the judgment.
fn cancel(confirm: ConfirmConfig, order: &str, cancels: &str) -> (Vec<Value>, Vec<Value>, Value) {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = scratch(&format!("judge-{run}"));
    let context = dir.join("context.jsonl");
    let active = Active {
        guards: Guards::for_domain("retail"),
        confirm: Some(confirm),
        context: Some(context.clone()),
        ..Default::default()
    };
    let judged = dir.join("confirm.jsonl");
    let mut h = harness_with(&active, None, (None, Some(judged.clone())));
    say(
        &context,
        "user",
        "Hi, I'm c7@example.com and I want to cancel an order.",
    );
    h.client(call(
        3,
        "find_user_id_by_email",
        json!({"email": "c7@example.com"}),
    ));
    h.server_says(result(3, "user_7"));
    h.client(call(4, "get_user_details", json!({"user_id": "user_7"})));
    let orders = json!({"user_id": "user_7", "orders": [
        {"order_id": "#W7a", "status": "pending", "item": "lamp 1"},
        {"order_id": "#W7b", "status": "pending", "item": "fan 2"}]});
    h.server_says(result(4, &orders.to_string()));
    for (id, o) in [(5, "#W7a"), (6, "#W7b")] {
        h.client(call(id, "get_order_details", json!({"order_id": o})));
        let details = shop(&json!("get_order_details"), &json!({"order_id": o}));
        h.server_says(result(id, &details));
    }
    say(
        &context,
        "assistant",
        &format!("I can cancel order {order} because you no longer need it. Shall I go ahead?"),
    );
    say(&context, "user", &format!("Yes, cancel {order}, please."));
    h.server.take();
    h.host.take();
    h.client(call(
        7,
        "cancel_pending_order",
        json!({"order_id": cancels, "reason": "no longer needed"}),
    ));
    let (server, host) = (h.server.take(), h.host.take());
    let judgment = std::fs::read_to_string(&judged)
        .map(|text| serde_json::from_str(text.lines().last().unwrap()).unwrap())
        .unwrap_or(Value::Null);
    std::fs::remove_dir_all(&dir).unwrap();
    (server, host, judgment)
}

#[test]
fn the_judge_passes_a_confirmed_write_and_refuses_one_it_doubts() {
    let (server, host, judged) = cancel(
        judge(Scripted::new(vec![noul(confirm::QUESTION, 0.9)]), true),
        "#W7a",
        "#W7a",
    );
    assert_eq!((server.len(), host.len()), (1, 0));
    assert_eq!(
        (&judged["p_yes"], &judged["fails"]),
        (&json!(0.9), &json!(false))
    );
    let (server, host, judged) = cancel(
        judge(Scripted::new(vec![noul(confirm::QUESTION, 0.1)]), true),
        "#W7a",
        "#W7a",
    );
    assert_eq!((server.len(), host.len()), (0, 1));
    let why = texts(&host[0])[0];
    assert!(why.contains("confirmation judge: p_yes 0.10"), "{why}");
    assert_eq!(judged["fails"], true);
    // Only logged when not enforced.
    let (server, _, judged) = cancel(
        judge(Scripted::new(vec![noul(confirm::QUESTION, 0.1)]), false),
        "#W7a",
        "#W7a",
    );
    assert_eq!((server.len(), &judged["enforced"]), (1, &json!(false)));
}

#[test]
fn a_judge_that_cannot_answer_refuses_nothing() {
    let choice = stretto_oracle::Answer::Choice {
        choice: "yes".to_string(),
        probabilities: BTreeMap::new(),
        confidence: 1.0,
    };
    for (answers, error) in [
        (
            vec![answer(confirm::QUESTION, choice)],
            "no yes/no answer to confirmed",
        ),
        (
            vec![Err(anyhow::anyhow!("the judge is away"))],
            "the judge is away",
        ),
    ] {
        let (server, _, judged) = cancel(judge(Scripted::new(answers), true), "#W7a", "#W7a");
        assert_eq!(server.len(), 1);
        assert_eq!(
            (&judged["error"], &judged["unknown"]),
            (&json!(error), &json!(true))
        );
    }
}

#[test]
fn a_second_question_counts_unless_shadowed_and_within_the_budget() {
    let second = |shadow: bool, max_questions: usize, answers| ConfirmConfig {
        second: Some(Second::Proposed),
        second_shadow: shadow,
        max_questions,
        ..judge(Scripted::new(answers), true)
    };
    // Shadowed: logged, and its no refuses nothing.
    let answers = vec![
        noul(confirm::QUESTION, 0.9),
        noul(Second::Proposed.id(), 0.1),
    ];
    let (server, _, judged) = cancel(second(true, 10, answers), "#W7a", "#W7a");
    assert_eq!(server.len(), 1);
    assert_eq!(
        (&judged["second_shadow"], &judged["p_second"]),
        (&json!(true), &json!(0.1))
    );
    // Past the session's budget, a question that counts is not asked, and
    // the judge does not know.
    let (server, _, judged) = cancel(
        second(false, 1, vec![noul(confirm::QUESTION, 0.9)]),
        "#W7a",
        "#W7a",
    );
    assert_eq!(server.len(), 1);
    assert_eq!(
        (&judged["skipped"], &judged["unknown"]),
        (
            &json!("the session's question budget is spent"),
            &json!(true)
        )
    );
}

#[test]
fn the_judge_logs_a_value_the_customer_did_not_choose() {
    let (_, _, judged) = cancel(
        judge(Scripted::new(vec![noul(confirm::QUESTION, 0.9)]), false),
        "#W7a",
        "#W7b",
    );
    assert_eq!(
        judged["proposal_check"],
        json!([{"argument": "order_id", "value": "#w7b"}])
    );
}

// ---- messages ----------------------------------------------------------------------------

#[test]
fn appends_the_flows_lookups_in_the_pilots_format() {
    let response = json!({"jsonrpc": "2.0", "id": 3, "result": {
        "content": [{"type": "text", "text": "user_7"}], "isError": false}});
    let looked = [
        Looked {
            tool: "get_user_details".to_string(),
            arguments: json!({"user_id": "user_7"}),
            text: "{\"orders\":[\"#W7a\"]}".to_string(),
            error: false,
        },
        Looked {
            tool: "get_order_details".to_string(),
            arguments: json!({"order_id": "#W9"}),
            text: "Order not found".to_string(),
            error: true,
        },
    ];
    let out = with_appendix(response, &looked);
    let content = out["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["text"], "user_7");
    let text = content[1]["text"].as_str().unwrap();
    assert!(text.starts_with(APPENDIX));
    assert!(
        text.contains("\n\nget_user_details {\"user_id\":\"user_7\"}:\n{\"orders\":[\"#W7a\"]}")
    );
    assert!(text.contains("\n\nget_order_details {\"order_id\":\"#W9\"} (error):\nOrder not found"));
    // A result with no content gets the appendix as its only item.
    let out = with_appendix(
        json!({"jsonrpc": "2.0", "id": 3, "result": {}}),
        &looked[..1],
    );
    assert_eq!(out["result"]["content"].as_array().unwrap().len(), 1);
}

#[test]
fn tells_requests_from_responses() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call"});
    let response = json!({"jsonrpc": "2.0", "id": 1, "result": {}});
    let notification = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert!(request_id(&request).is_some() && response_id(&request).is_none());
    assert!(response_id(&response).is_some() && request_id(&response).is_none());
    assert!(request_id(&notification).is_none() && response_id(&notification).is_none());
    assert_ne!(key(&json!(1)), key(&json!("1")));
    assert_eq!(
        result_text(&json!({"id": 1, "error": {"code": -1, "message": "no"}})),
        ("no".to_string(), true)
    );
    assert_eq!(
        result_text(&json!({"id": 1, "error": {"code": -1}})),
        ("error".to_string(), true)
    );
    assert_eq!(
        result_text(&json!({"id": 1, "error": null, "result": {"content": [
            {"type": "text", "text": "a"}, {"type": "image"}, {"type": "text", "text": "b"}],
            "isError": true}})),
        ("a\nb".to_string(), true)
    );
    assert_eq!(
        result_text(&json!({"id": 1, "result": {}})),
        (String::new(), false)
    );
}

/// The agent is told when to use the commit tool, in the instructions the
/// host puts in its prompt; without the tool, the server's answer is its
/// own.
#[test]
fn the_commit_tool_is_explained_in_the_servers_instructions() {
    let active = Active {
        commit: true,
        ..Default::default()
    };
    let mut h = harness(&active);
    h.client(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}));
    assert_eq!(h.server.take()[0]["method"], "initialize");
    h.server_says(json!({"jsonrpc": "2.0", "id": 1,
                         "result": {"protocolVersion": "2025-06-18", "instructions": "Be kind.\n"}}));
    let answered = h.host.take();
    assert_eq!(
        answered[0]["result"]["instructions"],
        format!("Be kind.\n\n{COMMIT_NOTE}")
    );
    assert_eq!(answered[0]["result"]["protocolVersion"], "2025-06-18");

    let plain = Active::default();
    let mut h = harness(&plain);
    h.client(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}));
    let theirs = json!({"jsonrpc": "2.0", "id": 1, "result": {"instructions": "Be kind."}});
    h.server_says(theirs.clone());
    assert_eq!(h.host.take(), [theirs]);
}

#[test]
fn the_note_follows_the_servers_instructions_or_stands_alone() {
    let noted = |result: Value| with_commit_note(json!({"id": 1, "result": result}));
    let instructions = |result: Value| noted(result)["result"]["instructions"].clone();
    assert_eq!(instructions(json!({})), COMMIT_NOTE);
    assert_eq!(instructions(json!({"instructions": "  "})), COMMIT_NOTE);
    assert_eq!(
        instructions(json!({"instructions": "Be kind."})),
        format!("Be kind.\n\n{COMMIT_NOTE}")
    );
    // An error is not an answer to add to.
    let refused = json!({"id": 1, "error": {"code": -32600, "message": "no"}});
    assert_eq!(with_commit_note(refused.clone()), refused);
}

#[test]
fn lists_the_commit_tool_after_the_servers() {
    let listed = with_commit_tool(json!({"id": 2, "result": {"tools": [{"name": "lookup"}]}}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, ["lookup", COMMIT_TOOL]);
    // A list that is not one is left alone.
    let odd = json!({"id": 2, "result": {"tools": "none"}});
    assert_eq!(with_commit_tool(odd.clone()), odd);
}

#[test]
fn a_runs_entry_holds_each_site_in_order() {
    let mut trace = Trace::default();
    for (address, value) in [
        (addr!("outcome", 10), ChoiceValue::Bool(true)),
        (addr!("decide", 10), ChoiceValue::Usize(1)),
        (addr!("decide", 2), ChoiceValue::U64(3)),
        (addr!("x"), ChoiceValue::I64(-1)),
        (addr!("y"), ChoiceValue::F64(0.5)),
    ] {
        trace.insert_choice(address, value, -0.25);
    }
    trace.log_likelihood = -0.5;
    let data = RunData {
        call: "lookup".to_string(),
        failed: true,
        max_lookups: 2,
    };
    let entry = run_entry(&json!(3), &data, &trace);
    assert_eq!(
        entry,
        json!({"after": 3, "run": {"call": "lookup", "failed": true, "max_lookups": 2,
               "sites": [["x", -1, -0.25], ["y", 0.5, -0.25], ["decide#2", 3, -0.25],
                         ["decide#10", 1, -0.25], ["outcome#10", true, -0.25]],
               "surprise": 0.5}})
    );
}

// ---- a flow learned across servers ----------------------------------------------------------

/// A flow of the habit alone, learned from host sessions of two servers,
/// `shop` and `notes`: the agent finds the user, reads their details, looks
/// the user up in its notes, then reads their order.
fn across_servers() -> Flow {
    use stretto_trace::{Event, ToolManifest};
    let server = |tool: &str| if tool == "lookup" { "notes" } else { "shop" };
    let q = |tool: &str| mcp::qualify(server(tool), tool);
    let call = |id: &str, tool: &str, arguments: Value| Event::Assistant {
        text: None,
        calls: vec![ToolCall {
            id: id.to_string(),
            name: q(tool),
            arguments,
        }],
        usage: None,
    };
    let result = |id: &str, tool: &str, content: String| Event::ToolResult {
        call_id: id.to_string(),
        name: q(tool),
        error: false,
        content,
    };
    let episodes: Vec<Episode> = (0..30)
        .map(|i| {
            let user = format!("user_{i}");
            let details = json!({"user_id": user, "orders": [format!("#W{i}a")]});
            Episode {
                id: format!("host-{i}"),
                task_id: String::new(),
                trial: 0,
                domain: "support".to_string(),
                agent_model: "agent".to_string(),
                reward: 1.0,
                events: vec![
                    call(
                        "1",
                        "find_user_id_by_email",
                        json!({"email": format!("c{i}@example.com")}),
                    ),
                    result("1", "find_user_id_by_email", user.clone()),
                    call("2", "get_user_details", json!({"user_id": user})),
                    result("2", "get_user_details", details.to_string()),
                    call("3", "lookup", json!({"text": user})),
                    result("3", "lookup", json!({"text": user}).to_string()),
                    call(
                        "4",
                        "get_order_details",
                        json!({"order_id": format!("#W{i}a")}),
                    ),
                    result("4", "get_order_details", "{}".to_string()),
                ],
            }
        })
        .collect();
    let tools = [
        "find_user_id_by_email",
        "get_user_details",
        "get_order_details",
        "lookup",
    ];
    let manifest = ToolManifest {
        domain: "support".to_string(),
        tools: tools.iter().map(|t| (q(t), ToolKind::Read)).collect(),
        docs: BTreeMap::new(),
    };
    let mut config = stretto_report::phase0::Config::new(PathBuf::new());
    config.alpha_samples = 0;
    stretto_report::phase0::compile_habit_flow_from_episodes(&config, &episodes, &manifest).unwrap()
}

/// The engine as the proxy of the server named `name`.
fn serving_as<'a>(active: &'a Active, name: &str) -> Harness<'a> {
    let mut named = header();
    named.server_name = Some(name.to_string());
    harness_of(active, named, None, (None, None))
}

#[test]
fn a_flow_learned_across_servers_looks_up_this_servers_tools_by_their_own_names() {
    let flow = across_servers();
    let granted = |tools: Option<&[&str]>| Active {
        flow: Some(FlowConfig {
            flow: flow.clone(),
            tools: tools.map(|t| t.iter().map(|t| t.to_string()).collect()),
            ..flow_config(SHOP, Decider::Habit)
        }),
        ..Active::default()
    };
    // Granted by its own name or the flow's, or not named at all.
    for tools in [
        None,
        Some(&["get_user_details"][..]),
        Some(&["shop::get_user_details"][..]),
    ] {
        let active = granted(tools);
        let mut h = serving_as(&active, "shop");
        h.found_user();
        h.server_says(result(3, "user_7"));
        let asked = h.server.take();
        assert_eq!(asked.len(), 1, "{tools:?}: {asked:?}");
        assert_eq!(asked[0]["params"]["name"], "get_user_details");
        let details = shop(
            &asked[0]["params"]["name"],
            &asked[0]["params"]["arguments"],
        );
        h.server_says(result(asked[0]["id"].clone(), &details));
        // Next came the notes server's lookup, which this proxy leaves to
        // the agent.
        assert!(h.server.take().is_empty());
        let answer = h.host.take();
        let parts = texts(&answer[0]);
        assert!(
            parts[1].contains("\n\nget_user_details {\"user_id\":\"user_7\"}:\n"),
            "{parts:?}"
        );
    }
    // A lookup not granted is left to the agent.
    let active = granted(Some(&["get_order_details"]));
    let mut h = serving_as(&active, "shop");
    h.found_user();
    h.server_says(result(3, "user_7"));
    assert!(h.server.take().is_empty());
    assert_eq!(texts(&h.host.take()[0]), ["user_7"]);
}

#[test]
fn a_server_the_flow_does_not_name_gets_no_lookups() {
    let active = Active {
        flow: Some(FlowConfig {
            flow: across_servers(),
            ..flow_config(SHOP, Decider::Habit)
        }),
        ..Active::default()
    };
    let mut h = serving_as(&active, "billing");
    h.found_user();
    for (id, user) in [(3, "user_7"), (4, "user_8")] {
        if id == 4 {
            h.client(call(
                id,
                "find_user_id_by_email",
                json!({"email": "c8@example.com"}),
            ));
            h.server.take();
        }
        h.server_says(result(id, user));
        assert!(h.server.take().is_empty());
        assert_eq!(texts(&h.host.take()[0]), [user]);
    }
}
