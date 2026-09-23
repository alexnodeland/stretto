//! TypeSafe's Jev over HTTP.
//!
//! `POST {base}/v1/systemone` with a bearer token. Configuration follows the
//! official SDKs' environment variables: `TYPESAFE_API_KEY` (required),
//! `TYPESAFE_BASE_URL` (default `https://api.typesafe.ai`) and
//! `TYPESAFE_DEFAULT_MODEL` (default `jev-latest`). Rate limiting (429) and
//! overload (529) are retried with exponential backoff, as the API reference
//! asks.

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
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
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
            let resp = self
                .http
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(request)
                .send()
                .context("sending request to TypeSafe")?;
            let status = resp.status();
            if status.is_success() {
                return resp.json().context("decoding TypeSafe response");
            }
            let retryable = status.as_u16() == 429 || status.as_u16() == 529;
            if retryable && attempt < self.max_retries {
                std::thread::sleep(Duration::from_secs(1 << attempt));
                attempt += 1;
                continue;
            }
            let body = resp.text().unwrap_or_default();
            bail!("TypeSafe returned {status}: {body}");
        }
    }
}
