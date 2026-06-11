//! Default [`HttpClient`](super::HttpClient) backed by [`ureq`] (feature `http-ureq`).
//!
//! This is the batteries-included client for embedders who do not want to wire
//! their own HTTP stack. It lifts the core request path from the CLI's
//! `http_client.rs` — a blocking [`ureq::Agent`], the `Git-Protocol` /
//! `User-Agent` / `Content-Type` / `Accept` headers, and reading the response
//! body into a `Vec<u8>` — and adds optional HTTP basic auth driven by a
//! [`CredentialProvider`]: on a `401` it parses `WWW-Authenticate`, fills a
//! credential, and retries once, then `approve`/`reject`s the credential per the
//! retry status (Git's `credential_approve` / `credential_reject`).
//!
//! Deliberately omitted vs. the CLI client (and noted for parity): HTTP/SOCKS
//! proxies, cookie jars, custom CA bundles, gzip request bodies, and the
//! `GIT_ASKPASS`/`GIT_TRACE_CURL` plumbing. Embedders that need those should
//! implement [`HttpClient`](super::HttpClient) over their own stack.

use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;

use crate::credentials::{Credential, CredentialProvider};
use crate::error::{Error, Result};

use super::HttpClient;

/// A blocking [`ureq`]-backed [`HttpClient`].
///
/// Build with [`UreqHttpClient::new`] for an unauthenticated client, or
/// [`UreqHttpClient::with_credentials`] to wire a [`CredentialProvider`] for
/// HTTP basic auth on `401`. A default `Git-Protocol` header can be set via
/// [`UreqHttpClient::with_git_protocol`].
pub struct UreqHttpClient {
    agent: ureq::Agent,
    user_agent: String,
    git_protocol: Option<String>,
    credentials: Option<Box<dyn CredentialProvider + Send + Sync>>,
    /// Cached `Authorization: Basic …` header from a successful auth, reused on
    /// subsequent requests (mirrors Git's per-connection auth cache).
    cached_auth: Mutex<Option<String>>,
}

impl UreqHttpClient {
    /// A client with default timeouts and no credential provider.
    #[must_use]
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout(Duration::from_secs(600))
            .build();
        Self {
            agent,
            user_agent: default_user_agent(),
            git_protocol: None,
            credentials: None,
            cached_auth: Mutex::new(None),
        }
    }

    /// A client that uses `provider` to fill HTTP basic-auth credentials when
    /// the server returns `401 Unauthorized`.
    #[must_use]
    pub fn with_credentials(provider: Box<dyn CredentialProvider + Send + Sync>) -> Self {
        let mut c = Self::new();
        c.credentials = Some(provider);
        c
    }

    /// Set a default `Git-Protocol` request-header value (e.g. `version=2`).
    #[must_use]
    pub fn with_git_protocol(mut self, value: impl Into<String>) -> Self {
        self.git_protocol = Some(value.into());
        self
    }

    /// Override the `User-Agent` header.
    #[must_use]
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    fn cached_auth_header(&self) -> Option<String> {
        self.cached_auth
            .lock()
            .ok()
            .and_then(|g| g.clone())
    }

    fn store_auth_header(&self, header: String) {
        if let Ok(mut g) = self.cached_auth.lock() {
            *g = Some(header);
        }
    }

    fn clear_auth_header(&self) {
        if let Ok(mut g) = self.cached_auth.lock() {
            *g = None;
        }
    }
}

impl Default for UreqHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// A raw HTTP response captured from ureq (success or error status).
struct RawResponse {
    status: u16,
    www_authenticate: Vec<String>,
    body: Vec<u8>,
}

fn default_user_agent() -> String {
    format!("grit-lib/{}", env!("CARGO_PKG_VERSION"))
}

/// Decompose a request URL into a target [`Credential`] (protocol/host/path).
fn credential_for_url(url: &str) -> Option<Credential> {
    let parsed = url::Url::parse(url).ok()?;
    let protocol = parsed.scheme().to_string();
    let host = parsed.host_str()?.to_string();
    let host = if let Some(port) = parsed.port() {
        format!("{host}:{port}")
    } else {
        host
    };
    let path = parsed.path().trim_start_matches('/').to_string();
    Some(Credential {
        protocol: Some(protocol),
        host: Some(host),
        path: if path.is_empty() { None } else { Some(path) },
        url: Some(url.to_string()),
        ..Default::default()
    })
}

/// Build the `Authorization: Basic …` header value for `cred`.
fn basic_auth_header(cred: &Credential) -> Option<String> {
    let user = cred.username.as_deref()?;
    let pass = cred.password.as_deref().unwrap_or("");
    let raw = format!("{user}:{pass}");
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    Some(format!("Basic {encoded}"))
}

/// True when the server's `WWW-Authenticate` challenges include a Basic scheme
/// (or none at all — be permissive and try basic).
fn offers_basic(challenges: &[String]) -> bool {
    challenges.is_empty()
        || challenges
            .iter()
            .any(|c| c.trim_start().to_ascii_lowercase().starts_with("basic"))
}

