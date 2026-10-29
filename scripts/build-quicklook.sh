#!/bin/sh
# Build enso's Quick Look extension into an app bundle: the Rust Markdown
# renderer as a static library, linked into a Swift preview provider.
# Usage: scripts/build-quicklook.sh path/to/enso.app
set -eu
cd "$(dirname "$0")/.."

bundle="$1"
source=macos/quicklook
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
arch=$(uname -m)

CARGO_TARGET_DIR="$PWD/target/quicklook" \
    cargo build --release --manifest-path "$source/Cargo.toml"

appex="$bundle/Contents/PlugIns/EnsoQuickLook.appex"
rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS"
sed "s/__VERSION__/$version/g" "$source/Info.plist" > "$appex/Contents/Info.plist"

xcrun swiftc -O \
    -parse-as-library \
    -application-extension \
    -module-name EnsoQuickLook \
    -target "$arch-apple-macos12.0" \
    -import-objc-header "$source/bridge.h" \
    "$source/PreviewProvider.swift" \
    -L "$PWD/target/quicklook/release" -lenso_quicklook \
    -Xlinker -e -Xlinker _NSExtensionMain \
    -o "$appex/Contents/MacOS/EnsoQuickLook"

# Extensions run sandboxed; the entitlements must be on the extension itself.
codesign --force --sign - \
    --entitlements "$source/QuickLook.entitlements" \
    --options runtime \
    "$appex"
