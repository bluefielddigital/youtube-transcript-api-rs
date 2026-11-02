# youtube-transcript-api

[![crates.io](https://img.shields.io/crates/v/youtube-transcript-api.svg)](https://crates.io/crates/youtube-transcript-api)
[![docs.rs](https://img.shields.io/docsrs/youtube-transcript-api)](https://docs.rs/youtube-transcript-api)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust library for fetching the transcripts and subtitles of YouTube videos,
including automatically generated ones. It needs no API key and no headless
browser.

Inspired by Jonas Depoix's
[youtube-transcript-api](https://github.com/jdepoix/youtube-transcript-api)
for Python. For the command-line tool, see
[`youtube-transcript-api-cli`](https://crates.io/crates/youtube-transcript-api-cli).

## Installation

```bash
cargo add youtube-transcript-api
cargo add tokio --features macros,rt-multi-thread
```

## Example

```rust
use youtube_transcript_api::YouTubeTranscriptApi;

#[tokio::main]
async fn main() -> Result<(), youtube_transcript_api::Error> {
    let api = YouTubeTranscriptApi::new()?;

    // German if there is one, otherwise English.
    let transcript = api.fetch("dQw4w9WgXcQ", &["de", "en"], false).await?;
    for snippet in &transcript {
        println!("{:>8.2}  {}", snippet.start, snippet.text);
    }
    Ok(())
}
```

The [API documentation](https://docs.rs/youtube-transcript-api) covers
listing and translating transcripts, video details, the output formats,
proxies, timeouts, your own HTTP client, and error handling. The
[repository README](https://github.com/bluefielddigital/youtube-transcript-api-rs#readme)
has the same material with more examples.

## Features

- `tracing`: [`tracing`](https://docs.rs/tracing) events under the
  `youtube_transcript_api` target for each request, retry, and consent page.

## License

MIT; see [LICENSE](LICENSE). Release notes are in [CHANGELOG](CHANGELOG.md).
