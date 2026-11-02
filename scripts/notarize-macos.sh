#!/usr/bin/env bash
# Signs the macOS binary in each release archive with a Developer ID
# certificate (hardened runtime, secure timestamp), has Apple notarize it, and
# repacks the archive in place. A bare binary can't be stapled, so Gatekeeper
# checks the notarization ticket online the first time it runs.
#
# Usage: notarize-macos.sh ARCHIVE.tar.gz...
#
# Environment:
#   IDENTITY        the signing identity, e.g. "Developer ID Application:
#                   Name (TEAMID)"; `-` signs ad hoc (with NOTARIZE=0, to
#                   test the script without a certificate)
#   NOTARIZE        0 to sign only; default 1
#   NOTARY_PROFILE  a `notarytool store-credentials` profile, for local use;
#                   otherwise an App Store Connect API key:
#   APPLE_API_KEY_PATH, APPLE_API_KEY_ID, APPLE_API_ISSUER_ID

set -euo pipefail

: "${IDENTITY:?set IDENTITY to the signing identity}"
notarize=${NOTARIZE:-1}
bin=youtube-transcript-api
# A stable code-signing identifier; codesign's default has a hash that
# changes with every build.
identifier=com.bluefielddigital.youtube-transcript-api

if [ "$notarize" = 1 ]; then
  if [ -n "${NOTARY_PROFILE:-}" ]; then
    auth=(--keychain-profile "$NOTARY_PROFILE")
  else
    auth=(--key "${APPLE_API_KEY_PATH:?}" --key-id "${APPLE_API_KEY_ID:?}" --issuer "${APPLE_API_ISSUER_ID:?}")
  fi
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for archive in "$@"; do
  name=$(basename "$archive" .tar.gz)
  tar -C "$work" -xzf "$archive"
  binary="$work/$name/$bin"

  codesign --force --options runtime --timestamp --identifier "$identifier" \
    --sign "$IDENTITY" "$binary"
  codesign --verify --strict --verbose=2 "$binary"

  if [ "$notarize" = 1 ]; then
    ditto -c -k --keepParent "$binary" "$work/$name.zip"
    result=$(xcrun notarytool submit "$work/$name.zip" "${auth[@]}" --wait --timeout 30m --output-format json)
    status=$(printf '%s' "$result" | plutil -extract status raw -o - -)
    if [ "$status" != Accepted ]; then
      id=$(printf '%s' "$result" | plutil -extract id raw -o - -)
      echo "notarization of $name: $status" >&2
      xcrun notarytool log "$id" "${auth[@]}" >&2 || true
      exit 1
    fi
    echo "notarized $name"
  fi

  tar -C "$work" -czf "$archive" "$name"
done
