use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::errors::Error;
use crate::http::{HttpClient, ReqwestClient, RetryPolicy, Session};
use crate::proxies::{ProxyConfig, RedactedProxy};
use crate::transcripts::{FetchedTranscript, TranscriptList, TranscriptListFetcher};

/// The default language list: English.
pub const DEFAULT_LANGUAGES: [&str; 1] = ["en"];

/// Fetches transcripts for YouTube videos.
///
/// ```no_run
/// # async fn run() -> Result<(), youtube_transcript_api::Error> {
/// use youtube_transcript_api::YouTubeTranscriptApi;
///
/// let api = YouTubeTranscriptApi::new()?;
/// let transcript = api.fetch("GJLlxj_dtq8", &["de", "en"], false).await?;
/// for snippet in &transcript {
///     println!("{:>7.2}  {}", snippet.start, snippet.text);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct YouTubeTranscriptApi {
    /// Fetches transcript lists.
    fetcher: TranscriptListFetcher,
    /// Whether lists match regional language variants.
    match_language_variants: bool,
}

impl YouTubeTranscriptApi {
    /// A client without proxies.
    ///
    /// # Errors
    ///
    /// Fails only if building the HTTP client fails.
    pub fn new() -> Result<Self, Error> {
        Self::builder().build()
    }

    /// Configure proxies or a custom HTTP client.
    #[must_use]
    pub fn builder() -> YouTubeTranscriptApiBuilder {
        YouTubeTranscriptApiBuilder::default()
    }

    /// Fetches the transcript in the first of `languages` that has one.
    /// Shortcut for `list(video_id)?.find_transcript(languages)?.fetch(..)`.
    ///
    /// `video_id` is a video ID or a YouTube URL (see [`crate::video_id`]).
    /// `preserve_formatting` keeps a few HTML formatting tags such as `<i>`.
    ///
    /// # Errors
    ///
    /// Any [`Error`]; most are [`Error::CouldNotRetrieveTranscript`].
    pub async fn fetch<S: AsRef<str> + Sync>(
        &self,
        video_id: &str,
        languages: &[S],
        preserve_formatting: bool,
    ) -> Result<FetchedTranscript, Error> {
        self.list(video_id)
            .await?
            .find_transcript(languages)?
            .fetch(preserve_formatting)
            .await
    }

    /// Lists the transcripts available for a video. Pass its ID or its
    /// YouTube URL (see [`crate::video_id`]).
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), youtube_transcript_api::Error> {
    /// use youtube_transcript_api::YouTubeTranscriptApi;
    ///
    /// let api = YouTubeTranscriptApi::new()?;
    /// let list = api.list("https://youtu.be/GJLlxj_dtq8").await?;
    /// for transcript in list.iter() {
    ///     println!(
    ///         "{} ({}){}",
    ///         transcript.language,
    ///         transcript.language_code,
    ///         if transcript.is_generated { ", generated" } else { "" },
    ///     );
    /// }
    /// if let Some(details) = list.video_details() {
    ///     println!("{:?} by {:?}", details.title, details.author);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Any [`Error`]; most are [`Error::CouldNotRetrieveTranscript`].
    pub async fn list(&self, video_id: &str) -> Result<TranscriptList, Error> {
        let list = self.fetcher.fetch(&crate::video_id(video_id)).await?;
        Ok(list.match_language_variants(self.match_language_variants))
    }
}

/// Builds a [`YouTubeTranscriptApi`]. `Debug` hides the credentials in the
/// proxy URLs.
#[derive(Default)]
pub struct YouTubeTranscriptApiBuilder {
    /// The proxy, if any.
    proxy_config: Option<Arc<dyn ProxyConfig>>,
    /// A custom HTTP client, if any.
    http_client: Option<Arc<dyn HttpClient>>,
    /// The request timeout for the built-in client.
    timeout: Option<Duration>,
    /// Whether lists match regional language variants.
    match_language_variants: bool,
    /// How requests retry.
    retry_policy: RetryPolicy,
    /// The response body limit for the built-in client.
    max_body_bytes: Option<usize>,
}

impl fmt::Debug for YouTubeTranscriptApiBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("YouTubeTranscriptApiBuilder")
            .field(
                "proxy_config",
                &self.proxy_config.as_deref().map(RedactedProxy),
            )
            .field("http_client", &self.http_client)
            .field("timeout", &self.timeout)
            .field("match_language_variants", &self.match_language_variants)
            .field("retry_policy", &self.retry_policy)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

impl YouTubeTranscriptApiBuilder {
    /// Send all requests through a proxy, for example to work around YouTube
    /// blocking your IP.
    #[must_use]
    pub fn proxy_config(mut self, config: impl ProxyConfig + 'static) -> Self {
        self.proxy_config = Some(Arc::new(config));
        self
    }

    /// Give up on a request after `timeout`, from sending it to reading the
    /// whole response. No timeout by default. Applies to the built-in
    /// client; with [`Self::http_client`], timeouts are up to that client.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Tunes retries: how many, the backoff between them, and how long a
    /// `Retry-After` is worth waiting for. See [`RetryPolicy`] for the
    /// defaults and for what happens when a server asks to wait longer. Applies
    /// with any HTTP client.
    #[must_use]
    pub const fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Fails a request when its response body passes `limit` bytes, `16 MiB`
    /// by default ([`ReqwestClient::DEFAULT_MAX_BODY_BYTES`]). Applies to the
    /// built-in client; with [`Self::http_client`], limits are up to that
    /// client.
    #[must_use]
    pub const fn max_body_bytes(mut self, limit: usize) -> Self {
        self.max_body_bytes = Some(limit);
        self
    }

    /// Also match regional variants of the requested language codes: `en`
    /// finds `en-US`, and `en-GB` finds `en`, when there is no exact match.
    /// Off by default. See [`TranscriptList::match_language_variants`].
    #[must_use]
    pub const fn match_language_variants(mut self, on: bool) -> Self {
        self.match_language_variants = on;
        self
    }

    /// Use your own HTTP client. The proxy URLs are then up to that client,
    /// but the proxy's retry and keep-alive settings still apply.
    #[must_use]
    pub fn http_client(mut self, client: Arc<dyn HttpClient>) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidProxyConfig`] if a proxy URL is invalid.
    pub fn build(self) -> Result<YouTubeTranscriptApi, Error> {
        let client: Arc<dyn HttpClient> = if let Some(client) = self.http_client {
            client
        } else {
            let mut client =
                ReqwestClient::with_options(self.proxy_config.as_deref(), self.timeout)?;
            if let Some(limit) = self.max_body_bytes {
                client = client.with_max_body_bytes(limit);
            }
            Arc::new(client)
        };
        let session = Session::new(client, self.proxy_config.as_deref(), self.retry_policy);
        Ok(YouTubeTranscriptApi {
            fetcher: TranscriptListFetcher::new(session, self.proxy_config),
            match_language_variants: self.match_language_variants,
        })
    }
}
