//! Jobs: the `stretto` CLI run as subprocesses, one at a time, in the order
//! they were submitted.
//!
//! Each job is kept in `console/jobs/` in the data directory: `<id>.json`,
//! the job without its output, rewritten at each change of status, and
//! `<id>.log`, what the CLI printed, stdout and stderr as they came. The
//! CLI is the one beside the console, else on PATH, else `--stretto`
//! ([`crate::Binaries`]); it runs in the data directory with the console's
//! environment, so it finds the keys and the salt the console was given
//! (the console only checks whether they are set). A job still queued or
//! running when the console stopped is marked failed when it starts again.
//! A queued job can be cancelled, and never runs; a running one's CLI is
//! killed ([`Jobs::cancel`]).

use crate::api::jobs::{Artifact, ArtifactKind, Job, JobStatus, Plan};
use crate::data::{self, paths};
use crate::watch::Event;
use crate::Config;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

/// The output lists and events carry: its last 64 KiB.
pub const TAIL: usize = 64 * 1024;
/// Progress events for one job come at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// How long a cancelled job's last output is read for.
const CANCEL_DRAIN: Duration = Duration::from_secs(2);

/// The jobs, and the queue of those to run.
#[derive(Clone)]
pub struct Jobs {
    inner: Arc<Inner>,
    /// The runner's queue. The runner holds none of it, so it ends once
    /// the last `Jobs` is gone.
    queue: mpsc::UnboundedSender<(String, Vec<String>)>,
}

struct Inner {
    /// `console/jobs/` in the data directory.
    dir: PathBuf,
    data_dir: PathBuf,
    home: Option<PathBuf>,
    stretto: Option<PathBuf>,
    read_only: bool,
    /// A fixed clock, for tests.
    now: Option<u64>,
    /// Every job, oldest first.
    jobs: Mutex<Vec<Job>>,
    /// The running job's id, and what cancels it. Taken after `jobs` when
    /// both are held.
    running: Mutex<Option<(String, oneshot::Sender<()>)>>,
    events: broadcast::Sender<Event>,
}

/// A new job's id: the time, as stretto's session ids start, and four hex
/// digits of chance.
pub fn new_id(now: u64) -> String {
    format!("{}-{:04x}", crate::time::stamp(now), rand::random::<u16>())
}

impl Jobs {
    /// The jobs kept in `config`'s data directory, and a runner for new
    /// ones. Must be called inside a Tokio runtime.
    pub fn start(config: &Config, events: broadcast::Sender<Event>) -> Jobs {
        let dir = config.data_dir.join(data::CONSOLE_DIR).join("jobs");
        let jobs = load(&dir, !config.read_only);
        let (queue, mut next) = mpsc::unbounded_channel::<(String, Vec<String>)>();
        let inner = Arc::new(Inner {
            dir,
            data_dir: config.data_dir.clone(),
            home: config.home.clone(),
            stretto: config
                .binaries
                .stretto
                .as_ref()
                .map(|b| PathBuf::from(&b.path)),
            read_only: config.read_only,
            now: config.now_unix_ms,
            jobs: Mutex::new(jobs),
            running: Mutex::new(None),
            events,
        });
        let runner = inner.clone();
        tokio::spawn(async move {
            // A job cancelled while queued is skipped.
            while let Some((id, args)) = next.recv().await {
                runner.run(&id, args).await;
            }
        });
        Jobs { inner, queue }
    }

    /// Every job, newest first, with the tail of its output.
    pub fn list(&self) -> Vec<Job> {
        let jobs = self.inner.lock();
        jobs.iter()
            .rev()
            .map(|j| self.inner.with_output(j, Some(TAIL)))
            .collect()
    }

    /// One job, with all its output.
    pub fn get(&self, id: &str) -> Option<Job> {
        let jobs = self.inner.lock();
        jobs.iter()
            .find(|j| j.id == id)
            .map(|j| self.inner.with_output(j, None))
    }

