#!/bin/sh
# Build enso.app. Release by default; pass --debug for a quick unoptimized bundle.
set -eu
cd "$(dirname "$0")/.."

profile=release
if [ "${1:-}" = "--debug" ]; then
    profile=debug
fi

binary="target/$profile/enso"
if [ -n "${ENSO_TARGETS:-}" ]; then
    slices=""
    for target in $ENSO_TARGETS; do
        cargo build --locked --release --target "$target"
        slices="$slices target/$target/release/enso"
    done
    mkdir -p target/universal
    lipo -create $slices -output target/universal/enso
    binary=target/universal/enso
elif [ "$profile" = release ]; then
    cargo build --locked --release
else
    cargo build --locked
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
bundle="$PWD/target/enso.app"
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$binary" "$bundle/Contents/MacOS/enso"
cp assets/enso.icns "$bundle/Contents/Resources/enso.icns"
icon_name=""
if xcrun actool assets/enso.icon \
    --compile "$bundle/Contents/Resources" \
    --app-icon enso \
    --platform macosx \
    --target-device mac \
    --minimum-deployment-target 11.0 \
    --output-partial-info-plist "$PWD/target/icon-info.plist" \
    --output-format human-readable-text >/dev/null 2>&1 \
    && [ -f "$bundle/Contents/Resources/Assets.car" ]; then
    icon_name="<key>CFBundleIconName</key><string>enso</string>"
else
    rm -f "$bundle/Contents/Resources/Assets.car"
    cp assets/enso.icns "$bundle/Contents/Resources/enso.icns"
    echo "Xcode 26 or later is needed for the layered macOS 26 icon; using the flat icon." >&2
fi
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>enso</string>
<key>CFBundleDisplayName</key><string>enso</string>
<key>CFBundleIdentifier</key><string>app.enso.markdown</string>
<key>CFBundleExecutable</key><string>enso</string>
<key>CFBundleIconFile</key><string>enso</string>
$icon_name
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
<key>CFBundleDocumentTypes</key>
<array>
  <dict>
    <key>CFBundleTypeName</key><string>Markdown Document</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSHandlerRank</key><string>Default</string>
    <key>LSItemContentTypes</key><array><string>net.daringfireball.markdown</string></array>
  </dict>
  <dict>
    <key>CFBundleTypeName</key><string>Folder</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSHandlerRank</key><string>None</string>
    <key>LSItemContentTypes</key><array><string>public.folder</string></array>
  </dict>
</array>
<key>UTImportedTypeDeclarations</key>
<array>
  <dict>
    <key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
    <key>UTTypeDescription</key><string>Markdown Document</string>
    <key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
    <key>UTTypeTagSpecification</key>
    <dict>
      <key>public.filename-extension</key>
      <array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
      <key>public.mime-type</key><array><string>text/markdown</string></array>
    </dict>
  </dict>
</array>
</dict></plist>
PLIST

# The Quick Look preview for Markdown files, signed with its own entitlements.
./scripts/build-quicklook.sh "$bundle"

# Ad-hoc signature: Apple Silicon refuses to run unsigned binaries. This is not
# a Developer ID signature, so Gatekeeper still warns on other Macs. Not
# --deep: that would re-sign the extension without its sandbox entitlements.
codesign --force --sign - "$bundle"

printf '%s\n' "$bundle"
