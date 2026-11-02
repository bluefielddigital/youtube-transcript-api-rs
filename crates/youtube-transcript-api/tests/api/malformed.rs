use std::sync::Arc;

use youtube_transcript_api::http::{HttpClient, Method};
use youtube_transcript_api::{
    DEFAULT_LANGUAGES, Error, Transcript, UnexpectedData, YouTubeTranscriptApi,
};

use crate::common::{Canned, FakeYouTube, PLAYER, TIMEDTEXT, WATCH, fixture};
use crate::{api, not_retrieved, player_with_captions, regional_captions};

/// A refused connection on the watch page, player or `timedtext` request fails with
/// `Error::Transport`.
#[tokio::test]
async fn connection_failures_are_transport_errors() {
    for (method, url) in [
        (Method::GET, WATCH),
        (Method::POST, PLAYER),
        (Method::GET, TIMEDTEXT),
    ] {
        let fake = FakeYouTube::new();
        fake.replace(&method, url, Canned::transport_error());
        let result = api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await;
        assert!(
            matches!(result, Err(Error::Transport(_))),
            "{url}: {result:?}"
        );
    }
}

/// A 503 on the watch page after accepting the consent page becomes a request failure naming the
/// status.
#[tokio::test]
async fn failing_watch_page_after_consent_is_youtube_request_failed() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page.html.static")),
    );
    fake.add(&Method::GET, WATCH, Canned::status(503));
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert!(
        error
            .to_string()
            .contains("HTTP status server error (503 Service Unavailable) for url (")
    );
}

/// Truncated transcript XML fails with `UnexpectedData::InvalidXml`.
#[tokio::test]
async fn invalid_transcript_xml_is_unexpected_data() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        TIMEDTEXT,
        Canned::ok("<transcript>".to_owned()),
    );
    let result = api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await;
    assert!(
        matches!(
            result,
            Err(Error::UnexpectedData(UnexpectedData::InvalidXml(_)))
        ),
        "{result:?}"
    );
}

/// Language codes match regional variants, such as `de-AT` to `de`, only after
/// `match_language_variants(true)`.
#[tokio::test]
async fn language_variants_match_only_when_asked() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, regional_captions());
    let exact = api(&fake).list("abc").await.unwrap();
    let code = |found: Result<&Transcript, Error>| found.map(|t| t.language_code.clone()).ok();

    // Exact by default: "en" is the generated track, "de-AT" nothing.
    assert_eq!(code(exact.find_transcript(&["en"])), Some("en".to_owned()));
    assert_eq!(code(exact.find_transcript(&["de-AT"])), None);
    assert_eq!(code(exact.find_manually_created_transcript(&["en"])), None);

    let variants = exact.match_language_variants(true);
    // An exact match still wins, across both kinds.
    assert_eq!(
        code(variants.find_transcript(&["en"])),
        Some("en".to_owned())
    );
    // Each code, variants included, before the next one.
    assert_eq!(
        code(variants.find_transcript(&["fr", "de-AT", "en"])),
        Some("de".to_owned())
    );
    assert_eq!(
        code(variants.find_manually_created_transcript(&["EN"])),
        Some("en-US".to_owned())
    );
    assert_eq!(code(variants.find_generated_transcript(&["fr"])), None);
}

/// `match_language_variants(true)` on the builder lets `fetch` find `de` for `de-AT`.
#[tokio::test]
async fn the_builder_turns_language_variants_on() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, regional_captions());
    let client: Arc<dyn HttpClient> = Arc::<FakeYouTube>::clone(&fake);
    let api = YouTubeTranscriptApi::builder()
        .http_client(client)
        .match_language_variants(true)
        .build()
        .unwrap();
    let transcript = api.fetch("abc", &["de-AT"], false).await.unwrap();
    assert_eq!(transcript.language_code, "de");
}

/// Caption data with a missing or wrong-typed field fails with `UnexpectedData::Field` naming that
/// field.
#[tokio::test]
async fn malformed_caption_data_is_unexpected_data() {
    let track = serde_json::json!({
        "baseUrl": "https://www.youtube.com/api/timedtext?v=abc",
        "name": {"runs": [{"text": "English"}]},
        "languageCode": "en",
    });
    let without = |key: &str| {
        let mut partial = track.clone();
        partial.as_object_mut().map(|t| t.remove(key));
        serde_json::json!({ "captionTracks": [partial] })
    };
    let cases = [
        (
            serde_json::json!({"captionTracks": "none"}),
            "captionTracks",
        ),
        (serde_json::json!({"captionTracks": null}), "captionTracks"),
        (without("baseUrl"), "baseUrl"),
        (without("name"), "name"),
        (without("languageCode"), "languageCode"),
        (
            serde_json::json!({
                "captionTracks": [track],
                "translationLanguages": [{"languageCode": "de"}],
            }),
            "languageName",
        ),
        (
            serde_json::json!({
                "captionTracks": [track],
                "translationLanguages": [{"languageName": {"runs": [{"text": "German"}]}}],
            }),
            "languageCode",
        ),
    ];
    for (captions, field) in cases {
        let fake = FakeYouTube::new();
        fake.replace(&Method::POST, PLAYER, player_with_captions(&captions));
        let result = api(&fake).list("abc").await;
        assert!(
            matches!(
                &result,
                Err(Error::UnexpectedData(UnexpectedData::Field { field: f })) if *f == field
            ),
            "{field}: {result:?}"
        );
    }
}

/// Caption data with only the required fields lists one translatable transcript.
#[tokio::test]
async fn well_formed_minimal_caption_data_lists() {
    let fake = FakeYouTube::new();
    let captions = serde_json::json!({
        "captionTracks": [{
            "baseUrl": "https://www.youtube.com/api/timedtext?v=abc",
            "name": {"runs": [{"text": "English"}]},
            "languageCode": "en",
            "isTranslatable": true,
        }],
        "translationLanguages": [{
            "languageName": {"runs": [{"text": "German"}]},
            "languageCode": "de",
        }],
    });
    fake.replace(&Method::POST, PLAYER, player_with_captions(&captions));
    let list = api(&fake).list("abc").await.unwrap();
    let codes: Vec<&str> = list.iter().map(|t| t.language_code.as_str()).collect();
    assert_eq!(codes, ["en"]);
    assert_eq!(
        list.iter().next().map(Transcript::is_translatable),
        Some(true)
    );
}
