use youtube_transcript_api::http::Method;
use youtube_transcript_api::{
    DEFAULT_LANGUAGES, Error, FetchedTranscript, RawFormat, Reason, Transcript, VideoDetails,
    YouTubeTranscriptApi,
};

use crate::common::{Canned, FakeYouTube, TIMEDTEXT, WATCH, ref_transcript};
use crate::{api, not_retrieved, replace_player};

/// Compiles only if `T` is `Send` and `Sync`.
const fn send_sync<T: Send + Sync>() {}

/// Compiles only if the value's type is `Send`.
const fn send<T: Send>(_: &T) {}

/// The client, its results and the futures it returns can cross threads, so
/// they work on a multi-threaded runtime and in `tokio::spawn`.
#[tokio::test]
async fn client_and_futures_are_send() {
    send_sync::<YouTubeTranscriptApi>();
    send_sync::<Transcript>();
    send_sync::<FetchedTranscript>();
    send_sync::<VideoDetails>();
    send_sync::<Error>();
    let fake = FakeYouTube::new();
    let api = api(&fake);
    send(&api.fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false));
    let listing = api.list("GJLlxj_dtq8");
    send(&listing);
    let list = listing.await.unwrap();
    let transcript = list.find_transcript(&["en"]).unwrap();
    send(&transcript.fetch(false));
    send(&transcript.fetch_raw(RawFormat::Srv3));
}

/// `fetch` returns the reference transcript for the reference video.
#[tokio::test]
async fn fetch() {
    let fake = FakeYouTube::new();
    let transcript = api(&fake)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();
    assert_eq!(transcript, ref_transcript());
}

/// With formatting kept, `fetch` leaves the `<i>` tags in the second snippet's text.
#[tokio::test]
async fn fetch_formatted() {
    let fake = FakeYouTube::new();
    let transcript = api(&fake)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, true)
        .await
        .unwrap();
    let mut expected = ref_transcript();
    expected.snippets[1].text = "this is <i>not</i> the original transcript".to_owned();
    assert_eq!(transcript, expected);
}

/// The player response YouTube sends to an altered user agent still gives the reference transcript.
#[tokio::test]
async fn fetch_with_altered_user_agent() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_altered_user_agent.innertube.json.static");
    let transcript = api(&fake)
        .fetch("GJLlxj_dtq8", &DEFAULT_LANGUAGES, false)
        .await
        .unwrap();
    assert_eq!(transcript, ref_transcript());
}

/// `list` returns every transcript in YouTube's order, with `en` once manual and once generated.
#[tokio::test]
async fn list() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let codes: Vec<&str> = list.iter().map(|t| t.language_code.as_str()).collect();
    assert_eq!(
        codes,
        ["zh", "cs", "en", "de", "hi", "ja", "ko", "es", "en"]
    );
}

/// `find_manually_created_transcript` finds the manually created `cs` transcript.
#[tokio::test]
async fn list_find_manually_created() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let transcript = list.find_manually_created_transcript(&["cs"]).unwrap();
    assert!(!transcript.is_generated);
}

/// `find_generated_transcript` finds no generated `cs` transcript but finds the generated `en` one.
#[tokio::test]
async fn list_find_generated() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let error = not_retrieved(list.find_generated_transcript(&["cs"]));
    assert!(matches!(error.reason, Reason::NoTranscriptFound { .. }));
    assert!(
        list.find_generated_transcript(&["en"])
            .unwrap()
            .is_generated
    );
}

/// Watch, `youtu.be` and Shorts URLs work as video IDs and load the watch page for the bare ID.
#[tokio::test]
async fn video_urls_are_accepted_as_ids() {
    for url in [
        "https://www.youtube.com/watch?v=GJLlxj_dtq8&t=42s",
        "https://youtu.be/GJLlxj_dtq8",
        "youtube.com/shorts/GJLlxj_dtq8",
    ] {
        let fake = FakeYouTube::new();
        let transcript = api(&fake)
            .fetch(url, &DEFAULT_LANGUAGES, false)
            .await
            .unwrap();
        assert_eq!(transcript.video_id, "GJLlxj_dtq8", "{url}");
        let watched: Vec<String> = fake
            .calls()
            .iter()
            .filter(|c| c.request.url.starts_with(WATCH))
            .flat_map(|c| c.query("v"))
            .collect();
        assert_eq!(watched, ["GJLlxj_dtq8"], "{url}");
    }
}

/// A URL that isn't a video link goes through as an ID, and YouTube's answer
/// for it becomes `InvalidVideoId`.
#[tokio::test]
async fn list_url_as_video_id() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_video_unavailable.innertube.json.static");
    let error = not_retrieved(
        api(&fake)
            .list("https://www.youtube.com/youtubei/v1/player?v=GJLlxj_dtq8")
            .await,
    );
    assert_eq!(error.reason, Reason::InvalidVideoId);
}

