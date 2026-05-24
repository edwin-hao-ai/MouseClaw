#!/usr/bin/env bash
# Acceptance · 听写统计（时长/字数/WPM/省时）v0.6
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

# 1. stats 模块 + 命令
rg -q 'pub fn count_words' src-tauri/src/stats.rs && pass "count_words 存在" || die "缺 count_words"
rg -q 'pub fn record_finish' src-tauri/src/stats.rs && pass "record_finish 存在" || die "缺 record_finish"
rg -q 'pub fn get_dictation_stats' src-tauri/src/commands/settings.rs && pass "命令存在" || die "缺命令"
rg -q 'commands::get_dictation_stats' src-tauri/src/lib.rs && pass "命令已注册" || die "命令未注册"
rg -q 'pub mod stats;' src-tauri/src/lib.rs && pass "stats 模块已注册" || die "stats 模块未注册"

# 2. 听写路径挂钩（起点 + 结束）
rg -q 'stats::mark_start' src-tauri/src/voice_ime.rs && pass "mark_start 挂在录音开始" || die "mark_start 未挂"
rg -q 'stats::record_finish' src-tauri/src/voice_ime.rs && pass "record_finish 挂在写入成功" || die "record_finish 未挂"

# 3. i18n + UI
for f in src/i18n/types.ts src/i18n/zh.ts src/i18n/en.ts; do
  rg -q '"hist.dict.summary"' "$f" && pass "i18n key 在 $f" || die "i18n 缺 @ $f"
done
rg -q 'data-testid="dictation-stats"' src/HistoryView.tsx && pass "banner testid" || die "缺 banner"
rg -q 'get_dictation_stats' src/lib/dev-tauri-mock.ts && pass "dev-mock 桩" || die "缺 dev-mock 桩"

# 4. 单测
if cargo test --manifest-path src-tauri/Cargo.toml --lib 'stats::tests' >/tmp/stats_test.log 2>&1; then
  pass "stats 单测通过"
else
  die "stats 单测失败（见 /tmp/stats_test.log）"
fi

echo "----"
[ "$fail" -eq 0 ] && echo "✅ dictation-stats 全部通过" || echo "❌ 有失败项"
exit $fail
