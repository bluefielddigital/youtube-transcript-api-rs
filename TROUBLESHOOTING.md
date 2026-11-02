# Troubleshooting

## Blocked requests

A `RequestBlocked` or `IpBlocked` error means YouTube is blocking your IP.
YouTube blocks requests from most cloud providers' IP ranges (AWS, GCP,
Azure, DigitalOcean, and others), and any IP that sends too many
requests. It tends to work on a laptop and fail on a server.

- **Use rotating residential proxies.** They suit this, because each request
  comes from a different home IP, and a blocked one tries again with
  another.
  With Webshare, create a "Residential" proxy package (not "Proxy Server" or
  "Static Residential"), then:

  ```rust
  let proxy = WebshareProxyConfig::new("username", "password");
  let api = YouTubeTranscriptApi::builder().proxy_config(proxy).build()?;
  ```

  From the command line, pass `--webshare-proxy-username` and
  `--webshare-proxy-password`.
- **Or any other proxy**, through `GenericProxyConfig` or `--http-proxy` and
  `--https-proxy`. A single fixed IP gets blocked like any other.
- **Slow down.** YouTube blocks an IP less when it sends fewer requests per
  minute.

## Translations fail while the original works

YouTube sometimes answers translation requests with HTTP 429 (shown as
`IpBlocked`) while it still serves the original transcript. It comes and
goes. Retry later, or through a rotating proxy.

## `NoTranscriptFound`

None of the language codes you asked for has a transcript. List what the
video has:

```bash
youtube-transcript-api --list-transcripts VIDEO_ID
```

Codes can be regional, such as `en-US`, `pt-BR` or `es-419`, so `en` doesn't
match `en-US`. Pass a list of codes in order of preference
(`--languages en en-US en-GB`), or let each code match its regional variants
with `--match-language-variants` (`match_language_variants(true)` in code).

## `TranscriptsDisabled`

The video has no transcripts, or its owner turned them off. Nothing in this
library can fetch one.

## `InvalidVideoId` or `VideoUnavailable`

Video IDs and YouTube video URLs both work. `InvalidVideoId` means a URL that
isn't a link to a video (a channel or playlist page, for example). A deleted,
private or region-blocked video is unavailable.

## A request hangs

A stalled proxy or connection can hang a request indefinitely. Set a timeout:
`--timeout 30`, or `.timeout(Duration::from_secs(30))` on the builder.

## `AgeRestricted`

Age-restricted videos need a signed-in user, and signing in isn't supported.

## `PoTokenRequired`

For some videos and some clients, YouTube asks for a proof-of-origin token,
which this library can't produce. It changes over time; if it persists,
please open an issue with the video ID.

## `YouTubeDataUnparsable` or `UnexpectedData`

YouTube's responses changed shape. Please open an issue with the video ID and
the full error message.

## When YouTube changes the client version or its wording

Two values in the code follow YouTube and age as it changes:

- **The client version.** Requests to the player endpoint claim to be the
  Android app, in `settings.rs` of the library crate
  (`crates/youtube-transcript-api/src`): `INNERTUBE_CLIENT_NAME` (`ANDROID`)
  and `INNERTUBE_CLIENT_VERSION` (`20.10.38`). When YouTube stops serving that
  version, the player response lacks captions or answers an error. Find the
  current Android version in the YouTube app's details, or in a request the
  Python youtube-transcript-api makes (its `INNERTUBE_CONTEXT`), then change the
  constant. Check a few videos, with and without captions, before releasing.
- **The reasons in English.** `transcripts/fetcher.rs` in the library crate
  compares the `playabilityStatus` reason with fixed English strings (the
  `REASON_*` constants: bot check, age restriction, unavailable video). The
  session sends `Accept-Language: en-US` for this. When YouTube rewords one, the
  matching error turns into `VideoUnplayable` with the new text. Copy the
  new wording from that error into the constant, and record a golden file
  for it.

## macOS refuses to open the binary

The macOS binaries carry Apple's notarization, but a bare binary can't hold
its ticket, so macOS checks it online the first time it runs. Run it once
while online. A copy you built yourself has no notarization; run that one
from `target/`, or build it with `cargo install`.

## Getting help

Open an issue on GitHub with the video ID, the command or code you ran, and
the full error message.
