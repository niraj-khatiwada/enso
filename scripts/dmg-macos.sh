#!/bin/sh
# Build a release Enso.app and wrap it in a drag-to-Applications disk image.
set -eu
cd "$(dirname "$0")/.."

./scripts/bundle-macos.sh
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(uname -m)
dmg="$PWD/target/Enso-$version-$arch.dmg"

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R target/Enso.app "$staging/"
ln -s /Applications "$staging/Applications"

rm -f "$dmg"
hdiutil create \
    -volname "Enso $version" \
    -srcfolder "$staging" \
    -fs HFS+ \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    "$dmg" >/dev/null

printf '%s\n' "$dmg"
