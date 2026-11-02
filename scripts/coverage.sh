#!/usr/bin/env bash
# The coverage gate: every line, function and region of each crate must run,
# by that crate's own tests. Each crate gets its own run, so the CLI's tests
# can't cover the library or the other way round.
#
# Usage: coverage.sh [CRATE...]   (default: both crates)

set -euo pipefail

crates=("$@")
if [ "${#crates[@]}" -eq 0 ]; then
  crates=(youtube-transcript-api youtube-transcript-api-cli)
fi

for crate in "${crates[@]}"; do
  echo "== coverage: $crate"
  cargo llvm-cov --locked -p "$crate" --all-targets --all-features \
    --fail-uncovered-lines 0 --fail-uncovered-functions 0 \
    --fail-uncovered-regions 0
done
