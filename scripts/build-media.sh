#!/usr/bin/env bash
# Run in macOS Terminal or the MSYS2 UCRT64 shell.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
case "$(uname -s)" in
    Darwin) platform=macos; suffix=""; flags=(--enable-videotoolbox --enable-audiotoolbox --enable-zlib);;
    MINGW*|MSYS*) platform=windows; suffix=.exe; flags=(--enable-mediafoundation --disable-x86asm --enable-zlib --extra-ldflags=-static);;
    *) echo 'Run this script on macOS or in MSYS2 UCRT64.' >&2; exit 1;;
esac
dir="$root/target/media-$platform"
mkdir -p "$dir"
cd "$dir"
version=7.1.2
archive="ffmpeg-$version.tar.xz"
if [[ ! -f "$archive" ]]; then
    curl -fL --retry 2 "https://ffmpeg.org/releases/$archive" -o "$archive.part"
    mv "$archive.part" "$archive"
fi
expected=089bc60fb59d6aecc5d994ff530fd0dcb3ee39aa55867849a2bbc4e555f9c304
if command -v sha256sum >/dev/null; then digest="$(sha256sum "$archive")"; else digest="$(shasum -a 256 "$archive")"; fi
[[ "${digest%% *}" = "$expected" ]] || { echo "Archive checksum mismatch" >&2; exit 1; }
tar -xf "$archive"
cd "ffmpeg-$version"
./configure --disable-autodetect --disable-ffplay --disable-debug --disable-doc "${flags[@]}"
make -j"${JOBS:-4}" "ffmpeg$suffix" "ffprobe$suffix"
