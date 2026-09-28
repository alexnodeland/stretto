//! Every command of the `stretto` binary, run in this process on miniature
//! data: what it writes, and how it fails.

use super::{run, Cli, Env};
use anyhow::Result;
use clap::{error::ErrorKind, Parser};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use stretto_report::init::Host;

#[path = "../tests/common/mod.rs"]
mod common;

/// A directory of a test's own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("stretto-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    /// A path in it.
    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    /// Write `text` to `rel`, with its directories.
    fn put(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.at(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    fn get(&self, rel: &str) -> String {
        fs::read_to_string(self.at(rel)).unwrap()
    }

    /// `stretto ARGS`, in a test's surroundings (see [`stretto`]).
    fn run(&self, args: &str) -> Outcome {
        stretto(&self.0, args, "", |_| {})
    }

    /// `stretto ARGS`, which must fail saying `error`.
    fn fails(&self, args: &str, error: &str) {
        let said = self.run(args).err();
        assert!(said.contains(error), "{args}: {said}");
    }

    /// The shop's manifest in `manifest.json`, and `n` of its sessions in
    /// `s/`.
    fn shop(&self, n: usize) {
        let manifest = serde_json::to_string(&common::manifest()).unwrap();
        self.put("manifest.json", &manifest);
        let sessions: Vec<_> = (0..n).map(common::session).collect();
        common::write_sessions(&self.at("s"), &sessions);
    }

    /// A flow of the habit alone, learned from the shop's sessions:
    /// `$T/habit.flow.json`.
    fn habit_flow(&self) -> &'static str {
        self.run(&format!(
            "{LEARN_SHOP} --habit-only --out $T/habit.flow.json"
        ))
        .ok();
        "$T/habit.flow.json"
    }

    /// A flow with an arbiter fitted on the mock oracle's answers, learned
    /// from the shop's sessions: `$T/arbiter.flow.json`.
    fn arbiter_flow(&self) -> &'static str {
        self.put("p.json", &predicates(&["p1"]));
        let args = "--oracle mock --predicates $T/p.json --out $T/arbiter.flow.json";
        self.run(&format!("{LEARN_SHOP} {args}")).ok();
        "$T/arbiter.flow.json"
    }

    /// The habit flow as a flow learned before flows held reach's counts:
    /// `$T/old.flow.json`.
    fn old_flow(&self) -> &'static str {
        let mut flow: Value = serde_json::from_str(&self.get("habit.flow.json")).unwrap();
        flow.as_object_mut().unwrap().remove("reach");
        self.put("old.flow.json", &flow.to_string());
        "$T/old.flow.json"
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `learn` on the shop's sessions.
const LEARN_SHOP: &str = "learn --domain shop --manifest $T/manifest.json --sessions $T/s";

/// What a command did: its exit code or its error, and what it wrote to
/// stdout.
struct Outcome {
    code: Result<i32>,
    stdout: String,
}

impl Outcome {
    /// What it wrote, once it exited with `code`.
    fn exits(self, code: i32) -> String {
        let Outcome { code: got, stdout } = self;
        assert_eq!(got.unwrap(), code, "{stdout}");
        stdout
    }

    fn ok(self) -> String {
        self.exits(0)
    }

    /// Its error, with its causes.
    fn err(self) -> String {
        format!("{:#}", self.code.unwrap_err())
    }
}

/// The surroundings of a test: HOME is `dir/home`, PATH is empty, no key is
/// set, and Jev is down.
fn test_env<'a>(dir: &Path, stdin: &'a mut &[u8], stdout: &'a mut Vec<u8>) -> Env<'a> {
    Env {
        stdin,
        stdout,
        home: Some(dir.join("home")),
        path: OsString::new(),
        exe: None,
        key: [false; 2],
        jev: jev_down,
    }
}

/// `stretto ARGS` in this process: ARGS split at whitespace, with `$T` in
/// them replaced by `dir`, `stdin` its input, and [`test_env`]'s
/// surroundings as `set` changes them.
fn stretto(dir: &Path, args: &str, stdin: &str, set: impl FnOnce(&mut Env)) -> Outcome {
    let args = args.replace("$T", &dir.display().to_string());
    let words = std::iter::once("stretto").chain(args.split_whitespace());
    let cli = Cli::try_parse_from(words).unwrap();
    let (mut input, mut output) = (stdin.as_bytes(), Vec::new());
    let mut env = test_env(dir, &mut input, &mut output);
    set(&mut env);
    let code = run(cli, &mut env);
    let stdout = String::from_utf8(output).unwrap();
    Outcome { code, stdout }
}

fn jev_down() -> Result<String> {
    anyhow::bail!("Jev is down")
}

fn jev_up() -> Result<String> {
    Ok("Jev OK: model jev-test".to_string())
}

/// A predicate file of `ids`, each asking whether to look anything up.
fn predicates(ids: &[&str]) -> String {
    let predicates: Vec<Value> = ids
        .iter()
        .map(|id| json!({"id": id, "favors": "any_lookup", "question": "Look it up?", "yes": "Yes.", "no": "No."}))
        .collect();
    json!({ "predicates": predicates }).to_string()
}

/// A port nothing listens on yet.
fn free_port() -> u16 {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    port
}

/// `line` asked of the flow served on `port`, once it listens: its answer.
fn ask_flow(port: u16, line: &str) -> Value {
    let mut stream = (0..500)
        .find_map(|_| {
            std::thread::sleep(Duration::from_millis(20));
            TcpStream::connect(("127.0.0.1", port)).ok()
        })
        .expect("the flow listens");
    stream.write_all(line.as_bytes()).unwrap();
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply).unwrap();
    serde_json::from_str(&reply).unwrap()
}

/// A τ²-bench conversation that ends with the result of `tool`, as a query
/// to a served flow.
fn query(user: &str, tool: &str, arguments: Value, result: &str) -> String {
    let messages = json!([
        {"role": "user", "content": user},
        {"role": "assistant", "tool_calls": [{"id": "a", "name": tool, "arguments": arguments,
            "requestor": "assistant"}]},
        {"role": "tool", "id": "a", "content": result, "error": false}
    ]);
    format!("{}\n", json!({"task_id": "9", "messages": messages}))
}

#[test]
fn phase0_reports_on_a_checkout_and_refuses_options_that_do_not_fit() {
    let t = Scratch::new("phase0");
    common::write_checkout(&t.0);
    let base = "phase0 --tau2 $T --domain retail --alpha-samples 0";
    let targets =
        "--source $T/targets/model-t_retail.json --target $T/targets/model-t_airline.json";
    let md = t.run(&format!("{base} {targets}")).ok();
    assert!(md.contains("## retail"), "{md}");
    t.run(&format!("{base} --out $T/out/p0.md --json $T/out/p0.json"))
        .ok();
    assert!(t.get("out/p0.md").contains("## retail"));
    let report: Value = serde_json::from_str(&t.get("out/p0.json")).unwrap();
    assert_eq!(report["domains"][0]["train_tasks"], 6);
    // Phase 0b, with the v2 questions, predicates and a candidate weighed.
    t.put("p.json", &predicates(&["p1"]));
    t.put("c.json", &predicates(&["c1"]));
    t.put("bad.json", "not json");
    let v2 = format!("{base} --oracle mock --questions v2 --predicates $T/p.json");
    let asked = "--candidates $T/c.json --weigh c1 --oracle-model m --oracle-log $T/log.jsonl";
    let md = t.run(&format!("{v2} {asked}")).ok();
    assert!(md.contains("Questions v2"), "{md}");
    assert!(t.at("log-retail.jsonl").exists());
    for (args, error) in [
        (
            "--oracle mock --pooled-arbiter",
            "--pooled-arbiter shapes a flow's arbiter",
        ),
        ("--oracle jev --no-features", "--oracle needs code features"),
        ("--train-fraction 0", "--train-fraction must be in (0, 1]"),
        ("--oracle mock --predicates $T/none.json", "reading"),
        ("--oracle mock --predicates $T/bad.json", "parsing"),
        ("--oracle mock --candidates $T/none.json", "reading"),
        ("--oracle mock --candidates $T/bad.json", "parsing"),
        (
            "--oracle mock --candidates $T/c.json",
            "--candidates needs --questions v2",
        ),
    ] {
        t.fails(&format!("{base} {args}"), error);
    }
    t.fails(
        &format!("{v2} --candidates $T/p.json"),
        "candidate `p1` is already a predicate",
    );
    t.fails(
        &format!("{v2} --candidates $T/c.json --weigh c2"),
        "--weigh c2: no such candidate",
    );
}

