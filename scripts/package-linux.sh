#!/bin/sh
# Every Linux package of a release build (cargo build --release first):
#   enso-<version>-linux-<arch>.tar.gz   the binary, icons and install.sh (any distro)
#   enso-<version>-linux-<arch>.deb      Debian, Ubuntu and derivatives
#   enso-<version>-linux-<arch>.rpm      Fedora, openSUSE, RHEL and derivatives
#   enso-<version>-linux-<arch>.AppImage one file that runs on most distros
# Usage: scripts/package-linux.sh <x64|arm64> [output folder, default dist]
# Needs cargo-deb, cargo-generate-rpm, dpkg-dev, and curl for linuxdeploy.
set -eu
cd "$(dirname "$0")/.."

arch=${1:?Usage: scripts/package-linux.sh <x64|arm64> [output folder]}
out=$(mkdir -p "${2:-dist}" && cd "${2:-dist}" && pwd)
case "$arch" in
    x64) machine=x86_64 ;;
    arm64) machine=aarch64 ;;
    *) echo "Unknown architecture: $arch (x64 or arm64)" >&2; exit 1 ;;
esac
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
name="enso-$version-linux-$arch"
bin=target/release/enso
[ -x "$bin" ] || { echo "No release build at $bin: run cargo build --release first." >&2; exit 1; }
id=app.enso.markdown

echo "==> $name.tar.gz"
# The binary, its launcher icon and the script that installs both: after
# unpacking, run ./install.sh (or sh install.sh) to add enso to the launcher.
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dir="$stage/$name"
mkdir -p "$dir/assets" "$dir/scripts"
cp "$bin" "$dir/enso"
cp assets/font-licenses.txt README.md "$dir/"
cp -R assets/linux "$dir/assets/"
cp scripts/install-linux.sh "$dir/scripts/"
cat > "$dir/install.sh" <<'SH'
#!/bin/sh
# Add enso to the app launcher, with its icon, for the current user.
# Works as `sh install.sh` too: some archive tools unpack without
# the executable bits, so they're restored here.
here=$(cd "$(dirname "$0")" && pwd)
chmod +x "$here/enso" "$here/scripts/install-linux.sh" "$here/install.sh" 2>/dev/null || true
exec sh "$here/scripts/install-linux.sh" "$here/enso"
SH
chmod +x "$dir/install.sh" "$dir/enso"
tar -czf "$out/$name.tar.gz" -C "$stage" "$name"

echo "==> $name.deb"
cargo deb --no-build --no-strip --output "$out/$name.deb"

echo "==> $name.rpm"
cargo generate-rpm --output "$out/$name.rpm"

echo "==> $name.AppImage"
# linuxdeploy copies the libraries enso links into the AppImage, except the
# ones every system has its own of (glibc, the GPU driver, fontconfig) and
# OpenSSL: the system's knows where its distro keeps trusted certificates.
tools="$PWD/target/appimage-tools"
mkdir -p "$tools"
linuxdeploy="$tools/linuxdeploy-$machine.AppImage"
if [ ! -x "$linuxdeploy" ]; then
    curl -fsSL -o "$linuxdeploy" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$machine.AppImage"
    chmod +x "$linuxdeploy"
fi
appdir="$stage/AppDir"
rm -rf "$appdir"
for size in 48 64 128 256 512; do
    mkdir -p "$appdir/usr/share/icons/hicolor/${size}x${size}/apps"
    cp "assets/linux/${size}x${size}/$id.png" "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/"
done
# Runs without FUSE (CI containers): it unpacks itself instead.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$machine LDAI_OUTPUT="$out/$name.AppImage" OUTPUT="$out/$name.AppImage" \
    "$linuxdeploy" \
    --appdir "$appdir" \
    --executable "$bin" \
    --desktop-file "assets/linux/$id.desktop" \
    --icon-file "assets/linux/512x512/$id.png" \
    --exclude-library 'libssl.so*' \
    --exclude-library 'libcrypto.so*' \
    --output appimage

ls -l "$out"
