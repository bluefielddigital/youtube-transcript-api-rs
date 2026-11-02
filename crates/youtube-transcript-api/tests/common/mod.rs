//! A fake YouTube for the tests.
//!
//! Routes match on method and URL without the query string. A route with a
//! list of canned responses returns them in order and then keeps returning
//! the last one, the way `responses` does.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use youtube_transcript_api::http::{
    BoxFuture, HeaderMap, HttpClient, HttpRequest, HttpResponse, Method, StatusCode,
    TransportError, TransportErrorKind,
};
use youtube_transcript_api::{FetchedTranscript, FetchedTranscriptSnippet};

/// The watch page URL, without the query string.
pub const WATCH: &str = "https://www.youtube.com/watch";
/// The InnerTube player endpoint, without the query string.
pub const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player";
/// The timedtext URL, without the query string.
pub const TIMEDTEXT: &str = "https://www.youtube.com/api/timedtext";

/// Reads a file from `tests/fixtures`.
pub fn fixture(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| unreachable!("{}: {e}", path.display()))
}

/// Reads a file from `tests/golden`: reference output recorded from
/// youtube-transcript-api.
pub fn golden(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "golden", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| unreachable!("{}: {e}", path.display()))
}

/// A response for a route to give.
#[derive(Debug, Clone)]
pub struct Canned {
    /// The HTTP status.
    pub status: StatusCode,
    /// The body.
    pub body: String,
    /// The headers.
    pub headers: HeaderMap,
    /// Fail below HTTP instead of responding.
    pub transport_error: Option<TransportErrorKind>,
}

impl Canned {
    /// A 200 with `body`.
    pub fn ok(body: String) -> Self {
        Self {
            status: StatusCode::OK,
            body,
            headers: HeaderMap::new(),
            transport_error: None,
        }
    }

    /// An empty response with `status`.
    pub fn status(status: u16) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or_else(|e| unreachable!("{e}")),
            ..Self::ok(String::new())
        }
    }

    /// The connection fails: refused or timed out, for example.
    pub fn transport_error() -> Self {
        Self::failure(TransportErrorKind::Connect)
    }

    /// The request fails at the transport level in the way `kind` describes.
    pub fn failure(kind: TransportErrorKind) -> Self {
        Self {
            transport_error: Some(kind),
            ..Self::ok(String::new())
        }
    }
}

/// The responses queued for one method and URL.
#[derive(Debug)]
struct Route {
    /// The method to match.
    method: Method,
    /// The URL to match, without the query string.
    url: String,
    /// The responses, in order; the last one repeats.
    responses: VecDeque<Canned>,
}

/// A request the fake received, with the `Cookie` header it would carry.
#[derive(Debug, Clone)]
pub struct Call {
    /// The request as sent.
    pub request: HttpRequest,
    /// The `Cookie` header the fake's jar would send with it.
    pub cookie: Option<String>,
}

impl Call {
    /// The value of header `name`, if any.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.request.headers.get(name)?.to_str().ok()
    }

    /// The values of a query parameter.
    pub fn query(&self, name: &str) -> Vec<String> {
        let Some((_, query)) = self.request.url.split_once('?') else {
            return Vec::new();
        };
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value.to_owned())
            .collect()
    }
}

/// Answers requests from its routes and records each call.
#[derive(Debug, Default)]
pub struct FakeYouTube {
    /// The routes, in the order the tests added them.
    routes: Mutex<Vec<Route>>,
    /// Every request received, in order.
    calls: Mutex<Vec<Call>>,
    /// The cookies set through `set_cookie`, as name-value pairs.
    cookies: Mutex<Vec<(String, String)>>,
}

/// Locks `mutex`, even after a panic in another test poisoned it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FakeYouTube {
    /// The fake with the default routes: a watch page, player response and
    /// transcript for `GJLlxj_dtq8`.
    pub fn new() -> Arc<Self> {
        let fake = Arc::new(Self::default());
        fake.add(
            &Method::POST,
            PLAYER,
            Canned::ok(fixture("youtube.innertube.json.static")),
        );
        fake.add(
            &Method::GET,
            WATCH,
            Canned::ok(fixture("youtube.html.static")),
        );
        fake.add(
            &Method::GET,
            TIMEDTEXT,
            Canned::ok(fixture("transcript.xml.static")),
        );
        fake
    }

    /// `responses.add`: queue another response for the route.
    pub fn add(&self, method: &Method, url: &str, canned: Canned) {
        let mut routes = lock(&self.routes);
        if let Some(route) = routes
            .iter_mut()
            .find(|r| r.method == *method && r.url == url)
        {
            route.responses.push_back(canned);
        } else {
            routes.push(Route {
                method: method.clone(),
                url: url.to_owned(),
                responses: VecDeque::from([canned]),
            });
        }
    }

    /// `responses.replace`: replace the first queued response for the route.
    pub fn replace(&self, method: &Method, url: &str, canned: Canned) {
        let mut routes = lock(&self.routes);
        let route = routes
            .iter_mut()
            .find(|r| r.method == *method && r.url == url)
            .unwrap_or_else(|| unreachable!("no route {url}"));
        route.responses[0] = canned;
        drop(routes);
    }

    /// Every request received so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    /// The next response for `request`, or `None` without a matching route.
    fn respond(&self, request: &HttpRequest) -> Option<Canned> {
        let base = request.url.split('?').next().unwrap_or_default();
        let mut routes = lock(&self.routes);
        let route = routes
            .iter_mut()
            .find(|r| r.method == request.method && r.url == base)?;
        let canned = if route.responses.len() > 1 {
            route.responses.pop_front()
        } else {
            route.responses.front().cloned()
        };
        drop(routes);
        canned
    }
}

impl HttpClient for FakeYouTube {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let cookies = lock(&self.cookies);
            let cookie = (!cookies.is_empty()).then(|| {
                cookies
                    .iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            });
            drop(cookies);
            lock(&self.calls).push(Call {
                request: request.clone(),
                cookie,
            });
            let canned = self.respond(&request).ok_or_else(|| {
                TransportError::new(
                    TransportErrorKind::Connect,
                    format!("Connection refused by Responses: {}", request.url),
                )
            })?;
            if let Some(kind) = canned.transport_error {
                return Err(TransportError::new(
                    kind,
                    format!("Connection refused by Responses: {}", request.url),
                ));
            }
            Ok(HttpResponse::new(
                canned.status,
                request.url,
                canned.headers,
                canned.body,
            ))
        })
    }

    fn set_cookie(&self, name: &str, value: &str, _domain: &str) {
        lock(&self.cookies).push((name.to_owned(), value.to_owned()));
    }
}

/// The transcript in `transcript.xml.static`.
pub fn ref_transcript() -> FetchedTranscript {
    FetchedTranscript::new(
        "GJLlxj_dtq8",
        "English",
        "en",
        false,
        vec![
            FetchedTranscriptSnippet::new("Hey, this is just a test", 0.0, 1.54),
            FetchedTranscriptSnippet::new("this is not the original transcript", 1.54, 4.16),
            FetchedTranscriptSnippet::new(
                "just something shorter, I made up for testing",
                5.7,
                3.239,
            ),
        ],
    )
}
