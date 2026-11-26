param([string]$MsysRoot = "C:\msys64")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $bash = Join-Path $MsysRoot "usr\bin\bash.exe"
    if (!(Test-Path $bash)) { throw "MSYS2 UCRT64 is required. See README.md." }
    $env:CHERE_INVOKING = "1"
    $env:MSYSTEM = "UCRT64"
    $media = "target/media-windows/ffmpeg-7.1.2"
    if (!(Test-Path "$media/ffmpeg.exe") -or !(Test-Path "$media/ffprobe.exe")) {
        & $bash -lc 'scripts/build-media.sh'
        if ($LASTEXITCODE) { throw "FFmpeg build failed" }
    }
    cargo build --release --locked --target x86_64-pc-windows-msvc --target-dir target
    if ($LASTEXITCODE) { throw "Rust build failed" }
    & $bash -lc 'scripts/stage-package.sh windows target/x86_64-pc-windows-msvc/release/FastEdit.exe target/media-windows/ffmpeg-7.1.2 target/media-windows/ffmpeg-7.1.2.tar.xz'
    if ($LASTEXITCODE) { throw "Packaging failed" }
    Write-Output "$root\dist\FastEdit-Windows-x64\FastEdit.exe"
} finally { Pop-Location }
