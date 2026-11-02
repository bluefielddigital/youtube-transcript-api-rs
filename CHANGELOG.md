# Changelog

This file records the notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project follows [Semantic Versioning](https://semver.org/).

Each release's notes are its section here.

## [0.1.0] - 2026-11-02

Initial release: a Rust library and command-line tool that fetches the
transcripts and subtitles of YouTube videos (automatically generated ones
included) with no API key and no headless browser. The library is the
`youtube-transcript-api` crate and the command line the
`youtube-transcript-api-cli` crate; both carry this version.

### Library

- `YouTubeTranscriptApi`: fetch a transcript in the first of your preferred
  languages, or list a video's transcripts, pick one by language and
  translate it. Async, on tokio.
- Any video ID or YouTube URL (`watch`, `youtu.be`, `shorts`, `embed`,
  `live`), with `video_id()` to extract the ID yourself.
- Video IDs are percent-encoded in request URLs, so an odd ID can't add
  query parameters. IDs still go to YouTube unchecked, which answers for
  them (`InvalidVideoId` or `VideoUnavailable`).
- Optional language variants: `en` also finds `en-US` and `en-GB`.
- Video details from the same requests: title, author, channel, length,
  views, likes, description, keywords, category, dates, and thumbnails.
- Transcripts and video details serialize with `serde`.
- Typed errors whose messages say what went wrong and what to try.

### Output formats

- Plain text, timestamped text, SRT, WebVTT, JSON, and pretty JSON. Text,
  SRT, and WebVTT are byte for byte the output of the Python
  youtube-transcript-api.
- YouTube's own formats: srv1, srv2, srv3 and json3 (with per-word timing
  for generated captions), TTML, and WebVTT.

### Networking

- Any HTTP, HTTPS or SOCKS5 proxy, or Webshare's rotating residential
  proxies. With a rotating proxy, a request YouTube blocks (including HTTP
  429) or a dropped connection gets another try through a new IP.
- Request timeouts.
- Retries wait between tries: exponential backoff with full jitter (500 ms
  doubling to 30 s), tunable with `RetryPolicy` and
  `YouTubeTranscriptApi::builder().retry_policy(..)`, which can also set
  the retry count without a proxy.
- A `Retry-After` up to a configurable cap (60 s by default) makes the client
  wait that long. A longer one fails the request at once with
  `TransportErrorKind::RateLimited`, and `TransportError::retry_after` holds
  the wait the server asked for, so the caller can wait and retry.
- A limit on response bodies (16 MiB by default, `max_body_bytes`), so a
  runaway response can't fill memory.
- A pluggable HTTP client: bring your own through the `HttpClient` trait.
- Optional `tracing` events for requests, retries and YouTube's consent
  page.

### Command line

- `youtube-transcript-api`, with the same options as the Python tool plus
  a few more, so existing scripts work unchanged. Install with
  `cargo install youtube-transcript-api-cli`, or download a binary.
- Binaries for Linux (x86_64, ARM64), macOS (Apple Silicon, Intel) and
  Windows (x86_64), with SHA-256 checksums. The macOS binaries carry a
  Developer ID signature and Apple's notarization.

### Stability

- Supports Rust 1.88 and newer.
- Public structs and enums are `#[non_exhaustive]`, so later releases can
  add fields and variants without breaking your code. Types you build
  yourself have constructors, such as `FetchedTranscript::new` and
  `HttpResponse::new`.
