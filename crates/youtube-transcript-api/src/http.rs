//! The HTTP layer.
//!
//! Requests go through the [`HttpClient`] trait. [`ReqwestClient`] is the
//! default, and tests or callers can plug in their own client.
//!
//! ```
//! use youtube_transcript_api::http::{
//!     BoxFuture, HeaderMap, HttpClient, HttpRequest, HttpResponse, Method, StatusCode,
//!     TransportError,
//! };
//!
//! /// Answers every request with an empty page, without the network.
//! #[derive(Debug)]
//! struct Offline;
//!
//! impl HttpClient for Offline {
//!     fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
//!         Box::pin(async move {
//!             Ok(HttpResponse::new(StatusCode::OK, request.url, HeaderMap::new(), String::new()))
//!         })
//!     }
//!
//!     fn set_cookie(&self, _name: &str, _value: &str, _domain: &str) {}
//! }
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let request = HttpRequest::new(Method::GET, "https://example.com/".to_owned(), HeaderMap::new(), None);
//! let response = Offline.send(request).await.unwrap();
//! assert_eq!(response.status, StatusCode::OK);
//! # }
//! ```

mod retry;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub use ::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use backon::Retryable;
use reqwest::cookie::{CookieStore, Jar};
use thiserror::Error;

use crate::proxies::{InvalidProxyConfig, ProxyConfig};
pub use retry::RetryPolicy;
use retry::retry_after;

/// A boxed future, so [`HttpClient`] stays object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A request handed to an [`HttpClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpRequest {
    /// The method: `GET` or `POST`.
    pub method: Method,
    /// The absolute URL.
    pub url: String,
    /// Extra headers.
    pub headers: HeaderMap,
    /// A JSON body for `POST` requests.
    pub json_body: Option<String>,
}

impl HttpRequest {
    /// A `method` request to `url` with `headers`, and a JSON body for a
    /// `POST`.
    #[must_use]
    pub const fn new(
        method: Method,
        url: String,
        headers: HeaderMap,
        json_body: Option<String>,
    ) -> Self {
        Self {
            method,
            url,
            headers,
            json_body,
        }
    }
}

/// A response from an [`HttpClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpResponse {
    /// The status code.
    pub status: StatusCode,
    /// The final URL, after redirects.
    pub url: String,
    /// Response headers.
    pub headers: HeaderMap,
    /// The decoded body.
    pub body: String,
}

impl HttpResponse {
    /// A response with `status` from `url`, the final URL after redirects.
    #[must_use]
    pub const fn new(status: StatusCode, url: String, headers: HeaderMap, body: String) -> Self {
        Self {
            status,
            url,
            headers,
            body,
        }
    }
}

/// A failure below HTTP: DNS, TLS, a refused connection, a timeout, or
/// retries running out. They arrive as [`crate::Error::Transport`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
#[non_exhaustive]
pub struct TransportError {
    /// Where the request failed, which decides whether a retry follows.
    pub kind: TransportErrorKind,
    /// What went wrong.
    pub message: String,
    /// For [`TransportErrorKind::RateLimited`], how long the server asked
    /// the caller to wait before trying again.
    pub retry_after: Option<Duration>,
}

impl TransportError {
    /// An error of `kind` with `message`.
    pub fn new(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after: None,
        }
    }

    /// The error for a 429 whose `Retry-After` of `wait` is longer than the
    /// `cap` that [`RetryPolicy`] allows.
    fn rate_limited(url: &str, wait: Duration, cap: Duration) -> Self {
        let message = format!(
            "HTTP 429 Too Many Requests for url ({url}): the server asks to wait {} s, \
             over the {} s limit for waiting",
            wait.as_secs(),
            cap.as_secs(),
        );
        Self {
            retry_after: Some(wait),
            ..Self::new(TransportErrorKind::RateLimited, message)
        }
    }
}

