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

# v0.3.6 · CT-Transformer 标点模型（int8 量化版 72MB）—— 给 sherpa 出来的纯
# 文本补 。，？！。本地推理 ~10ms 免费。从 k2-fsa GitHub release 拉 tarball，
# 解 model.int8.onnx 出来即可（其它文件 README / config 不用）。
PUNCT_DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/sherpa-punct"
PUNCT_FILE="$PUNCT_DIR/model.int8.onnx"
PUNCT_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/punctuation-models/sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8.tar.bz2"

mkdir -p "$PUNCT_DIR"
if [ -f "$PUNCT_FILE" ] && [ -s "$PUNCT_FILE" ]; then
  echo "✅ Cached punctuation model ($(stat -f%z "$PUNCT_FILE" 2>/dev/null || stat -c%s "$PUNCT_FILE") bytes)"
else
  TMP="$(mktemp -d)"
  echo "⬇️  Downloading punctuation tarball (~30MB compressed)…"
  curl -fL --progress-bar -o "$TMP/punct.tar.bz2" "$PUNCT_URL"
  echo "📦 Extracting model.int8.onnx…"
  (cd "$TMP" && tar -xjf punct.tar.bz2)
  # tarball 解压出一个同名目录，从里头挑 model.int8.onnx
  FOUND="$(find "$TMP" -name "model.int8.onnx" -type f | head -1)"
  if [ -z "$FOUND" ]; then
    echo "❌ model.int8.onnx not found in tarball"
    exit 1
  fi
  mv "$FOUND" "$PUNCT_FILE"
  rm -rf "$TMP"
  echo "    → $(stat -f%z "$PUNCT_FILE" 2>/dev/null || stat -c%s "$PUNCT_FILE") bytes"
fi
echo "🦞 punctuation model ready in $PUNCT_DIR"
