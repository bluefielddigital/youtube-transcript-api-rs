//! Proxy configuration: any HTTP/HTTPS/SOCKS proxy, or Webshare's rotating
//! residential proxies.

use std::fmt;

use percent_encoding::utf8_percent_encode;
use thiserror::Error;

use crate::settings::ESCAPE;

/// `url` with its credentials replaced by asterisks, and its path and query
/// dropped, since they can carry tokens. A URL without credentials keeps its
/// scheme and host.
#[must_use]
pub(crate) fn redact_url(url: &str) -> String {
    let (scheme, rest) = url
        .split_once("://")
        .map_or(("", url), |(scheme, rest)| (scheme, rest));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (userinfo, host) = authority
        .rsplit_once('@')
        .map_or((false, authority), |(_, host)| (true, host));
    let credentials = if userinfo { "***@" } else { "" };
    if scheme.is_empty() {
        format!("{credentials}{host}")
    } else {
        format!("{scheme}://{credentials}{host}")
    }
}

/// Shows a [`ProxyConfig`] in `Debug` output with its credentials hidden,
/// whatever the implementation's own `Debug` prints.
pub(crate) struct RedactedProxy<'a>(pub(crate) &'a dyn ProxyConfig);

impl fmt::Debug for RedactedProxy<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("kind", &self.0.kind())
            .field("urls", &self.0.proxy_urls())
            .field("retries_when_blocked", &self.0.retries_when_blocked())
            .finish()
    }
}

/// An invalid proxy config.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
#[non_exhaustive]
pub struct InvalidProxyConfig(pub String);

/// The proxy URLs used for HTTP and HTTPS requests. `Debug` hides the
/// credentials in them, which show as asterisks before the `@`.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProxyUrls {
    /// Proxy used for `http://` requests.
    pub http: String,
    /// Proxy used for `https://` requests.
    pub https: String,
}

impl ProxyUrls {
    /// The proxies for `http://` and `https://` requests.
    pub fn new(http: impl Into<String>, https: impl Into<String>) -> Self {
        Self {
            http: http.into(),
            https: https.into(),
        }
    }
}

impl fmt::Debug for ProxyUrls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyUrls")
            .field("http", &redact_url(&self.http))
            .field("https", &redact_url(&self.https))
            .finish()
    }
}

/// Which built-in proxy config a [`ProxyConfig`] is. The advice in a
/// blocked request's error message depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProxyKind {
    /// A [`GenericProxyConfig`].
    Generic,
    /// A [`WebshareProxyConfig`].
    Webshare,
    /// Any other implementation of [`ProxyConfig`].
    Custom,
}

/// The base trait for all proxy configs. Anything can be a proxy config, as
/// long as it yields [`ProxyUrls`].
///
/// ```
/// use youtube_transcript_api::YouTubeTranscriptApi;
/// use youtube_transcript_api::proxies::{ProxyConfig, ProxyUrls};
///
/// /// A pool that rotates its IP on every connection.
/// #[derive(Debug)]
/// struct MyPool;
///
/// impl ProxyConfig for MyPool {
///     fn proxy_urls(&self) -> ProxyUrls {
///         ProxyUrls::new("http://user:pass@pool.example:8080", "http://user:pass@pool.example:8080")
///     }
///
///     fn prevent_keeping_connections_alive(&self) -> bool {
///         true
///     }
///
///     fn retries_when_blocked(&self) -> u32 {
///         5
///     }
/// }
///
/// let api = YouTubeTranscriptApi::builder()
///     .proxy_config(MyPool)
///     .build()?;
/// # Ok::<(), youtube_transcript_api::Error>(())
/// ```
pub trait ProxyConfig: fmt::Debug + Send + Sync {
    /// The proxies used for HTTP and HTTPS requests.
    fn proxy_urls(&self) -> ProxyUrls;

    /// Close each connection instead of keeping it alive. With rotating
    /// proxies, a connection that stays open keeps its IP, so only a new one
    /// gets a new IP.
    fn prevent_keeping_connections_alive(&self) -> bool {
        false
    }

    /// How many tries a fetch gets while YouTube blocks it, the first
    /// included; each request also gets this many retries after a 429 or a
    /// failed connection. With rotating residential proxies, each try gets a
    /// new IP, which YouTube may not block.
    fn retries_when_blocked(&self) -> u32 {
        0
    }

