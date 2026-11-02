//! The `youtube-transcript-api` command-line tool.

/// Argument parsing, running, and output.
mod cli;

use std::io;
use std::process::ExitCode;

use clap::Parser;

use cli::{Args, Output, report, run};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = Args::parse();
    let output = match args.build_api() {
        Ok(api) => run(&args, &api).await,
        Err(error) => Output {
            stdout: String::new(),
            errors: vec![report(&error)],
        },
    };
    let written = output.write_to(io::stdout().lock(), io::stderr().lock());
    if written.is_err() || !output.errors.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
