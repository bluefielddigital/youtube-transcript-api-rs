use youtube_transcript_api::http::Method;
use youtube_transcript_api::{DEFAULT_LANGUAGES, Error, Reason, UnexpectedData};

use crate::common::{Canned, FakeYouTube, PLAYER, TIMEDTEXT, WATCH, golden};
use crate::{api, not_retrieved};

/// A player response with `status` as its `playabilityStatus`.
fn player_with_status(status: &serde_json::Value) -> Canned {
    Canned::ok(serde_json::json!({ "playabilityStatus": status }).to_string())
}

/// Reference `start` values, including digit-group underscores.
#[tokio::test]
async fn start_times_parse_like_reference() {
    let cases: Vec<(String, Option<String>)> =
        serde_json::from_str(&golden("numbers.json")).unwrap();
    for (number, expected) in cases {
        let fake = FakeYouTube::new();
        let xml = format!(r#"<transcript><text start="{number}" dur="1">x</text></transcript>"#);
        fake.replace(&Method::GET, TIMEDTEXT, Canned::ok(xml));
        let start = api(&fake)
            .fetch("abc", &DEFAULT_LANGUAGES, false)
            .await
            .map(|transcript| transcript.snippets[0].start);
        match expected.as_deref() {
            None => assert!(start.is_err(), "{number:?}: {start:?}"),
            Some("nan") => assert!(start.unwrap().is_nan(), "{number:?}"),
            Some(value) => assert_eq!(start.unwrap(), value.parse::<f64>().unwrap(), "{number:?}"),
        }
    }
}

/// A 404 from the `timedtext` endpoint becomes a request failure that names the status and URL.
#[tokio::test]
async fn client_error_status_is_youtube_request_failed() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, TIMEDTEXT, Canned::status(404));
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert!(
        error
            .to_string()
            .contains("HTTP status client error (404 Not Found) for url (")
    );
}

/// A `null` playability status counts as playable, so a video without captions reports
/// `TranscriptsDisabled`.
#[tokio::test]
async fn null_playability_status_counts_as_playable() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::POST,
        PLAYER,
        player_with_status(&serde_json::json!({"status": null})),
    );
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(error.reason, Reason::TranscriptsDisabled);
}

/// `LOGIN_REQUIRED` with an unrecognized reason becomes `VideoUnplayable` carrying that reason.
#[tokio::test]
async fn login_required_for_another_reason_is_unplayable() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::POST,
        PLAYER,
        player_with_status(&serde_json::json!({"status": "LOGIN_REQUIRED", "reason": "Other"})),
    );
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(
        error.reason,
        Reason::VideoUnplayable {
            reason: Some("Other".to_owned()),
            sub_reasons: Vec::new(),
        }
    );
}

/// An unplayable video without a reason shows `No reason specified!` in its error message.
#[tokio::test]
async fn unplayable_without_reason() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::POST,
        PLAYER,
        player_with_status(&serde_json::json!({"status": "ERROR"})),
    );
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert!(
        error
            .to_string()
            .contains("the following reason: No reason specified!\n\n")
    );
}

/// Only `"OK"` is playable, so a non-string status is a failure.
#[tokio::test]
async fn non_string_playability_status_is_unplayable() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::POST,
        PLAYER,
        Canned::ok(r#"{"playabilityStatus": {"status": 5}}"#.to_owned()),
    );
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(
        error.reason,
        Reason::VideoUnplayable {
            reason: None,
            sub_reasons: Vec::new()
        }
    );
}

/// A player response without `playabilityStatus` fails with `UnexpectedData::Field` naming that
/// field.
#[tokio::test]
async fn missing_playability_status_is_unexpected_data() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, Canned::ok("{}".to_owned()));
    let result = api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await;
    assert!(matches!(
        result,
        Err(Error::UnexpectedData(UnexpectedData::Field {
            field: "playabilityStatus"
        }))
    ));
}

/// A player response that isn't JSON fails with `InvalidJson`, and the parser's message becomes the
/// error's source.
#[tokio::test]
async fn invalid_json_is_unexpected_data() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, Canned::ok("not json".to_owned()));
    let error = api(&fake)
        .fetch("abc", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::UnexpectedData(UnexpectedData::InvalidJson(_))
    ));
    assert_eq!(error.to_string(), "YouTube returned invalid JSON");
    let source = std::error::Error::source(&error).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("expected ident at line 1 column 2"));
}

/// A watch page without the API key fails with `YouTubeDataUnparsable`.
#[tokio::test]
async fn missing_api_key_is_data_unparsable() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::ok("<html></html>".to_owned()));
    let error = not_retrieved(api(&fake).fetch("abc", &DEFAULT_LANGUAGES, false).await);
    assert_eq!(error.reason, Reason::YouTubeDataUnparsable);
}

/// When two tracks share a language code, the last one wins but keeps the first one's position
/// in the list.
#[tokio::test]
async fn duplicate_language_codes_keep_the_first_position_like_a_dict() {
    let fake = FakeYouTube::new();
    let track = |name: &str, url: &str| {
        serde_json::json!({
            "baseUrl": url, "name": {"runs": [{"text": name}]}, "languageCode": "en"
        })
    };
    let player = serde_json::json!({
        "playabilityStatus": {"status": "OK"},
        "captions": {"playerCaptionsTracklistRenderer": {"captionTracks": [
            track("First", "https://www.youtube.com/api/timedtext?a=1"),
            {"baseUrl": "u", "name": {"runs": [{"text": "German"}]}, "languageCode": "de"},
            track("Second", "https://www.youtube.com/api/timedtext?a=2"),
        ]}},
    });
    fake.replace(&Method::POST, PLAYER, Canned::ok(player.to_string()));

    let list = api(&fake).list("abc").await.unwrap();

    let names: Vec<&str> = list.iter().map(|t| t.language.as_str()).collect();
    assert_eq!(names, ["Second", "German"]);
    assert_eq!((&list).into_iter().count(), 2);
    assert!(
        list.to_string()
            .contains("(GENERATED)\nNone\n\n(TRANSLATION LANGUAGES)\nNone")
    );
}
