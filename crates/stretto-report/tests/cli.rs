//! The `stretto` binary as a process: what it takes from its surroundings.
//! Its commands run in-process in `src/cli_tests.rs`.

use std::io::Write;
use std::process::{Command, Stdio};

fn stretto() -> Command {
    Command::new(env!("CARGO_BIN_EXE_stretto"))
}

/// A directory of the test's own.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-process-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn jev_check_without_a_key_fails_asking_for_one() {
    let out = stretto()
        .arg("jev-check")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_API_KEY_FILE")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("TYPESAFE_API_KEY"), "{said}");
}

/// Without HOME, the data directory is under USERPROFILE, as on Windows. A
/// key file is noted as set, never read.
#[test]
fn doctor_finds_the_home_and_the_key_it_is_given() {
    let dir = scratch("doctor");
    std::fs::create_dir_all(dir.join(".stretto")).unwrap();
    let out = stretto()
        .arg("doctor")
        .env_remove("HOME")
        .env("USERPROFILE", &dir)
        .env_remove("TYPESAFE_API_KEY")
        .env("TYPESAFE_API_KEY_FILE", dir.join("no-such-key"))
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    let data = dir.join(".stretto").display().to_string();
    assert!(
        said.contains(&format!("{data} exists and is writable")),
        "{said}"
    );
    assert!(said.contains("TYPESAFE_API_KEY_FILE is set"), "{said}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn import_answers_reads_its_standard_input() {
    let dir = scratch("import");
    let mut child = stretto()
        .args(["import-answers", "--oracle-cache"])
        .arg(dir.join("cache"))
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A cache key is a request's SHA-256, in hex.
    let key = "0f".repeat(32);
    let answer = format!(r#"{{"key": "{key}", "response": {{"model": "m", "answers": {{}}}}}}"#);
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{answer}").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("imported 1 answers"), "{said}");
    std::fs::remove_dir_all(&dir).ok();
}
