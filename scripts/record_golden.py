"""Record output from the Python youtube-transcript-api into the library's tests/golden.

The Rust tests compare the library's own text (error messages, transcript
lists, text/SRT/WebVTT output) byte for byte against these files.
Needs the Python package plus `responses`:

    python3 -m venv .venv
    .venv/bin/pip install youtube-transcript-api==1.2.4 responses
    .venv/bin/python scripts/record_golden.py
"""

import json
import random
from pathlib import Path

import responses
from requests import HTTPError

from youtube_transcript_api import (
    AgeRestricted,
    FetchedTranscript,
    FetchedTranscriptSnippet,
    FailedToCreateConsentCookie,
    InvalidVideoId,
    IpBlocked,
    NotTranslatable,
    NoTranscriptFound,
    PoTokenRequired,
    RequestBlocked,
    TranscriptsDisabled,
    TranslationLanguageNotAvailable,
    VideoUnavailable,
    VideoUnplayable,
    YouTubeDataUnparsable,
    YouTubeRequestFailed,
    YouTubeTranscriptApi,
)
from youtube_transcript_api.formatters import (
    SRTFormatter,
    TextFormatter,
    WebVTTFormatter,
)
from youtube_transcript_api._transcripts import _TranscriptParser
from youtube_transcript_api.proxies import GenericProxyConfig, WebshareProxyConfig

ROOT = Path(__file__).resolve().parent.parent / "crates" / "youtube-transcript-api"
FIXTURES = ROOT / "tests" / "fixtures"
GOLDEN = ROOT / "tests" / "golden"

VIDEO_ID = "abc"
GENERIC = GenericProxyConfig(http_url="http://localhost:8080")
WEBSHARE = WebshareProxyConfig(proxy_username="u", proxy_password="p")
HTTP_ERROR = HTTPError(
    "500 Server Error: Internal Server Error for url: https://www.youtube.com/x"
)

ERRORS = {
    "youtube_data_unparsable": YouTubeDataUnparsable(VIDEO_ID),
    "youtube_request_failed": YouTubeRequestFailed(VIDEO_ID, HTTP_ERROR),
    "video_unplayable": VideoUnplayable(VIDEO_ID, "R", ["a", "b"]),
    "video_unplayable_no_reason": VideoUnplayable(VIDEO_ID, None, []),
    "video_unavailable": VideoUnavailable(VIDEO_ID),
    "invalid_video_id": InvalidVideoId(VIDEO_ID),
    "request_blocked": RequestBlocked(VIDEO_ID),
    "request_blocked_generic": RequestBlocked(VIDEO_ID).with_proxy_config(GENERIC),
    "request_blocked_webshare": RequestBlocked(VIDEO_ID).with_proxy_config(WEBSHARE),
    "ip_blocked": IpBlocked(VIDEO_ID),
    "ip_blocked_generic": IpBlocked(VIDEO_ID).with_proxy_config(GENERIC),
    "ip_blocked_webshare": IpBlocked(VIDEO_ID).with_proxy_config(WEBSHARE),
    "transcripts_disabled": TranscriptsDisabled(VIDEO_ID),
    "age_restricted": AgeRestricted(VIDEO_ID),
    "not_translatable": NotTranslatable(VIDEO_ID),
    "translation_language_not_available": TranslationLanguageNotAvailable(VIDEO_ID),
    "failed_to_create_consent_cookie": FailedToCreateConsentCookie(VIDEO_ID),
    "no_transcript_found": NoTranscriptFound(VIDEO_ID, ["de", "en"], "DATA"),
    "po_token_required": PoTokenRequired(VIDEO_ID),
}


def write(name: str, text: str) -> None:
    path = GOLDEN / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def record_transcript_list() -> None:
    with responses.RequestsMock() as mock:
        mock.post(
            "https://www.youtube.com/youtubei/v1/player",
            body=(FIXTURES / "youtube.innertube.json.static").read_bytes(),
        )
        mock.get(
            "https://www.youtube.com/watch",
            body=(FIXTURES / "youtube.html.static").read_bytes(),
        )
        write("transcript_list.txt", str(YouTubeTranscriptApi().list("GJLlxj_dtq8")))


# Overlapping cues and times past an hour, for the SRT/WebVTT end-time rule.
LONG = FetchedTranscript(
    snippets=[
        FetchedTranscriptSnippet(text="first", start=0.0, duration=5.0),
        FetchedTranscriptSnippet(text="overlaps", start=2.5, duration=1.25),
        FetchedTranscriptSnippet(text="later", start=3725.5, duration=2.0),
    ],
    video_id="long",
    language="English",
    language_code="en",
    is_generated=True,
)

