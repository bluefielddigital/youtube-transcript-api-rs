#!/usr/bin/env bash
# Prints the CHANGELOG.md section for VERSION, without its heading. Fails if
# there is no such section, so a release can't go out without notes.
#
# Usage: release-notes.sh VERSION

set -euo pipefail

version=${1:?usage: $0 VERSION}
changelog="$(dirname "$0")/../CHANGELOG.md"

notes=$(awk -v heading="## [$version]" '
  /^## \[/ { if (found) exit; found = index($0, heading) == 1; next }
  found
' "$changelog")

if [ -z "${notes//[[:space:]]/}" ]; then
  echo "CHANGELOG.md has no notes for $version" >&2
  exit 1
fi
# Drop the blank lines between the heading and the notes.
printf '%s\n' "${notes#"${notes%%[![:space:]]*}"}"
