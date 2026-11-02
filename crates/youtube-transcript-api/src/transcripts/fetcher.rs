use std::fmt;
use std::sync::Arc;

use lazy_regex::regex_captures;
use serde_json::json;

use super::{TranscriptList, raise_http_errors, unexpected};
use crate::errors::{CouldNotRetrieveTranscript, Error, Reason, UnexpectedData};
use crate::http::Session;
use crate::innertube::{
    self, PlayabilityStatus, PlayerResponse, Status, TracklistRenderer, Tracks,
};
use crate::proxies::{ProxyConfig, RedactedProxy};
use crate::settings::{
    INNERTUBE_CLIENT_NAME, INNERTUBE_CLIENT_VERSION, innertube_api_url, watch_url,
};
use crate::video::VideoDetails;

/// The playability status of a video that plays.
const PLAYABILITY_OK: &str = "OK";
/// The status of a video with an error, such as a missing one.
const PLAYABILITY_ERROR: &str = "ERROR";
/// The status of a video that needs signing in.
const PLAYABILITY_LOGIN_REQUIRED: &str = "LOGIN_REQUIRED";
/// The reason YouTube gives when it suspects a bot.
const REASON_BOT_DETECTED: &str = "Sign in to confirm you\u{2019}re not a bot";
/// The reason YouTube gives for an age-restricted video.
const REASON_AGE_RESTRICTED: &str = "This video may be inappropriate for some users.";
/// The reason YouTube gives for a missing video.
const REASON_VIDEO_UNAVAILABLE: &str = "This video is unavailable";
/// The form on YouTube's cookie consent page.
const CONSENT_FORM: &str = "action=\"https://consent.youtube.com/s\"";

/// Fetches a video's transcript list, retrying through the proxy when
/// YouTube blocks the request. `Debug` hides the proxy's credentials.
#[derive(Clone)]
pub struct TranscriptListFetcher {
    /// The session for every request.
    session: Session,
    /// The proxy, whose retry count and kind apply.
    proxy_config: Option<Arc<dyn ProxyConfig>>,
}

impl fmt::Debug for TranscriptListFetcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TranscriptListFetcher")
            .field("session", &self.session)
            .field(
                "proxy_config",
                &self.proxy_config.as_deref().map(RedactedProxy),
            )
            .finish()
    }
}

impl TranscriptListFetcher {
    /// A fetcher using `session`, with `proxy_config`'s retries.
    pub(crate) fn new(session: Session, proxy_config: Option<Arc<dyn ProxyConfig>>) -> Self {
        Self {
            session,
            proxy_config,
        }
    }

    /// The transcript list for `video_id`.
    pub(crate) async fn fetch(&self, video_id: &str) -> Result<TranscriptList, Error> {
        let (captions, video_details) = self.fetch_captions(video_id).await?;
        TranscriptList::build(&self.session, video_id, captions, video_details)
    }

    /// The captions, retrying while YouTube blocks the request.
    async fn fetch_captions(&self, video_id: &str) -> Result<Captions, Error> {
        let retries = self
            .proxy_config
            .as_ref()
            .map_or(0, |config| config.retries_when_blocked());
        let mut attempts = 1;
        loop {
            let mut error = match self.try_fetch_captions(video_id).await {
                Err(Error::CouldNotRetrieveTranscript(error)) => error,
                other => return other,
            };
            if let Some(proxy) = error.reason.blocked_by_proxy_mut() {
                // `retries_when_blocked` counts every try, the first included.
                if attempts < retries {
                    attempts += 1;
                    event!(
                        info,
                        video_id,
                        attempt = attempts,
                        of = retries,
                        "YouTube blocked the request; retrying through the proxy",
                    );
                    continue;
                }
                *proxy = self.proxy_config.as_ref().map(|config| config.kind());
            }
            return Err(error.into());
        }
    }

    /// One try at the captions: the watch page, then the player response.
    async fn try_fetch_captions(&self, video_id: &str) -> Result<Captions, Error> {
        let page = self.fetch_video_html(video_id).await?;
        let api_key = extract_innertube_api_key(&page.html, video_id)?;
        let mut player = self.fetch_innertube_data(video_id, &api_key).await?;
        let details = VideoDetails::from_responses(
            video_id,
            player.video_details.take(),
            innertube::watch_page_microformat(&page.raw),
        );
        Ok((extract_captions(player, video_id)?, details))
    }

    /// The watch page, accepting the cookie consent page first if YouTube
    /// shows it.
    async fn fetch_video_html(&self, video_id: &str) -> Result<WatchPage, Error> {
        let page = self.fetch_html(video_id).await?;
        if !page.html.contains(CONSENT_FORM) {
            return Ok(page);
        }
        event!(debug, video_id, "accepting YouTube's cookie consent page");
        self.create_consent_cookie(&page.html, video_id)?;
        let retried = self.fetch_html(video_id).await?;
        if retried.html.contains(CONSENT_FORM) {
            return Err(CouldNotRetrieveTranscript::new(
                video_id,
                Reason::FailedToCreateConsentCookie,
            )
            .into());
        }
        Ok(retried)
    }

