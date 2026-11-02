use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;

use crate::errors::{CouldNotRetrieveTranscript, Error, Reason, UnexpectedData};
use crate::http::{HttpResponse, Session, StatusCode};
use crate::innertube::{self, CaptionTrack, TracklistRenderer, Tracks};
use crate::video::VideoDetails;

/// Fetching a video's transcript list from YouTube.
mod fetcher;
/// Parsing timedtext XML.
mod parser;

pub use fetcher::TranscriptListFetcher;
use parser::TranscriptParser;

/// One line of a transcript.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FetchedTranscriptSnippet {
    /// The text.
    pub text: String,
    /// When the snippet appears on screen, in seconds.
    pub start: f64,
    /// How long the snippet stays on screen, in seconds. This isn't the
    /// length of the speech, so snippets can overlap.
    pub duration: f64,
}

/// A fetched transcript.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FetchedTranscript {
    /// The lines of the transcript.
    pub snippets: Vec<FetchedTranscriptSnippet>,
    /// The video it belongs to.
    pub video_id: String,
    /// The language name, for example `English`.
    pub language: String,
    /// The language code, for example `en`.
    pub language_code: String,
    /// Whether YouTube generated it automatically.
    pub is_generated: bool,
}

impl FetchedTranscriptSnippet {
    /// A snippet showing `text` from `start` for `duration` seconds.
    pub fn new(text: impl Into<String>, start: f64, duration: f64) -> Self {
        Self {
            text: text.into(),
            start,
            duration,
        }
    }
}

impl FetchedTranscript {
    /// A transcript of `video_id` in `language` (`language_code`), for
    /// formatting snippets that didn't come from [`Transcript::fetch`].
    pub fn new(
        video_id: impl Into<String>,
        language: impl Into<String>,
        language_code: impl Into<String>,
        is_generated: bool,
        snippets: Vec<FetchedTranscriptSnippet>,
    ) -> Self {
        Self {
            snippets,
            video_id: video_id.into(),
            language: language.into(),
            language_code: language_code.into(),
            is_generated,
        }
    }

    /// Iterates over the snippets.
    pub fn iter(&self) -> std::slice::Iter<'_, FetchedTranscriptSnippet> {
        self.snippets.iter()
    }

    /// The number of snippets.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.snippets.len()
    }

    /// Whether there are no snippets.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.snippets.is_empty()
    }
}

impl<'a> IntoIterator for &'a FetchedTranscript {
    type Item = &'a FetchedTranscriptSnippet;
    type IntoIter = std::slice::Iter<'a, FetchedTranscriptSnippet>;

    fn into_iter(self) -> Self::IntoIter {
        self.snippets.iter()
    }
}

impl std::ops::Index<usize> for FetchedTranscript {
    type Output = FetchedTranscriptSnippet;

    fn index(&self, index: usize) -> &Self::Output {
        &self.snippets[index]
    }
}

/// A language YouTube can translate a transcript into.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TranslationLanguage {
    /// The language name.
    pub language: String,
    /// The language code.
    pub language_code: String,
}

/// YouTube's own caption formats, for [`Transcript::fetch_raw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawFormat {
    /// XML, one element per caption with start and duration in seconds.
    Srv1,
    /// XML with times in milliseconds.
    Srv2,
    /// XML with per-word timing for generated transcripts.
    Srv3,
    /// JSON with per-word timing for generated transcripts.
    Json3,
    /// TTML (Timed Text Markup Language).
    Ttml,
    /// WebVTT as YouTube writes it.
    Vtt,
}

impl RawFormat {
    /// Every format.
    pub const ALL: &[Self] = &[
        Self::Srv1,
        Self::Srv2,
        Self::Srv3,
        Self::Json3,
        Self::Ttml,
        Self::Vtt,
    ];

    /// YouTube's name for it, as in `&fmt=srv3`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Srv1 => "srv1",
            Self::Srv2 => "srv2",
            Self::Srv3 => "srv3",
            Self::Json3 => "json3",
            Self::Ttml => "ttml",
            Self::Vtt => "vtt",
        }
    }
}

