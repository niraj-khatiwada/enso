#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

bundle="$1"
: "${MACOS_CERTIFICATE:?}" "${MACOS_CERTIFICATE_PASSWORD:?}"

keychain="${RUNNER_TEMP:-$(mktemp -d)}/enso-signing.keychain-db"
password=$(uuidgen)
certificate="${RUNNER_TEMP:-$(mktemp -d)}/enso-signing.p12"

printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > "$certificate"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$certificate" -k "$keychain" -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
rm -f "$certificate"

identity=$(security find-identity -v -p codesigning "$keychain" \
    | sed -n 's/.*"\(Developer ID Application[^"]*\)".*/\1/p' | head -n 1)
[ -n "$identity" ] || { echo "No Developer ID Application certificate in MACOS_CERTIFICATE." >&2; exit 1; }

codesign --force --timestamp --options runtime \
    --entitlements macos/quicklook/QuickLook.entitlements \
    --sign "$identity" \
    "$bundle/Contents/PlugIns/EnsoQuickLook.appex"
codesign --force --timestamp --options runtime --sign "$identity" "$bundle"
codesign --verify --deep --strict "$bundle"
printf '%s\n' "$identity"
