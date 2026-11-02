//! How the built-in session retries: the budget, the wait between tries,
//! and `Retry-After`.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::{Duration, SystemTime};

use super::{HttpResponse, header};

/// Tunes how the client retries a failed request.
///
/// A request retries when a connection fails, when a `GET` fails after
/// sending, and when a `GET` gets HTTP 429. Retries only happen when a
/// proxy config asks for them ([`ProxyConfig::retries_when_blocked`]) or
/// when [`Self::with_retries`] sets a count.
///
/// - **`Retry-After`.** A 429 that names a wait up to
///   [`Self::with_retry_after_cap`] (60 seconds by default) makes the client
///   sleep that long, then retry. The client does not
///   shorten a longer wait to fit, since retrying early only gets another 429. The
///   request fails at once with
///   [`TransportErrorKind::RateLimited`](super::TransportErrorKind::RateLimited),
///   whose [`TransportError::retry_after`](super::TransportError::retry_after)
///   holds the wait the server asked for, so the caller can wait that long
///   and try again.
/// - **Delay between retries.** Any other retry waits a random time between zero and a
///   ceiling that starts at 500 ms and doubles with each retry, up to 30
///   seconds (exponential backoff with full jitter).
///
/// [`ProxyConfig::retries_when_blocked`]: crate::proxies::ProxyConfig::retries_when_blocked
///
/// ```
/// use std::time::Duration;
/// use youtube_transcript_api::YouTubeTranscriptApi;
/// use youtube_transcript_api::http::RetryPolicy;
///
/// let policy = RetryPolicy::new()
///     .with_retries(3)
///     .with_backoff(Duration::from_secs(1), Duration::from_secs(20))
///     .with_retry_after_cap(Duration::from_secs(10));
/// let api = YouTubeTranscriptApi::builder().retry_policy(policy).build()?;
/// # Ok::<(), youtube_transcript_api::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetryPolicy {
    /// Retries per request; `None` takes the proxy config's count.
    pub(super) retries: Option<u32>,
    /// The first backoff ceiling.
    pub(super) backoff_base: Duration,
    /// The largest backoff ceiling.
    pub(super) backoff_max: Duration,
    /// The longest `Retry-After` worth waiting for.
    pub(super) retry_after_cap: Duration,
}

impl RetryPolicy {
    /// The default backoff ceiling for the first retry.
    pub const DEFAULT_BACKOFF_BASE: Duration = Duration::from_millis(500);
    /// The default largest backoff ceiling.
    pub const DEFAULT_BACKOFF_MAX: Duration = Duration::from_secs(30);
    /// The default longest `Retry-After` the client waits for.
    pub const DEFAULT_RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

