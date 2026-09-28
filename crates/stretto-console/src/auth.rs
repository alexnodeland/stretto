//! Who may use the console, and how the browser is kept from being used
//! against it.
//!
//! - **The token.** `/api` (all but `/api/health`) needs the token, from
//!   the cookie `stretto_console` or `Authorization: Bearer`, compared in
//!   constant time; else 401. Opening the UI with `?token=` sets the cookie
//!   (`HttpOnly; SameSite=Strict; Path=/`) and redirects to the same URL
//!   without it.
//! - **Writes.** POST, PUT and DELETE need `X-Stretto-Console: 1`, which a
//!   cross-site form cannot send; else 403. With `--read-only` they are
//!   refused (403), all but signing out.
//! - **The Host.** With `--no-auth`, which is only allowed on a loopback
//!   address, the Host header must be `localhost`, `127.0.0.1` or `[::1]`
//!   with the console's port, so a page on another name that resolves to
//!   127.0.0.1 (DNS rebinding) gets nothing.
//! - **Headers.** Every response says `Content-Security-Policy`,
//!   `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and
//!   `X-Frame-Options: DENY`; API responses are not cached.

use crate::api::{ApiError, Ok as OkBody};
use crate::Shared;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use sha2::{Digest, Sha256};

/// The cookie that holds the token.
pub const COOKIE: &str = "stretto_console";
/// The header every write needs, with the value `1`.
pub const WRITE_HEADER: &str = "x-stretto-console";
/// The UI's Content-Security-Policy.
pub const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self'";

/// A new token: 32 random bytes from the operating system, as hex.
pub fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Whether `given` is `token`, in time that does not depend on where they
/// differ: both are hashed, and the digests compared whole.
pub fn same(given: &str, token: &str) -> bool {
    let (a, b) = (
        Sha256::digest(given.as_bytes()),
        Sha256::digest(token.as_bytes()),
    );
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// The token a request carries: the cookie's, else the bearer's.
fn presented(headers: &HeaderMap) -> Vec<String> {
    let mut out: Vec<String> = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().strip_prefix(&format!("{COOKIE}=")))
        .map(str::to_string)
        .collect();
    if let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        out.push(bearer.trim().to_string());
    }
    out
}

/// Whether the request carries `token`.
pub fn authorized(headers: &HeaderMap, token: &str) -> bool {
    presented(headers).iter().any(|t| same(t, token))
}

/// Whether the Host header names this machine's loopback, with `port`
/// (or none, on port 80).
pub fn host_allowed(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let host = host.trim().to_ascii_lowercase();
    let (name, given_port) = if let Some(rest) = host.strip_prefix('[') {
        match rest.split_once(']') {
            Some((inside, after)) => (format!("[{inside}]"), after.strip_prefix(':')),
            None => return false,
        }
    } else {
        match host.rsplit_once(':') {
            Some((name, p)) => (name.to_string(), Some(p)),
            None => (host.clone(), None),
        }
    };
    let loopback = matches!(name.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    let port_ok = match given_port {
        Some(p) => p.parse::<u16>().is_ok_and(|p| p == port),
        None => port == 80,
    };
    loopback && port_ok
}

/// The token, the Host check, the write header and read-only mode, in
/// that order (see the module's documentation).
pub async fn guard(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let config = &state.config;
    if config.token.is_none() && !host_allowed(request.headers(), config.port) {
        return ApiError::forbidden(
            "the Host header must be localhost, 127.0.0.1 or [::1] with the console's port: \
             without a token (--no-auth), the console answers only its own address",
        )
        .into_response();
    }
    let path = request.uri().path();
    let api = path == "/api" || path.starts_with("/api/");
    if !api {
        if let Some(given) = query_token(request.uri().query()) {
            return token_link(&state, &request, &given);
        }
        return next.run(request).await;
    }
    if path == "/api/health" {
        return next.run(request).await;
    }
    if let Some(token) = &config.token {
        if !authorized(request.headers(), token) {
            let mut response = ApiError::new(
                StatusCode::UNAUTHORIZED,
                "sign in: open the URL with the token that stretto-console printed when it \
                 started, or send Authorization: Bearer <token>",
            )
            .into_response();
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            return response;
        }
    }
    let writes = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    );
    if writes {
        let marked = request
            .headers()
            .get(WRITE_HEADER)
            .is_some_and(|v| v.as_bytes() == b"1");
        if !marked {
            return ApiError::forbidden(
                "a change needs the header X-Stretto-Console: 1, which the console's UI sends",
            )
            .into_response();
        }
        if config.read_only && path != "/api/logout" {
            return ApiError::forbidden(
                "the console is read-only (--read-only): it changes nothing",
            )
            .into_response();
        }
    }
    next.run(request).await
}

