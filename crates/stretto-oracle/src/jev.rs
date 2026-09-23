//! TypeSafe's Jev over HTTP.
//!
//! `POST {base}/v1/systemone` with a bearer token. Configuration follows the
//! official SDKs' environment variables: `TYPESAFE_API_KEY` (required),
//! `TYPESAFE_BASE_URL` (default `https://api.typesafe.ai`) and
//! `TYPESAFE_DEFAULT_MODEL` (default `jev-latest`). Rate limiting (429),
//! overload (529), transient server errors and dropped connections are retried
//! with exponential backoff.
//!
//! Certificates in `SSL_CERT_FILE`, when it is set, are trusted in addition to
//! the built-in roots, so the client works behind TLS-inspecting proxies.

use crate::{Oracle, Request, Response};
use anyhow::{anyhow, bail, Context, Result};
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
        let mut builder = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30));
        if let Some(path) = std::env::var_os("SSL_CERT_FILE") {
            let pem = std::fs::read(&path)
                .with_context(|| format!("reading SSL_CERT_FILE {}", path.to_string_lossy()))?;
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
        let key = std::env::var("TYPESAFE_API_KEY")
            .map_err(|_| anyhow!("TYPESAFE_API_KEY is not set"))?;
        let base =
            std::env::var("TYPESAFE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
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

/// Statuses worth retrying: rate limiting, overload and transient server
/// errors.
fn retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}

#[cfg(test)]
mod tests {
    use super::retryable;

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
