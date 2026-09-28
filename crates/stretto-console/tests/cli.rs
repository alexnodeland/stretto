//! The `stretto-console` binary: it serves a data directory on the address
//! it is given, answers its own healthcheck, and stops cleanly.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const CONSOLE: &str = env!("CARGO_BIN_EXE_stretto-console");

fn data(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("stretto-console-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn it_refuses_no_auth_off_loopback() {
    let dir = data("refused");
    let out = Command::new(CONSOLE)
        .args(["--data"])
        .arg(&dir)
        .args(["--listen", "0.0.0.0:0", "--no-auth"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("stretto-console: --no-auth is refused on 0.0.0.0:0"),
        "{err}"
    );
    // Read-only creates nothing, not even the data directory.
    let out = Command::new(CONSOLE)
        .arg("--data")
        .arg(&dir)
        .arg("--read-only")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!dir.exists());
}

#[cfg(unix)]
#[test]
fn it_serves_answers_its_healthcheck_and_stops_on_sigterm() {
    let dir = data("serve");
    let token = "f".repeat(64);
    let mut console = Command::new(CONSOLE)
        .arg("--data")
        .arg(&dir)
        .args(["--listen", "127.0.0.1:0"])
        .env("STRETTO_CONSOLE_TOKEN", &token)
        .env_remove("STRETTO_CONSOLE_LISTEN")
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(console.stderr.take().unwrap()).lines();
    // "stretto-console: <dir> on 127.0.0.1:<port>", after a note if the
    // stretto CLI is not beside it; then the URL to open.
    let mut next = || lines.next().unwrap().unwrap();
    let serving = std::iter::repeat_with(&mut next)
        .find(|l| l.contains(" on 127.0.0.1:"))
        .unwrap();
    let port: u16 = serving
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("{serving}"));
    let open = next();
    // A token given is never printed.
    assert!(!open.contains(&token), "{open}");
    assert!(
        open.contains(&format!("http://127.0.0.1:{port}/")),
        "{open}"
    );
    // It made the data directory it was given.
    assert!(dir.is_dir());

    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let meta: serde_json::Value = client
        .get(format!("http://127.0.0.1:{port}/api/meta"))
        .bearer_auth(&token)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(meta["auth"], true);
    let listen = format!("127.0.0.1:{port}");
    let health = Command::new(CONSOLE)
        .args(["healthcheck", "--listen", &listen])
        .status()
        .unwrap();
    assert!(health.success());

    Command::new("kill")
        .args(["-TERM", &console.id().to_string()])
        .status()
        .unwrap();
    let status = console.wait().unwrap();
    assert!(status.success(), "{status}");
    let rest: Vec<String> = lines.map_while(Result::ok).collect();
    assert_eq!(
        rest.last().map(String::as_str),
        Some("stretto-console: stopped"),
        "{rest:?}"
    );
    // Nothing answers now.
    let health = Command::new(CONSOLE)
        .args(["healthcheck", "--listen", &listen])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(health.code(), Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
}
