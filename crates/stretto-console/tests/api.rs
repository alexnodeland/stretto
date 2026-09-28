//! The console's API, end to end through the router, over a copy of
//! `tests/fixtures/home` (a small `~/.stretto` the real tools made; see
//! `tests/fixtures/regenerate.sh`).
//!
//! The probe and the jobs run stretto's own binaries, `stretto-mcp-demo` and
//! `stretto`, from the target directory these tests were built in; `cargo
//! test` at the workspace's root builds them. Built alone, the tests that
//! need them say so and pass, except under CI, where they fail.

use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use stretto_console::api::jobs::{JobKind, JobStatus, Plan};
use stretto_console::api::meta::Binary;
use stretto_console::{Binaries, Config, Shared, State};
use tower::ServiceExt;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/home")
}

/// A copy of the fixtures, for one test.
fn home(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("stretto-console-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&fixtures(), &dir);
    std::fs::canonicalize(&dir).unwrap()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// An hour after the newest fixture session started, so the overview's
/// week and fortnight hold them.
fn fixture_now() -> u64 {
    let mut newest = 0;
    for dir in ["logs/shop", "logs/retail", "shadow/shop", "served/shop"] {
        for entry in std::fs::read_dir(fixtures().join(dir)).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".jsonl")
                || name.ends_with(".flow.jsonl")
                || name.ends_with(".confirm.jsonl")
            {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let header: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
            newest = newest.max(header["started_unix_ms"].as_u64().unwrap());
        }
    }
    newest + 3_600_000
}

/// A binary of the workspace's, from the target directory the tests run
/// from.
fn workspace_bin(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let debug = exe.parent()?.parent()?;
    let path = debug.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if path.is_file() {
        return Some(path);
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "{} is missing: build the workspace (cargo test at its root)",
        path.display()
    );
    eprintln!("skipping: {} is not built", path.display());
    None
}

fn binary(name: &str) -> Option<Binary> {
    workspace_bin(name).map(|p| Binary {
        version: stretto_report::doctor::version_of(&p),
        path: p.display().to_string(),
    })
}

struct Console {
    dir: PathBuf,
    state: Shared,
    app: Router,
}

fn console(name: &str, tweak: impl FnOnce(&mut Config)) -> Console {
    let dir = home(name);
    let mut config = Config::new(&dir, Some(TOKEN.to_string()));
    config.now_unix_ms = Some(fixture_now());
    config.home = Some(dir.join("home"));
    config.binaries = Binaries {
        stretto: binary("stretto"),
        proxy: binary("stretto-proxy"),
        procedure: None,
        demo: None,
    };
    // No key and no salt, whatever this process's environment holds.
    config.env.set.clear();
    tweak(&mut config);
    let state = State::start(config);
    let app = stretto_console::router(state.clone());
    Console { dir, state, app }
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.body)))
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or_default()
    }
}

impl Console {
    /// A request with the token and, for a change, the write header.
    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> Answer {
        let writes = method != Method::GET;
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
        if writes {
            req = req.header("x-stretto-console", "1");
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        };
        self.send(req.unwrap()).await
    }

    async fn get(&self, uri: &str) -> Answer {
        self.call(Method::GET, uri, None).await
    }

    async fn send(&self, req: Request<Body>) -> Answer {
        let response = self.app.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        Answer {
            status,
            headers,
            body,
        }
    }

