//! The proxy's command line in this process: what each flag builds, the
//! retention sweep, the upstream's headers, and serving a host.

use super::*;
use clap::CommandFactory;
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};
use stretto_report::cli_doc;

/// `docs/cli.md` documents this CLI as it is.
#[test]
fn the_cli_reference_is_current() {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    let section = cli_doc::markdown(&Cli::command());
    if let Err(e) = cli_doc::check_page(&page, "stretto-proxy", &section) {
        panic!("{e}");
    }
}

/// A flow with an arbiter.
const ARBITER: &str = "docs/examples/retail-5-sessions-shipped-arbiter.flow.json";
/// A flow with neither an arbiter nor reach counts: the habit alone.
const HABIT: &str = "docs/examples/retail-5-sessions.flow.json";
/// A flow with reach counts and no arbiter.
const REACH: &str = "crates/stretto-console/tests/fixtures/home/shop.flow.json";

const URL: &str = "http://127.0.0.1:9/mcp";

fn repo(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.join(path).display().to_string()
}

fn cli(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("stretto-proxy").chain(args.iter().copied())).unwrap()
}

fn no_jev() -> Result<JevClient> {
    bail!("no Jev here")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-proxy-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// What `args`, and a server command, ask the proxy to do.
fn built(args: &[&str]) -> Result<Active> {
    let mut all = args.to_vec();
    all.extend(["--", "cat"]);
    active(&cli(&all), &no_jev)
}

fn error(args: &[&str]) -> String {
    match built(args) {
        Ok(_) => panic!("{args:?} built"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn each_flow_is_served_as_asked() {
    let dir = scratch("flows");
    let cache = dir.join("cache").display().to_string();
    let (arbiter, habit, reach) = (repo(ARBITER), repo(HABIT), repo(REACH));

    // A flow is served as it says, unless told otherwise.
    let a = built(&["--flow", &arbiter, "--oracle", "mock"]).unwrap();
    assert!(a.is_active());
    let fc = a.flow.unwrap();
    assert_eq!(fc.decider, Decider::Arbiter);
    assert!(fc.explore.is_none() && fc.tools.is_none() && !fc.shadow);
    let args = [
        "--flow",
        &arbiter,
        "--flow-decider",
        "arbiter",
        "--oracle",
        "replay",
        "--oracle-cache",
        &cache,
    ];
    assert_eq!(
        built(&args).unwrap().flow.unwrap().decider,
        Decider::Arbiter
    );
    let a = built(&["--flow", &habit, "--flow-decider", "habit"]).unwrap();
    assert_eq!(a.flow.unwrap().decider, Decider::Habit);
    let log = dir.join("flow.jsonl");
    let a = built(&[
        "--flow",
        &reach,
        "--flow-decider",
        "reach",
        "--flow-shadow",
        "--flow-tools",
        "get_user_details",
        "--flow-explore",
        "0.2",
        "--flow-explore-seed",
        "3",
        "--flow-log",
        log.to_str().unwrap(),
    ])
    .unwrap();
    let fc = a.flow.unwrap();
    assert_eq!((fc.decider, fc.shadow), (Decider::Reach, true));
    assert_eq!(fc.tools, Some(["get_user_details".to_string()].into()));
    assert_eq!(
        fc.explore,
        Some(stretto_report::flow::Explore {
            epsilon: 0.2,
            seed: 3
        })
    );
    assert_eq!(fc.log, Some(log));

    // Jev is asked for only when the oracle is Jev.
    assert_eq!(error(&["--flow", &arbiter]), "no Jev here");
    // What cannot be served.
    assert!(error(&["--flow", &habit, "--flow-decider", "arbiter"])
        .ends_with("was learned without a System-One model: serve it with --flow-decider habit"));
    assert!(error(&["--flow", &habit, "--flow-decider", "reach"])
        .ends_with("learn it again for --flow-decider reach"));
    let args = [
        "--flow",
        &arbiter,
        "--oracle",
        "mock",
        "--flow-explore",
        "2",
    ];
    assert_eq!(error(&args), "--flow-explore must be in [0, 1]");
    let missing = dir.join("missing.flow.json");
    assert!(error(&["--flow", missing.to_str().unwrap()])
        .starts_with(&format!("loading {}: ", missing.display())));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn guards_and_the_confirmation_judge_are_built_as_asked() {
    let dir = scratch("judge");
    let cache = dir.join("cache").display().to_string();
    let context = dir.join("context.jsonl").display().to_string();

    assert!(!built(&[]).unwrap().is_active());
    assert_eq!(
        error(&["--guards"]),
        "--guards needs --domain (retail or airline)"
    );
    assert_eq!(
        error(&["--guards", "--domain", "telecom"]),
        "no guards for telecom"
    );
    let a = built(&[
        "--guards",
        "--domain",
        "retail",
        "--commit",
        "--context",
        &context,
        "--task-id",
        "7",
    ])
    .unwrap();
    assert!(a.guards.is_some() && a.commit && a.confirm.is_none());
    assert_eq!(
        (a.context, a.task_id),
        (Some(PathBuf::from(&context)), Some("7".to_string()))
    );

    let judge = ["--guards", "--domain", "retail", "--context", &context];
    assert_eq!(
        error(&[&judge[..], &["--confirm-judge", "log"]].concat()),
        "no Jev here"
    );
    let a = built(
        &[
            &judge[..],
            &[
                "--confirm-judge",
                "enforce",
                "--oracle",
                "replay",
                "--oracle-cache",
                &cache,
                "--confirm-second",
                "proposed",
            ],
        ]
        .concat(),
    )
    .unwrap();
    let cc = a.confirm.unwrap();
    assert_eq!((cc.enforce, cc.second), (true, Some(Second::Proposed)));
    let a = built(
        &[
            &judge[..],
            &[
                "--confirm-judge",
                "log",
                "--oracle",
                "mock",
                "--confirm-second",
                "described",
                "--confirm-second-shadow",
            ],
        ]
        .concat(),
    )
    .unwrap();
    let cc = a.confirm.unwrap();
    assert_eq!(
        (cc.enforce, cc.second, cc.second_shadow),
        (false, Some(Second::Described), true)
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_upstreams_headers_come_from_the_environment() {
    let var = |name: &str| (name == "AUTH").then(|| "Bearer t".to_string());
    assert!(upstream(&cli(&["--", "cat"]), &var).unwrap().is_none());
    let args = ["--upstream", URL, "--upstream-header", "Authorization=AUTH"];
    let u = upstream(&cli(&args), &var).unwrap().unwrap();
    assert_eq!(
        (u.url.as_str(), u.headers),
        (
            URL,
            vec![("Authorization".to_string(), "Bearer t".to_string())]
        )
    );
    let args = ["--upstream", URL, "--upstream-header", "Authorization"];
    let e = upstream(&cli(&args), &var).unwrap_err();
    assert!(e
        .to_string()
        .starts_with("--upstream-header takes NAME=VAR"));
    let args = [
        "--upstream",
        URL,
        "--upstream-header",
        "Authorization=UNSET",
    ];
    let e = upstream(&cli(&args), &var).unwrap_err();
    assert_eq!(
        e.to_string(),
        "the environment variable UNSET, for the header Authorization, is not set"
    );
}

#[test]
fn old_logs_and_answers_are_deleted_before_serving() {
    let dir = scratch("retain");
    let (logs, cache) = (dir.join("logs"), dir.join("cache"));
    fs::create_dir_all(&logs).unwrap();
    fs::create_dir_all(&cache).unwrap();
    let old = SystemTime::now() - Duration::from_secs(30 * 86_400);
    File::create(logs.join("old.jsonl"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    File::create(logs.join("new.jsonl")).unwrap();
    // A cache that is a file cannot be pruned: that is reported, and the
    // rest goes on.
    let not_a_dir = dir.join("cache-file");
    fs::write(&not_a_dir, "").unwrap();
    let args = |cache: &Path| {
        cli(&[
            "--record",
            logs.to_str().unwrap(),
            "--oracle-cache",
            cache.to_str().unwrap(),
            "--",
            "cat",
        ])
    };
    retain(&args(&not_a_dir), 7);
    assert!(!logs.join("old.jsonl").exists() && logs.join("new.jsonl").exists());
    // Nothing is old enough now.
    retain(&args(&cache), 7);
    assert!(logs.join("new.jsonl").exists());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn setup_sweeps_then_says_what_to_serve() {
    let env = Env {
        jev: &no_jev,
        var: &|_| None,
    };
    let dir = scratch("setup");
    let (logs, cache) = (dir.join("logs"), dir.join("cache"));
    fs::create_dir_all(&logs).unwrap();
    let old = SystemTime::now() - Duration::from_secs(30 * 86_400);
    File::create(logs.join("old.jsonl"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let args = cli(&[
        "--retain-days",
        "7",
        "--record",
        logs.to_str().unwrap(),
        "--oracle-cache",
        cache.to_str().unwrap(),
        "--domain",
        "retail",
        "--agent-model",
        "glm-5.3",
        "--commit",
        "--",
        "cat",
        "-u",
    ]);
    let (config, active) = setup(&args, &env).unwrap();
    assert!(!logs.join("old.jsonl").exists());
    assert_eq!(config.command, ["cat", "-u"]);
    assert_eq!(config.record, Some(logs.clone()));
    assert_eq!(
        (config.domain.as_deref(), config.agent_model.as_deref()),
        (Some("retail"), Some("glm-5.3"))
    );
    assert!(config.upstream.is_none() && active.commit);

    // Failing before anything is served.
    let missing = dir.join("missing.flow.json");
    let args = cli(&["--flow", missing.to_str().unwrap(), "--", "cat"]);
    assert!(setup(&args, &env).is_err());
    let args = cli(&["--upstream", URL, "--upstream-header", "Authorization"]);
    assert!(setup(&args, &env).is_err());
    fs::remove_dir_all(&dir).unwrap();
}