    /// Cancel job `id` at `now`: a queued one is cancelled at once; a
    /// running one's CLI is killed, and the runner marks it cancelled when
    /// the CLI has ended. The job as it is now.
    pub fn cancel(&self, id: &str, now: u64) -> Result<Job, CancelError> {
        let job = {
            let mut jobs = self.inner.lock();
            let job = jobs
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or(CancelError::NotFound)?;
            match job.status {
                JobStatus::Queued => {
                    job.status = JobStatus::Cancelled;
                    job.finished_unix_ms = Some(now);
                }
                JobStatus::Running => {
                    let mut running = self
                        .inner
                        .running
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some((_, stop)) = running.take_if(|(r, _)| r == id) {
                        let _ = stop.send(());
                    }
                    return Ok(self.inner.with_output(job, Some(TAIL)));
                }
                status => return Err(CancelError::Ended(status)),
            }
            job.clone()
        };
        let note = "stretto-console: cancelled before it started\n";
        if !self.inner.read_only {
            let _ = std::fs::create_dir_all(&self.inner.dir);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.inner.log(id))
                .and_then(|mut f| std::io::Write::write_all(&mut f, note.as_bytes()));
        }
        self.inner.save(&job);
        self.inner.announce(&job);
        Ok(self.inner.with_output(&job, Some(TAIL)))
    }

    /// Queue `plan` as job `id`, created `now`.
    pub fn submit(&self, id: String, plan: Plan, now: u64) -> Job {
        let job = Job {
            id: id.clone(),
            kind: plan.kind,
            title: plan.title,
            params: plan.params,
            status: JobStatus::Queued,
            created_unix_ms: now,
            started_unix_ms: None,
            finished_unix_ms: None,
            exit_code: None,
            alarm: false,
            output: String::new(),
            artifacts: plan.artifacts,
        };
        self.inner.lock().push(job.clone());
        self.inner.save(&job);
        self.inner.announce(&job);
        let _ = self.queue.send((id, plan.args));
        job
    }
}

impl Inner {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Job>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn log(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.log"))
    }

    /// `job` with its output: the last `tail` bytes, or all of it.
    fn with_output(&self, job: &Job, tail: Option<usize>) -> Job {
        let mut job = job.clone();
        job.output = read_output(&self.log(&job.id), tail);
        job
    }

    /// Change job `id` with `f`, keep it, and announce it.
    fn update(&self, id: &str, f: impl FnOnce(&mut Job)) -> Option<Job> {
        let job = {
            let mut jobs = self.lock();
            let job = jobs.iter_mut().find(|j| j.id == id)?;
            f(job);
            job.clone()
        };
        self.save(&job);
        self.announce(&job);
        Some(job)
    }