impl fmt::Display for RawFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An unknown raw format name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error(
    "The raw format '{0}' is not supported. Choose one of the following formats: \
     srv1, srv2, srv3, json3, ttml, vtt"
)]
pub struct UnknownRawFormat(pub String);

impl FromStr for RawFormat {
    type Err = UnknownRawFormat;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|format| format.name() == name)
            .ok_or_else(|| UnknownRawFormat(name.to_owned()))
    }
}

/// A transcript that's available for a video but not fetched yet.
#[derive(Debug, Clone)]
pub struct Transcript {
    /// The session that downloads it.
    session: Session,
    /// The video it belongs to.
    pub video_id: String,
    /// The timedtext URL.
    url: String,
    /// The language name.
    pub language: String,
    /// The language code.
    pub language_code: String,
    /// Whether YouTube generated it automatically.
    pub is_generated: bool,
    /// The languages YouTube can translate it into. Empty if none.
    pub translation_languages: Vec<TranslationLanguage>,
}

impl Transcript {
    /// The timedtext URL this transcript comes from.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Downloads and parses the transcript.
    ///
    /// `preserve_formatting` keeps a few HTML formatting tags such as `<i>`.
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), youtube_transcript_api::Error> {
    /// use youtube_transcript_api::YouTubeTranscriptApi;
    ///
    /// let list = YouTubeTranscriptApi::new()?.list("GJLlxj_dtq8").await?;
    /// let transcript = list.find_transcript(&["en"])?.fetch(false).await?;
    /// println!("{} snippets", transcript.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with `PoTokenRequired`, `IpBlocked` (HTTP 429) or
    /// `YouTubeRequestFailed`, or if the XML is invalid.
    pub async fn fetch(&self, preserve_formatting: bool) -> Result<FetchedTranscript, Error> {
        let body = self.download(&self.url).await?;
        Ok(FetchedTranscript {
            snippets: TranscriptParser::new(preserve_formatting).parse(&body)?,
            video_id: self.video_id.clone(),
            language: self.language.clone(),
            language_code: self.language_code.clone(),
            is_generated: self.is_generated,
        })
    }

    /// Downloads the transcript in one of YouTube's own formats, as YouTube
    /// sends it: for per-word timing (`srv3`, `json3`), or to convert it
    /// with other tools (`ttml`, `vtt`).
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), youtube_transcript_api::Error> {
    /// use youtube_transcript_api::{RawFormat, YouTubeTranscriptApi};
    ///
    /// let list = YouTubeTranscriptApi::new()?.list("GJLlxj_dtq8").await?;
    /// let generated = list.find_generated_transcript(&["en"])?;
    /// // XML with the start time of every word.
    /// let srv3 = generated.fetch_raw(RawFormat::Srv3).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with `PoTokenRequired`, `IpBlocked` (HTTP 429) or
    /// `YouTubeRequestFailed`, and with `YouTubeDataUnparsable` if YouTube
    /// sends an empty response, which it occasionally does: a real track,
    /// even one without captions, is never empty.
    pub async fn fetch_raw(&self, format: RawFormat) -> Result<String, Error> {
        let body = self
            .download(&format!("{}&fmt={}", self.url, format.name()))
            .await?;
        if body.trim().is_empty() {
            return Err(self.error(Reason::YouTubeDataUnparsable));
        }
        Ok(body)
    }

    /// Downloads `url`, after checking it doesn't need a PO token.
    async fn download(&self, url: &str) -> Result<String, Error> {
        // YouTube marks a track that needs a PO token with `exp=xpe` in its
        // URL, and answers it with an empty body.
        if url.contains("&exp=xpe") {
            return Err(self.error(Reason::PoTokenRequired));
        }
        let response = self.session.get(url).await?;
        Ok(raise_http_errors(response, &self.video_id)?.body)
    }

    /// Whether YouTube can translate the transcript.
    #[must_use]
    pub const fn is_translatable(&self) -> bool {
        !self.translation_languages.is_empty()
    }

    /// A machine-translated version of this transcript.
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), youtube_transcript_api::Error> {
    /// use youtube_transcript_api::YouTubeTranscriptApi;
    ///
    /// let list = YouTubeTranscriptApi::new()?.list("GJLlxj_dtq8").await?;
    /// let english = list.find_transcript(&["en"])?;
    /// if english.is_translatable() {
    ///     let german = english.translate("de")?.fetch(false).await?;
    ///     println!("{}", german[0].text);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with `NotTranslatable` or `TranslationLanguageNotAvailable`.
    pub fn translate(&self, language_code: &str) -> Result<Self, Error> {
        if !self.is_translatable() {
            return Err(self.error(Reason::NotTranslatable));
        }
        let Some(target) = self
            .translation_languages
            .iter()
            // A later duplicate code wins, as in a map built from the list.
            .rev()
            .find(|lang| lang.language_code == language_code)
        else {
            return Err(self.error(Reason::TranslationLanguageNotAvailable));
        };
        Ok(Self {
            session: self.session.clone(),
            video_id: self.video_id.clone(),
            url: format!("{}&tlang={language_code}", self.url),
            language: target.language.clone(),
            language_code: language_code.to_owned(),
            // A machine translation counts as generated.
            is_generated: true,
            translation_languages: Vec::new(),
        })
    }

    /// A transcript from a `captionTracks` entry. It reads the fields in a
    /// fixed order, so a broken entry always reports the same one missing.
    fn from_track(
        session: &Session,
        video_id: &str,
        caption: CaptionTrack,
        translation_languages: &[TranslationLanguage],
    ) -> Result<Self, Error> {
        let url = caption
            .base_url
            .ok_or_else(|| unexpected("baseUrl"))?
            .replace("&fmt=srv3", "");
        let language = run_text(caption.name.as_ref(), "name")?;
        let language_code = caption
            .language_code
            .ok_or_else(|| unexpected("languageCode"))?;
        Ok(Self {
            session: session.clone(),
            video_id: video_id.to_owned(),
            url,
            language,
            language_code,
            is_generated: caption.kind.as_deref() == Some("asr"),
            translation_languages: if caption.is_translatable {
                translation_languages.to_vec()
            } else {
                Vec::new()
            },
        })
    }

    /// A [`CouldNotRetrieveTranscript`] for this transcript's video.
    fn error(&self, reason: Reason) -> Error {
        CouldNotRetrieveTranscript::new(&self.video_id, reason).into()
    }
}

