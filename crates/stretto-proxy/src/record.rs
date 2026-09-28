//! The session log: its lines, and the thread that writes them.

use anyhow::{Context, Result};
use serde::de::IgnoredAny;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use stretto_trace::mcp::{LogEntry, LogHeader, Peer};

/// What the writer thread receives.
enum Record {
    /// A line as read, newline included.
    Line {
        t_ms: u64,
        from: Peer,
        bytes: Vec<u8>,
    },
    /// The session is over.
    End,
}

/// Hands lines to the writer thread. Each direction holds a clone.
#[derive(Clone)]
pub(crate) struct Tap {
    tx: Arc<Mutex<Sender<Record>>>,
    started: Instant,
}

impl Tap {
    /// Record `line`, sent by `from`. This queues the line and returns
    /// without waiting for the disk.
    pub(crate) fn record(&self, from: Peer, line: &[u8]) {
        let bytes = line.to_vec();
        // Timestamp under the lock, so times never decrease down the log.
        if let Ok(tx) = self.tx.lock() {
            let t_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            // Sending fails only once the writer has stopped, which it reports.
            let _ = tx.send(Record::Line { t_ms, from, bytes });
        }
    }

    fn end(&self) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Record::End);
        }
    }
}

/// A session log being written.
pub(crate) struct Recorder {
    tap: Tap,
    writer: JoinHandle<()>,
    path: PathBuf,
}

impl Recorder {
    /// Create `dir` if needed, start `<dir>/<session>.jsonl` with `header`,
    /// and start the writer thread. `started` is time zero for `t_ms`.
    pub(crate) fn start(dir: &Path, header: &LogHeader, started: Instant) -> Result<Self> {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(format!("{}.jsonl", header.session));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("creating {}", path.display()))?;
        Self::start_on(Box::new(file), path, header, started)
    }

    /// [`Recorder::start`], writing to `log`, the file at `path`.
    fn start_on(
        mut log: Box<dyn Write + Send>,
        path: PathBuf,
        header: &LogHeader,
        started: Instant,
    ) -> Result<Self> {
        let mut line = serde_json::to_vec(header).expect("headers always serialize");
        line.push(b'\n');

        let (tx, rx) = mpsc::channel();
        let writer = log
            .write_all(&line)
            .with_context(|| format!("writing {}", path.display()))
            .and_then(|()| {
                let path = path.clone();
                thread::Builder::new()
                    .name("stretto-proxy recorder".into())
                    .spawn(move || write_entries(rx, log, &path))
                    .context("starting the recorder thread")
            });
        match writer {
            Ok(writer) => Ok(Self {
                tap: Tap {
                    tx: Arc::new(Mutex::new(tx)),
                    started,
                },
                writer,
                path,
            }),
            Err(e) => {
                // A log with no session in it would only mislead.
                let _ = fs::remove_file(&path);
                Err(e)
            }
        }
    }

    /// A handle for recording lines.
    pub(crate) fn tap(&self) -> Tap {
        self.tap.clone()
    }

    /// Where the log is.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Write out every line recorded so far, then stop. Lines recorded after
    /// this are dropped.
    pub(crate) fn finish(self) {
        self.tap.end();
        if self.writer.join().is_err() {
            eprintln!(
                "stretto-proxy: the recorder thread panicked; {} may be incomplete",
                self.path.display()
            );
        }
    }

    /// Stop, and delete the log.
    pub(crate) fn discard(self) {
        let path = self.path.clone();
        self.finish();
        let _ = fs::remove_file(path);
    }
}

/// The writer thread: one log line per record, each written as soon as it
/// arrives, until [`Record::End`].
fn write_entries(rx: Receiver<Record>, mut file: Box<dyn Write + Send>, path: &Path) {
    while let Ok(Record::Line { t_ms, from, bytes }) = rx.recv() {
        if let Err(e) = file.write_all(&entry_line(t_ms, from, &bytes)) {
            eprintln!(
                "stretto-proxy: recording stopped: writing {}: {e}",
                path.display()
            );
            return;
        }
    }
}

/// The log line for `line`, read from `from` at `t_ms`.
///
/// A line that is one JSON value is kept verbatim as `message`, without its
/// newline and surrounding whitespace, provided the entry reads back as a
/// [`LogEntry`]. Anything else is kept as `raw` text, with invalid UTF-8
/// replaced.
pub(crate) fn entry_line(t_ms: u64, from: Peer, line: &[u8]) -> Vec<u8> {
    let text = line.strip_suffix(b"\n").unwrap_or(line);
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    // Exactly one JSON value: embedding anything more could forge the
    // entry's other fields.
    let json = std::str::from_utf8(text)
        .ok()
        .filter(|json| serde_json::from_str::<IgnoredAny>(json).is_ok());
    if let Some(json) = json {
        let from = serde_json::to_string(&from).expect("peers always serialize");
        let entry = format!(
            "{{\"t_ms\":{t_ms},\"from\":{from},\"message\":{}}}\n",
            json.trim_ascii()
        );
        // JSON that serde_json can skip over but not hold as a value (lone
        // surrogates, very deep nesting) would make the log unreadable.
        if serde_json::from_str::<LogEntry>(&entry).is_ok() {
            return entry.into_bytes();
        }
    }
    let entry = LogEntry {
        t_ms,
        from,
        message: None,
        raw: Some(String::from_utf8_lossy(text).into_owned()),
    };
    let mut out = serde_json::to_vec(&entry).expect("log entries always serialize");
    out.push(b'\n');
    out
}

/// A session id: the UTC start time to the millisecond, then the process id,
/// e.g. `20260923T212000.123Z-4242`. Ids sort by start time.
pub(crate) fn session_id(unix_ms: u64, pid: u32) -> String {
    let secs = unix_ms / 1000;
    let (year, month, day) = civil_date(secs / 86_400);
    let s = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{:03}Z-{pid}",
        s / 3600,
        s / 60 % 60,
        s % 60,
        unix_ms % 1000
    )
}

/// The Gregorian date `days` after 1970-01-01, as (year, month, day).
/// Howard Hinnant's `civil_from_days`, for non-negative days.
fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + u64::from(month <= 2), month, day)
}

/// Stands in for a redacted value in the log header.
pub(crate) const REDACTED: &str = "<redacted>";

/// `command` as the log header shows it, with values that look like
/// credentials replaced by `<redacted>`.
///
/// A value is redacted when it follows a flag whose name mentions a key,
/// token, secret, password, credential or auth (`--api-key X`), when it is
/// assigned to such a name (`--token=X`, `GITHUB_TOKEN=X`), or when the
/// argument is an authorization header (`Authorization: Bearer X`). This is
/// best effort: credentials belong in the server's environment, which the
/// proxy never records.
pub(crate) fn redact_command(command: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(command.len());
    let mut after_secret_flag = false;
    for arg in command {
        let lower = arg.to_ascii_lowercase();
        let flag_value = std::mem::take(&mut after_secret_flag) && !arg.starts_with('-');
        let auth_header = lower.starts_with("authorization:") || lower.contains("bearer ");
        let shown = if flag_value || auth_header {
            REDACTED.to_string()
        } else if let Some((name, _)) = arg.split_once('=').filter(|(name, _)| names_secret(name)) {
            format!("{name}={REDACTED}")
        } else {
            after_secret_flag = arg.starts_with('-') && !arg.contains('=') && names_secret(arg);
            arg.clone()
        };
        out.push(shown);
    }
    out
}

/// Whether a flag or variable name suggests its value is a credential.
pub(crate) fn names_secret(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "auth",
    ]
    .iter()
    .any(|word| name.contains(word))
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