    /// Write `<id>.json`, the job without its output.
    fn save(&self, job: &Job) {
        if self.read_only {
            return;
        }
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let mut kept = job.clone();
            kept.output = String::new();
            let path = self.dir.join(format!("{}.json", job.id));
            let temp = self.dir.join(format!(".{}.json.tmp", job.id));
            let text = serde_json::to_vec_pretty(&kept).map_err(std::io::Error::other)?;
            std::fs::write(&temp, text)?;
            std::fs::rename(&temp, &path)
        };
        if let Err(e) = write() {
            eprintln!("stretto-console: keeping job {}: {e}", job.id);
        }
    }

    /// A `job` event, with the tail of the output.
    fn announce(&self, job: &Job) {
        let _ = self
            .events
            .send(Event::Job(Box::new(self.with_output(job, Some(TAIL)))));
    }

    async fn run(&self, id: &str, args: Vec<String>) {
        let fixed = self.now;
        let now = move || fixed.unwrap_or_else(crate::now_unix_ms);
        // Start it only if it is still queued, and make it cancellable in
        // the same step, so a cancel cannot fall between the two.
        let (stop, stopped) = oneshot::channel();
        let started = {
            let mut jobs = self.lock();
            match jobs.iter_mut().find(|j| j.id == id) {
                Some(job) if job.status == JobStatus::Queued => {
                    job.status = JobStatus::Running;
                    job.started_unix_ms = Some(now());
                    *self
                        .running
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some((id.to_string(), stop));
                    Some(job.clone())
                }
                _ => None,
            }
        };
        let Some(job) = started else {
            return;
        };
        self.save(&job);
        self.announce(&job);
        let log_path = self.log(id);
        let outcome = self.execute(id, &args, &log_path, stopped).await;
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let (status, code, alarm) = match outcome {
            Ok(Ended::Exited(0)) => (JobStatus::Succeeded, Some(0), false),
            // It did its work, and found something to act on.
            Ok(Ended::Exited(code)) if job.kind.alarm_exit() == Some(code) => {
                (JobStatus::Succeeded, Some(code), true)
            }
            Ok(Ended::Exited(code)) => (JobStatus::Failed, Some(code), false),
            Ok(Ended::Cancelled(code)) => (JobStatus::Cancelled, Some(code), false),
            Err(e) => {
                append(&log_path, &format!("stretto-console: {e}\n")).await;
                (JobStatus::Failed, None, false)
            }
        };
        let catalog = data::catalog(&self.data_dir);
        self.update(id, |j| {
            j.status = status;
            j.exit_code = code;
            j.alarm = alarm;
            j.finished_unix_ms = Some(now());
            j.artifacts = std::mem::take(&mut j.artifacts)
                .into_iter()
                .filter_map(|a| {
                    let path =
                        paths::resolve(&self.data_dir, self.home.as_deref(), &a.path).ok()?;
                    path.exists().then(|| Artifact {
                        key: match a.kind {
                            ArtifactKind::Flow => catalog.flow_at(&path).map(|f| f.key.clone()),
                            _ => None,
                        },
                        ..a
                    })
                })
                .collect();
        });
    }

    /// Run the CLI with `args`, its output into `log`, until it ends or
    /// `stop` fires: how it ended.
    async fn execute(
        &self,
        id: &str,
        args: &[String],
        log: &Path,
        mut stop: oneshot::Receiver<()>,
    ) -> Result<Ended, String> {
        let stretto = self.stretto.as_ref().ok_or(
            "the stretto CLI was not found beside the console or on PATH; pass --stretto PATH",
        )?;
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| format!("creating {}: {e}", self.dir.display()))?;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .await
            .map_err(|e| format!("opening the job's log: {e}"))?;
        let shown: Vec<String> = args
            .iter()
            .map(|a| stretto_report::init::shell_quote(a))
            .collect();
        let _ = file
            .write_all(format!("$ stretto {}\n", shown.join(" ")).as_bytes())
            .await;
        let mut child = tokio::process::Command::new(stretto)
            .args(args)
            .current_dir(&self.data_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("starting {}: {e}", stretto.display()))?;
        let (lines, mut received) = mpsc::unbounded_channel::<String>();
        let pipe = |out: Box<dyn AsyncRead + Unpin + Send>,
                    lines: mpsc::UnboundedSender<String>| {
            tokio::spawn(async move {
                let mut reader = BufReader::new(out).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    // Until no one reads on: the job has ended.
                    let Ok(()) = lines.send(line) else { break };
                }
            })
        };
        if let Some(out) = child.stdout.take() {
            pipe(Box::new(out), lines.clone());
        }
        if let Some(err) = child.stderr.take() {
            pipe(Box::new(err), lines.clone());
        }
        drop(lines);
        let mut last = Instant::now();
        let mut cancelled = false;
        loop {
            tokio::select! {
                line = received.recv() => {
                    let Some(line) = line else { break };
                    let _ = file.write_all(format!("{line}\n").as_bytes()).await;
                    if last.elapsed() >= PROGRESS_EVERY {
                        let _ = file.flush().await;
                        last = Instant::now();
                        let job = self.lock().iter().find(|j| j.id == id).cloned();
                        if let Some(job) = job {
                            self.announce(&job);
                        }
                    }
                }
                // Only a cancel ends `stop`: its sender is kept, in
                // `running`, until this returns.
                _ = &mut stop => {
                    cancelled = true;
                    let _ = child.start_kill();
                    break;
                }
            }
        }
        if cancelled {
            // What it printed before it was killed, for a moment only: a
            // process it started may hold the pipes open after it.
            let rest = async {
                while let Some(line) = received.recv().await {
                    let _ = file.write_all(format!("{line}\n").as_bytes()).await;
                }
            };
            let _ = tokio::time::timeout(CANCEL_DRAIN, rest).await;
            let _ = file.write_all(b"stretto-console: cancelled\n").await;
        }
        let _ = file.flush().await;
        // A child that cannot be waited for has no code to give.
        let code = child.wait().await.map_or(-1, exit_code);
        Ok(if cancelled {
            Ended::Cancelled(code)
        } else {
            Ended::Exited(code)
        })
    }
}

/// Why [`Jobs::cancel`] refused.
#[derive(Debug, PartialEq, Eq)]
pub enum CancelError {
    /// No job has the id.
    NotFound,
    /// The job has ended, as it says.
    Ended(JobStatus),
}

/// How a job's CLI ended: on its own, or killed by a cancel; its exit code
/// either way.
enum Ended {
    Exited(i32),
    Cancelled(i32),
}

/// The exit code, or `128 + n` if signal `n` ended it, as a shell reports.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(-1)
}

