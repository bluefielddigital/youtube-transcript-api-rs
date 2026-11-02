//! YouTube changes these responses without notice, so a field that's
//! missing, null, or of the wrong shape reads as `None` ([`lenient`]), and
//! the caller decides whether that's an error. Where "missing" and "wrong
//! shape" mean different things, the type keeps them apart ([`Status`],
//! [`Tracks`]).

use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A field that reads as `None` when missing, null, or of the wrong shape.
/// JSON that doesn't parse is still an error.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Value::deserialize(deserializer).map(|value| serde_json::from_value(value).ok())
}

/// Like [`lenient`], for a field holding an object. Only a JSON object
/// counts: serde would also build a struct from an array.
fn object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Value::deserialize(deserializer).map(|value| {
        Some(value)
            .filter(Value::is_object)
            .and_then(|object| serde_json::from_value(object).ok())
    })
}

/// A flag that YouTube may send as a bool, a number or a string: anything
/// non-empty and non-zero counts as true.
fn truthy<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Value::deserialize(deserializer).map(|value| is_truthy(&value))
}

/// Whether a JSON value is non-empty, non-zero and not `false` or null.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// A number YouTube sends as a string (`"lengthSeconds": "213"`), or as a
/// number.
fn number_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    Value::deserialize(deserializer).map(|value| match value {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    })
}

/// The InnerTube `player` response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerResponse {
    /// Whether the video plays, and why not.
    #[serde(default, deserialize_with = "object")]
    pub playability_status: Option<PlayabilityStatus>,
    /// The caption tracks.
    #[serde(default, deserialize_with = "object")]
    pub captions: Option<Captions>,
    /// The title, author, length, and other details.
    #[serde(default, deserialize_with = "object")]
    pub video_details: Option<VideoDetails>,
}

/// `playabilityStatus`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayabilityStatus {
    /// `OK`, or why the video doesn't play.
    #[serde(default)]
    pub status: Option<Status>,
    /// The reason YouTube shows.
    #[serde(default, deserialize_with = "lenient")]
    pub reason: Option<String>,
    /// Where the detail lines of an unplayable video live.
    #[serde(default, deserialize_with = "object")]
    pub error_screen: Option<ErrorScreen>,
}

/// `playabilityStatus.status`. A missing or null status counts as playable,
/// and any value other than `"OK"`, string or not, as a failure.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Status {
    /// A string status.
    Text(String),
    /// Any other JSON, a failure.
    Other(IgnoredAny),
}

/// `errorScreen`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorScreen {
    /// The renderer holding the detail lines.
    #[serde(default, deserialize_with = "object")]
    pub player_error_message_renderer: Option<ErrorMessage>,
}

/// `playerErrorMessageRenderer`.
#[derive(Debug, Deserialize)]
pub struct ErrorMessage {
    /// The detail lines.
    #[serde(default, deserialize_with = "object")]
    pub subreason: Option<Text>,
}

/// YouTube's formatted text: `{"runs": [{"text": …}, …]}` or
/// `{"simpleText": …}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    /// The pieces of the text.
    #[serde(default, deserialize_with = "lenient")]
    pub runs: Option<Vec<Run>>,
    /// The text in one piece.
    #[serde(default, deserialize_with = "lenient")]
    pub simple_text: Option<String>,
}

/// One piece of a [`Text`].
#[derive(Debug, Deserialize)]
pub struct Run {
    /// The piece's text.
    #[serde(default, deserialize_with = "lenient")]
    pub text: Option<String>,
}

impl Text {
    /// The first run's text.
    pub fn first_run(&self) -> Option<&str> {
        self.runs.as_ref()?.first()?.text.as_deref()
    }

    /// Every run's text, `""` for a run without one.
    pub fn run_texts(&self) -> Vec<String> {
        self.runs
            .iter()
            .flatten()
            .map(|run| run.text.clone().unwrap_or_default())
            .collect()
    }

    /// The whole text, from `simpleText` or the runs joined.
    pub fn plain(&self) -> String {
        self.simple_text
            .clone()
            .unwrap_or_else(|| self.run_texts().concat())
    }
}

/// `captions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Captions {
    /// The track list.
    #[serde(default, deserialize_with = "object")]
    pub player_captions_tracklist_renderer: Option<TracklistRenderer>,
}

/// `playerCaptionsTracklistRenderer`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TracklistRenderer {
    /// The transcripts.
    #[serde(default)]
    pub caption_tracks: Tracks,
    /// The languages YouTube can translate into.
    #[serde(default, deserialize_with = "lenient")]
    pub translation_languages: Option<Vec<TranslationLanguage>>,
}

