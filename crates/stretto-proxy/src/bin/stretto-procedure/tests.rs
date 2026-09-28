//! The procedure runner in this process, against scripted stdio servers.

use super::*;
use clap::CommandFactory;
use std::path::Path;
use stretto_report::cli_doc;

/// `docs/cli.md` documents this CLI as it is.
#[test]
fn the_cli_reference_is_current() {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    let section = cli_doc::markdown(&Cli::command());
    if let Err(e) = cli_doc::check_page(&page, "stretto-procedure", &section) {
        panic!("{e}");
    }
}

/// A server that runs `script` in `sh`.
fn sh(script: &str) -> Vec<String> {
    ["sh", "-c", script].map(String::from).to_vec()
}

/// A server that answers each request `ok`, and ends when its input does.
const ANSWERS_OK: &str = r#"n=0
while IFS= read -r line; do
  case "$line" in
    *'"id"'*) n=$((n+1))
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"ok"}]}}\n' "$n" ;;
  esac
done"#;

const TICKET: &str = "The customer at 555-123-4567 has no mobile data.";

/// `stretto-procedure` on the telecom procedure with `args`, then `server`.
fn cli(args: &[&str], server: &[String]) -> Cli {
    let procedure = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/results/telecom-workflow-2026-09-26.procedure.json");
    let mut argv = vec![
        "stretto-procedure".to_string(),
        "--procedure".to_string(),
        procedure.display().to_string(),
    ];
    argv.extend(args.iter().map(|a| a.to_string()));
    argv.push("--".to_string());
    argv.extend(server.iter().cloned());
    Cli::try_parse_from(argv).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-procedure-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn runs_a_procedure_and_writes_its_run() {
    let mut out = Vec::new();
    run(&cli(&["--ticket", TICKET], &sh(ANSWERS_OK)), &mut out).unwrap();
    let written: Value = serde_json::from_slice(&out).unwrap();
    let calls = written["calls"].as_array().unwrap();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|c| c["failed"] == false), "{written}");
    // The ticket states no outcome the procedure can check.
    assert_eq!(written["verdict"], "hand_back");

    // The same ticket from a file, and the run to a file.
    let dir = scratch("files");
    let (ticket, run_file) = (dir.join("ticket.txt"), dir.join("run.json"));
    std::fs::write(&ticket, TICKET).unwrap();
    let mut stdout = Vec::new();
    let args = [
        "--ticket-file",
        ticket.to_str().unwrap(),
        "--out",
        run_file.to_str().unwrap(),
    ];
    run(&cli(&args, &sh(ANSWERS_OK)), &mut stdout).unwrap();
    assert!(stdout.is_empty());
    let from_file: Value = serde_json::from_slice(&std::fs::read(&run_file).unwrap()).unwrap();
    assert_eq!(from_file, written);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn says_what_it_could_not_read_or_write() {
    let dir = scratch("errors");
    let missing = dir.join("missing.txt");
    let e = run(
        &cli(
            &["--ticket-file", missing.to_str().unwrap()],
            &sh(ANSWERS_OK),
        ),
        &mut Vec::new(),
    )
    .unwrap_err();
    assert_eq!(e.to_string(), format!("reading {}", missing.display()));
    let e = run(&cli(&[], &sh(ANSWERS_OK)), &mut Vec::new()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "give the ticket with --ticket or --ticket-file"
    );
    let nowhere = dir.join("no/such/dir/run.json");
    let args = ["--ticket", TICKET, "--out", nowhere.to_str().unwrap()];
    let e = run(&cli(&args, &sh(ANSWERS_OK)), &mut Vec::new()).unwrap_err();
    assert_eq!(e.to_string(), format!("writing {}", nowhere.display()));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_server_must_be_named_and_start() {
    let e = Server::start(&[]).err().unwrap();
    assert_eq!(e.to_string(), "no server command");
    let e = Server::start(&["/nonexistent/stretto-server".to_string()])
        .err()
        .unwrap();
    assert_eq!(e.to_string(), "starting /nonexistent/stretto-server");
}

#[cfg(unix)]
#[test]
fn the_client_takes_only_the_answer_to_its_request() {
    // Before its answer to `initialize`, the server writes a line that is
    // not JSON, a notification, and an answer to another request.
    let script = r#"read -r line
echo 'not json'
echo '{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info"}}'
echo '{"jsonrpc":"2.0","id":9,"result":{}}'
echo '{"jsonrpc":"2.0","id":1,"result":{}}'
read -r line
read -r line
echo '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"a"},{"type":"image"},{"type":"text","text":"b"}],"isError":true}}'
read -r line
echo '{"jsonrpc":"2.0","id":3,"result":{}}'
read -r line
echo '{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"Unknown tool: x"}}'"#;
    let mut server = Server::start(&sh(script)).unwrap();
    let none = Map::new();
    assert_eq!(
        server.call("lookup", &none).unwrap(),
        ("a\nb".to_string(), true)
    );
    assert_eq!(
        server.call("lookup", &none).unwrap(),
        (String::new(), false)
    );
    let e = server.call("x", &none).unwrap_err();
    assert_eq!(
        e.to_string(),
        r#"tools/call: {"code":-32602,"message":"Unknown tool: x"}"#
    );
}

#[cfg(unix)]
#[test]
fn a_server_that_ends_without_answering_is_an_error() {
    let e = Server::start(&sh("read -r line")).err().unwrap();
    assert_eq!(
        e.to_string(),
        "the server closed its output before answering initialize"
    );
}

#[cfg(unix)]
#[test]
fn a_server_that_does_not_exit_is_stopped() {
    let script = r#"read -r line; echo '{"jsonrpc":"2.0","id":1,"result":{}}'; exec sleep 30"#;
    let mut server = Server::start(&sh(script)).unwrap();
    server.patience = Duration::from_millis(150);
    let asked = Instant::now();
    drop(server);
    assert!(asked.elapsed() < Duration::from_secs(5));
}

#[test]
fn says_what_each_verdict_means() {
    assert_eq!(
        [Verdict::Resolved, Verdict::Transferred, Verdict::HandBack].map(meaning),
        [
            "resolved by its own check",
            "transferred to a person",
            "handed back: its check failed"
        ]
    );
}