    /// Wait for job `id` to end.
    async fn finished(&self, id: &str) -> Value {
        for _ in 0..600 {
            let job = self.get(&format!("/api/jobs/{id}")).await.json();
            if job["status"] == "succeeded"
                || job["status"] == "failed"
                || job["status"] == "cancelled"
            {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("job {id} did not end within a minute");
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn plain(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

// ---- auth and safety ----------------------------------------------------------

#[tokio::test]
async fn health_needs_no_token_and_everything_else_does() {
    let c = console("auth", |_| {});
    let health = c.send(plain(Method::GET, "/api/health")).await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(
        health.json(),
        json!({"ok": true, "version": stretto_console::VERSION})
    );

    let refused = c.send(plain(Method::GET, "/api/meta")).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert!(refused.json()["error"]
        .as_str()
        .unwrap()
        .contains("sign in"));
    assert_eq!(refused.header(header::WWW_AUTHENTICATE), "Bearer");

    let wrong = Request::builder()
        .uri("/api/meta")
        .header(header::AUTHORIZATION, format!("Bearer {}", &TOKEN[1..]))
        .body(Body::empty())
        .unwrap();
    assert_eq!(c.send(wrong).await.status, StatusCode::UNAUTHORIZED);

    let cookie = Request::builder()
        .uri("/api/meta")
        .header(
            header::COOKIE,
            format!("theme=dark; stretto_console={TOKEN}"),
        )
        .body(Body::empty())
        .unwrap();
    let meta = c.send(cookie).await;
    assert_eq!(meta.status, StatusCode::OK);
    let m = meta.json();
    assert_eq!(m["auth"], true);
    assert_eq!(m["read_only"], false);
    assert_eq!(m["data_dir"], c.dir.display().to_string());
    assert_eq!(m["version"], stretto_console::VERSION);
    assert!(m["key_set"].is_boolean());
    if let Some(stretto) = &c.state.config.binaries.stretto {
        assert_eq!(m["stretto"]["path"], stretto.path);
        assert_eq!(
            m["stretto"]["version"],
            format!("stretto {}", stretto_console::VERSION)
        );
    }
    // The API is never cached, and every response has the security headers.
    assert_eq!(meta.header(header::CACHE_CONTROL), "no-store");
    for answer in [&health, &refused, &meta] {
        assert_eq!(
            answer.header(header::CONTENT_SECURITY_POLICY),
            stretto_console::auth::CSP
        );
        assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        assert_eq!(answer.header(header::REFERRER_POLICY), "no-referrer");
        assert_eq!(answer.header(header::X_FRAME_OPTIONS), "DENY");
    }
}

#[tokio::test]
async fn the_token_in_a_link_becomes_a_cookie() {
    let c = console("cookie", |_| {});
    let signed_in = c
        .send(plain(
            Method::GET,
            &format!("/sessions/x?view=raw&token={TOKEN}"),
        ))
        .await;
    assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_in.header(header::LOCATION), "/sessions/x?view=raw");
    assert_eq!(
        signed_in.header(header::SET_COOKIE),
        format!("stretto_console={TOKEN}; HttpOnly; SameSite=Strict; Path=/")
    );
    let root = c
        .send(plain(Method::GET, &format!("/?token={TOKEN}")))
        .await;
    assert_eq!(root.header(header::LOCATION), "/");
    let wrong = c.send(plain(Method::GET, "/?token=nope")).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(wrong.headers.get(header::SET_COOKIE).is_none());
    // Signing out clears the cookie.
    let out = c.call(Method::POST, "/api/logout", None).await;
    assert_eq!(out.status, StatusCode::OK);
    assert!(out.header(header::SET_COOKIE).contains("Max-Age=0"));
}

#[tokio::test]
async fn a_change_needs_the_write_header() {
    let c = console("csrf", |_| {});
    let key = c.get("/api/sessions?limit=1").await.json()["items"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let forged = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/sessions/{key}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let refused = c.send(forged).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.json()["error"]
        .as_str()
        .unwrap()
        .contains("X-Stretto-Console: 1"));
    let wrong_value = Request::builder()
        .method(Method::POST)
        .uri("/api/jobs")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header("x-stretto-console", "yes")
        .body(Body::from("{\"kind\":\"doctor\"}"))
        .unwrap();
    assert_eq!(c.send(wrong_value).await.status, StatusCode::FORBIDDEN);
    // Nothing moved.
    assert_eq!(
        c.get(&format!("/api/sessions/{key}")).await.status,
        StatusCode::OK
    );
    // A change without the token is refused before the header counts.
    let anonymous = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/api/sessions/{key}"))
        .header("x-stretto-console", "1")
        .body(Body::empty())
        .unwrap();
    assert_eq!(c.send(anonymous).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn read_only_refuses_every_change_and_action() {
    let c = console("read-only", |config| config.read_only = true);
    assert_eq!(c.get("/api/meta").await.json()["read_only"], true);
    let key = c.get("/api/sessions?limit=1").await.json()["items"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    for (method, uri, body) in [
        (Method::DELETE, format!("/api/sessions/{key}"), None),
        (Method::DELETE, "/api/flows/shop".to_string(), None),
        (Method::POST, "/api/servers".to_string(), Some(json!({}))),
        (
            Method::PUT,
            "/api/servers/shop".to_string(),
            Some(json!({})),
        ),
        (Method::DELETE, "/api/servers/shop".to_string(), None),
        (Method::POST, "/api/servers/shop/probe".to_string(), None),
        (Method::POST, "/api/flows/shop/commit".to_string(), None),
        (Method::POST, "/api/flows/shop/rollback".to_string(), None),
        (
            Method::POST,
            "/api/jobs".to_string(),
            Some(json!({"kind": "doctor"})),
        ),
    ] {
        let answer = c.call(method.clone(), &uri, body).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert!(answer.json()["error"]
            .as_str()
            .unwrap()
            .contains("read-only"));
    }
    assert_eq!(
        c.call(Method::POST, "/api/logout", None).await.status,
        StatusCode::OK
    );
    assert!(c.dir.join("shop.flow.json").is_file());
    assert!(!c.dir.join("console").exists());
}

#[tokio::test]
async fn without_a_token_only_the_loopback_host_is_answered() {
    let c = console("no-auth", |config| {
        config.token = None;
        config.port = 7999;
    });
    let with_host = |host: &str| {
        Request::builder()
            .uri("/api/meta")
            .header(header::HOST, host)
            .body(Body::empty())
            .unwrap()
    };
    let ok = c.send(with_host("127.0.0.1:7999")).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.json()["auth"], false);
    assert_eq!(
        c.send(with_host("localhost:7999")).await.status,
        StatusCode::OK
    );
    assert_eq!(c.send(with_host("[::1]:7999")).await.status, StatusCode::OK);
    for host in [
        "attacker.example:7999",
        "localhost:80",
        "127.0.0.1:7998",
        "[::1",
    ] {
        let answer = c.send(with_host(host)).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{host}");
        assert!(answer.json()["error"].as_str().unwrap().contains("Host"));
    }
    // Writes still need the header.
    let write = Request::builder()
        .method(Method::POST)
        .uri("/api/jobs")
        .header(header::HOST, "127.0.0.1:7999")
        .body(Body::from("{\"kind\":\"doctor\"}"))
        .unwrap();
    assert_eq!(c.send(write).await.status, StatusCode::FORBIDDEN);
    // A link's token is dropped: there is none to sign in with.
    let link = Request::builder()
        .uri("/sessions?token=anything")
        .header(header::HOST, "127.0.0.1:7999")
        .body(Body::empty())
        .unwrap();
    let answer = c.send(link).await;
    assert_eq!(answer.status, StatusCode::SEE_OTHER);
    assert_eq!(answer.header(header::LOCATION), "/sessions");
    assert!(answer.headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn unknown_paths_and_methods_and_the_ui() {
    let c = console("paths", |_| {});
    let missing = c.get("/api/nothing").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.json()["error"]
        .as_str()
        .unwrap()
        .contains("/api/nothing"));
    let wrong = c.call(Method::PUT, "/api/overview", Some(json!({}))).await;
    assert_eq!(wrong.status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(wrong.json()["error"].is_string());
    let posted = c.call(Method::POST, "/sessions", Some(json!({}))).await;
    assert_eq!(posted.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(posted.json()["error"], "the UI is read with GET");
    // A key cannot name a path.
    for uri in [
        "/api/sessions/..%2F..%2Fservers.json",
        "/api/sessions/%2e%2e",
        "/api/flows/..%2Fshop",
        "/api/flows/shop%00",
    ] {
        assert_eq!(c.get(uri).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
    // The UI: its page, or without a build, the page that says how to build it.
    let page = c.send(plain(Method::GET, "/")).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.header(header::CONTENT_TYPE).starts_with("text/html"));
    assert_eq!(
        page.header(header::CONTENT_SECURITY_POLICY),
        stretto_console::auth::CSP
    );
    if !stretto_console::assets::built() {
        assert!(page.text().contains("The API is up"));
        let route = c.send(plain(Method::GET, "/flows/shop")).await;
        assert!(route.text().contains("The API is up"));
    }
    // A route whose key holds a dot is the app's too: a reload of a
    // session, a job, or a flow such as shop.promoted.
    for route in [
        "/sessions/20260928T020401.195Z-14715",
        "/jobs/20260928T034808.428Z-1ec2",
        "/flows/shop.promoted",
    ] {
        let page = c.send(plain(Method::GET, route)).await;
        assert_eq!(page.status, StatusCode::OK, "{route}");
        assert!(
            page.header(header::CONTENT_TYPE).starts_with("text/html"),
            "{route}"
        );
    }
    // A file of the build that is not there is not found.
    for file in ["/assets/missing.js", "/missing.ico", "/missing.css"] {
        assert_eq!(
            c.send(plain(Method::GET, file)).await.status,
            StatusCode::NOT_FOUND,
            "{file}"
        );
    }
    assert_eq!(
        c.send(plain(Method::GET, "/../servers.json")).await.status,
        StatusCode::NOT_FOUND
    );
}

// ---- overview, settings -------------------------------------------------------

#[tokio::test]
async fn the_overview_counts_the_fixtures() {
    let c = console("overview", |_| {});
    let o = c.get("/api/overview").await.json();
    let t = &o["totals"];
    // Six recorded and one retail session, two served, three in shadow.
    assert_eq!(t["sessions"], 12);
    assert_eq!(t["sessions_7d"], 12);
    assert_eq!(t["flows"], 2);
    assert_eq!(t["servers"], 2);
    assert_eq!(t["domains"], 3, "{o:#}");
    assert_eq!(t["errors"], 1);
    assert!(t["tool_calls"].as_u64().unwrap() > 40);
    assert!(t["flow_lookups"].as_u64().unwrap() >= 3);
    assert!(t["shadow_decisions"].as_u64().unwrap() >= 9);
    let names: Vec<&str> = o["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["orders", "retail", "shop"]);
    let shop = &o["domains"][2];
    assert_eq!(shop["sessions"], 11);
    assert_eq!(
        shop["modes"],
        json!({"recorded": 6, "shadow": 3, "served": 2})
    );
    let mut flows: Vec<&str> = shop["flows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    flows.sort();
    assert_eq!(flows, ["shop", "shop.promoted"]);
    assert_eq!(shop["servers"], json!(["shop"]));
    let activity = o["activity"].as_array().unwrap();
    assert_eq!(activity.len(), 14);
    assert_eq!(activity[13]["sessions"], 12);
    assert_eq!(
        activity[13]["day"],
        stretto_console::time::day(fixture_now())
    );
    assert!(activity[..13].iter().all(|d| d["sessions"] == 0));
    assert_eq!(o["recent_sessions"].as_array().unwrap().len(), 8);
    let health = o["health"].as_array().unwrap();
    assert!(health.iter().any(|h| h["level"] == "ok"
        && h["message"]
            .as_str()
            .unwrap()
            .contains("exists and is writable")));
    assert!(health
        .iter()
        .any(|h| h["message"].as_str().unwrap().contains("TYPESAFE_API_KEY")));
    // The HTTP server's command is not a question, but its flow-less record
    // mode is fine; nothing in the fixtures is broken.
    assert!(!health.iter().any(|h| h["level"] == "error"), "{health:#?}");
    assert_eq!(o["jobs"], json!([]));
}

#[tokio::test]
async fn settings_say_where_everything_is() {
    let c = console("settings", |_| {});
    let s = c.get("/api/settings").await.json();
    assert_eq!(s["data_dir"], c.dir.display().to_string());
    assert_eq!(
        (s["sessions"].as_u64(), s["flows"].as_u64()),
        (Some(12), Some(2))
    );
    let d = &s["disk"];
    let parts: u64 = ["logs_bytes", "flows_bytes", "cache_bytes", "other_bytes"]
        .iter()
        .map(|k| d[k].as_u64().unwrap())
        .sum();
    assert_eq!(parts, d["total_bytes"].as_u64().unwrap());
    assert!(d["logs_bytes"].as_u64().unwrap() > 50_000);
    let names: Vec<&str> = s["binaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "stretto",
            "stretto-proxy",
            "stretto-procedure",
            "stretto-mcp-demo"
        ]
    );
    assert!(s["retention_note"]
        .as_str()
        .unwrap()
        .contains("--retain-days"));
    assert_eq!(
        (s["read_only"].as_bool(), s["auth"].as_bool()),
        (Some(false), Some(true))
    );
    assert_eq!(s["version"], stretto_console::VERSION);
}

// ---- sessions -----------------------------------------------------------------

#[tokio::test]
async fn sessions_are_listed_newest_first_and_filtered() {
    let c = console("sessions", |_| {});
    let all = c.get("/api/sessions").await.json();
    assert_eq!(all["total"], 12);
    let items = all["items"].as_array().unwrap();
    let started: Vec<u64> = items
        .iter()
        .map(|s| s["started_unix_ms"].as_u64().unwrap())
        .collect();
    assert!(started.windows(2).all(|w| w[0] >= w[1]));
    let count = |q: &str| {
        let c = &c;
        let q = q.to_string();
        async move {
            c.get(&format!("/api/sessions?{q}")).await.json()["total"]
                .as_u64()
                .unwrap()
        }
    };
    assert_eq!(count("domain=shop").await, 11);
    assert_eq!(count("domain=retail").await, 1);
    assert_eq!(count("mode=served").await, 2);
    assert_eq!(count("mode=shadow").await, 3);
    assert_eq!(count("mode=recorded").await, 7);
    assert_eq!(count("domain=shop&mode=recorded").await, 6);
    assert_eq!(count("q=RETAIL").await, 1);
    // Tool names: every customer's first call, only cancellations in some.
    assert_eq!(count("q=find_user_id").await, 12);
    assert_eq!(count("q=quickstart").await, 8);
    let page = c.get("/api/sessions?limit=5&offset=10").await.json();
    assert_eq!(page["total"], 12);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    let bad = c.get("/api/sessions?mode=live").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.json()["error"]
        .as_str()
        .unwrap()
        .contains("recorded, shadow or served"));

    let served = c.get("/api/sessions?mode=served&limit=1").await.json()["items"][0].clone();
    assert!(served["key"].is_string());
    assert!(served["path"].as_str().unwrap().starts_with("served/shop/"));
    assert_eq!(served["agent"], "quickstart");
    assert_eq!(served["has_flow_log"], true);
    assert_eq!(
        served["upstream"],
        json!({"kind": "stdio", "command": ["stretto-mcp-demo", "--world", "retail"]})
    );
}

#[tokio::test]
async fn a_served_session_in_full() {
    let c = console("session", |_| {});
    let list = c.get("/api/sessions?mode=served").await.json();
    // The customer who cancelled: two calls of the agent's own.
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tool_calls"] == 2)
        .unwrap()
        .clone();
    let key = item["key"].as_str().unwrap();
    let d = c.get(&format!("/api/sessions/{key}")).await.json();
    assert_eq!(d["summary"], item);
    assert_eq!(d["header"]["stretto_mcp_log"], 2);
    assert_eq!(d["tools"].as_array().unwrap().len(), 4);
    let calls = d["calls"].as_array().unwrap();
    let flow: Vec<&Value> = calls.iter().filter(|c| c["by"] == "flow").collect();
    assert_eq!(flow.len(), 3);
    for call in &flow {
        let decision = &d["decisions"][call["decision"].as_u64().unwrap() as usize];
        assert_eq!(decision["action"], "lookup");
        assert_eq!(decision["tool"], call["tool"]);
        assert_eq!(decision["after"], call["after"]);
        assert_eq!(call["kind"], "read");
        assert!(call["turn"].is_null());
    }
    let first = &calls[0];
    assert_eq!(
        (first["by"].as_str(), first["ok"].as_bool()),
        (Some("agent"), Some(true))
    );
    assert_eq!(first["turn"], 0);
    assert!(first["result_json"].is_null() || first["result_json"].is_string());
    assert!(d["turns"].as_array().unwrap().len() >= 2);
    assert!(d["runs"].as_array().unwrap().len() >= 2);
    assert_eq!(d["context"][0]["role"], "user");
    assert!(d["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["from"] == "proxy"));
    assert_eq!(d["truncated"], false);
    assert_eq!(d["confirmations"], json!([]));

    assert_eq!(
        c.get("/api/sessions/nothing").await.status,
        StatusCode::NOT_FOUND
    );
    let raw = c.get(&format!("/api/sessions/{key}/raw")).await;
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(raw.header(header::CONTENT_TYPE), "application/x-ndjson");
    assert_eq!(
        raw.header(header::CONTENT_DISPOSITION),
        format!("attachment; filename=\"{key}.jsonl\"")
    );
    assert!(raw.text().starts_with("{\"stretto_mcp_log\":2"));
}

#[tokio::test]
async fn a_deleted_session_goes_to_the_trash_with_its_flow_log() {
    let c = console("delete-session", |_| {});
    let item = c.get("/api/sessions?mode=served&limit=1").await.json()["items"][0].clone();
    let (key, path) = (
        item["key"].as_str().unwrap(),
        item["path"].as_str().unwrap(),
    );
    let flow_log = path.replace(".jsonl", ".flow.jsonl");
    assert!(c.dir.join(&flow_log).is_file());
    let deleted = c
        .call(Method::DELETE, &format!("/api/sessions/{key}"), None)
        .await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(deleted.json(), json!({"ok": true}));
    assert!(!c.dir.join(path).exists() && !c.dir.join(&flow_log).exists());
    let trash = c.dir.join("console/trash").join(fixture_now().to_string());
    assert!(trash.join(path).is_file());
    assert!(trash.join(&flow_log).is_file());
    assert_eq!(
        c.get(&format!("/api/sessions/{key}")).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(c.get("/api/sessions").await.json()["total"], 11);
    let again = c
        .call(Method::DELETE, &format!("/api/sessions/{key}"), None)
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

// ---- flows ----------------------------------------------------------------------

#[tokio::test]
async fn flows_are_listed_and_reviewed() {
    let c = console("flows", |_| {});
    let list = c.get("/api/flows").await.json();
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let shop = items.iter().find(|f| f["key"] == "shop").unwrap();
    assert_eq!(shop["path"], "shop.flow.json");
    assert_eq!(shop["domain"], "shop");
    assert_eq!(shop["decider"], "reach");
    assert_eq!(shop["served_by"], json!(["shop"]));
    assert_eq!(shop["tools"], json!({"read": 3, "write": 1, "generic": 0}));
    assert!(shop["promoted"].is_null() && shop["error"].is_null());
    let promoted = items.iter().find(|f| f["key"] == "shop.promoted").unwrap();
    assert_eq!(
        promoted["promoted"],
        json!({"sites_promoted": 3, "sites_scored": 4})
    );

    let d = c.get("/api/flows/shop").await.json();
    assert_eq!(d["summary"], *shop);
    assert_eq!(d["threshold"], 0.3);
    assert_eq!(d["tools"].as_array().unwrap().len(), 4);
    let sites = d["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 3);
    let first = sites
        .iter()
        .find(|s| s["name"] == "find_user_id_by_email")
        .unwrap();
    assert_eq!(first["weighed_by"], "reach");
    assert_eq!(first["choice"]["tool"], "get_user_details");
    assert_eq!(first["choice"]["acts"], true);
    assert_eq!(first["lookups"][0]["acts"], true);
    assert!(first["verdict"].as_str().unwrap().starts_with("looks up"));
    assert!(d["review_markdown"]
        .as_str()
        .unwrap()
        .starts_with("# Flow: shop"));
    assert!(d["review_markdown"]
        .as_str()
        .unwrap()
        .contains(first["verdict"].as_str().unwrap()));
    let b = d["bindings"].as_array().unwrap();
    let details = b.iter().find(|b| b["tool"] == "get_user_details").unwrap();
    assert_eq!(details["args"][0]["name"], "user_id");
    assert_eq!(details["args"][0]["required"], true);
    assert_eq!(
        details["args"][0]["sources"][0]["tool"],
        "find_user_id_by_email"
    );
    assert_eq!(d["provenance"]["sources"], json!(["quickstart"]));
    assert!(d["promotion"].is_null());
    assert_eq!(d["warnings"], json!([]));

    // A higher threshold hands back where a lower one looked up.
    let strict = c.get("/api/flows/shop?threshold=0.95").await.json();
    assert!(strict["sites"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["choice"]["acts"] == false));
    let p = c.get("/api/flows/shop.promoted").await.json();
    assert_eq!(p["promotion"]["bar"]["min_tasks"], 3);
    assert_eq!(
        c.get("/api/flows/shop?threshold=2").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        c.get("/api/flows/nothing").await.status,
        StatusCode::NOT_FOUND
    );

    let raw = c.get("/api/flows/shop/raw").await;
    assert_eq!(raw.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(
        raw.header(header::CONTENT_DISPOSITION),
        "attachment; filename=\"shop.flow.json\""
    );
    assert_eq!(raw.json()["stretto_flow"], 1);

    // A flow whose file changes is read again.
    let path = c.dir.join("shop.flow.json");
    let mut flow: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    flow["provenance"]["sources"] = json!(["quickstart", "again"]);
    std::fs::write(&path, flow.to_string()).unwrap();
    let again = c.get("/api/flows/shop").await.json();
    assert_eq!(
        again["provenance"]["sources"],
        json!(["quickstart", "again"])
    );
}

#[tokio::test]
async fn two_flows_are_compared() {
    let c = console("diff", |_| {});
    let d = c
        .get("/api/flows/diff?from=shop&to=shop.promoted")
        .await
        .json();
    assert_eq!(
        (d["from"].as_str(), d["to"].as_str()),
        (Some("shop"), Some("shop.promoted"))
    );
    let sections: Vec<&str> = d["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    assert!(sections.contains(&"Promotion"), "{sections:?}");
    assert!(d["markdown"]
        .as_str()
        .unwrap()
        .starts_with("# Flow diff: shop"));
    // Promotion only narrows where the flow acts: nothing to review.
    assert_eq!(d["review"], json!([]));
    assert!(!d["changes"].as_array().unwrap().is_empty());
    let promotion = d["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["title"] == "Promotion")
        .unwrap()
        .clone();
    assert!(promotion["changes"][0]
        .as_str()
        .unwrap()
        .starts_with("none: it may act after every call → 3 of 4 sites scored"));
    // Every site with a lookup was promoted, so lifting the promotion lets
    // the flow act nowhere new either.
    let back = c
        .get("/api/flows/diff?from=shop.promoted&to=shop")
        .await
        .json();
    assert_eq!(back["review"], json!([]));
    assert_eq!(
        c.get("/api/flows/diff?from=shop").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        c.get("/api/flows/diff?from=shop&to=shop.promoted&tolerance=-1")
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        c.get("/api/flows/diff?from=shop&to=none").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_flow_that_does_not_load_is_listed_with_its_error() {
    let c = console("broken-flow", |_| {});
    std::fs::write(c.dir.join("broken.flow.json"), "{\"stretto_flow\": 7}").unwrap();
    let list = c.get("/api/flows").await.json();
    let broken = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "broken")
        .unwrap()
        .clone();
    assert!(broken["error"].as_str().unwrap().contains("flow format 7"));
    assert_eq!(broken["format_version"], 7);
    let detail = c.get("/api/flows/broken").await;
    assert_eq!(detail.status, StatusCode::UNPROCESSABLE_ENTITY);
    for (from, to) in [("broken", "shop"), ("shop", "broken")] {
        let diff = c.get(&format!("/api/flows/diff?from={from}&to={to}")).await;
        assert_eq!(diff.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(diff.json()["error"]
            .as_str()
            .unwrap()
            .starts_with("broken.flow.json does not load: "));
    }
    for (method, uri) in [
        (Method::GET, "/api/flows/nothing/raw"),
        (Method::DELETE, "/api/flows/nothing"),
    ] {
        let answer = c.call(method, uri, None).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{uri}");
    }
    let health = c.get("/api/overview").await.json()["health"].clone();
    assert!(health
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["level"] == "error"
            && h["message"].as_str().unwrap().contains("broken.flow.json")));
    let deleted = c.call(Method::DELETE, "/api/flows/broken", None).await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert!(!c.dir.join("broken.flow.json").exists());
}

/// A flow the file system lists but will not open (here a socket) cannot
/// be downloaded.
#[cfg(unix)]
#[tokio::test]
async fn a_flow_that_cannot_be_read_is_not_downloaded() {
    let c = console("socket-flow", |_| {});
    // Bound where its path is short enough for macOS, then moved in.
    let bound = std::env::temp_dir().join(format!("stretto-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&bound);
    let _socket = std::os::unix::net::UnixListener::bind(&bound).unwrap();
    std::fs::rename(&bound, c.dir.join("socket.flow.json")).unwrap();
    let raw = c.get("/api/flows/socket/raw").await;
    assert_eq!(raw.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(raw.json()["error"]
        .as_str()
        .unwrap()
        .starts_with("reading socket.flow.json: "));
}

/// A flow with an arbiter needs a key where it is served; a flow of a
/// domain no session recorded is checked against no server's tools.
#[tokio::test]
async fn a_flow_is_warned_about_what_it_needs() {
    let c = console("flow-warnings", |_| {});
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    std::fs::copy(
        docs.join("retail-5-sessions-shipped-arbiter.flow.json"),
        c.dir.join("arbiter.flow.json"),
    )
    .unwrap();
    let d = c.get("/api/flows/arbiter").await.json();
    assert_eq!(d["summary"]["decider"], "arbiter");
    assert!(
        d["warnings"].as_array().unwrap().iter().any(|w| w
            .as_str()
            .unwrap()
            .starts_with("the flow has an arbiter, which asks TypeSafe's Jev")),
        "{}",
        d["warnings"]
    );
    let mut flow: Value =
        serde_json::from_slice(&std::fs::read(c.dir.join("shop.flow.json")).unwrap()).unwrap();
    flow["manifest"]["domain"] = json!("elsewhere");
    flow["manifest"]["tools"]["notes"] = json!("generic");
    std::fs::write(c.dir.join("elsewhere.flow.json"), flow.to_string()).unwrap();
    let d = c.get("/api/flows/elsewhere").await.json();
    assert_eq!(d["warnings"], json!([]));
    assert_eq!(d["summary"]["tools"]["generic"], 1);

    // The newest shop session's server lists less than the flow looks up.
    write_lines(
        &c.dir,
        "logs/shop/newest.jsonl",
        &[
            json!({"stretto_mcp_log": 2, "session": "newest", "started_unix_ms": fixture_now(),
                   "server_command": ["x"], "domain": "shop", "agent_model": null}),
            json!({"t_ms": 1, "from": "client",
                   "message": {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}}),
            json!({"t_ms": 2, "from": "server",
                   "message": {"jsonrpc": "2.0", "id": 1,
                               "result": {"tools": [{"name": "get_user_details"}]}}}),
        ],
    );
    let health = c.get("/api/overview").await.json()["health"].clone();
    let said = "shop.flow.json: the flow looks up `get_order_details`, which the latest \
                recorded tools/list (session newest) does not list";
    assert!(
        health
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["level"] == "warn" && h["message"] == said),
        "{health:#}"
    );
}

// ---- servers --------------------------------------------------------------------

// ---- staged flows --------------------------------------------------------------

/// The flow `key` in `list`.
fn listed(list: &Value, key: &str) -> Value {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == key)
        .unwrap_or_else(|| panic!("no flow {key} in {list:#}"))
        .clone()
}

/// Run `stretto stage` on `sessions` as a job, and wait for it.
async fn stage(c: &Console, sessions: &str) -> Value {
    let queued = c
        .call(
            Method::POST,
            "/api/jobs",
            Some(json!({"kind": "stage", "flow": "shop", "sessions": sessions})),
        )
        .await;
    assert_eq!(queued.status, StatusCode::ACCEPTED, "{}", queued.text());
    let job = queued.json();
    let done = c.finished(job["id"].as_str().unwrap()).await;
    assert_eq!(done["status"], "succeeded", "{done:#}");
    done
}

#[tokio::test]
async fn a_flow_is_staged_committed_and_rolled_back() {
    if workspace_bin("stretto").is_none() {
        return;
    }
    let c = console("stage", |_| {});
    // Before stretto stage: the committed flow alone, and no version.
    let before = c.get("/api/flows/shop/stage").await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.text());
    let v = before.json();
    assert_eq!(
        (&v["committed"], &v["committed_path"], &v["staged_path"]),
        (
            &json!("shop"),
            &json!("shop.flow.json"),
            &json!("shop.staged.flow.json")
        )
    );
    assert!(v["staged"].is_null() && v["last"].is_null() && v["diff"].is_null());
    assert!(v["refused"]
        .as_str()
        .unwrap()
        .ends_with("stretto stage learns it"));
    assert_eq!(
        (&v["versions"], &v["unrecorded"]),
        (&json!([]), &json!(true))
    );
    assert!(listed(&c.get("/api/flows").await.json(), "shop")["stage"].is_null());

    // Staged from the six recorded sessions: nothing to compare yet.
    let job = stage(&c, "logs/shop").await;
    assert_eq!(job["title"], "Stage shop from logs/shop");
    assert_eq!(job["params"]["window"], 50);
    let artifacts: Vec<(&str, &str)> = job["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["kind"].as_str().unwrap(), a["path"].as_str().unwrap()))
        .collect();
    assert_eq!(artifacts[0], ("flow", "shop.staged.flow.json"));
    assert_eq!(job["artifacts"][0]["key"], "shop.staged");
    assert!(artifacts[1..].iter().all(|(kind, _)| *kind == "report"));
    let v = c.get("/api/flows/shop/stage").await.json();
    assert_eq!(
        (&v["last"]["sessions"], &v["last"]["compared"]),
        (&json!(6), &json!(0))
    );

    // Then with five more, each scored by both flows before it is learned.
    let all = c.dir.join("all/shop");
    std::fs::create_dir_all(&all).unwrap();
    for dir in ["logs/shop", "served/shop", "shadow/shop"] {
        for entry in std::fs::read_dir(c.dir.join(dir)).unwrap().flatten() {
            std::fs::copy(entry.path(), all.join(entry.file_name())).unwrap();
        }
    }
    stage(&c, "all/shop").await;
    let v = c.get("/api/flows/shop/stage").await.json();
    let last = &v["last"];
    assert_eq!(
        (
            &last["sessions"],
            &last["new"],
            &last["compared"],
            &last["committed"]
        ),
        (&json!(11), &json!(5), &json!(5), &json!(true))
    );
    for side in ["committed", "staged"] {
        let t = &last["total"][side];
        let n = |k: &str| t[k].as_u64().unwrap();
        assert!(n("lookups") > 0, "{last:#}");
        assert_eq!(n("detours"), n("lookups") - n("used") - n("served"));
        let share = t["used_share"]["share"].as_f64().unwrap();
        let (lower, upper) = (
            t["used_share"]["lower"].as_f64().unwrap(),
            t["used_share"]["upper"].as_f64().unwrap(),
        );
        assert!(lower <= share && share <= upper, "{t:#}");
    }
    assert!(!last["sites"].as_array().unwrap().is_empty());
    assert_eq!(
        (&v["evidence"], &v["refused"]),
        (&json!(true), &Value::Null)
    );
    assert!(v["report_markdown"]
        .as_str()
        .unwrap()
        .contains("stretto flow-commit --flow"));
    assert_eq!(
        (&v["diff"]["from"], &v["diff"]["to"]),
        (&json!("shop"), &json!("shop.staged"))
    );
    assert_eq!(v["staged"]["key"], "shop.staged");
    // Each is marked in the list, and the staged flow's key gives the same view.
    let flows = c.get("/api/flows").await.json();
    assert_eq!(
        listed(&flows, "shop")["stage"],
        json!({"staged": false, "other": "shop.staged", "pending": true})
    );
    assert_eq!(
        listed(&flows, "shop.staged")["stage"],
        json!({"staged": true, "other": "shop", "pending": true})
    );
    assert!(listed(&flows, "shop.promoted")["stage"].is_null());
    let detail = c.get("/api/flows/shop").await.json();
    assert_eq!(detail["summary"]["stage"]["other"], "shop.staged");
    assert_eq!(
        c.get("/api/flows/shop.staged/stage").await.json()["committed"],
        "shop"
    );

    // Committed, with a note: the flow as it was is kept first.
    let committed = c
        .call(
            Method::POST,
            "/api/flows/shop/commit",
            Some(json!({"note": " reads the order first "})),
        )
        .await;
    assert_eq!(committed.status, StatusCode::OK, "{}", committed.text());
    let r = committed.json();
    assert_eq!(
        (&r["version"], &r["kind"], &r["note"], &r["current"]),
        (
            &json!(2),
            &json!("commit"),
            &json!("reads the order first"),
            &json!(true)
        )
    );
    assert_eq!(r["evidence"]["compared"], 5);
    assert_eq!(
        std::fs::read(c.dir.join("shop.flow.json")).unwrap(),
        std::fs::read(c.dir.join("shop.staged.flow.json")).unwrap()
    );
    let v = c.get("/api/flows/shop/stage").await.json();
    let versions: Vec<(u64, &str, bool)> = v["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["version"].as_u64().unwrap(),
                r["kind"].as_str().unwrap(),
                r["current"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(versions, [(2, "commit", true), (1, "found", false)]);
    assert_eq!(v["unrecorded"], false);
    assert!(v["refused"].as_str().unwrap().contains("nothing to commit"));
    assert_eq!(v["staged"]["stage"]["pending"], false);
    let again = c.call(Method::POST, "/api/flows/shop/commit", None).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert!(again.json()["error"]
        .as_str()
        .unwrap()
        .contains("nothing to commit"));

    // Rolled back, by default to the version before, as a version of its own.
    let back = c.call(Method::POST, "/api/flows/shop/rollback", None).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());
    let back = back.json();
    assert_eq!(
        (&back["version"], &back["kind"], &back["restored"]),
        (&json!(3), &json!("rollback"), &json!(1))
    );
    assert_eq!(
        std::fs::read(c.dir.join("shop.flow.json")).unwrap(),
        std::fs::read(c.dir.join("shop.history/1.flow.json")).unwrap()
    );
    for (body, says) in [
        (json!({"to": 3}), "is the committed flow already"),
        (json!({"to": 9}), "there is no version 9"),
    ] {
        let answer = c
            .call(Method::POST, "/api/flows/shop/rollback", Some(body))
            .await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        assert!(answer.json()["error"].as_str().unwrap().contains(says));
    }
    let bad = Request::builder()
        .method(Method::POST)
        .uri("/api/flows/shop/rollback")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header("x-stretto-console", "1")
        .body(Body::from("{"))
        .unwrap();
    let bad = c.send(bad).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.json()["error"]
        .as_str()
        .unwrap()
        .starts_with("the request is not as expected"));
}

#[tokio::test]
async fn what_cannot_be_staged_or_committed_says_why() {
    let c = console("stage-refused", |_| {});
    let shop = std::fs::read(c.dir.join("shop.flow.json")).unwrap();
    for (method, uri) in [
        (Method::GET, "/api/flows/nothing/stage"),
        (Method::POST, "/api/flows/nothing/commit"),
        (Method::POST, "/api/flows/nothing/rollback"),
    ] {
        assert_eq!(
            c.call(method, uri, None).await.status,
            StatusCode::NOT_FOUND
        );
    }
    // A name that makes it a staged flow's staged flow.
    std::fs::write(c.dir.join("x.staged.staged.flow.json"), &shop).unwrap();
    let odd = c.get("/api/flows/x.staged.staged/stage").await;
    assert_eq!(odd.status, StatusCode::BAD_REQUEST);
    assert!(odd.json()["error"]
        .as_str()
        .unwrap()
        .contains("is a staged flow"));
    // A staged flow before its first commit: no committed flow to compare
    // with, and a commit makes it.
    std::fs::write(c.dir.join("solo.staged.flow.json"), &shop).unwrap();
    let v = c.get("/api/flows/solo.staged/stage").await.json();
    assert_eq!(
        (
            &v["committed"],
            &v["committed_path"],
            &v["diff"],
            &v["refused"]
        ),
        (
            &Value::Null,
            &json!("solo.flow.json"),
            &Value::Null,
            &Value::Null
        )
    );
    assert_eq!(v["unrecorded"], false);
    assert_eq!(
        listed(&c.get("/api/flows").await.json(), "solo.staged")["stage"],
        json!({"staged": true, "other": null, "pending": true})
    );
    let first = c
        .call(Method::POST, "/api/flows/solo.staged/commit", None)
        .await;
    assert_eq!(first.json()["version"], 1);
    assert_eq!(std::fs::read(c.dir.join("solo.flow.json")).unwrap(), shop);
    // Nothing to roll back to.
    let none = c.call(Method::POST, "/api/flows/shop/rollback", None).await;
    assert_eq!(none.status, StatusCode::CONFLICT);
    assert!(none.json()["error"]
        .as_str()
        .unwrap()
        .contains("no earlier version"));
    // A staged flow that does not load: said, and not committed.
    std::fs::write(c.dir.join("shop.staged.flow.json"), "not a flow").unwrap();
    let v = c.get("/api/flows/shop/stage").await.json();
    assert!(v["refused"].as_str().unwrap().starts_with("parsing "));
    assert!(v["diff"].is_null());
    let broken = c.call(Method::POST, "/api/flows/shop/commit", None).await;
    assert_eq!(broken.status, StatusCode::UNPROCESSABLE_ENTITY);
    // A state or a history that cannot be read.
    std::fs::write(c.dir.join("shop.stage.json"), "{").unwrap();
    for (method, uri) in [
        (Method::GET, "/api/flows/shop/stage"),
        (Method::POST, "/api/flows/shop/commit"),
    ] {
        let answer = c.call(method, uri, None).await;
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert!(answer.json()["error"]
            .as_str()
            .unwrap()
            .contains("shop.stage.json"));
    }
    std::fs::remove_file(c.dir.join("shop.stage.json")).unwrap();
    std::fs::create_dir_all(c.dir.join("shop.history")).unwrap();
    std::fs::write(c.dir.join("shop.history/1.json"), "{}").unwrap();
    let history = c.get("/api/flows/shop/stage").await;
    assert_eq!(history.status, StatusCode::UNPROCESSABLE_ENTITY);
    // A version whose flow is gone.
    let record = json!({"version": 1, "kind": "commit", "unix_ms": 1});
    std::fs::write(c.dir.join("shop.history/1.json"), record.to_string()).unwrap();
    let gone = c.get("/api/flows/shop/stage").await;
    assert_eq!(gone.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(gone.json()["error"]
        .as_str()
        .unwrap()
        .contains("1.flow.json"));

    // The stage job's own refusals.
    std::fs::write(c.dir.join("broken.flow.json"), "{\"stretto_flow\": 7}").unwrap();
    for (body, status, says) in [
        (
            json!({"kind": "stage", "flow": "nothing", "sessions": "logs/shop"}),
            StatusCode::BAD_REQUEST,
            "no flow",
        ),
        (
            json!({"kind": "stage", "flow": "x.staged.staged", "sessions": "logs/shop"}),
            StatusCode::BAD_REQUEST,
            "is a staged flow",
        ),
        (
            json!({"kind": "stage", "flow": "broken", "sessions": "logs/shop"}),
            StatusCode::UNPROCESSABLE_ENTITY,
            "broken.flow.json does not load",
        ),
        (
            json!({"kind": "stage", "flow": "shop", "sessions": "logs/nothing"}),
            StatusCode::BAD_REQUEST,
            "not a directory",
        ),
        (
            json!({"kind": "stage", "flow": "shop", "sessions": "logs/shop", "window": 0}),
            StatusCode::BAD_REQUEST,
            "window",
        ),
        (
            json!({"kind": "stage", "flow": "shop", "sessions": "logs/shop", "half_life": 0}),
            StatusCode::BAD_REQUEST,
            "half_life 0",
        ),
    ] {
        let answer = c.call(Method::POST, "/api/jobs", Some(body.clone())).await;
        assert_eq!(answer.status, status, "{body}: {}", answer.text());
        let error = answer.json()["error"].as_str().unwrap().to_string();
        assert!(error.contains(says), "{body}: {error}");
    }
    // What it runs, with every option, named by the staged flow.
    std::fs::write(c.dir.join("shop.staged.flow.json"), &shop).unwrap();
    let request: stretto_console::api::jobs::JobRequest = serde_json::from_value(json!({
        "kind": "stage", "flow": "shop.staged", "sessions": "logs/shop",
        "window": 20, "decider": "reach", "half_life": 100.0, "constants": true
    }))
    .unwrap();
    let plan = stretto_console::api::jobs::plan(&c.state, "j", request).unwrap();
    assert_eq!(plan.kind, JobKind::Stage);
    let args = plan.args.join(" ");
    for part in [
        format!("stage --flow {}", c.dir.join("shop.flow.json").display()),
        "--domain shop".to_string(),
        "--window 20".to_string(),
        "--decider reach --half-life 100 --constants".to_string(),
    ] {
        assert!(args.contains(&part), "{args}");
    }
}

fn shop_server(name: &str) -> Value {
    json!({
        "name": name,
        "upstream": {"kind": "stdio", "command": ["stretto-mcp-demo", "--world", "retail"], "env": ["SHOP_API_KEY"]},
        "mode": "shadow",
        "flow": "shop.flow.json"
    })
}

#[tokio::test]
async fn the_registry_is_listed_with_what_the_console_knows() {
    let c = console("servers", |_| {});
    let list = c.get("/api/servers").await.json();
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let shop = &items[0];
    assert_eq!(shop["name"], "shop");
    assert_eq!(shop["mode"], "serve");
    assert_eq!(shop["sessions"], 11);
    assert_eq!(shop["flow_summary"]["key"], "shop");
    let args: Vec<&str> = shop["proxy_args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let record = c.dir.join("served/shop").display().to_string();
    let flow = c.dir.join("shop.flow.json").display().to_string();
    assert_eq!(
        args,
        [
            "--record",
            &record,
            "--domain",
            "shop",
            "--flow",
            &flow,
            "--flow-decider",
            "reach",
            "--",
            "stretto-mcp-demo",
            "--world",
            "retail"
        ]
    );
    let orders = &items[1];
    assert_eq!(orders["upstream"]["kind"], "http");
    let args: Vec<&str> = orders["proxy_args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        &args[4..],
        [
            "--upstream",
            "https://mcp.example.com/orders/mcp",
            "--upstream-header",
            "Authorization=ORDERS_AUTH"
        ]
    );
    assert_eq!(orders["sessions"], 0);
    assert!(orders["last_session_unix_ms"].is_null());
    let discovered = list["discovered"].as_array().unwrap();
    let shop_seen = discovered.iter().find(|d| d["domain"] == "shop").unwrap();
    assert_eq!(shop_seen["registered"], true);
    assert_eq!(shop_seen["sessions"], 11);
    let retail = discovered.iter().find(|d| d["domain"] == "retail").unwrap();
    assert_eq!(retail["registered"], false);
    assert_eq!(retail["upstream"]["kind"], "stdio");
}

#[tokio::test]
async fn servers_are_created_changed_and_removed() {
    let c = console("server-crud", |_| {});
    let created = c
        .call(Method::POST, "/api/servers", Some(shop_server("shop-two")))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let v = created.json();
    assert_eq!(v["name"], "shop-two");
    assert_eq!(v["created_unix_ms"], fixture_now());
    assert_eq!(v["upstream"]["env"], json!(["SHOP_API_KEY"]));
    assert!(v["description"].is_null());
    let issues: Vec<&str> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap())
        .collect();
    assert!(
        issues
            .iter()
            .any(|i| i.contains("the flow's domain is shop, not shop-two")),
        "{issues:?}"
    );
    let args = v["proxy_args"].as_array().unwrap();
    assert!(args.contains(&json!("--flow-shadow")));
    assert!(args.contains(&json!(c.dir.join("shadow/shop-two").display().to_string())));

    let dup = c
        .call(Method::POST, "/api/servers", Some(shop_server("shop-two")))
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    for (body, says) in [
        (
            json!({"name": "Bad Name", "upstream": {"kind": "stdio", "command": ["x"]}, "mode": "record"}),
            "name",
        ),
        (
            json!({"name": "x", "upstream": {"kind": "stdio", "command": ["server", "--api-key", "sk-live-1"]}, "mode": "record"}),
            "credential",
        ),
        (
            json!({"name": "x", "upstream": {"kind": "http", "url": "https://u:p@example.com/mcp"}, "mode": "record"}),
            "password",
        ),
        (
            json!({"name": "x", "upstream": {"kind": "stdio", "command": ["x"]}, "mode": "record", "flow": "../../etc/passwd"}),
            "flow",
        ),
        (
            json!({"name": "x", "upstream": {"kind": "stdio", "command": ["x"]}, "mode": "record", "threshold": 3}),
            "threshold",
        ),
        (json!({"name": "x", "mode": "record"}), "not as expected"),
        (json!("not an object"), "not as expected"),
    ] {
        let refused = c
            .call(Method::POST, "/api/servers", Some(body.clone()))
            .await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
        let error = refused.json()["error"].as_str().unwrap().to_string();
        assert!(error.contains(says), "{error}");
        assert!(!error.contains("sk-live-1"), "{error}");
    }

    let mut changed = shop_server("shop-three");
    changed["mode"] = json!("record");
    changed["description"] = json!("the second shop");
    changed["decider"] = json!("habit");
    changed["threshold"] = json!(0.5);
    let renamed = c
        .call(Method::PUT, "/api/servers/shop-two", Some(changed.clone()))
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    let r = renamed.json();
    assert_eq!(
        (r["name"].as_str(), r["description"].as_str()),
        (Some("shop-three"), Some("the second shop"))
    );
    assert_eq!(r["created_unix_ms"], fixture_now());
    assert!(r["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i.as_str().unwrap().contains("mode is record")));
    let taken = c
        .call(
            Method::PUT,
            "/api/servers/shop-three",
            Some(shop_server("shop")),
        )
        .await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    let missing = c
        .call(
            Method::PUT,
            "/api/servers/nobody",
            Some(shop_server("nobody")),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // The file holds names, never values, and nothing is left beside it.
    let text = std::fs::read_to_string(c.dir.join("servers.json")).unwrap();
    let registry: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(registry["stretto_servers"], 1);
    assert_eq!(registry["servers"].as_array().unwrap().len(), 3);
    assert!(std::fs::read_dir(&c.dir)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));

    let deleted = c
        .call(Method::DELETE, "/api/servers/shop-three", None)
        .await;
    assert_eq!(deleted.json(), json!({"ok": true}));
    assert_eq!(
        c.call(Method::DELETE, "/api/servers/shop-three", None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        c.get("/api/servers").await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn a_servers_surprise_gate_reaches_the_proxy() {
    let c = console("server-surprise", |_| {});
    let args = |v: &Value| -> Vec<String> {
        v["proxy_args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect()
    };
    let flag = |v: &Value| {
        let a = args(v);
        let i = a.iter().position(|w| w == "--flow-surprise")?;
        a.get(i + 1).cloned()
    };
    let mut body = shop_server("shop-gated");
    let plain = c
        .call(Method::POST, "/api/servers", Some(body.clone()))
        .await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.text());
    // Served as the flow stores its gate, which the flow's summary shows.
    assert_eq!(flag(&plain.json()), None);
    assert!(plain.json()["flow_summary"]["surprise"].is_null());
    body["surprise"] = json!({"kind": "threshold", "nats": 2.5});
    let at = c
        .call(Method::PUT, "/api/servers/shop-gated", Some(body.clone()))
        .await;
    assert_eq!(at.status, StatusCode::OK, "{}", at.text());
    assert_eq!(at.json()["surprise"]["nats"], 2.5);
    assert_eq!(flag(&at.json()).as_deref(), Some("2.5"));
    body["surprise"] = json!({"kind": "off"});
    let off = c
        .call(Method::PUT, "/api/servers/shop-gated", Some(body.clone()))
        .await;
    assert_eq!(flag(&off.json()).as_deref(), Some("off"));
    // Before the server's command, where the proxy reads its options.
    let a = args(&off.json());
    let dash = a.iter().position(|w| w == "--").unwrap();
    assert!(a.iter().position(|w| w == "--flow-surprise").unwrap() < dash);
    body["surprise"] = json!({"kind": "threshold", "nats": 0});
    let refused = c
        .call(Method::PUT, "/api/servers/shop-gated", Some(body))
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.json()["error"]
        .as_str()
        .unwrap()
        .starts_with("surprise 0: a threshold in nats"));
}

#[tokio::test]
async fn a_servers_guards_judge_commit_and_retention_reach_the_proxy() {
    let c = console("server-policy", |_| {});
    let body = json!({
        "name": "retail",
        "upstream": {"kind": "stdio", "command": ["retail-mcp"]},
        "mode": "record",
        "guards": true,
        "judge": {"mode": "enforce", "context": "~/.stretto/context/retail.jsonl"},
        "commit": true,
        "retain_days": 30
    });
    let created = c
        .call(Method::POST, "/api/servers", Some(body.clone()))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let v = created.json();
    assert_eq!(v["judge"]["mode"], "enforce");
    let args: Vec<&str> = v["proxy_args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        &args[4..],
        [
            "--retain-days",
            "30",
            "--guards",
            "--confirm-judge",
            "enforce",
            "--context",
            "~/.stretto/context/retail.jsonl",
            "--commit",
            "--",
            "retail-mcp"
        ]
    );
    let config = c
        .get("/api/servers/retail/config?host=claude-code")
        .await
        .json();
    assert!(config["snippet"]
        .as_str()
        .unwrap()
        .contains("--guards --confirm-judge enforce"));
    // The next steps keep them, and say what the judge needs.
    let steps = config["next_steps"].as_str().unwrap();
    assert!(
        steps.contains("--domain retail --retain-days 30 --guards --confirm-judge enforce"),
        "{steps}"
    );
    assert!(
        steps.contains("reads the conversation from ~/.stretto/context/retail.jsonl"),
        "{steps}"
    );
    // Guards are the domain's, and the judge asks about the writes they check.
    let mut unguarded = body.clone();
    unguarded["name"] = json!("shop-guarded");
    let mut judge_alone = body.clone();
    judge_alone["name"] = json!("airline");
    judge_alone["guards"] = json!(false);
    for (changed, says) in [
        (unguarded, "no policy guards for shop-guarded"),
        (judge_alone, "turn the guards on"),
    ] {
        let refused = c.call(Method::POST, "/api/servers", Some(changed)).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        assert!(refused.json()["error"].as_str().unwrap().contains(says));
    }
}

#[tokio::test]
async fn host_configuration_is_what_stretto_init_prints() {
    let c = console("config", |_| {});
    let cursor = c.get("/api/servers/shop/config?host=cursor").await.json();
    assert_eq!(cursor["host"], "cursor");
    assert_eq!(cursor["language"], "json");
    let snippet: Value = serde_json::from_str(cursor["snippet"].as_str().unwrap()).unwrap();
    let server = &snippet["mcpServers"]["shop"];
    assert!(server["command"]
        .as_str()
        .unwrap()
        .ends_with("stretto-proxy"));
    assert_eq!(server["args"][0], "--record");
    assert!(cursor["placement"].as_str().unwrap().contains("mcp.json"));
    assert!(cursor["next_steps"]
        .as_str()
        .unwrap()
        .starts_with("Next steps"));
    let claude = c
        .get("/api/servers/shop/config?host=claude-code")
        .await
        .json();
    assert_eq!(claude["language"], "shell");
    assert!(claude["snippet"]
        .as_str()
        .unwrap()
        .starts_with("claude mcp add shop -- "));
    let vscode = c.get("/api/servers/orders/config?host=vscode").await.json();
    let snippet: Value = serde_json::from_str(vscode["snippet"].as_str().unwrap()).unwrap();
    assert_eq!(snippet["servers"]["orders"]["type"], "stdio");
    assert!(vscode["next_steps"]
        .as_str()
        .unwrap()
        .contains("--upstream https://mcp.example.com/orders/mcp"));
    // The default host is Claude Code.
    assert_eq!(
        c.get("/api/servers/shop/config").await.json()["host"],
        "claude-code"
    );
    assert_eq!(
        c.get("/api/servers/shop/config?host=emacs").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        c.get("/api/servers/nobody/config").await.status,
        StatusCode::NOT_FOUND
    );
}

/// A registry entry as `servers.json` holds it, with `extra` over a
/// serving stdio server that has no flow.
fn entry(name: &str, extra: Value) -> Value {
    let mut e = json!({
        "name": name,
        "description": null,
        "upstream": {"kind": "stdio", "command": ["stretto-mcp-demo"], "env": []},
        "mode": "serve",
        "flow": null,
        "record_dir": null,
        "decider": null,
        "threshold": null,
        "created_unix_ms": 1_790_553_600_000_u64,
        "updated_unix_ms": 1_790_553_600_000_u64,
    });
    for (k, v) in extra.as_object().unwrap() {
        e[k] = v.clone();
    }
    e
}

/// A registry edited by hand can hold what the API would refuse; the
/// console says what is wrong with each server.
#[tokio::test]
async fn what_is_wrong_with_a_server_is_said() {
    let c = console("server-issues", |_| {});
    std::fs::write(c.dir.join("broken.flow.json"), "{").unwrap();
    let mut flow: Value =
        serde_json::from_slice(&std::fs::read(c.dir.join("shop.flow.json")).unwrap()).unwrap();
    flow.as_object_mut().unwrap().remove("reach");
    std::fs::write(c.dir.join("unreached.flow.json"), flow.to_string()).unwrap();
    let servers = [
        entry("unflowed", json!({"mode": "shadow"})),
        entry("unserved", json!({})),
        entry("broken", json!({"flow": "broken.flow.json"})),
        entry(
            "shop",
            json!({"flow": "shop.flow.json", "decider": "arbiter"}),
        ),
        entry(
            "unreached",
            json!({"flow": "unreached.flow.json", "decider": "reach"}),
        ),
        entry(
            "outside",
            json!({"flow": "../../../../../x.flow.json", "record_dir": "../../../../../x"}),
        ),
        entry(
            "nothing",
            json!({"upstream": {"kind": "stdio", "command": [], "env": []}, "mode": "record"}),
        ),
    ];
    let registry = json!({"stretto_servers": 1, "servers": servers});
    std::fs::write(c.dir.join("servers.json"), registry.to_string()).unwrap();
    // A session that names no domain is no server's.
    write_lines(
        &c.dir,
        "logs/anon/anon.jsonl",
        &[
            json!({"stretto_mcp_log": 2, "session": "anon", "started_unix_ms": fixture_now(),
                   "server_command": ["x"], "domain": null, "agent_model": null}),
        ],
    );

    let list = c.get("/api/servers").await.json();
    let issues = |name: &str| -> Vec<String> {
        let item = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap();
        item["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_str().unwrap().to_string())
            .collect()
    };
    let has = |name: &str, issue: &str| {
        let all = issues(name);
        assert!(all.iter().any(|i| i.starts_with(issue)), "{name}: {all:?}");
    };
    has("unflowed", "mode is shadow but no flow is set");
    has("unserved", "mode is serve but no flow is set");
    has("broken", "the flow does not load: ");
    has("shop", "decider is arbiter, but the flow has no arbiter");
    has(
        "unreached",
        "decider is reach, but the flow holds no reach counts: learn it again",
    );
    has(
        "outside",
        "the flow's path cannot be used: ../../../../../x.flow.json",
    );
    assert_eq!(issues("nothing"), Vec::<String>::new());
    // A path that cannot be resolved is passed on as it was given.
    let outside = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "outside")
        .unwrap();
    let args = outside["proxy_args"].as_array().unwrap();
    assert!(args.contains(&json!("../../../../../x")), "{args:?}");
    assert!(
        args.contains(&json!("../../../../../x.flow.json")),
        "{args:?}"
    );
    let discovered = list["discovered"].as_array().unwrap();
    assert!(discovered.iter().all(|d| d["domain"].is_string()));
    let health = c.get("/api/overview").await.json()["health"].clone();
    assert!(health
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["level"] == "warn"
            && h["message"] == "server unflowed: mode is shadow but no flow is set"));
}

/// With `stretto-proxy` on the console's PATH, a host starts it by name,
/// but Claude Desktop, which starts servers with a minimal PATH, by where
/// it is.
#[tokio::test]
async fn the_proxy_is_named_as_the_host_can_find_it() {
    let bin = std::env::temp_dir().join(format!("stretto-console-it-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::create_dir_all(&bin).unwrap();
    let proxy = bin.join(format!("stretto-proxy{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&proxy, "").unwrap();
    let path = bin.clone().into_os_string();
    let c = console("proxy-on-path", |config| config.env.path = path);
    let command = |host: &str| {
        let c = &c;
        let host = host.to_string();
        async move {
            let answer = c
                .get(&format!("/api/servers/shop/config?host={host}"))
                .await
                .json();
            answer["snippet"].as_str().unwrap().to_string()
        }
    };
    let desktop: Value = serde_json::from_str(&command("claude-desktop").await).unwrap();
    assert_eq!(
        desktop["mcpServers"]["shop"]["command"],
        proxy.display().to_string()
    );
    assert!(command("claude-code")
        .await
        .starts_with("claude mcp add shop -- stretto-proxy --record"));
    std::fs::remove_dir_all(&bin).unwrap();
}

#[tokio::test]
async fn a_stdio_server_is_probed() {
    let Some(demo) = workspace_bin("stretto-mcp-demo") else {
        return;
    };
    let c = console("probe-stdio", |_| {});
    let mut body = shop_server("shop");
    body["upstream"]["command"] = json!([demo.display().to_string(), "--world", "retail"]);
    body["mode"] = json!("serve");
    let put = c
        .call(Method::PUT, "/api/servers/shop", Some(body.clone()))
        .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.text());
    let r = c
        .call(Method::POST, "/api/servers/shop/probe", None)
        .await
        .json();
    assert_eq!(r["ok"], true, "{r:#}");
    assert_eq!(r["server_name"], "stretto-mcp-demo");
    assert_eq!(r["server_version"], stretto_console::VERSION);
    assert_eq!(r["protocol_version"], "2025-06-18");
    assert!(r["instructions"].is_string());
    let tools: Vec<(&str, &str)> = r["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), t["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        tools,
        [
            ("cancel_pending_order", "write"),
            ("find_user_id_by_email", "read"),
            ("get_order_details", "read"),
            ("get_user_details", "read")
        ]
    );
    assert_eq!(r["flow_warnings"], json!([]));

    // The echo world lists none of the shop's tools: the flow's lookups are
    // gone.
    body["upstream"]["command"] = json!([demo.display().to_string(), "--world", "echo"]);
    c.call(Method::PUT, "/api/servers/shop", Some(body)).await;
    let r = c
        .call(Method::POST, "/api/servers/shop/probe", None)
        .await
        .json();
    assert_eq!(r["ok"], true);
    let warnings = r["flow_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].as_str().unwrap().contains("does not list"));

    let missing = c
        .call(Method::POST, "/api/servers/nobody/probe", None)
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_http_server_is_probed_with_its_header_from_the_environment() {
    let Some(demo) = workspace_bin("stretto-mcp-demo") else {
        return;
    };
    // The header's value comes from a variable already set: HOME, or
    // USERPROFILE on Windows, which has no HOME.
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var(var).unwrap_or_else(|_| "/".to_string());
    let mut server = std::process::Command::new(&demo)
        .args([
            "--world",
            "retail",
            "--http",
            "127.0.0.1:0",
            "--require-auth",
            &home,
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(server.stdout.take().unwrap()),
        &mut url,
    )
    .unwrap();
    let url = url.trim().to_string();
    let c = console("probe-http", |_| {});
    let body = json!({
        "name": "remote",
        "upstream": {"kind": "http", "url": url, "headers": [{"name": "Authorization", "env": var}]},
        "mode": "record"
    });
    assert_eq!(
        c.call(Method::POST, "/api/servers", Some(body))
            .await
            .status,
        StatusCode::OK
    );
    let r = c
        .call(Method::POST, "/api/servers/remote/probe", None)
        .await
        .json();
    assert_eq!(r["ok"], true, "{r:#}");
    assert_eq!(r["tools"].as_array().unwrap().len(), 4);
    assert_eq!(r["protocol_version"], "2025-06-18");
    // The value is sent, never answered.
    assert!(!r.to_string().contains(&home) || home == "/");

    // Without the header, the server refuses.
    let body = json!({
        "name": "anonymous",
        "upstream": {"kind": "http", "url": url},
        "mode": "record"
    });
    c.call(Method::POST, "/api/servers", Some(body)).await;
    let r = c
        .call(Method::POST, "/api/servers/anonymous/probe", None)
        .await
        .json();
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("HTTP 401"), "{r:#}");
    let _ = server.kill();
    let _ = server.wait();
}

// ---- jobs -----------------------------------------------------------------------

#[tokio::test]
async fn a_learn_job_runs_the_cli_and_writes_a_flow() {
    if workspace_bin("stretto").is_none() {
        return;
    }
    let c = console("learn", |_| {});
    let body = json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop", "out": "learned/shop.flow.json"});
    let queued = c.call(Method::POST, "/api/jobs", Some(body.clone())).await;
    assert_eq!(queued.status, StatusCode::ACCEPTED, "{}", queued.text());
    let job = queued.json();
    assert_eq!(job["kind"], "learn");
    assert_eq!(job["title"], "Learn shop from logs/shop");
    assert_eq!(job["params"]["habit_only"], true);
    assert_eq!(job["params"]["out"], "learned/shop.flow.json");
    let id = job["id"].as_str().unwrap();
    let done = c.finished(id).await;
    assert_eq!(done["status"], "succeeded", "{done:#}");
    assert_eq!(done["exit_code"], 0);
    assert!(done["started_unix_ms"].is_u64() && done["finished_unix_ms"].is_u64());
    let output = done["output"].as_str().unwrap();
    assert!(
        output.starts_with("$ stretto learn --sessions "),
        "{output}"
    );
    let artifact = &done["artifacts"][0];
    assert_eq!(done["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(
        (artifact["kind"].as_str(), artifact["path"].as_str()),
        (Some("flow"), Some("learned/shop.flow.json"))
    );
    // Two flows' files are named shop now, so each key has its path's hash.
    let key = artifact["key"].as_str().unwrap();
    assert!(key.starts_with("shop~"), "{key}");
    // The learned flow is the fixture's, learned again from the same sessions.
    let flow = c.get(&format!("/api/flows/{key}")).await.json();
    assert_eq!(flow["summary"]["path"], "learned/shop.flow.json");
    assert_eq!(flow["summary"]["sites"], 3);
    // Kept on disk, listed newest first.
    assert!(c.dir.join(format!("console/jobs/{id}.json")).is_file());
    assert!(c.dir.join(format!("console/jobs/{id}.log")).is_file());
    let list = c.get("/api/jobs").await.json();
    assert_eq!(list["items"][0]["id"], id);
    // Not over the flow it wrote, unless asked.
    let again = c.call(Method::POST, "/api/jobs", Some(body.clone())).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let mut overwrite = body;
    overwrite["overwrite"] = json!(true);
    let id = c
        .call(Method::POST, "/api/jobs", Some(overwrite))
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(c.finished(&id).await["status"], "succeeded");
}

#[tokio::test]
async fn jobs_run_one_at_a_time_and_report_their_artifacts() {
    if workspace_bin("stretto").is_none() {
        return;
    }
    let c = console("jobs", |_| {});
    let audit = c
        .call(
            Method::POST,
            "/api/jobs",
            Some(json!({"kind": "audit", "flow": "shop", "sessions": "served/shop"})),
        )
        .await
        .json();
    let promote = c
        .call(
            Method::POST,
            "/api/jobs",
            Some(json!({"kind": "promote", "flow": "shop", "sessions": "shadow/shop", "out": "shop.again.flow.json"})),
        )
        .await
        .json();
    let doctor = c
        .call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await
        .json();
    let (a, p, d) = (
        c.finished(audit["id"].as_str().unwrap()).await,
        c.finished(promote["id"].as_str().unwrap()).await,
        c.finished(doctor["id"].as_str().unwrap()).await,
    );
    assert_eq!(a["status"], "succeeded", "{a:#}");
    assert_eq!(p["status"], "succeeded", "{p:#}");
    // Each started after the one before ended.
    assert!(p["started_unix_ms"].as_u64() >= a["finished_unix_ms"].as_u64());
    assert!(d["started_unix_ms"].as_u64() >= p["finished_unix_ms"].as_u64());
    // doctor exits 1 when something needs fixing, as stretto-proxy not on PATH.
    assert!(d["exit_code"] == 0 || d["exit_code"] == 1, "{d:#}");
    // It checks the directory the console serves (--data), not ~/.stretto:
    // the fixtures' flow is there.
    let out = d["output"].as_str().unwrap();
    assert!(out.contains("shop.flow.json"), "{out}");

    let kinds: Vec<&str> = a["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["report", "report"]);
    let report = c
        .get(&format!(
            "/api/jobs/{}/artifacts/0",
            a["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(report.header(header::CONTENT_TYPE), "application/json");
    assert!(report.json().is_object());
    let md = c
        .get(&format!(
            "/api/jobs/{}/artifacts/1",
            a["id"].as_str().unwrap()
        ))
        .await;
    assert!(md.header(header::CONTENT_TYPE).starts_with("text/markdown"));
    assert_eq!(
        c.get(&format!(
            "/api/jobs/{}/artifacts/9",
            a["id"].as_str().unwrap()
        ))
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(p["artifacts"][0]["kind"], "flow");
    assert_eq!(p["artifacts"][0]["key"], "shop.again");
    assert_eq!(p["params"]["min_tasks"], 3);
    let promoted = c.get("/api/flows/shop.again").await.json();
    assert_eq!(promoted["promotion"]["sites_promoted"], 3);

    let list = c.get("/api/jobs").await.json();
    let ids: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            d["id"].as_str().unwrap(),
            p["id"].as_str().unwrap(),
            a["id"].as_str().unwrap()
        ]
    );
    let overview = c.get("/api/overview").await.json();
    assert_eq!(overview["jobs"].as_array().unwrap().len(), 3);
    assert_eq!(
        c.get("/api/jobs/nothing").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_job_that_would_leave_the_data_directory_is_refused() {
    let c = console("job-paths", |_| {});
    for (body, status, says) in [
        (
            json!({"kind": "learn", "domain": "shop", "sessions": "../"}),
            StatusCode::BAD_REQUEST,
            "..",
        ),
        (
            json!({"kind": "learn", "domain": "shop", "sessions": "/etc"}),
            StatusCode::BAD_REQUEST,
            "outside the data directory",
        ),
        (
            json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop", "out": "../../x.flow.json"}),
            StatusCode::BAD_REQUEST,
            "out",
        ),
        (
            json!({"kind": "learn", "domain": "shop", "sessions": "logs/nothing"}),
            StatusCode::BAD_REQUEST,
            "not a directory",
        ),
        (
            json!({"kind": "learn", "domain": "../x", "sessions": "logs/shop"}),
            StatusCode::BAD_REQUEST,
            "domain",
        ),
        (
            json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop"}),
            StatusCode::CONFLICT,
            "exists",
        ),
        (
            json!({"kind": "promote", "flow": "nothing", "sessions": "shadow/shop"}),
            StatusCode::BAD_REQUEST,
            "no flow",
        ),
        (
            json!({"kind": "promote", "flow": "shop", "sessions": "shadow/shop", "threshold": 1.5}),
            StatusCode::BAD_REQUEST,
            "threshold",
        ),
        (
            json!({"kind": "audit", "flow": "shop", "sessions": "~root/x"}),
            StatusCode::BAD_REQUEST,
            "~/",
        ),
        (
            json!({"kind": "dance"}),
            StatusCode::BAD_REQUEST,
            "not as expected",
        ),
    ] {
        let answer = c.call(Method::POST, "/api/jobs", Some(body.clone())).await;
        assert_eq!(answer.status, status, "{body}: {}", answer.text());
        let error = answer.json()["error"].as_str().unwrap().to_string();
        assert!(error.contains(says), "{body}: {error}");
    }
    // Redacting needs the salt in the console's environment.
    if !stretto_console::env_set("STRETTO_REDACT_SALT") {
        let answer = c
            .call(
                Method::POST,
                "/api/jobs",
                Some(json!({"kind": "redact", "sessions": "logs/shop", "out": "shared"})),
            )
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert!(answer.json()["error"]
            .as_str()
            .unwrap()
            .contains("STRETTO_REDACT_SALT"));
    }
    // Fitting an arbiter needs a key.
    if !c.state.key_set() {
        let answer = c
            .call(
                Method::POST,
                "/api/jobs",
                Some(json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop", "out": "x.flow.json", "habit_only": false})),
            )
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert!(answer.json()["error"]
            .as_str()
            .unwrap()
            .contains("TYPESAFE_API_KEY"));
    }
    assert_eq!(c.get("/api/jobs").await.json()["items"], json!([]));
}

#[tokio::test]
async fn a_job_without_the_cli_fails_and_says_why() {
    let c = console("no-cli", |config| config.binaries.stretto = None);
    let job = c
        .call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await
        .json();
    let done = c.finished(job["id"].as_str().unwrap()).await;
    assert_eq!(done["status"], "failed");
    assert!(done["exit_code"].is_null());
    assert!(done["output"].as_str().unwrap().contains("--stretto"));
}

// ---- live updates ---------------------------------------------------------------

#[tokio::test]
async fn events_say_what_changed_and_carry_jobs() {
    use futures_util::StreamExt;
    let c = console("events", |config| config.binaries.stretto = None);
    stretto_console::watch::spawn(c.state.clone());
    let response = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE].to_str().unwrap(),
        "text/event-stream"
    );
    let mut stream = response.into_body().into_data_stream();
    // Let the watcher take its first look.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let copy = c.dir.join("logs/shop/copy.jsonl");
    let source = std::fs::read_dir(c.dir.join("logs/shop"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    std::fs::copy(source, &copy).unwrap();
    c.call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await;
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !(seen.contains("event: changed") && seen.contains("event: job")) {
        let chunk = tokio::time::timeout_at(deadline, stream.next())
            .await
            .expect("events within 10 s")
            .unwrap()
            .unwrap();
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(
        seen.contains("data: {\"what\":\"sessions\",\"keys\":[\"copy\"]}"),
        "{seen}"
    );
    assert!(seen.contains("\"kind\":\"doctor\""), "{seen}");
}

/// A client too slow for the events misses the oldest and reads on from
/// there; when the console goes, the stream ends.
#[tokio::test]
async fn a_slow_client_misses_events_and_the_stream_ends_with_the_console() {
    use futures_util::StreamExt;
    use stretto_console::watch::{Changed, ChangedWhat, Event};
    let c = console("events-lag", |_| {});
    let response = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();
    // More than the channel holds, before the client reads any.
    for i in 0..300 {
        let keys = vec![format!("f{i}")];
        let change = Changed {
            what: ChangedWhat::Flows,
            keys,
        };
        c.state.events.send(Event::Changed(change)).unwrap();
    }
    drop(c);
    let mut seen = String::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("the stream ends")
    {
        seen.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
    }
    assert!(!seen.contains("[\"f0\"]"), "{seen}");
    assert!(seen.contains("[\"f299\"]"), "{seen}");
}

// ---- a first run ----------------------------------------------------------------

#[tokio::test]
async fn an_empty_data_directory_answers_with_empty_lists() {
    let dir = std::env::temp_dir().join(format!("stretto-console-it-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = Config::new(&dir, Some(TOKEN.to_string()));
    config.now_unix_ms = Some(fixture_now());
    let state = State::start(config);
    let c = Console {
        app: stretto_console::router(state.clone()),
        dir,
        state,
    };
    let o = c.get("/api/overview").await.json();
    assert_eq!(o["totals"]["sessions"], 0);
    assert_eq!(o["domains"], json!([]));
    assert_eq!(o["recent_sessions"], json!([]));
    assert_eq!(o["activity"].as_array().unwrap().len(), 14);
    assert!(o["health"].as_array().unwrap().iter().any(|h| h["message"]
        .as_str()
        .unwrap()
        .contains("stretto is not beside the console")));
    assert_eq!(
        c.get("/api/sessions").await.json(),
        json!({"total": 0, "items": []})
    );
    assert_eq!(c.get("/api/flows").await.json(), json!({"items": []}));
    assert_eq!(
        c.get("/api/servers").await.json(),
        json!({"items": [], "discovered": []})
    );
    assert_eq!(c.get("/api/jobs").await.json(), json!({"items": []}));
    let s = c.get("/api/settings").await.json();
    assert_eq!(
        (s["sessions"].as_u64(), s["disk"]["total_bytes"].as_u64()),
        (Some(0), Some(0))
    );
    // The first server makes the registry.
    let created = c
        .call(Method::POST, "/api/servers", Some(shop_server("shop")))
        .await;
    assert_eq!(created.status, StatusCode::OK);
    assert!(created.json()["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i.as_str().unwrap().starts_with("flow not found")));
    assert!(c.dir.join("servers.json").is_file());
    // A registry that does not parse is not written over.
    std::fs::write(c.dir.join("servers.json"), "{").unwrap();
    let broken = c.get("/api/servers").await;
    assert_eq!(broken.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(broken.json()["error"]
        .as_str()
        .unwrap()
        .contains("not a server registry"));
    let refused = c
        .call(Method::POST, "/api/servers", Some(shop_server("other")))
        .await;
    assert_eq!(refused.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        std::fs::read_to_string(c.dir.join("servers.json")).unwrap(),
        "{"
    );
    let health = c.get("/api/overview").await.json()["health"].clone();
    assert!(health
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["level"] == "error" && h["message"].as_str().unwrap().contains("servers.json")));
}

/// A plan for a job, as the API would make it, to queue directly.
fn doctor_plan() -> Plan {
    Plan {
        kind: JobKind::Doctor,
        title: "Check".into(),
        params: json!({"kind": "doctor"}),
        args: vec!["doctor".into()],
        artifacts: Vec::new(),
    }
}

/// A job whose `stretto` cannot be started, or whose log cannot be
/// opened, fails, and says why where it can.
#[tokio::test]
async fn a_job_that_cannot_start_says_why() {
    let missing = std::env::temp_dir().join("stretto-console-no-such-dir/stretto");
    let c = console("unstartable", |config| {
        config.binaries.stretto = Some(Binary {
            path: missing.display().to_string(),
            version: None,
        });
        config.binaries.proxy = Some(Binary {
            path: "/old/stretto-proxy".into(),
            version: Some("stretto-proxy 0.0.1".into()),
        });
    });
    let health = c.get("/api/overview").await.json()["health"].clone();
    let messages: Vec<&str> = health
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["message"].as_str().unwrap())
        .collect();
    let silent = format!("stretto ({}) does not answer --version", missing.display());
    assert!(messages.contains(&silent.as_str()), "{messages:?}");
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("stretto-proxy 0.0.1 (/old/stretto-proxy) is not stretto ")),
        "{messages:?}"
    );
    let answer = c
        .call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await;
    let id = answer.json()["id"].as_str().unwrap().to_string();
    let job = c.finished(&id).await;
    assert_eq!(job["status"], "failed");
    let said = format!("stretto-console: starting {}: ", missing.display());
    assert!(job["output"].as_str().unwrap().contains(&said), "{job:#}");

    // A log that cannot be opened: the job fails with nothing to show.
    std::fs::create_dir_all(c.dir.join("console/jobs/blocked.log")).unwrap();
    c.state
        .jobs
        .submit("blocked".into(), doctor_plan(), fixture_now());
    let job = c.finished("blocked").await;
    assert_eq!(
        (&job["status"], &job["exit_code"], &job["output"]),
        (&json!("failed"), &Value::Null, &json!(""))
    );
}

/// Where jobs cannot be kept (here `console/jobs` is a file), a job fails.
#[tokio::test]
async fn a_job_where_jobs_cannot_be_kept_fails() {
    let c = console("jobs-blocked", |config| {
        std::fs::create_dir_all(config.data_dir.join("console")).unwrap();
        std::fs::write(config.data_dir.join("console/jobs"), "").unwrap();
    });
    assert_eq!(c.get("/api/jobs").await.json()["items"], json!([]));
    let answer = c
        .call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await;
    let id = answer.json()["id"].as_str().unwrap().to_string();
    let job = c.finished(&id).await;
    assert_eq!(
        (&job["status"], &job["exit_code"]),
        (&json!("failed"), &Value::Null)
    );
}

/// A read-only console writes nothing for a job, even one cancelled before
/// it ran (which only this API, not the console's routes, can queue there).
#[tokio::test]
async fn a_read_only_console_writes_nothing_for_a_job() {
    let c = console("read-only-jobs", |config| config.read_only = true);
    let job = c
        .state
        .jobs
        .submit("queued".into(), doctor_plan(), fixture_now());
    assert_eq!(job.status, JobStatus::Queued);
    let cancelled = c.state.jobs.cancel("queued", fixture_now()).unwrap();
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert!(!c.dir.join("console").exists());
    let health = c.get("/api/overview").await.json()["health"].clone();
    let said = format!(
        "read-only: the console changes nothing in {}",
        c.dir.display()
    );
    assert_eq!(health[0]["message"], said, "{health:#}");
}

/// A `stretto` that runs `script` (after `#!/bin/sh`), so that a job runs
/// until it is cancelled.
#[cfg(unix)]
fn fake_stretto(dir: &std::path::Path, script: &str) -> Binary {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-stretto");
    std::fs::write(&path, format!("#!/bin/sh\n{script}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Binary {
        path: path.display().to_string(),
        version: None,
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_job_is_cancelled_queued_or_running() {
    let bin = std::env::temp_dir().join(format!("stretto-console-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&bin).unwrap();
    // It prints a line, and another later, so the job's progress is
    // announced. What it starts outlives it: that holds the pipes open,
    // prints once `sleep`, which keeps the FIFO open, has been killed, and
    // then ends. It says it is ready only once the FIFO is open at both
    // ends, so a cancel from then on closes it, whichever process it kills.
    let stretto = fake_stretto(
        &bin,
        "echo started\nsleep 0.3\nmkfifo gate\n(read x < gate; echo after) &\n\
         exec 3>gate\necho ready\nexec sleep 30\n",
    );
    let c = console("cancel", |config| config.binaries.stretto = Some(stretto));
    let doctor = || c.call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})));
    let first = doctor().await.json();
    let second = doctor().await.json();
    let (first, second) = (
        first["id"].as_str().unwrap().to_string(),
        second["id"].as_str().unwrap().to_string(),
    );
    // The first runs (and sleeps); the second waits behind it.
    for _ in 0..100 {
        let job = c.get(&format!("/api/jobs/{first}")).await.json();
        if job["status"] == "running" && job["output"].as_str().unwrap().contains("ready") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // A queued job is cancelled at once, and never runs.
    let answer = c
        .call(Method::POST, &format!("/api/jobs/{second}/cancel"), None)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    let queued = answer.json();
    assert_eq!(queued["status"], "cancelled", "{queued:#}");
    assert_eq!(queued["started_unix_ms"], Value::Null);
    assert!(queued["output"]
        .as_str()
        .unwrap()
        .contains("cancelled before it started"));

    // A running job's stretto is killed, and the job ends cancelled.
    let started = std::time::Instant::now();
    let answer = c
        .call(Method::POST, &format!("/api/jobs/{first}/cancel"), None)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.json()["status"], "running");
    let ended = c.finished(&first).await;
    assert!(started.elapsed() < Duration::from_secs(10), "{ended:#}");
    assert_eq!(ended["status"], "cancelled", "{ended:#}");
    // SIGKILL, as a shell reports it.
    assert_eq!(ended["exit_code"], 137, "{ended:#}");
    let output = ended["output"].as_str().unwrap();
    assert!(
        output.contains("started") && output.ends_with("after\nstretto-console: cancelled\n"),
        "{output}"
    );
    // The second never started.
    let never = c.get(&format!("/api/jobs/{second}")).await.json();
    assert_eq!(never["status"], "cancelled");
    assert_eq!(never["started_unix_ms"], Value::Null);

    // An ended job, or none, is not cancelled.
    let again = c
        .call(Method::POST, &format!("/api/jobs/{first}/cancel"), None)
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert!(again.json()["error"]
        .as_str()
        .unwrap()
        .contains("has ended (cancelled)"));
    assert_eq!(
        c.call(Method::POST, "/api/jobs/nope/cancel", None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // It is a change: the CSRF header is required, as for every POST.
    let bare = c
        .send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/jobs/{second}/cancel"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(bare.status, StatusCode::FORBIDDEN);
    std::fs::remove_dir_all(&bin).unwrap();
}

/// A process the CLI started, holding its output open after the CLI was
/// killed, delays the end of a cancelled job for a moment only.
#[cfg(unix)]
#[tokio::test]
async fn a_cancelled_job_ends_though_a_child_holds_its_output() {
    let bin = std::env::temp_dir().join(format!("stretto-console-orphan-{}", std::process::id()));
    std::fs::create_dir_all(&bin).unwrap();
    let stretto = fake_stretto(&bin, "echo started\nsleep 6 &\nwait\n");
    let c = console("orphan", |config| config.binaries.stretto = Some(stretto));
    let job = c
        .call(Method::POST, "/api/jobs", Some(json!({"kind": "doctor"})))
        .await
        .json();
    let id = job["id"].as_str().unwrap().to_string();
    for _ in 0..100 {
        let job = c.get(&format!("/api/jobs/{id}")).await.json();
        if job["output"].as_str().unwrap().contains("started") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let started = std::time::Instant::now();
    let answer = c
        .call(Method::POST, &format!("/api/jobs/{id}/cancel"), None)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    let ended = c.finished(&id).await;
    assert_eq!(ended["status"], "cancelled", "{ended:#}");
    // The sleep holds the pipes for 6 s; the job ends after the 2 s drain.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    std::fs::remove_dir_all(&bin).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_drift_job_that_sounds_its_alarm_is_no_failure() {
    let bin = std::env::temp_dir().join(format!("stretto-console-drift-{}", std::process::id()));
    std::fs::create_dir_all(&bin).unwrap();
    // It writes its reports where it is told, and exits as `stretto drift`
    // does: 1 while its alarm sounds, 0 when the agent did not change, 2 on
    // an error. The window says which.
    let stretto = fake_stretto(
        &bin,
        r#"while [ $# -gt 0 ]; do
  case "$1" in --json) json=$2; shift;; --out) md=$2; shift;; --window) window=$2; shift;; esac
  shift
done
if [ "$window" = 20 ]; then
  printf '{"sessions":[{},{}],"alarms":[],"sounding":false,"unknown_tools":{}}' > "$json"
  echo '# No change' > "$md"
  exit 0
fi
if [ "$window" = 30 ]; then echo 'no sessions'; exit 2; fi
printf '{"sessions":[{},{},{},{},{},{}],"alarms":[{"at":5,"change":2,"probability":0.83,"moved":[{"site":"get_order_details"},{"site":"find_user_id_by_email"},{"site":"a"},{"site":"b"}]}],"sounding":true,"unknown_tools":{"refund":3}}' > "$json"
echo '# The agent changed' > "$md"
echo 'the alarm sounds'
exit 1
"#,
    );
    let c = console("drift", |config| config.binaries.stretto = Some(stretto));
    let drift = |window: u32| json!({"kind": "drift", "flow": "shop", "sessions": "served/shop", "window": window});
    let (status, job) = submit(&c, drift(10)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{job}");
    assert_eq!(job["params"]["threshold"], 0.5);
    let done = c.finished(job["id"].as_str().unwrap()).await;
    assert_eq!(
        (&done["status"], &done["exit_code"], &done["alarm"]),
        (&json!("succeeded"), &json!(1), &json!(true))
    );
    // The flow's page and the overview say so.
    let d = c.get("/api/flows/shop").await.json()["drift"].clone();
    assert_eq!(d["job"], job["id"]);
    assert_eq!(
        (
            d["alarm"].as_bool(),
            d["sessions"].as_u64(),
            d["since"].as_u64()
        ),
        (Some(true), Some(6), Some(4))
    );
    assert_eq!(d["probability"], 0.83);
    assert_eq!(
        d["sites"],
        json!(["get_order_details", "find_user_id_by_email", "a"])
    );
    assert_eq!(d["unknown_tools"], json!(["refund"]));
    let overview = c.get("/api/overview").await.json();
    assert!(
        overview["health"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["message"]
                .as_str()
                .unwrap()
                .contains("stretto drift sounded its alarm")),
        "{overview}"
    );
    // A later run that finds no change is the one the flow shows.
    let (_, calm) = submit(&c, drift(20)).await;
    let done = c.finished(calm["id"].as_str().unwrap()).await;
    assert_eq!(
        (&done["status"], &done["alarm"]),
        (&json!("succeeded"), &json!(false))
    );
    let d = c.get("/api/flows/shop").await.json()["drift"].clone();
    assert_eq!(
        (d["alarm"].as_bool(), d["sessions"].as_u64(), &d["since"]),
        (Some(false), Some(2), &Value::Null)
    );
    // Any other exit is a failure, and leaves the last run as it was.
    let (_, broken) = submit(&c, drift(30)).await;
    let done = c.finished(broken["id"].as_str().unwrap()).await;
    assert_eq!(
        (&done["status"], &done["alarm"]),
        (&json!("failed"), &json!(false))
    );
    assert_eq!(
        c.get("/api/flows/shop").await.json()["drift"]["job"],
        calm["id"]
    );
    // What a drift job refuses.
    for (body, says) in [
        (
            json!({"kind": "drift", "flow": "shop", "sessions": "served/shop", "window": 2}),
            "window 2: at least 3",
        ),
        (
            json!({"kind": "drift", "flow": "shop", "sessions": "served/shop", "threshold": 0}),
            "threshold 0: a probability above 0",
        ),
    ] {
        let (status, answer) = submit(&c, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            answer["error"].as_str().unwrap().starts_with(says),
            "{answer}"
        );
    }
    std::fs::remove_dir_all(&bin).unwrap();
}

// ---- planning jobs ----------------------------------------------------------------

/// Post `body` as a job: the status, and the job or the error.
async fn submit(c: &Console, body: Value) -> (StatusCode, Value) {
    let answer = c.call(Method::POST, "/api/jobs", Some(body)).await;
    (answer.status, answer.json())
}

#[tokio::test]
async fn each_job_is_planned_with_its_options() {
    let c = console("plans", |config| {
        config.binaries.stretto = None;
        config.env.set.insert("TYPESAFE_API_KEY".to_string());
    });
    // With a key, a learn job may fit an arbiter, and bind constants.
    let (status, job) = submit(
        &c,
        json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop",
               "out": "flows/shop.txt", "habit_only": false, "constants": true}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{job}");
    assert_eq!(
        (&job["params"]["habit_only"], &job["params"]["constants"]),
        (&json!(false), &json!(true))
    );
    // A file to write is not written over unless asked, and goes where a
    // directory can be made.
    std::fs::write(c.dir.join("notes"), "").unwrap();
    let (status, answer) = submit(
        &c,
        json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop", "out": "notes/x.flow.json"}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{answer}");
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .starts_with("creating notes: "));

    // A promotion's defaults, and what it refuses.
    let (status, job) = submit(
        &c,
        json!({"kind": "promote", "flow": "shop", "sessions": "shadow/shop",
               "oracle_cache": "oracle-cache", "overwrite": true}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{job}");
    assert_eq!(job["params"]["out"], "shop.promoted.flow.json");
    let (status, answer) = submit(
        &c,
        json!({"kind": "promote", "flow": "shop", "sessions": "shadow/shop", "min_tasks": 0}),
    )
    .await;
    assert_eq!(
        (status, &answer["error"]),
        (StatusCode::BAD_REQUEST, &json!("min_tasks: at least 1"))
    );

    // An audit may name the decider.
    let (status, job) = submit(
        &c,
        json!({"kind": "audit", "flow": "shop", "sessions": "served/shop", "decider": "habit"}),
    )
    .await;
    assert_eq!(
        (status, &job["params"]["decider"]),
        (StatusCode::ACCEPTED, &json!("habit"))
    );
}

#[tokio::test]
async fn a_redaction_needs_a_salt_and_a_directory_of_its_own() {
    // A home outside the data directory.
    let home = std::env::temp_dir().join(format!("stretto-console-home-{}", std::process::id()));
    let c = console("redact", |config| {
        config.binaries.stretto = None;
        config.env.set.insert("STRETTO_REDACT_SALT".to_string());
        config.home = Some(home.clone());
    });
    let redact = |more: Value| {
        let mut body = json!({"kind": "redact", "sessions": "logs/shop", "out": "redacted"});
        body.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        body
    };
    for (more, error) in [
        (
            json!({"out": "logs/shop/copy"}),
            "out: the redacted copy goes in a directory of its own, apart from the sessions",
        ),
        (
            json!({"out": "logs"}),
            "out: the redacted copy goes in a directory of its own, apart from the sessions",
        ),
        (json!({"keep_shared": 0}), "keep_shared: at least 1"),
        (
            json!({"hash_fields": ["user id"]}),
            "hash_fields: field names, without spaces",
        ),
    ] {
        let (status, answer) = submit(&c, redact(more)).await;
        assert_eq!(
            (status, &answer["error"]),
            (StatusCode::BAD_REQUEST, &json!(error))
        );
    }
    // Fields may come as one list with commas; the copy may go outside the
    // data directory.
    let (status, job) = submit(
        &c,
        redact(json!({"out": "~/redacted", "hash_fields": ["user_id, email", ""]})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{job}");
    assert_eq!(job["params"]["out"], "~/redacted");
    assert_eq!(job["params"]["hash_fields"], json!(["user_id", "email"]));
    assert_eq!(job["params"]["keep_shared"], 3);

    // A directory the job wrote is not shown as a file.
    std::fs::create_dir_all(c.dir.join("redacted")).unwrap();
    let (_, job) = submit(&c, redact(json!({}))).await;
    let id = job["id"].as_str().unwrap();
    c.finished(id).await;
    let answer = c.get(&format!("/api/jobs/{id}/artifacts/0")).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.json()["error"], "redacted is a directory");
    let _ = std::fs::remove_dir_all(&home);
}

#[cfg(unix)]
#[tokio::test]
async fn an_artifact_is_shown_once_it_exists() {
    let bin =
        std::env::temp_dir().join(format!("stretto-console-artifacts-{}", std::process::id()));
    std::fs::create_dir_all(&bin).unwrap();
    let stretto = fake_stretto(&bin, "echo started\nexec sleep 30\n");
    let c = console("artifacts", |config| {
        config.binaries.stretto = Some(stretto)
    });
    // The first job runs (and sleeps); the second waits, and has written
    // nothing yet.
    let (_, first) = submit(&c, json!({"kind": "doctor"})).await;
    std::fs::create_dir_all(c.dir.join("flows")).unwrap();
    std::fs::write(c.dir.join("flows/shop.txt"), "a flow, as text").unwrap();
    let (_, text) = submit(
        &c,
        json!({"kind": "learn", "domain": "shop", "sessions": "logs/shop",
               "out": "flows/shop.txt", "overwrite": true}),
    )
    .await;
    let (_, waiting) = submit(
        &c,
        json!({"kind": "audit", "flow": "shop", "sessions": "served/shop"}),
    )
    .await;
    let missing = c
        .get(&format!(
            "/api/jobs/{}/artifacts/0",
            waiting["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    // A file that is neither JSON nor Markdown is plain text.
    let shown = c
        .get(&format!(
            "/api/jobs/{}/artifacts/0",
            text["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(
        shown.header(header::CONTENT_TYPE),
        "text/plain; charset=utf-8"
    );
    assert_eq!(shown.text(), "a flow, as text");
    assert_eq!(
        c.get("/api/jobs/nothing/artifacts/0").await.status,
        StatusCode::NOT_FOUND
    );
    for job in [&first, &text, &waiting] {
        let id = job["id"].as_str().unwrap();
        c.call(Method::POST, &format!("/api/jobs/{id}/cancel"), None)
            .await;
    }
    std::fs::remove_dir_all(&bin).unwrap();
}

// ---- sessions unlike the fixtures -------------------------------------------------

/// Write `lines`, one JSON value each, to `rel` under `dir`.
fn write_lines(dir: &Path, rel: &str, lines: &[Value]) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn a_session_unlike_the_fixtures_is_read_in_full() {
    let c = console("odd-session", |_| {});
    let e =
        |t: u64, from: &str, message: Value| json!({"t_ms": t, "from": from, "message": message});
    write_lines(
        &c.dir,
        "logs/odd/odd.jsonl",
        &[
            json!({"stretto_mcp_log": 2, "session": "odd", "started_unix_ms": fixture_now() - 60_000,
                   "server_command": ["odd-server"], "domain": "odd", "agent_model": null}),
            e(
                1,
                "client",
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            ),
            e(
                2,
                "server",
                json!({"jsonrpc": "2.0", "id": 2, "result": {"tools": [{"name": "plain"}]}}),
            ),
            e(
                3,
                "context",
                json!({"role": "system", "content": "Be brief."}),
            ),
            json!({"t_ms": 4, "from": "context", "raw": "not JSON"}),
            // The server asks the host something, and the host answers.
            e(
                5,
                "server",
                json!({"jsonrpc": "2.0", "id": "s1", "method": "roots/list"}),
            ),
            e(
                6,
                "client",
                json!({"jsonrpc": "2.0", "id": "s1", "result": {"roots": []}}),
            ),
            // A call that names no tool, and a request that is no call.
            e(
                7,
                "client",
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {}}),
            ),
            e(
                8,
                "client",
                json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}),
            ),
            e(
                9,
                "client",
                json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                                  "params": {"name": "plain", "arguments": {}}}),
            ),
            e(
                10,
                "server",
                json!({"jsonrpc": "2.0", "id": 5, "error": {"code": -32000}}),
            ),
            e(
                11,
                "client",
                json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                                   "params": {"name": "plain"}}),
            ),
            e(
                12,
                "server",
                json!({"jsonrpc": "2.0", "id": 6, "result": {"structuredContent": {"a": 1}}}),
            ),
            // Neither a request nor a response.
            e(13, "server", json!({"jsonrpc": "2.0", "result": {}})),
            // A batch: a notification, and the agent's commit.
            e(
                14,
                "client",
                json!([
                    {"jsonrpc": "2.0", "method": "notifications/progress", "params": {}},
                    {"jsonrpc": "2.0", "id": 7, "method": "tools/call",
                     "params": {"name": "stretto_commit", "arguments": {}}},
                ]),
            ),
            // The proxy's call for the commit is the agent's, after it; its
            // answer is longer than is kept.
            e(
                15,
                "proxy",
                json!({"jsonrpc": "2.0", "id": "stretto-1", "method": "tools/call",
                       "params": {"name": "plain", "arguments": {"n": 1}}}),
            ),
            e(
                16,
                "server",
                json!({"jsonrpc": "2.0", "id": "stretto-1",
                       "result": {"content": [{"type": "text", "text": "x".repeat(70_000)}]}}),
            ),
            e(
                17,
                "proxy",
                json!({"jsonrpc": "2.0", "id": 7, "error": {"code": -32000, "message": "refused"}}),
            ),
        ],
    );
    write_lines(
        &c.dir,
        "logs/odd/odd.flow.jsonl",
        &[
            json!({"after": 5, "action": "respond"}),
            json!({"after": 5, "action": "hand_back", "ms": 2.5}),
        ],
    );
    // Where a session log would be, something else.
    write_lines(
        &c.dir,
        "logs/odd/broken.jsonl",
        &[json!({"not": "a session"})],
    );

    let list = c.get("/api/sessions").await.json();
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["session_id"] == "odd")
        .unwrap()
        .clone();
    let d = c
        .get(&format!("/api/sessions/{}", item["key"].as_str().unwrap()))
        .await
        .json();
    assert_eq!(d["tools"][0]["kind"], "generic");
    let calls = d["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        (&calls[0]["ok"], &calls[0]["result_text"]),
        (&json!(false), &json!("{\"code\":-32000}"))
    );
    assert_eq!(calls[1]["result_text"], "{\"a\":1}");
    let commit = &calls[2];
    assert_eq!(
        (&commit["tool"], &commit["by"], &commit["ok"]),
        (&json!("stretto_commit"), &json!("agent"), &json!(false))
    );
    assert_eq!(commit["result_text"], "refused");
    let made = &calls[3];
    assert_eq!(
        (&made["id"], &made["by"], &made["after"]),
        (&json!("stretto-1"), &json!("agent"), &json!("7"))
    );
    assert_eq!(made["result_truncated"], true);
    assert!(made["result_json"].is_null());
    assert_eq!(made["result_text"].as_str().unwrap().len(), 64 * 1024);
    let events = d["events"].as_array().unwrap();
    let shown: Vec<(&str, &str, &str)> = events
        .iter()
        .map(|e| {
            (
                e["from"].as_str().unwrap(),
                e["kind"].as_str().unwrap(),
                e["summary"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(
        shown.contains(&("context", "context", "system: Be brief.")),
        "{shown:?}"
    );
    assert!(shown.contains(&("context", "raw", "not JSON")), "{shown:?}");
    assert!(
        shown.contains(&("server", "raw", "{\"jsonrpc\":\"2.0\",\"result\":{}}")),
        "{shown:?}"
    );
    let answered = events
        .iter()
        .find(|e| e["from"] == "client" && e["id"] == "s1")
        .unwrap();
    assert_eq!(answered["method"], "roots/list");
    // Only the host's own roles are the conversation.
    assert_eq!(d["context"], json!([]));
    assert_eq!(d["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(d["decisions"][0]["ms"], 2);

    let overview = c.get("/api/overview").await.json();
    let health: Vec<&str> = overview["health"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["message"].as_str().unwrap())
        .collect();
    assert!(
        health
            .iter()
            .any(|h| h.starts_with("logs/odd/broken.jsonl is not a session log")),
        "{health:?}"
    );
    assert!(
        stretto_console::data::sessions::read_json_lines(&c.dir.join("missing.jsonl")).is_empty()
    );
    // Listed by its name, but not a session log.
    let broken = c.get("/api/sessions/broken").await;
    assert_eq!(broken.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(broken.json()["error"]
        .as_str()
        .unwrap()
        .starts_with("logs/odd/broken.jsonl is not a session log stretto can read: "));
    // A file name a header cannot carry as it is.
    std::fs::copy(
        c.dir.join("logs/odd/odd.jsonl"),
        c.dir.join("logs/odd/odd copy 2.jsonl"),
    )
    .unwrap();
    let raw = c.get("/api/sessions/odd_copy_2/raw").await;
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(
        raw.header(header::CONTENT_DISPOSITION),
        "attachment; filename=\"odd_copy_2.jsonl\""
    );
    assert_eq!(
        c.get("/api/sessions/nothing/raw").await.status,
        StatusCode::NOT_FOUND
    );
}
