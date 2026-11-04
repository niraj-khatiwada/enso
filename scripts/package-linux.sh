#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

binary="$1"
out="${2:-dist}"
id=app.enso.markdown
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
case "$(uname -m)" in
    x86_64) deb_arch=amd64; appimage_arch=x86_64 ;;
    aarch64 | arm64) deb_arch=arm64; appimage_arch=aarch64 ;;
    *) echo "Unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

desktop() {
    cat <<DESKTOP
[Desktop Entry]
Type=Application
Name=enso
GenericName=Markdown Editor
Comment=A quiet, native Markdown workspace
Exec=enso %F
Icon=$id
Terminal=false
Categories=Office;TextEditor;
MimeType=text/markdown;text/x-markdown;
StartupWMClass=$id
Keywords=markdown;notes;writing;editor;
DESKTOP
}

icons() {
    for size in 48 64 128 256 512; do
        dir="$1/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$dir"
        install -m 644 "assets/linux/${size}x${size}/$id.png" "$dir/$id.png"
    done
}

deb="$work/deb"
mkdir -p "$deb/DEBIAN" "$deb/usr/bin" "$deb/usr/share/applications"
install -m 755 "$binary" "$deb/usr/bin/enso"
desktop > "$deb/usr/share/applications/$id.desktop"
icons "$deb"
cat > "$deb/DEBIAN/control" <<CONTROL
Package: enso
Version: $version
Section: editors
Priority: optional
Architecture: $deb_arch
Installed-Size: $(du -sk "$deb/usr" | cut -f1)
Depends: libc6, libfontconfig1, libfreetype6, libxkbcommon0, libxkbcommon-x11-0, libxcb1, libx11-xcb1, libwayland-client0, libvulkan1, libzstd1
Recommends: mesa-vulkan-drivers
Maintainer: enso <https://github.com/viai957/enso>
Homepage: https://github.com/viai957/enso
Description: A quiet, native Markdown workspace
 Write in the typeset page and keep plain Markdown files on disk.
CONTROL
dpkg-deb --root-owner-group --build "$deb" "$out/enso-linux-$deb_arch.deb" >/dev/null

appdir="$work/enso.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications"
install -m 755 "$binary" "$appdir/usr/bin/enso"
desktop > "$appdir/$id.desktop"
cp "$appdir/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -m 644 "assets/linux/256x256/$id.png" "$appdir/$id.png"
icons "$appdir"
cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/enso" "$@"
APPRUN
chmod 755 "$appdir/AppRun"

tool="${APPIMAGETOOL:-$work/appimagetool}"
if [ ! -x "$tool" ]; then
    curl -fsSL -o "$tool" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$appimage_arch.AppImage"
    chmod +x "$tool"
fi
ARCH="$appimage_arch" "$tool" --appimage-extract-and-run --no-appstream \
    "$appdir" "$out/enso-linux-$appimage_arch.AppImage" >/dev/null
printf '%s\n' "$out/enso-linux-$deb_arch.deb" "$out/enso-linux-$appimage_arch.AppImage"
