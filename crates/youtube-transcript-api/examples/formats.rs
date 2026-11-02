//! Fetch a transcript and print it as SRT. Swap the formatter for
//! `WebVttFormatter`, `TextFormatter`, `JsonFormatter` or
//! `PrettyPrintFormatter`.
//!
//! ```bash
//! cargo run --example formats -- dQw4w9WgXcQ > rick.srt
//! ```

use std::io::Write;

use youtube_transcript_api::YouTubeTranscriptApi;
use youtube_transcript_api::formatters::{Formatter, SrtFormatter};

/// Prints the transcript of the video given as the first argument as SRT.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    let video_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());
    let api = YouTubeTranscriptApi::new()?;
    let transcript = api.fetch(&video_id, &["en"], false).await?;
    write!(out, "{}", SrtFormatter.format_transcript(&transcript))?;
    Ok(())
}