    /// Which built-in config this is.
    fn kind(&self) -> ProxyKind {
        ProxyKind::Custom
    }
}

/// Any generic HTTP/HTTPS/SOCKS proxy. With only one of the two URLs, that
/// one serves both kinds of connection. `Debug` hides the credentials in
/// the URLs.
#[derive(Clone, PartialEq, Eq)]
pub struct GenericProxyConfig {
    /// The HTTP proxy URL.
    http_url: Option<String>,
    /// The HTTPS proxy URL.
    https_url: Option<String>,
}

impl fmt::Debug for GenericProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenericProxyConfig")
            .field("http_url", &self.http_url.as_deref().map(redact_url))
            .field("https_url", &self.https_url.as_deref().map(redact_url))
            .finish()
    }
}

impl GenericProxyConfig {
    /// Creates the config. At least one of the two URLs must be non-empty.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidProxyConfig`] if both URLs are missing or empty.
    pub fn new(http: Option<String>, https: Option<String>) -> Result<Self, InvalidProxyConfig> {
        if is_empty(http.as_deref()) && is_empty(https.as_deref()) {
            return Err(InvalidProxyConfig(
                "GenericProxyConfig requires you to define at least one of the two: http or https"
                    .to_owned(),
            ));
        }
        Ok(Self {
            http_url: http,
            https_url: https,
        })
    }

    /// The configured HTTP proxy URL.
    #[must_use]
    pub fn http_url(&self) -> Option<&str> {
        self.http_url.as_deref()
    }

    /// The configured HTTPS proxy URL.
    #[must_use]
    pub fn https_url(&self) -> Option<&str> {
        self.https_url.as_deref()
    }
}

/// The first of two optional strings that's non-empty.
fn first_truthy(first: Option<&str>, second: Option<&str>) -> String {
    first
        .filter(|s| !s.is_empty())
        .or(second)
        .unwrap_or_default()
        .to_owned()
}

/// Whether `url` is missing or empty.
const fn is_empty(url: Option<&str>) -> bool {
    match url {
        Some(s) => s.is_empty(),
        None => true,
    }
}

impl ProxyConfig for GenericProxyConfig {
    fn proxy_urls(&self) -> ProxyUrls {
        ProxyUrls::new(
            first_truthy(self.http_url(), self.https_url()),
            first_truthy(self.https_url(), self.http_url()),
        )
    }

    fn kind(&self) -> ProxyKind {
        ProxyKind::Generic
    }
}

/// Rotating residential proxies from Webshare.
///
/// This is the most reliable way to work around YouTube blocking your IP.
/// Sign up at <https://www.webshare.io/> and buy a
/// **Residential** package; **Proxy Server** and **Static Residential**
/// don't work.
#[derive(Clone, PartialEq, Eq)]
pub struct WebshareProxyConfig {
    /// "Proxy Username" from <https://dashboard.webshare.io/proxy/settings>.
    pub proxy_username: String,
    /// "Proxy Password" from <https://dashboard.webshare.io/proxy/settings>.
    pub proxy_password: String,
    /// Proxy host.
    pub domain_name: String,
    /// Proxy port.
    pub proxy_port: u16,
    /// Countries to rotate through.
    filter_ip_locations: Vec<String>,
    /// See [`ProxyConfig::retries_when_blocked`].
    retries_when_blocked: u32,
}

impl fmt::Debug for WebshareProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebshareProxyConfig")
            .field("proxy_username", &self.proxy_username)
            .field("proxy_password", &"<redacted>")
            .field("domain_name", &self.domain_name)
            .field("proxy_port", &self.proxy_port)
            .field("filter_ip_locations", &self.filter_ip_locations)
            .field("retries_when_blocked", &self.retries_when_blocked)
            .finish()
    }
}

impl WebshareProxyConfig {
    /// Default proxy host.
    pub const DEFAULT_DOMAIN_NAME: &str = "p.webshare.io";
    /// Default proxy port.
    pub const DEFAULT_PORT: u16 = 80;
    /// The default for [`ProxyConfig::retries_when_blocked`].
    pub const DEFAULT_RETRIES_WHEN_BLOCKED: u32 = 10;

    /// Creates a config with the default host, port, and retry count.
    #[must_use]
    pub fn new(proxy_username: impl Into<String>, proxy_password: impl Into<String>) -> Self {
        Self {
            proxy_username: proxy_username.into(),
            proxy_password: proxy_password.into(),
            domain_name: Self::DEFAULT_DOMAIN_NAME.to_owned(),
            proxy_port: Self::DEFAULT_PORT,
            filter_ip_locations: Vec::new(),
            retries_when_blocked: Self::DEFAULT_RETRIES_WHEN_BLOCKED,
        }
    }

