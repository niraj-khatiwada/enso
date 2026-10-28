#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked
bundle="$PWD/target/Enso.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp target/debug/enso "$bundle/Contents/MacOS/Enso"
cp assets/Enso.icns "$bundle/Contents/Resources/Enso.icns"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Enso</string>
<key>CFBundleDisplayName</key><string>Enso</string>
<key>CFBundleIdentifier</key><string>app.enso.markdown</string>
<key>CFBundleExecutable</key><string>Enso</string>
<key>CFBundleIconFile</key><string>Enso.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$bundle"
