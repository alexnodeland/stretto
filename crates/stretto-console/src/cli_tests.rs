//! The console's command line in this process: where it serves, with which
//! token, what it says, how it stops, and its healthcheck.

use super::*;
use clap::CommandFactory;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// An environment with nothing set, a UI, and no binaries anywhere.
fn env() -> Env {
    Env {
        listen: None,
        token: None,
        stretto_home: None,
        home: None,
        browser: None,
        console: Environment::default(),
        beside: None,
        ui_built: true,
    }
}

fn cli(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("stretto-console").chain(args.iter().copied())).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "stretto-console-main-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A stop that comes at once.
fn now() -> Stop {
    Box::pin(std::future::ready(()))
}

/// `run` with `args` in `env`, stopped at once: how it exited, and what it
/// said.
async fn ran(args: &[&str], env: &Env) -> (ExitCode, String) {
    let mut out = Vec::new();
    let code = run(cli(args), env, now(), &mut out).await;
    (code, String::from_utf8(out).unwrap())
}

/// What the console says, readable while it runs.
#[derive(Clone, Default)]
struct Said(Arc<Mutex<Vec<u8>>>);

impl Write for Said {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Said {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

/// The port in "stretto-console: <dir> on 127.0.0.1:<port>".
fn port_in(said: &str) -> Option<u16> {
    let (_, rest) = said.split_once(" on 127.0.0.1:")?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[test]
fn the_command_line_parses() {
    Cli::command().debug_assert();
    let c = cli(&["--data", "/d", "--read-only"]);
    assert_eq!(c.data.as_deref(), Some(Path::new("/d")));
    assert!(c.read_only && !c.no_auth);
    let both = ["stretto-console", "--token", "t", "--no-auth"];
    assert!(Cli::try_parse_from(both).is_err());
    let c = cli(&["healthcheck", "--listen", "0.0.0.0:8080"]);
    assert!(matches!(
        c.command,
        Some(Command::Healthcheck { listen: Some(a) }) if a.port() == 8080
    ));
}

#[test]
fn the_address_is_the_option_the_environment_or_the_default() {
    let given: SocketAddr = "127.0.0.1:1".parse().unwrap();
    assert_eq!(
        listen_address(Some(given), Some("127.0.0.1:2")).unwrap(),
        given
    );
    let from_env = listen_address(None, Some(" 127.0.0.1:2 ")).unwrap();
    assert_eq!(from_env.port(), 2);
    for unset in [None, Some(""), Some("  ")] {
        let default = listen_address(None, unset).unwrap();
        assert_eq!(default.to_string(), DEFAULT_LISTEN);
    }
    let e = listen_address(None, Some("nowhere")).unwrap_err();
    assert_eq!(
        e.to_string(),
        "STRETTO_CONSOLE_LISTEN=\"nowhere\" is not an address"
    );
}

#[test]
fn the_data_directory_is_the_option_stretto_home_or_the_home() {
    let mut env = env();
    let e = data_dir(None, &env).unwrap_err();
    assert!(e.to_string().starts_with("neither HOME nor USERPROFILE"));
    env.home = Some(PathBuf::from("/home/me"));
    let home = Path::new("/home/me/.stretto");
    assert_eq!(data_dir(None, &env).unwrap(), home);
    env.stretto_home = Some(OsString::new());
    assert_eq!(data_dir(None, &env).unwrap(), home);
    env.stretto_home = Some(OsString::from("/srv/stretto"));
    assert_eq!(data_dir(None, &env).unwrap(), Path::new("/srv/stretto"));
    let given = data_dir(Some(Path::new("/d")), &env).unwrap();
    assert_eq!(given, Path::new("/d"));
}

#[test]
fn a_token_is_given_or_new_and_only_a_new_one_is_printed() {
    let mut env = env();
    assert_eq!(token(&cli(&["--no-auth"]), &env), (None, false));
    let (new, printed) = token(&cli(&[]), &env);
    assert!(printed && !new.unwrap().is_empty());
    env.token = Some("from-env".into());
    let from_env = (Some("from-env".to_string()), false);
    assert_eq!(token(&cli(&[]), &env), from_env);
    let given = token(&cli(&["--token", "given"]), &env);
    assert_eq!(given, (Some("given".to_string()), false));
    // An empty token is none.
    assert_eq!(token(&cli(&["--token", ""]), &env), from_env);
    assert_eq!(token(&cli(&["--no-auth"]), &env), (None, false));
}

#[test]
fn a_url_names_a_loopback_address_for_an_unspecified_one() {
    for (ip, host) in [
        ("0.0.0.0", "127.0.0.1"),
        ("::", "[::1]"),
        ("10.0.0.2", "10.0.0.2"),
        ("fe80::1", "[fe80::1]"),
    ] {
        assert_eq!(url_host(ip.parse().unwrap()), host);
    }
}

#[tokio::test]
async fn what_cannot_be_served_is_said() {
    let dir = scratch("refused");
    let mut env = env();
    // What it said last: why it stopped.
    let refused = |said: &str, start: &str| {
        let line = format!("stretto-console: {start}");
        assert!(said.lines().last().unwrap().starts_with(&line), "{said}");
    };

    let args = ["--data", s(&dir), "--listen", "0.0.0.0:0", "--no-auth"];
    let (code, said) = ran(&args, &env).await;
    assert_eq!(code, ExitCode::FAILURE);
    refused(&said, "--no-auth is refused on 0.0.0.0:0");
    // Read-only creates nothing, not even the data directory.
    let missing = dir.join("missing");
    let (_, said) = ran(&["--data", s(&missing), "--read-only"], &env).await;
    refused(&said, &format!("{} does not exist", missing.display()));
    assert!(!missing.exists());
    let file = dir.join("file");
    std::fs::write(&file, "").unwrap();
    let (_, said) = ran(&["--data", s(&file)], &env).await;
    refused(&said, &format!("{} is not a directory", file.display()));
    let under = file.join("data");
    let (_, said) = ran(&["--data", s(&under)], &env).await;
    refused(&said, &format!("creating {}: ", under.display()));
    // A port another program holds.
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let (_, said) = ran(&["--data", s(&dir), "--listen", &addr], &env).await;
    refused(&said, &format!("listening on {addr}: "));
    env.listen = Some("nowhere".into());
    let (_, said) = ran(&["--data", s(&dir)], &env).await;
    refused(
        &said,
        "STRETTO_CONSOLE_LISTEN=\"nowhere\" is not an address",
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn it_says_where_it_serves_and_how_to_sign_in() {
    let dir = scratch("serve");
    let data = dir.join("data");
    let nowhere = dir.join("no-browser");
    let mut env = env();
    env.ui_built = false;
    env.browser = Some(nowhere.clone().into_os_string());

    // A new data directory, a new token, no UI, no stretto, no browser.
    let args = ["--data", s(&data), "--listen", "127.0.0.1:0", "--open"];
    let (code, said) = ran(&args, &env).await;
    assert_eq!(code, ExitCode::SUCCESS, "{said}");
    assert!(data.is_dir());
    let port = port_in(&said).unwrap();
    let lines: Vec<&str> = said.lines().collect();
    assert!(lines[0].contains("the stretto CLI is not beside the console or on PATH"));
    assert_eq!(
        lines[1],
        format!("stretto-console: {} on 127.0.0.1:{port}", data.display())
    );
    let open = format!("stretto-console: open http://127.0.0.1:{port}/?token=");
    assert!(lines[2].starts_with(&open), "{said}");
    assert!(lines[3].starts_with("stretto-console: this build has no UI"));
    let opening = format!(
        "stretto-console: opening the browser ({}): ",
        nowhere.display()
    );
    assert!(lines[4].starts_with(&opening), "{said}");
    assert_eq!(lines[5], "stretto-console: stopped");

    // A token given is never printed; a read-only console says so; a
    // stretto named is found.
    env.token = Some("the-token-given".into());
    env.ui_built = true;
    let stretto = dir.join("stretto");
    std::fs::write(&stretto, "").unwrap();
    let args = ["--data", s(&data), "--listen", "127.0.0.1:0", "--read-only"];
    let args = [&args[..], &["--stretto", s(&stretto)]].concat();
    let (_, said) = ran(&args, &env).await;
    let port = port_in(&said).unwrap();
    assert_eq!(
        said,
        format!(
            "stretto-console: {} on 127.0.0.1:{port}, read-only\n\
             stretto-console: open http://127.0.0.1:{port}/?token=<your token> (the token you \
             gave; it is not printed)\n\
             stretto-console: stopped\n",
            data.display()
        )
    );

    // Without a token.
    let args = ["--data", s(&data), "--listen", "127.0.0.1:0", "--no-auth"];
    let (_, said) = ran(&args, &env).await;
    let port = port_in(&said).unwrap();
    let open = format!("stretto-console: open http://127.0.0.1:{port}/ (no token: --no-auth)");
    assert!(said.lines().any(|l| l == open), "{said}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The console stops when it is asked to, even with a page open on its
/// event stream, which would otherwise hold it.
#[tokio::test]
async fn it_stops_with_an_event_stream_open() {
    let dir = scratch("stop");
    let mut env = env();
    let token = "t".repeat(64);
    env.token = Some(token.clone());
    let said = Said::default();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let stopped: Stop = Box::pin(async move {
        let _ = stopped.await;
    });
    let mut out = said.clone();
    let serving = run(
        cli(&["--data", s(&dir), "--listen", "127.0.0.1:0"]),
        &env,
        stopped,
        &mut out,
    );
    let using = async {
        let port = loop {
            if let Some(port) = port_in(&said.text()) {
                break port;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        healthcheck(Some(addr), &env).await.unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let events = client
            .get(format!("http://{addr}/api/events"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(events.status(), reqwest::StatusCode::OK);
        stop.send(()).unwrap();
        // Held open until the console has stopped.
        events
    };
    let both = async { tokio::join!(serving, using) };
    let (code, events) = tokio::time::timeout(Duration::from_secs(20), both)
        .await
        .expect("the console stops");
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(said.text().ends_with("stretto-console: stopped\n"));
    // The stream ended with it.
    let ended = tokio::time::timeout(Duration::from_secs(10), events.text()).await;
    assert!(ended.is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A server on a local port that answers every request with `answer`.
fn answering(answer: &'static str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            stream.write_all(answer.as_bytes()).unwrap();
        }
    });
    addr
}

fn json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn the_healthcheck_says_what_it_found() {
    let mut env = env();
    let ok = answering(json(r#"{"ok":true}"#).leak());
    assert!(healthcheck(Some(ok), &env).await.is_ok());
    let failed = |e: anyhow::Error| format!("{e:#}");
    let not_ok = answering(json(r#"{"ok":false}"#).leak());
    let e = healthcheck(Some(not_ok), &env).await.unwrap_err();
    let url = format!("http://127.0.0.1:{}/api/health", not_ok.port());
    assert_eq!(failed(e), format!("{url} did not answer ok"));
    let broken = answering(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let e = healthcheck(Some(broken), &env).await.unwrap_err();
    let url = format!("http://127.0.0.1:{}/api/health", broken.port());
    assert_eq!(
        failed(e),
        format!("{url} answered 500 Internal Server Error")
    );
    // Nothing answers: the port is free again.
    let closed = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let e = healthcheck(Some(closed), &env).await.unwrap_err();
    let url = format!("http://127.0.0.1:{}/api/health", closed.port());
    assert!(failed(e).starts_with(&format!("{url}: ")));
    // An IPv6 address is checked on IPv6's loopback.
    let e = healthcheck(Some("[::1]:9".parse().unwrap()), &env)
        .await
        .unwrap_err();
    assert!(failed(e).starts_with("http://[::1]:9/api/health: "));

    // As the command: its exit code, and what it says.
    let (code, said) = ran(&["healthcheck", "--listen", &ok.to_string()], &env).await;
    assert_eq!((code, said.as_str()), (ExitCode::SUCCESS, ""));
    let (code, said) = ran(&["--listen", &closed.to_string(), "healthcheck"], &env).await;
    assert_eq!(code, ExitCode::FAILURE);
    assert!(
        said.starts_with("stretto-console: http://127.0.0.1:"),
        "{said}"
    );
    env.listen = Some("nowhere".into());
    assert!(healthcheck(None, &env).await.is_err());
}

#[cfg(unix)]
#[test]
fn the_browser_is_given_the_url() {
    let dir = scratch("browser");
    // `touch` stands in for a browser: it makes the file the "URL" names.
    let opened = dir.join("opened");
    let mut out = Vec::new();
    open_browser(s(&opened), Some("touch".into()), &mut out);
    assert!(out.is_empty());
    for _ in 0..100 {
        if opened.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(opened.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