FORMATTERS = {"text": TextFormatter(), "srt": SRTFormatter(), "webvtt": WebVTTFormatter()}


def record_formatters() -> None:
    with responses.RequestsMock() as mock:
        mock.post(
            "https://www.youtube.com/youtubei/v1/player",
            body=(FIXTURES / "youtube.innertube.json.static").read_bytes(),
        )
        mock.get(
            "https://www.youtube.com/watch",
            body=(FIXTURES / "youtube.html.static").read_bytes(),
        )
        mock.get(
            "https://www.youtube.com/api/timedtext",
            body=(FIXTURES / "transcript.xml.static").read_bytes(),
        )
        transcript = YouTubeTranscriptApi().fetch("GJLlxj_dtq8")
    for name, formatter in FORMATTERS.items():
        write(f"formatters/{name}.txt", formatter.format_transcript(transcript))
        write(f"formatters/{name}_long.txt", formatter.format_transcript(LONG))
        write(
            f"formatters/{name}_many.txt",
            formatter.format_transcripts([transcript, LONG]),
        )


# Pieces of caption text for random inputs to `preserve_formatting`'s tag
# stripping, including characters Python's (?i) and \w treat specially.
TAG_PIECES = [
    "<", "<", ">", ">", "/", " ", "\n", "_", "x", "font", '"', "ſ", "K",
    "ı", "İ", "Ｂ", "\u0301", "B", "STRONG", "Em", "<i>", "</i>", "<br/>",
    '<font color="red">', "</font>", "<sub>", "strong", "em", "b", "i",
    "mark", "small", "del", "ins", "sub", "sup",
    # Marks Rust calls alphabetic and Python's \w doesn't, a Devanagari
    # word, and a digit only Python's \w (str.isalnum) knows as numeric.
    "\u0345", "\u093e", "\u0939\u093f\u0902\u0926\u0940", "\u00b2",
]


def record_tag_stripping() -> None:
    pattern = _TranscriptParser(preserve_formatting=True)._html_regex
    rng = random.Random(7)
    cases = []
    for _ in range(3000):
        pieces = rng.choices(TAG_PIECES, k=rng.randint(0, 24))
        text = "".join(pieces)
        cases.append([text, pattern.sub("", text)])
    write("strip_tags.json", json.dumps(cases, ensure_ascii=False, indent=0) + "\n")


# Cue times whose sub-millisecond part shows how Python rounds: to two
# decimals of a millisecond, then truncated, so .9999996 prints as ",1000".
EDGE_TIMES = [
    -1.5, 0.0, 0.0004999, 0.0005, 0.9999996, 1.0006, 2.0005, 6.93, 59.9999,
    3599.9996, 3599.99996, 3725.5, 86399.9999, 90061.25, 360000.0,
]


def record_timestamps() -> None:
    srt, webvtt = SRTFormatter(), WebVTTFormatter()
    rng = random.Random(11)
    times = EDGE_TIMES + [
        round(rng.uniform(0, 20000), rng.choice([2, 3, 4, 6, 9])) for _ in range(2000)
    ]
    cases = [
        [t, srt._seconds_to_timestamp(t), webvtt._seconds_to_timestamp(t)]
        for t in times
    ]
    write("timestamps.json", json.dumps(cases, indent=0) + "\n")


# `start` attributes for float(): Python also takes digit-group underscores.
NUMBERS = [
    "1.5", " 1.5 ", "+3", ".5", "5.", "1e3", "inf", "nan", "1_000",
    "1_000.5_5", "1__0", "_1", "1_", "1._5", "0x10", "1,5", "", "1e",
]


def record_numbers() -> None:
    cases = []
    for number in NUMBERS:
        xml = f'<transcript><text start="{number}" dur="1">x</text></transcript>'
        try:
            start = _TranscriptParser(preserve_formatting=False).parse(xml)[0].start
            cases.append([number, repr(start)])
        except ValueError:
            cases.append([number, None])
    write("numbers.json", json.dumps(cases, indent=0) + "\n")


def main() -> None:
    for name, error in ERRORS.items():
        write(f"errors/{name}.txt", str(error))
    record_transcript_list()
    record_formatters()
    record_tag_stripping()
    record_timestamps()
    record_numbers()


if __name__ == "__main__":
    main()
