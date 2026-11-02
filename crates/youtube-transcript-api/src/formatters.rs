//! Turn fetched transcripts into text: plain text, SRT, WebVTT, and compact
//! or indented JSON.
//!
//! ```
//! use youtube_transcript_api::formatters::{Formatter, FormatterKind, SrtFormatter};
//! use youtube_transcript_api::{FetchedTranscript, FetchedTranscriptSnippet};
//!
//! let transcript = FetchedTranscript::new(
//!     "GJLlxj_dtq8",
//!     "English",
//!     "en",
//!     false,
//!     vec![
//!         FetchedTranscriptSnippet::new("Hey, this is just a test", 0.0, 1.54),
//!         FetchedTranscriptSnippet::new("this is not the original transcript", 1.54, 4.16),
//!     ],
//! );
//! assert_eq!(
//!     SrtFormatter.format_transcript(&transcript),
//!     "1\n00:00:00,000 --> 00:00:01,540\nHey, this is just a test\n\n\
//!      2\n00:00:01,540 --> 00:00:05,700\nthis is not the original transcript\n"
//! );
//!
//! // Or pick the formatter by name, as the command line does.
//! let kind: FormatterKind = "text".parse()?;
//! assert_eq!(
//!     kind.formatter().format_transcript(&transcript),
//!     "Hey, this is just a test\nthis is not the original transcript"
//! );
//! # Ok::<(), youtube_transcript_api::formatters::UnknownFormatterType>(())
//! ```

use std::fmt;
use std::str::FromStr;

use num_traits::ToPrimitive;

use crate::transcripts::{FetchedTranscript, FetchedTranscriptSnippet};

/// Formats one or more transcripts as a string.
pub trait Formatter {
    /// Formats one transcript.
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String;

    /// Formats a list of transcripts as one string; by default, two blank
    /// lines separate them.
    fn format_transcripts(&self, transcripts: &[FetchedTranscript]) -> String {
        let parts: Vec<String> = transcripts
            .iter()
            .map(|t| self.format_transcript(t))
            .collect();
        parts.join("\n\n\n")
    }
}

/// The snippets as a JSON array; a list of transcripts as an array of arrays.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonFormatter;

impl Formatter for JsonFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        // Strings and floats always serialise.
        serde_json::to_string(&transcript.snippets).unwrap_or_default()
    }

    fn format_transcripts(&self, transcripts: &[FetchedTranscript]) -> String {
        serde_json::to_string(&snippet_lists(transcripts)).unwrap_or_default()
    }
}

/// The same JSON as [`JsonFormatter`], indented for reading.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrettyPrintFormatter;

impl Formatter for PrettyPrintFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        serde_json::to_string_pretty(&transcript.snippets).unwrap_or_default()
    }

    fn format_transcripts(&self, transcripts: &[FetchedTranscript]) -> String {
        serde_json::to_string_pretty(&snippet_lists(transcripts)).unwrap_or_default()
    }
}

/// Each transcript's snippets, for formatting a list as an array of arrays.
fn snippet_lists(transcripts: &[FetchedTranscript]) -> Vec<&[FetchedTranscriptSnippet]> {
    transcripts.iter().map(|t| t.snippets.as_slice()).collect()
}

/// Plain text, one snippet per line, no timestamps. Two blank lines separate
/// transcripts.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextFormatter;

impl Formatter for TextFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        let lines: Vec<&str> = transcript.iter().map(|s| s.text.as_str()).collect();
        lines.join("\n")
    }
}

/// Plain text with each snippet's start time.
///
/// Lines read `[MM:SS] text`, or `[HH:MM:SS] text` on every line once the
/// transcript runs past an hour. A caption that spans lines becomes one
/// line, so every line has a time.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimestampedTextFormatter;

impl Formatter for TimestampedTextFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        let with_hours = transcript.iter().any(|s| s.start >= 3600.0);
        let lines: Vec<String> = transcript
            .iter()
            .map(|snippet| {
                let total = snippet.start.floor().to_i64().unwrap_or_default().max(0);
                let text = snippet.text.replace('\n', " ");
                let (minutes, seconds) = (total / 60, total % 60);
                if with_hours {
                    format!(
                        "[{:02}:{:02}:{seconds:02}] {text}",
                        total / 3600,
                        minutes % 60
                    )
                } else {
                    format!("[{minutes:02}:{seconds:02}] {text}")
                }
            })
            .collect();
        lines.join("\n")
    }
}

/// `SubRip` (`.srt`) subtitles.
#[derive(Debug, Clone, Copy, Default)]
pub struct SrtFormatter;

impl Formatter for SrtFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        let cues: Vec<String> = cue_times(transcript)
            .zip(transcript)
            .enumerate()
            .map(|(i, ((start, end), snippet))| {
                format!(
                    "{}\n{} --> {}\n{}",
                    i + 1,
                    timestamp(start, ','),
                    timestamp(end, ','),
                    snippet.text
                )
            })
            .collect();
        format!("{}\n", cues.join("\n\n"))
    }
}

/// WebVTT (`.vtt`) subtitles.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebVttFormatter;

impl Formatter for WebVttFormatter {
    fn format_transcript(&self, transcript: &FetchedTranscript) -> String {
        let cues: Vec<String> = cue_times(transcript)
            .zip(transcript)
            .map(|((start, end), snippet)| {
                format!(
                    "{} --> {}\n{}",
                    timestamp(start, '.'),
                    timestamp(end, '.'),
                    snippet.text
                )
            })
            .collect();
        format!("WEBVTT\n\n{}\n", cues.join("\n\n"))
    }
}

