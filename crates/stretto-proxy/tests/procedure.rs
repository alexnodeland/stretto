//! `stretto-procedure` as a process: a compiled procedure against a stdio
//! server it starts.

#[cfg(unix)]
#[test]
fn runs_a_procedure_against_a_server_it_starts() {
    use std::process::Command;
    // Answers each request `ok`, and ends when its input does.
    let server = r#"n=0
while IFS= read -r line; do
  case "$line" in
    *'"id"'*) n=$((n+1))
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"ok"}]}}\n' "$n" ;;
  esac
done"#;
    let procedure = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/results/telecom-workflow-2026-09-26.procedure.json"
    );
    let out = Command::new(env!("CARGO_BIN_EXE_stretto-procedure"))
        .args(["--procedure", procedure, "--ticket"])
        .arg("The customer at 555-123-4567 has no mobile data.")
        .args(["--", "sh", "-c", server])
        .output()
        .unwrap();
    let summary = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{summary}");
    let run: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(run["verdict"], "hand_back");
    assert!(
        summary.ends_with("calls, handed back: its check failed\n"),
        "{summary}"
    );
}