    /// Sets the `CONSENT` cookie from the consent form's value.
    fn create_consent_cookie(&self, html: &str, video_id: &str) -> Result<(), Error> {
        let Some((_, value)) = regex_captures!(r#"name="v" value="(.*?)""#, html) else {
            return Err(CouldNotRetrieveTranscript::new(
                video_id,
                Reason::FailedToCreateConsentCookie,
            )
            .into());
        };
        // The cookie the consent form's "Accept all" sets.
        self.session
            .set_cookie("CONSENT", &format!("YES+{value}"), ".youtube.com");
        Ok(())
    }

    /// The watch page for `video_id`.
    async fn fetch_html(&self, video_id: &str) -> Result<WatchPage, Error> {
        let url = watch_url(video_id);
        let response = self.session.get(&url).await?;
        let raw = raise_http_errors(response, video_id)?.body;
        let html = htmlize::unescape(&raw).into_owned();
        Ok(WatchPage { raw, html })
    }

    /// The InnerTube player response for `video_id`.
    async fn fetch_innertube_data(
        &self,
        video_id: &str,
        api_key: &str,
    ) -> Result<PlayerResponse, Error> {
        let url = innertube_api_url(api_key);
        let body = json!({
            "context": {
                "client": {
                    "clientName": INNERTUBE_CLIENT_NAME,
                    "clientVersion": INNERTUBE_CLIENT_VERSION,
                }
            },
            "videoId": video_id,
        });
        let response = self.session.post_json(&url, &body).await?;
        let text = raise_http_errors(response, video_id)?.body;
        serde_json::from_str(&text).map_err(|e| UnexpectedData::InvalidJson(Box::new(e)).into())
    }
}

/// A video's caption list, and its details if YouTube sent them.
type Captions = (TracklistRenderer, Option<VideoDetails>);

/// A watch page, as sent and HTML-unescaped.
struct WatchPage {
    /// As sent, for the JSON embedded in it.
    raw: String,
    /// HTML-unescaped; the API key and the consent form come from this one.
    html: String,
}

/// The InnerTube API key in the watch page; a captcha means YouTube is
/// blocking the IP.
fn extract_innertube_api_key(html: &str, video_id: &str) -> Result<String, Error> {
    if let Some((_, api_key)) =
        regex_captures!(r#""INNERTUBE_API_KEY":\s*"([a-zA-Z0-9_-]+)""#, html)
    {
        return Ok(api_key.to_owned());
    }
    let reason = if html.contains("class=\"g-recaptcha\"") {
        Reason::IpBlocked { proxy: None }
    } else {
        Reason::YouTubeDataUnparsable
    };
    Err(CouldNotRetrieveTranscript::new(video_id, reason).into())
}

/// The track list, once the video turns out playable.
fn extract_captions(player: PlayerResponse, video_id: &str) -> Result<TracklistRenderer, Error> {
    assert_playability(player.playability_status, video_id)?;
    match player
        .captions
        .and_then(|captions| captions.player_captions_tracklist_renderer)
    {
        Some(renderer) if !matches!(renderer.caption_tracks, Tracks::Missing) => Ok(renderer),
        _ => Err(CouldNotRetrieveTranscript::new(video_id, Reason::TranscriptsDisabled).into()),
    }
}

/// Maps a status other than `OK` to the [`Reason`] it means.
fn assert_playability(status: Option<PlayabilityStatus>, video_id: &str) -> Result<(), Error> {
    let Some(status_data) = status else {
        return Err(unexpected("playabilityStatus"));
    };
    // Not "OK" and not missing; a non-string status also counts as not OK.
    let status_text = match &status_data.status {
        None => return Ok(()),
        Some(Status::Text(text)) if text == PLAYABILITY_OK => return Ok(()),
        Some(Status::Text(text)) => text.as_str(),
        Some(Status::Other(_)) => "",
    };
    let reason = status_data.reason.as_deref();
    let fail = |why: Reason| -> Result<(), Error> {
        Err(CouldNotRetrieveTranscript::new(video_id, why).into())
    };
    if status_text == PLAYABILITY_LOGIN_REQUIRED {
        if reason == Some(REASON_BOT_DETECTED) {
            return fail(Reason::RequestBlocked { proxy: None });
        }
        if reason == Some(REASON_AGE_RESTRICTED) {
            return fail(Reason::AgeRestricted);
        }
    }
    if status_text == PLAYABILITY_ERROR && reason == Some(REASON_VIDEO_UNAVAILABLE) {
        if video_id.starts_with("http://") || video_id.starts_with("https://") {
            return fail(Reason::InvalidVideoId);
        }
        return fail(Reason::VideoUnavailable);
    }
    let sub_reasons = status_data
        .error_screen
        .as_ref()
        .and_then(|screen| screen.player_error_message_renderer.as_ref())
        .and_then(|message| message.subreason.as_ref())
        .map(innertube::Text::run_texts)
        .unwrap_or_default();
    fail(Reason::VideoUnplayable {
        reason: reason.map(str::to_owned),
        sub_reasons,
    })
}
