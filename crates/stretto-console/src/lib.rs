//! `stretto-console`: stretto's management plane, a local web app over the
//! files stretto writes to `~/.stretto`.
//!
//! It reads what the rest of stretto already writes, through the same
//! library: sessions through [`stretto_trace::mcp`], flows through
//! [`stretto_report::flow::Flow`] and [`stretto_report::review::view`], the
//! host configuration through [`stretto_report::init`], and the data
//! directory's layout as [`stretto_report::doctor::scan_with`] walks it.
//! The one file it owns is the server registry, `servers.json`
//! ([`data::registry`]), and its own state lives in `console/`: jobs
//! ([`jobs`], the `stretto` CLI run as subprocesses) and the trash.
//!
//! [`router`] is the whole HTTP surface, the JSON API under `/api` and the
//! embedded UI ([`assets`]), behind [`auth`]; `main.rs` only parses the
//! command line and serves it.

pub mod api;
pub mod assets;
pub mod auth;
pub mod data;
pub mod jobs;
pub mod probe;
pub mod time;
pub mod watch;

pub use api::router;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

/// The version of this build, which `/api/health` and `/api/meta` report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How the console runs.
#[derive(Clone, Debug)]
pub struct Config {
    /// The data directory: `--data`, else `$STRETTO_HOME`, else `~/.stretto`.
    pub data_dir: PathBuf,
    /// The token `/api` requires; `None` with `--no-auth`.
    pub token: Option<String>,
    /// `--read-only`: every write and action is refused.
    pub read_only: bool,
    /// The port the console listens on, which the Host check under
    /// `--no-auth` accepts.
    pub port: u16,
    /// stretto's binaries, as found at start: `stretto` runs the jobs.
    pub binaries: Binaries,
    /// The home directory, for paths that start with `~/`.
    pub home: Option<PathBuf>,
    /// A fixed "now", in milliseconds since the Unix epoch, for tests.
    pub now_unix_ms: Option<u64>,
    /// What the console reads from its environment.
    pub env: Environment,
}

/// The variables whose presence the console reads: the keys for TypeSafe's
/// Jev, and the salt `stretto redact` needs.
pub const VARIABLES: [&str; 3] = [
    "TYPESAFE_API_KEY",
    "TYPESAFE_API_KEY_FILE",
    "STRETTO_REDACT_SALT",
];

/// What the console reads from its environment, once, when it starts:
/// which of [`VARIABLES`] are set (never their values), and PATH.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    /// Those of [`VARIABLES`] that are set and not empty.
    pub set: BTreeSet<String>,
    /// PATH, where the console looks for programs.
    pub path: OsString,
}

impl Environment {
    /// This process's.
    pub fn of_process() -> Self {
        Environment {
            set: VARIABLES
                .into_iter()
                .filter(|v| env_set(v))
                .map(String::from)
                .collect(),
            path: std::env::var_os("PATH").unwrap_or_default(),
        }
    }

    /// Whether `name`, one of [`VARIABLES`], is set and not empty.
    pub fn is_set(&self, name: &str) -> bool {
        self.set.contains(name)
    }
}

/// stretto's binaries, as found beside the console or on PATH.
#[derive(Clone, Debug, Default)]
pub struct Binaries {
    pub stretto: Option<api::meta::Binary>,
    pub proxy: Option<api::meta::Binary>,
    pub procedure: Option<api::meta::Binary>,
    pub demo: Option<api::meta::Binary>,
}

impl Config {
    /// A configuration for `data_dir` requiring `token` (none: no auth),
    /// not read-only, with no binaries found.
    pub fn new(data_dir: impl Into<PathBuf>, token: Option<String>) -> Self {
        Config {
            data_dir: data_dir.into(),
            token,
            read_only: false,
            port: 7878,
            binaries: Binaries::default(),
            home: home_dir(),
            now_unix_ms: None,
            env: Environment::of_process(),
        }
    }
}

/// What every handler shares.
pub struct State {
    pub config: Config,
    /// Parsed sessions and flows, kept while their files are unchanged.
    pub cache: Mutex<data::Cache>,
    /// Held while the registry is read, changed and written back.
    pub registry_lock: tokio::sync::Mutex<()>,
    pub jobs: jobs::Jobs,
    /// Live updates for `/api/events`.
    pub events: broadcast::Sender<watch::Event>,
}

/// The state, shared.
pub type Shared = Arc<State>;

impl State {
    /// The state for `config`, with its job runner started. Must be called
    /// inside a Tokio runtime.
    pub fn start(config: Config) -> Shared {
        let (events, _) = broadcast::channel(256);
        let jobs = jobs::Jobs::start(&config, events.clone());
        Arc::new(State {
            config,
            cache: Mutex::new(data::Cache::default()),
            registry_lock: tokio::sync::Mutex::new(()),
            jobs,
            events,
        })
    }

    /// The data directory.
    pub fn data_dir(&self) -> &Path {
        &self.config.data_dir
    }

    /// Now, in milliseconds since the Unix epoch.
    pub fn now(&self) -> u64 {
        self.config.now_unix_ms.unwrap_or_else(now_unix_ms)
    }

    /// Whether a key for TypeSafe's Jev is configured, from whether
    /// `TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` is set; never its value.
    pub fn key_set(&self) -> bool {
        let env = &self.config.env;
        env.is_set("TYPESAFE_API_KEY") || env.is_set("TYPESAFE_API_KEY_FILE")
    }
}

/// Whether the environment variable `name` is set and not empty. Only its
/// presence is read.
pub fn env_set(name: &str) -> bool {
    !std::env::var_os(name).unwrap_or_default().is_empty()
}

/// Now, in milliseconds since the Unix epoch.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The home directory: HOME, or USERPROFILE on Windows.
pub fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .find_map(std::env::var_os)
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}
