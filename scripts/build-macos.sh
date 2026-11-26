#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -sm)" = "Darwin arm64" ]] || { echo 'Apple Silicon Mac で実行してください。' >&2; exit 1; }
media="target/media-macos/ffmpeg-7.1.2"
if [[ ! -x "$media/ffmpeg" || ! -x "$media/ffprobe" ]]; then scripts/build-media.sh; fi
export MACOSX_DEPLOYMENT_TARGET=12.0
cargo build --release --locked --target aarch64-apple-darwin --target-dir target
scripts/stage-package.sh macos target/aarch64-apple-darwin/release/FastEdit "$media" "$media.tar.xz"
app=dist/FastEdit.app
for name in ffmpeg ffprobe; do codesign --force --sign - "$app/Contents/MacOS/$name"; done
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
echo "$PWD/$app"
