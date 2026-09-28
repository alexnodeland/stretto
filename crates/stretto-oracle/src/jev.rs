//! TypeSafe's Jev over HTTP.
//!
//! `POST {base}/v1/systemone` with a bearer token. Configuration follows the
//! official SDKs' environment variables: `TYPESAFE_API_KEY` (required),
//! `TYPESAFE_BASE_URL` (default `https://api.typesafe.ai`) and
//! `TYPESAFE_DEFAULT_MODEL` (default `jev-latest`). Without
//! `TYPESAFE_API_KEY`, the key is read from the file `TYPESAFE_API_KEY_FILE`
//! names, so a process can be handed the key without its environment, or its
//! parent's, carrying it. Rate limiting (429),
//! overload (529), transient server errors and dropped connections are retried
//! with exponential backoff.
//!
//! Certificates in `SSL_CERT_FILE`, when it is set, are trusted in addition to
//! the built-in roots, so the client works behind TLS-inspecting proxies.

use crate::{Oracle, Request, Response};
use anyhow::{anyhow, bail, Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// Default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
/// Default model id.
pub const DEFAULT_MODEL: &str = "jev-latest";

/// A blocking Jev client.
pub struct JevClient {
    http: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    max_retries: u32,
}

impl JevClient {
    /// A client for `base_url` authenticated with `api_key`.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let roots = std::env::var_os("SSL_CERT_FILE").map(PathBuf::from);
        Self::with_roots(base_url, api_key, roots)
    }

    /// [`JevClient::new`], trusting the certificates of the PEM bundle at
    /// `roots` as well as the built-in ones.
    fn with_roots(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        roots: Option<PathBuf>,
    ) -> Result<Self> {
        let mut builder = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30));
        if let Some(path) = roots {
            let pem = std::fs::read(&path)
                .with_context(|| format!("reading SSL_CERT_FILE {}", path.display()))?;
            for cert in reqwest::Certificate::from_pem_bundle(&pem)? {
                builder = builder.add_root_certificate(cert);
            }
        }
        let http = builder.build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            max_retries: 4,
        })
    }

    /// A client configured from the environment.
    pub fn from_env() -> Result<Self> {
        Self::from_vars(&|name| std::env::var_os(name))
    }

    /// [`JevClient::from_env`], with `var` reading each variable.
    fn from_vars(var: &dyn Fn(&str) -> Option<OsString>) -> Result<Self> {
        let text = |name: &str| var(name).and_then(|v| v.into_string().ok());
        let file = var("TYPESAFE_API_KEY_FILE").map(PathBuf::from);
        let key = read_key(text("TYPESAFE_API_KEY"), file)?;
        let base = text("TYPESAFE_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self::new(base, key)
    }

    /// The model id to request: `TYPESAFE_DEFAULT_MODEL`, or `jev-latest`.
    pub fn default_model() -> String {
        std::env::var("TYPESAFE_DEFAULT_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string())
    }
}

impl Oracle for JevClient {
    fn ask(&self, request: &Request) -> Result<Response> {
        let url = format!("{}/v1/systemone", self.base_url);
        let mut attempt = 0;
        loop {
            let backoff = Duration::from_secs(1 << attempt);
            let resp = match self
                .http
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(request)
                .send()
            {
                Ok(resp) => resp,
                Err(e) if attempt < self.max_retries && (e.is_connect() || e.is_timeout()) => {
                    std::thread::sleep(backoff);
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e).context("sending request to TypeSafe"),
            };
            let status = resp.status();
            if status.is_success() {
                return resp.json().context("decoding TypeSafe response");
            }
            if retryable(status.as_u16()) && attempt < self.max_retries {
                std::thread::sleep(backoff);
                attempt += 1;
                continue;
            }
            let body = resp.text().unwrap_or_default();
            bail!("TypeSafe returned {status}: {body}");
        }
    }
}

/// The key: `var` (`TYPESAFE_API_KEY`) when it is set, else the contents of
/// `file` (`TYPESAFE_API_KEY_FILE`), trimmed.
fn read_key(var: Option<String>, file: Option<PathBuf>) -> Result<String> {
    if let Some(key) = var {
        return Ok(key);
    }
    let path = file.ok_or_else(|| anyhow!("TYPESAFE_API_KEY is not set"))?;
    let key = std::fs::read_to_string(&path)
        .with_context(|| format!("reading TYPESAFE_API_KEY_FILE {}", path.display()))?;
    let key = key.trim();
    if key.is_empty() {
        bail!("TYPESAFE_API_KEY_FILE {} is empty", path.display());
    }
    Ok(key.to_string())
}

/// Statuses worth retrying: rate limiting, overload and transient server
/// errors.
fn retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}

#[cfg(test)]
mod tests {
    use super::{read_key, retryable, JevClient};
    use crate::{Oracle, Request};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;

