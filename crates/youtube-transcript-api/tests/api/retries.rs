use std::sync::Arc;
use std::time::Duration;

use youtube_transcript_api::http::{
    HeaderValue, HttpClient, Method, RetryPolicy, TransportErrorKind, header,
};
use youtube_transcript_api::proxies::{ProxyKind, WebshareProxyConfig};
use youtube_transcript_api::{DEFAULT_LANGUAGES, Error, Reason, YouTubeTranscriptApi};

use crate::common::{
    Canned, FakeYouTube, PLAYER, TIMEDTEXT, WATCH, fixture, golden, ref_transcript,
};
use crate::{api, api_with_proxy, no_waits, not_retrieved};

/// A watch page that keeps answering 429 uses up the retries and fails with `RetriesExhausted`.
#[tokio::test]
async fn watch_page_429_with_retries_runs_out() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::status(429));
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(2);

    let result = api_with_proxy(&fake, proxy)
        .fetch("abc", &DEFAULT_LANGUAGES, false)
        .await;

    // A `GET` retries on 429 (1 try + 2 retries); running out is a
    // transport error, not `IpBlocked`.
    assert_eq!(fake.calls().len(), 3);
    let Err(Error::Transport(error)) = result else {
        unreachable!("expected a transport error, got {result:?}");
    };
    assert_eq!(error.kind, TransportErrorKind::RetriesExhausted);
    assert!(
        error
            .message
            .starts_with("gave up after 2 retries: HTTP 429")
    );
}

/// One failure of `kind` on `method url`, then the usual response.
fn fails_once(method: &Method, url: &str, kind: TransportErrorKind) -> Arc<FakeYouTube> {
    let fake = FakeYouTube::new();
    let usual = if *method == Method::POST {
        fixture("youtube.innertube.json.static")
    } else if url == WATCH {
        fixture("youtube.html.static")
    } else {
        fixture("transcript.xml.static")
    };
    fake.replace(method, url, Canned::failure(kind));
    fake.add(method, url, Canned::ok(usual));
    fake
}

/// A refused connection retries for `GET` and `POST` alike, and the fetch then succeeds.
#[tokio::test]
async fn connection_failures_are_retried_for_every_method() {
    // A request that never reached the server retries, POST included.
    for (method, url) in [
        (Method::GET, WATCH),
        (Method::POST, PLAYER),
        (Method::GET, TIMEDTEXT),
    ] {
        let fake = fails_once(&method, url, TransportErrorKind::Connect);
        let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(1);
        let transcript = api_with_proxy(&fake, proxy)
            .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
            .await;
        assert_eq!(transcript.ok(), Some(ref_transcript()), "{url}");
        assert_eq!(fake.calls().len(), 4, "{url}");
    }
}

/// A failure after sending retries for a `GET` but not for a `POST`, which fails with
/// `TransportErrorKind::Request`.
#[tokio::test]
async fn failures_after_sending_are_retried_only_for_get() {
    let get = fails_once(&Method::GET, TIMEDTEXT, TransportErrorKind::Request);
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(1);
    let transcript = api_with_proxy(&get, proxy.clone())
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    assert_eq!(transcript.ok(), Some(ref_transcript()));

    let post = fails_once(&Method::POST, PLAYER, TransportErrorKind::Request);
    let result = api_with_proxy(&post, proxy)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    let Err(Error::Transport(error)) = result else {
        unreachable!("expected a transport error, got {result:?}");
    };
    assert_eq!(error.kind, TransportErrorKind::Request);
    assert_eq!(post.calls().len(), 2);
}

/// A failure while reading the body never retries, and neither does a refused connection without a
/// proxy.
#[tokio::test]
async fn body_failures_and_unproxied_failures_are_not_retried() {
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(1);
    let body = fails_once(&Method::GET, TIMEDTEXT, TransportErrorKind::Body);
    let body_result = api_with_proxy(&body, proxy)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    assert!(
        matches!(body_result, Err(Error::Transport(_))),
        "{body_result:?}"
    );
    assert_eq!(body.calls().len(), 3);

    let direct = fails_once(&Method::GET, WATCH, TransportErrorKind::Connect);
    let direct_result = api(&direct)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    assert!(
        matches!(direct_result, Err(Error::Transport(_))),
        "{direct_result:?}"
    );
    assert_eq!(direct.calls().len(), 1);
}

/// A 429 and a refused connection count against the same retry budget.
#[tokio::test]
async fn connection_failures_share_the_retry_budget_with_429s() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::status(429));
    fake.add(&Method::GET, WATCH, Canned::transport_error());
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(2);

    let result = api_with_proxy(&fake, proxy)
        .fetch("abc", &DEFAULT_LANGUAGES, false)
        .await;

    assert_eq!(fake.calls().len(), 3);
    let Err(Error::Transport(error)) = result else {
        unreachable!("expected a transport error, got {result:?}");
    };
    assert_eq!(error.kind, TransportErrorKind::RetriesExhausted);
    assert_eq!(
        error.to_string(),
        format!("gave up after 2 retries: Connection refused by Responses: {WATCH}?v=abc")
    );
}

/// A 429 from the player becomes `IpBlocked`, and the fetcher, not the HTTP layer, retries it.
#[tokio::test]
async fn player_429_is_ip_blocked_and_retried_by_the_fetcher() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, Canned::status(429));
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(3);

    let error = not_retrieved(
        api_with_proxy(&fake, proxy)
            .fetch("abc", &DEFAULT_LANGUAGES, false)
            .await,
    );

    // The HTTP layer never retries a POST; the fetcher's own retry loop does.
    assert_eq!(fake.calls().len(), 3 * 2);
    assert_eq!(
        error.reason,
        Reason::IpBlocked {
            proxy: Some(ProxyKind::Webshare)
        }
    );
}