/// Where a request failed.
///
/// With a proxy that asks for retries, the client retries a connection
/// failure for every method, a failure after sending only for `GET` (a
/// `POST` may have reached the server), and a failed body read never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportErrorKind {
    /// No connection, so the request never went out: DNS, a refused
    /// connection, the proxy or TLS handshake, a connect timeout.
    Connect,
    /// The connection failed after sending the request, before a response.
    Request,
    /// Reading the response body failed.
    Body,
    /// Retries ran out.
    RetriesExhausted,
    /// HTTP 429 with a `Retry-After` longer than [`RetryPolicy`] waits for.
    /// The client did not retry the request; [`TransportError::retry_after`] holds the
    /// wait the server asked for.
    RateLimited,
    /// The response body passed the size limit (see
    /// [`ReqwestClient::with_max_body_bytes`]).
    BodyTooLarge,
}

/// Sends HTTP requests and keeps a cookie jar.
pub trait HttpClient: fmt::Debug + Send + Sync {
    /// Sends one request. A non-2xx status is a response, not an error.
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>>;

    /// Stores a cookie for later requests to `domain`.
    fn set_cookie(&self, name: &str, value: &str, domain: &str);
}

/// The default [`HttpClient`], backed by `reqwest` with a cookie jar.
#[derive(Clone)]
pub struct ReqwestClient {
    /// The `reqwest` client.
    client: reqwest::Client,
    /// Its cookie jar, shared so `set_cookie` and `cookies_for` see it.
    jar: Arc<Jar>,
    /// The largest response body accepted, in bytes.
    max_body_bytes: usize,
}

impl ReqwestClient {
    /// The default limit on a response body: `16 MiB`. A watch page is 1 to 3
    /// MB, so normal use never reaches it.
    pub const DEFAULT_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

    /// A client without proxies.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidProxyConfig`] only when built with a proxy; never here.
    pub fn new() -> Result<Self, InvalidProxyConfig> {
        Self::with_proxy(None)
    }

    /// A client that sends every request through `proxy_config`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidProxyConfig`] if a proxy URL is invalid or the TLS
    /// backend fails to start.
    pub fn with_proxy(proxy_config: Option<&dyn ProxyConfig>) -> Result<Self, InvalidProxyConfig> {
        Self::with_options(proxy_config, None)
    }

    /// A client with a proxy and a timeout for each request, from sending it
    /// to reading the whole response. Without a timeout, a stalled proxy or
    /// connection can hang a request indefinitely.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidProxyConfig`] if a proxy URL is invalid or the TLS
    /// backend fails to start.
    pub fn with_options(
        proxy_config: Option<&dyn ProxyConfig>,
        timeout: Option<Duration>,
    ) -> Result<Self, InvalidProxyConfig> {
        let jar = Arc::new(Jar::default());
        let client = build_client(&jar, proxy_config, timeout)
            .map_err(|error| InvalidProxyConfig(error.to_string()))?;
        Ok(Self {
            client,
            jar,
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
        })
    }

    /// Fails a request with [`TransportErrorKind::BodyTooLarge`] when its
    /// response body passes `limit` bytes, instead of reading it all into
    /// memory. [`Self::DEFAULT_MAX_BODY_BYTES`] by default.
    #[must_use]
    pub const fn with_max_body_bytes(mut self, limit: usize) -> Self {
        self.max_body_bytes = limit;
        self
    }

    /// The `Cookie` header this client would send to `url`.
    #[must_use]
    pub fn cookies_for(&self, url: &str) -> Option<String> {
        let parsed = reqwest::Url::parse(url).ok()?;
        self.jar
            .cookies(&parsed)
            .and_then(|value| value.to_str().ok().map(str::to_owned))
    }
}

impl fmt::Debug for ReqwestClient {
    /// Leaves out the `reqwest` client, whose `Debug` lists its proxies,
    /// and the cookie jar.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqwestClient")
            .field("max_body_bytes", &self.max_body_bytes)
            .finish_non_exhaustive()
    }
}