#[test]
fn compile_writes_one_domains_flow_and_flow_serve_serves_it() {
    let t = Scratch::new("compile");
    common::write_checkout(&t.0);
    t.put("c.json", &predicates(&["c1"]));
    let data = "--tau2 $T --domain retail --alpha-samples 0 --oracle replay --questions v2 \
                --oracle-cache $T/cache";
    t.run(&format!("compile {data} --out $T/flows/retail.flow.json"))
        .ok();
    let flow = stretto_report::flow::Flow::load(&t.at("flows/retail.flow.json")).unwrap();
    assert_eq!(flow.domain(), "retail");
    let one = "--tau2 $T --domain retail";
    for (args, error) in [
        (
            "compile --tau2 $T --out $T/f.json".to_string(),
            "compile builds one domain's flow",
        ),
        (
            "flow-serve --tau2 $T".to_string(),
            "flow-serve serves one domain",
        ),
        (
            format!("compile {one} --out $T/f.json"),
            "compiling a flow needs --oracle",
        ),
        (
            format!("compile {one} --oracle mock --out $T/f.json"),
            "flows ask the v2 questions",
        ),
        (
            format!("compile {one} --oracle mock --questions v2 --candidates $T/c.json --out $T/f"),
            "--candidates is for phase0 and refine",
        ),
    ] {
        t.fails(&args, error);
    }
    // Served until the tests end.
    let port = free_port();
    let (dir, args) = (
        t.0.clone(),
        format!("flow-serve {data} --surprise 3 --listen 127.0.0.1:{port}"),
    );
    std::thread::spawn(move || stretto(&dir, &args, "", |_| {}));
    let call = query(
        "Cancel #W0001, I am a@b.com",
        "find_user",
        json!({"email": "a@b.com"}),
        "\"u_1\"",
    );
    let answer = ask_flow(port, &call);
    assert!(answer["action"].is_string(), "{answer}");
}

