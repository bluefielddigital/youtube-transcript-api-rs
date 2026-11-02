# youtube-transcript-api-cli

[![crates.io](https://img.shields.io/crates/v/youtube-transcript-api-cli.svg)](https://crates.io/crates/youtube-transcript-api-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The `youtube-transcript-api` command-line tool: fetch the transcripts and
subtitles of YouTube videos, including automatically generated ones, with no
API key and no headless browser. It takes the same options, plus a few more,
and writes the same formats as Jonas Depoix's
[youtube-transcript-api](https://github.com/jdepoix/youtube-transcript-api)
for Python.

Its Rust library, for use in your own code, is
[`youtube-transcript-api`](https://crates.io/crates/youtube-transcript-api).

## Installation

Download a binary for your platform from the [releases
page](https://github.com/bluefielddigital/youtube-transcript-api-rs/releases),
or build it with Cargo:

```bash
cargo install youtube-transcript-api-cli
```

Either way, you run `youtube-transcript-api`.

## Usage

```bash
# The English transcript, as indented JSON.
youtube-transcript-api dQw4w9WgXcQ

# Two videos, by ID or URL, German or else English, as SRT.
youtube-transcript-api dQw4w9WgXcQ https://youtu.be/jNQXAC9IVRw --languages de en --format srt

# What's available.
youtube-transcript-api --list-transcripts dQw4w9WgXcQ
```

Transcripts go to stdout and errors to stderr. The exit code is 1 if any
video failed. Run `youtube-transcript-api --help` for every option, or see the
[repository README][readme].

[readme]: https://github.com/bluefielddigital/youtube-transcript-api-rs#command-line

## License

MIT; see [LICENSE](LICENSE). Release notes are in [CHANGELOG](CHANGELOG.md).