/// Start and end of each cue. A cue ends when the next one starts, if that
/// is before its own end.
fn cue_times(transcript: &FetchedTranscript) -> impl Iterator<Item = (f64, f64)> + '_ {
    transcript.iter().enumerate().map(|(i, snippet)| {
        let end = snippet.start + snippet.duration;
        let next_start = transcript
            .snippets
            .get(i + 1)
            .map(|next| next.start)
            .filter(|&start| start < end);
        (snippet.start, next_start.unwrap_or(end))
    })
}

/// `HH:MM:SS<sep>mmm`. Rounding the milliseconds to two decimals and then
/// truncating lets them read 1000 (`0.9999996` is `00:00:00,1000`), which
/// keeps files identical to youtube-transcript-api's. Hours, minutes and
/// seconds come from the floored time. A time that isn't a finite `i64`
/// prints as zero.
fn timestamp(seconds: f64, separator: char) -> String {
    let whole = seconds.floor().to_i64().unwrap_or_default();
    let ms = truncated_round_2((seconds - seconds.trunc()) * 1000.0)
        .to_i64()
        .unwrap_or_default();
    let hours = whole.div_euclid(3600);
    let minutes = whole.rem_euclid(3600) / 60;
    let secs = whole.rem_euclid(60);
    format!("{hours:02}:{minutes:02}:{secs:02}{separator}{ms:03}")
}

/// `x` rounded to two decimals, then truncated, as a whole `f64`. Rounding
/// to two decimals can only carry into the next whole number when the
/// fraction is greater than 0.995, and the `f64` nearest 0.995 is just below it,
/// so `> 0.995` is exact: no decimal round trip needed.
fn truncated_round_2(x: f64) -> f64 {
    let magnitude = x.abs();
    let whole = magnitude.trunc();
    let carried = if magnitude - whole > 0.995 {
        whole + 1.0
    } else {
        whole
    };
    carried.copysign(x)
}

/// The output formats, by their CLI names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FormatterKind {
    /// Compact JSON: an array of snippets per transcript.
    Json,
    /// Indented JSON, easier to read. The default.
    #[default]
    Pretty,
    /// Plain text, one caption per line, no timestamps.
    Text,
    /// Plain text with each caption's start time (`MM:SS`, or `HH:MM:SS` past
    /// an hour).
    TimestampedText,
    /// WebVTT subtitles (`.vtt`).
    WebVtt,
    /// SRT subtitles (`.srt`).
    Srt,
}

impl FormatterKind {
    /// Every kind, in the CLI's order.
    pub const ALL: &[Self] = &[
        Self::Json,
        Self::Pretty,
        Self::Text,
        Self::TimestampedText,
        Self::WebVtt,
        Self::Srt,
    ];

    /// The CLI name, for example `webvtt`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Pretty => "pretty",
            Self::Text => "text",
            Self::TimestampedText => "timestamped-text",
            Self::WebVtt => "webvtt",
            Self::Srt => "srt",
        }
    }

    /// A formatter of this kind.
    #[must_use]
    pub fn formatter(self) -> Box<dyn Formatter> {
        match self {
            Self::Json => Box::new(JsonFormatter),
            Self::Pretty => Box::new(PrettyPrintFormatter),
            Self::Text => Box::new(TextFormatter),
            Self::TimestampedText => Box::new(TimestampedTextFormatter),
            Self::WebVtt => Box::new(WebVttFormatter),
            Self::Srt => Box::new(SrtFormatter),
        }
    }
}

impl fmt::Display for FormatterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An unknown format name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error(
    "The format '{0}' is not supported. Choose one of the following formats: \
     json, pretty, text, timestamped-text, webvtt, srt"
)]
pub struct UnknownFormatterType(pub String);

impl FromStr for FormatterKind {
    type Err = UnknownFormatterType;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.name() == name)
            .ok_or_else(|| UnknownFormatterType(name.to_owned()))
    }
}

/// Timestamps and formatter names.
#[cfg(test)]
mod tests {
    use super::{FormatterKind, timestamp};

    /// `tests/golden/timestamps.json` has 2000 more.
    #[test]
    fn timestamps() {
        assert_eq!(timestamp(0.0, '.'), "00:00:00.000");
        assert_eq!(timestamp(6.93, '.'), "00:00:06.930");
        assert_eq!(timestamp(3725.5, ','), "01:02:05,500");
        assert_eq!(timestamp(59.9996, '.'), "00:00:59.999");
        assert_eq!(timestamp(0.999_999_6, ','), "00:00:00,1000");
        assert_eq!(timestamp(-1.0, '.'), "-1:59:59.000");
        assert_eq!(timestamp(-0.25, ','), "-1:59:59,-250");
        assert_eq!(timestamp(f64::NAN, ','), "00:00:00,000");
    }

    /// Every kind parses back from its name, and others fail.
    #[test]
    fn kinds_round_trip_by_name() {
        for &kind in FormatterKind::ALL {
            assert_eq!(kind.name().parse::<FormatterKind>(), Ok(kind));
            assert_eq!(kind.to_string(), kind.name());
            assert!(!kind.formatter().format_transcripts(&[]).contains("error"));
        }
        let error = "xml".parse::<FormatterKind>().unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("The format 'xml' is not supported.")
        );
    }
}
