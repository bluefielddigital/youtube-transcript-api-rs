use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

use crate::http::TransportError;
use crate::proxies::{InvalidProxyConfig, ProxyKind};
use crate::settings::watch_url;

/// Everything this crate can fail with.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Fetching a transcript failed; the [`Reason`] says why.
    #[error(transparent)]
    CouldNotRetrieveTranscript(#[from] CouldNotRetrieveTranscript),
    /// A proxy config is invalid.
    #[error(transparent)]
    InvalidProxyConfig(#[from] InvalidProxyConfig),
    /// The connection failed below HTTP.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// YouTube returned data with an unexpected shape.
    #[error(transparent)]
    UnexpectedData(#[from] UnexpectedData),
}

/// A boxed error from a parser this crate uses, kept as the `source`.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// YouTube returned data with an unexpected shape.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UnexpectedData {
    /// A field is missing from the player response or the transcript XML,
    /// or has the wrong type.
    #[error("missing or invalid `{field}` in YouTube's response")]
    Field {
        /// The field's name, for example `captionTracks`.
        field: &'static str,
    },
    /// The player response isn't JSON.
    #[error("YouTube returned invalid JSON")]
    InvalidJson(#[source] BoxError),
    /// The transcript isn't well-formed XML.
    #[error("invalid transcript XML")]
    InvalidXml(#[source] BoxError),
    /// A transcript timestamp isn't a number.
    #[error("transcript `{attribute}` is not a number: {value:?}")]
    NotANumber {
        /// `start` or `dur`.
        attribute: &'static str,
        /// The attribute's value.
        value: String,
        /// Why it didn't parse.
        source: std::num::ParseFloatError,
    },
}

/// Why fetching a transcript failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reason {
    /// YouTube's response lacks the data a transcript needs.
    YouTubeDataUnparsable,
    /// YouTube answered with an HTTP error status.
    YouTubeRequestFailed {
        /// What failed, for example `HTTP status server error (500 Internal Server
        /// Error) for url (…)`.
        reason: String,
    },
    /// YouTube says the video can't play.
    VideoUnplayable {
        /// The reason YouTube gave, if any.
        reason: Option<String>,
        /// Extra detail lines YouTube gave.
        sub_reasons: Vec<String>,
    },
    /// The video doesn't exist, or no longer does.
    VideoUnavailable,
    /// The video ID is a URL; pass the ID instead.
    InvalidVideoId,
    /// YouTube is blocking requests from this IP.
    RequestBlocked {
        /// The proxy YouTube blocked, if any.
        proxy: Option<ProxyKind>,
    },
    /// YouTube answered with HTTP 429 or a captcha: a kind of `RequestBlocked`.
    IpBlocked {
        /// The proxy YouTube blocked, if any.
        proxy: Option<ProxyKind>,
    },
    /// The video has subtitles turned off.
    TranscriptsDisabled,
    /// The video is age-restricted, and signing in isn't supported.
    AgeRestricted,
    /// YouTube can't translate this transcript.
    NotTranslatable,
    /// YouTube can't translate this transcript into that language.
    TranslationLanguageNotAvailable,
    /// Accepting YouTube's cookie consent page failed.
    FailedToCreateConsentCookie,
    /// None of the requested languages has a transcript.
    NoTranscriptFound {
        /// The requested language codes, in order.
        requested_language_codes: Vec<String>,
        /// The video's transcript list, as text: what's available instead.
        transcript_data: String,
    },
    /// YouTube requires a PO token for this video.
    PoTokenRequired,
}

/// Fetching the transcript for `video_id` failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub struct CouldNotRetrieveTranscript {
    /// The requested video ID.
    pub video_id: String,
    /// What went wrong.
    pub reason: Reason,
}

/// The start of every message, before the video URL.
const ERROR_MESSAGE: &str = "\nCould not retrieve a transcript for the video ";
/// Between the URL and the cause.
const CAUSE_MESSAGE_INTRO: &str = " This is most likely caused by:\n\n";
/// The end of every message.
const REPORT_AN_ISSUE: &str = "\n\nIf you are sure that the described cause is not responsible \
for this error and that a transcript should be retrievable, please create an issue at \
https://github.com/bluefielddigital/youtube-transcript-api-rs/issues. Please add which version \
of youtube-transcript-api-rs you are using (`youtube-transcript-api --version`, or the crate \
version in your Cargo.lock) and provide the information needed to replicate the error. Also \
make sure that there are no open issues which already describe your problem!";

/// The troubleshooting guide's section on blocked requests.
const IP_BANS_GUIDE: &str = "https://github.com/bluefielddigital/youtube-transcript-api-rs\
/blob/main/TROUBLESHOOTING.md#blocked-requests";

/// The README's proxy section.
const PROXY_GUIDE: &str = "https://github.com/bluefielddigital/youtube-transcript-api-rs#proxies";

/// The blocked message without a proxy.
const BASE_BLOCKED_MESSAGE: &str = "YouTube is blocking requests from your IP. This usually is \
due to one of the following reasons:\n\
- You have done too many requests and your IP has been blocked by YouTube\n\
- You are doing requests from an IP belonging to a cloud provider (like AWS, Google Cloud \
Platform, Azure, etc.). Unfortunately, most IPs from cloud providers are blocked by \
YouTube.\n\n";

/// The end of the blocked message without a proxy, before the guide URL.
const BLOCKED_TAIL: &str = "Use proxies to hide your IP address, as explained in the \
troubleshooting guide (";

/// The blocked message with a generic proxy, before the proxy guide URL.
const WITH_GENERIC_PROXY: &str = "YouTube is blocking your requests, despite you using \
proxies. Keep in mind that a proxy is just a way to hide your real IP behind the IP of that \
proxy, but there is no guarantee that the IP of that proxy won't be blocked as well.\n\n\
The only truly reliable way to prevent IP blocks is rotating through a large pool of \
residential IPs, by using a provider like Webshare (https://www.webshare.io/), which \
provides you with a pool of >30M residential IPs (make sure to purchase \"Residential\" \
proxies, NOT \"Proxy Server\" or \"Static Residential\"!).\n\n\
You will find more information on how to use Webshare here: ";

/// The blocked message with Webshare.
const WITH_WEBSHARE_PROXY: &str = "YouTube is blocking your requests, despite you using \
Webshare proxies. Please make sure that you have purchased \"Residential\" proxies and NOT \
\"Proxy Server\" or \"Static Residential\", as those won't work as reliably! The free tier \
also uses \"Proxy Server\" and will NOT work!\n\n\
The only reliable option is using \"Residential\" proxies (not \"Static Residential\"), as \
this allows you to rotate through a pool of over 30M IPs, which means you will always find \
an IP that hasn't been blocked by YouTube yet!";

/// The cause for a URL passed as a video ID.
const INVALID_VIDEO_ID: &str = "You provided an invalid video id. Make sure you are using the \
video id and NOT the url!\n\n\
For https://www.youtube.com/watch?v=1234, pass \"1234\".";

/// The cause for an age-restricted video.
const AGE_RESTRICTED: &str = "This video is age-restricted. Therefore, you are unable to \
retrieve transcripts for it without authenticating yourself, and signing in isn't \
supported.";

/// The cause when YouTube's data doesn't parse.
const DATA_UNPARSABLE: &str = "The data required to fetch the transcript is not parsable. \
This should not happen, please open an issue (make sure to include the video ID)!";

/// The cause when YouTube wants a PO token.
const PO_TOKEN_REQUIRED: &str = "The requested video cannot be retrieved without a PO Token. \
If this happens, please open a GitHub issue!";

impl Reason {
    /// The explanation after "most likely caused by" in the message.
    #[must_use]
    pub fn cause(&self) -> Cow<'static, str> {
        match self {
            Self::YouTubeDataUnparsable => DATA_UNPARSABLE.into(),
            Self::YouTubeRequestFailed { reason } => {
                format!("Request to YouTube failed: {reason}").into()
            }
            Self::VideoUnplayable {
                reason,
                sub_reasons,
            } => unplayable_cause(reason.as_deref(), sub_reasons).into(),
            Self::VideoUnavailable => "The video is no longer available".into(),
            Self::InvalidVideoId => INVALID_VIDEO_ID.into(),
            Self::RequestBlocked { proxy } | Self::IpBlocked { proxy } => blocked_cause(*proxy),
            Self::TranscriptsDisabled => "Subtitles are disabled for this video".into(),
            Self::AgeRestricted => AGE_RESTRICTED.into(),
            Self::NotTranslatable => "The requested language is not translatable".into(),
            Self::TranslationLanguageNotAvailable => {
                "The requested translation language is not available".into()
            }
            Self::FailedToCreateConsentCookie => {
                "Failed to automatically give consent to saving cookies".into()
            }
            Self::NoTranscriptFound {
                requested_language_codes,
                transcript_data,
            } => format!(
                "No transcripts were found for any of the requested language codes: \
                 {requested_language_codes:?}\n\n{transcript_data}"
            )
            .into(),
            Self::PoTokenRequired => PO_TOKEN_REQUIRED.into(),
        }
    }

    /// True for `RequestBlocked` and `IpBlocked`: YouTube is blocking this IP.
    #[must_use]
    pub const fn is_request_blocked(&self) -> bool {
        matches!(self, Self::RequestBlocked { .. } | Self::IpBlocked { .. })
    }

    /// The proxy slot of `RequestBlocked` and `IpBlocked`, filled in once
    /// retries through the proxy run out.
    pub(crate) const fn blocked_by_proxy_mut(&mut self) -> Option<&mut Option<ProxyKind>> {
        match self {
            Self::RequestBlocked { proxy } | Self::IpBlocked { proxy } => Some(proxy),
            _ => None,
        }
    }
}

