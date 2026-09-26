//! Active mode: a flow behind the agent's calls, guards before its writes,
//! and a commit tool (RFC-001 §3.5, §3.8 and §3.14).
//!
//! In active mode the proxy reads what crosses it, one line at a time on
//! one thread. Anything it does not act on is still forwarded byte for byte.
//!
//! - **Flows.** When the server answers one of the agent's `tools/call`s,
//!   the proxy holds the response and runs the flow: its run is the fugue
//!   program the flow holds ([`stretto_report::program`]), with a decision
//!   site before each lookup and an outcome site after it, interpreted with
//!   fugue's `run_async`. At a decision site the flow's arbiter decides what
//!   comes next ([`Flow::next_with`]). While it proposes lookups (tools that
//!   the flow reads as read-only and the server does not mark otherwise),
//!   the proxy makes them itself, as requests with ids `stretto-<n>`, and
//!   the outcome site takes the server's answer. When the flow hands back,
//!   the agent gets its own result with the flow's results appended as one
//!   more text item under [`APPENDIX`]. The agent's prompt and tools are
//!   unchanged (arm D0). One flow runs at a time; a response that arrives
//!   while one runs is forwarded as it is.
//! - **Guards.** Before one of the agent's calls reaches the server, the
//!   domain's guards check it against what the session has shown. A call an
//!   enforced rule refuses never reaches the server: the agent gets an error
//!   result that says why.
//! - **Confirmation.** With a [`ConfirmConfig`], a write the guards check for
//!   the customer's confirmation is also put to a System-One model, with the
//!   questions `stretto confirm` asks offline. Each judgment is logged; in
//!   enforce mode, a write it fails is refused like one a guard refuses. A
//!   judge that cannot answer refuses nothing.
//! - **Commit.** [`COMMIT_TOOL`] is added to the tools the server lists. It
//!   makes several calls in one, in order, each checked by the guards first,
//!   and stops at the first that is refused or fails.
//! - **Context.** MCP never carries the conversation, so the host can append
//!   it to a file as JSON lines, `{"role": "user" | "assistant", "content":
//!   text}`. The proxy reads new lines before each decision and logs them as
//!   `context` entries, so flows see what the customer said and guards see
//!   their confirmation.
//!
//! Everything the proxy sends on its own is logged as a `proxy` entry.

use crate::record::Tap;
use fugue::program::Value as RunValue;
use fugue::{
    run_async, Address, AsyncHandler, Categorical, Choice, ChoiceValue, Distribution, Trace,
    WithMeta,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::future::Future;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use stretto_model::{Action, Outcome as StepOutcome};
use stretto_oracle::{request_key, Oracle};
use stretto_report::confirm::{self, Second};
use stretto_report::flow::{Decider, Flow, Proposal};
use stretto_report::guards::Guards;
use stretto_report::program::{DecideSite, FlowProgram, HAND_BACK};
use stretto_trace::mcp::{self, LogEntry, LogHeader, McpLog, Peer};
use stretto_trace::{Episode, ToolCall, ToolKind};

/// The commit tool's name.
pub const COMMIT_TOOL: &str = "stretto_commit";

/// The first line of the flow's results, appended to the agent's own.
pub const APPENDIX: &str =
    "--- Also looked up automatically (current results; no need to repeat these calls) ---";

/// How long a request of the proxy's own may wait for the server.
const PATIENCE: Duration = Duration::from_secs(60);

/// How often the loop wakes, to give up on requests past their patience.
const TICK: Duration = Duration::from_millis(250);

/// A flow to run behind the agent's calls.
pub struct FlowConfig {
    /// The flow.
    pub flow: Flow,
    /// Who answers its questions.
    pub oracle: Box<dyn Oracle + Sync>,
    /// Take a lookup when the tool's probability times its arguments'
    /// agreement is at least this.
    pub threshold: f64,
    /// Where the tool's probability comes from; the habit alone never asks
    /// `oracle`.
    pub decider: Decider,
    /// Lookups appended to one response, at most.
    pub per_call: usize,
    /// Lookups per session, at most.
    pub per_session: usize,
    /// Questions to the System-One model per session, at most.
    pub max_questions: usize,
    /// Append every decision here, as JSON lines.
    pub log: Option<PathBuf>,
    /// Shadow mode (RFC-001 §3.7): decide and log, but make no lookups.
    pub shadow: bool,
    /// Exploration at the flow's decisions, logged with each one.
    pub explore: Option<stretto_report::flow::Explore>,
}

/// A System-One model judging whether the customer confirmed each write.
pub struct ConfirmConfig {
    /// Who answers.
    pub oracle: Box<dyn Oracle + Sync>,
    /// The model id to request; it is part of each question's cache key.
    pub model: String,
    /// Also ask this question; a write then fails unless both answers are yes.
    pub second: Option<Second>,
    /// Only log the second question's answer: a write fails on the first
    /// answer alone.
    pub second_shadow: bool,
    /// A write fails when an answer's probability of a yes is below this.
    pub threshold: f64,
    /// Refuse a write the judge fails; otherwise only log the judgment.
    pub enforce: bool,
    /// Questions per session, at most.
    pub max_questions: usize,
    /// Append every judgment here, as JSON lines.
    pub log: Option<PathBuf>,
}

/// What the proxy does besides forwarding.
#[derive(Default)]
pub struct Active {
    /// Run this flow after the agent's calls.
    pub flow: Option<FlowConfig>,
    /// Check the agent's calls against these guards.
    pub guards: Option<Guards>,
    /// Judge the writes the guards check for a confirmation (needs `guards`).
    pub confirm: Option<ConfirmConfig>,
    /// Add [`COMMIT_TOOL`] to the server's tools.
    pub commit: bool,
    /// Read the conversation from this file (JSON lines).
    pub context: Option<PathBuf>,
    /// The task, for the flow's fold (default: the session).
    pub task_id: Option<String>,
}

impl Active {
    /// Whether there is anything to do besides forwarding.
    pub fn is_active(&self) -> bool {
        self.flow.is_some()
            || self.guards.is_some()
            || self.confirm.is_some()
            || self.commit
            || self.context.is_some()
    }
}

/// A line from either side, or the end of one.
pub(crate) enum Input {
    Client(Vec<u8>),
    Server(Vec<u8>),
    ClientEnd,
    ServerEnd,
}

/// Read `input` line by line into `tx`, then send `end`.
pub(crate) fn read_lines(
    input: impl Read,
    tx: Sender<Input>,
    wrap: fn(Vec<u8>) -> Input,
    end: Input,
) {
    let mut input = BufReader::new(input);
    loop {
        let mut line = Vec::new();
        match input.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if tx.send(wrap(line)).is_err() {
                    return;
                }
            }
            Err(e) => {
                eprintln!("stretto-proxy: reading: {e}");
                break;
            }
        }
    }
    let _ = tx.send(end);
}