impl fmt::Display for Transcript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (\"{}\"){}",
            self.language_code,
            self.language,
            if self.is_translatable() {
                "[TRANSLATABLE]"
            } else {
                ""
            }
        )
    }
}

/// The transcripts available for a video. Iterating yields the manually
/// created ones first, then the generated ones.
#[derive(Debug, Clone)]
pub struct TranscriptList {
    /// The video.
    pub video_id: String,
    /// Keyed by language code, in YouTube's order. A later duplicate code
    /// replaces the earlier transcript in its place.
    manually_created: IndexMap<String, Transcript>,
    /// Generated transcripts, keyed like `manually_created`.
    generated: IndexMap<String, Transcript>,
    /// The languages YouTube can translate into.
    translation_languages: Vec<TranslationLanguage>,
    /// The video's details, if YouTube sent them.
    video_details: Option<VideoDetails>,
    /// Whether the `find_*` methods also match regional variants.
    match_language_variants: bool,
}

impl TranscriptList {
    /// The list from YouTube's caption tracks.
    fn build(
        session: &Session,
        video_id: &str,
        captions: TracklistRenderer,
        video_details: Option<VideoDetails>,
    ) -> Result<Self, Error> {
        let translation_languages = parse_translation_languages(captions.translation_languages)?;
        let Tracks::List(tracks) = captions.caption_tracks else {
            return Err(unexpected("captionTracks"));
        };
        let mut manually_created = IndexMap::new();
        let mut generated = IndexMap::new();
        for caption in tracks {
            let transcript =
                Transcript::from_track(session, video_id, caption, &translation_languages)?;
            let target = if transcript.is_generated {
                &mut generated
            } else {
                &mut manually_created
            };
            target.insert(transcript.language_code.clone(), transcript);
        }
        Ok(Self {
            video_id: video_id.to_owned(),
            manually_created,
            generated,
            translation_languages,
            video_details,
            match_language_variants: false,
        })
    }