    /// Only rotate through IPs in these countries (location codes, for example `de`).
    #[must_use]
    pub fn with_filter_ip_locations<I>(mut self, locations: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.filter_ip_locations = locations.into_iter().map(Into::into).collect();
        self
    }

    /// Sets [`ProxyConfig::retries_when_blocked`].
    #[must_use]
    pub const fn with_retries_when_blocked(mut self, retries: u32) -> Self {
        self.retries_when_blocked = retries;
        self
    }

    /// Uses a different proxy host.
    #[must_use]
    pub fn with_domain_name(mut self, domain_name: impl Into<String>) -> Self {
        self.domain_name = domain_name.into();
        self
    }

    /// Uses a different proxy port.
    #[must_use]
    pub const fn with_proxy_port(mut self, port: u16) -> Self {
        self.proxy_port = port;
        self
    }

    /// The rotating proxy URL, with the location filter in the username.
    /// The username and password are percent-encoded.
    #[must_use]
    pub fn url(&self) -> String {
        const SUFFIX: &str = "-rotate";
        let location_codes =
            self.filter_ip_locations
                .iter()
                .fold(String::new(), |mut codes, code| {
                    codes.push('-');
                    codes.push_str(&code.to_uppercase());
                    codes
                });
        // Webshare reads `user-DE-US-rotate`: the location codes go before
        // the suffix, which a username may already carry.
        let username = self
            .proxy_username
            .strip_suffix(SUFFIX)
            .unwrap_or(&self.proxy_username);
        // Escaped, so a password with `@` or `/` can't change the host.
        let userinfo = format!(
            "{}:{}",
            utf8_percent_encode(&format!("{username}{location_codes}{SUFFIX}"), ESCAPE),
            utf8_percent_encode(&self.proxy_password, ESCAPE),
        );
        format!(
            "http://{userinfo}@{}:{}/",
            self.domain_name, self.proxy_port
        )
    }
}

impl ProxyConfig for WebshareProxyConfig {
    fn proxy_urls(&self) -> ProxyUrls {
        let url = self.url();
        ProxyUrls::new(url.clone(), url)
    }

    fn prevent_keeping_connections_alive(&self) -> bool {
        true
    }

    fn retries_when_blocked(&self) -> u32 {
        self.retries_when_blocked
    }

    fn kind(&self) -> ProxyKind {
        ProxyKind::Webshare
    }
}

/// Proxy configs.
#[cfg(test)]
mod tests {
    use super::{
        GenericProxyConfig, ProxyConfig, ProxyUrls, RedactedProxy, WebshareProxyConfig, redact_url,
    };

    /// A generic proxy needs at least one URL.
    #[test]
    fn generic_requires_a_url() {
        assert!(GenericProxyConfig::new(None, None).is_err());
        assert!(GenericProxyConfig::new(Some(String::new()), Some(String::new())).is_err());
    }

    /// A generic proxy with one URL uses it for both.
    #[test]
    fn generic_falls_back_to_the_other_url() {
        let only_http = GenericProxyConfig::new(Some("http://a".into()), None).unwrap();
        let urls = only_http.proxy_urls();
        assert_eq!(urls.http, "http://a");
        assert_eq!(urls.https, "http://a");

        let only_https = GenericProxyConfig::new(Some(String::new()), Some("http://b".into()))
            .unwrap()
            .proxy_urls();
        assert_eq!(only_https.http, "http://b");
        assert_eq!(only_https.https, "http://b");
    }

    /// The Webshare URL, with the rotating username.
    #[test]
    fn webshare_url() {
        let config =
            WebshareProxyConfig::new("user", "pass").with_filter_ip_locations(["de", "us"]);
        assert_eq!(
            config.url(),
            "http://user-DE-US-rotate:pass@p.webshare.io:80/"
        );
        assert!(config.prevent_keeping_connections_alive());
        assert_eq!(config.retries_when_blocked(), 10);
    }

    /// Webshare with a custom host and port.
    #[test]
    fn webshare_custom_host_and_port() {
        let config = WebshareProxyConfig::new("user", "pass")
            .with_domain_name("proxy.example")
            .with_proxy_port(8080);
        let urls = config.proxy_urls();
        assert_eq!(urls.http, "http://user-rotate:pass@proxy.example:8080/");
        assert_eq!(urls.https, urls.http);
    }