/// A transcript list displays like the reference listing.
#[tokio::test]
async fn transcript_list_display_matches_reference() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    assert_eq!(list.to_string(), golden("transcript_list.txt"));
}

/// `YouTubeTranscriptApi::new` builds a client without touching the network.
#[tokio::test]
async fn default_client_builds_without_network() {
    assert!(YouTubeTranscriptApi::new().is_ok());
}

/// A 429 with `Retry-After` waits that long, then retries and succeeds.
#[tokio::test]
async fn retry_after_is_honoured_before_retrying() {
    let fake = FakeYouTube::new();
    let mut too_many = Canned::status(429);
    too_many
        .headers
        .insert(header::RETRY_AFTER, HeaderValue::from_static("0"));
    fake.replace(&Method::GET, WATCH, too_many);
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube.html.static")),
    );
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(1);

    let transcript = api_with_proxy(&fake, proxy)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();

    assert_eq!(transcript, ref_transcript());
    assert_eq!(fake.calls().len(), 4);
}

/// A client over `fake` with `policy`, and no proxy.
fn api_with_policy(fake: &Arc<FakeYouTube>, policy: RetryPolicy) -> YouTubeTranscriptApi {
    let client: Arc<dyn HttpClient> = Arc::<FakeYouTube>::clone(fake);
    YouTubeTranscriptApi::builder()
        .http_client(client)
        .retry_policy(policy)
        .build()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

/// A watch page that answers 429 with `Retry-After: seconds`, then succeeds.
fn too_many_requests_once(seconds: &'static str) -> Arc<FakeYouTube> {
    let fake = FakeYouTube::new();
    let mut too_many = Canned::status(429);
    too_many
        .headers
        .insert(header::RETRY_AFTER, HeaderValue::from_static(seconds));
    fake.replace(&Method::GET, WATCH, too_many);
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube.html.static")),
    );
    fake
}

/// Retries wait between tries, never longer than the doubling ceiling, on
/// the paused clock so the test takes no time.
#[tokio::test(start_paused = true)]
async fn backoff_waits_stay_under_the_ceiling() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::transport_error());
    let policy = RetryPolicy::new()
        .with_retries(5)
        .with_backoff(Duration::from_secs(1), Duration::from_secs(2));
    let started = tokio::time::Instant::now();

    let result = api_with_policy(&fake, policy)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;

    // Five retries, each under 1 s, 2 s, 2 s, 2 s, and 2 s.
    assert_eq!(fake.calls().len(), 6);
    assert!(result.is_err());
    assert!(started.elapsed() <= Duration::from_secs(9));
}

/// A `Retry-After` within the cap makes the client sleep that long, then
/// retry.
#[tokio::test(start_paused = true)]
async fn retry_after_within_the_cap_is_waited_out() {
    let fake = too_many_requests_once("30");
    let started = tokio::time::Instant::now();

    let transcript = api_with_policy(&fake, RetryPolicy::new().with_retries(1))
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();

    assert_eq!(transcript, ref_transcript());
    assert!(started.elapsed() >= Duration::from_secs(30));
    assert!(started.elapsed() < Duration::from_secs(31));
}

/// A `Retry-After` over the cap fails at once with the wait in the error,
/// without a retry, or a sleep.
#[tokio::test(start_paused = true)]
async fn retry_after_over_the_cap_fails_with_the_wait() {
    let fake = too_many_requests_once("120");
    let started = tokio::time::Instant::now();

    let result = api_with_policy(&fake, RetryPolicy::new().with_retries(3))
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;

    let Err(Error::Transport(error)) = result else {
        unreachable!("expected a transport error, got {result:?}");
    };
    assert_eq!(error.kind, TransportErrorKind::RateLimited);
    assert_eq!(error.retry_after, Some(Duration::from_secs(120)));
    assert_eq!(fake.calls().len(), 1);
    assert_eq!(started.elapsed(), Duration::ZERO);
}

/// The cap is configurable: 10 s is over a 5 s cap and within a 15 s one.
#[tokio::test(start_paused = true)]
async fn the_retry_after_cap_is_configurable() {
    let fake = too_many_requests_once("10");
    let strict = RetryPolicy::new()
        .with_retries(1)
        .with_retry_after_cap(Duration::from_secs(5));
    let result = api_with_policy(&fake, strict)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    assert!(
        matches!(&result, Err(Error::Transport(e)) if e.kind == TransportErrorKind::RateLimited),
        "{result:?}"
    );

    let relaxed_fake = too_many_requests_once("10");
    let relaxed = RetryPolicy::new()
        .with_retries(1)
        .with_retry_after_cap(Duration::from_secs(15));
    let transcript = api_with_policy(&relaxed_fake, relaxed)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await;
    assert_eq!(transcript.ok(), Some(ref_transcript()));
}

/// Without retries, a 429 stays a response, even one asking for a long wait.
#[tokio::test]
async fn a_long_retry_after_without_retries_is_ip_blocked() {
    let fake = too_many_requests_once("9999");
    let error = not_retrieved(
        api(&fake)
            .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert!(error.reason.is_request_blocked());
    assert_eq!(fake.calls().len(), 1);
}

/// `RetryPolicy::with_retries` turns retries on without a proxy.
#[tokio::test]
async fn the_policy_sets_the_retry_count_without_a_proxy() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::status(429));

    let result = api_with_policy(&fake, no_waits().with_retries(2))
        .fetch("abc", &DEFAULT_LANGUAGES, false)
        .await;

    assert_eq!(fake.calls().len(), 3);
    assert!(
        matches!(&result, Err(Error::Transport(e)) if e.kind == TransportErrorKind::RetriesExhausted),
        "{result:?}"
    );
}