#[test]
fn learn_takes_sessions_or_results_and_an_arbiter_from_either_file() {
    let t = Scratch::new("learn");
    t.shop(30);
    t.put("rewards.json", r#"{"session-3": 0.0}"#);
    t.put("bad.json", "not json");
    let habit = "--habit-only --rewards $T/rewards.json --out $T/habit.flow.json";
    t.run(&format!("{LEARN_SHOP} {habit}")).ok();
    let arbiter = t.arbiter_flow();
    t.run(&format!(
        "{LEARN_SHOP} --oracle mock --out $T/plain.flow.json"
    ))
    .ok();
    // Without a manifest, the tools are the sessions' own tools/list: none.
    t.run("learn --domain shop --sessions $T/s --habit-only --out $T/bare.flow.json")
        .ok();
    let bare = stretto_report::flow::Flow::load(&t.at("bare.flow.json")).unwrap();
    assert!(bare.sites().is_empty());
    // An arbiter from its own file, or from another flow.
    t.run(&format!(
        "export-arbiter --flow {arbiter} --out $T/shop.arbiter.json"
    ))
    .ok();
    for from in ["$T/shop.arbiter.json", arbiter] {
        let args = format!("{LEARN_SHOP} --arbiter-from {from} --out $T/from.flow.json");
        t.run(&args).ok();
        let flow = stretto_report::flow::Flow::load(&t.at("from.flow.json")).unwrap();
        assert!(flow.has_arbiter());
    }
    for (args, error) in [
        ("--arbiter-from $T/none.json", "reading"),
        ("--arbiter-from $T/bad.json", "parsing"),
        ("--oracle mock --rewards $T/none.json", "reading"),
        ("--oracle mock --rewards $T/bad.json", "expected"),
        (
            "--oracle mock --trials 1",
            "--train-fraction, --train-tasks and --trials apply to --results",
        ),
        (
            "--oracle mock --train-fraction 0",
            "--train-fraction must be in (0, 1]",
        ),
    ] {
        t.fails(&format!("{LEARN_SHOP} {args} --out $T/x.json"), error);
    }
    // τ²-bench results in place of sessions: the checkout's training tasks.
    common::write_checkout(&t.0);
    let results = "--domain retail --habit-only --tau2 $T --results \
                   $T/data/tau2/results/final/model-a_retail_default_user-sim_2trials.json";
    let args = format!("learn {results} --train-tasks 0,1,2 --trials 0 --out $T/r.flow.json");
    t.run(&args).ok();
    t.fails(
        "learn --domain retail --habit-only --tau2 $T --results \
         $T/targets/model-t_airline.json --out $T/x.json",
        "is for airline, not retail",
    );
    t.fails(
        &format!("learn {results} --trials 7 --out $T/x.json"),
        "no episodes on the sampled training tasks",
    );
    let other = "learn --habit-only --tau2 $T --out $T/x.json --domain";
    t.fails(&format!("{other} retail --results $T/none.json"), "reading");
    t.fails(
        &format!("{other} airline --results $T/none.json"),
        "tools.py",
    );
}

#[test]
fn learn_and_audit_read_a_host_sessions_servers_as_one_session() {
    let t = Scratch::new("servers");
    for n in 0..30 {
        for (name, text) in common::host_session_logs(&common::session(n), n) {
            t.put(&format!("m/{name}"), &text);
        }
    }
    // A host session with two servers of one name at once stays apart.
    for (name, text) in common::host_session_logs(&common::session(30), 30) {
        t.put(
            &format!("m/{name}"),
            &text.replace("\"orders\"", "\"accounts\""),
        );
    }
    let manifest = json!({"domain": "shop", "tools": {
        "accounts::find_account": "read", "accounts::get_account": "read",
        "orders::get_order": "read", "orders::close_order": "write"}});
    t.put("mm.json", &manifest.to_string());
    let learn = "learn --domain shop --manifest $T/mm.json --sessions $T/m --habit-only";
    t.run(&format!("{learn} --out $T/m.flow.json")).ok();
    let flow = stretto_report::flow::Flow::load(&t.at("m.flow.json")).unwrap();
    for site in [
        "accounts::find_account",
        "accounts::get_account",
        "orders::get_order",
    ] {
        assert!(flow.sites().iter().any(|s| s == site), "{:?}", flow.sites());
    }
    let md = t.run("audit --flow $T/m.flow.json --sessions $T/m").ok();
    assert!(md.contains("# Flow audit"), "{md}");
}

/// Session `i` of the shop's as a served flow's log names it to sort `i`th:
/// the proxy read the first order after the account, and the agent only
/// the second.
fn served_log(i: usize) -> (String, String) {
    let (name, text) = common::session_log(&common::session(i), i);
    let mut lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let order = json!({"order_id": format!("o{i}a")});
    let lookup = json!({"jsonrpc": "2.0", "id": "stretto-1", "method": "tools/call",
        "params": {"name": "get_order", "arguments": order}});
    let answer = json!({"jsonrpc": "2.0", "id": "stretto-1",
        "result": {"content": [{"type": "text", "text": "open"}]}});
    lines.splice(
        5..7,
        [
            json!({"t_ms": 2011, "from": "proxy", "message": lookup}),
            json!({"t_ms": 2020, "from": "server", "message": answer}),
        ],
    );
    (name, lines.iter().map(|l| format!("{l}\n")).collect())
}

/// `stage` learns the staged flow as `learn --habit-only` would, and scores
/// each new session before it learns from it; `flow-commit`, `flow-log` and
/// `flow-rollback` keep the committed flow's versions.
#[test]
fn stage_learns_as_learn_does_and_commits_with_rollback() {
    let t = Scratch::new("stage");
    t.shop(20);
    let flow = "$T/shop.flow.json";
    let stage = format!("stage --flow {flow} --manifest $T/manifest.json --sessions $T/s");
    t.fails(&stage, "name the domain with --domain");
    let md = t.run(&format!("{stage} --domain shop")).ok();
    assert!(
        md.contains("It learned from 20 sessions, 20 of them new"),
        "{md}"
    );
    assert!(
        md.contains("No session has been scored by the staged flow yet"),
        "{md}"
    );
    // The staged flow is the flow `learn` writes from the same sessions.
    t.run(&format!(
        "{LEARN_SHOP} --habit-only --out $T/learned.flow.json"
    ))
    .ok();
    let flow_json = |rel: &str| {
        let mut v: Value = serde_json::from_str(&t.get(rel)).unwrap();
        v["provenance"]["compiled_unix_ms"] = json!(0);
        v
    };
    assert_eq!(
        flow_json("shop.staged.flow.json"),
        flow_json("learned.flow.json")
    );

    // The first commit is the first version.
    let log = t
        .run(&format!("flow-commit --flow {flow} --note first"))
        .ok();
    assert!(log.contains("## Version 1"), "{log}");
    assert_eq!(t.get("shop.flow.json"), t.get("shop.staged.flow.json"));
    t.fails(&format!("flow-commit --flow {flow}"), "nothing to commit");
    t.fails(
        &format!("flow-rollback --flow {flow}"),
        "no earlier version",
    );
    // Nothing new: nothing learned, and nothing scored by both yet.
    let md = t.run(&stage).ok();
    assert!(md.contains("20 sessions, 0 of them new"), "{md}");
    assert!(md.contains("scored by both flows yet"), "{md}");

    // Ten more sessions, each scored by both flows as it arrived. In the
    // last five the committed flow served.
    for i in 20..30 {
        let (name, text) = match i {
            25.. => served_log(i),
            _ => common::session_log(&common::session(i), i),
        };
        t.put(&format!("s/{name}"), &text);
    }
    let md = t.run(&stage).ok();
    assert!(md.contains("30 sessions, 10 of them new"), "{md}");
    assert!(md.contains("## The last 10 sessions"), "{md}");
    // Both flows would read the first order twice in each, after the
    // account and in the chain after finding it: the proxy had.
    assert!(
        md.contains("The proxy had already made 10 of the committed flow's lookups and 10"),
        "{md}"
    );
    let state: Value = serde_json::from_str(&t.get("shop.stage.json")).unwrap();
    assert_eq!(state["sessions"].as_array().unwrap().len(), 30);
    assert_eq!(state["ledger"].as_array().unwrap().len(), 30);

    // Committed with the comparison it rested on.
    let log = t.run(&format!("flow-commit --flow {flow}")).ok();
    assert!(log.contains("## Version 2"), "{log}");
    assert!(log.contains("On the last 10 sessions before it"), "{log}");
    let log = t.run(&format!("flow-log --flow {flow}")).ok();
    assert!(
        log.contains("## Version 2 (committed now)") && log.contains("> first"),
        "{log}"
    );

    // Rolled back to the first, as the third version.
    t.run(&format!("flow-rollback --flow {flow}")).ok();
    assert_eq!(t.get("shop.flow.json"), t.get("shop.history/1.flow.json"));
    t.fails(
        &format!("flow-rollback --flow {flow} --to 9"),
        "there is no version 9",
    );
    t.fails(
        &format!("flow-rollback --flow {flow} --to 3"),
        "is the committed flow already",
    );
    // A committed flow changed by other means is kept before the next.
    fs::copy(t.at("learned.flow.json"), t.at("shop.flow.json")).unwrap();
    let log = t.run(&format!("flow-log --flow {flow}")).ok();
    assert!(log.contains("is not the latest version"), "{log}");
    t.run(&format!("flow-rollback --flow {flow} --to 2")).ok();
    let log = t.run(&format!("flow-log --flow {flow}")).ok();
    assert!(
        log.contains("## Version 5 (committed now)")
            && log.contains("## Version 4\n\nfound in place"),
        "{log}"
    );

    // What is not a committed flow's name.
    t.fails(
        "stage --flow $T/shop.json --sessions $T/s",
        "name the committed flow NAME.flow.json",
    );
    t.fails(
        "flow-log --flow $T/shop.staged.flow.json",
        "is a staged flow",
    );
}

/// `stage` takes spans and benchmark runs as `learn` does, and keeps what
/// the committed flow has that it does not learn: its arbiter, promotion
/// and thresholds.
#[test]
fn stage_learns_the_committed_flows_surprise_gate_again() {
    let t = Scratch::new("stage-surprise");
    t.shop(30);
    t.run(&format!(
        "{LEARN_SHOP} --habit-only --surprise 0.9 --surprise-window 2 --out $T/shop.flow.json"
    ))
    .ok();
    let committed = stretto_report::flow::Flow::load(&t.at("shop.flow.json")).unwrap();
    let gate = *committed.surprise().unwrap();
    t.run("stage --flow $T/shop.flow.json --sessions $T/s --manifest $T/manifest.json --json $T/stage.json")
        .ok();
    // Learned again from the same sessions at its quantile: the same gate,
    // and nothing carried.
    let staged = stretto_report::flow::Flow::load(&t.at("shop.staged.flow.json")).unwrap();
    assert_eq!(staged.surprise(), Some(&gate));
    let report: Value = serde_json::from_str(&t.get("stage.json")).unwrap();
    assert_eq!(report["carried"], json!([]), "{report}");
}

#[test]
fn stage_takes_what_learn_takes_and_keeps_the_committed_flows_arbiter() {
    let t = Scratch::new("stage-inputs");
    t.shop(30);
    let arbiter = t.arbiter_flow();
    let promote = format!("promote --flow {arbiter} --sessions $T/s --oracle mock");
    t.run(&format!("{promote} --out $T/shop.flow.json")).ok();
    let mut committed: Value = serde_json::from_str(&t.get("shop.flow.json")).unwrap();
    committed["thresholds"] = json!({"find_account": 0.4});
    committed["surprise"] = json!({"window": 3, "threshold": 1.5});
    committed["stretto_flow"] = json!(2);
    t.put("shop.flow.json", &committed.to_string());
    let stage = "stage --flow $T/shop.flow.json --sessions $T/s --manifest $T/manifest.json";
    t.run(&format!(
        "{stage} --oracle mock --rewards $T/rewards.json --out $T/stage.md --json $T/stage.json"
    ))
    .err();
    t.put("rewards.json", "{}");
    t.run(&format!(
        "{stage} --oracle mock --decider habit --rewards $T/rewards.json --out $T/stage.md --json $T/stage.json"
    ))
    .ok();
    let md = t.get("stage.md");
    assert!(
        md.contains("its arbiter, its promotion (it acts at")
            && md.contains("its thresholds at 1 sites, its surprise gate, set by hand"),
        "{md}"
    );
    let report: Value = serde_json::from_str(&t.get("stage.json")).unwrap();
    assert_eq!(report["carried"].as_array().unwrap().len(), 4);
    let staged = stretto_report::flow::Flow::load(&t.at("shop.staged.flow.json")).unwrap();
    assert!(staged.has_arbiter() && staged.promotion().is_some());
    assert_eq!(staged.thresholds().len(), 1);
    assert_eq!(staged.surprise().map(|g| g.threshold), Some(1.5));
    t.fails(
        &format!("{stage} --domain airline"),
        "the committed flow is for shop, not airline",
    );
    t.put(
        "other.stage.json",
        &json!({"stretto_stage": 1, "domain": "retail",
        "sessions": [], "ledger": []})
        .to_string(),
    );
    t.fails(
        "stage --flow $T/other.flow.json --domain shop --sessions $T/s",
        "is the staged retail flow's state, not shop's",
    );

    // Spans, as `learn --otel` reads them.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spans = repo.join("docs/examples/pydantic-ai-shop.otlp.jsonl");
    let manifest = repo.join("docs/examples/shop.manifest.json");
    let md = t
        .run(&format!(
            "stage --flow $T/spans/shop.flow.json --domain shop --otel {} --manifest {}",
            spans.display(),
            manifest.display()
        ))
        .ok();
    assert!(
        md.contains("It learned from 8 sessions, 8 of them new"),
        "{md}"
    );
    let staged = stretto_report::flow::Flow::load(&t.at("spans/shop.staged.flow.json")).unwrap();
    assert_eq!(
        staged.provenance().conventions.as_deref(),
        Some(stretto_trace::otel::CONVENTIONS)
    );

    // Sessions with the tools their logs list.
    t.run("stage --flow $T/listed/shop.flow.json --domain shop --sessions $T/s")
        .ok();

    // Benchmark runs, their episodes in the order listed.
    common::write_checkout(&t.0);
    let runs = "$T/data/tau2/results/final/model-a_retail_default_user-sim_2trials.json";
    let bench = format!("stage --flow $T/bench/retail.flow.json --domain retail --results {runs}");
    t.fails(
        &bench,
        "--results needs --tau2, for the tools, or --manifest",
    );
    let md = t.run(&format!("{bench} --tau2 $T")).ok();
    assert!(
        md.contains("It learned from 16 sessions, 16 of them new"),
        "{md}"
    );
    t.run(&format!("{bench} --manifest $T/manifest.json")).ok();
    t.fails(
        &format!(
            "stage --flow $T/bench/airline.flow.json --domain airline --results {runs} \
             --manifest $T/manifest.json"
        ),
        "there are no airline sessions to learn from",
    );
    t.fails(
        &format!(
            "stage --flow $T/bench/airline.flow.json --domain airline --results {runs} --tau2 $T"
        ),
        "airline/tools.py",
    );
    t.fails(
        "stage --flow $T/bench/retail.flow.json --domain retail --results $T/none.json --tau2 $T",
        "reading",
    );
}

#[test]
fn learn_audit_promote_and_drift_read_opentelemetry_spans() {
    use stretto_report::flow::{Decider, Flow, Proposal};
    let t = Scratch::new("otel");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spans = repo.join("docs/examples/pydantic-ai-shop.otlp.jsonl");
    let manifest = repo
        .join("docs/examples/shop.manifest.json")
        .display()
        .to_string();
    let learn = format!("learn --domain shop --manifest {manifest} --habit-only");
    t.run(&format!(
        "{learn} --otel {} --out $T/spans.flow.json",
        spans.display()
    ))
    .ok();
    let flow = Flow::load(&t.at("spans.flow.json")).unwrap();
    assert_eq!(
        flow.provenance().conventions.as_deref(),
        Some(stretto_trace::otel::CONVENTIONS)
    );
    // After a customer is found, their details, bound from what the spans
    // say the lookup returned.
    let after_find = |path: &Path| {
        let mut ep = stretto_trace::otel::read_otel(path).unwrap().episodes[0].clone();
        ep.events.truncate(3);
        ep
    };
    let found = after_find(&spans);
    let next = flow.next_with(&found, &Mute, 0.3, Decider::Reach).unwrap();
    assert!(
        matches!(&next.proposal, Proposal::Lookup { tool, .. } if tool == "get_user_details"),
        "{next:?}"
    );

    // Without the tools' arguments and results, the same lookup is left to
    // the agent: its arguments are unknown.
    let bare: String = fs::read_to_string(&spans)
        .unwrap()
        .lines()
        .map(|line| {
            let mut export: Value = serde_json::from_str(line).unwrap();
            for resource in export["resourceSpans"].as_array_mut().unwrap() {
                for scope in resource["scopeSpans"].as_array_mut().unwrap() {
                    for span in scope["spans"].as_array_mut().unwrap() {
                        span["attributes"].as_array_mut().unwrap().retain(|a| {
                            !["gen_ai.tool.call.arguments", "gen_ai.tool.call.result"]
                                .contains(&a["key"].as_str().unwrap())
                        });
                    }
                }
            }
            format!("{export}\n")
        })
        .collect();
    t.put("bare.jsonl", &bare);
    t.run(&format!(
        "{learn} --otel $T/bare.jsonl --out $T/bare.flow.json"
    ))
    .ok();
    let bare = Flow::load(&t.at("bare.flow.json")).unwrap();
    let next = bare
        .next_with(&after_find(&t.at("bare.jsonl")), &Mute, 0.3, Decider::Reach)
        .unwrap();
    assert!(
        matches!(&next.proposal, Proposal::HandBack { reason } if reason.contains("arguments are unknown")),
        "{next:?}"
    );

    // Measured on the spans, as on sessions.
    let otel = format!("--otel {}", spans.display());
    let md = t
        .run(&format!("audit --flow $T/spans.flow.json {otel}"))
        .ok();
    assert!(md.contains("# Flow audit"), "{md}");
    let md = t
        .run(&format!(
            "promote --flow $T/spans.flow.json {otel} --out $T/p.flow.json"
        ))
        .ok();
    assert!(md.contains("find_user_id_by_email"), "{md}");
    t.run(&format!("drift --flow $T/spans.flow.json {otel}"))
        .exits(0);
    // Spans it cannot read are an error, not a drift.
    t.put("notes.jsonl", "not spans\n");
    t.run("drift --flow $T/spans.flow.json --otel $T/notes.jsonl")
        .exits(2);
    assert_eq!(
        parse_error("learn --domain shop --otel x --out y"),
        Some(ErrorKind::MissingRequiredArgument)
    );
    assert_eq!(
        parse_error("audit --flow f --sessions s --otel x"),
        Some(ErrorKind::ArgumentConflict)
    );
}

#[test]
fn learn_counts_the_customers_tools_as_the_agents_in_a_solo_run() {
    let t = Scratch::new("solo");
    common::write_checkout(&t.0);
    t.put(
        "src/tau2/domains/retail/user_tools.py",
        "    @is_tool(ToolType.READ)\n    def check_status_bar(self) -> str:\n        ...\n",
    );
    // The agent checks the customer's phone itself, and it answers.
    let mut sim = common::simulation(1, 0, "yes");
    let messages = sim["messages"].as_array_mut().unwrap();
    messages.splice(
        2..2,
        [
            json!({"role": "assistant", "tool_calls": [{"id": "p", "name": "check_status_bar",
                "arguments": {}, "requestor": "assistant"}]}),
            json!({"role": "tool", "id": "p", "content": "full bars", "error": false}),
        ],
    );
    let mut results = common::results_file("model-s", "retail", "user-sim");
    results["simulations"] = json!([sim]);
    t.put("solo.json", &results.to_string());
    let args = "--domain retail --habit-only --tau2 $T --results $T/solo.json";
    t.run(&format!("learn {args} --out $T/solo.flow.json")).ok();
    let flow: Value = serde_json::from_str(&t.get("solo.flow.json")).unwrap();
    assert!(flow.to_string().contains("check_status_bar"), "{flow}");
}

#[test]
fn serve_serves_a_flow_file_with_a_decider_the_flow_has() {
    let t = Scratch::new("serve");
    t.shop(30);
    let habit = t.habit_flow();
    let old = t.old_flow();
    let serve = format!("serve --flow {habit} --oracle mock");
    for (args, error) in [
        (serve.clone(), "serve it with --decider reach"),
        (
            format!("serve --flow {old} --decider reach"),
            "learn it again for --decider reach",
        ),
        (
            format!("{serve} --decider habit --explore 2"),
            "--explore must be in [0, 1]",
        ),
        (
            format!("{serve} --decider habit --listen 127.0.0.1:99999"),
            "listening on",
        ),
        (
            format!("{serve} --decider habit --log $T/no/log.jsonl"),
            "opening",
        ),
    ] {
        t.fails(&args, error);
    }
    let port = free_port();
    let args = format!(
        "{serve} --decider reach --listen 127.0.0.1:{port} --explore 0.5 --explore-seed 3 \
         --log $T/served.jsonl"
    );
    let dir = t.0.clone();
    std::thread::spawn(move || stretto(&dir, &args, "", |_| {}));
    let found = query(
        "I'm c7@example.com.",
        "find_account",
        json!({"email": "c7@example.com"}),
        "acct_7",
    );
    let answer = ask_flow(port, &found);
    assert_eq!(answer["tool"], "get_account", "{answer}");
    // It logs an answer before it takes the next query.
    ask_flow(port, "not json\n");
    let logged = t.get("served.jsonl");
    assert!(
        logged.lines().next().unwrap().contains("get_account"),
        "{logged}"
    );
}

#[test]
fn learn_sets_a_surprise_gate_and_serve_overrides_it() {
    let t = Scratch::new("surprise");
    t.shop(30);
    let learn = format!("{LEARN_SHOP} --habit-only --surprise 0.9");
    t.fails(
        &format!("{learn} --surprise-window 40 --out $T/f.json"),
        "no held-out training session had 40 of the agent's steps",
    );
    t.run(&format!(
        "{learn} --surprise-window 1 --out $T/gated.flow.json"
    ))
    .ok();
    let gated = stretto_report::flow::Flow::load(&t.at("gated.flow.json")).unwrap();
    let gate = gated.surprise().unwrap();
    assert_eq!((gate.window, gate.quantile), (1, Some(0.9)));
    // The agent's step after finding the account, which the flow found a
    // little surprising: above a threshold that low, the flow hands back;
    // without the gate, it looks up.
    let call = |id: &str, tool: &str, arguments: Value| {
        json!({"role": "assistant", "tool_calls": [{"id": id, "name": tool,
            "arguments": arguments, "requestor": "assistant"}]})
    };
    let messages = json!([
        {"role": "user", "content": "I'm c7@example.com."},
        call("a", "find_account", json!({"email": "c7@example.com"})),
        {"role": "tool", "id": "a", "content": "acct_7", "error": false},
        call("b", "get_account", json!({"account_id": "acct_7"})),
        {"role": "tool", "id": "b", "error": false,
            "content": json!({"account_id": "acct_7", "orders": ["o7a", "o7b"]}).to_string()}
    ]);
    let line = format!("{}\n", json!({"task_id": "9", "messages": messages}));
    for (over, action) in [("0.000001", "hand_back"), ("off", "lookup")] {
        let port = free_port();
        let args = format!(
            "serve --flow $T/gated.flow.json --oracle mock --decider reach --surprise {over} \
             --listen 127.0.0.1:{port}"
        );
        let dir = t.0.clone();
        std::thread::spawn(move || stretto(&dir, &args, "", |_| {}));
        let answer = ask_flow(port, &line);
        assert_eq!(answer["action"], action, "{over}: {answer}");
    }
}

#[test]
fn guards_confirm_and_match_judge_a_checkouts_writes() {
    let t = Scratch::new("judge");
    common::write_checkout(&t.0);
    let data = "--tau2 $T --domain retail --source $T/targets/model-t_retail.json \
                --source $T/targets/model-t_airline.json";
    let md = t.run(&format!("guards {data}")).ok();
    assert!(md.contains("retail"), "{md}");
    t.run(&format!("guards {data} --out $T/g.md --json $T/g.json"))
        .ok();
    assert!(t.at("g.json").exists());
    let md = t
        .run(&format!(
            "confirm {data} --oracle mock --second-question proposed"
        ))
        .ok();
    assert!(md.contains("retail"), "{md}");
    let args = "--oracle mock --second-question described --oracle-dump $T/confirm.jsonl";
    t.run(&format!(
        "confirm {data} {args} --out $T/c.md --json $T/c.json"
    ))
    .ok();
    assert!(t.at("c.json").exists());
    let md = t.run(&format!("match {data} --oracle mock")).ok();
    assert!(md.contains("retail"), "{md}");
    let args = "--oracle mock --oracle-dump $T/match.jsonl --out $T/m.md --json $T/m.json";
    t.run(&format!("match {data} {args}")).ok();
    assert!(t.at("m.json").exists());
    t.fails("match --tau2 $T --domain airline --oracle mock", "tools.py");
    for judge in ["guards", "confirm --oracle mock"] {
        t.fails(
            &format!("{judge} --tau2 $T --domain nosuch"),
            "no guards for nosuch",
        );
    }
}

#[test]
fn audit_scores_sessions_or_test_episodes_under_a_flow() {
    let t = Scratch::new("audit");
    t.shop(30);
    let (habit, arbiter, old) = (t.habit_flow(), t.arbiter_flow(), t.old_flow());
    let md = t.run(&format!("audit --flow {habit} --sessions $T/s")).ok();
    assert!(md.contains("# Flow audit"), "{md}");
    let args = format!("audit --flow {arbiter} --sessions $T/s --oracle mock");
    t.run(&format!("{args} --out $T/a.md --json $T/a.json"))
        .ok();
    assert!(t.at("a.json").exists());
    for (args, error) in [
        (format!("audit --flow {habit}"), "no episodes to audit"),
        (
            format!("audit --flow {habit} --sessions $T/s --decider arbiter"),
            "the flow has no arbiter",
        ),
        (
            format!("audit --flow {old} --sessions $T/s --decider reach"),
            "no counts for --decider reach",
        ),
    ] {
        t.fails(&args, error);
    }
    // τ²-bench's test episodes, and another domain's skipped.
    common::write_checkout(&t.0);
    let results =
        "--results $T/data/tau2/results/final/model-a_retail_default_user-sim_2trials.json";
    let learned = format!("learn --domain retail --habit-only --tau2 $T {results} --out $T/r.json");
    t.run(&learned).ok();
    let skipped = "--results $T/targets/model-t_airline.json";
    let md = t
        .run(&format!(
            "audit --flow $T/r.json {results} {skipped} --tau2 $T"
        ))
        .ok();
    assert!(md.contains("# Flow audit"), "{md}");
    let args = format!("audit --flow $T/r.json {results} --tau2 $T/nowhere");
    t.fails(&args, "split_tasks.json");
}

#[test]
fn drift_exits_with_1_on_an_alarm_and_2_on_an_error() {
    let t = Scratch::new("drift");
    t.shop(60);
    let (habit, old) = (t.habit_flow(), t.old_flow());
    let a: Vec<_> = (100..130).map(common::session).collect();
    let b: Vec<_> = (0..8).map(common::session_with_rewards).collect();
    common::write_sessions(&t.at("a"), &a);
    common::write_sessions(&t.at("ab"), &[a, b].concat());
    let quiet = t.run(&format!("drift --flow {habit} --sessions $T/a")).ok();
    assert!(quiet.contains("**No alarm now:**"), "{quiet}");
    let args = format!("drift --flow {habit} --sessions $T/ab --out $T/d.md --json $T/d.json");
    t.run(&args).exits(1);
    assert!(t
        .get("d.md")
        .contains("**Alarm: the agent changed at session 31"));
    for args in [
        format!("drift --flow {habit} --sessions $T/a --decider arbiter"),
        format!("drift --flow {old} --sessions $T/a --decider reach"),
        "drift --flow $T/none.json".to_string(),
    ] {
        t.run(&args).exits(2);
    }
    // Benchmark episodes come first.
    common::write_checkout(&t.0);
    let results =
        "--results $T/data/tau2/results/final/model-a_retail_default_user-sim_2trials.json";
    let learned = format!("learn --domain retail --habit-only --tau2 $T {results} --out $T/r.json");
    t.run(&learned).ok();
    let md = t
        .run(&format!("drift --flow $T/r.json {results} --tau2 $T"))
        .ok();
    assert!(md.contains("**No alarm now:**"), "{md}");
}

#[test]
fn promote_scores_a_flows_sites_and_writes_the_promoted_flow() {
    let t = Scratch::new("promote");
    t.shop(30);
    let (habit, old) = (t.habit_flow(), t.old_flow());
    let md = t
        .run(&format!(
            "promote --flow {habit} --sessions $T/s --out $T/p.flow.json"
        ))
        .ok();
    assert!(md.contains("find_account"), "{md}");
    let promoted = stretto_report::flow::Flow::load(&t.at("p.flow.json")).unwrap();
    assert!(promoted.promotion().is_some());
    // A bar no site meets, and the report in a file.
    let args = "--min-tasks 1000 --out $T/none.flow.json --report $T/none.md";
    t.run(&format!("promote --flow {habit} --sessions $T/s {args}"))
        .ok();
    assert!(t.get("none.md").contains("find_account"));
    for (args, error) in [
        (
            format!("promote --flow {habit} --out $T/x.json"),
            "no episodes to score",
        ),
        (
            format!("promote --flow {habit} --sessions $T/none --out $T/x.json"),
            "none",
        ),
        (
            format!("promote --flow {habit} --sessions $T/s --decider arbiter --out $T/x.json"),
            "the flow has no arbiter: promote it with --decider reach",
        ),
        (
            format!("promote --flow {old} --sessions $T/s --decider reach --out $T/x.json"),
            "no counts for --decider reach",
        ),
    ] {
        t.fails(&args, error);
    }
}

#[test]
fn flow_show_and_flow_diff_review_flows() {
    let t = Scratch::new("review");
    t.shop(30);
    let (habit, arbiter) = (t.habit_flow(), t.arbiter_flow());
    let md = t.run(&format!("flow-show {habit}")).ok();
    assert!(md.contains("find_account"), "{md}");
    t.run(&format!(
        "flow-show {habit} --threshold 0.5 --out $T/show.md"
    ))
    .ok();
    assert!(t.get("show.md").contains("find_account"));
    t.fails("flow-show $T/none.json", "none.json");
    let same = t.run(&format!("flow-diff {habit} {habit}")).ok();
    assert!(!same.is_empty());
    let args = format!("flow-diff {habit} {arbiter} --tolerance 0.1 --out $T/diff.md");
    t.run(&args).exits(1);
    assert!(!t.get("diff.md").is_empty());
    t.run(&format!("flow-diff {habit} $T/none.json")).exits(2);
}

#[cfg(unix)]
#[test]
fn search_scores_settings_with_a_replay_command_and_rescores_a_front() {
    let t = Scratch::new("search");
    t.shop(30);
    let habit = t.habit_flow();
    let totals = r#"{"turns": 10, "turns_saved": 2, "detours": 1, "episodes_with_detour": 1, "episodes": 5}"#;
    // Each setting's replay hands back one decision for want of an answer.
    let log = "echo 'the System-One model failed' > \"$6/flow.jsonl\"";
    t.put("replay.sh", &format!("{log}\necho 'CHECK {totals}'\n"));
    t.put("bad.json", "not json");
    let search = format!("search --flow {habit} --population 4 --generations 1");
    let replay = "-- sh $T/replay.sh";
    let args = format!("{search} --site find_account --dir $T/runs --json $T/s.json {replay}");
    let md = t.run(&args).ok();
    assert!(md.contains("find_account"), "{md}");
    assert!(md.contains("for want of an answer"), "{md}");
    let args = format!("{search} --dir $T/runs2 --out $T/search.md {replay}");
    t.run(&args).ok();
    assert!(t.at("search.md").exists());
    let rescore = format!("search --flow {habit} --rescore $T/s.json");
    let md = t.run(&format!("{rescore} --dir $T/re {replay}")).ok();
    assert!(md.contains("replayed again"), "{md}");
    let args = format!("{rescore} --dir $T/re2 --out $T/re.md --json $T/re.json {replay}");
    t.run(&args).ok();
    assert!(t.at("re.json").exists());
    for (args, error) in [
        (
            format!("search --flow $T/none.json --dir $T/x {replay}"),
            "reading",
        ),
        (
            format!("search --flow $T/bad.json --dir $T/x {replay}"),
            "expected",
        ),
        (format!("{search} --dir $T/x -- false"), "false exited with"),
        (
            format!("{search} --dir $T/x -- true"),
            "no CHECK line from true",
        ),
        (
            format!("{search} --dir $T/x -- $T/no-such-replay"),
            "no-such-replay",
        ),
        (
            format!("{search} --site nosuch --dir $T/x {replay}"),
            "no lookup after nosuch",
        ),
        (
            format!("{search} --dir $T/replay.sh/x {replay}"),
            "creating",
        ),
        (
            format!("search --flow {habit} --rescore $T/none.json --dir $T/x {replay}"),
            "reading",
        ),
        (
            format!("search --flow {habit} --rescore $T/replay.sh --dir $T/x {replay}"),
            "not a search's --json",
        ),
    ] {
        t.fails(&args, error);
    }
}

#[test]
fn refine_weighs_candidates_on_a_decision_log() {
    let t = Scratch::new("refine");
    common::write_checkout(&t.0);
    t.put("p.json", &predicates(&["p1"]));
    t.put("c.json", &predicates(&["c1"]));
    t.put("c2.json", &predicates(&["c2"]));
    t.put("bad.json", "not json");
    t.put("empty.jsonl", "");
    let asked = "--oracle mock --questions v2 --predicates $T/p.json --candidates $T/c.json \
                 --oracle-log $T/log.jsonl --oracle-dump $T/dump.jsonl";
    let args = format!("phase0 --tau2 $T --domain retail --alpha-samples 0 {asked}");
    t.run(&args).ok();
    let refine = "refine --log $T/log-retail.jsonl";
    let md = t
        .run(&format!("{refine} --candidates $T/c.json --domain retail"))
        .ok();
    assert!(md.contains("retail"), "{md}");
    let examples = "--examples $T/examples.md --dump $T/dump-retail.jsonl --sites 2 --per-site 2";
    let args = format!("{refine} --penalty 0 {examples} --out $T/r.md --json $T/r.json");
    t.run(&args).ok();
    assert!(t.at("examples.md").exists() && t.at("r.json").exists());
    t.run(refine).ok();
    t.fails("refine --log $T/none.jsonl", "reading");
    t.fails("refine --log $T/empty.jsonl", "no next-step decisions");
    t.fails("refine --log $T/bad.json", "log line 1");
    for (args, error) in [
        ("--examples $T/e.md --dump $T/none.jsonl", "reading"),
        ("--examples $T/e.md --dump $T/bad.json", "dump line 1"),
        ("--examples $T --dump $T/dump-retail.jsonl", "writing"),
        ("--candidates $T/none.json", "reading"),
        ("--candidates $T/bad.json", "parsing"),
        (
            "--candidates $T/c2.json",
            "run phase0 with --candidates first",
        ),
    ] {
        t.fails(&format!("{refine} {args}"), error);
    }
}

/// A decision logged by `decider`, with an option, and `labels` labels.
fn decision(decider: &str, labels: usize) -> String {
    let label = json!({"used": true, "detour": false, "turn": 1.0});
    json!({
        "action": "lookup", "tool": "get_order", "site": "find_user", "episode": "e1",
        "policy": {"decider": decider, "threshold": 0.3, "epsilon": 0.1, "explored": false,
            "greedy": "get_order", "propensity": 0.9, "task_id": "t1", "events": 3,
            "options": [{"tool": "get_order", "p": 0.6, "habit": 0.5, "binding": 1.0,
                "arguments": {}}]},
        "labels": vec![label; labels],
    })
    .to_string()
}

#[test]
fn evaluate_estimates_other_rules_on_labelled_decisions() {
    let t = Scratch::new("evaluate");
    let logged = decision("arbiter", 1);
    t.put("d.jsonl", &format!("{logged}\n\n{logged}\n"));
    t.put("bad.jsonl", "not json\n");
    t.put("labels.jsonl", &decision("arbiter", 2));
    t.put("habit.jsonl", &decision("habit", 1));
    let targets = "--target C=habit@0.9 --target D0=arbiter@0.3 --min-ess 1";
    let md = t
        .run(&format!("evaluate --decisions $T/d.jsonl {targets}"))
        .ok();
    assert!(md.contains("D0"), "{md}");
    let args = "--target C=habit@0.9 --out $T/e.md --json $T/e.json";
    t.run(&format!("evaluate --decisions $T/d.jsonl {args}"))
        .ok();
    assert!(t.at("e.json").exists());
    for (args, error) in [
        ("d.jsonl --target habit", "habit"),
        ("none.jsonl --target C=habit@0.9", "reading"),
        (
            "bad.jsonl --target C=habit@0.9",
            "bad.jsonl:1: not a labelled decision",
        ),
        (
            "labels.jsonl --target C=habit@0.9",
            "2 labels for 1 options",
        ),
        (
            "habit.jsonl --target D0=arbiter@0.3",
            "needs the arbiter's probabilities",
        ),
    ] {
        t.fails(&format!("evaluate --decisions $T/{args}"), error);
    }
}

#[test]
fn redact_pseudonymizes_sessions_into_another_directory() {
    let t = Scratch::new("redact");
    t.shop(4);
    // Any variable that is set will do for a salt.
    let salted = "--salt-env PATH --keep-shared 2 --hash-field email";
    t.run(&format!("redact --sessions $T/s --out $T/r {salted}"))
        .ok();
    let redacted = t.get("r/20260928T000000_000Z-session-0.jsonl");
    assert!(!redacted.contains("c0@example.com"), "{redacted}");
    fs::create_dir_all(t.at("empty")).unwrap();
    let first = fs::read_dir(t.at("s")).unwrap().next().unwrap().unwrap();
    let log = fs::read_to_string(first.path()).unwrap();
    t.put("dup/a.jsonl", &log);
    t.put("dup/b.jsonl", &log);
    // A redacted log's name, taken by a directory.
    fs::create_dir_all(t.at("taken/20260928T000000_000Z-session-0.jsonl")).unwrap();
    for (args, error) in [
        (
            "s --out $T/r --salt-env STRETTO_TEST_NO_SALT",
            "set STRETTO_TEST_NO_SALT",
        ),
        (
            "s --out $T/s --salt-env PATH",
            "--out must be another directory",
        ),
        ("empty --out $T/r2 --salt-env PATH", "no session logs in"),
        ("dup --out $T/r3 --salt-env PATH", "share the session id"),
        ("s --out $T/manifest.json/r --salt-env PATH", "creating"),
        ("s --out $T/taken --salt-env PATH", "writing"),
    ] {
        t.fails(&format!("redact --sessions $T/{args}"), error);
    }
}

#[test]
fn arbiters_are_exported_from_a_flow_or_fitted_on_decision_logs() {
    let t = Scratch::new("arbiters");
    t.shop(30);
    let (habit, arbiter) = (t.habit_flow(), t.arbiter_flow());
    t.run(&format!(
        "export-arbiter --flow {arbiter} --out $T/shop.arbiter.json"
    ))
    .ok();
    assert!(t.get("shop.arbiter.json").contains("stretto_arbiter"));
    t.fails(
        &format!("export-arbiter --flow {habit} --out $T/x.json"),
        "arbiter",
    );
    common::write_checkout(&t.0);
    t.put("two.json", &predicates(&["p1", "p2"]));
    t.put("bad.json", "not json");
    t.put("nulls.jsonl", "{\"case\": null}\n");
    let asked = "--oracle mock --questions v2 --predicates $T/p.json --oracle-log $T/log.jsonl";
    let args = format!("phase0 --tau2 $T --domain retail --alpha-samples 0 {asked}");
    t.run(&args).ok();
    // A decision of an agent the log does not name.
    let log = t.get("log-retail.jsonl");
    let case = log.lines().find(|l| !l.contains("\"case\":null")).unwrap();
    let mut unnamed: Value = serde_json::from_str(case).unwrap();
    unnamed.as_object_mut().unwrap().remove("model");
    t.put("log-retail.jsonl", &format!("{log}{unnamed}\n"));
    let fit = "fit-arbiter --domain retail --out $T/fit.json";
    let args = format!("{fit} --log $T/log-retail.jsonl --predicates $T/p.json --model m");
    t.run(&args).ok();
    assert!(t.get("fit.json").contains("stretto_arbiter"));
    for (args, error) in [
        (
            "--log $T/log-retail.jsonl --predicates $T/none.json",
            "reading",
        ),
        (
            "--log $T/log-retail.jsonl --predicates $T/bad.json",
            "parsing",
        ),
        ("--log $T/none.jsonl --predicates $T/p.json", "reading"),
        (
            "--log $T/bad.json --predicates $T/p.json",
            "not a decision log",
        ),
        (
            "--log $T/log-retail.jsonl --predicates $T/two.json",
            "weigh 1 predicates, not the 2",
        ),
        (
            "--log $T/nulls.jsonl --predicates $T/p.json",
            "no v2 decisions in the logs",
        ),
    ] {
        t.fails(&format!("{fit} {args}"), error);
    }
}

#[test]
fn answers_are_asked_imported_and_exported() {
    let t = Scratch::new("answers");
    common::write_checkout(&t.0);
    let asked = "--oracle mock --questions v2 --oracle-dump $T/dump.jsonl --oracle-limit 2";
    let args = format!("phase0 --tau2 $T --domain retail --alpha-samples 0 {asked}");
    t.run(&args).ok();
    // A dumped request, and the same request bare, after a blank line.
    let dumped = t.get("dump-retail.jsonl");
    let first: Value = serde_json::from_str(dumped.lines().next().unwrap()).unwrap();
    t.put(
        "requests.jsonl",
        &format!("{dumped}\n{}\n", first["request"]),
    );
    let requests = "ask --requests $T/requests.jsonl";
    t.run(&format!(
        "{requests} --oracle mock --oracle-concurrency 2 --out $T/answers.jsonl"
    ))
    .ok();
    let answers = t.get("answers.jsonl");
    assert!(
        answers.lines().all(|l| l.contains("\"response\"")),
        "{answers}"
    );
    // From an empty cache, every answer is an error.
    t.run(&format!(
        "{requests} --oracle-cache $T/empty --out $T/failed.jsonl"
    ))
    .ok();
    let failed = t.get("failed.jsonl");
    assert!(failed.lines().all(|l| l.contains("\"error\"")), "{failed}");
    // Into a cache, and out again.
    stretto(
        &t.0,
        "import-answers --oracle-cache $T/cache",
        &format!("{answers}\n"),
        |_| {},
    )
    .ok();
    let exported = t.run("export-answers --oracle-cache $T/cache").ok();
    assert_eq!(
        exported.lines().count(),
        dumped.lines().count(),
        "{exported}"
    );
    t.put("bad.jsonl", "not json\n");
    t.put("other.jsonl", "{\"x\": 1}\n");
    for (args, error) in [
        ("none.jsonl --out $T/x", "reading"),
        ("bad.jsonl --out $T/x", "line 1"),
        ("other.jsonl --out $T/x", "line 1: not a request"),
        (
            "requests.jsonl --oracle jev --oracle-budget 0 --out $T/x",
            "exceeds the budget",
        ),
    ] {
        t.fails(&format!("ask --requests $T/{args}"), error);
    }
}

/// An oracle that answers no question.
struct Mute;

impl stretto_oracle::Oracle for Mute {
    fn ask(&self, _: &stretto_oracle::Request) -> Result<stretto_oracle::Response> {
        Ok(stretto_oracle::Response {
            model: "mute".to_string(),
            answers: BTreeMap::new(),
            usage: Default::default(),
        })
    }
}

#[test]
fn jev_check_says_how_jev_answered() {
    let t = Scratch::new("jev");
    let said = stretto(&t.0, "jev-check", "", |env| env.jev = jev_up).ok();
    assert_eq!(said, "Jev OK: model jev-test\n");
    t.fails("jev-check", "Jev is down");
    let mock = stretto_oracle::MockOracle {
        confidence: 0.6,
        noul: 0.25,
    };
    let said = super::ask_jev(&mock).unwrap();
    assert!(
        said.starts_with("Jev OK: model mock, P(yes) = 0.250"),
        "{said}"
    );
    let said = super::ask_jev(&Mute).unwrap();
    assert!(said.starts_with("Jev OK: model mute, None"), "{said}");
}

#[test]
fn init_prints_or_writes_each_hosts_configuration() {
    let t = Scratch::new("init");
    t.shop(30);
    t.habit_flow();
    for host in ["claude-code", "claude-desktop", "cursor", "vscode"] {
        let said = t
            .run(&format!(
                "init --host {host} --domain notes -- npx -y server"
            ))
            .ok();
        assert!(said.contains("stretto-proxy"), "{said}");
    }
    // A flow names the domain. Relative paths become absolute, and a leading
    // ~ is left for the proxy.
    fs::create_dir_all(t.at("home")).unwrap();
    fs::copy(t.at("habit.flow.json"), t.at("home/shop.flow.json")).unwrap();
    let args = "--flow ~/shop.flow.json --shadow --record rel/logs -- ./bin/server --verbose";
    let said = t.run(&format!("init --host cursor {args}")).ok();
    let here = std::env::current_dir().unwrap();
    assert!(
        said.contains(&here.join("rel/logs").display().to_string()),
        "{said}"
    );
    assert!(
        said.contains(&here.join("bin/server").display().to_string()),
        "{said}"
    );
    let upstream = "--upstream https://example.com/mcp --upstream-header Authorization=NOTES_TOKEN";
    let said = t
        .run(&format!(
            "init --host claude-code --domain notes {upstream}"
        ))
        .ok();
    assert!(said.contains("NOTES_TOKEN"), "{said}");
    let write = "init --host claude-code --domain notes --write $T/.mcp.json";
    t.run(&format!("{write} -- server")).ok();
    t.fails(&format!("{write} -- server"), ".mcp.json");
    t.run(&format!("{write} --force -- server")).ok();
    for (args, error) in [
        (
            "--upstream ftp://example.com",
            "starts with http:// or https://",
        ),
        (
            "--upstream https://example.com --upstream-header Authorization",
            "NAME=VAR",
        ),
        (
            "--guards -- server",
            "no policy guards for notes; retail and airline have them",
        ),
    ] {
        t.fails(&format!("init --host cursor --domain notes {args}"), error);
    }
    assert!(super::host_path("").is_err());
    // The guards, the judge, stretto_commit and retention, for the proxy.
    let said = t
        .run(
            "init --host claude-code --domain retail --guards --confirm-judge log \
             --context ~/.stretto/context/retail.jsonl --commit --retain-days 30 -- server",
        )
        .ok();
    assert!(
        said.contains(
            "--retain-days 30 --guards --confirm-judge log --context \
             ~/.stretto/context/retail.jsonl --commit -- server"
        ),
        "{said}"
    );
    let said = t
        .run("init --host cursor --domain airline --guards --confirm-judge enforce --context ctx.jsonl -- server")
        .ok();
    assert!(said.contains("\"enforce\""), "{said}");
    // One proxy that every host connects to by URL, sending the token in
    // its file.
    let listen = "--domain notes --listen 127.0.0.1:8931 --listen-token-file rel/token";
    let said = t
        .run(&format!("init --host cursor {listen} -- server"))
        .ok();
    assert!(
        said.contains("\"url\": \"http://127.0.0.1:8931/mcp\"")
            && said.contains("${env:STRETTO_PROXY_TOKEN}"),
        "{said}"
    );
    let said = t
        .run(&format!("init --host claude-code {listen} -- server"))
        .ok();
    let token = here.join("rel/token").display().to_string();
    assert!(said.contains(&format!("$(cat {token})")), "{said}");
    // Written, Claude Code's configuration reads the token from a variable.
    t.run(&format!(
        "init --host claude-code {listen} --write $T/listen/.mcp.json -- server"
    ))
    .ok();
    let written = fs::read_to_string(t.at("listen/.mcp.json")).unwrap();
    assert!(
        written.contains("\"Authorization\": \"Bearer ${STRETTO_PROXY_TOKEN}\""),
        "{written}"
    );
    let said = t
        .run("init --host claude-code --domain notes --listen 127.0.0.1:8931 -- server")
        .ok();
    assert!(
        said.contains("claude mcp add --transport http notes http://127.0.0.1:8931/mcp"),
        "{said}"
    );
    t.fails(
        "init --host cursor --domain notes --listen 0.0.0.0:8931 -- server",
        "without --listen-token-file",
    );
    t.fails(
        "init --host cursor --domain notes --listen 127.0.0.1:0 -- server",
        "not 0",
    );
}

#[test]
fn init_names_the_proxy_as_the_host_can_find_it() {
    let t = Scratch::new("proxy-command");
    let exe = std::env::consts::EXE_SUFFIX;
    let on_path = t.put(&format!("bin/stretto-proxy{exe}"), "");
    let beside = t.put(&format!("beside/stretto-proxy{exe}"), "");
    let (mut input, mut output) = (&b""[..], Vec::new());
    let mut env = test_env(&t.0, &mut input, &mut output);
    env.path = t.at("bin").into_os_string();
    let named = super::proxy_command(Host::ClaudeDesktop, &env);
    assert_eq!(named, on_path.display().to_string());
    assert_eq!(super::proxy_command(Host::Cursor, &env), "stretto-proxy");
    env.path = OsString::new();
    env.exe = Some(t.at("beside/stretto"));
    let named = super::proxy_command(Host::Cursor, &env);
    assert_eq!(named, beside.display().to_string());
    env.exe = None;
    assert_eq!(super::proxy_command(Host::Cursor, &env), "stretto-proxy");
}

#[test]
fn doctor_reports_on_the_data_directory_and_asks_jev_only_with_network() {
    let t = Scratch::new("doctor");
    fs::create_dir_all(t.at("data")).unwrap();
    fs::create_dir_all(t.at("home/.stretto")).unwrap();
    // No companion binary on an empty PATH: a problem, so 1.
    let said = t.run("doctor --data $T/data").exits(1);
    assert!(said.starts_with("stretto "), "{said}");
    let said = t.run("doctor").exits(1);
    assert!(said.contains(".stretto"), "{said}");
    let said = t.run("doctor --network --data $T/data").exits(1);
    assert!(
        said.contains("no key is set, so Jev was not asked"),
        "{said}"
    );
    let args = "doctor --network --data $T/data";
    let said = stretto(&t.0, args, "", |env| {
        env.key = [true, false];
        env.jev = jev_up;
    })
    .exits(1);
    assert!(said.contains("Jev OK"), "{said}");
    let said = stretto(&t.0, args, "", |env| env.key = [false, true]).exits(1);
    assert!(said.contains("Jev did not answer: Jev is down"), "{said}");
    let bin = t.at("bin/stretto");
    let said = stretto(&t.0, "doctor", "", |env| {
        env.home = None;
        env.exe = Some(bin);
    })
    .exits(1);
    assert!(
        said.contains("neither HOME nor USERPROFILE is set"),
        "{said}"
    );
}

#[test]
fn completions_print_a_shells_script() {
    let t = Scratch::new("completions");
    let script = t.run("completions bash").ok();
    assert!(script.contains("stretto"), "{script}");
}

fn parse_error(args: &str) -> Option<ErrorKind> {
    let words = std::iter::once("stretto").chain(args.split_whitespace());
    Cli::try_parse_from(words).err().map(|e| e.kind())
}

#[test]
fn numbers_out_of_range_are_refused() {
    for args in [
        "learn --sessions s --domain d --out f --half-life 0",
        "drift --flow f --hazard 1.5",
        "drift --flow f --threshold 0",
        "drift --flow f --window 2",
        "learn --sessions s --domain d --out f --surprise 1.5",
        "learn --sessions s --domain d --out f --surprise 0.9 --surprise-window 0",
        "serve --flow f --surprise 0",
        "serve --flow f --surprise never",
        "init --host cursor --domain d --retain-days 0 -- s",
    ] {
        assert_eq!(
            parse_error(args),
            Some(ErrorKind::ValueValidation),
            "{args}"
        );
    }
    assert_eq!(
        parse_error("drift --flow f --hazard 0.5 --threshold 1"),
        None
    );
    assert_eq!(
        parse_error("learn --sessions s --domain d --out f --surprise-window 3"),
        Some(ErrorKind::MissingRequiredArgument)
    );
}

#[test]
fn a_compiled_flow_says_how_often_its_bindings_agreed() {
    let t = Scratch::new("agreement");
    t.shop(30);
    t.habit_flow();
    let flow = stretto_report::flow::Flow::load(&t.at("habit.flow.json")).unwrap();
    let said = super::agreement(&flow);
    assert!(
        said.contains("stretto: binding get_account: agreed 30/30 unmentioned"),
        "{said}"
    );
}

#[test]
fn reports_go_where_asked_and_their_directories_are_made() {
    let t = Scratch::new("write");
    super::write(&t.at("a/b/report.md"), b"# Report").unwrap();
    assert_eq!(t.get("a/b/report.md"), "# Report");
    // A file in the current directory needs none made.
    super::make_parent(Path::new("report.md")).unwrap();
    let said = format!("{:#}", super::write(&t.at("a"), b"x").unwrap_err());
    assert!(said.starts_with("writing "), "{said}");
}