    /// A custom config that sets only the URLs.
    #[derive(Debug)]
    struct Custom;

    impl ProxyConfig for Custom {
        /// Both URLs point at one test proxy.
        fn proxy_urls(&self) -> ProxyUrls {
            ProxyUrls::new("http://c", "http://c")
        }
    }

    /// A custom config gets the trait's defaults.
    #[test]
    fn custom_config_defaults() {
        assert_eq!(Custom.kind(), super::ProxyKind::Custom);
        assert!(!Custom.prevent_keeping_connections_alive());
        assert_eq!(Custom.retries_when_blocked(), 0);
        assert_eq!(Custom.proxy_urls().http, "http://c");
    }

    /// A username already ending in `-rotate` keeps a single suffix.
    #[test]
    fn webshare_does_not_double_the_rotate_suffix() {
        let config = WebshareProxyConfig::new("user-rotate", "pass");
        assert_eq!(config.url(), "http://user-rotate:pass@p.webshare.io:80/");
    }

    /// `Debug` hides the Webshare password.
    #[test]
    fn webshare_debug_hides_the_password() {
        let config = WebshareProxyConfig::new("user", "secret");
        assert!(!format!("{config:?}").contains("secret"));
    }

    /// The credentials escape reserved characters and non-ASCII text, so the URL still splits into the same parts.
    #[test]
    fn webshare_url_percent_encodes_the_credentials() {
        let config =
            WebshareProxyConfig::new("us@er:/#", "p@ss:w/rd#?%41 ünï").with_proxy_port(8080);
        let url = config.url();
        assert_eq!(
            url,
            "http://us%40er%3A%2F%23-rotate:p%40ss%3Aw%2Frd%23%3F%2541%20%C3%BCn%C3%AF@p.webshare.io:8080/"
        );
        let parsed = reqwest::Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("p.webshare.io"));
        assert_eq!(parsed.port(), Some(8080));
        assert_eq!(parsed.path(), "/");
        assert_eq!(parsed.fragment(), None);
        assert_eq!(parsed.username(), "us%40er%3A%2F%23-rotate");
    }

    /// `redact_url` hides the credentials, the path, and the query.
    #[test]
    fn redact_url_hides_userinfo() {
        assert_eq!(
            redact_url("http://user:secret@host:8080/path?token=1"),
            "http://***@host:8080"
        );
        // A password holding a raw `@` still redacts the whole password.
        assert_eq!(
            redact_url("socks5h://u:p@ss@host:1"),
            "socks5h://***@host:1"
        );
        assert_eq!(redact_url("host:1?x=u:p@y"), "host:1");
        assert_eq!(redact_url("http://host:8080/"), "http://host:8080");
        assert_eq!(redact_url("user:secret@host:80"), "***@host:80");
        assert_eq!(redact_url("not a url"), "not a url");
    }

    /// No `Debug` output of a proxy type shows the password.
    #[test]
    fn debug_never_shows_the_password() {
        let generic = GenericProxyConfig::new(
            Some("http://user:hunter2@a:1".to_owned()),
            Some("https://user:hunter2@b:2/x".to_owned()),
        )
        .unwrap();
        let urls = ProxyUrls::new("http://user:hunter2@a:1", "http://user:hunter2@b:2");
        let webshare = WebshareProxyConfig::new("user", "hunter2");
        let shown = [
            format!("{generic:?}"),
            format!("{generic:#?}"),
            format!("{urls:?}"),
            format!("{:?}", RedactedProxy(&generic)),
            format!("{:?}", RedactedProxy(&webshare)),
            format!("{:?}", RedactedProxy(&Leaky)),
        ];
        for text in &shown {
            assert!(!text.contains("hunter2"), "{text}");
        }
        assert!(shown[0].contains("http://***@a:1"));
        assert!(shown[2].contains("http://***@b:2"));
        assert!(shown[3].contains("Generic"));
    }

    /// A custom config whose URLs carry credentials.
    #[derive(Debug)]
    struct Leaky;

    impl ProxyConfig for Leaky {
        /// A URL with credentials.
        fn proxy_urls(&self) -> ProxyUrls {
            ProxyUrls::new("http://user:hunter2@c:3", "http://user:hunter2@c:3")
        }
    }
}
