use super::*;
use stretto_trace::mcp::parse_log;

const HEADER: &str = r#"{"stretto_mcp_log":1,"session":"s","started_unix_ms":0,"server_command":[],"domain":null,"agent_model":null}"#;

/// The entry `entry_line` writes for `line`, read back as the reader would.
fn read_back(line: &[u8]) -> LogEntry {
    let written = String::from_utf8(entry_line(7, Peer::Server, line)).unwrap();
    assert!(written.ends_with('\n') && written.matches('\n').count() == 1);
    let mut log = parse_log(&format!("{HEADER}\n{written}")).unwrap();
    assert_eq!(log.entries.len(), 1);
    log.entries.remove(0)
}

#[test]
fn keeps_json_messages_verbatim() {
    let line = entry_line(
        5,
        Peer::Client,
        b" {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n",
    );
    assert_eq!(
        String::from_utf8(line).unwrap(),
        "{\"t_ms\":5,\"from\":\"client\",\"message\":{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}\n"
    );
    // Key order, number spelling and escapes as sent; no final newline needed.
    let sent = r#"{"z":1.50,"a":"café","batch":[1e3]}"#;
    let written = String::from_utf8(entry_line(0, Peer::Server, sent.as_bytes())).unwrap();
    assert!(written.contains(&format!("\"message\":{sent}}}")));
    assert_eq!(
        read_back(sent.as_bytes()).message,
        serde_json::from_str(sent).ok()
    );
}

#[test]
fn keeps_other_lines_as_raw_text() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let cases: [(&[u8], &str); 6] = [
        (b"Listening on stdio\n", "Listening on stdio"),
        (b"\n", ""),
        // Valid JSON followed by more would otherwise forge the entry.
        (br#"1,"raw":"forged""#, r#"1,"raw":"forged""#),
        (b"\xff\xfe not UTF-8\n", "\u{fffd}\u{fffd} not UTF-8"),
        // JSON the reader cannot hold as a value.
        (br#"{"text":"\ud83d"}"#, r#"{"text":"\ud83d"}"#),
        (deep.as_bytes(), deep.as_str()),
    ];
    for (line, raw) in cases {
        let entry = read_back(line);
        assert_eq!(entry.message, None, "{raw}");
        assert_eq!(entry.raw.as_deref(), Some(raw));
        assert_eq!((entry.t_ms, entry.from), (7, Peer::Server));
    }
}

fn header(session: &str) -> LogHeader {
    let mut header: LogHeader = serde_json::from_str(HEADER).unwrap();
    header.session = session.to_string();
    header
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stretto-record-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Takes the first `ok` writes, then fails each write, or panics.
struct Disk {
    ok: usize,
    panics: bool,
}

impl Write for Disk {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.ok == 0 {
            assert!(!self.panics, "the disk caught fire");
            return Err(std::io::Error::other("the disk is full"));
        }
        self.ok -= 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_log_that_cannot_be_started_says_why_and_leaves_nothing() {
    let dir = scratch("start");
    // Where a file stands, no directory can be made.
    fs::write(dir.join("taken"), "").unwrap();
    let e = Recorder::start(&dir.join("taken/logs"), &header("s"), Instant::now()).err();
    let taken = dir.join("taken/logs").display().to_string();
    assert_eq!(e.unwrap().to_string(), format!("creating {taken}"));
    // A session's log is never written over.
    let first = Recorder::start(&dir, &header("s"), Instant::now()).unwrap();
    let e = Recorder::start(&dir, &header("s"), Instant::now()).err();
    let log = dir.join("s.jsonl");
    assert_eq!(
        e.unwrap().to_string(),
        format!("creating {}", log.display())
    );
    first.finish();
    // A header that cannot be written leaves no log behind.
    let path = dir.join("full.jsonl");
    fs::write(&path, "").unwrap();
    let disk = Box::new(Disk {
        ok: 0,
        panics: false,
    });
    let e = Recorder::start_on(disk, path.clone(), &header("full"), Instant::now()).err();
    assert_eq!(
        e.unwrap().to_string(),
        format!("writing {}", path.display())
    );
    assert!(!path.exists());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn recording_stops_when_the_log_cannot_be_written() {
    let path = PathBuf::from("full.jsonl");
    // The disk fills up after the header: the line is lost, and the
    // session goes on.
    let disk = Box::new(Disk {
        ok: 1,
        panics: false,
    });
    let recorder = Recorder::start_on(disk, path.clone(), &header("s"), Instant::now()).unwrap();
    recorder.tap().record(Peer::Client, b"{}\n");
    recorder.finish();
    // The writer thread panics: finishing says the log may be incomplete.
    let disk = Box::new(Disk {
        ok: 1,
        panics: true,
    });
    let recorder = Recorder::start_on(disk, path, &header("s"), Instant::now()).unwrap();
    recorder.tap().record(Peer::Client, b"{}\n");
    recorder.finish();
}

#[test]
fn session_ids_are_utc_start_times() {
    assert_eq!(session_id(0, 1), "19700101T000000.000Z-1");
    assert_eq!(session_id(951_782_400_000, 42), "20000229T000000.000Z-42");
    assert_eq!(session_id(1_709_251_199_999, 7), "20240229T235959.999Z-7");
    assert_eq!(
        session_id(1_790_198_400_123, 4242),
        "20260923T212000.123Z-4242"
    );
    assert_eq!(session_id(4_107_542_399_000, 7), "21000228T235959.000Z-7");
}

#[test]
fn redacts_credentials_in_the_command() {
    let command: Vec<String> = [
        "npx",
        "-y",
        "some-mcp-server",
        "--api-key",
        "sk-123",
        "--token=abc",
        "GITHUB_TOKEN=ghp_456",
        "--header",
        "Authorization: Bearer xyz",
        "--no-auth",
        "--port",
        "8080",
        "https://example.com/mcp?access_token=789",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(
        redact_command(&command),
        [
            "npx",
            "-y",
            "some-mcp-server",
            "--api-key",
            REDACTED,
            "--token=<redacted>",
            "GITHUB_TOKEN=<redacted>",
            "--header",
            REDACTED,
            "--no-auth",
            "--port",
            "8080",
            "https://example.com/mcp?access_token=<redacted>",
        ]
    );
}
