use youtube_transcript_api::formatters::{
    Formatter, FormatterKind, JsonFormatter, PrettyPrintFormatter, SrtFormatter, TextFormatter,
    TimestampedTextFormatter, WebVttFormatter,
};
use youtube_transcript_api::{FetchedTranscript, FetchedTranscriptSnippet};

use crate::common::{golden, ref_transcript};

/// Same as `LONG` in `scripts/record_golden.py`.
fn long_transcript() -> FetchedTranscript {
    FetchedTranscript::new(
        "long",
        "English",
        "en",
        true,
        vec![
            FetchedTranscriptSnippet::new("first", 0.0, 5.0),
            FetchedTranscriptSnippet::new("overlaps", 2.5, 1.25),
            FetchedTranscriptSnippet::new("later", 3725.5, 2.0),
        ],
    )
}

/// Checks `formatter` against the golden output for one transcript, the long one, and both
/// together.
fn assert_matches_reference(name: &str, formatter: &impl Formatter) {
    let both = [ref_transcript(), long_transcript()];
    assert_eq!(
        formatter.format_transcript(&both[0]),
        golden(&format!("formatters/{name}.txt")),
        "{name}"
    );
    assert_eq!(
        formatter.format_transcript(&both[1]),
        golden(&format!("formatters/{name}_long.txt")),
        "{name}_long"
    );
    assert_eq!(
        formatter.format_transcripts(&both),
        golden(&format!("formatters/{name}_many.txt")),
        "{name}_many"
    );
}

/// Text, SRT and WebVTT output match the golden files for one transcript and for many.
#[test]
fn text_srt_and_webvtt_match_reference() {
    assert_matches_reference("text", &TextFormatter);
    assert_matches_reference("srt", &SrtFormatter);
    assert_matches_reference("webvtt", &WebVttFormatter);
}

/// Reference cue timestamps, including the rounding that makes `0.9999996`
/// `00:00:00,1000`.
#[test]
fn timestamps_match_reference() {
    let cases: Vec<(f64, String, String)> =
        serde_json::from_str(&golden("timestamps.json")).unwrap();
    for (time, srt, webvtt) in cases {
        let mut transcript = long_transcript();
        transcript.snippets = vec![FetchedTranscriptSnippet::new("x", time, 0.0)];
        assert_eq!(
            SrtFormatter.format_transcript(&transcript),
            format!("1\n{srt} --> {srt}\nx\n"),
            "{time}"
        );
        assert_eq!(
            WebVttFormatter.format_transcript(&transcript),
            format!("WEBVTT\n\n{webvtt} --> {webvtt}\nx\n"),
            "{time}"
        );
    }
}

/// Timestamped text puts each caption on one line after its start time, with hours once the
/// transcript passes an hour.
#[test]
fn timestamped_text_shows_start_times() {
    let snippet = |text: &str, start: f64| FetchedTranscriptSnippet::new(text, start, 1.0);
    let mut short = long_transcript();
    short.snippets = vec![
        snippet("one", -0.5),
        snippet("two\nlines", 65.9),
        snippet("three", 3599.0),
    ];
    // A caption over two lines stays one line, so every line has a time.
    assert_eq!(
        TimestampedTextFormatter.format_transcript(&short),
        "[00:00] one\n[01:05] two lines\n[59:59] three"
    );
    // Past an hour, every line gets hours, so the columns line up.
    assert_eq!(
        TimestampedTextFormatter.format_transcript(&long_transcript()),
        "[00:00:00] first\n[00:00:02] overlaps\n[01:02:05] later"
    );
    assert_eq!(
        "timestamped-text"
            .parse::<FormatterKind>()
            .map(FormatterKind::name),
        Ok("timestamped-text")
    );
}

/// A fetched transcript serializes to JSON with keys such as `language_code` and reads back
/// unchanged.
#[test]
fn fetched_transcripts_survive_a_serde_round_trip() {
    let transcript = long_transcript();
    let json = serde_json::to_string(&transcript).unwrap();
    assert!(json.contains(r#""language_code":"en""#), "{json}");
    assert_eq!(
        serde_json::from_str::<FetchedTranscript>(&json).unwrap(),
        transcript
    );
}

/// JSON output turns a transcript into an array of snippet objects and many transcripts into an
/// array of arrays.
#[test]
fn json_is_an_array_of_snippets() {
    let transcript = long_transcript();
    let json: serde_json::Value =
        serde_json::from_str(&JsonFormatter.format_transcript(&transcript)).unwrap();
    assert_eq!(
        json[1],
        serde_json::json!({"text": "overlaps", "start": 2.5, "duration": 1.25})
    );
    let many: serde_json::Value =
        serde_json::from_str(&JsonFormatter.format_transcripts(&[transcript.clone(), transcript]))
            .unwrap();
    assert_eq!(many.as_array().map(Vec::len), Some(2));
}

/// Pretty output holds the same JSON as `JsonFormatter`, indented by two spaces.
#[test]
fn pretty_is_indented_json() {
    let transcript = long_transcript();
    let one = PrettyPrintFormatter.format_transcript(&transcript);
    assert!(
        one.starts_with("[\n  {\n    \"text\": \"first\",\n    \"start\": 0.0,"),
        "{one}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&one).unwrap(),
        serde_json::from_str::<serde_json::Value>(&JsonFormatter.format_transcript(&transcript))
            .unwrap()
    );
    let both = [transcript.clone(), transcript];
    let many = PrettyPrintFormatter.format_transcripts(&both);
    assert!(many.starts_with("[\n  [\n    {"), "{many}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&many).unwrap(),
        serde_json::from_str::<serde_json::Value>(&JsonFormatter.format_transcripts(&both))
            .unwrap()
    );
}
