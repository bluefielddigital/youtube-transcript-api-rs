//! Endpoints and the InnerTube client identity.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// What a URL component must escape: everything except the unreserved
/// characters, so `&`, `#` and `%` in untrusted text can't change where a
/// URL splits.
pub(crate) const ESCAPE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The watch page for a video. A real ID (`[A-Za-z0-9_-]`) goes in as is;
/// anything else is percent-encoded, so an odd string can't add query
/// parameters or end the URL early.
#[must_use]
pub fn watch_url(video_id: &str) -> String {
    format!(
        "https://www.youtube.com/watch?v={}",
        utf8_percent_encode(video_id, ESCAPE)
    )
}

/// The InnerTube player endpoint for an API key.
#[must_use]
pub fn innertube_api_url(api_key: &str) -> String {
    format!("https://www.youtube.com/youtubei/v1/player?key={api_key}")
}

/// The InnerTube client the requests claim to be.
pub const INNERTUBE_CLIENT_NAME: &str = "ANDROID";

/// The version of that client.
pub const INNERTUBE_CLIENT_VERSION: &str = "20.10.38";

/// Escaping video IDs in URLs.
#[cfg(test)]
mod tests {
    use super::watch_url;

    /// A real ID goes in unchanged.
    #[test]
    fn real_ids_are_unchanged() {
        assert_eq!(
            watch_url("a-B_9xYz012"),
            "https://www.youtube.com/watch?v=a-B_9xYz012"
        );
    }

    /// Reserved characters, spaces and non-ASCII text turn into escapes.
    #[test]
    fn odd_ids_are_escaped() {
        assert_eq!(
            watch_url("a&list=x#y %ü"),
            "https://www.youtube.com/watch?v=a%26list%3Dx%23y%20%25%C3%BC"
        );
    }
}
