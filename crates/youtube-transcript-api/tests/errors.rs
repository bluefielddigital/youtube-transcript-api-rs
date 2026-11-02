//! Every error message. Where youtube-transcript-api shares the wording, the
//! tests compare it byte for byte with the reference messages in
//! `tests/golden/errors`, which `scripts/record_golden.py` records. Advice that
//! points somewhere (where to report a bug, how to work around blocks) is
//! this project's own, so it links here.

use std::path::PathBuf;

use youtube_transcript_api::proxies::ProxyKind;
use youtube_transcript_api::{CouldNotRetrieveTranscript, Reason};

/// Reads a reference message from `tests/golden/errors`.
fn golden(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        "errors",
        name,
    ]
    .iter()
    .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| unreachable!("{}: {e}", path.display()))
}

/// The paragraph every message ends with: where to report a problem.
const REPORT: &str = "\n\nIf you are sure that the described cause is not responsible for \
this error and that a transcript should be retrievable, please create an issue at \
https://github.com/bluefielddigital/youtube-transcript-api-rs/issues. Please add which version \
of youtube-transcript-api-rs you are using (`youtube-transcript-api --version`, or the crate \
version in your Cargo.lock) and provide the information needed to replicate the error. Also \
make sure that there are no open issues which already describe your problem!";

/// The full message for `reason`, for video `abc`.
fn message(reason: Reason) -> String {
    CouldNotRetrieveTranscript::new("abc", reason).to_string()
}

/// A message without its closing report paragraph.
fn without_report(text: &str) -> &str {
    text.split_once("\n\nIf you are sure that the described cause")
        .map_or(text, |(before, _)| before)
}

/// Each blocked reason with each proxy kind, by golden filename.
fn blocked_cases() -> Vec<(&'static str, Reason)> {
    let blocked = |proxy| Reason::RequestBlocked { proxy };
    let ip_blocked = |proxy| Reason::IpBlocked { proxy };
    vec![
        ("request_blocked", blocked(None)),
        ("request_blocked_generic", blocked(Some(ProxyKind::Generic))),
        (
            "request_blocked_webshare",
            blocked(Some(ProxyKind::Webshare)),
        ),
        ("ip_blocked", ip_blocked(None)),
        ("ip_blocked_generic", ip_blocked(Some(ProxyKind::Generic))),
        ("ip_blocked_webshare", ip_blocked(Some(ProxyKind::Webshare))),
    ]
}

/// Messages whose wording, up to the report paragraph, is the reference's.
fn shared_cases() -> Vec<(&'static str, Reason)> {
    vec![
        ("youtube_data_unparsable", Reason::YouTubeDataUnparsable),
        (
            "youtube_request_failed",
            Reason::YouTubeRequestFailed {
                reason:
                    "500 Server Error: Internal Server Error for url: https://www.youtube.com/x"
                        .to_owned(),
            },
        ),
        (
            "video_unplayable",
            Reason::VideoUnplayable {
                reason: Some("R".to_owned()),
                sub_reasons: vec!["a".to_owned(), "b".to_owned()],
            },
        ),
        (
            "video_unplayable_no_reason",
            Reason::VideoUnplayable {
                reason: None,
                sub_reasons: Vec::new(),
            },
        ),
        ("video_unavailable", Reason::VideoUnavailable),
        ("transcripts_disabled", Reason::TranscriptsDisabled),
        ("not_translatable", Reason::NotTranslatable),
        (
            "translation_language_not_available",
            Reason::TranslationLanguageNotAvailable,
        ),
        (
            "failed_to_create_consent_cookie",
            Reason::FailedToCreateConsentCookie,
        ),
        ("po_token_required", Reason::PoTokenRequired),
    ]
}

/// Every reason the tests build, once each.
fn all_reasons() -> Vec<Reason> {
    let mut reasons: Vec<Reason> = shared_cases()
        .into_iter()
        .chain(blocked_cases())
        .map(|(_, reason)| reason)
        .collect();
    reasons.extend([
        Reason::InvalidVideoId,
        Reason::AgeRestricted,
        Reason::NoTranscriptFound {
            requested_language_codes: vec!["en".to_owned()],
            transcript_data: "DATA".to_owned(),
        },
    ]);
    reasons
}

/// The shared wording matches the reference byte for byte.
#[test]
fn shared_wording_matches_reference() {
    for (name, reason) in shared_cases() {
        let reference = golden(&format!("{name}.txt"));
        assert_eq!(
            without_report(&message(reason)),
            without_report(&reference),
            "{name}"
        );
    }
}

/// Every message ends with this project's report paragraph and mentions nothing of the
/// reference's.
#[test]
fn every_message_ends_by_pointing_to_this_project() {
    for reason in all_reasons() {
        let text = message(reason);
        assert!(text.ends_with(REPORT), "{text}");
        for foreign in ["jdepoix", "referral", "affiliate", "requests using cookies"] {
            assert!(!text.contains(foreign), "{foreign:?} in {text}");
        }
    }
}

/// Blocked messages point to this project's troubleshooting guide, its proxy docs, or
/// Webshare advice.
#[test]
fn blocked_messages_link_to_our_guides() {
    for (name, reason) in blocked_cases() {
        let text = message(reason);
        let guide = if name.ends_with("_generic") {
            "https://github.com/bluefielddigital/youtube-transcript-api-rs#proxies"
        } else if name.ends_with("_webshare") {
            "Residential"
        } else {
            "https://github.com/bluefielddigital/youtube-transcript-api-rs/blob/main/\
             TROUBLESHOOTING.md#blocked-requests"
        };
        assert!(text.contains(guide), "{name}: {text}");
    }
}

/// A custom proxy gets the same message as no proxy.
#[test]
fn a_custom_proxy_gets_the_plain_blocked_message() {
    assert_eq!(
        message(Reason::RequestBlocked {
            proxy: Some(ProxyKind::Custom)
        }),
        message(Reason::RequestBlocked { proxy: None })
    );
}

/// The invalid-ID and age-restricted messages say what to do.
#[test]
fn invalid_id_and_age_restricted_explain_themselves() {
    assert!(message(Reason::InvalidVideoId).contains(r#"pass "1234""#));
    assert!(message(Reason::AgeRestricted).contains("signing in isn't supported"));
}

/// `NoTranscriptFound` lists the codes in Rust's debug format.
#[test]
fn no_transcript_found_lists_codes_with_debug_formatting() {
    // Rust's `["de", "en"]` rather than the reference's `['de', 'en']`.
    let text = message(Reason::NoTranscriptFound {
        requested_language_codes: vec!["de".to_owned(), "en".to_owned()],
        transcript_data: "DATA".to_owned(),
    });
    let reference = golden("no_transcript_found.txt");
    assert_eq!(
        without_report(&text),
        without_report(&reference).replace("['de', 'en']", r#"["de", "en"]"#)
    );
}
