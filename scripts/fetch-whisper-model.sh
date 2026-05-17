#!/usr/bin/env bash
# 把 Whisper base 模型拉到 src-tauri/resources/ —— tauri.conf.json 会把它打进 .app
# bundle 的 Resources/models/ 下，transcribe.rs 首次启动从那儿 copy 到
# ~/.mouseclaw/models/ 给 whisper-rs 用。
#
# 模型 57MB，不放进 git；CI 和本地构建前各自拉一次（有缓存就跳过）。
# bun run tauri build 会通过 package.json 的 prebuild 钩子先调这个脚本。

set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources"
FILE="$DIR/ggml-base-q5_1.bin"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin"
EXPECTED_BYTES=59707625  # base q5_1 是定数；拉错了会用大小兜底

mkdir -p "$DIR"

if [ -f "$FILE" ]; then
  actual=$(stat -f%z "$FILE" 2>/dev/null || stat -c%s "$FILE")
  if [ "$actual" = "$EXPECTED_BYTES" ]; then
    echo "✅ Whisper base model already cached at $FILE"
    exit 0
  fi
  echo "⚠️  $FILE size mismatch ($actual vs $EXPECTED_BYTES). Re-downloading…"
  rm -f "$FILE"
fi

echo "⬇️  Downloading Whisper base (~57MB) from HuggingFace…"
curl -fL --progress-bar -o "$FILE.tmp" "$URL"
mv "$FILE.tmp" "$FILE"
echo "✅ Saved → $FILE"
