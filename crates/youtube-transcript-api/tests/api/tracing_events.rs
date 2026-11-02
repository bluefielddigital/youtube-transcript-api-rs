use std::cell::RefCell;
use std::fmt::{self, Write as _};
use std::sync::{Arc, OnceLock};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};
use youtube_transcript_api::http::{Method, TransportErrorKind};
use youtube_transcript_api::proxies::WebshareProxyConfig;

use youtube_transcript_api::DEFAULT_LANGUAGES;

use crate::common::{Canned, FakeYouTube, PLAYER, WATCH, fixture};
use crate::{api, api_with_proxy};

thread_local! {
    /// The events this thread emitted, as `LEVEL target: field=value ...`.
    static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The global subscriber. A thread-scoped one (`with_default`) races
/// with tests on other threads over each callsite's cached interest and
/// loses events; a global one that files each event under the thread
/// that emitted it doesn't.
#[derive(Debug)]
struct Recorder;

/// Runs `future` on this thread and returns its output and its events.
fn record<F: Future>(future: F) -> (F::Output, Vec<String>) {
    /// Holds a value once `Recorder` becomes the global subscriber.
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        tracing::subscriber::set_global_default(Recorder).unwrap_or_else(|e| unreachable!("{e}"));
    });
    LINES.with_borrow_mut(Vec::clear);
    let output = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap_or_else(|e| unreachable!("{e}"))
        .block_on(future);
    (output, LINES.with_borrow_mut(std::mem::take))
}

/// Collects an event's fields as `name=value` text after its level and target.
struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        write!(self.0, " {}={value:?}", field.name()).unwrap_or_else(|e| unreachable!("{e}"));
    }
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let metadata = event.metadata();
        let mut fields = Fields(format!("{} {}:", metadata.level(), metadata.target()));
        event.record(&mut fields);
        LINES.with_borrow_mut(|lines| lines.push(fields.0));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// Each request and the consent page acceptance emit a `DEBUG` event with the method, URL and
/// status.
#[test]
fn requests_and_the_consent_page_are_events() {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube_consent_page.html.static")),
    );
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube.html.static")),
    );
    let (fetched, lines) = record(api(&fake).fetch("F1xioXWb8CY", &DEFAULT_LANGUAGES, false));
    fetched.unwrap();

    assert_eq!(lines.len(), 5, "{lines:#?}");
    assert!(
        lines[0].starts_with(
            "DEBUG youtube_transcript_api::http: message=request method=GET \
             url=\"https://www.youtube.com/watch?v=F1xioXWb8CY\" status=200"
        ),
        "{lines:#?}"
    );
    assert_eq!(
        lines[1],
        "DEBUG youtube_transcript_api::transcripts::fetcher: message=accepting YouTube's cookie \
         consent page video_id=\"F1xioXWb8CY\""
    );
    assert!(lines[4].contains("url=\"https://www.youtube.com/api/timedtext"));
}

/// A YouTube that refuses a connection, answers 429, then serves the
/// watch page, and blocks the first player request.
fn flaky_youtube() -> Arc<FakeYouTube> {
    let fake = FakeYouTube::new();
    fake.replace(
        &Method::GET,
        WATCH,
        Canned::failure(TransportErrorKind::Connect),
    );
    fake.add(&Method::GET, WATCH, Canned::status(429));
    fake.add(
        &Method::GET,
        WATCH,
        Canned::ok(fixture("youtube.html.static")),
    );
    let blocked = Canned::ok(fixture("youtube_request_blocked.innertube.json.static"));
    fake.replace(&Method::POST, PLAYER, blocked);
    fake.add(
        &Method::POST,
        PLAYER,
        Canned::ok(fixture("youtube.innertube.json.static")),
    );
    fake
}

/// Each retry emits an `INFO` event with its cause, and failed requests emit their error.
#[test]
fn retries_are_events() {
    let fake = flaky_youtube();
    let proxy = WebshareProxyConfig::new("u", "p").with_retries_when_blocked(3);
    let (fetched, lines) =
        record(api_with_proxy(&fake, proxy).fetch("Njp5uhTorCo", &DEFAULT_LANGUAGES, false));
    fetched.unwrap();

    let infos: Vec<&String> = lines.iter().filter(|l| l.starts_with("INFO")).collect();
    assert_eq!(infos.len(), 3, "{lines:#?}");
    assert!(
        infos[0].contains("message=request failed")
            && infos[0].contains("cause=Connection refused")
            && infos[0].contains("retry_in=Some(0ns)"),
        "{lines:#?}"
    );
    assert!(
        infos[1].contains("cause=HTTP 429 Too Many Requests"),
        "{lines:#?}"
    );
    assert!(
        infos[2].contains("message=YouTube blocked the request; retrying through the proxy")
            && infos[2].contains("attempt=2 of=3"),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("method=GET") && l.contains("error=\"Connection refused")),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("method=POST") && l.contains("status=200")),
        "{lines:#?}"
    );
}