/// A lookup the flow made.
struct Looked {
    tool: String,
    arguments: Value,
    text: String,
    error: bool,
}

/// What became of one call of a commit.
enum Outcome {
    Done { text: String, error: bool },
    Refused(String),
    NoAnswer,
}

struct Committed {
    name: String,
    arguments: Value,
    outcome: Outcome,
}

enum Work {
    /// A flow after the agent's call, holding the server's response to it,
    /// and the flow's run.
    Flow {
        response: Value,
        original: Vec<u8>,
        looked: Vec<Looked>,
        run: Run,
    },
    /// A commit's calls, in order.
    Commit {
        queue: VecDeque<(String, Value)>,
        done: Vec<Committed>,
    },
}

/// A flow's run as `run_async` interprets it: the program's value, and the
/// run's trace.
type RunFuture = Pin<Box<dyn Future<Output = (RunValue, Trace)>>>;

/// A flow's run in progress: its program's data, the program, interpreted
/// with `run_async` by a [`LiveRun`] that hands each site to the engine
/// through `mailbox`, and the lookup its last decision chose, until the
/// outcome site makes it.
struct Run {
    data: RunData,
    future: RunFuture,
    mailbox: Rc<RefCell<Mailbox>>,
    pending: Option<(String, Value)>,
}

/// What a run starts from: the call that just returned, whether it failed,
/// and the most lookups to make.
struct RunData {
    call: String,
    failed: bool,
    max_lookups: usize,
}

impl Run {
    fn new(data: RunData, program: &FlowProgram) -> Self {
        let model = program.run(&data.call, data.failed, data.max_lookups);
        let mailbox = Rc::new(RefCell::new(Mailbox::default()));
        let handler = LiveRun {
            mailbox: mailbox.clone(),
            trace: Trace::default(),
        };
        Self {
            data,
            future: Box::pin(run_async(handler, model)),
            mailbox,
            pending: None,
        }
    }
}

/// What a run asks the engine at a site, and the engine's answer.
#[derive(Default)]
struct Mailbox {
    ask: Option<Ask>,
    answer: Option<Answer>,
}

enum Ask {
    /// Decide at the decision site `address`: which of `site`'s options.
    Decide { address: String, site: DecideSite },
    /// Make the lookup the last decision chose.
    LookUp,
}

enum Answer {
    /// The option taken, an index into the site's options.
    Option(usize),
    /// Whether the lookup failed.
    Failed(bool),
}

/// The handler that executes a flow's run: the engine answers each site.
/// A decision is the arbiter's, and deterministic, so its propensity is 1
/// (`logp` 0). An outcome is the server's answer, scored under the
/// program's distribution like an observation, in the likelihood. A
/// program's own observations and factors are scored as any handler scores
/// them. The flow's distributions (`Decide`, `Outcome`) are the only ones a
/// program can use, so it has no site of another type.
struct LiveRun {
    mailbox: Rc<RefCell<Mailbox>>,
    trace: Trace,
}

impl LiveRun {
    fn ask(&self, ask: Ask) -> impl Future<Output = Answer> {
        self.mailbox.borrow_mut().ask = Some(ask);
        let mailbox = self.mailbox.clone();
        std::future::poll_fn(move |_| match mailbox.borrow_mut().answer.take() {
            Some(answer) => Poll::Ready(answer),
            None => Poll::Pending,
        })
    }
}

