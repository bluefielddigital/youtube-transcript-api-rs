use std::borrow::Cow;

use lazy_regex::{regex_find, regex_is_match, regex_replace_all};

/// Removes every HTML tag.
pub fn strip_all(text: &str) -> Cow<'_, str> {
    regex_replace_all!(r"<[^>]*>", text, "")
}

/// Removes every HTML tag except the formatting ones (`strong`, `em`, `b`,
/// `i`, `mark`, `small`, `del`, `ins`, `sub`, `sup`, in any case).
///
/// The rules, which keep output identical to youtube-transcript-api's:
///
/// - A tag runs from `<` to the first `>` that follows a word character on
///   the same line, so `<font color="red">` runs on to the next such `>`.
/// - Word characters are letters, numbers and `_` (`[\p{L}\p{N}_]`), not
///   combining marks such as Devanagari vowel signs, which the `regex`
///   crate's `\w` would include.
/// - Tag names match case-insensitively, `ı` and `İ` counting as `i`.
///
/// The `regex` crate has no lookahead for `any tag but these`, so two
/// anchored regexes check each `<`.
pub fn strip_unformatting(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after_open)) = rest.split_once('<') {
        stripped.push_str(before);
        if let Some(after_tag) = skip_tag(after_open) {
            rest = after_tag;
        } else {
            stripped.push('<');
            rest = after_open;
        }
    }
    stripped.push_str(rest);
    stripped
}

/// For the text after a `<`, the text after the tag it removes, or `None`
/// if it keeps the tag or finds no end.
fn skip_tag(after_open: &str) -> Option<&str> {
    // Keep a tag that names a formatting tag both with and without its
    // leading `/`: `</b>` stays and `<//b>` goes.
    if keeps(after_open) && after_open.strip_prefix('/').is_none_or(keeps) {
        return None;
    }
    let tag = regex_find!(r"^[^\n]*?[\p{L}\p{N}_]>", after_open)?;
    after_open.strip_prefix(tag)
}

/// Whether `text` starts with an optional `/` and a formatting tag name,
/// followed by a non-word character or the end. Only the names are
/// case-insensitive: under `(?i)` the `regex` crate also case-folds the
/// class, and U+0345 folds to `ι`, a letter.
fn keeps(text: &str) -> bool {
    regex_is_match!(
        r"^/?(?i:strong|em|b|[iıİ]|mark|small|del|[iıİ]ns|sub|sup)(?:[^\p{L}\p{N}_]|$)",
        text
    )
}

/// Properties of tag stripping over generated text.
#[cfg(test)]
mod properties {
    use proptest::prelude::*;

    use super::{strip_all, strip_unformatting};

    proptest! {
        /// No `<...>` remains, and the text never grows.
        #[test]
        fn strip_all_leaves_no_tag(text in any::<String>()) {
            let stripped = strip_all(&text);
            prop_assert!(!lazy_regex::regex_is_match!(r"<[^>]*>", &stripped));
            prop_assert!(stripped.len() <= text.len());
        }

        /// Stripping only removes text.
        #[test]
        fn strip_unformatting_only_removes(text in any::<String>()) {
            prop_assert!(strip_unformatting(&text).len() <= text.len());
        }

        /// Text without a `<` stays the same.
        #[test]
        fn text_without_tags_is_unchanged(text in "[^<]*") {
            prop_assert_eq!(strip_all(&text), text.as_str());
            prop_assert_eq!(strip_unformatting(&text), text.as_str());
        }

        /// Formatting tags stay with `strip_unformatting` and go with
        /// `strip_all`; other tags go with both.
        #[test]
        fn formatting_tags_are_kept(
            text in "[a-z ]{0,20}",
            tag in prop::sample::select(&["b", "i", "em", "strong", "mark", "small", "del", "ins", "sub", "sup"][..]),
        ) {
            let formatted = format!("<{tag}>{text}</{tag}>");
            prop_assert_eq!(strip_unformatting(&formatted), formatted.as_str());
            prop_assert_eq!(strip_all(&formatted), text.as_str());
            let other = format!("<span>{text}</span>");
            prop_assert_eq!(strip_unformatting(&other), text.as_str());
        }
    }
}

/// Tag stripping cases.
#[cfg(test)]
mod tests {
    use super::{strip_all, strip_unformatting};

    /// `strip_all` removes every tag.
    #[test]
    fn strip_all_removes_every_tag() {
        assert_eq!(strip_all("a <i>b</i> <font x='1'>c</font>"), "a b c");
    }

    /// Formatting tags stay in any case.
    #[test]
    fn keeps_formatting_tags_in_any_case() {
        assert_eq!(
            strip_unformatting("<B>x</b> <Strong>y</STRONG> <u>z</u>"),
            "<B>x</b> <Strong>y</STRONG> z"
        );
    }

    /// A non-word character before `>` lets the tag run on.
    #[test]
    fn non_word_character_before_the_close_runs_on() {
        assert_eq!(
            strip_unformatting(r#"a <font color="red">c</font> d"#),
            "a  d"
        );
    }

    /// Tag names fold `ſ` to `s`; a full-width `Ｂ` isn't `b`.
    #[test]
    fn unicode_case_folding() {
        assert_eq!(strip_unformatting("<ſub>x</ſub>"), "<ſub>x</ſub>");
        assert_eq!(strip_unformatting("<Ｂ>x</Ｂ>"), "x");
    }

    /// The output matches the reference's on recorded cases.
    #[test]
    fn matches_reference_output() {
        // Random caption text and the expected output, recorded from
        // youtube-transcript-api by scripts/record_golden.py.
        let cases: Vec<(String, String)> =
            serde_json::from_str(include_str!("../tests/golden/strip_tags.json")).unwrap();
        assert_eq!(cases.len(), 3000);
        for (text, expected) in &cases {
            assert_eq!(&strip_unformatting(text), expected, "{text:?}");
        }
    }

    /// Combining marks aren't word characters.
    #[test]
    fn combining_marks_are_not_word_characters() {
        // U+0940 is a vowel sign: the tag runs on to the next `>` after a
        // word character, and a mark ends a kept tag's name.
        assert_eq!(strip_unformatting("<u \u{940}>x<u>"), "");
        assert_eq!(strip_unformatting("<u \u{940}>"), "<u \u{940}>");
        assert_eq!(strip_unformatting("<ıns>x</İ>"), "<ıns>x</İ>");
        assert_eq!(strip_unformatting("<STRONG\u{345}>"), "<STRONG\u{345}>");
    }

    /// An unclosed tag, or one cut by a newline, stays.
    #[test]
    fn unclosed_tags_and_newlines_stop_a_match() {
        assert_eq!(strip_unformatting("<a\nb>"), "<a\nb>");
        assert_eq!(strip_unformatting("<a "), "<a ");
    }
}
