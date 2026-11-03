#!/bin/sh
# The full macOS release: optimized build, enso.app with its Quick Look
# extension, signing, and a drag-to-Applications disk image.
set -eu
cd "$(dirname "$0")/.."

step() {
    printf '\n==> %s\n' "$1"
}

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(uname -m)
app="$PWD/target/enso.app"
if [ -n "${ENSO_TARGETS:-}" ]; then
    arch=universal
fi
dmg="$PWD/target/${ENSO_DMG_NAME:-enso-$version-$arch.dmg}"

# bundle-macos.sh builds the release binary, assembles enso.app, builds the
# Quick Look extension into it (build-quicklook.sh), and signs the bundle.
step "Building enso $version ($arch): app, Quick Look extension, signature"
./scripts/bundle-macos.sh

if [ -n "${MACOS_CERTIFICATE:-}" ]; then
    step "Signing with the Developer ID certificate"
    ./scripts/sign-macos.sh "$app"
fi

step "Checking the signature"
codesign --verify --deep --strict "$app"

step "Creating $(basename "$dmg")"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
rm -f "$dmg"
hdiutil create \
    -volname "enso $version" \
    -srcfolder "$staging" \
    -fs HFS+ \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    "$dmg" >/dev/null 2>&1
hdiutil verify "$dmg" >/dev/null 2>&1

if [ -n "${MACOS_CERTIFICATE:-}" ]; then
    codesign --force --timestamp --sign "$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application[^"]*\)".*/\1/p' | head -n 1)" "$dmg"
fi
if [ -n "${APPLE_ID:-}" ] && [ -n "${MACOS_CERTIFICATE:-}" ]; then
    step "Notarizing $(basename "$dmg")"
    ./scripts/notarize-macos.sh "$dmg"
fi

step "Done"
printf 'App: %s\nDMG: %s (%s)\n' "$app" "$dmg" "$(du -h "$dmg" | cut -f1 | tr -d ' ')"
