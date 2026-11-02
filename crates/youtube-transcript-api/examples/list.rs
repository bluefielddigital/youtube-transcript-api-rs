//! List a video's transcripts, pick one, translate it, and show the video's
//! details.
//!
//! ```bash
//! cargo run --example list -- dQw4w9WgXcQ
//! ```

use std::io::Write;

use youtube_transcript_api::YouTubeTranscriptApi;

/// Lists, picks and translates the transcripts of the video given as the first argument.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    let video_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());
    let api = YouTubeTranscriptApi::new()?;
    let list = api.list(&video_id).await?;

    if let Some(details) = list.video_details() {
        writeln!(out, "{:?} by {:?}", details.title, details.author)?;
        writeln!(
            out,
            "{:?} seconds, {:?} views, published {:?}",
            details.length_seconds, details.view_count, details.publish_date
        )?;
    }

    for transcript in &list {
        writeln!(
            out,
            "{} {:?} generated={} translatable={}",
            transcript.language_code,
            transcript.language,
            transcript.is_generated,
            transcript.is_translatable()
        )?;
    }

    // Prefer a manually created transcript, in the first language that has one.
    let english = list.find_manually_created_transcript(&["en", "en-US"])?;
    if english.is_translatable() {
        // YouTube sometimes refuses translations (HTTP 429) even when it
        // serves the original, so don't let that end the example.
        match english.translate("de")?.fetch(false).await {
            Ok(german) => writeln!(out, "German: {} lines", german.len())?,
            Err(error) => writeln!(out, "No German translation: {error:?}")?,
        }
    }
    Ok(())
}