impl AsyncHandler for LiveRun {
    async fn on_sample_usize(&mut self, addr: &Address, dist: &dyn Distribution<usize>) -> usize {
        // A site with no options, such as one whose distribution the program
        // could not build, hands back.
        let site = dist
            .downcast_ref::<WithMeta<Categorical, DecideSite>>()
            .map(|d| d.meta().clone());
        let answer = match site {
            Some(site) => {
                self.ask(Ask::Decide {
                    address: addr.to_string(),
                    site,
                })
                .await
            }
            None => Answer::Option(HAND_BACK),
        };
        let choice = match answer {
            Answer::Option(i) => i,
            Answer::Failed(_) => HAND_BACK,
        };
        self.trace
            .insert_choice(addr.clone(), ChoiceValue::Usize(choice), 0.0);
        choice
    }

    async fn on_sample_bool(&mut self, addr: &Address, dist: &dyn Distribution<bool>) -> bool {
        let ok = match self.ask(Ask::LookUp).await {
            Answer::Failed(failed) => !failed,
            Answer::Option(_) => false,
        };
        let logp = dist.log_prob(&ok);
        self.trace.log_likelihood += logp;
        self.trace
            .insert_choice(addr.clone(), ChoiceValue::Bool(ok), logp);
        ok
    }

    async fn on_sample_f64(&mut self, addr: &Address, _: &dyn Distribution<f64>) -> f64 {
        unreachable!("a flow's program has no f64 site ({addr})")
    }

    async fn on_sample_u64(&mut self, addr: &Address, _: &dyn Distribution<u64>) -> u64 {
        unreachable!("a flow's program has no u64 site ({addr})")
    }

    async fn on_observe_f64(&mut self, _: &Address, dist: &dyn Distribution<f64>, value: f64) {
        self.trace.log_likelihood += dist.log_prob(&value);
    }

    async fn on_observe_bool(&mut self, _: &Address, dist: &dyn Distribution<bool>, value: bool) {
        self.trace.log_likelihood += dist.log_prob(&value);
    }

    async fn on_observe_u64(&mut self, _: &Address, dist: &dyn Distribution<u64>, value: u64) {
        self.trace.log_likelihood += dist.log_prob(&value);
    }

    async fn on_observe_usize(
        &mut self,
        _: &Address,
        dist: &dyn Distribution<usize>,
        value: usize,
    ) {
        self.trace.log_likelihood += dist.log_prob(&value);
    }

    async fn on_factor(&mut self, logw: f64) {
        self.trace.log_factors += logw;
    }

    async fn finish(self) -> Trace {
        self.trace
    }
}

/// A request of the proxy's own, awaiting the server.
struct Waiting {
    key: String,
    tool: String,
    arguments: Value,
    since: Instant,
}

struct Job {
    /// The id of the agent's request this job answers.
    client_id: Value,
    waiting: Option<Waiting>,
    work: Work,
}

/// The proxy in active mode.
pub(crate) struct Engine<'a, W: Write> {
    active: &'a Active,
    log: McpLog,
    tap: Option<Tap>,
    started: Instant,
    to_server: Option<Box<dyn Write>>,
    to_host: W,
    host_ok: bool,
    /// The agent's calls awaiting the server, by id.
    calls: HashSet<String>,
    /// The agent's `tools/list` requests awaiting the server, by id.
    lists: HashSet<String>,
    /// The server's tools and their `readOnlyHint`, once listed.
    server_tools: HashMap<String, Option<bool>>,
    /// The server's tools' input contracts, once listed
    /// ([`stretto_trace::mcp::contract`]).
    server_contracts: HashMap<String, String>,
    /// Tools the flow pins another contract for, reported once each.
    changed: HashSet<String>,
    jobs: Vec<Job>,
    /// Requests of the proxy's own it stopped waiting for.
    abandoned: HashSet<String>,
    next_id: u64,
    lookups: usize,
    questions: usize,
    context_read: u64,
    flow_log: Option<File>,
    confirm_log: Option<File>,
    confirm_questions: usize,
    /// The flow's program, which each of its runs interprets.
    program: Option<FlowProgram>,
}

