use super::*;

#[test]
fn prunes_what_is_older_than_the_retention() {
    let dir = std::env::temp_dir().join(format!("stretto-prune-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ab")).unwrap();
    let old = SystemTime::now() - std::time::Duration::from_secs(10 * 86_400);
    for (name, age) in [
        ("s1.jsonl", Some(old)),
        ("s1.flow.jsonl", Some(old)),
        ("ab/key.json", Some(old)),
        ("s2.jsonl", None),
        ("notes.txt", Some(old)),
    ] {
        let file = std::fs::File::create(dir.join(name)).unwrap();
        if let Some(t) = age {
            file.set_modified(t).unwrap();
        }
    }
    assert_eq!(prune(&dir, 7).unwrap(), 3);
    let left: Vec<bool> = ["s2.jsonl", "notes.txt", "s1.jsonl", "ab/key.json"]
        .iter()
        .map(|n| dir.join(n).exists())
        .collect();
    assert_eq!(left, [true, true, false, false]);
    assert_eq!(prune(&dir.join("missing"), 7).unwrap(), 0);
    // A file is not a directory to prune.
    let e = prune(&dir.join("notes.txt"), 7).unwrap_err();
    assert_eq!(
        e.to_string(),
        format!("listing {}", dir.join("notes.txt").display())
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn expands_a_leading_tilde() {
    let home = || Some(OsString::from("/home/a"));
    let expand = |path: &str, home| expand_home_with(PathBuf::from(path), home);
    assert_eq!(
        expand("~/.stretto/logs", home()),
        PathBuf::from("/home/a/.stretto/logs")
    );
    assert_eq!(expand("~", home()), PathBuf::from("/home/a"));
    assert_eq!(expand("logs/~", home()), PathBuf::from("logs/~"));
    assert_eq!(expand("~other/logs", home()), PathBuf::from("~other/logs"));
    assert_eq!(expand("~/logs", None), PathBuf::from("~/logs"));
}

/// Reads that fail, and writes that fail.
struct Broken;

impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("broken"))
    }
}

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("broken"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn pumps_until_its_input_ends_or_fails() {
    // A read that fails ends the pump, with what it had read so far.
    let mut input = BufReader::new(io::Cursor::new(b"a\nb".to_vec()).chain(Broken));
    let mut output = Vec::new();
    pump(&mut input, &mut output, Peer::Server, None);
    assert_eq!(output, b"a\n");

    // Once the output fails, lines are still read and recorded.
    let dir = std::env::temp_dir().join(format!("stretto-proxy-pump-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let recorder = Recorder::start(&dir, &header(&Config::new(["cat"])), Instant::now()).unwrap();
    let mut input = io::Cursor::new(b"{\"a\":1}\n{\"b\":2}\n".to_vec());
    pump(&mut input, &mut Broken, Peer::Client, Some(&recorder.tap()));
    let path = recorder.path().to_path_buf();
    recorder.finish();
    let log = stretto_trace::mcp::read_log(&path).unwrap();
    assert_eq!(log.messages().count(), 2);
    assert!(log.entries.iter().all(|e| e.from == Peer::Client));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn exit_codes_follow_the_shell() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(exit_code(ExitStatus::from_raw(3 << 8)), 3);
    assert_eq!(exit_code(ExitStatus::from_raw(9)), 128 + 9);
}

/// With `cat` as the server, whatever the host sends comes straight back.
#[cfg(unix)]
#[test]
fn forwards_every_line_unchanged_and_records_it() {
    use stretto_trace::mcp::read_log;

    let input: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\
        not json\r\n\
        \xff\xfe not UTF-8\n\
        [{\"jsonrpc\":\"2.0\",\"method\":\"a\"},{\"jsonrpc\":\"2.0\",\"method\":\"b\"}]\n\
        {\"no\":\"final newline\"}";
    let dir = std::env::temp_dir().join(format!("stretto-proxy-unit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut config = Config::new(["cat"]);
    config.record = Some(dir.join("logs"));
    config.domain = Some("echo".into());
    config.host_session = Some("task-1".into());
    config.server_name = Some("echoes".into());

    let mut output = Vec::new();
    let code = run_with(&config, io::Cursor::new(input.to_vec()), &mut output).unwrap();
    assert_eq!(code, 0);
    assert_eq!(output, input);

    let logs: Vec<PathBuf> = std::fs::read_dir(dir.join("logs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(logs.len(), 1);
    let log = read_log(&logs[0]).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(log.header.server_command, ["cat"]);
    assert_eq!(log.header.domain.as_deref(), Some("echo"));
    assert_eq!(
        (
            log.header.host_session.as_deref(),
            log.header.server_name.as_deref()
        ),
        (Some("task-1"), Some("echoes"))
    );
    let lines = |from| {
        log.entries
            .iter()
            .enumerate()
            .filter(move |(_, e)| e.from == from)
    };
    let raw: Vec<Option<&str>> = lines(Peer::Client).map(|(_, e)| e.raw.as_deref()).collect();
    assert_eq!(
        raw,
        [
            None,
            Some("not json"),
            Some("\u{fffd}\u{fffd} not UTF-8"),
            None,
            None
        ]
    );
    // Each echo is logged after the line it echoes, and times never
    // decrease.
    let sent: Vec<usize> = lines(Peer::Client).map(|(i, _)| i).collect();
    let echoed: Vec<usize> = lines(Peer::Server).map(|(i, _)| i).collect();
    assert_eq!(echoed.len(), sent.len());
    assert!(sent.iter().zip(&echoed).all(|(s, e)| s < e));
    assert!(log.entries.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));
    assert_eq!(log.messages().count(), 8);
}
