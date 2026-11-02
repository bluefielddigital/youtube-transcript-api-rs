//! A fake YouTube for the CLI's run tests: one canned body per route, and a
//! record of the requests. The library's tests have a fuller one.
//!
//! The recorded responses are the library's: they live in its `tests/`
//! directory, and the CLI's tests read them from there.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use youtube_transcript_api::http::{
    BoxFuture, HeaderMap, HttpClient, HttpRequest, HttpResponse, Method, StatusCode, TransportError,
};

/// The watch page URL, without the query string.
pub const WATCH: &str = "https://www.youtube.com/watch";
/// The InnerTube player endpoint, without the query string.
pub const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player";
/// The timedtext URL, without the query string.
pub const TIMEDTEXT: &str = "https://www.youtube.com/api/timedtext";

/// Reads `name` from a directory of the library crate's `tests`.
fn library_test_file(directory: &str, name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "youtube-transcript-api",
        "tests",
        directory,
        name,
    ]
    .iter()
    .collect();
    std::fs::read_to_string(&path).unwrap()
}

/// Reads a recorded YouTube response from the library's `tests/fixtures`.
pub fn fixture(name: &str) -> String {
    library_test_file("fixtures", name)
}

/// Reads output recorded from the reference from the library's `tests/golden`.
pub fn golden(name: &str) -> String {
    library_test_file("golden", name)
}

/// Locks `mutex`, even after a panic in another test poisoned it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A route: the 200 body to answer a method and URL with.
#[derive(Debug)]
struct Route {
    /// The method to match.
    method: Method,
    /// The URL to match, without the query string.
    url: String,
    /// The body to answer with.
    body: String,
}

/// Answers requests from its routes and records each one.
#[derive(Debug, Default)]
pub struct FakeYouTube {
    /// The routes.
    routes: Mutex<Vec<Route>>,
    /// Every request received, in order.
    calls: Mutex<Vec<HttpRequest>>,
    /// The cookies set through `set_cookie`, as name-value pairs.
    cookies: Mutex<Vec<(String, String)>>,
}

impl FakeYouTube {
    /// The fake with the default routes: a watch page, player response and
    /// transcript for `GJLlxj_dtq8`.
    pub fn new() -> Arc<Self> {
        let fake = Arc::new(Self::default());
        fake.replace(
            &Method::POST,
            PLAYER,
            fixture("youtube.innertube.json.static"),
        );
        fake.replace(&Method::GET, WATCH, fixture("youtube.html.static"));
        fake.replace(&Method::GET, TIMEDTEXT, fixture("transcript.xml.static"));
        fake
    }

    /// Makes the route answer with `body`, adding the route if new.
    pub fn replace(&self, method: &Method, url: &str, body: String) {
        let mut routes = lock(&self.routes);
        routes.retain(|r| r.method != *method || r.url != url);
        routes.push(Route {
            method: method.clone(),
            url: url.to_owned(),
            body,
        });
    }

    /// Every request received so far, in order.
    pub fn calls(&self) -> Vec<HttpRequest> {
        lock(&self.calls).clone()
    }
}

/// The values of query parameter `name` in the request's URL.
pub fn query(request: &HttpRequest, name: &str) -> Vec<String> {
    let Some((_, query)) = request.url.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
        .collect()
}

impl HttpClient for FakeYouTube {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            lock(&self.calls).push(request.clone());
            let base = request.url.split('?').next().unwrap_or_default();
            let body = lock(&self.routes)
                .iter()
                .find(|r| r.method == request.method && r.url == base)
                .map(|r| r.body.clone())
                .unwrap_or_default();
            Ok(HttpResponse::new(
                StatusCode::OK,
                request.url,
                HeaderMap::new(),
                body,
            ))
        })
    }

    fn set_cookie(&self, name: &str, value: &str, _domain: &str) {
        lock(&self.cookies).push((name.to_owned(), value.to_owned()));
    }
}

/// The fake's own behavior: cookies, unknown routes, and the query helper.
#[cfg(test)]
mod tests {
    use super::*;

    /// A cookie set through the client stays in the record.
    #[test]
    fn set_cookie_is_recorded() {
        let fake = FakeYouTube::default();
        fake.set_cookie("CONSENT", "YES+", ".youtube.com");
        assert_eq!(
            *lock(&fake.cookies),
            [("CONSENT".to_owned(), "YES+".to_owned())]
        );
    }

    /// A request to a route nobody added gets an empty body, and `query`
    /// reads every value of a parameter, none without a query string.
    #[tokio::test]
    async fn unknown_routes_answer_empty() {
        let fake = FakeYouTube::default();
        let request =
            |url: &str| HttpRequest::new(Method::GET, url.to_owned(), HeaderMap::new(), None);
        let with_query = request("https://example.com/?a=1&a=2&b");
        let response = fake.send(with_query.clone()).await.unwrap();
        assert_eq!(response.body, "");
        assert_eq!(fake.calls().len(), 1);
        assert_eq!(query(&with_query, "a"), ["1", "2"]);
        assert_eq!(
            query(&request("https://example.com/"), "a"),
            Vec::<String>::new()
        );
    }
}
