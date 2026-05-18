#!/usr/bin/env bash
# v0.3 · 把 sherpa-onnx streaming Zipformer 模型拉到 src-tauri/resources/
# tauri.conf.json bundle.resources 会把它打进 .app 的 Resources/models/sherpa-zh-en/
# transcribe_stream.rs::try_seed_from_bundle() 首次启动从 bundle 拷到
# ~/.mouseclaw/models/sherpa-zh-en/ 给 sherpa-onnx 加载用。
#
# 4 个模型文件，总 ~189MB。不放 git，build 前各自 fetch 一次（有缓存就跳过）。
#
# 替代了 fetch-whisper-model.sh —— v0.3 起 Whisper 整套被删除。

set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/sherpa-zh-en"
HF_BASE="https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/resolve/main"

# 文件名 + 预期大小（字节）—— 大小用来 detect 半下载坏文件
# 大小是 2026-05 实测；如 HF 端 reupload 略变，去掉 size check 即可
declare -a FILES=(
  "encoder-epoch-99-avg-1.int8.onnx"
  "decoder-epoch-99-avg-1.onnx"
  "joiner-epoch-99-avg-1.int8.onnx"
  "tokens.txt"
)

mkdir -p "$DIR"

for file in "${FILES[@]}"; do
  dest="$DIR/$file"
  if [ -f "$dest" ] && [ -s "$dest" ]; then
    echo "✅ Cached: $file ($(stat -f%z "$dest" 2>/dev/null || stat -c%s "$dest") bytes)"
    continue
  fi
  echo "⬇️  Downloading $file from HuggingFace…"
  curl -fL --progress-bar -o "$dest.tmp" "$HF_BASE/$file"
  mv "$dest.tmp" "$dest"
  echo "    → $(stat -f%z "$dest" 2>/dev/null || stat -c%s "$dest") bytes"
done

echo "🦞 All 4 sherpa-zh-en model files ready in $DIR"