    /// Whether the `find_*` methods also match regional variants of a
    /// language code: with this on, `en` finds `en-US` and `en-GB` finds
    /// `en` when there is no exact match. The exact code always wins, and
    /// each code, variants included, comes before the next one. Off by
    /// default.
    #[must_use]
    pub const fn match_language_variants(mut self, on: bool) -> Self {
        self.match_language_variants = on;
        self
    }

    /// The video's title, author, length and other details, if YouTube sent
    /// them.
    #[must_use]
    pub const fn video_details(&self) -> Option<&VideoDetails> {
        self.video_details.as_ref()
    }

    /// Iterates over all transcripts: manually created first, then generated.
    pub fn iter(&self) -> impl Iterator<Item = &Transcript> {
        self.manually_created
            .values()
            .chain(self.generated.values())
    }

    /// Finds a transcript for the first language code that has one, preferring
    /// manually created transcripts over generated ones for each code.
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), youtube_transcript_api::Error> {
    /// use youtube_transcript_api::YouTubeTranscriptApi;
    ///
    /// let list = YouTubeTranscriptApi::new()?.list("GJLlxj_dtq8").await?;
    /// // German if there is one, otherwise English.
    /// let transcript = list.find_transcript(&["de", "en"])?;
    /// println!("{}", transcript.language_code);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// `NoTranscriptFound` if none of the codes has a transcript.
    pub fn find_transcript<S: AsRef<str>>(
        &self,
        language_codes: &[S],
    ) -> Result<&Transcript, Error> {
        self.find(language_codes, &[&self.manually_created, &self.generated])
    }

    /// Like [`Self::find_transcript`], but only generated transcripts.
    ///
    /// # Errors
    ///
    /// `NoTranscriptFound` if none of the codes has a generated transcript.
    pub fn find_generated_transcript<S: AsRef<str>>(
        &self,
        language_codes: &[S],
    ) -> Result<&Transcript, Error> {
        self.find(language_codes, &[&self.generated])
    }

    /// Like [`Self::find_transcript`], but only manually created transcripts.
    ///
    /// # Errors
    ///
    /// `NoTranscriptFound` if none of the codes has a manual transcript.
    pub fn find_manually_created_transcript<S: AsRef<str>>(
        &self,
        language_codes: &[S],
    ) -> Result<&Transcript, Error> {
        self.find(language_codes, &[&self.manually_created])
    }

    /// The first transcript in `groups` for the first of `language_codes` that has one.
    fn find<'a, S: AsRef<str>>(
        &'a self,
        language_codes: &[S],
        groups: &[&'a IndexMap<String, Transcript>],
    ) -> Result<&'a Transcript, Error> {
        for requested in language_codes {
            let code = requested.as_ref();
            for group in groups {
                if let Some(found) = group.get(code) {
                    return Ok(found);
                }
            }
            if self.match_language_variants {
                for group in groups {
                    if let Some(found) = group
                        .values()
                        .find(|t| same_language(&t.language_code, code))
                    {
                        return Ok(found);
                    }
                }
            }
        }
        Err(CouldNotRetrieveTranscript::new(
            &self.video_id,
            Reason::NoTranscriptFound {
                requested_language_codes: language_codes
                    .iter()
                    .map(|c| c.as_ref().to_owned())
                    .collect(),
                transcript_data: self.to_string(),
            },
        )
        .into())
    }
}

impl<'a> IntoIterator for &'a TranscriptList {
    type Item = &'a Transcript;
    type IntoIter = std::iter::Chain<
        indexmap::map::Values<'a, String, Transcript>,
        indexmap::map::Values<'a, String, Transcript>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.manually_created
            .values()
            .chain(self.generated.values())
    }
}