/// A `reqwest` client with the cookie jar, the proxies of `proxy_config` and
/// the timeout.
fn build_client(
    jar: &Arc<Jar>,
    proxy_config: Option<&dyn ProxyConfig>,
    timeout: Option<Duration>,
) -> reqwest::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().cookie_provider(Arc::clone(jar));
    if let Some(limit) = timeout {
        builder = builder.timeout(limit);
    }
    if let Some(config) = proxy_config {
        let urls = config.proxy_urls();
        builder = builder
            .proxy(reqwest::Proxy::http(&urls.http)?)
            .proxy(reqwest::Proxy::https(&urls.https)?);
        if config.prevent_keeping_connections_alive() {
            // No idle connection to reuse, so each request gets a new IP.
            builder = builder.pool_max_idle_per_host(0);
        }
    }
    builder.build()
}

impl HttpClient for ReqwestClient {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let mut builder = self
                .client
                .request(request.method, &request.url)
                .headers(request.headers);
            if let Some(body) = request.json_body {
                builder = builder
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body);
            }
            let response = builder.send().await.map_err(|e| {
                // Short of a connect error, reqwest can't tell whether the
                // request reached the server, so it may have.
                let kind = if e.is_connect() {
                    TransportErrorKind::Connect
                } else {
                    TransportErrorKind::Request
                };
                TransportError::new(kind, e.to_string())
            })?;
            let status = response.status();
            let url = response.url().to_string();
            let headers = response.headers().clone();
            let body = read_body(response, self.max_body_bytes).await?;
            Ok(HttpResponse {
                status,
                url,
                headers,
                body,
            })
        })
    }

    /// Ignores a cookie whose name, value or domain holds `;`, CR or LF,
    /// which would add attributes or headers to the cookie line.
    fn set_cookie(&self, name: &str, value: &str, domain: &str) {
        let host = domain.trim_start_matches('.');
        if [name, value, domain]
            .iter()
            .any(|text| text.contains([';', '\r', '\n']))
        {
            return;
        }
        if let Ok(url) = reqwest::Url::parse(&format!("https://{host}/")) {
            let cookie = format!("{name}={value}; Domain={domain}; Path=/");
            self.jar.add_cookie_str(&cookie, &url);
        }
    }
}

/// The response body as text, failing once it passes `limit` bytes. A
/// `Content-Length` over the limit fails before reading anything.
async fn read_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, TransportError> {
    let too_large = || {
        TransportError::new(
            TransportErrorKind::BodyTooLarge,
            format!("the response body is over the limit of {limit} bytes"),
        )
    };
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| TransportError::new(TransportErrorKind::Body, e.to_string()))?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// The client plus what every request needs: default headers, and retries
/// when a proxy asks for them.
#[derive(Debug, Clone)]
pub(crate) struct Session {
    /// Sends the requests.
    client: Arc<dyn HttpClient>,
    /// Headers for every request.
    headers: HeaderMap,
    /// The retry budget for each request.
    retries: u32,
    /// The backoff and `Retry-After` limits.
    policy: RetryPolicy,
}

