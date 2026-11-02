//! The CLI run over a fake YouTube. `tests.rs` covers argument parsing.

use std::sync::Arc;

use clap::Parser;
use youtube_transcript_api::YouTubeTranscriptApi;
use youtube_transcript_api::http::{HttpClient, Method};

use super::fake::{FakeYouTube, PLAYER, TIMEDTEXT, WATCH, fixture, golden, query};
use super::{Args, Output, run};

/// A client that sends every request to `fake`.
fn api(fake: &Arc<FakeYouTube>) -> YouTubeTranscriptApi {
    let client: Arc<dyn HttpClient> = Arc::<FakeYouTube>::clone(fake);
    YouTubeTranscriptApi::builder()
        .http_client(client)
        .build()
        .unwrap()
}

/// Makes `fake` answer player requests with the fixture `name`.
fn replace_player(fake: &FakeYouTube, name: &str) {
    fake.replace(&Method::POST, PLAYER, fixture(name));
}

/// Manual `en-US` and `de`, generated `en`.
fn regional_captions() -> String {
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
    serde_json::json!({
        "playabilityStatus": {"status": "OK"},
        "captions": {"playerCaptionsTracklistRenderer": {
            "captionTracks": [track("en-US", None), track("en", Some("asr")), track("de", None)],
        }},
    })
    .to_string()
}

/// Parses `line`, split on spaces, into CLI arguments.
fn cli(line: &str) -> Args {
    Args::try_parse_from(std::iter::once("youtube-transcript-api").chain(line.split(' '))).unwrap()
}

/// Runs the CLI with the arguments in `line` against `fake`.
async fn run_cli(fake: &Arc<FakeYouTube>, line: &str) -> Output {
    run(&cli(line), &api(fake)).await
}

/// Two videos with `--format json` print a JSON array of two transcripts and load both watch pages
/// in order.
#[tokio::test]
async fn cli_json_output_for_several_videos() {
    let fake = FakeYouTube::new();
    let output = run_cli(&fake, "v1 v2 --languages de en --format json").await;
    assert_eq!(output.errors, Vec::<String>::new());
    let json: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(json.as_array().map(Vec::len), Some(2));
    let watched: Vec<String> = fake
        .calls()
        .iter()
        .filter(|c| c.url.starts_with(WATCH))
        .flat_map(|c| query(c, "v"))
        .collect();
    assert_eq!(watched, ["v1", "v2"]);
}

/// `--raw json3` asks YouTube for `json3` and prints each video's body as sent, with a blank line
/// between them.
#[tokio::test]
async fn cli_raw_prints_youtubes_format_per_video() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::GET, TIMEDTEXT, "{\"events\": []}".to_owned());
    let output = run_cli(&fake, "v1 v2 --raw json3").await;
    assert_eq!(output.errors, Vec::<String>::new());
    assert_eq!(output.stdout, "{\"events\": []}\n\n{\"events\": []}");
    assert!(fake.calls().iter().any(|c| query(c, "fmt") == ["json3"]));
}

/// Text output drops HTML tags like `<i>` unless the CLI gets `--preserve-formatting`.
#[tokio::test]
async fn cli_preserve_formatting_keeps_tags() {
    let xml = "<transcript><text start=\"0\" dur=\"1\">&lt;i&gt;hi&lt;/i&gt;</text></transcript>";
    for (flag, expected) in [("", "hi"), (" --preserve-formatting", "<i>hi</i>")] {
        let fake = FakeYouTube::new();
        fake.replace(&Method::GET, TIMEDTEXT, xml.to_owned());
        let output = run_cli(&fake, &format!("v1 --format text{flag}")).await;
        assert_eq!(output.stdout, expected, "{flag}");
    }
}

/// `--languages de-AT` finds the `de` transcript with `--match-language-variants` and fails without
/// it.
#[tokio::test]
async fn cli_match_language_variants() {
    for (flag, expected) in [("", None), (" --match-language-variants", Some("de"))] {
        let fake = FakeYouTube::new();
        fake.replace(&Method::POST, PLAYER, regional_captions());
        let output = run_cli(&fake, &format!("abc --languages de-AT --format json{flag}")).await;
        let fetched_lang: Vec<String> = fake
            .calls()
            .iter()
            .filter(|c| c.url.starts_with(TIMEDTEXT))
            .flat_map(|c| query(c, "lang"))
            .collect();
        assert_eq!(fetched_lang.first().map(String::as_str), expected, "{flag}");
        assert_eq!(output.errors.is_empty(), expected.is_some(), "{flag}");
    }
}

