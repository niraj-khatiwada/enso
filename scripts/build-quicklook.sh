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

appex="$bundle/Contents/PlugIns/EnsoQuickLook.appex"
rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS"
sed "s/__VERSION__/$version/g" "$source/Info.plist" > "$appex/Contents/Info.plist"

build_slice() {
    target=$1
    swift_arch=$2
    output=$3
    if [ -n "$target" ]; then
        CARGO_TARGET_DIR="$PWD/target/quicklook" \
            cargo build --release --manifest-path "$source/Cargo.toml" --target "$target"
        libs="$PWD/target/quicklook/$target/release"
    else
        CARGO_TARGET_DIR="$PWD/target/quicklook" \
            cargo build --release --manifest-path "$source/Cargo.toml"
        libs="$PWD/target/quicklook/release"
    fi
    xcrun swiftc -O \
        -parse-as-library \
        -application-extension \
        -module-name EnsoQuickLook \
        -target "$swift_arch-apple-macos12.0" \
        -import-objc-header "$source/bridge.h" \
        "$source/PreviewProvider.swift" \
        -L "$libs" -lenso_quicklook \
        -Xlinker -e -Xlinker _NSExtensionMain \
        -o "$output"
}

if [ -n "${ENSO_TARGETS:-}" ]; then
    slices=""
    for target in $ENSO_TARGETS; do
        case "$target" in
            aarch64-*) swift_arch=arm64 ;;
            *) swift_arch=${target%%-*} ;;
        esac
        out="$PWD/target/quicklook/EnsoQuickLook-$swift_arch"
        build_slice "$target" "$swift_arch" "$out"
        slices="$slices $out"
    done
    lipo -create $slices -output "$appex/Contents/MacOS/EnsoQuickLook"
else
    build_slice "" "$arch" "$appex/Contents/MacOS/EnsoQuickLook"
fi

# Extensions run sandboxed; the entitlements must be on the extension itself.
codesign --force --sign - \
    --entitlements "$source/QuickLook.entitlements" \
    --options runtime \
    "$appex"
