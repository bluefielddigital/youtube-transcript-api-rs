//! Argument parsing and output writing.

use std::time::Duration;

use clap::Parser;
use youtube_transcript_api::RawFormat;
use youtube_transcript_api::formatters::FormatterKind;

use super::{Args, Output, format_help, parse_timeout, raw_help};

/// Parses a command line given after the program name.
fn parse(line: &str) -> Args {
    Args::try_parse_from(std::iter::once("youtube-transcript-api").chain(line.split(' '))).unwrap()
}

/// Options and video IDs mix in any order.
#[test]
fn options_and_ids_in_any_order() {
    for line in [
        "v1 v2 --format json --languages de en",
        "v1 v2 --languages de en --format json",
        "--format json v1 v2 --languages de en",
    ] {
        let args = parse(line);
        assert_eq!(args.video_ids, ["v1", "v2"], "{line}");
        assert_eq!(args.format, FormatterKind::Json);
        assert_eq!(args.languages, ["de", "en"]);
        assert_eq!(args.http_proxy, None);
    }
}

/// The defaults for every option.
#[test]
fn defaults() {
    let args = parse("v1 v2");
    assert_eq!(args.format, FormatterKind::Pretty);
    assert_eq!(args.languages, ["en"]);
    assert!(!args.list_transcripts);
    assert_eq!(args.translate, None);
}

/// A backslash escapes an ID starting with `-`.
#[test]
fn escaped_ids_starting_with_a_dash() {
    assert_eq!(
        parse(r"\-v1 \-\-v2 \--v3").video_ids(),
        ["-v1", "--v2", "--v3"]
    );
}

/// Parsing fails without a video ID.
#[test]
fn video_ids_are_required() {
    assert!(Args::try_parse_from(["youtube-transcript-api", "--format", "json"]).is_err());
}

/// An unknown `--format` fails to parse.
#[test]
fn unknown_format_is_rejected() {
    assert!(Args::try_parse_from(["youtube-transcript-api", "v1", "--format", "xml"]).is_err());
}

/// Webshare needs both the username and the password.
#[test]
fn webshare_needs_both_username_and_password() {
    let args = [
        "youtube-transcript-api",
        "v1",
        "--webshare-proxy-username",
        "u",
    ];
    assert!(Args::try_parse_from(args).is_err());
}

/// Every proxy option builds a client.
#[test]
fn proxies_build_a_client() {
    assert!(parse("v1 --http-proxy http://h:1").build_api().is_ok());
    assert!(parse("v1 --https-proxy http://h:1").build_api().is_ok());
    let args = parse("v1 --webshare-proxy-username u --webshare-proxy-password p");
    assert!(args.build_api().is_ok());
    assert!(parse("v1").build_api().is_ok());
}

/// The on/off flags and `--translate`.
#[test]
fn flags() {
    let args = parse("--list-transcripts v1 --translate cz --exclude-generated");
    assert!(args.list_transcripts);
    assert!(args.matching.exclude_generated);
    assert!(!args.matching.exclude_manually_created);
    assert_eq!(args.translate.as_deref(), Some("cz"));
}

/// What landed in stdout and stderr.
type Written = (Vec<u8>, Vec<u8>);

// Every test goes through this one instantiation of the generic
// `write_to`, so coverage sees every path in it at once.
/// Writes `output` into buffers of `capacity` bytes and returns what
/// landed in each.
fn write(output: &Output, capacity: (usize, usize)) -> std::io::Result<Written> {
    let (mut stdout, mut stderr) = (vec![0_u8; capacity.0], vec![0_u8; capacity.1]);
    let (mut out, mut err) = (&mut stdout[..], &mut stderr[..]);
    output.write_to(&mut out, &mut err)?;
    let (out_left, err_left) = (out.len(), err.len());
    stdout.truncate(capacity.0 - out_left);
    stderr.truncate(capacity.1 - err_left);
    Ok((stdout, stderr))
}

/// Errors go to stderr, one per line, and the output to stdout.
#[test]
fn output_writes_errors_then_stdout() {
    let output = Output {
        stdout: "out".to_owned(),
        errors: vec!["e1".to_owned(), "e2".to_owned()],
    };
    let (stdout, stderr) = write(&output, (16, 16)).unwrap();
    assert_eq!(stdout, b"out\n");
    assert_eq!(stderr, b"e1\ne2\n");
}

/// An empty output writes nothing.
#[test]
fn empty_output_writes_nothing() {
    let (stdout, stderr) = write(&Output::default(), (16, 16)).unwrap();
    assert_eq!(stdout, b"");
    assert_eq!(stderr, b"");
}

/// A full stdout or stderr is an error.
#[test]
fn write_errors_are_returned() {
    let output = Output {
        stdout: "out".to_owned(),
        errors: vec!["e".to_owned()],
    };
    assert!(write(&output, (0, 16)).is_err());
    assert!(write(&output, (16, 0)).is_err());
}

/// `--preserve-formatting`, `--match-language-variants`, `--raw` and
/// `--timeout`.
#[test]
fn new_options() {
    let args =
        parse("v1 --preserve-formatting --match-language-variants --raw json3 --timeout 2.5");
    assert!(args.preserve_formatting);
    assert!(args.matching.match_language_variants);
    assert_eq!(args.raw, Some(RawFormat::Json3));
    assert_eq!(args.timeout, Some(Duration::from_millis(2500)));
    assert!(args.build_api().is_ok());
    let plain = parse("v1");
    assert!(!plain.preserve_formatting);
    assert_eq!((plain.raw, plain.timeout), (None, None));
}

/// A timeout is a finite, non-negative number of seconds.
#[test]
fn timeouts_must_be_a_usable_number_of_seconds() {
    assert_eq!(parse_timeout("0.25"), Ok(Duration::from_millis(250)));
    for bad in ["soon", "-1", "inf", "NaN"] {
        assert!(parse_timeout(bad).is_err(), "{bad}");
    }
}

/// Bad timeouts, unknown raw formats and `--raw` with `--format` fail to
/// parse.
#[test]
fn bad_timeouts_and_raw_with_format_are_rejected() {
    for line in [
        "v1 --timeout soon",
        "v1 --timeout -1",
        "v1 --raw srv3 --format json",
        "v1 --raw xml",
    ] {
        let parsed =
            Args::try_parse_from(std::iter::once("youtube-transcript-api").chain(line.split(' ')));
        assert!(parsed.is_err(), "{line}");
    }
}

/// An empty proxy URL fails when building the client.
#[test]
fn empty_proxy_url_is_rejected() {
    let args = Args::try_parse_from(["youtube-transcript-api", "v1", "--http-proxy", ""]).unwrap();
    assert!(args.build_api().is_err());
}

/// Every formatter and raw format the library offers has a `--help` line, so a
/// format added to the library can't reach the command line without one.
#[test]
fn every_format_has_help() {
    for &kind in FormatterKind::ALL {
        assert_ne!(format_help(kind), "", "{kind:?}");
    }
    for &format in RawFormat::ALL {
        assert_ne!(raw_help(format), "", "{format:?}");
    }
}