/// The cause for an unplayable video, with YouTube's detail lines.
fn unplayable_cause(reason: Option<&str>, sub_reasons: &[String]) -> String {
    let mut text = reason.unwrap_or("No reason specified!").to_owned();
    if !sub_reasons.is_empty() {
        let lines: Vec<String> = sub_reasons.iter().map(|s| format!(" - {s}")).collect();
        text = format!("{text}\n\nAdditional Details:\n{}", lines.join("\n"));
    }
    format!("The video is unplayable for the following reason: {text}")
}

/// The cause for a blocked request, with advice for the proxy in use.
fn blocked_cause(proxy: Option<ProxyKind>) -> Cow<'static, str> {
    match proxy {
        Some(ProxyKind::Webshare) => WITH_WEBSHARE_PROXY.into(),
        Some(ProxyKind::Generic) => format!("{WITH_GENERIC_PROXY}{PROXY_GUIDE}").into(),
        None | Some(ProxyKind::Custom) => {
            format!("{BASE_BLOCKED_MESSAGE}{BLOCKED_TAIL}{IP_BANS_GUIDE}).").into()
        }
    }
}

impl CouldNotRetrieveTranscript {
    /// The error for `video_id`, which failed for `reason`.
    pub fn new(video_id: impl Into<String>, reason: Reason) -> Self {
        Self {
            video_id: video_id.into(),
            reason,
        }
    }
}

