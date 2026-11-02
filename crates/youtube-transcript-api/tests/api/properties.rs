use proptest::prelude::*;
use serde_json::{Map, Value};
use youtube_transcript_api::http::Method;
use youtube_transcript_api::video_id;

use youtube_transcript_api::DEFAULT_LANGUAGES;

use crate::api;
use crate::common::{Canned, FakeYouTube, PLAYER, TIMEDTEXT};

/// Field names YouTube's player response uses, so generated objects
/// reach the parser's fields rather than only unknown keys.
const KEYS: &[&str] = &[
    "playabilityStatus",
    "status",
    "reason",
    "errorScreen",
    "playerErrorMessageRenderer",
    "subreason",
    "runs",
    "simpleText",
    "text",
    "captions",
    "playerCaptionsTracklistRenderer",
    "captionTracks",
    "translationLanguages",
    "baseUrl",
    "name",
    "languageCode",
    "languageName",
    "kind",
    "isTranslatable",
    "videoDetails",
    "videoId",
    "title",
    "lengthSeconds",
    "viewCount",
    "keywords",
    "thumbnail",
    "thumbnails",
    "url",
    "width",
    "height",
    "isLiveContent",
    "microformat",
    "playerMicroformatRenderer",
];

/// Generates JSON values nested up to five levels deep, with object keys taken from `KEYS`.
fn json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_map(Value::from),
        prop_oneof![Just("OK".to_owned()), "[ -~]{0,12}".prop_map(String::from)]
            .prop_map(Value::from),
    ];
    leaf.prop_recursive(5, 96, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::vec((prop::sample::select(KEYS), inner), 0..8).prop_map(|fields| {
                Value::Object(
                    fields
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value))
                        .collect::<Map<_, _>>(),
                )
            }),
        ]
    })
}

/// Runs `future` to completion on a new single-threaded runtime.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap_or_else(|e| unreachable!("{e}"))
        .block_on(future)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Any player response lists transcripts or fails with an error.
    #[test]
    fn any_player_response_is_handled(player in json()) {
        let fake = FakeYouTube::new();
        fake.replace(&Method::POST, PLAYER, Canned::ok(player.to_string()));
        let listed = block_on(api(&fake).list("GJLlxj_dtq8"));
        if let Ok(list) = listed {
            prop_assert!(list.iter().all(|t| t.video_id == "GJLlxj_dtq8"));
        }
    }

    /// Any timedtext body, XML or not, either parses or fails with an error.
    #[test]
    fn any_timedtext_body_is_handled(body in any::<String>(), preserve in any::<bool>()) {
        let fake = FakeYouTube::new();
        fake.replace(&Method::GET, TIMEDTEXT, Canned::ok(body));
        let fetched = block_on(api(&fake).fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, preserve));
        if let Ok(transcript) = fetched {
            prop_assert!(transcript.iter().all(|s| s.start.is_finite()));
        }
    }

    /// Captions survive the XML round trip: text, start, and duration.
    #[test]
    fn generated_transcripts_round_trip(
        captions in prop::collection::vec(
            ("[A-Za-z0-9 ,.!?']{1,30}", 0_u32..10_000_000, 0_u32..100_000),
            0..20,
        )
    ) {
        let millis = |ms: u32| format!("{}.{:03}", ms / 1000, ms % 1000);
        let texts = captions
            .iter()
            .map(|(text, start, dur)| {
                format!(r#"<text start="{}" dur="{}">{text}</text>"#, millis(*start), millis(*dur))
            })
            .collect::<Vec<_>>()
            .concat();
        let fake = FakeYouTube::new();
        fake.replace(
            &Method::GET,
            TIMEDTEXT,
            Canned::ok(format!(r#"<?xml version="1.0" encoding="utf-8" ?><transcript>{texts}</transcript>"#)),
        );
        let transcript = block_on(api(&fake).fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false))
            .unwrap();
        prop_assert_eq!(transcript.len(), captions.len());
        for (snippet, (text, start, dur)) in transcript.iter().zip(&captions) {
            prop_assert_eq!(&snippet.text, text);
            // Exactly: both sides round the same decimal to the nearest f64.
            prop_assert_eq!(snippet.start.to_bits(), (f64::from(*start) / 1000.0).to_bits());
            prop_assert_eq!(snippet.duration.to_bits(), (f64::from(*dur) / 1000.0).to_bits());
        }
    }

    /// Every URL form gives back the ID; a bare ID comes back unchanged.
    #[test]
    fn video_ids_come_out_of_every_url_form(id in "[A-Za-z0-9_-]{11}") {
        for url in [
            format!("https://www.youtube.com/watch?v={id}"),
            format!("m.youtube.com/watch?v={id}&list=PL1&index=2"),
            format!("https://youtu.be/{id}?t=42"),
            format!("youtube.com/shorts/{id}"),
            format!("https://www.youtube.com/embed/{id}"),
            format!("https://www.youtube.com/live/{id}?feature=share"),
            format!("https://www.youtube-nocookie.com/embed/{id}"),
        ] {
            prop_assert_eq!(video_id(&url), id.as_str(), "{}", url);
        }
        prop_assert_eq!(video_id(&id), id.as_str());
    }

    /// Any input gives an ID or itself.
    #[test]
    fn any_input_gives_an_id_or_itself(input in any::<String>()) {
        let id = video_id(&input);
        prop_assert!(id == input.as_str() || input.contains(&*id));
    }
}
