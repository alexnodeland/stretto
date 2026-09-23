//! A stdio MCP proxy that records what crosses it.
//!
//! [`run`] starts an MCP server as a child process and sits between it and
//! the MCP host. Lines from the host go to the server's stdin, and lines
//! from the server's stdout go to the host, byte for byte, each flushed as
//! soon as it is complete. Forwarding never parses anything, so every line
//! passes through: requests, responses and notifications in either
//! direction, server-initiated requests, and lines that are not JSON at all.
//!
//! With [`Config::record`] set, each line is also handed to a writer thread
//! that appends it to a session log in the format [`stretto_trace::mcp`]
//! reads. A line is handed over just before it is forwarded, so a request
//! always precedes its response in the log. Handing over never waits for the
//! disk, so recording cannot stall the protocol, and the writer thread
//! writes each line out as soon as it gets it, so a crash loses at most the
//! lines still in flight.
//!
//! The proxy's own messages go to stderr, prefixed `stretto-proxy:`; stdout
//! carries only the protocol.

mod record;

use anyhow::{Context, Result};
use record::{Recorder, Tap};
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use stretto_trace::mcp::{LogHeader, Peer, LOG_VERSION};

/// Exit status for the proxy's own failures, such as a log that cannot be
/// created or a server that cannot be started. Like `env(1)`, the proxy
/// keeps 125 for itself so that it is not confused with the server's status.
pub const FAILURE_EXIT_CODE: i32 = 125;

/// What to run, and whether to record it.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The server's command line: the program, then its arguments.
    pub command: Vec<OsString>,
    /// Directory for the session log, created if missing; `None` only
    /// forwards.
    pub record: Option<PathBuf>,
    /// Domain for the log header, e.g. `retail`.
    pub domain: Option<String>,
    /// Model that drives the agent, for the log header.
    pub agent_model: Option<String>,
}

impl Config {
    /// Forward to `command`, without recording.
    pub fn new<I, S>(command: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        Self {
            command: command.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }
}

/// Run the server behind the proxy, with the host on this process's stdin
/// and stdout, and return the status to exit with.
///
/// This returns once the server has exited: after the host closes stdin,
/// which closes the server's stdin in turn, or when the server exits on its
/// own. Either way, everything the server wrote is forwarded first. The
/// status is the server's exit code, or `128 + signal` if a signal killed
/// it, as in a shell.
pub fn run(config: &Config) -> Result<i32> {
    run_with(config, io::stdin(), io::stdout().lock())
}

/// [`run`], with the host's side of the connection given as `host_in` and
/// `host_out`.
///
/// `host_in` is read on a thread of its own. If the server exits first, that
/// thread stays blocked until `host_in` ends, which for a process's stdin is
/// when the process exits.
pub fn run_with<R, W>(config: &Config, host_in: R, mut host_out: W) -> Result<i32>
where
    R: Read + Send + 'static,
    W: Write,
{
    let (program, args) = config
        .command
        .split_first()
        .context("no server command given")?;
    let started = Instant::now();
    let recorder = match &config.record {
        Some(dir) => {
            let recorder = Recorder::start(dir, &header(config), started)?;
            eprintln!("stretto-proxy: recording to {}", recorder.path().display());
            Some(recorder)
        }
        None => None,
    };

    let spawned = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", program.to_string_lossy()));
    let mut server = match spawned {
        Ok(server) => server,
        Err(e) => {
            // Nothing was proxied, so there is no session to keep.
            if let Some(recorder) = recorder {
                recorder.discard();
            }
            return Err(e);
        }
    };
    let to_server = server
        .stdin
        .take()
        .context("the server's stdin is not piped")?;
    let from_server = server
        .stdout
        .take()
        .context("the server's stdout is not piped")?;

    // Host to server. This thread may still be blocked reading the host when
    // the server exits first; it ends with the process.
    let tap = recorder.as_ref().map(Recorder::tap);
    thread::Builder::new()
        .name("stretto-proxy client".into())
        .spawn(move || {
            // Dropping `to_server` when the host's input ends closes the
            // server's stdin, which tells it to shut down.
            pump(
                BufReader::new(host_in),
                to_server,
                Peer::Client,
                tap.as_ref(),
            );
        })
        .context("starting the client thread")?;

    // Server to host, until the server closes its stdout.
    let tap = recorder.as_ref().map(Recorder::tap);
    pump(
        BufReader::new(from_server),
        &mut host_out,
        Peer::Server,
        tap.as_ref(),
    );
    let status = server.wait().context("waiting for the server");
    if let Some(recorder) = recorder {
        recorder.finish();
    }
    Ok(exit_code(status?))
}

/// `path` with a leading `~` replaced by the home directory, as a shell would
/// do. MCP hosts start servers without a shell, so without this,
/// `--record ~/.stretto/logs` would create a directory named `~`.
pub fn expand_home(path: PathBuf) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    expand_home_with(path, home)
}

fn expand_home_with(path: PathBuf, home: Option<OsString>) -> PathBuf {
    if let (Ok(rest), Some(home)) = (path.strip_prefix("~"), home) {
        return PathBuf::from(home).join(rest);
    }
    path
}

/// The log header for a session starting now.
fn header(config: &Config) -> LogHeader {
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let command: Vec<String> = config
        .command
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    LogHeader {
        stretto_mcp_log: LOG_VERSION,
        session: record::session_id(started_unix_ms, std::process::id()),
        started_unix_ms,
        server_command: record::redact_command(&command),
        domain: config.domain.clone(),
        agent_model: config.agent_model.clone(),
    }
}

/// Forward `input` to `output` line by line, unchanged, flushing after each
/// line, until `input` ends.
///
/// Each line goes to `tap` first, so it is in the log before the other side
/// can answer it. If `output` fails, lines are still read and recorded, so
/// the sender never blocks on a full pipe.
fn pump(mut input: impl BufRead, mut output: impl Write, from: Peer, tap: Option<&Tap>) {
    let (sender, receiver) = match from {
        Peer::Client => ("client", "server"),
        Peer::Server => ("server", "client"),
    };
    let mut line = Vec::new();
    let mut forwarding = true;
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                eprintln!("stretto-proxy: reading from the {sender}: {e}");
                return;
            }
        }
        if let Some(tap) = tap {
            tap.record(from, &line);
        }
        if forwarding {
            if let Err(e) = output.write_all(&line).and_then(|()| output.flush()) {
                eprintln!("stretto-proxy: forwarding to the {receiver}: {e}");
                forwarding = false;
            }
        }
    }
}

/// The status to exit with for a server that exited with `status`.
fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(FAILURE_EXIT_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