/// `fetch_raw` asks for each raw format through `fmt` and returns the body unchanged.
#[tokio::test]
async fn raw_formats_are_requested_and_returned_as_sent() {
    for format in [
        RawFormat::Srv1,
        RawFormat::Srv2,
        RawFormat::Srv3,
        RawFormat::Json3,
        RawFormat::Ttml,
        RawFormat::Vtt,
    ] {
        let fake = FakeYouTube::new();
        let body = format!("<raw {}>", format.name());
        fake.replace(&Method::GET, TIMEDTEXT, Canned::ok(body.clone()));
        let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
        let raw = list
            .find_transcript(&["en"])
            .unwrap()
            .fetch_raw(format)
            .await;
        assert_eq!(raw.unwrap(), body);
        let call = fake.calls().pop().unwrap();
        assert_eq!(call.query("fmt"), [format.name()]);
    }
}

/// A raw response with only whitespace fails with `YouTubeDataUnparsable`.
#[tokio::test]
async fn an_empty_raw_response_is_an_error() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, TIMEDTEXT, Canned::ok(" \n".to_owned()));
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let error = not_retrieved(
        list.find_transcript(&["en"])
            .unwrap()
            .fetch_raw(RawFormat::Ttml)
            .await,
    );
    assert_eq!(error.reason, Reason::YouTubeDataUnparsable);
}

/// `fetch_raw` on a translation sends both `tlang` and `fmt`.
#[tokio::test]
async fn raw_formats_of_a_translation() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let german = list
        .find_transcript(&["en"])
        .unwrap()
        .translate("de")
        .unwrap();
    german.fetch_raw(RawFormat::Json3).await.unwrap();
    let call = fake.calls().pop().unwrap();
    assert_eq!(call.query("tlang"), ["de"]);
    assert_eq!(call.query("fmt"), ["json3"]);
}

/// `fetch_raw` fails with `PoTokenRequired` too when the player response demands a proof-of-origin
/// token.
#[tokio::test]
async fn raw_formats_need_no_po_token_either() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_po_token_required.innertube.json.static");
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let error = not_retrieved(
        list.find_transcript(&["en"])
            .unwrap()
            .fetch_raw(RawFormat::Srv3)
            .await,
    );
    assert_eq!(error.reason, Reason::PoTokenRequired);
}

/// `translate` gives a transcript in the target language whose URL carries `tlang`.
#[tokio::test]
async fn translate_transcript() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let translated = list
        .find_transcript(&["en"])
        .unwrap()
        .translate("ar")
        .unwrap();
    assert_eq!(translated.language_code, "ar");
    assert!(translated.url().contains("&tlang=ar"));
}

/// `translate` to a language YouTube doesn't offer fails with `TranslationLanguageNotAvailable`.
#[tokio::test]
async fn translate_transcript_translation_language_not_available() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let error = not_retrieved(list.find_transcript(&["en"]).unwrap().translate("xyz"));
    assert_eq!(error.reason, Reason::TranslationLanguageNotAvailable);
}

/// `translate` on a transcript without translation languages fails with `NotTranslatable`.
#[tokio::test]
async fn translate_transcript_not_translatable() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let mut transcript = list.find_transcript(&["en"]).unwrap().clone();
    transcript.translation_languages.clear();
    let error = not_retrieved(transcript.translate("af"));
    assert_eq!(error.reason, Reason::NotTranslatable);
}

/// `fetch` with `de` then `en` fetches `de`, the first code the video has.
#[tokio::test]
async fn fetch_correct_language_is_used() {
    let fake = FakeYouTube::new();
    api(&fake)
        .fetch("GJLlxj_dtq8", &["de", "en"], false)
        .await
        .unwrap();
    let last = fake.calls().pop().unwrap();
    assert_eq!(last.query("lang"), ["de"]);
}

/// `fetch` falls back to `en` when the video has no `de` transcript.
#[tokio::test]
async fn fetch_fallback_language_is_used() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_ww1_nl_en.innertube.json.static");
    api(&fake)
        .fetch("F1xioXWb8CY", &["de", "en"], false)
        .await
        .unwrap();
    let last = fake.calls().pop().unwrap();
    assert_eq!(last.query("lang"), ["en"]);
}

/// An ID with reserved characters is percent-encoded in the watch URL, so
/// it can't add query parameters or cut the URL short.
#[tokio::test]
async fn odd_video_ids_are_percent_encoded_in_the_watch_url() {
    let fake = FakeYouTube::new();
    let _ = api(&fake).list("abc&hl=de#x y").await;
    let watch = fake
        .calls()
        .into_iter()
        .find(|call| call.request.url.starts_with(WATCH))
        .unwrap_or_else(|| unreachable!("no watch request"));
    assert_eq!(
        watch.request.url,
        format!("{WATCH}?v=abc%26hl%3Dde%23x%20y")
    );
    assert_eq!(watch.query("hl"), Vec::<String>::new());
}
