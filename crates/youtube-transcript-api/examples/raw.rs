//! Print a transcript in one of YouTube's own formats: here json3, which has
//! per-word timing for generated captions.
//!
//! ```bash
//! cargo run --example raw -- https://youtu.be/dQw4w9WgXcQ
//! ```

use std::io::Write;

use youtube_transcript_api::{RawFormat, YouTubeTranscriptApi};

/// Prints the `json3` transcript of the video given as the first argument.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let video = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dQw4w9WgXcQ".to_owned());
    let api = YouTubeTranscriptApi::new()?;
    let list = api.list(&video).await?;
    let generated = list.find_generated_transcript(&["en"])?;
    let json3 = generated.fetch_raw(RawFormat::Json3).await?;
    write!(std::io::stdout().lock(), "{json3}")?;
    Ok(())
}
