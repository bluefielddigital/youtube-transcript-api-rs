//! Fetch the transcripts and subtitles of YouTube videos, including
//! automatically generated ones. It needs no API key and no headless browser.
//!
//! ```no_run
//! # async fn run() -> Result<(), youtube_transcript_api::Error> {
//! use youtube_transcript_api::YouTubeTranscriptApi;
//!
//! let api = YouTubeTranscriptApi::new()?;
//! let transcript = api.fetch("dQw4w9WgXcQ", &["en"], false).await?;
//! for snippet in &transcript {
//!     println!("{:>7.2}  {}", snippet.start, snippet.text);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! - [`YouTubeTranscriptApi`]: fetch a transcript by video ID or URL, or list
//!   a video's transcripts to pick one by language, translate it, and read
//!   the video's details.
//! - [`formatters`]: plain text, timestamped text, SRT, WebVTT and JSON;
//!   [`Transcript::fetch_raw`] for YouTube's own formats.
//! - [`proxies`]: route requests through a proxy, with retries when YouTube
//!   blocks one.
//!
//! Inspired by Jonas Depoix's [youtube-transcript-api] for Python, whose
//! command line and output formats this crate shares.
//!
//! [youtube-transcript-api]: https://github.com/jdepoix/youtube-transcript-api
//!
//! # Features
//!
//! - `tracing`: [`tracing`](https://docs.rs/tracing) events under the
//!   `youtube_transcript_api` target. Each request and its status (`DEBUG`),
//!   each retry with its cause and wait (`INFO`), including a rotating
//!   proxy's retry through a new IP when YouTube blocks one, and accepting
//!   YouTube's cookie consent page (`DEBUG`).

/// A `tracing` event with the `tracing` feature, and nothing without it.
macro_rules! event {
    ($level:ident, $($field:tt)+) => {
        #[cfg(feature = "tracing")]
        ::tracing::$level!($($field)+);
    };
}

/// The client and its builder.
mod api;
/// Errors and their messages.
mod errors;
pub mod formatters;
pub mod http;
mod innertube;
pub mod proxies;
pub mod settings;
/// HTML tag stripping.
mod tags;
/// Listing, finding, translating, fetching, and parsing transcripts.
mod transcripts;
/// Video details.
mod video;
mod video_id;

pub use api::{DEFAULT_LANGUAGES, YouTubeTranscriptApi, YouTubeTranscriptApiBuilder};
pub use errors::{BoxError, CouldNotRetrieveTranscript, Error, Reason, UnexpectedData};
pub use transcripts::{
    FetchedTranscript, FetchedTranscriptSnippet, RawFormat, Transcript, TranscriptList,
    TranslationLanguage, UnknownRawFormat,
};
pub use video::{Thumbnail, VideoDetails};
pub use video_id::video_id;
