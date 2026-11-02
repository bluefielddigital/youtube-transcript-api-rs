#!/usr/bin/env bash
# Publishes one workspace crate to crates.io unless crates.io already has this
# version, so a rerun after a failed release is safe. Run it for the library
# first and the CLI second: the CLI depends on the library at this exact
# version, and `cargo publish` waits until crates.io serves the one it just
# published before it returns.
#
# Usage: publish-crate.sh CRATE VERSION
# Environment: CARGO_REGISTRY_TOKEN (a trusted-publishing or API token) and
# CRATES_IO_AGENT (crates.io asks API clients to identify themselves).

set -euo pipefail

crate=${1:?usage: $0 CRATE VERSION}
version=${2:?usage: $0 CRATE VERSION}

if curl -sf -A "${CRATES_IO_AGENT:?}" "https://crates.io/api/v1/crates/$crate/$version" > /dev/null; then
  echo "crates.io already has $crate $version."
else
  cargo publish --locked -p "$crate"
fi
