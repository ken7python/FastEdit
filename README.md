# FastEdit

Mac・Windows 向けの Rust 製動画エディターです。素材をドロップすると最初の素材がタイムラインに入ります。追加した素材は左の一覧から「末尾につなぐ」「上に重ねる」「音声を追加」で配置できます。

## ダウンロード

[GitHub Releases](https://github.com/ken7python/FastEdit/releases) から使用する OS の配布ファイルを取得します。

- **Mac（Apple Silicon、macOS 12 以降）** — DMG を開き、`FastEdit.app` を Applications にドラッグして起動します。初版はアドホック署名で、公証は未取得です。
- **Windows（x64）** — 展開したフォルダー内の `FastEdit.exe` を起動します。同じフォルダーの `ffmpeg.exe` と `ffprobe.exe` も使用します。初版の Windows バイナリは未署名です。

## 使い方

1. タイムライン上でクリップをクリックし、再生位置を合わせて「分割」します。「詰めて削除」は選んだ主映像の区間を字幕・音声も含めて詰めます。
2. 「テキストを追加」で本文を入力し、右側で書体・色・表示時間を設定します。プレビューの選択枠をドラッグして移動、右下のハンドルで拡縮します。
3. 元音声の区間だけを編集するときは「音声を切り離す」を押し、音声レーンを選んで分割・削除します。音量は dB で調整できます。
4. 「保存」で編集内容を `.fastedit` に保存します。別の PC に移すときは素材も一緒にコピーしてください。「動画を書き出す」は右側の画面サイズに従って H.264 / AAC の MP4 を作ります。

## ローカルビルド

**macOS**　Apple Silicon Mac に Rust stable と Xcode Command Line Tools を用意し、リポジトリ直下で `./scripts/build-macos.sh` を実行します。完成した `dist/FastEdit.app` は Finder から起動できます。ローカル用のアドホック署名を使用します。

**Windows**　x64 の Rust stable（MSVC）、Visual Studio C++ Build Tools と Windows SDK、MSYS2 を用意します。MSYS2 UCRT64 のターミナルで `pacman -S --needed python make diffutils tar curl xz mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-zlib` を実行後、PowerShell で `.\scripts\build-windows.ps1` を実行します。配布フォルダーは `dist\FastEdit-Windows-x64` です。動画の書き出しには Windows の Media Foundation を使用します。

初回ビルドでは Rust の依存パッケージと FFmpeg 7.1.2 のソースをダウンロードします。完成した配布物には FFmpeg と ffprobe が含まれます。開発時に `cargo run` を使用する場合は、各 OS 用の FFmpeg と ffprobe を PATH に用意してください。実機での確認項目は [設計書](DESIGN.md) にあります。
