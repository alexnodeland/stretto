//! `stretto-console`: serve the console over a data directory, or check that
//! one is serving (`healthcheck`, for a container).

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{ExitCode, Stdio};
use std::time::Duration;
use stretto_console::api::meta;
use stretto_console::{auth, Binaries, Config, Environment, State};

/// stretto's management plane: a local web console over the sessions,
/// flows, servers and jobs in the data directory. It prints the URL to open,
/// with the token that signs the browser in.
#[derive(Parser)]
#[command(name = "stretto-console", version)]
struct Cli {
    /// The data directory (default: $STRETTO_HOME, else ~/.stretto).
    #[arg(long, value_name = "DIR")]
    data: Option<PathBuf>,
    /// Where to listen (default: $STRETTO_CONSOLE_LISTEN, else
    /// 127.0.0.1:7878).
    #[arg(long, value_name = "ADDR")]
    listen: Option<SocketAddr>,
    /// The token the API requires (default: $STRETTO_CONSOLE_TOKEN, else a
    /// new one, printed with the URL).
    #[arg(long, value_name = "TOKEN", conflicts_with = "no_auth")]
    token: Option<String>,
    /// Require no token. Refused unless --listen is a loopback address;
    /// requests must then name localhost, 127.0.0.1 or [::1] as their Host.
    #[arg(long)]
    no_auth: bool,
    /// Change nothing: every write and action (jobs, the connection test)
    /// is refused.
    #[arg(long)]
    read_only: bool,
    /// Open the console in the browser ($BROWSER when it is set).
    #[arg(long)]
    open: bool,
    /// The `stretto` CLI that jobs run (default: beside this binary, else on
    /// PATH).
    #[arg(long, value_name = "PATH")]
    stretto: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// GET /api/health on 127.0.0.1 at the --listen port: exit 0 if the
    /// console answers, 1 if not (a container's healthcheck).
    Healthcheck {
        #[arg(long, value_name = "ADDR")]
        listen: Option<SocketAddr>,
    },
}

const DEFAULT_LISTEN: &str = "127.0.0.1:7878";

/// The program that opens a URL in the browser here, and its arguments
/// before the URL.
#[cfg(target_os = "macos")]
const BROWSER: (&str, &[&str]) = ("open", &[]);
#[cfg(windows)]
const BROWSER: (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
#[cfg(not(any(target_os = "macos", windows)))]
const BROWSER: (&str, &[&str]) = ("xdg-open", &[]);

/// What the console takes from the process it runs in.
struct Env {
    /// STRETTO_CONSOLE_LISTEN
    listen: Option<String>,
    /// STRETTO_CONSOLE_TOKEN
    token: Option<String>,
    /// STRETTO_HOME
    stretto_home: Option<OsString>,
    /// HOME, or USERPROFILE on Windows.
    home: Option<PathBuf>,
    /// BROWSER
    browser: Option<OsString>,
    /// What the console's API reads: which keys are set, and PATH.
    console: Environment,
    /// The directory this binary is in, where the other binaries are looked
    /// for first.
    beside: Option<PathBuf>,
    /// Whether this build has a UI.
    ui_built: bool,
}

impl Env {
    fn of_process() -> Env {
        let var = |name| std::env::var(name).ok();
        Env {
            listen: var("STRETTO_CONSOLE_LISTEN"),
            token: var("STRETTO_CONSOLE_TOKEN"),
            stretto_home: std::env::var_os("STRETTO_HOME"),
            home: stretto_console::home_dir(),
            browser: std::env::var_os("BROWSER"),
            console: Environment::of_process(),
            beside: std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(Path::to_path_buf)),
            ui_built: stretto_console::assets::built(),
        }
    }
}

/// What stops the console.
type Stop = Pin<Box<dyn Future<Output = ()> + Send>>;

#[tokio::main]
async fn main() -> ExitCode {
    let stop = Box::pin(stop_signal());
    run(
        Cli::parse(),
        &Env::of_process(),
        stop,
        &mut std::io::stderr(),
    )
    .await
}

/// Ctrl-C, or SIGTERM (a container stopping).
#[cfg(unix)]
async fn stop_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    // Where SIGTERM cannot be caught, Ctrl-C alone stops the console.
    let mut term = signal(SignalKind::terminate()).ok();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        Some(()) = async { term.as_mut()?.recv().await } => {}
    }
}