async fn append(path: &Path, text: &str) {
    if let Ok(mut f) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    {
        let _ = f.write_all(text.as_bytes()).await;
    }
}

/// A job's output: the last `tail` bytes (from a line's start), or all.
fn read_output(log: &Path, tail: Option<usize>) -> String {
    let Ok(bytes) = std::fs::read(log) else {
        return String::new();
    };
    let start = match tail {
        Some(n) if bytes.len() > n => {
            let from = bytes.len() - n;
            bytes[from..]
                .iter()
                .position(|b| *b == b'\n')
                .map_or(from, |i| from + i + 1)
        }
        _ => 0,
    };
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// The jobs kept in `dir`, oldest first. One left queued or running by a
/// console that stopped is failed now (and kept so, when `write`).
fn load(dir: &Path, write: bool) -> Vec<Job> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut jobs: Vec<Job> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                && !p.to_string_lossy().ends_with(".audit.json")
        })
        .filter_map(|p| serde_json::from_slice::<Job>(&std::fs::read(p).ok()?).ok())
        .collect();
    jobs.sort_by(|a, b| {
        a.created_unix_ms
            .cmp(&b.created_unix_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    for job in &mut jobs {
        if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
            job.status = JobStatus::Failed;
            job.finished_unix_ms = Some(crate::now_unix_ms());
            if write {
                let log = dir.join(format!("{}.log", job.id));
                let note = "stretto-console: the console stopped before this job finished\n";
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, note.as_bytes()));
                let _ = std::fs::write(
                    dir.join(format!("{}.json", job.id)),
                    serde_json::to_vec_pretty(job).unwrap_or_default(),
                );
            }
        }
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_starts_at_a_line() {
        let dir = std::env::temp_dir().join(format!("stretto-console-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("j.log");
        std::fs::write(&log, "first line\nsecond line\nthird\n").unwrap();
        assert_eq!(read_output(&log, None), "first line\nsecond line\nthird\n");
        assert_eq!(read_output(&log, Some(10)), "third\n");
        assert_eq!(read_output(&dir.join("none.log"), Some(10)), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_job_left_running_is_failed_on_start() {
        let dir =
            std::env::temp_dir().join(format!("stretto-console-jobs-left-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let job = Job {
            id: "20260928T000000.000Z-0001".into(),
            kind: crate::api::jobs::JobKind::Doctor,
            title: "Check".into(),
            params: serde_json::json!({"kind": "doctor"}),
            status: JobStatus::Running,
            created_unix_ms: 1,
            started_unix_ms: Some(2),
            finished_unix_ms: None,
            exit_code: None,
            alarm: false,
            output: String::new(),
            artifacts: Vec::new(),
        };
        std::fs::write(
            dir.join(format!("{}.json", job.id)),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("x.audit.json"), "{}").unwrap();
        let jobs = load(&dir, true);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Failed);
        assert!(read_output(&dir.join(format!("{}.log", job.id)), None).contains("stopped before"));
        assert!(new_id(1_790_561_041_312).starts_with("20260928T020401.312Z-"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn jobs_are_loaded_oldest_first_and_only_unfinished_ones_fail() {
        let dir =
            std::env::temp_dir().join(format!("stretto-console-jobs-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let job = |id: &str, created: u64, status: JobStatus| Job {
            id: id.into(),
            kind: crate::api::jobs::JobKind::Doctor,
            title: "Check".into(),
            params: serde_json::json!({"kind": "doctor"}),
            status,
            created_unix_ms: created,
            started_unix_ms: None,
            finished_unix_ms: None,
            exit_code: None,
            alarm: false,
            output: String::new(),
            artifacts: Vec::new(),
        };
        for j in [
            job("b", 1, JobStatus::Succeeded),
            job("a", 1, JobStatus::Queued),
            job("c", 0, JobStatus::Failed),
        ] {
            let path = dir.join(format!("{}.json", j.id));
            std::fs::write(path, serde_json::to_vec(&j).unwrap()).unwrap();
        }
        // Read-only: the queued job is failed, and nothing is written.
        let jobs = load(&dir, false);
        let ids: Vec<&str> = jobs.iter().map(|j| j.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(jobs[1].status, JobStatus::Failed);
        assert_eq!(jobs[2].status, JobStatus::Succeeded);
        assert!(!dir.join("a.log").exists());
        assert!(load(&dir.join("missing"), true).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
