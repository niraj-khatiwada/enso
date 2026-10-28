#!/bin/sh
# Build Enso.app. Release by default; pass --debug for a quick unoptimized bundle.
set -eu
cd "$(dirname "$0")/.."

profile=release
if [ "${1:-}" = "--debug" ]; then
    profile=debug
fi

if [ "$profile" = release ]; then
    cargo build --locked --release
else
    cargo build --locked
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
bundle="$PWD/target/Enso.app"
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "target/$profile/enso" "$bundle/Contents/MacOS/Enso"
cp assets/Enso.icns "$bundle/Contents/Resources/Enso.icns"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Enso</string>
<key>CFBundleDisplayName</key><string>Enso</string>
<key>CFBundleIdentifier</key><string>app.enso.markdown</string>
<key>CFBundleExecutable</key><string>Enso</string>
<key>CFBundleIconFile</key><string>Enso.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST

# Ad-hoc signature: Apple Silicon refuses to run unsigned binaries. This is not
# a Developer ID signature, so Gatekeeper still warns on other Macs.
codesign --force --deep --sign - "$bundle"

printf '%s\n' "$bundle"
