//! The `youtube-transcript-api` binary, in cases that need no network.

use std::process::Command;

/// Runs the binary with `args` and captures its output.
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_youtube-transcript-api"))
        .args(args)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

/// Excluding both kinds prints nothing and exits 0.
#[test]
fn excluding_both_kinds_prints_nothing_and_succeeds() {
    let output = run(&["v1", "--exclude-manually-created", "--exclude-generated"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

/// An unusable proxy exits 1 with a message on stderr.
#[test]
fn unusable_proxy_fails_with_a_message_on_stderr() {
    let output = run(&["v1", "--http-proxy", "::not a url::"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_ne!(output.stderr, b"");
}

/// No video IDs is a usage error, exit code 2.
#[test]
fn missing_video_ids_is_a_usage_error() {
    let output = run(&["--format", "json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("<VIDEO_IDS>"));
}
