//! The library against a fake YouTube (`tests/common`): fetching, listing,
//! translating, errors, proxies and retries, and formatters.
//! `tests/golden` holds reference output recorded from youtube-transcript-api
//! by `scripts/record_golden.py`; the formats must match it.

/// The fake YouTube, fixtures and golden output that the integration tests share.
#[path = "../common/mod.rs"]
mod common;

/// Playability edge cases, timestamps, and duplicate codes.
mod edge_cases;
/// The client, its futures, and the main fetch, list, and translate paths.
mod fetching;
/// Text, SRT, WebVTT, and JSON output.
mod formatters;
/// Connection failures, malformed caption data, and language variants.
mod malformed;
/// Properties over generated input: YouTube's JSON and XML, and URLs, are
/// untrusted, so the parsers must answer every input with a value or an
/// error, never a panic.
mod properties;
/// Fetching through generic and Webshare proxies.
mod proxies;
/// Retries on 429 responses and connection failures.
mod retries;
/// With the `tracing` feature, requests, retries, and the consent page are
/// events.
#[cfg(feature = "tracing")]
mod tracing_events;
/// Video details from the player response and the watch page.
mod video_details;
/// YouTube's failures: consent, blocking, and unplayable or missing videos.
mod youtube_errors;

use std::sync::Arc;
use std::time::Duration;

use common::{Canned, FakeYouTube, PLAYER, fixture};
use youtube_transcript_api::http::{HttpClient, Method, RetryPolicy};
use youtube_transcript_api::proxies::{GenericProxyConfig, ProxyConfig};
use youtube_transcript_api::{CouldNotRetrieveTranscript, Error, YouTubeTranscriptApi};

/// A client that sends every request to `fake`.
fn api(fake: &Arc<FakeYouTube>) -> YouTubeTranscriptApi {
    let client: Arc<dyn HttpClient> = Arc::<FakeYouTube>::clone(fake);
    YouTubeTranscriptApi::builder()
        .http_client(client)
        .build()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

/// A retry policy without waits, so tests of retries don't sleep.
const fn no_waits() -> RetryPolicy {
    RetryPolicy::new().with_backoff(Duration::ZERO, Duration::ZERO)
}

/// A client like `api` that also uses `proxy` as its proxy configuration,
/// and retries without waiting.
fn api_with_proxy(
    fake: &Arc<FakeYouTube>,
    proxy: impl ProxyConfig + 'static,
) -> YouTubeTranscriptApi {
    let client: Arc<dyn HttpClient> = Arc::<FakeYouTube>::clone(fake);
    YouTubeTranscriptApi::builder()
        .http_client(client)
        .proxy_config(proxy)
        .retry_policy(no_waits())
        .build()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

/// A generic proxy with its HTTP and HTTPS URLs both at `localhost:8080`.
fn generic_proxy() -> GenericProxyConfig {
    GenericProxyConfig::new(
        Some("http://localhost:8080".to_owned()),
        Some("http://localhost:8080".to_owned()),
    )
    .unwrap_or_else(|e| unreachable!("{e}"))
}

/// Makes `fake` answer player requests with the fixture `name`.
fn replace_player(fake: &FakeYouTube, name: &str) {
    fake.replace(&Method::POST, PLAYER, Canned::ok(fixture(name)));
}

/// The `CouldNotRetrieveTranscript` inside an error, or a failed test.
fn not_retrieved(result: Result<impl std::fmt::Debug, Error>) -> CouldNotRetrieveTranscript {
    match result {
        Err(Error::CouldNotRetrieveTranscript(error)) => error,
        other => unreachable!("expected CouldNotRetrieveTranscript, got {other:?}"),
    }
}

/// A playable video whose caption data is `captions`.
fn player_with_captions(captions: &serde_json::Value) -> Canned {
    Canned::ok(
        serde_json::json!({
            "playabilityStatus": {"status": "OK"},
            "captions": {"playerCaptionsTracklistRenderer": captions},
        })
        .to_string(),
    )
}

/// Manual `en-US` and `de`, generated `en`.
fn regional_captions() -> Canned {
    let track = |code: &str, kind: Option<&str>| {
        let mut track = serde_json::json!({
            "baseUrl": format!("https://www.youtube.com/api/timedtext?v=abc&lang={code}"),
            "name": {"runs": [{"text": code}]},
            "languageCode": code,
        });
        if let Some(asr) = kind {
            track["kind"] = asr.into();
        }
        track
    };
    player_with_captions(&serde_json::json!({
        "captionTracks": [track("en-US", None), track("en", Some("asr")), track("de", None)],
    }))
}