impl fmt::Display for CouldNotRetrieveTranscript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{ERROR_MESSAGE}{}!{CAUSE_MESSAGE_INTRO}{}{REPORT_AN_ISSUE}",
            watch_url(&self.video_id),
            self.reason.cause()
        )
    }
}

/// The error messages.
#[cfg(test)]
mod tests {
    use super::{CouldNotRetrieveTranscript, Reason};
    use crate::proxies::ProxyKind;

    /// The full message for `reason`.
    fn message(reason: Reason) -> String {
        CouldNotRetrieveTranscript::new("abc", reason).to_string()
    }

    /// The text around every cause.
    #[test]
    fn message_frame() {
        let text = message(Reason::VideoUnavailable);
        assert!(text.starts_with(
            "\nCould not retrieve a transcript for the video https://www.youtube.com/watch?v=abc! \
             This is most likely caused by:\n\nThe video is no longer available\n\nIf you are sure"
        ));
        assert!(text.ends_with("which already describe your problem!"));
    }

    /// An unplayable video without a reason or detail lines.
    #[test]
    fn unplayable_without_reason_or_sub_reasons() {
        assert_eq!(
            Reason::VideoUnplayable {
                reason: None,
                sub_reasons: vec![],
            }
            .cause(),
            "The video is unplayable for the following reason: No reason specified!"
        );
    }

    /// An unplayable video's detail lines.
    #[test]
    fn unplayable_with_sub_reasons() {
        assert_eq!(
            Reason::VideoUnplayable {
                reason: Some("R".into()),
                sub_reasons: vec!["a".into(), "b".into()],
            }
            .cause(),
            "The video is unplayable for the following reason: R\n\nAdditional Details:\n - a\n - b"
        );
    }

    /// The blocked-request advice depends on the proxy.
    #[test]
    fn blocked_messages_depend_on_proxy() {
        for blocked in [
            Reason::RequestBlocked { proxy: None },
            Reason::IpBlocked {
                proxy: Some(ProxyKind::Custom),
            },
        ] {
            assert!(
                blocked
                    .cause()
                    .ends_with("TROUBLESHOOTING.md#blocked-requests)."),
                "{blocked:?}"
            );
        }
        assert!(
            Reason::IpBlocked {
                proxy: Some(ProxyKind::Webshare)
            }
            .cause()
            .contains("despite you using Webshare proxies")
        );
        assert!(
            Reason::RequestBlocked {
                proxy: Some(ProxyKind::Generic)
            }
            .cause()
            .starts_with("YouTube is blocking your requests, despite you using proxies.")
        );
    }

    /// `is_request_blocked` covers both blocked reasons.
    #[test]
    fn request_blocked_includes_ip_blocked() {
        assert!(Reason::RequestBlocked { proxy: None }.is_request_blocked());
        assert!(Reason::IpBlocked { proxy: None }.is_request_blocked());
        assert!(!Reason::VideoUnavailable.is_request_blocked());
    }

    /// `NoTranscriptFound` lists the requested codes and the transcripts.
    #[test]
    fn no_transcript_found_lists_codes() {
        let cause = Reason::NoTranscriptFound {
            requested_language_codes: vec!["de".into(), "en".into()],
            transcript_data: "DATA".into(),
        }
        .cause();
        assert_eq!(
            cause,
            "No transcripts were found for any of the requested language codes: [\"de\", \"en\"]\n\nDATA"
        );
    }
}