/// `captionTracks`. A missing list means the video has subtitles turned
/// off; a malformed one is an `UnexpectedData` error.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
pub enum Tracks {
    /// A list.
    List(Vec<CaptionTrack>),
    /// Any other JSON.
    Other(IgnoredAny),
    /// No `captionTracks` at all.
    #[default]
    #[serde(skip)]
    Missing,
}

/// One transcript in `captionTracks`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionTrack {
    /// The timedtext URL.
    #[serde(default, deserialize_with = "lenient")]
    pub base_url: Option<String>,
    /// The language name.
    #[serde(default, deserialize_with = "object")]
    pub name: Option<Text>,
    /// The language code.
    #[serde(default, deserialize_with = "lenient")]
    pub language_code: Option<String>,
    /// `asr` for a generated transcript.
    #[serde(default, deserialize_with = "lenient")]
    pub kind: Option<String>,
    /// Whether YouTube can translate it.
    #[serde(default, deserialize_with = "truthy")]
    pub is_translatable: bool,
}

/// A language YouTube can translate into.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationLanguage {
    /// The language name.
    #[serde(default, deserialize_with = "object")]
    pub language_name: Option<Text>,
    /// The language code.
    #[serde(default, deserialize_with = "lenient")]
    pub language_code: Option<String>,
}

/// `videoDetails` in the player response.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetails {
    /// The video ID.
    #[serde(default, deserialize_with = "lenient")]
    pub video_id: Option<String>,
    /// The title.
    #[serde(default, deserialize_with = "lenient")]
    pub title: Option<String>,
    /// The channel's name.
    #[serde(default, deserialize_with = "lenient")]
    pub author: Option<String>,
    /// The channel ID.
    #[serde(default, deserialize_with = "lenient")]
    pub channel_id: Option<String>,
    /// The length in seconds.
    #[serde(default, deserialize_with = "number_string")]
    pub length_seconds: Option<u64>,
    /// The view count.
    #[serde(default, deserialize_with = "number_string")]
    pub view_count: Option<u64>,
    /// The tags.
    #[serde(default, deserialize_with = "lenient")]
    pub keywords: Option<Vec<String>>,
    /// The description.
    #[serde(default, deserialize_with = "lenient")]
    pub short_description: Option<String>,
    /// The thumbnails.
    #[serde(default, deserialize_with = "object")]
    pub thumbnail: Option<Thumbnails>,
    /// Whether it's a live stream, now or in the past.
    #[serde(default, deserialize_with = "truthy")]
    pub is_live_content: bool,
    /// Whether it's private.
    #[serde(default, deserialize_with = "truthy")]
    pub is_private: bool,
}

/// `thumbnail`.
#[derive(Debug, Deserialize)]
pub struct Thumbnails {
    /// The thumbnails, smallest first.
    #[serde(default, deserialize_with = "lenient")]
    pub thumbnails: Option<Vec<Thumbnail>>,
}

/// One thumbnail.
#[derive(Debug, Deserialize)]
pub struct Thumbnail {
    /// The image URL.
    #[serde(default, deserialize_with = "lenient")]
    pub url: Option<String>,
    /// The width in pixels.
    #[serde(default, deserialize_with = "lenient")]
    pub width: Option<u32>,
    /// The height in pixels.
    #[serde(default, deserialize_with = "lenient")]
    pub height: Option<u32>,
}

/// The `ytInitialPlayerResponse` embedded in the watch page, for its
/// `microformat`, which the InnerTube response doesn't have.
#[derive(Debug, Deserialize)]
pub struct WatchPlayerResponse {
    /// The `microformat`.
    #[serde(default, deserialize_with = "object")]
    pub microformat: Option<MicroformatWrapper>,
}

/// `microformat`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicroformatWrapper {
    /// The renderer with the details.
    #[serde(default, deserialize_with = "object")]
    pub player_microformat_renderer: Option<Microformat>,
}

