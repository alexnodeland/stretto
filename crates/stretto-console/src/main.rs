//! `stretto-console`: serve the console over a data directory, or check that
//! one is serving (`healthcheck`, for a container).

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
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
    /// Open the console in the browser.
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

fn main() {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("stretto-console: starting the runtime: {e}");
            std::process::exit(1);
        }
    };
    let result = match cli.command {
        Some(Command::Healthcheck { listen }) => {
            let listen = listen.or(cli.listen);
            std::process::exit(runtime.block_on(healthcheck(listen)))
        }
        None => runtime.block_on(serve(cli)),
    };
    if let Err(e) = result {
        eprintln!("stretto-console: {e:#}");
        std::process::exit(1);
    }
}

/// The address to listen on: the option, else STRETTO_CONSOLE_LISTEN, else
/// the default.
fn listen_address(given: Option<SocketAddr>) -> Result<SocketAddr> {
    if let Some(addr) = given {
        return Ok(addr);
    }
    match std::env::var("STRETTO_CONSOLE_LISTEN") {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse()
            .with_context(|| format!("STRETTO_CONSOLE_LISTEN={v:?} is not an address")),
        _ => Ok(DEFAULT_LISTEN.parse().expect("the default parses")),
    }
}

async fn serve(cli: Cli) -> Result<()> {
    let listen = listen_address(cli.listen)?;
    if cli.no_auth && !listen.ip().is_loopback() {
        bail!(
            "--no-auth is refused on {listen}: without a token, listen on a loopback address \
             (127.0.0.1 or [::1])"
        );
    }
    let home = stretto_console::home_dir();
    let data = match (&cli.data, std::env::var_os("STRETTO_HOME")) {
        (Some(d), _) => d.clone(),
        (None, Some(h)) if !h.is_empty() => PathBuf::from(h),
        _ => home
            .as_ref()
            .map(|h| h.join(".stretto"))
            .context("neither HOME nor USERPROFILE is set: pass --data")?,
    };
    if !data.exists() {
        if cli.read_only {
            bail!(
                "{} does not exist (and --read-only creates nothing)",
                data.display()
            );
        }
        std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
    }
    let data =
        std::fs::canonicalize(&data).with_context(|| format!("resolving {}", data.display()))?;
    if !data.is_dir() {
        bail!("{} is not a directory", data.display());
    }
    // The token: given, or from the environment, never printed; else new,
    // and printed in the URL to open.
    let (token, printed) = if cli.no_auth {
        (None, false)
    } else if let Some(t) = cli.token.clone().filter(|t| !t.is_empty()) {
        (Some(t), false)
    } else if let Some(t) = std::env::var("STRETTO_CONSOLE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
    {
        (Some(t), false)
    } else {
        (Some(auth::new_token()), true)
    };
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    let env = Environment::of_process();
    let find = |name, explicit| meta::find(name, explicit, beside.as_deref(), &env.path);
    let binaries = Binaries {
        stretto: find("stretto", cli.stretto.as_deref()),
        proxy: find("stretto-proxy", None),
        procedure: find("stretto-procedure", None),
        demo: find("stretto-mcp-demo", None),
    };
    if binaries.stretto.is_none() {
        eprintln!(
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
        home,
        now_unix_ms: None,
        env,
    };
    let state = State::start(config);
    stretto_console::watch::spawn(state.clone());
    let app = stretto_console::router(state);

    let host = match bound.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "[::1]".to_string(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let url = format!("http://{host}:{}/", bound.port());
    let open_url = match (&token, printed) {
        (Some(t), true) => format!("{url}?token={t}"),
        _ => url.clone(),
    };
    eprintln!(
        "stretto-console: {} on {bound}{}",
        data.display(),
        if cli.read_only { ", read-only" } else { "" }
    );
    match (&token, printed) {
        (Some(_), true) => eprintln!("stretto-console: open {open_url}"),
        (Some(_), false) => eprintln!(
            "stretto-console: open {url}?token=<your token> (the token you gave; it is not printed)"
        ),
        (None, _) => eprintln!("stretto-console: open {url} (no token: --no-auth)"),
    }
    if !stretto_console::assets::built() {
        eprintln!(
            "stretto-console: this build has no UI (console/dist); the API is up, and / says how \
             to build the UI"
        );
    }
    if cli.open {
        open_browser(&open_url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .context("serving")?;
    eprintln!("stretto-console: stopped");
    Ok(())
}

/// Ctrl-C, or SIGTERM (a container stopping).
async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}

/// Open `url` in the browser, as the platform does.
fn open_browser(url: &str) {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    if let Err(e) = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        eprintln!("stretto-console: opening the browser ({program}): {e}");
    }
}

/// GET /api/health on 127.0.0.1 (or [::1]) at the port: 0 if it answers
/// `ok`, else 1.
async fn healthcheck(listen: Option<SocketAddr>) -> i32 {
    let listen = match listen_address(listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("stretto-console: {e:#}");
            return 1;
        }
    };
    let host = if listen.is_ipv6() {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let url = format!("http://{host}:{}/api/health", listen.port());
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("stretto-console: {e}");
            return 1;
        }
    };
    match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(v) if v["ok"] == true => 0,
            _ => {
                eprintln!("stretto-console: {url} did not answer ok");
                1
            }
        },
        Ok(r) => {
            eprintln!("stretto-console: {url} answered {}", r.status());
            1
        }
        Err(e) => {
            eprintln!("stretto-console: {url}: {}", e.without_url());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_parses() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from(["stretto-console", "--data", "/d", "--read-only"]).unwrap();
        assert_eq!(cli.data.as_deref(), Some(Path::new("/d")));
        assert!(cli.read_only && !cli.no_auth);
        assert!(Cli::try_parse_from(["stretto-console", "--token", "t", "--no-auth"]).is_err());
        let cli =
            Cli::try_parse_from(["stretto-console", "healthcheck", "--listen", "0.0.0.0:8080"])
                .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Healthcheck { listen: Some(a) }) if a.port() == 8080
        ));
        assert_eq!(
            listen_address(Some("127.0.0.1:1".parse().unwrap()))
                .unwrap()
                .port(),
            1
        );
    }
}