    /// The defaults: the proxy config's retry count, a backoff from 500 ms
    /// to 30 s, and a 60 s cap on `Retry-After`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            retries: None,
            backoff_base: Self::DEFAULT_BACKOFF_BASE,
            backoff_max: Self::DEFAULT_BACKOFF_MAX,
            retry_after_cap: Self::DEFAULT_RETRY_AFTER_CAP,
        }
    }

    /// Retries each request this many times, instead of the proxy config's
    /// count. The proxy config's [`retries_when_blocked`] still sets how many
    /// tries a fetch gets while YouTube blocks it.
    ///
    /// [`retries_when_blocked`]: crate::proxies::ProxyConfig::retries_when_blocked
    #[must_use]
    pub const fn with_retries(mut self, retries: u32) -> Self {
        self.retries = Some(retries);
        self
    }

    /// Waits up to `base` before the first retry, doubling the ceiling with
    /// each retry up to `max`. Zero for both turns the waiting off.
    #[must_use]
    pub const fn with_backoff(mut self, base: Duration, max: Duration) -> Self {
        self.backoff_base = base;
        self.backoff_max = max;
        self
    }

    /// Honours a `Retry-After` up to `cap`; a longer one fails the request
    /// with its wait in the error (see the type's docs).
    #[must_use]
    pub const fn with_retry_after_cap(mut self, cap: Duration) -> Self {
        self.retry_after_cap = cap;
        self
    }

    /// The waits between `retries` retries.
    pub(super) const fn backoff(&self, retries: usize) -> FullJitter {
        FullJitter {
            ceiling: self.backoff_base,
            max: self.backoff_max,
            left: retries,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponential backoff with full jitter: each wait is random between zero
/// and a ceiling that doubles after each one.
#[derive(Debug)]
pub(super) struct FullJitter {
    /// The ceiling for the next wait.
    ceiling: Duration,
    /// The ceiling never passes this.
    max: Duration,
    /// Waits left.
    left: usize,
}

impl Iterator for FullJitter {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        self.left = self.left.checked_sub(1)?;
        let wait = jittered(self.ceiling.min(self.max), random_unit());
        self.ceiling = self.ceiling.saturating_mul(2).min(self.max);
        Some(wait)
    }
}

/// `ceiling` scaled by `unit`, a number from 0 to 1.
fn jittered(ceiling: Duration, unit: f64) -> Duration {
    ceiling.mul_f64(unit.clamp(0.0, 1.0))
}

/// A random number from 0 to 1. The standard library seeds each
/// `RandomState` randomly, which is all the jitter needs.
fn random_unit() -> f64 {
    let bits = RandomState::new().hash_one(0_u8) >> 32;
    f64::from(u32::try_from(bits).unwrap_or(u32::MAX)) / f64::from(u32::MAX)
}

/// A 429's `Retry-After`: seconds, or an HTTP date.
pub(super) fn retry_after(response: &HttpResponse) -> Option<Duration> {
    let value = response
        .headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let when = httpdate::parse_http_date(value).ok()?;
    Some(
        when.duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

/// The backoff, the policy's defaults and `Retry-After`.
#[cfg(test)]
mod tests {
    use super::{FullJitter, RetryPolicy, jittered, random_unit, retry_after};
    use crate::http::{HeaderMap, HeaderValue, HttpResponse, StatusCode, header};
    use std::time::Duration;

    /// A 429 response with `Retry-After: value`.
    fn with_retry_after(value: &'static str) -> HttpResponse {
        let mut headers = HeaderMap::new();
        headers.insert(header::RETRY_AFTER, HeaderValue::from_static(value));
        HttpResponse::new(
            StatusCode::TOO_MANY_REQUESTS,
            String::new(),
            headers,
            String::new(),
        )
    }

    /// `Retry-After` in seconds.
    #[test]
    fn retry_after_seconds() {
        assert_eq!(
            retry_after(&with_retry_after("3")),
            Some(Duration::from_secs(3))
        );
    }

    /// A `Retry-After` date in the past means no wait.
    #[test]
    fn retry_after_past_date_is_zero() {
        assert_eq!(
            retry_after(&with_retry_after("Wed, 21 Oct 2015 07:28:00 GMT")),
            Some(Duration::ZERO)
        );
    }

    /// A missing or unreadable `Retry-After` gives no wait.
    #[test]
    fn retry_after_garbage_is_ignored() {
        assert_eq!(retry_after(&with_retry_after("soon")), None);
        let mut missing = with_retry_after("1");
        missing.headers.clear();
        assert_eq!(retry_after(&missing), None);
        let mut response = with_retry_after("1");
        response.headers.insert(
            header::RETRY_AFTER,
            HeaderValue::from_bytes(b"\xff").unwrap(),
        );
        assert_eq!(retry_after(&response), None);
    }

    /// The defaults match the documented numbers.
    #[test]
    fn defaults() {
        let policy = RetryPolicy::default();
        assert_eq!(policy, RetryPolicy::new());
        assert_eq!(policy.retries, None);
        assert_eq!(policy.backoff_base, Duration::from_millis(500));
        assert_eq!(policy.backoff_max, Duration::from_secs(30));
        assert_eq!(policy.retry_after_cap, Duration::from_secs(60));
    }

    /// Each setter changes only its own field.
    #[test]
    fn setters() {
        let policy = RetryPolicy::new()
            .with_retries(4)
            .with_backoff(Duration::from_secs(1), Duration::from_secs(2))
            .with_retry_after_cap(Duration::from_secs(3));
        assert_eq!(policy.retries, Some(4));
        assert_eq!(policy.backoff_base, Duration::from_secs(1));
        assert_eq!(policy.backoff_max, Duration::from_secs(2));
        assert_eq!(policy.retry_after_cap, Duration::from_secs(3));
    }

    /// The scale clamps to the ceiling.
    #[test]
    fn jitter_scales_the_ceiling() {
        let ceiling = Duration::from_secs(10);
        assert_eq!(jittered(ceiling, 0.0), Duration::ZERO);
        assert_eq!(jittered(ceiling, 0.5), Duration::from_secs(5));
        assert_eq!(jittered(ceiling, 7.0), ceiling);
        assert_eq!(jittered(ceiling, -1.0), Duration::ZERO);
    }

    /// The random number stays in range and varies.
    #[test]
    fn random_unit_is_in_range() {
        let draws: Vec<f64> = (0..64).map(|_| random_unit()).collect();
        assert!(draws.iter().all(|unit| (0.0..=1.0).contains(unit)));
        assert!(
            draws
                .windows(2)
                .any(|pair| pair[0].to_bits() != pair[1].to_bits())
        );
    }

    /// The backoff yields one wait per retry, each under a ceiling that
    /// doubles from the base up to the limit.
    #[test]
    fn backoff_ceilings_double_up_to_the_maximum() {
        let policy = RetryPolicy::new();
        let mut ceiling = Duration::from_millis(500);
        let mut backoff: FullJitter = policy.backoff(8);
        for _ in 0..8 {
            let wait = backoff.next().unwrap();
            assert!(wait <= ceiling, "{wait:?} over {ceiling:?}");
            ceiling = (ceiling * 2).min(Duration::from_secs(30));
        }
        assert_eq!(backoff.next(), None);
        assert_eq!(policy.backoff(0).next(), None);
    }

    /// A zero backoff never waits.
    #[test]
    fn zero_backoff_never_waits() {
        let policy = RetryPolicy::new().with_backoff(Duration::ZERO, Duration::ZERO);
        assert!(policy.backoff(5).all(|wait| wait.is_zero()));
    }
}
