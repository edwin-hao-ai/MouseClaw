#!/usr/bin/env bash
# Acceptance · 语音统一用 SenseVoice 离线模型（v0.7）
# 替代 voice-improvements-e2e.sh（旧流式 zipformer + vocab/hotword 时代，已整体删除）。
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

ST=src-tauri/src

# 1. SenseVoice 模块就位 + use_itn（自带标点）
rg -q 'OfflineSenseVoiceModelConfig' "$ST/transcribe_sense.rs" && pass "transcribe_sense 用 SenseVoice offline" || die "缺 SenseVoice config"
rg -q 'use_itn: true' "$ST/transcribe_sense.rs" && pass "use_itn=true（原生标点）" || die "未开 use_itn"
rg -q 'pub fn transcribe\(' "$ST/transcribe_sense.rs" && pass "transcribe() 入口存在" || die "缺 transcribe()"

# 2. 三条语音路径都走 SenseVoice
rg -q 'transcribe_sense::transcribe' "$ST/voice_ime.rs" && pass "听写走 SenseVoice" || die "听写未走 SenseVoice"
rg -q 'transcribe_sense::transcribe' "$ST/pipeline.rs" && pass "AI召唤走 SenseVoice" || die "AI召唤未走 SenseVoice"
rg -q 'transcribe_sense::transcribe' "$ST/feed_flow.rs" && pass "feed 走 SenseVoice" || die "feed 未走 SenseVoice"

# 3. 旧流式 zipformer + 标点模型彻底删除
[ ! -f "$ST/transcribe_stream.rs" ] && pass "transcribe_stream.rs 已删" || die "transcribe_stream.rs 仍在"
[ ! -f "$ST/punctuation.rs" ] && pass "punctuation.rs 已删" || die "punctuation.rs 仍在"
rg -q 'pub mod transcribe_stream' "$ST/lib.rs" && die "transcribe_stream 模块仍注册" || pass "transcribe_stream 模块已移除"
rg -q 'pub mod punctuation' "$ST/lib.rs" && die "punctuation 模块仍注册" || pass "punctuation 模块已移除"
rg -q 'fn zh_en_spec|fn punct_spec' "$ST/model_downloader.rs" && die "旧 spec 仍在" || pass "zh_en/punct spec 已删"

# 4. vocab/hotword 子系统删除（SenseVoice 不支持 hotwords）
[ ! -f "$ST/vocab.rs" ] && pass "vocab.rs 已删" || die "vocab.rs 仍在"
rg -q 'pub mod vocab' "$ST/lib.rs" && die "vocab 模块仍注册" || pass "vocab 模块已移除"
rg -q 'vocab_add_word|vocab_set_builtin' "$ST/lib.rs" && die "vocab 命令仍注册" || pass "vocab 命令已移除"
rg -q 'vocab-submenu' "$ST/tray_menu.rs" && die "tray 仍有术语表子菜单" || pass "tray 术语表子菜单已移除"

# 5. 下载只剩 SenseVoice
rg -q 'sense_voice_spec' "$ST/model_downloader.rs" && pass "sense_voice_spec 存在" || die "缺 sense_voice_spec"
rg -q 'transcribe_stream::kick_off|punctuation::kick_off' "$ST/lib.rs" "$ST/commands.rs" "$ST/commands/windows.rs" && die "仍下载旧模型" || pass "下载只剩 SenseVoice"

# 6. 库编译 + 单测
if cargo build --manifest-path src-tauri/Cargo.toml --lib >/tmp/sv_build.log 2>&1; then
  pass "cargo lib 编译通过"
else
  die "cargo lib 编译失败（见 /tmp/sv_build.log）"
fi

echo "----"
[ "$fail" -eq 0 ] && echo "✅ sense-voice 全部通过" || echo "❌ 有失败项"
exit $fail
