# Contributing

Thanks for helping. Bug reports, fixes and features are all welcome. For a
large change, open an issue first to agree on the approach.

## Setup

`rust-toolchain.toml` pins the toolchain; rustup installs it on first use.
The checks run as [pre-commit](https://pre-commit.com) hooks:

```bash
pip install pre-commit
pre-commit install
cargo install --locked cargo-deny cargo-llvm-cov
```

The Markdown hook runs `npx`, so it also needs Node.

## Checks

A change must pass these, which CI runs too:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo test --workspace --all-features
scripts/coverage.sh
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --all-features
cargo deny check
```

- **Lints:** clippy runs with the pedantic, nursery, and cargo groups as
  errors. Fix the code rather than adding `#[allow]`s.
- **Coverage:** tests cover every line, function, and region of each crate,
  the library by its own tests and the command line by its own
  (`scripts/coverage.sh` runs both). A new branch needs a test that takes it.
- **Prose:** [Vale](https://vale.sh) checks the Markdown and the comments in
  Rust files against the Google developer documentation style. Rewrite a
  flagged sentence rather than adding an ignore.
- **File length:** no file passes 800 lines; split it by area first.
- **No unwraps outside tests.** Helpers that must not fail use
  `unwrap_or_else(|e| unreachable!("{e}"))`.

## Tests

The repository is a workspace of two crates: the library in
`crates/youtube-transcript-api` and the command line in
`crates/youtube-transcript-api-cli`.

- `crates/youtube-transcript-api/tests/api/` runs the library against a fake
  YouTube (`tests/common`), which answers with the recorded responses in
  `tests/fixtures`.
- `tests/reqwest_client.rs` there covers the real HTTP client against a local
  server.
- The command line's tests are in its crate (`src/cli/`, `tests/bin.rs`) and
  read the same recorded responses.
- `crates/youtube-transcript-api/tests/golden` holds reference output:
  error messages, transcript lists, formatter output, timestamps, and tag
  stripping. `scripts/record_golden.py` records them from
  youtube-transcript-api, so the command line and output formats stay
  compatible with it. Re-record them; don't edit them by hand.

## Commits and pull requests

- Write commit messages as [Conventional Commits][cc] (`feat:`, `fix:`,
  `docs:`, …) that say what changed and why.
- Add a line to the newest section of `CHANGELOG.md` for anything a user
  would notice.
- Keep a pull request to one change, with its tests.

[cc]: https://www.conventionalcommits.org