    /// A server on a local port that answers each connection with the next
    /// of `replies`, a status and a body, and then stops.
    fn serve(replies: Vec<(u16, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                // The request: its headers, then as much body as they say.
                let mut request = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                let mut line = String::new();
                while request.read_line(&mut line).unwrap() > 2 {
                    let header = line.to_lowercase();
                    if let Some(n) = header.strip_prefix("content-length: ") {
                        length = n.trim().parse().unwrap();
                    }
                    line.clear();
                }
                request.read_exact(&mut vec![0; length]).unwrap();
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        url
    }

    /// A client of `url` that goes straight there, retrying `retries` times.
    fn local(url: &str, retries: u32) -> JevClient {
        JevClient {
            http: reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap(),
            base_url: url.to_string(),
            api_key: "test".to_string(),
            max_retries: retries,
        }
    }

    fn request() -> Request {
        Request {
            model: "m".to_string(),
            state: serde_json::Value::Null,
            questions: Default::default(),
        }
    }

    /// A busy server is asked again, a refusal is an error with the
    /// server's words, and a server nobody runs is tried again, then given
    /// up on.
    #[test]
    fn a_request_is_retried_only_while_it_may_succeed() {
        let ok = r#"{"model": "jev-1", "answers": {}, "usage": {"input_tokens": 3, "output_tokens": 1}}"#;
        let url = serve(vec![(503, "busy".into()), (200, ok.into())]);
        let response = local(&url, 1).ask(&request()).unwrap();
        assert_eq!(
            (response.model.as_str(), response.usage.input_tokens),
            ("jev-1", 3)
        );
        let url = serve(vec![(400, "no such model".into())]);
        let refused = local(&url, 1).ask(&request()).unwrap_err().to_string();
        assert_eq!(refused, "TypeSafe returned 400 Bad Request: no such model");
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", listener.local_addr().unwrap())
        };
        let down = local(&closed, 1).ask(&request()).unwrap_err();
        assert!(
            format!("{down:#}").starts_with("sending request to TypeSafe"),
            "{down:#}"
        );
    }

    /// The key and the base URL come from the environment, the URL by
    /// default.
    #[test]
    fn the_environment_names_the_key_and_the_url() {
        let vars = |base: Option<&'static str>, key: Option<&'static str>| {
            move |name: &str| match name {
                "TYPESAFE_API_KEY" => key.map(Into::into),
                "TYPESAFE_BASE_URL" => base.map(Into::into),
                _ => None,
            }
        };
        let client = JevClient::from_vars(&vars(None, Some("k"))).unwrap();
        assert_eq!(
            (client.base_url.as_str(), client.api_key.as_str()),
            (super::DEFAULT_BASE_URL, "k")
        );
        let client = JevClient::from_vars(&vars(Some("http://local/"), Some("k"))).unwrap();
        assert_eq!(client.base_url, "http://local");
        let e = JevClient::from_vars(&vars(None, None))
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("TYPESAFE_API_KEY is not set"), "{e}");
    }

    /// A self-signed root made for this test; its key was never kept.
    const TEST_ROOT: &str = "-----BEGIN CERTIFICATE-----
MIIBjjCCATWgAwIBAgIUTiJTUovwc5Ao1EqHpApEVHkMHI4wCgYIKoZIzj0EAwIw
HDEaMBgGA1UEAwwRc3RyZXR0byB0ZXN0IHJvb3QwIBcNMjYwOTI4MTIwMTQ4WhgP
MjEyNjA5MDQxMjAxNDhaMBwxGjAYBgNVBAMMEXN0cmV0dG8gdGVzdCByb290MFkw
EwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEIcSiQe4G7/SmnsrClQV5T9/IPYQUBacC
Ol03F2wLO7VEkbdXQxdTbTX4eI8ndTTSdVzfaxhyfQSuVUKzDHJ0MqNTMFEwHQYD
VR0OBBYEFMqX+ZkMrJ8zSZ30BGNuQ7xBHEsqMB8GA1UdIwQYMBaAFMqX+ZkMrJ8z
SZ30BGNuQ7xBHEsqMA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDRwAwRAIg
Uhe1wiHWaphdzA+0SaR7wjhPm7FiZrzUh9TRZbQjiXICIDtzBgA0SGB2HM+2NOvS
96hnc8sEzg1JuUYcQbjKIkpg
-----END CERTIFICATE-----
";

    /// Certificates to trust besides the built-in ones, as `SSL_CERT_FILE`
    /// names them.
    #[test]
    fn extra_roots_are_read_from_a_pem_bundle() {
        let dir = std::env::temp_dir().join(format!("stretto-jev-roots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pem = dir.join("roots.pem");
        std::fs::write(&pem, TEST_ROOT).unwrap();
        let client = JevClient::with_roots("https://example.test/", "k", Some(pem)).unwrap();
        assert_eq!(client.base_url, "https://example.test");
        assert!(JevClient::with_roots("https://example.test", "k", None).is_ok());
        let missing = dir.join("missing.pem");
        let e = format!(
            "{:#}",
            JevClient::with_roots("x", "k", Some(missing))
                .err()
                .unwrap()
        );
        assert!(e.starts_with("reading SSL_CERT_FILE "), "{e}");
        assert!(JevClient::new("https://example.test", "k").is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_key_comes_from_the_variable_then_the_file() {
        let dir = std::env::temp_dir().join(format!("stretto-jev-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("key");
        std::fs::write(&file, "from-file\n").unwrap();
        // The variable wins; the file is read only without it, and trimmed.
        assert_eq!(
            read_key(Some("from-var".into()), Some(file.clone())).unwrap(),
            "from-var"
        );
        assert_eq!(read_key(None, Some(file.clone())).unwrap(), "from-file");
        // Neither, a missing file and an empty one are errors that name no key.
        let none = read_key(None, None).unwrap_err().to_string();
        assert!(none.contains("TYPESAFE_API_KEY is not set"), "{none}");
        assert!(read_key(None, Some(PathBuf::from("/nonexistent/stretto-key"))).is_err());
        std::fs::write(&file, " \n").unwrap();
        let empty = read_key(None, Some(file)).unwrap_err().to_string();
        assert!(empty.contains("is empty"), "{empty}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_transient_statuses_are_retried() {
        for s in [429, 500, 502, 503, 504, 529] {
            assert!(retryable(s), "{s}");
        }
        for s in [400, 401, 403, 404, 422] {
            assert!(!retryable(s), "{s}");
        }
    }
}