impl Session {
    /// A session over `client`, with `proxy_config`'s keep-alive and retry
    /// settings, and the waits `policy` sets.
    pub(crate) fn new(
        client: Arc<dyn HttpClient>,
        proxy_config: Option<&dyn ProxyConfig>,
        policy: RetryPolicy,
    ) -> Self {
        let mut headers = HeaderMap::new();
        // The playability checks compare YouTube's reasons in English.
        headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static("en-US"));
        let retries = proxy_config.map_or(0, |config| {
            if config.prevent_keeping_connections_alive() {
                headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
            }
            config.retries_when_blocked()
        });
        Self {
            client,
            headers,
            retries: policy.retries.unwrap_or(retries),
            policy,
        }
    }

    /// Stores a cookie for later requests to `domain`.
    pub(crate) fn set_cookie(&self, name: &str, value: &str, domain: &str) {
        self.client.set_cookie(name, value, domain);
    }

    /// `GET`, with retries.
    pub(crate) async fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        self.send(Method::GET, url, None).await
    }

    /// `POST` with a JSON body; retries only when connecting failed.
    pub(crate) async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<HttpResponse, TransportError> {
        self.send(Method::POST, url, Some(body.to_string())).await
    }

    /// Sends with retries: one budget for every kind of retry, exponential
    /// backoff with full jitter, and `Retry-After` honoured up to the
    /// policy's cap. Only a `GET` retries on 429 or after sending.
    async fn send(
        &self,
        method: Method,
        url: &str,
        json_body: Option<String>,
    ) -> Result<HttpResponse, TransportError> {
        let idempotent = method == Method::GET;
        let attempt = || async {
            let request = self.request(method.clone(), url, json_body.clone());
            let result = self.client.send(request).await;
            event!(
                debug,
                %method,
                url,
                status = result.as_ref().ok().map(|response| response.status.as_u16()),
                error = result.as_ref().err().map(|error| error.message.as_str()),
                "request",
            );
            Attempt::classify(result, idempotent)
                .map_err(|failure| self.too_long(&failure, url).map_or(failure, Attempt::Final))
        };
        let retries = usize::try_from(self.retries).unwrap_or(usize::MAX);
        let policy = self.policy;
        let outcome = attempt
            .retry(policy.backoff(retries))
            .sleep(tokio::time::sleep)
            .when(Attempt::is_retryable)
            // `None` means no budget left: keep it, or this retries forever.
            .adjust(|failure, delay| {
                let next = delay.map(|jittered| failure.retry_after().unwrap_or(jittered));
                event!(info, url, cause = %failure.cause(url), retry_in = ?next, "request failed");
                next
            })
            .await;
        match outcome {
            Ok(response) => Ok(response),
            Err(Attempt::Final(error)) => Err(error),
            // Without retries, a 429 is a response for the caller to judge.
            Err(Attempt::TooManyRequests(response)) if self.retries == 0 => Ok(*response),
            Err(Attempt::Failed(error)) if self.retries == 0 => Err(error),
            Err(failure) => Err(gave_up(self.retries, &failure.cause(url))),
        }
    }

    /// For a 429 whose `Retry-After` is over the cap, while retries are on,
    /// a [`TransportErrorKind::RateLimited`] error carrying the wait:
    /// sleeping less and retrying would only draw another 429. Without
    /// retries, a 429 stays a response for the caller to judge.
    fn too_long(&self, failure: &Attempt, url: &str) -> Option<TransportError> {
        let cap = self.policy.retry_after_cap;
        let wait = failure.retry_after().filter(|&wait| wait > cap)?;
        (self.retries > 0).then(|| TransportError::rate_limited(url, wait, cap))
    }

    /// The request, with the session's headers.
    fn request(&self, method: Method, url: &str, json_body: Option<String>) -> HttpRequest {
        HttpRequest {
            method,
            url: url.to_owned(),
            headers: self.headers.clone(),
            json_body,
        }
    }
}

/// One try at a request, classified for retrying.
#[derive(Debug)]
enum Attempt {
    /// A 429 on a `GET`: retried, after `Retry-After` if it says.
    TooManyRequests(Box<HttpResponse>),
    /// No connection, or a `GET` that failed after sending: retried.
    Failed(TransportError),
    /// Any other failure: returned as is.
    Final(TransportError),
}

impl Attempt {
    /// Classifies a result; `idempotent` is true for a `GET`.
    fn classify(
        result: Result<HttpResponse, TransportError>,
        idempotent: bool,
    ) -> Result<HttpResponse, Self> {
        match result {
            Ok(response) if response.status == StatusCode::TOO_MANY_REQUESTS && idempotent => {
                Err(Self::TooManyRequests(Box::new(response)))
            }
            Ok(response) => Ok(response),
            Err(error)
                if error.kind == TransportErrorKind::Connect
                    || (error.kind == TransportErrorKind::Request && idempotent) =>
            {
                Err(Self::Failed(error))
            }
            Err(error) => Err(Self::Final(error)),
        }
    }

    /// Whether to retry.
    const fn is_retryable(&self) -> bool {
        !matches!(self, Self::Final(_))
    }

    /// What went wrong with the request to `url`.
    fn cause(&self, url: &str) -> String {
        match self {
            Self::TooManyRequests(_) => format!("HTTP 429 Too Many Requests for url ({url})"),
            Self::Failed(error) | Self::Final(error) => error.message.clone(),
        }
    }

