use youtube_transcript_api::http::Method;
use youtube_transcript_api::proxies::{ProxyKind, WebshareProxyConfig};
use youtube_transcript_api::{DEFAULT_LANGUAGES, Reason};

use crate::common::{Canned, FakeYouTube, PLAYER, fixture, ref_transcript};
use crate::{api_with_proxy, generic_proxy, not_retrieved, replace_player};

/// `fetch` through a generic proxy returns the reference transcript.
#[tokio::test]
async fn fetch_with_proxy() {
    let fake = FakeYouTube::new();
    let transcript = api_with_proxy(&fake, generic_proxy())
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();
    assert_eq!(transcript, ref_transcript());
}

/// A Webshare proxy sends `Connection: close`, so each request opens a new connection.
#[tokio::test]
async fn fetch_with_proxy_prevent_alive_connections() {
    let fake = FakeYouTube::new();
    api_with_proxy(&fake, WebshareProxyConfig::new("username", "password"))
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();
    let last = fake.calls().pop().unwrap();
    assert_eq!(last.header("Connection"), Some("close"));
}

/// Through a Webshare proxy, `fetch` retries after three blocked player responses and succeeds on
/// the fourth try.
#[tokio::test]
async fn fetch_with_proxy_retry_when_blocked() {
    let fake = FakeYouTube::new();
    let blocked = || Canned::ok(fixture("youtube_request_blocked.innertube.json.static"));
    fake.replace(&Method::POST, PLAYER, blocked());
    for _ in 0..2 {
        fake.add(&Method::POST, PLAYER, blocked());
    }
    fake.add(
        &Method::POST,
        PLAYER,
        Canned::ok(fixture("youtube.innertube.json.static")),
    );

    api_with_proxy(&fake, WebshareProxyConfig::new("username", "password"))
        .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();

    assert_eq!(fake.calls().len(), 2 * 3 + 3);
}

/// Once a Webshare proxy runs out of retries, `fetch` fails with `RequestBlocked` naming Webshare.
#[tokio::test]
async fn fetch_with_webshare_proxy_reraise_when_blocked() {
    let retries = 5;
    let fake = FakeYouTube::new();
    let blocked = || Canned::ok(fixture("youtube_request_blocked.innertube.json.static"));
    fake.replace(&Method::POST, PLAYER, blocked());
    for _ in 0..retries - 1 {
        fake.add(&Method::POST, PLAYER, blocked());
    }
    let proxy = WebshareProxyConfig::new("username", "password").with_retries_when_blocked(retries);

    let error = not_retrieved(
        api_with_proxy(&fake, proxy)
            .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
            .await,
    );

    assert_eq!(fake.calls().len(), usize::try_from(retries * 2).unwrap());
    assert_eq!(
        error.reason,
        Reason::RequestBlocked {
            proxy: Some(ProxyKind::Webshare)
        }
    );
    assert!(error.to_string().contains("Webshare"));
}

/// A generic proxy never retries a blocked request and fails with `RequestBlocked` naming the
/// generic proxy.
#[tokio::test]
async fn fetch_with_generic_proxy_reraise_when_blocked() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_request_blocked.innertube.json.static");

    let error = not_retrieved(
        api_with_proxy(&fake, generic_proxy())
            .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
            .await,
    );

    assert_eq!(fake.calls().len(), 2);
    assert_eq!(
        error.reason,
        Reason::RequestBlocked {
            proxy: Some(ProxyKind::Generic)
        }
    );
    assert!(
        error
            .to_string()
            .contains("YouTube is blocking your requests")
    );
}

/// `Debug` of the builder and the client never shows a proxy password, for
/// built-in and custom proxy configs alike.
#[test]
fn debug_output_hides_proxy_credentials() {
    use std::time::Duration;
    use youtube_transcript_api::YouTubeTranscriptApi;
    use youtube_transcript_api::http::RetryPolicy;
    use youtube_transcript_api::proxies::GenericProxyConfig;

    let generic = || {
        GenericProxyConfig::new(Some("http://user:hunter2@127.0.0.1:9".to_owned()), None)
            .unwrap_or_else(|e| unreachable!("{e}"))
    };
    let builder = || {
        YouTubeTranscriptApi::builder()
            .timeout(Duration::from_secs(5))
            .max_body_bytes(1024)
            .retry_policy(RetryPolicy::new())
    };
    let shown = [
        format!("{:?}", builder().proxy_config(generic())),
        format!(
            "{:?}",
            builder().proxy_config(WebshareProxyConfig::new("user", "hunter2"))
        ),
        format!(
            "{:?}",
            builder()
                .proxy_config(generic())
                .build()
                .unwrap_or_else(|e| unreachable!("{e}"))
        ),
        format!("{:?}", builder()),
    ];
    for text in &shown {
        assert!(!text.contains("hunter2"), "{text}");
    }
    assert!(shown[0].contains("http://***@127.0.0.1:9"), "{}", shown[0]);
    assert!(shown[2].contains("http://***@127.0.0.1:9"), "{}", shown[2]);
    assert!(
        shown[3].contains("max_body_bytes: Some(1024)"),
        "{}",
        shown[3]
    );
}

/// A proxy URL the HTTP client can't use fails `build`, not a later request.
#[test]
fn unusable_proxy_url_fails_to_build() {
    use youtube_transcript_api::YouTubeTranscriptApi;
    use youtube_transcript_api::proxies::GenericProxyConfig;

    let proxy = GenericProxyConfig::new(Some("::not a url::".to_owned()), None)
        .unwrap_or_else(|e| unreachable!("{e}"));
    let result = YouTubeTranscriptApi::builder().proxy_config(proxy).build();
    assert!(matches!(
        result,
        Err(youtube_transcript_api::Error::InvalidProxyConfig(_))
    ));
}