/// `?token=` on a page: with the right token, the cookie is set and the
/// browser sent to the same URL without it; with a wrong one, 401.
fn token_link(state: &Shared, request: &Request, given: &str) -> Response {
    let uri = request.uri();
    let rest: Vec<&str> = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("token=") && *p != "token")
        .collect();
    let location = if rest.is_empty() {
        uri.path().to_string()
    } else {
        format!("{}?{}", uri.path(), rest.join("&"))
    };
    let mut response = match &state.config.token {
        Some(token) if !same(given, token) => {
            let page = "<!doctype html><meta charset=\"utf-8\"><title>stretto console</title>\
                <p>This link's token is not this console's. Open the URL stretto-console \
                printed when it started.</p>";
            let mut r = (StatusCode::UNAUTHORIZED, page).into_response();
            r.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            return r;
        }
        Some(token) => {
            let mut r = Response::new(Body::empty());
            if let Ok(cookie) = HeaderValue::from_str(&format!(
                "{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/"
            )) {
                r.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            r
        }
        None => Response::new(Body::empty()),
    };
    *response.status_mut() = StatusCode::SEE_OTHER;
    if let Ok(location) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The `token` of a query string, percent-decoded.
fn query_token(query: Option<&str>) -> Option<String> {
    query?
        .split('&')
        .find_map(|p| p.strip_prefix("token="))
        .map(percent_decode)
}

fn percent_decode(text: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `POST /api/logout`: the cookie, cleared.
pub async fn logout() -> Response {
    let mut response = Json(OkBody { ok: true }).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("stretto_console=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
    );
    response
}

/// The security headers, on every response.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    if api && !h.contains_key(header::CACHE_CONTROL) {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(k.clone(), HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn a_token_is_32_random_bytes_and_compared_whole() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(same(&a, &a));
        assert!(!same(&a, &b));
        assert!(!same("", &a));
        assert!(!same(&a[..63], &a));
    }

    #[test]
    fn the_token_comes_from_the_cookie_or_the_bearer() {
        let t = "abc123";
        assert!(authorized(
            &headers(&[(header::COOKIE, "stretto_console=abc123")]),
            t
        ));
        assert!(authorized(
            &headers(&[(header::COOKIE, "theme=dark; stretto_console=abc123; x=1")]),
            t
        ));
        assert!(authorized(
            &headers(&[
                (header::COOKIE, "a=b"),
                (header::COOKIE, "stretto_console=abc123")
            ]),
            t
        ));
        assert!(authorized(
            &headers(&[(header::AUTHORIZATION, "Bearer abc123")]),
            t
        ));
        assert!(!authorized(
            &headers(&[(header::AUTHORIZATION, "Basic abc123")]),
            t
        ));
        assert!(!authorized(
            &headers(&[(header::COOKIE, "stretto_console=abc12")]),
            t
        ));
        assert!(!authorized(
            &headers(&[(header::COOKIE, "other_stretto_console=abc123")]),
            t
        ));
        assert!(!authorized(&HeaderMap::new(), t));
    }

    #[test]
    fn only_loopback_names_with_the_port_pass_the_host_check() {
        let ok = |h: &str| host_allowed(&headers(&[(header::HOST, h)]), 7878);
        assert!(ok("localhost:7878"));
        assert!(ok("LOCALHOST:7878"));
        assert!(ok("127.0.0.1:7878"));
        assert!(ok("[::1]:7878"));
        for bad in [
            "evil.example:7878",
            "localhost",
            "localhost:80",
            "127.0.0.1:7879",
            "127.0.0.2:7878",
            "localhost.evil.example:7878",
            "[::1]",
            "[::2]:7878",
            "0.0.0.0:7878",
            "",
        ] {
            assert!(!ok(bad), "{bad}");
        }
        assert!(!host_allowed(&HeaderMap::new(), 7878));
        assert!(host_allowed(&headers(&[(header::HOST, "localhost")]), 80));
    }

    #[test]
    fn the_query_token_is_found_and_decoded() {
        assert_eq!(
            query_token(Some("token=ab%20c&x=1")).as_deref(),
            Some("ab c")
        );
        assert_eq!(query_token(Some("x=1&token=abc")).as_deref(), Some("abc"));
        assert_eq!(query_token(Some("x=1")), None);
        assert_eq!(query_token(None), None);
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
