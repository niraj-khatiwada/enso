#!/bin/sh
# The full macOS release: optimized build, Enso.app with its Quick Look
# extension, signing, and a drag-to-Applications disk image.
set -eu
cd "$(dirname "$0")/.."

step() {
    printf '\n==> %s\n' "$1"
}

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(uname -m)
app="$PWD/target/Enso.app"
dmg="$PWD/target/Enso-$version-$arch.dmg"

# bundle-macos.sh builds the release binary, assembles Enso.app, builds the
# Quick Look extension into it (build-quicklook.sh), and signs the bundle.
step "Building Enso $version ($arch): app, Quick Look extension, signature"
./scripts/bundle-macos.sh

step "Checking the signature"
codesign --verify --deep --strict "$app"

step "Creating $(basename "$dmg")"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
rm -f "$dmg"
hdiutil create \
    -volname "Enso $version" \
    -srcfolder "$staging" \
    -fs HFS+ \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    "$dmg" >/dev/null 2>&1
hdiutil verify "$dmg" >/dev/null 2>&1

step "Done"
printf 'App: %s\nDMG: %s (%s)\n' "$app" "$dmg" "$(du -h "$dmg" | cut -f1 | tr -d ' ')"
