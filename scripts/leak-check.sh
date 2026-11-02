#!/usr/bin/env bash
# Fails if a published file contains a term listed in .leakcheck-terms (one
# case-insensitive extended regex per line; # starts a comment). Published
# files are the tracked ones except the paths in .publicignore. Does nothing
# where either file is missing, as in a checkout of the published tree.
#
# The github.com/bluefielddigital/ URLs are the project's home and are allowed.
#
# Usage: leak-check.sh

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
[ -f .publicignore ] && [ -f .leakcheck-terms ] || exit 0

pattern=$(grep -v -e '^#' -e '^$' .leakcheck-terms | paste -sd'|' -)
[ -n "$pattern" ] || exit 0

specs=(.)
while IFS= read -r path; do
  specs+=(":(exclude)$path")
done < <(grep -v -e '^#' -e '^$' .publicignore)

status=0
while IFS= read -r -d '' file; do
  [ -f "$file" ] || continue
  hits=$(sed 's#github\.com/bluefielddigital/#github.com/ORG/#Ig' "$file" \
    | grep -n -a -i -E -e "$pattern" || true)
  if [ -n "$hits" ]; then
    while IFS= read -r line; do
      echo "$file:$line" >&2
    done <<<"$hits"
    status=1
  fi
done < <(git ls-files -z -- "${specs[@]}")

if [ "$status" -ne 0 ]; then
  echo "error: published files contain terms from .leakcheck-terms" >&2
fi
exit "$status"