/// `playerMicroformatRenderer`: details the InnerTube response lacks.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Microformat {
    /// The title.
    #[serde(default, deserialize_with = "object")]
    pub title: Option<Text>,
    /// The description.
    #[serde(default, deserialize_with = "object")]
    pub description: Option<Text>,
    /// The category, for example `Music`.
    #[serde(default, deserialize_with = "lenient")]
    pub category: Option<String>,
    /// The publish date.
    #[serde(default, deserialize_with = "lenient")]
    pub publish_date: Option<String>,
    /// The upload date.
    #[serde(default, deserialize_with = "lenient")]
    pub upload_date: Option<String>,
    /// The channel's name.
    #[serde(default, deserialize_with = "lenient")]
    pub owner_channel_name: Option<String>,
    /// The channel's URL.
    #[serde(default, deserialize_with = "lenient")]
    pub owner_profile_url: Option<String>,
    /// The channel ID.
    #[serde(default, deserialize_with = "lenient")]
    pub external_channel_id: Option<String>,
    /// Whether YouTube considers it family safe.
    #[serde(default, deserialize_with = "lenient")]
    pub is_family_safe: Option<bool>,
    /// Whether only people with the link can find it.
    #[serde(default, deserialize_with = "lenient")]
    pub is_unlisted: Option<bool>,
    /// The countries it's available in.
    #[serde(default, deserialize_with = "lenient")]
    pub available_countries: Option<Vec<String>>,
    /// The like count.
    #[serde(default, deserialize_with = "number_string")]
    pub like_count: Option<u64>,
}

/// A streaming deserializer reads the JSON and stops at the end of the
/// object, so no regular expression has to find where it ends.
pub fn watch_page_microformat(raw_html: &str) -> Option<Microformat> {
    const MARKER: &str = "var ytInitialPlayerResponse = ";
    let (_, json) = raw_html.split_once(MARKER)?;
    serde_json::Deserializer::from_str(json)
        .into_iter::<WatchPlayerResponse>()
        .next()?
        .ok()?
        .microformat?
        .player_microformat_renderer
}

/// Lenient parsing of YouTube's JSON.
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PlayerResponse, Text, VideoDetails, is_truthy, watch_page_microformat};

    /// Which JSON values count as true.
    #[test]
    fn truthiness() {
        for falsy in [
            json!(null),
            json!(false),
            json!(0),
            json!(""),
            json!([]),
            json!({}),
        ] {
            assert!(!is_truthy(&falsy), "{falsy}");
        }
        for truthy in [
            json!(true),
            json!(1),
            json!("x"),
            json!([0]),
            json!({"a": 0}),
        ] {
            assert!(is_truthy(&truthy), "{truthy}");
        }
    }

    /// Parses a player response.
    fn player(value: &serde_json::Value) -> PlayerResponse {
        serde_json::from_value(value.clone()).unwrap()
    }

    /// Fields of the wrong shape read as missing.
    #[test]
    fn wrong_shapes_read_as_missing() {
        let response = player(&json!({
            "playabilityStatus": [],
            "captions": [],
            "videoDetails": {"title": 5, "lengthSeconds": "abc", "viewCount": 7},
        }));
        assert!(response.playability_status.is_none());
        assert!(response.captions.is_none());
        let details = response.video_details.unwrap();
        assert_eq!(details.title, None);
        assert_eq!(details.length_seconds, None);
        assert_eq!(details.view_count, Some(7));
    }

    /// Numbers come as numbers or strings, nothing else.
    #[test]
    fn numbers_may_be_strings_or_numbers_only() {
        let details: VideoDetails =
            serde_json::from_value(json!({"lengthSeconds": true, "viewCount": "12"})).unwrap();
        assert_eq!(details.length_seconds, None);
        assert_eq!(details.view_count, Some(12));
    }

    /// `first_run` needs a first run with text.
    #[test]
    fn first_run_needs_a_run_with_text() {
        let text = |value: serde_json::Value| serde_json::from_value::<Text>(value).unwrap();
        assert_eq!(text(json!({})).first_run(), None);
        assert_eq!(text(json!({"runs": []})).first_run(), None);
        assert_eq!(text(json!({"runs": [{}]})).first_run(), None);
        assert_eq!(
            text(json!({"runs": [{"text": "a"}]})).first_run(),
            Some("a")
        );
    }

    /// The `microformat` comes from the JSON embedded in the watch page.
    #[test]
    fn microformat_comes_from_the_embedded_player_response() {
        let html = r#"<script>var ytInitialPlayerResponse = {"microformat":
            {"playerMicroformatRenderer": {"category": "Music",
             "title": {"simpleText": "A }; title"}, "likeCount": "12"}}};var x = 1;</script>"#;
        let microformat = watch_page_microformat(html).unwrap();
        assert_eq!(microformat.category.as_deref(), Some("Music"));
        assert_eq!(microformat.title.unwrap().plain(), "A }; title");
        assert_eq!(microformat.like_count, Some(12));
        for page in [
            "<html></html>",
            "var ytInitialPlayerResponse = ",
            "var ytInitialPlayerResponse = {oops",
            "var ytInitialPlayerResponse = {};",
        ] {
            assert!(watch_page_microformat(page).is_none(), "{page}");
        }
    }
}
