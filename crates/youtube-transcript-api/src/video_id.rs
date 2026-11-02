//! Video IDs from YouTube URLs.

use std::borrow::Cow;

use reqwest::Url;

/// The video ID in a YouTube URL, or `input` unchanged if it isn't one.
///
/// Recognises `youtube.com/watch?v=ID` (also on `m.`, `music.` and
/// `www.`), `youtu.be/ID`, and the `/shorts/`, `/embed/`, `/live/`, `/v/`
/// and `/e/` paths, including on `youtube-nocookie.com`. The `https://` is
/// optional. Anything else, such as a bare ID, comes back as given: this
/// doesn't check that an ID is 11 characters of `[A-Za-z0-9_-]`, so that an
/// unusual ID still reaches YouTube, which answers for it. Request URLs
/// percent-encode the ID instead (see
/// [`watch_url`](crate::settings::watch_url)), so an odd string can't alter
/// the URL.
///
/// ```
/// use youtube_transcript_api::video_id;
///
/// assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ?t=42"), "dQw4w9WgXcQ");
/// assert_eq!(video_id("youtube.com/shorts/dQw4w9WgXcQ"), "dQw4w9WgXcQ");
/// assert_eq!(video_id("dQw4w9WgXcQ"), "dQw4w9WgXcQ");
/// ```
#[must_use]
pub fn video_id(input: &str) -> Cow<'_, str> {
    let trimmed = input.trim();
    let with_scheme = if trimmed.contains("://") {
        Cow::Borrowed(trimmed)
    } else {
        Cow::Owned(format!("https://{trimmed}"))
    };
    Url::parse(&with_scheme)
        .ok()
        .and_then(|url| id_in(&url))
        .map_or(Cow::Borrowed(input), Cow::Owned)
}

/// The video ID in a parsed YouTube URL.
fn id_in(url: &Url) -> Option<String> {
    let host = url.host_str()?.trim_start_matches("www.");
    // A URL without path segments has no host either, so it never gets here.
    let mut segments = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty());
    let id = match host {
        "youtu.be" => segments.next().map(str::to_owned),
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => {
            match segments.next() {
                Some("watch") => url
                    .query_pairs()
                    .find(|(key, _)| key == "v")
                    .map(|(_, value)| value.into_owned()),
                Some("shorts" | "embed" | "live" | "v" | "e") => segments.next().map(str::to_owned),
                _ => None,
            }
        }
        _ => None,
    };
    id.filter(|found| !found.is_empty())
}

/// Video IDs from URLs.
#[cfg(test)]
mod tests {
    use super::video_id;

    /// Each URL shape gives its video ID.
    #[test]
    fn urls_give_their_video_id() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ&t=42s",
            "http://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD",
            "www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "youtu.be/dQw4w9WgXcQ?si=abc",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ?start=10",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/live/dQw4w9WgXcQ?feature=share",
            "https://www.youtube.com/v/dQw4w9WgXcQ",
            "https://www.youtube.com/e/dQw4w9WgXcQ",
            "  https://youtu.be/dQw4w9WgXcQ  ",
        ] {
            assert_eq!(video_id(url), "dQw4w9WgXcQ", "{url}");
        }
    }

    /// Anything else comes back unchanged.
    #[test]
    fn anything_else_is_unchanged() {
        for input in [
            "dQw4w9WgXcQ",
            "-abc",
            "https://example.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?list=PL123",
            "https://www.youtube.com/watch?v=",
            "https://www.youtube.com/channel/UC123",
            "https://www.youtube.com/",
            "https://youtu.be/",
            "mailto:someone",
            "file:///watch?v=dQw4w9WgXcQ",
            "http://[::1",
        ] {
            assert_eq!(video_id(input), input, "{input}");
        }
    }
}
