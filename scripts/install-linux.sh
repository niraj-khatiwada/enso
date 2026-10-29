#!/bin/sh
# Install enso for the current user on Linux: a launcher entry with its icon,
# which docks and task switchers match to the running window by its app id.
# Usage: scripts/install-linux.sh [path/to/enso]   (default: target/release/enso,
# else target/debug/enso). Run it again after moving the binary.
set -eu
cd "$(dirname "$0")/.."

id=app.enso.markdown
bin=${1:-}
if [ -z "$bin" ]; then
    if [ -x target/release/enso ]; then bin=target/release/enso; else bin=target/debug/enso; fi
fi
[ -f "$bin" ] || { echo "No enso binary at $bin: build it first (cargo build)." >&2; exit 1; }
# Archive tools that drop the executable bit would leave the launcher broken.
chmod +x "$bin" 2>/dev/null || true
bin=$(realpath "$bin")

data=${XDG_DATA_HOME:-$HOME/.local/share}
mkdir -p "$data/applications"
# The icon at the sizes docks and launchers ask the icon theme for.
for size in 48 64 128 256 512; do
    dir="$data/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    cp "assets/linux/${size}x${size}/$id.png" "$dir/$id.png"
done
# The entry names the icon by its full path: every desktop accepts that,
# whatever its icon theme or icon caches hold.
icon="$data/icons/hicolor/512x512/apps/$id.png"

cat > "$data/applications/$id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=enso
Comment=A quiet, native Markdown workspace
Exec="$bin" %F
Icon=$icon
Terminal=false
Categories=Office;TextEditor;
MimeType=text/markdown;text/x-markdown;text/plain;
StartupWMClass=$id
DESKTOP

# Let launchers and docks notice the new entry. Icon caches are refreshed only
# where they already exist: forcing a new one can hide icons added later.
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
if [ -f "$data/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -f "$data/icons/hicolor" || true
fi
for rebuild in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$rebuild" >/dev/null 2>&1; then
        "$rebuild" >/dev/null 2>&1 || true
        break
    fi
done
printf 'Installed %s -> %s\n' "$data/applications/$id.desktop" "$bin"
echo "If the dock still shows a generic icon, log out and back in to refresh it."