/// Ctrl-C.
#[cfg(not(unix))]
async fn stop_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Serve until `stop`, or check that a console is serving. What the console
/// has to say goes to `out`.
async fn run(cli: Cli, env: &Env, stop: Stop, out: &mut dyn Write) -> ExitCode {
    let done = match cli.command {
        Some(Command::Healthcheck { listen }) => healthcheck(listen.or(cli.listen), env).await,
        None => serve(cli, env, stop, out).await,
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(out, "stretto-console: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// The address to listen on: the option, else STRETTO_CONSOLE_LISTEN
/// (`from_env`), else the default.
fn listen_address(given: Option<SocketAddr>, from_env: Option<&str>) -> Result<SocketAddr> {
    let from_env = from_env.map(str::trim).filter(|v| !v.is_empty());
    match (given, from_env) {
        (Some(addr), _) => Ok(addr),
        (None, Some(v)) => v
            .parse()
            .with_context(|| format!("STRETTO_CONSOLE_LISTEN={v:?} is not an address")),
        (None, None) => Ok(DEFAULT_LISTEN.parse().expect("the default parses")),
    }
}

/// The data directory: `--data`, else STRETTO_HOME, else `~/.stretto`.
fn data_dir(given: Option<&Path>, env: &Env) -> Result<PathBuf> {
    match (given, env.stretto_home.as_deref()) {
        (Some(d), _) => Ok(d.to_path_buf()),
        (None, Some(h)) if !h.is_empty() => Ok(PathBuf::from(h)),
        _ => env
            .home
            .as_ref()
            .map(|h| h.join(".stretto"))
            .context("neither HOME nor USERPROFILE is set: pass --data"),
    }
}

/// The token, and whether to print it: none with `--no-auth`; the one
/// given (`--token`, else STRETTO_CONSOLE_TOKEN), never printed; else a new
/// one, printed in the URL to open.
fn token(cli: &Cli, env: &Env) -> (Option<String>, bool) {
    let given = [cli.token.clone(), env.token.clone()]
        .into_iter()
        .flatten()
        .find(|t| !t.is_empty());
    match (cli.no_auth, given) {
        (true, _) => (None, false),
        (false, Some(t)) => (Some(t), false),
        (false, None) => (Some(auth::new_token()), true),
    }
}

/// How a URL names the host at `ip`: a loopback address for an unspecified
/// one.
fn url_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "[::1]".to_string(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

async fn serve(cli: Cli, env: &Env, stop: Stop, out: &mut dyn Write) -> Result<()> {
    let listen = listen_address(cli.listen, env.listen.as_deref())?;
    if cli.no_auth && !listen.ip().is_loopback() {
        bail!(
            "--no-auth is refused on {listen}: without a token, listen on a loopback address \
             (127.0.0.1 or [::1])"
        );
    }
    let data = data_dir(cli.data.as_deref(), env)?;
    if !data.exists() {
        if cli.read_only {
            bail!(
                "{} does not exist (and --read-only creates nothing)",
                data.display()
            );
        }
        std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
    }
    let data = stretto_console::data::paths::canonical(&data);
    if !data.is_dir() {
        bail!("{} is not a directory", data.display());
    }
    let (token, printed) = token(&cli, env);
    let path = &env.console.path;
    let find = |name, explicit| meta::find(name, explicit, env.beside.as_deref(), path);
    let binaries = Binaries {
        stretto: find("stretto", cli.stretto.as_deref()),
        proxy: find("stretto-proxy", None),
        procedure: find("stretto-procedure", None),
        demo: find("stretto-mcp-demo", None),
    };
    if binaries.stretto.is_none() {
        let _ = writeln!(
            out,
            "stretto-console: the stretto CLI is not beside the console or on PATH, so jobs \
             cannot run (pass --stretto PATH)"
        );
    }
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("listening on {listen}"))?;
    let bound = listener.local_addr()?;
    let config = Config {
        data_dir: data.clone(),
        token: token.clone(),
        read_only: cli.read_only,
        port: bound.port(),
        binaries,
        home: env.home.clone(),
        now_unix_ms: None,
        env: env.console.clone(),
    };
    let state = State::start(config);
    stretto_console::watch::spawn(state.clone());
    let app = stretto_console::router(state.clone());

    let url = format!("http://{}:{}/", url_host(bound.ip()), bound.port());
    let open_url = match (&token, printed) {
        (Some(t), true) => format!("{url}?token={t}"),
        _ => url.clone(),
    };
    let read_only = if cli.read_only { ", read-only" } else { "" };
    let _ = writeln!(
        out,
        "stretto-console: {} on {bound}{read_only}",
        data.display()
    );
    let _ = match (&token, printed) {
        (Some(_), true) => writeln!(out, "stretto-console: open {open_url}"),
        (Some(_), false) => writeln!(
            out,
            "stretto-console: open {url}?token=<your token> (the token you gave; it is not printed)"
        ),
        (None, _) => writeln!(out, "stretto-console: open {url} (no token: --no-auth)"),
    };
    if !env.ui_built {
        let _ = writeln!(
            out,
            "stretto-console: this build has no UI (console/dist); the API is up, and / says how \
             to build the UI"
        );
    }
    if cli.open {
        open_browser(&open_url, env.browser.clone(), out);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            stop.await;
            // The event streams end, or serving would wait for them.
            state.stop();
        })
        .await
        .context("serving")?;
    let _ = writeln!(out, "stretto-console: stopped");
    Ok(())
}

/// Open `url` with `browser` ($BROWSER), else as the platform does.
fn open_browser(url: &str, browser: Option<OsString>, out: &mut dyn Write) {
    let (program, args) = browser
        .filter(|b| !b.is_empty())
        .map_or((OsString::from(BROWSER.0), BROWSER.1), |b| (b, &[][..]));
    if let Err(e) = std::process::Command::new(&program)
        .args(args)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        let program = program.to_string_lossy();
        let _ = writeln!(out, "stretto-console: opening the browser ({program}): {e}");
    }
}

/// GET /api/health on 127.0.0.1 (or [::1]) at the port: `Ok` if it answers
/// `ok`.
async fn healthcheck(listen: Option<SocketAddr>, env: &Env) -> Result<()> {
    let listen = listen_address(listen, env.listen.as_deref())?;
    let host = if listen.is_ipv6() {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let url = format!("http://{host}:{}/api/health", listen.port());
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("an HTTP client with a timeout builds");
    let answer = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("{url}: {}", e.without_url()))?;
    if !answer.status().is_success() {
        bail!("{url} answered {}", answer.status());
    }
    let health: serde_json::Value = answer.json().await.unwrap_or_default();
    if health["ok"] != true {
        bail!("{url} did not answer ok");
    }
    Ok(())
}

#[cfg(test)]
mod cli_tests;