impl<'a, W: Write> Engine<'a, W> {
    pub(crate) fn new(
        active: &'a Active,
        header: LogHeader,
        tap: Option<Tap>,
        started: Instant,
        (to_server, to_host): (Box<dyn Write>, W),
        (flow_log, confirm_log): (Option<PathBuf>, Option<PathBuf>),
    ) -> Self {
        let open = |path: &PathBuf, what: &str| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| eprintln!("stretto-proxy: {what} log {}: {e}", path.display()))
                .ok()
        };
        let flow_log = flow_log.as_ref().and_then(|path| open(path, "flow"));
        let confirm_log = confirm_log
            .as_ref()
            .and_then(|path| open(path, "confirmation"));
        Self {
            active,
            log: McpLog {
                header,
                entries: Vec::new(),
            },
            tap,
            started,
            to_server: Some(to_server),
            to_host,
            host_ok: true,
            calls: HashSet::new(),
            lists: HashSet::new(),
            server_tools: HashMap::new(),
            jobs: Vec::new(),
            abandoned: HashSet::new(),
            next_id: 0,
            server_contracts: HashMap::new(),
            changed: HashSet::new(),
            lookups: 0,
            questions: 0,
            context_read: 0,
            flow_log,
            confirm_log,
            confirm_questions: 0,
            // A flow from a file was checked when it loaded.
            program: active.flow.as_ref().and_then(|fc| {
                FlowProgram::new(&fc.flow)
                    .map_err(|e| eprintln!("stretto-proxy: {e:#}; the flow will not run"))
                    .ok()
            }),
        }
    }

    /// Serve until the server's output ends.
    pub(crate) fn run(mut self, rx: Receiver<Input>) {
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Input::Client(line)) => self.on_client(line),
                Ok(Input::Server(line)) => self.on_server(line),
                // Closing the server's input tells it to shut down.
                Ok(Input::ClientEnd) => {
                    self.read_context();
                    self.to_server = None;
                }
                Ok(Input::ServerEnd) | Err(RecvTimeoutError::Disconnected) => {
                    self.server_ended();
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.expire();
        }
    }

    // ---- the two sides ------------------------------------------------------------------

    fn on_client(&mut self, line: Vec<u8>) {
        let message = parse(&line);
        let id = message.as_ref().and_then(request_id).cloned();
        match (message.as_ref().and_then(method), id) {
            (Some("tools/list"), Some(id)) => {
                self.lists.insert(key(&id));
                self.record(Peer::Client, &line);
                self.send_server(&line);
            }
            (Some("tools/call"), Some(id)) => {
                // What was said before the call, logged before it.
                self.read_context();
                let m = message.as_ref().expect("a method means a message");
                let name = m
                    .pointer("/params/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let arguments = m
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if self.active.commit && name == COMMIT_TOOL {
                    self.record(Peer::Client, &line);
                    self.start_commit(id, &arguments);
                    return;
                }
                // Judged against what came before the call.
                let refusal = self.refusal(&name, &arguments);
                self.record(Peer::Client, &line);
                match refusal {
                    Some(reason) => self.refuse(id, &name, &reason),
                    None => {
                        self.calls.insert(key(&id));
                        self.send_server(&line);
                    }
                }
            }
            _ => {
                self.record(Peer::Client, &line);
                self.send_server(&line);
            }
        }
    }

    fn on_server(&mut self, line: Vec<u8>) {
        self.record(Peer::Server, &line);
        let Some(m) = parse(&line) else {
            return self.send_host(&line);
        };
        let Some(id) = response_id(&m).cloned() else {
            return self.send_host(&line);
        };
        let k = key(&id);
        let mine = self
            .jobs
            .iter()
            .position(|j| j.waiting.as_ref().is_some_and(|w| w.key == k));
        if let Some(i) = mine {
            let job = self.jobs.swap_remove(i);
            return self.resume(job, &m);
        }
        if self.abandoned.remove(&k) {
            return;
        }
        if self.lists.remove(&k) {
            self.learn_tools(&m);
            if self.active.commit {
                return self.send_own(with_commit_tool(m));
            }
            return self.send_host(&line);
        }
        if self.calls.remove(&k) && self.flow_may_follow(&m) {
            if let Some(run) = self.flow_run() {
                let job = Job {
                    client_id: id,
                    waiting: None,
                    work: Work::Flow {
                        response: m,
                        original: line,
                        looked: Vec::new(),
                        run,
                    },
                };
                return self.drive(job);
            }
        }
        self.send_host(&line);
    }

    fn server_ended(&mut self) {
        self.to_server = None;
        for mut job in std::mem::take(&mut self.jobs) {
            if let (Some(w), Work::Commit { done, .. }) = (job.waiting.take(), &mut job.work) {
                done.push(Committed {
                    name: w.tool,
                    arguments: w.arguments,
                    outcome: Outcome::NoAnswer,
                });
            }
            self.finish(job);
        }
    }

    /// Give up on requests of the proxy's own past their patience.
    fn expire(&mut self) {
        while let Some(i) = self.jobs.iter().position(|j| {
            j.waiting
                .as_ref()
                .is_some_and(|w| w.since.elapsed() > PATIENCE)
        }) {
            let mut job = self.jobs.swap_remove(i);
            let w = job.waiting.take().expect("found waiting");
            eprintln!(
                "stretto-proxy: no answer to {} within {} s; going on without it",
                w.tool,
                PATIENCE.as_secs()
            );
            self.abandoned.insert(w.key);
            if let Work::Commit { done, .. } = &mut job.work {
                done.push(Committed {
                    name: w.tool,
                    arguments: w.arguments,
                    outcome: Outcome::NoAnswer,
                });
            }
            self.finish(job);
        }
    }

    // ---- jobs ---------------------------------------------------------------------------

    fn flow_may_follow(&self, response: &Value) -> bool {
        let Some(fc) = &self.active.flow else {
            return false;
        };
        response.get("result").is_some()
            && !self
                .jobs
                .iter()
                .any(|j| matches!(j.work, Work::Flow { .. }))
            && self.lookups < fc.per_session
            && self.questions < fc.max_questions
    }

    fn start_commit(&mut self, id: Value, arguments: &Value) {
        let calls: Option<VecDeque<(String, Value)>> = arguments
            .get("calls")
            .and_then(Value::as_array)
            .filter(|calls| !calls.is_empty())
            .and_then(|calls| {
                calls
                    .iter()
                    .map(|c| {
                        let name = c.get("name").and_then(Value::as_str)?;
                        let args = c.get("arguments").cloned().unwrap_or_else(|| json!({}));
                        (name != COMMIT_TOOL && args.is_object()).then(|| (name.to_string(), args))
                    })
                    .collect()
            });
        match calls {
            Some(queue) => self.step(Job {
                client_id: id,
                waiting: None,
                work: Work::Commit {
                    queue,
                    done: Vec::new(),
                },
            }),
            None => self.send_own(tool_result(
                id,
                &format!(
                    "{COMMIT_TOOL} takes {{\"calls\": [{{\"name\": tool, \"arguments\": {{...}}}}, ...]}}, \
                     one or more calls of the server's tools; nothing was run."
                ),
                true,
            )),
        }
    }

    /// The flow's run after the agent's call that just returned: its
    /// program, from the call's site. `None` when the session's last step is
    /// not a tool call, so there is no site to start from.
    fn flow_run(&self) -> Option<Run> {
        let fc = self.active.flow.as_ref()?;
        let program = self.program.as_ref()?;
        let episode = self.episode();
        let steps = stretto_model::steps(&episode);
        let last = steps.last()?;
        let Action::Tool(tool) = &last.action else {
            return None;
        };
        let data = RunData {
            call: tool.clone(),
            failed: last.outcome == StepOutcome::Err,
            max_lookups: fc.per_call,
        };
        Some(Run::new(data, program))
    }

    /// Run the job's flow until it waits on the server or ends: each
    /// decision it asks for is made here, and each lookup it asks for is
    /// sent, the run resuming when the server answers ([`Self::resume`]).
    fn drive(&mut self, mut job: Job) {
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            let Work::Flow { run, .. } = &mut job.work else {
                unreachable!("only flow jobs have a run")
            };
            if let Poll::Ready((_, trace)) = run.future.as_mut().poll(&mut cx) {
                let entry = run_entry(&job.client_id, &run.data, &trace);
                self.log_flow(&entry);
                return self.finish(job);
            }
            let mailbox = run.mailbox.clone();
            let ask = mailbox.borrow_mut().ask.take();
            match ask {
                Some(Ask::Decide { address, site }) => {
                    let choice = self.decide(&mut job, &address, &site);
                    mailbox.borrow_mut().answer = Some(Answer::Option(choice));
                }
                Some(Ask::LookUp) => {
                    let Work::Flow { run, .. } = &mut job.work else {
                        unreachable!("only flow jobs have a run")
                    };
                    match run.pending.take() {
                        Some((tool, arguments)) => return self.request(job, tool, arguments),
                        None => mailbox.borrow_mut().answer = Some(Answer::Failed(true)),
                    }
                }
                // A run that waits on nothing cannot go on.
                None => return self.finish(job),
            }
        }
    }

    /// Decide at a flow's decision site `address`: the arbiter's choice
    /// among `site`'s options, logged. A lookup is taken only within the
    /// session's limits, outside shadow mode, and when the server has the
    /// tool and does not mark it as a write; otherwise the flow hands back.
    fn decide(&mut self, job: &mut Job, address: &str, site: &DecideSite) -> usize {
        let active = self.active;
        let fc = active.flow.as_ref().expect("flow jobs need a flow");
        let Work::Flow { looked, run, .. } = &mut job.work else {
            unreachable!("only flow jobs decide")
        };
        if looked.len() >= fc.per_call
            || self.lookups >= fc.per_session
            || self.questions >= fc.max_questions
        {
            return HAND_BACK;
        }
        self.read_context();
        let episode = self.episode();
        let asked = Instant::now();
        let next = match fc.flow.next_explored(
            &episode,
            fc.oracle.as_ref(),
            fc.threshold,
            fc.decider,
            fc.explore,
        ) {
            Ok(next) => next,
            Err(e) => {
                eprintln!("stretto-proxy: the flow failed, handing back: {e:#}");
                return HAND_BACK;
            }
        };
        self.questions += usize::from(next.key.is_some());
        let mut entry = serde_json::to_value(&next).unwrap_or(Value::Null);
        entry["after"] = job.client_id.clone();
        entry["address"] = json!(address);
        entry["ms"] = json!(asked.elapsed().as_millis() as u64);
        if fc.shadow {
            entry["shadow"] = json!(true);
        }
        self.log_flow(&entry);
        match next.proposal {
            // In shadow mode the decision is only logged, and the agent gets
            // its result as the server sent it.
            Proposal::Lookup { tool, arguments } if !fc.shadow && self.may_look_up(fc, &tool) => {
                let Some(choice) = site.options.iter().position(|o| *o == tool) else {
                    eprintln!(
                        "stretto-proxy: the flow chose {tool}, which its program does not offer at {}; handing back",
                        site.site
                    );
                    return HAND_BACK;
                };
                self.lookups += 1;
                run.pending = Some((tool, arguments));
                choice
            }
            _ => HAND_BACK,
        }
    }

    /// Take a commit's next step: a request of the proxy's own, or its end.
    fn step(&mut self, mut job: Job) {
        match &mut job.work {
            Work::Flow { .. } => self.drive(job),
            Work::Commit { queue, done } => {
                let Some((name, arguments)) = queue.pop_front() else {
                    return self.finish(job);
                };
                if let Some(reason) = self.refusal(&name, &arguments) {
                    done.push(Committed {
                        name,
                        arguments,
                        outcome: Outcome::Refused(reason),
                    });
                    return self.finish(job);
                }
                self.request(job, name, arguments);
            }
        }
    }

    /// Send the server a request of the proxy's own for `job`, and wait.
    fn request(&mut self, mut job: Job, tool: String, arguments: Value) {
        if self.to_server.is_none() {
            if let Work::Commit { done, .. } = &mut job.work {
                done.push(Committed {
                    name: tool,
                    arguments,
                    outcome: Outcome::NoAnswer,
                });
            }
            return self.finish(job);
        }
        self.next_id += 1;
        let id = json!(format!("stretto-{}", self.next_id));
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments},
        });
        let line = line_of(&message);
        self.record(Peer::Proxy, &line);
        self.send_server(&line);
        job.waiting = Some(Waiting {
            key: key(&id),
            tool,
            arguments,
            since: Instant::now(),
        });
        self.jobs.push(job);
    }

    /// The server answered the job's request.
    fn resume(&mut self, mut job: Job, response: &Value) {
        let w = job.waiting.take().expect("resumed jobs wait");
        let (text, error) = result_text(response);
        match &mut job.work {
            Work::Flow { looked, run, .. } => {
                looked.push(Looked {
                    tool: w.tool,
                    arguments: w.arguments,
                    text,
                    error,
                });
                run.mailbox.borrow_mut().answer = Some(Answer::Failed(error));
                self.drive(job);
            }
            Work::Commit { done, .. } => {
                done.push(Committed {
                    name: w.tool,
                    arguments: w.arguments,
                    outcome: Outcome::Done { text, error },
                });
                if error {
                    self.finish(job);
                } else {
                    self.step(job);
                }
            }
        }
    }

    /// Answer the agent's request the job holds.
    fn finish(&mut self, job: Job) {
        match job.work {
            Work::Flow {
                response,
                original,
                looked,
                ..
            } => {
                if looked.is_empty() {
                    return self.send_host(&original);
                }
                self.send_own(with_appendix(response, &looked));
            }
            Work::Commit { queue, done } => {
                let failed = done
                    .iter()
                    .any(|c| !matches!(c.outcome, Outcome::Done { error: false, .. }));
                let mut parts: Vec<String> = done
                    .iter()
                    .map(|c| {
                        let call = format!("{} {}", c.name, c.arguments);
                        match &c.outcome {
                            Outcome::Done { text, error: false } => format!("{call}:\n{text}"),
                            Outcome::Done { text, error: true } => {
                                format!("{call} (error):\n{text}")
                            }
                            Outcome::Refused(reason) => {
                                format!("{call} was refused by the policy check: {reason}")
                            }
                            Outcome::NoAnswer => format!("{call} got no answer from the server"),
                        }
                    })
                    .collect();
                parts.extend(
                    queue
                        .iter()
                        .map(|(name, arguments)| format!("{name} {arguments} was not run")),
                );
                self.send_own(tool_result(job.client_id, &parts.join("\n\n"), failed));
            }
        }
    }

    // ---- guards and flows ---------------------------------------------------------------

    /// Why an enforced guard, or the enforced confirmation judge, refuses
    /// `name(arguments)` now, if one does.
    fn refusal(&mut self, name: &str, arguments: &Value) -> Option<String> {
        let guards = self.active.guards.as_ref()?;
        self.read_context();
        let call = ToolCall {
            id: "pending".to_string(),
            name: name.to_string(),
            arguments: arguments.clone(),
        };
        let episode = self.episode();
        if let Some(reason) = guards.refusal(&episode, &call) {
            return Some(reason);
        }
        self.judge(guards, &episode, &call)
    }

    /// Put `call` to the confirmation judge, if the guards check its
    /// confirmation, log the judgment, and say why it is refused if the judge
    /// is enforced and fails it.
    fn judge(&mut self, guards: &Guards, episode: &Episode, call: &ToolCall) -> Option<String> {
        let cc = self.active.confirm.as_ref()?;
        let word_list = confirm::word_list(guards, episode, call)?;
        let asked = Instant::now();
        let (proposal, customer) = confirm::exchange(episode);
        let first = confirm::request(&cc.model, &proposal, &customer, call);
        // Each question, and whether its answer counts towards the judgment.
        let mut questions = vec![("p_yes", "key", confirm::QUESTION, first.clone(), true)];
        if let Some(second) = cc.second {
            let request = confirm::second_request(&first, second);
            questions.push((
                "p_second",
                "second_key",
                second.id(),
                request,
                !cc.second_shadow,
            ));
        }
        let mut entry = json!({
            "tool": call.name,
            "arguments": call.arguments,
            "word_list": word_list,
            "enforced": cc.enforce,
        });
        if cc.second_shadow {
            entry["second_shadow"] = json!(true);
        }
        let (mut fails, mut unknown) = (Vec::new(), false);
        for (field, key_field, id, request, counts) in &questions {
            entry[*key_field] = json!(request_key(request));
            if self.confirm_questions >= cc.max_questions {
                entry["skipped"] = json!("the session's question budget is spent");
                unknown |= *counts;
                break;
            }
            self.confirm_questions += 1;
            match cc.oracle.ask(request) {
                Ok(response) => match confirm::yes(&response, id) {
                    Some(p) => {
                        entry[*field] = json!(p);
                        if p < cc.threshold && *counts {
                            fails.push(format!("{field} {p:.2}"));
                        }
                    }
                    None => {
                        entry["error"] = json!(format!("no yes/no answer to {id}"));
                        unknown |= *counts;
                    }
                },
                Err(e) => {
                    eprintln!("stretto-proxy: the confirmation judge could not answer: {e:#}");
                    entry["error"] = json!(format!("{e:#}"));
                    unknown |= *counts;
                }
            }
        }
        // An answer that counts and fails is enough; a judge that could not
        // answer refuses nothing.
        entry["fails"] = json!(!fails.is_empty());
        entry["unknown"] = json!(fails.is_empty() && unknown);
        entry["ms"] = json!(asked.elapsed().as_millis() as u64);
        if let Some(f) = self.confirm_log.as_mut() {
            let _ = writeln!(f, "{entry}");
        }
        (cc.enforce && !fails.is_empty()).then(|| {
            format!(
                "the customer has not explicitly agreed to this exact change (confirmation judge: {}); \
                 describe the change and ask the customer to confirm it first",
                fails.join(", ")
            )
        })
    }

    fn refuse(&mut self, id: Value, name: &str, reason: &str) {
        eprintln!("stretto-proxy: refused {name}: {reason}");
        self.send_own(tool_result(
            id,
            &format!(
                "Refused by the policy check, so {name} was not run and nothing changed: {reason}."
            ),
            true,
        ));
    }

    /// Append an entry to the flow log.
    fn log_flow(&mut self, entry: &Value) {
        if let Some(f) = self.flow_log.as_mut() {
            let _ = writeln!(f, "{entry}");
        }
    }

    /// Whether the flow may call `tool`: the flow reads it as a lookup, and
    /// the server, once it has listed its tools, has it, does not mark it as
    /// a write, and lists the input contract the flow pinned for it, if any.
    /// A tool whose arguments changed since the flow learned to bind them is
    /// left to the agent.
    fn may_look_up(&mut self, fc: &FlowConfig, tool: &str) -> bool {
        let flow_reads = fc.flow.manifest().tools.get(tool) == Some(&ToolKind::Read);
        let server_allows = match self.server_tools.get(tool) {
            Some(Some(false)) => false,
            Some(_) => true,
            None => self.server_tools.is_empty(),
        };
        let changed = match (
            fc.flow.contracts().get(tool),
            self.server_contracts.get(tool),
        ) {
            (Some(pinned), Some(now)) => pinned != now,
            _ => false,
        };
        if changed && self.changed.insert(tool.to_string()) {
            eprintln!(
                "stretto-proxy: {tool}'s input changed since the flow was learned ({} → {}); the flow no longer looks it up",
                fc.flow.contracts()[tool],
                self.server_contracts[tool]
            );
        }
        flow_reads && server_allows && !changed
    }

    fn learn_tools(&mut self, response: &Value) {
        let listed = response.pointer("/result/tools").and_then(Value::as_array);
        for tool in listed.into_iter().flatten() {
            if let Some(name) = tool.get("name").and_then(Value::as_str) {
                let hint = tool
                    .pointer("/annotations/readOnlyHint")
                    .and_then(Value::as_bool);
                self.server_tools.insert(name.to_string(), hint);
                if let Some(c) = mcp::contract(tool) {
                    self.server_contracts.insert(name.to_string(), c);
                }
            }
        }
    }

    /// The session so far, as an episode.
    fn episode(&self) -> Episode {
        let mut episode = mcp::episode(&self.log);
        episode.task_id = self
            .active
            .task_id
            .clone()
            .unwrap_or_else(|| episode.id.clone());
        episode
    }

    /// Log the conversation's new lines, if the host keeps one.
    fn read_context(&mut self) {
        let Some(path) = &self.active.context else {
            return;
        };
        // No file yet means nothing said yet.
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let mut text = Vec::new();
        if file.seek(SeekFrom::Start(self.context_read)).is_err()
            || file.read_to_end(&mut text).is_err()
        {
            return;
        }
        // Only whole lines: the host may be halfway through writing one.
        let Some(end) = text.iter().rposition(|&b| b == b'\n') else {
            return;
        };
        self.context_read += end as u64 + 1;
        for line in text[..=end].split_inclusive(|&b| b == b'\n') {
            if !line.iter().all(u8::is_ascii_whitespace) {
                self.record(Peer::Context, line);
            }
        }
    }

    // ---- the wire and the log -----------------------------------------------------------

    fn record(&mut self, from: Peer, line: &[u8]) {
        if let Some(tap) = &self.tap {
            tap.record(from, line);
        }
        let (message, raw) = match parse_any(line) {
            Some(v) => (Some(v), None),
            None => (
                None,
                Some(
                    String::from_utf8_lossy(line)
                        .trim_end_matches(['\n', '\r'])
                        .to_string(),
                ),
            ),
        };
        self.log.entries.push(LogEntry {
            t_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            from,
            message,
            raw,
        });
    }

    fn send_server(&mut self, line: &[u8]) {
        if let Some(to) = self.to_server.as_mut() {
            if let Err(e) = to.write_all(line).and_then(|()| to.flush()) {
                eprintln!("stretto-proxy: forwarding to the server: {e}");
                self.to_server = None;
            }
        }
    }

    fn send_host(&mut self, line: &[u8]) {
        if self.host_ok {
            if let Err(e) = self
                .to_host
                .write_all(line)
                .and_then(|()| self.to_host.flush())
            {
                eprintln!("stretto-proxy: forwarding to the client: {e}");
                self.host_ok = false;
            }
        }
    }

    /// Send the host a message of the proxy's own, and log it.
    fn send_own(&mut self, message: Value) {
        let line = line_of(&message);
        self.record(Peer::Proxy, &line);
        self.send_host(&line);
    }
}

