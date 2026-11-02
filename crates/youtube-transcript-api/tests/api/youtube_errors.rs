use youtube_transcript_api::http::Method;
use youtube_transcript_api::{DEFAULT_LANGUAGES, Reason};

use crate::common::{Canned, FakeYouTube, PLAYER, TIMEDTEXT, WATCH, fixture};
use crate::{api, not_retrieved, replace_player};

/// On the consent page, `fetch` sets the consent cookie and sends it with every later request.
#[tokio::test]
async fn fetch_create_consent_cookie_if_needed() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page.html.static")),
    );
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube.html.static")),
    );

    api(&fake)
        .fetch("F1xioXWb8CY", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();

    let calls = fake.calls();
    assert_eq!(calls.len(), 4);
    for call in &calls[1..] {
        assert_eq!(
            call.cookie.as_deref(),
            Some("CONSENT=YES+cb.20210328-17-p0.de+FX+119")
        );
    }
}

/// A second consent page after setting the cookie fails with `FailedToCreateConsentCookie`.
#[tokio::test]
async fn fetch_exception_if_create_consent_cookie_failed() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page.html.static")),
    );
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page.html.static")),
    );
    let error = not_retrieved(
        api(&fake)
            .fetch("F1xioXWb8CY", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::FailedToCreateConsentCookie);
}

/// A consent page without a usable cookie value fails with `FailedToCreateConsentCookie`.
#[tokio::test]
async fn fetch_exception_if_consent_cookie_age_invalid() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page_invalid.html.static")),
    );
    let error = not_retrieved(
        api(&fake)
            .fetch("F1xioXWb8CY", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::FailedToCreateConsentCookie);
}

/// An unavailable video fails with `VideoUnavailable`.
#[tokio::test]
async fn fetch_exception_if_video_unavailable() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_video_unavailable.innertube.json.static");
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(error.reason, Reason::VideoUnavailable);
}

/// A 500 from the player fails with `YouTubeRequestFailed`, which names the status and URL.
#[tokio::test]
async fn fetch_exception_if_youtube_request_fails() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, Canned::status(500));
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert!(matches!(error.reason, Reason::YouTubeRequestFailed { .. }));
    assert!(error.to_string().contains("Request to YouTube failed: "));
    assert!(error.to_string().contains(
        "HTTP status server error (500 Internal Server Error) for url \
         (https://www.youtube.com/youtubei/v1/player?key="
    ));
}

/// YouTube's too-many-requests watch page fails with `IpBlocked`.
#[tokio::test]
async fn fetch_exception_if_youtube_request_limit_reached() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_too_many_requests.html.static")),
    );
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(error.reason, Reason::IpBlocked { proxy: None });
}

/// A 429 from the `timedtext` endpoint fails with `IpBlocked`.
#[tokio::test]
async fn fetch_exception_if_timedtext_request_limit_reached() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, TIMEDTEXT, Canned::status(429));
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(error.reason, Reason::IpBlocked { proxy: None });
}

/// An age-restricted video fails with `AgeRestricted`.
#[tokio::test]
async fn fetch_exception_if_age_restricted() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_age_restricted.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::AgeRestricted);
}

/// A player response that demands a proof-of-origin token fails with `PoTokenRequired`.
#[tokio::test]
async fn fetch_exception_if_po_token_required() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_po_token_required.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::PoTokenRequired);
}

/// A blocked player request without a proxy fails with `RequestBlocked`, saying YouTube blocks the
/// IP.
#[tokio::test]
async fn fetch_exception_request_blocked() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_request_blocked.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::RequestBlocked { proxy: None });
    assert!(
        error
            .to_string()
            .contains("YouTube is blocking requests from your IP")
    );
}

/// An unplayable video fails with `VideoUnplayable`, carrying its reason and sub-reasons.
#[tokio::test]
async fn fetch_exception_unplayable() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_unplayable.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(
        error.reason,
        Reason::VideoUnplayable {
            reason: Some("Custom Reason".to_owned()),
            sub_reasons: vec!["Sub Reason 1".to_owned(), "Sub Reason 2".to_owned()],
        }
    );
    assert!(error.to_string().contains("Custom Reason"));
}

/// Both fixtures for videos without captions fail with `TranscriptsDisabled`.
#[tokio::test]
async fn fetch_exception_if_transcripts_disabled() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_transcripts_disabled.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .fetch("dsMFmonKDD4", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(error.reason, Reason::TranscriptsDisabled);

    replace_player(&fake, "youtube_transcripts_disabled2.innertube.json.static");
    let second = not_retrieved(
        api(&fake)
            .fetch("Fjg5lYqvzUs", &DEFAULT_LANGUAGES, false)
            .await,
    );
    assert_eq!(second.reason, Reason::TranscriptsDisabled);
}

/// A language code the video lacks fails with a message listing the requested codes.
#[tokio::test]
async fn fetch_exception_if_language_unavailable() {
    let fake = FakeYouTube::new();
    let error = not_retrieved(api(&fake).fetch("GJLlxj_dtq8", &["cz"], false).await);
    let text = error.to_string();
    assert!(text.contains("No transcripts were found for"));
    assert!(text.contains("requested language codes: [\"cz\"]\n\nFor this video (GJLlxj_dtq8)"));
}
