//! Fetch a transcript and print it with timestamps.
//!
//! ```bash
//! cargo run --example fetch -- dQw4w9WgXcQ
//! ```

use std::io::Write;

use youtube_transcript_api::YouTubeTranscriptApi;

/// Fetches the transcript of the video given as the first argument.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    let video_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());
    let api = YouTubeTranscriptApi::new()?;

    // The first language that has a transcript wins.
    let transcript = api.fetch(&video_id, &["de", "en"], false).await?;
    writeln!(
        out,
        "{} ({})",
        transcript.language, transcript.language_code
    )?;
    for snippet in &transcript {
        writeln!(out, "{:>8.2}  {}", snippet.start, snippet.text)?;
    }
    Ok(())
}