// ---- messages -------------------------------------------------------------------------------

fn parse_any(line: &[u8]) -> Option<Value> {
    serde_json::from_slice(line).ok()
}

/// One JSON-RPC message (batches are left alone).
fn parse(line: &[u8]) -> Option<Value> {
    parse_any(line).filter(Value::is_object)
}

fn method(m: &Value) -> Option<&str> {
    m.get("method").and_then(Value::as_str)
}

fn request_id(m: &Value) -> Option<&Value> {
    m.get("id")
        .filter(|id| !id.is_null() && m.get("method").is_some())
}

fn response_id(m: &Value) -> Option<&Value> {
    m.get("id").filter(|id| {
        !id.is_null()
            && m.get("method").is_none()
            && (m.get("result").is_some() || m.get("error").is_some())
    })
}

/// A map key for an id: `1` and `"1"` are different ids.
fn key(id: &Value) -> String {
    id.to_string()
}

fn line_of(message: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(message).expect("messages serialize");
    line.push(b'\n');
    line
}

/// A `tools/call` result with one text item.
fn tool_result(id: Value, text: &str, error: bool) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": {
        "content": [{"type": "text", "text": text}],
        "isError": error,
    }})
}

/// A `tools/call` response's text, and whether it failed.
fn result_text(response: &Value) -> (String, bool) {
    // As the session's episode reads it (a null error is none), so a
    // flow's program and its arbiter agree on whether a lookup failed.
    if let Some(e) = response.get("error").filter(|e| !e.is_null()) {
        let message = e.get("message").and_then(Value::as_str).unwrap_or("error");
        return (message.to_string(), true);
    }
    let result = &response["result"];
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let error = result.get("isError").and_then(Value::as_bool) == Some(true);
    (text, error)
}