    /// The wait a 429 asks for.
    fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::TooManyRequests(response) => retry_after(response),
            Self::Failed(_) | Self::Final(_) => None,
        }
    }
}

/// The error once `retries` retries have all failed, the last with `cause`.
fn gave_up(retries: u32, cause: &str) -> TransportError {
    TransportError::new(
        TransportErrorKind::RetriesExhausted,
        format!("gave up after {retries} retries: {cause}"),
    )
}

/// Retry classification and the rate-limit error.
#[cfg(test)]
mod tests {
    use super::{
        Arc, Attempt, HeaderMap, HeaderValue, HttpResponse, ReqwestClient, RetryPolicy, Session,
        StatusCode, TransportError, TransportErrorKind, gave_up, header,
    };
    use std::time::Duration;

    /// A 429 response with `Retry-After: value`.
    fn with_retry_after(value: &'static str) -> HttpResponse {
        let mut headers = HeaderMap::new();
        headers.insert(header::RETRY_AFTER, HeaderValue::from_static(value));
        HttpResponse::new(
            StatusCode::TOO_MANY_REQUESTS,
            String::new(),
            headers,
            String::new(),
        )
    }

    /// A session over `client` with `retries` retries and a 10 s
    /// `Retry-After` cap.
    fn session(client: ReqwestClient, retries: u32) -> Session {
        let policy = RetryPolicy::new()
            .with_retries(retries)
            .with_retry_after_cap(Duration::from_secs(10));
        Session::new(Arc::new(client), None, policy)
    }

    /// The give-up error names the retry count and the last failure.
    #[test]
    fn gave_up_names_the_last_failure() {
        let error = gave_up(3, "connection refused");
        assert_eq!(error.kind, TransportErrorKind::RetriesExhausted);
        assert_eq!(
            error.to_string(),
            "gave up after 3 retries: connection refused"
        );
    }

    /// Each attempt describes its failure.
    #[test]
    fn attempt_causes() {
        let error = || TransportError::new(TransportErrorKind::Body, "cut short");
        let too_many = Attempt::TooManyRequests(Box::new(with_retry_after("1")));
        assert_eq!(
            too_many.cause("https://x"),
            "HTTP 429 Too Many Requests for url (https://x)"
        );
        assert_eq!(Attempt::Failed(error()).cause("https://x"), "cut short");
        assert_eq!(Attempt::Final(error()).cause("https://x"), "cut short");
        assert_eq!(Attempt::Failed(error()).retry_after(), None);
    }

    /// A `Retry-After` over the cap, with retries on, is a final error
    /// that carries the wait.
    #[test]
    fn long_retry_after_is_a_rate_limit_error() {
        let long = Attempt::TooManyRequests(Box::new(with_retry_after("120")));
        let error = session(ReqwestClient::new().unwrap(), 2)
            .too_long(&long, "https://x")
            .unwrap();
        assert_eq!(error.kind, TransportErrorKind::RateLimited);
        assert_eq!(error.retry_after, Some(Duration::from_secs(120)));
        assert_eq!(
            error.message,
            "HTTP 429 Too Many Requests for url (https://x): the server asks to wait 120 s, \
             over the 10 s limit for waiting"
        );
    }

    /// Short waits, other failures, and sessions without retries pass through.
    #[test]
    fn other_failures_pass_through() {
        let short = Attempt::TooManyRequests(Box::new(with_retry_after("10")));
        let unasked = Attempt::TooManyRequests(Box::new(with_retry_after("soon")));
        let long = Attempt::TooManyRequests(Box::new(with_retry_after("120")));
        let failed = Attempt::Failed(TransportError::new(TransportErrorKind::Connect, "x"));
        let retrying = session(ReqwestClient::new().unwrap(), 2);
        assert_eq!(retrying.too_long(&short, "u"), None);
        assert_eq!(retrying.too_long(&unasked, "u"), None);
        assert_eq!(retrying.too_long(&failed, "u"), None);
        let direct = session(ReqwestClient::new().unwrap(), 0);
        assert_eq!(direct.too_long(&long, "u"), None);
    }
}