impl UreqHttpClient {
    fn do_get(&self, url: &str, git_protocol: Option<&str>, auth: Option<&str>) -> Result<RawResponse> {
        let mut req = self.agent.get(url).set("User-Agent", &self.user_agent);
        if let Some(v) = git_protocol {
            req = req.set("Git-Protocol", v);
        }
        if let Some(a) = auth {
            req = req.set("Authorization", a);
        }
        finish(req.call())
    }

    fn do_post(
        &self,
        url: &str,
        content_type: &str,
        accept: &str,
        body: &[u8],
        git_protocol: Option<&str>,
        auth: Option<&str>,
    ) -> Result<RawResponse> {
        let mut req = self
            .agent
            .post(url)
            .set("Content-Type", content_type)
            .set("Accept", accept)
            .set("User-Agent", &self.user_agent);
        if let Some(v) = git_protocol {
            req = req.set("Git-Protocol", v);
        }
        if let Some(a) = auth {
            req = req.set("Authorization", a);
        }
        finish(req.send_bytes(body))
    }

    /// Run `attempt` (a GET or POST closure) with auth-retry on 401.
    fn with_auth_retry<F>(&self, url: &str, attempt: F) -> Result<Vec<u8>>
    where
        F: Fn(Option<&str>) -> Result<RawResponse>,
    {
        let initial_auth = self.cached_auth_header();
        let first = attempt(initial_auth.as_deref())?;
        if first.status != 401 {
            return finalize_status(url, first.status, first.body);
        }
        // 401: need credentials. Without a provider, surface the error.
        let Some(provider) = self.credentials.as_ref() else {
            self.clear_auth_header();
            return Err(http_status_error(url, 401));
        };
        if !offers_basic(&first.www_authenticate) {
            return Err(Error::Message(format!(
                "{url}: server requires an unsupported auth scheme: {:?}",
                first.www_authenticate
            )));
        }
        let Some(input) = credential_for_url(url) else {
            return Err(http_status_error(url, 401));
        };
        let cred = provider.fill(&input)?;
        let Some(header) = basic_auth_header(&cred) else {
            return Err(Error::Message(format!(
                "{url}: credential helper returned no usable username/password"
            )));
        };
        let retry = attempt(Some(&header))?;
        if retry.status >= 400 {
            let _ = provider.reject(&cred);
            self.clear_auth_header();
            return Err(http_status_error(url, retry.status));
        }
        // Success: approve + cache the working auth header.
        let _ = provider.approve(&cred);
        self.store_auth_header(header);
        Ok(retry.body)
    }
}

impl HttpClient for UreqHttpClient {
    fn get(&self, url: &str, git_protocol: Option<&str>) -> Result<Vec<u8>> {
        let gp = git_protocol.or(self.git_protocol.as_deref());
        self.with_auth_retry(url, |auth| self.do_get(url, gp, auth))
    }

    fn post(
        &self,
        url: &str,
        content_type: &str,
        accept: &str,
        body: &[u8],
        git_protocol: Option<&str>,
    ) -> Result<Vec<u8>> {
        let gp = git_protocol.or(self.git_protocol.as_deref());
        self.with_auth_retry(url, |auth| {
            self.do_post(url, content_type, accept, body, gp, auth)
        })
    }

    fn git_protocol_header(&self) -> Option<&str> {
        self.git_protocol.as_deref()
    }
}

/// Convert a ureq call result into a [`RawResponse`], reading the body.
///
/// `ureq` returns `Err(ureq::Error::Status(..))` for >= 400 responses; we map
/// those into a `RawResponse` so the auth-retry logic can inspect the status and
/// `WWW-Authenticate` headers rather than treating them as hard errors.
fn finish(result: std::result::Result<ureq::Response, ureq::Error>) -> Result<RawResponse> {
    match result {
        Ok(resp) => Ok(read_response(resp)),
        Err(ureq::Error::Status(_code, resp)) => Ok(read_response(resp)),
        Err(e) => Err(Error::Message(format!("http transport error: {e}"))),
    }
}

fn read_response(resp: ureq::Response) -> RawResponse {
    let status = resp.status();
    let www_authenticate = resp
        .all("WWW-Authenticate")
        .into_iter()
        .map(std::string::ToString::to_string)
        .collect();
    let mut body = Vec::new();
    // Read the body; an error leaves `body` as whatever was read.
    let _ = resp.into_reader().read_to_end(&mut body);
    RawResponse {
        status,
        www_authenticate,
        body,
    }
}

fn finalize_status(url: &str, status: u16, body: Vec<u8>) -> Result<Vec<u8>> {
    if status >= 400 {
        return Err(http_status_error(url, status));
    }
    Ok(body)
}

fn http_status_error(url: &str, status: u16) -> Error {
    Error::Message(format!("HTTP {status} from {url}"))
}