/// A flow's run, for the flow log: its program's data, each site with its
/// value and log-probability in the order the standard program visits them,
/// and the surprise of the server's answers, `-ln p` of the outcomes under
/// the flow's statistics, in nats. Given the data, the flow's program can
/// score the run again with `ScoreGivenTrace`.
fn run_entry(after: &Value, data: &RunData, trace: &Trace) -> Value {
    let mut sites: Vec<(&Address, &Choice)> = trace.choices.iter().collect();
    // `decide#i` before `outcome#i`, and step 2 before step 10.
    let step = |a: &Address| {
        let name = a.to_string();
        let i = name
            .rsplit_once('#')
            .and_then(|(_, i)| i.parse::<usize>().ok());
        (i, name)
    };
    sites.sort_by_key(|(a, _)| step(a));
    let sites: Vec<Value> = sites
        .into_iter()
        .map(|(a, c)| {
            let value = match &c.value {
                ChoiceValue::Usize(v) => json!(v),
                ChoiceValue::Bool(v) => json!(v),
                ChoiceValue::U64(v) => json!(v),
                ChoiceValue::I64(v) => json!(v),
                ChoiceValue::F64(v) => json!(v),
            };
            json!([a.to_string(), value, c.logp])
        })
        .collect();
    json!({
        "after": after,
        "run": {
            "call": data.call,
            "failed": data.failed,
            "max_lookups": data.max_lookups,
            "sites": sites,
            "surprise": -trace.log_likelihood,
        },
    })
}

