use youtube_transcript_api::VideoDetails;
use youtube_transcript_api::http::Method;

use crate::api;
use crate::common::{Canned, FakeYouTube, PLAYER, WATCH, fixture};

/// A watch page with the API key and no embedded player response.
const BARE_WATCH_PAGE: &str = r#"<html>"INNERTUBE_API_KEY": "key"</html>"#;

/// `list` reads the video details from the reference watch page and player response.
#[tokio::test]
async fn list_has_the_video_details() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let details = list.video_details().unwrap();
    assert_eq!(details.video_id, "GJLlxj_dtq8");
    assert_eq!(
        details.title.as_deref(),
        Some("Surface Go Review - It\u{2019}s Awesome")
    );
    assert_eq!(details.author.as_deref(), Some("Dave2D"));
    assert_eq!(
        details.channel_id.as_deref(),
        Some("UCVYamHliCI9rw1tHR1xbkfw")
    );
    assert_eq!(
        details.channel_url.as_deref(),
        Some("http://www.youtube.com/@Dave2D")
    );
    assert_eq!(details.length_seconds, Some(316));
    assert_eq!(details.view_count, Some(1_856_109));
    assert_eq!(details.like_count, Some(43_864));
    assert!(
        details
            .description
            .as_deref()
            .unwrap()
            .starts_with("Dave2D review of the Microsoft Surface Go.")
    );
    assert_eq!(details.keywords.len(), 15);
    assert_eq!(details.keywords[..2], ["Dave2D", "Review"]);
    assert_eq!(details.category.as_deref(), Some("Science & Technology"));
    assert_eq!(
        details.publish_date.as_deref(),
        Some("2018-08-02T06:00:08-07:00")
    );
    assert_eq!(details.upload_date, details.publish_date);
    assert_eq!(details.thumbnails.len(), 4);
    let first = &details.thumbnails[0];
    assert_eq!(
        (first.url.as_str(), first.width, first.height),
        (
            "https://i.ytimg.com/vi/GJLlxj_dtq8/default.jpg",
            Some(120),
            Some(90)
        )
    );
    assert_eq!(details.available_countries.len(), 249);
    assert!(!details.is_live_content);
    assert!(!details.is_private);
    assert_eq!(details.is_unlisted, Some(false));
    assert_eq!(details.is_family_safe, Some(true));
}

/// Video details serialize to JSON and read back unchanged.
#[tokio::test]
async fn video_details_survive_a_serde_round_trip() {
    let fake = FakeYouTube::new();
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let details = list.video_details().unwrap();
    let json = serde_json::to_string(details).unwrap();
    assert!(json.contains(r#""length_seconds":316"#), "{json}");
    assert_eq!(
        &serde_json::from_str::<VideoDetails>(&json).unwrap(),
        details
    );
}

/// With no player response on the watch page, the details come from the player request, and
/// thumbnails without a URL drop out.
#[tokio::test]
async fn video_details_from_the_player_response_alone() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::ok(BARE_WATCH_PAGE.to_owned()));
    let mut player: serde_json::Value =
        serde_json::from_str(&fixture("youtube.innertube.json.static")).unwrap();
    player["videoDetails"]["thumbnail"]["thumbnails"] =
        serde_json::json!([{"width": 1}, {"url": "https://i.ytimg.com/x.jpg"}]);
    fake.replace(&Method::POST, PLAYER, Canned::ok(player.to_string()));
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let details = list.video_details().unwrap();
    assert_eq!(details.author.as_deref(), Some("Dave2D"));
    assert_eq!(details.category, None);
    assert_eq!(details.like_count, None);
    let thumbnails: Vec<_> = details
        .thumbnails
        .iter()
        .map(|t| (t.url.as_str(), t.width, t.height))
        .collect();
    assert_eq!(thumbnails, [("https://i.ytimg.com/x.jpg", None, None)]);
}

/// Without `videoDetails` in the player request, the details come from the watch page's
/// `microformat` data.
#[tokio::test]
async fn video_details_from_the_watch_page_alone() {
    let fake = FakeYouTube::new();
    let page = format!(
        "{BARE_WATCH_PAGE}<script>var ytInitialPlayerResponse = {};</script>",
        serde_json::json!({"microformat": {"playerMicroformatRenderer": {
            "title": {"runs": [{"text": "Part one, "}, {"text": "part two"}]},
            "description": {"simpleText": "About it"},
            "ownerChannelName": "Channel",
            "externalChannelId": "UC1",
        }}})
    );
    fake.replace(&Method::GET, WATCH, Canned::ok(page));
    let mut player: serde_json::Value =
        serde_json::from_str(&fixture("youtube.innertube.json.static")).unwrap();
    player.as_object_mut().map(|p| p.remove("videoDetails"));
    fake.replace(&Method::POST, PLAYER, Canned::ok(player.to_string()));
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    let details = list.video_details().unwrap();
    assert_eq!(details.video_id, "GJLlxj_dtq8");
    assert_eq!(details.title.as_deref(), Some("Part one, part two"));
    assert_eq!(details.description.as_deref(), Some("About it"));
    assert_eq!(details.author.as_deref(), Some("Channel"));
    assert_eq!(details.channel_id.as_deref(), Some("UC1"));
    assert_eq!(details.length_seconds, None);
}

/// With no details on the watch page or from the player, `video_details` returns `None`.
#[tokio::test]
async fn no_video_details_when_youtube_sends_none() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, WATCH, Canned::ok(BARE_WATCH_PAGE.to_owned()));
    let mut player: serde_json::Value =
        serde_json::from_str(&fixture("youtube.innertube.json.static")).unwrap();
    player.as_object_mut().map(|p| p.remove("videoDetails"));
    fake.replace(&Method::POST, PLAYER, Canned::ok(player.to_string()));
    let list = api(&fake).list("GJLlxj_dtq8").await.unwrap();
    assert_eq!(list.video_details(), None);
}
