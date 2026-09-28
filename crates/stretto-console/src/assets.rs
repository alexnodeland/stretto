//! The UI, embedded: `console/dist`, as `npm run build` in `console/`
//! writes it. A path that is a file is that file; any other path is the
//! single-page app's route, so `index.html`. Without a built UI, `/` is a
//! short page that says the API is up and how to build the UI.
//!
//! In a debug build the files are read from `console/dist` as they are on
//! disk; a release build holds them.

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../console/dist"]
#[allow_missing = true]
struct Ui;

/// Whether this build has a UI.
pub fn built() -> bool {
    Ui::get("index.html").is_some()
}

/// The page served when no UI was built into the console.
pub const NO_UI: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>stretto console</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 16px/1.5 system-ui, sans-serif; max-width: 40rem; margin: 4rem auto; padding: 0 1rem; }
  code, pre { font-family: ui-monospace, monospace; font-size: 0.9em; }
  pre { padding: 0.75rem 1rem; border-radius: 6px; background: rgba(127, 127, 127, 0.12); overflow-x: auto; }
</style>
</head>
<body>
<h1>stretto console</h1>
<p>The API is up: <a href="/api/health"><code>/api/health</code></a>. This build of <code>stretto-console</code> has no UI in it.</p>
<p>Build the UI, then the console, which embeds it:</p>
<pre>cd console &amp;&amp; npm ci &amp;&amp; npm run build &amp;&amp; cd ..
cargo build --release -p stretto-console</pre>
<p>The API is documented in <code>crates/stretto-console/README.md</code>.</p>
</body>
</html>
"#;

/// The response for UI path `path`.
pub async fn serve(path: &str) -> Response {
    let path = path.trim_start_matches('/');
    // No way up and out of the UI's folder.
    let safe = !path
        .split('/')
        .any(|part| part == ".." || part == "." || part.contains('\\'));
    if !safe {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let file = if path.is_empty() { "index.html" } else { path };
    if let Some(asset) = Ui::get(file) {
        return respond(file, asset);
    }
    // A file that is not there is not found; any other path is a route of
    // the app.
    let last = file.rsplit('/').next().unwrap_or_default();
    if last.contains('.') && file != "index.html" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match Ui::get("index.html") {
        Some(index) => respond("index.html", index),
        None => html(NO_UI),
    }
}

fn respond(path: &str, asset: rust_embed::EmbeddedFile) -> Response {
    let mut response = Body::from(asset.data.into_owned()).into_response();
    let mime = asset.metadata.mimetype().to_string();
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    // Vite names what it builds by its hash; the page itself is looked at
    // each time.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

fn html(page: &'static str) -> Response {
    let mut response = page.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}
