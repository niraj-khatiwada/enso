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
icon_dir="$data/icons/hicolor/512x512/apps"
mkdir -p "$icon_dir" "$data/applications"
cp assets/linux/$id.png "$icon_dir/$id.png"

cat > "$data/applications/$id.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=enso
Comment=A quiet, native Markdown workspace
Exec="$bin" %F
Icon=$id
Terminal=false
Categories=Office;TextEditor;
MimeType=text/markdown;text/x-markdown;text/plain;
StartupWMClass=$id
EOF

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
printf 'Installed %s -> %s\n' "$data/applications/$id.desktop" "$bin"
