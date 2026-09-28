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
    for host in ["attacker.example:7999", "localhost:80", "127.0.0.1:7998"] {
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
    assert_eq!(
        c.send(plain(Method::GET, "/assets/missing.js"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
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

// ---- servers --------------------------------------------------------------------

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
    let stretto = fake_stretto(&bin, "echo started\nexec sleep 30\n");
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
        if job["status"] == "running" && job["output"].as_str().unwrap().contains("started") {
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
        output.contains("started") && output.ends_with("stretto-console: cancelled\n"),
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