/// `--format text` prints the same output as the reference text formatter.
#[tokio::test]
async fn cli_text_output_matches_the_text_formatter() {
    let fake = FakeYouTube::new();
    let output = run_cli(&fake, "GJLlxj_dtq8 --format text").await;
    assert_eq!(output.stdout, golden("formatters/text.txt"));
}

/// An unavailable video prints nothing and adds its error message to `errors`.
#[tokio::test]
async fn cli_failing_video_goes_to_errors() {
    let fake = FakeYouTube::new();
    replace_player(&fake, "youtube_video_unavailable.innertube.json.static");
    let output = run_cli(&fake, "v1 --languages de en").await;
    assert_eq!(output.stdout, "");
    assert_eq!(output.errors.len(), 1);
    assert!(output.errors[0].contains("The video is no longer available"));
}

/// `--exclude-generated` fetches the manually created `en` transcript, not the generated one.
#[tokio::test]
async fn cli_exclude_generated_uses_manual_transcripts() {
    let fake = FakeYouTube::new();
    // `en` exists both ways in the fixture; the manual one has no `kind=asr`.
    run_cli(&fake, "v1 --exclude-generated").await;
    let last = fake.calls().pop().unwrap();
    assert!(!last.url.contains("kind=asr"));
}

/// `--exclude-manually-created` fetches the generated transcript.
#[tokio::test]
async fn cli_exclude_manually_created_uses_generated_transcripts() {
    let fake = FakeYouTube::new();
    run_cli(&fake, "v1 --exclude-manually-created").await;
    let last = fake.calls().pop().unwrap();
    assert!(last.url.contains("kind=asr"));
}

/// Excluding both kinds prints nothing, reports no error, and sends no request.
#[tokio::test]
async fn cli_excluding_both_prints_nothing() {
    let fake = FakeYouTube::new();
    let output = run_cli(&fake, "v1 --exclude-manually-created --exclude-generated").await;
    assert_eq!(output, Output::default());
    assert_eq!(fake.calls().len(), 0);
}

/// `--translate de` asks YouTube for a German translation through `tlang`.
#[tokio::test]
async fn cli_translate() {
    let fake = FakeYouTube::new();
    run_cli(&fake, "v1 --translate de").await;
    let last = fake.calls().pop().unwrap();
    assert_eq!(query(&last, "tlang"), ["de"]);
}

/// A language code that no transcript or translation has prints nothing and reports one error.
#[tokio::test]
async fn cli_missing_language_goes_to_errors() {
    for line in [
        "v1 --languages xx",
        "v1 --languages xx --exclude-generated",
        "v1 --languages xx --exclude-manually-created",
        "v1 --translate xx",
    ] {
        let fake = FakeYouTube::new();
        let output = run_cli(&fake, line).await;
        assert_eq!(output.stdout, "", "{line}");
        assert_eq!(output.errors.len(), 1, "{line}");
    }
}

/// An error message ends with its cause, such as the JSON parser's message after invalid JSON.
#[tokio::test]
async fn cli_errors_include_their_causes() {
    let fake = FakeYouTube::new();
    fake.replace(&Method::POST, PLAYER, "not json".to_owned());
    let output = run_cli(&fake, "v1").await;
    assert_eq!(
        output.errors,
        ["YouTube returned invalid JSON: expected ident at line 1 column 2"]
    );
}

/// `--list-transcripts` prints the reference listing once per video, with a blank line between
/// them.
#[tokio::test]
async fn cli_list_transcripts() {
    let fake = FakeYouTube::new();
    let output = run_cli(&fake, "--list-transcripts GJLlxj_dtq8 GJLlxj_dtq8").await;
    let one = golden("transcript_list.txt");
    assert_eq!(output.stdout, format!("{one}\n\n{one}"));
}