/// The agent's result with the flow's lookups appended, as the pilot
/// showed them.
fn with_appendix(mut response: Value, looked: &[Looked]) -> Value {
    let mut text = APPENDIX.to_string();
    for l in looked {
        let status = if l.error { " (error)" } else { "" };
        text.push_str(&format!(
            "\n\n{} {}{status}:\n{}",
            l.tool, l.arguments, l.text
        ));
    }
    let item = json!({"type": "text", "text": text});
    match response.pointer_mut("/result/content") {
        Some(Value::Array(content)) => content.push(item),
        _ => response["result"]["content"] = json!([item]),
    }
    response
}

/// A `tools/list` response with the commit tool added.
fn with_commit_tool(mut response: Value) -> Value {
    let tool = json!({
        "name": COMMIT_TOOL,
        "description": "Make several changes in one call, in order, once the user has confirmed all of them. \
            Each call is checked against the policy first; the first call that is refused or fails \
            stops the rest. Returns each call's result.",
        "inputSchema": {
            "type": "object",
            "properties": {"calls": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "One of the other tools."},
                        "arguments": {"type": "object"}
                    },
                    "required": ["name", "arguments"]
                }
            }},
            "required": ["calls"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": true}
    });
    if let Some(Value::Array(tools)) = response.pointer_mut("/result/tools") {
        tools.push(tool);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(text
            .contains("\n\nget_user_details {\"user_id\":\"user_7\"}:\n{\"orders\":[\"#W7a\"]}"));
        assert!(
            text.contains("\n\nget_order_details {\"order_id\":\"#W9\"} (error):\nOrder not found")
        );
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
    }
}