/// Whether two language codes share their primary language: `en`, `en-US`
/// and `EN-gb` all do.
fn same_language(a: &str, b: &str) -> bool {
    let primary = |code: &str| {
        code.split('-')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    primary(a) == primary(b)
}

/// One ` - item` line per item, or `None`.
fn describe<I: Iterator<Item = String>>(items: I) -> String {
    let description = items
        .map(|item| format!(" - {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    if description.is_empty() {
        "None".to_owned()
    } else {
        description
    }
}

impl fmt::Display for TranscriptList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "For this video ({}) transcripts are available in the following languages:\n\n\
             (MANUALLY CREATED)\n{}\n\n(GENERATED)\n{}\n\n(TRANSLATION LANGUAGES)\n{}",
            self.video_id,
            describe(self.manually_created.values().map(ToString::to_string)),
            describe(self.generated.values().map(ToString::to_string)),
            describe(
                self.translation_languages
                    .iter()
                    .map(|l| format!("{} (\"{}\")", l.language_code, l.language))
            ),
        )
    }
}

/// The translation languages, or the first missing field.
fn parse_translation_languages(
    languages: Option<Vec<innertube::TranslationLanguage>>,
) -> Result<Vec<TranslationLanguage>, Error> {
    languages
        .unwrap_or_default()
        .into_iter()
        .map(|language| {
            Ok(TranslationLanguage {
                language: run_text(language.language_name.as_ref(), "languageName")?,
                language_code: language
                    .language_code
                    .ok_or_else(|| unexpected("languageCode"))?,
            })
        })
        .collect()
}

/// An [`UnexpectedData::Field`] error for `field`.
fn unexpected(field: &'static str) -> Error {
    UnexpectedData::Field { field }.into()
}

/// The first run's text, or an error naming `key`.
fn run_text(text: Option<&innertube::Text>, key: &'static str) -> Result<String, Error> {
    text.and_then(innertube::Text::first_run)
        .map(str::to_owned)
        .ok_or_else(|| unexpected(key))
}

/// Turns 429 into `IpBlocked` and other error statuses into
/// `YouTubeRequestFailed`.
fn raise_http_errors(response: HttpResponse, video_id: &str) -> Result<HttpResponse, Error> {
    let status = response.status;
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(
            CouldNotRetrieveTranscript::new(video_id, Reason::IpBlocked { proxy: None }).into(),
        );
    }
    if status.is_client_error() || status.is_server_error() {
        let kind = if status.is_client_error() {
            "client"
        } else {
            "server"
        };
        // Worded like reqwest's `error_for_status`.
        let reason = format!(
            "HTTP status {kind} error ({status}) for url ({})",
            response.url
        );
        return Err(CouldNotRetrieveTranscript::new(
            video_id,
            Reason::YouTubeRequestFailed { reason },
        )
        .into());
    }
    Ok(response)
}

/// Raw formats and fetched transcripts.
#[cfg(test)]
mod tests {
    use super::{FetchedTranscript, FetchedTranscriptSnippet, RawFormat};

    /// Every raw format parses back from its name, and others fail.
    #[test]
    fn raw_formats_round_trip_by_name() {
        for &format in RawFormat::ALL {
            assert_eq!(format.name().parse::<RawFormat>(), Ok(format));
            assert_eq!(format.to_string(), format.name());
        }
        let error = "xml".parse::<RawFormat>().unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("The raw format 'xml' is not supported.")
        );
    }

    /// A fetched transcript has a length, indexing, and iteration.
    #[test]
    fn fetched_transcript_behaves_like_a_sequence() {
        let transcript = FetchedTranscript {
            snippets: vec![FetchedTranscriptSnippet {
                text: "x".to_owned(),
                start: 1.0,
                duration: 2.0,
            }],
            video_id: "v".to_owned(),
            language: "English".to_owned(),
            language_code: "en".to_owned(),
            is_generated: false,
        };
        assert_eq!(transcript.len(), 1);
        assert!(!transcript.is_empty());
        assert_eq!(transcript[0].text, "x");
        assert_eq!(transcript.iter().count(), 1);
        assert_eq!((&transcript).into_iter().count(), 1);
    }
}
