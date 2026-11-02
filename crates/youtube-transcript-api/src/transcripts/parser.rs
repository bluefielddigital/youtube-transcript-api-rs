use std::borrow::Cow;

use lazy_regex::regex_is_match;

use super::FetchedTranscriptSnippet;
use crate::errors::UnexpectedData;
use crate::tags;

/// Accepts underscores between digits, as in `1_000`.
fn parse_seconds(attribute: &'static str, value: &str) -> Result<f64, UnexpectedData> {
    let trimmed = value.trim();
    let number: Cow<'_, str> =
        if trimmed.contains('_') && !regex_is_match!("(?:^|[^0-9])_|_(?:[^0-9]|$)", trimmed) {
            trimmed.replace('_', "").into()
        } else {
            trimmed.into()
        };
    number.parse().map_err(|source| UnexpectedData::NotANumber {
        attribute,
        value: value.to_owned(),
        source,
    })
}

/// Parses timedtext XML into snippets.
pub struct TranscriptParser {
    /// Whether to keep formatting tags.
    preserve_formatting: bool,
}

impl TranscriptParser {
    /// A parser that keeps formatting tags if `preserve_formatting`.
    pub const fn new(preserve_formatting: bool) -> Self {
        Self {
            preserve_formatting,
        }
    }

    /// The text with tags stripped as configured.
    fn strip_tags(&self, text: &str) -> String {
        if self.preserve_formatting {
            tags::strip_unformatting(text)
        } else {
            tags::strip_all(text).into_owned()
        }
    }

    /// The snippets in `raw_data`.
    pub fn parse(&self, raw_data: &str) -> Result<Vec<FetchedTranscriptSnippet>, UnexpectedData> {
        let document = roxmltree::Document::parse(raw_data)
            .map_err(|e| UnexpectedData::InvalidXml(Box::new(e)))?;
        let mut snippets = Vec::new();
        for element in document
            .root_element()
            .children()
            .filter(roxmltree::Node::is_element)
        {
            // ElementTree's `.text`: the character data before the first child
            // element. Its parser drops comments, so text around them joins up.
            let text_nodes: Vec<&str> = element
                .children()
                .take_while(|n| !n.is_element())
                .filter_map(|n| n.is_text().then(|| n.text()).flatten())
                .collect();
            if text_nodes.is_empty() {
                continue;
            }
            let text = text_nodes.concat();
            let start = element
                .attribute("start")
                .ok_or(UnexpectedData::Field { field: "start" })?;
            let duration = element.attribute("dur").unwrap_or("0.0");
            snippets.push(FetchedTranscriptSnippet {
                text: self.strip_tags(&htmlize::unescape(text)),
                start: parse_seconds("start", start)?,
                duration: parse_seconds("dur", duration)?,
            });
        }
        Ok(snippets)
    }
}

/// Parsing of timedtext XML.
#[cfg(test)]
mod tests {
    use super::TranscriptParser;

    /// A transcript with tags, entities, an empty snippet and one without
    /// `dur`.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8" ?><transcript>
        <text start="0" dur="1.5">a &lt;i&gt;b&lt;/i&gt; &amp;amp; &lt;font color="red"&gt;c&lt;/font&gt;</text>
        <text start="2"></text>
        <text start="3">no dur</text>
    </transcript>"#;

    /// Tags go, entities decode twice, and empty snippets drop.
    #[test]
    fn strips_tags_and_double_unescapes() {
        let snippets = TranscriptParser::new(false).parse(XML).unwrap();
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].text, "a b & c");
        assert!((snippets[1].duration - 0.0).abs() < f64::EPSILON);
    }

    /// `preserve_formatting` keeps only the formatting tags.
    #[test]
    fn preserve_formatting_keeps_formatting_tags_only() {
        let snippets = TranscriptParser::new(true).parse(XML).unwrap();
        // A tag ends at a `>` after a word character, so after
        // `color="red"` it runs on to `</font>` and swallows the `c` too.
        assert_eq!(snippets[0].text, "a <i>b</i> & ");
    }

    /// Many unclosed `<` don't hit a backtracking limit.
    #[test]
    fn preserve_formatting_has_no_backtracking_limit() {
        let text = "&lt;a ".repeat(1000);
        let xml = format!(r#"<transcript><text start="0">{text}</text></transcript>"#);
        let snippets = TranscriptParser::new(true).parse(&xml).unwrap();
        assert_eq!(snippets[0].text, "<a ".repeat(1000));
    }

    /// A non-numeric `start` is an error.
    #[test]
    fn non_numeric_start_is_an_error() {
        let xml = r#"<transcript><text start="soon">x</text></transcript>"#;
        let error = TranscriptParser::new(false).parse(xml).unwrap_err();
        assert_eq!(
            error.to_string(),
            "transcript `start` is not a number: \"soon\""
        );
    }

    /// A missing `start` or a bad `dur` is an error.
    #[test]
    fn missing_start_or_bad_dur_is_an_error() {
        for xml in [
            r#"<transcript><text dur="1">x</text></transcript>"#,
            r#"<transcript><text start="1" dur="long">x</text></transcript>"#,
        ] {
            assert!(TranscriptParser::new(false).parse(xml).is_err(), "{xml}");
        }
    }

    /// Malformed XML is an error.
    #[test]
    fn malformed_xml_is_an_error() {
        assert!(TranscriptParser::new(false).parse("<transcript>").is_err());
    }
}
