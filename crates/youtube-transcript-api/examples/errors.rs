//! Tell the ways a fetch can fail apart.
//!
//! ```bash
//! cargo run --example errors -- dQw4w9WgXcQ
//! ```

use std::io::Write;

use youtube_transcript_api::{Error, Reason, YouTubeTranscriptApi};

/// Fetches the video given as the first argument and explains any failure.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let video_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());
    let api = YouTubeTranscriptApi::new()?;
    let result = api.fetch(&video_id, &["en"], false).await;
    let mut out = std::io::stdout().lock();
    match result {
        Ok(transcript) => writeln!(out, "{} lines", transcript.len())?,
        Err(Error::CouldNotRetrieveTranscript(error)) => match &error.reason {
            Reason::NoTranscriptFound { .. } => {
                writeln!(out, "No English transcript; try `list` for the languages")?;
            }
            Reason::TranscriptsDisabled => writeln!(out, "Transcripts are disabled")?,
            reason if reason.is_request_blocked() => {
                writeln!(out, "YouTube is blocking this IP: use a proxy")?;
            }
            // The message explains the likely cause and what to try.
            _ => writeln!(out, "{error}")?,
        },
        // Network failures, proxy configuration, unexpected responses.
        Err(other) => writeln!(out, "{other}")?,
    }
    Ok(())
}
