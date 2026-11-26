#!/usr/bin/env bash
# Arguments: platform, release executable, configured FFmpeg source tree, matching archive.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
platform="$1"; app="$2"; media="$3"; archive="$4"
if [[ "$platform" = macos ]]; then
    contents="$root/dist/FastEdit.app/Contents"
    bin="$contents/MacOS"; resources="$contents/Resources"; suffix=""
    mkdir -p "$bin" "$resources"
    cp "$root/assets/Info.plist" "$contents/"
    cp "$root/assets/FastEdit.icns" "$resources/"
elif [[ "$platform" = windows ]]; then
    bin="$root/dist/FastEdit-Windows-x64"; resources="$bin"; suffix=".exe"
    mkdir -p "$bin"
else
    exit 2
fi
install -m755 "$app" "$bin/FastEdit$suffix"
for name in ffmpeg ffprobe; do install -m755 "$media/$name$suffix" "$bin/$name$suffix"; done
notices="$resources/ThirdParty/FFmpeg"
mkdir -p "$notices"
cp "$media/COPYING.LGPLv2.1" "$media/LICENSE.md" "$media/config.h" "$archive" "$notices/"
cp "$root/scripts/build-media.sh" "$notices/"
cp "$root/assets/THIRD-PARTY.txt" "$resources/ThirdParty/"
case "$platform" in
    macos) target=aarch64-apple-darwin;;
    windows) target="${RUST_PACKAGE_TARGET:-x86_64-pc-windows-msvc}";;
esac
python3 "$root/scripts/collect-notices.py" "$target" "$resources/ThirdParty/Rust.txt"
