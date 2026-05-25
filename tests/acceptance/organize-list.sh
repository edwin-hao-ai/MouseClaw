#!/usr/bin/env bash
# Acceptance · 规则版"整理成清单" + 确认 580MB LLM 已彻底移除 v0.6
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

# 1. 规则 organize 存在 + 中英标记
rg -q 'pub fn organize_into_list' src-tauri/src/tidy_up.rs && pass "organize_into_list 存在" || die "缺 organize"
rg -q 'LIST_MARKERS_ZH' src-tauri/src/tidy_up.rs && rg -q 'LIST_MARKERS_EN' src-tauri/src/tidy_up.rs && pass "中英标记表" || die "缺标记表"

# 2. 听写整理走规则（不再调本地模型）
rg -q 'tidy_up::organize_into_list' src-tauri/src/voice_ime.rs && pass "听写整理走规则" || die "听写整理未走规则"
rg -q 'local_model::generate' src-tauri/src/voice_ime.rs && die "voice_ime 仍引用 local_model" || pass "voice_ime 无 local_model 残留"

# 3. 清理走规则；翻译/解释/写回信走 CLI
rg -q 'action == "clean"' src-tauri/src/clipboard_action.rs && rg -q 'tidy_up::light_clean' src-tauri/src/clipboard_action.rs && pass "清理走规则" || die "清理未走规则"
rg -q 'local_model' src-tauri/src/clipboard_action.rs && die "clipboard 仍引用 local_model" || pass "clipboard 无 local_model 残留"

# 4. 580MB LLM 彻底移除
[ ! -f src-tauri/src/local_model.rs ] && pass "local_model.rs 已删" || die "local_model.rs 仍在"
rg -q '^ort = ' src-tauri/Cargo.toml && die "ort 依赖仍在" || pass "ort 依赖已移除"
rg -q '^tokenizers = ' src-tauri/Cargo.toml && die "tokenizers 仍在" || pass "tokenizers 已移除"
rg -q 'qwen_06b_spec' src-tauri/src/model_downloader.rs && die "qwen spec 仍在" || pass "qwen spec 已移除"
rg -q 'pub mod local_model' src-tauri/src/lib.rs && die "local_model 模块仍注册" || pass "local_model 模块已移除"

# 5. 听写整理开关仍在（纯规则）+ i18n
rg -q '"set.tidy"' src/i18n/zh.ts && rg -q 'save_dictation_tidy' src/SettingsView.tsx && pass "整理开关 UI+i18n" || die "缺整理开关"
rg -q 'set.localmodel' src/i18n/zh.ts && die "localmodel i18n 残留" || pass "localmodel i18n 已清"

# 6. 单测
if cargo test --manifest-path src-tauri/Cargo.toml --lib 'tidy_up::tests::organize' >/tmp/org_test.log 2>&1; then
  pass "organize 单测通过"
else
  die "organize 单测失败（见 /tmp/org_test.log）"
fi

echo "----"
[ "$fail" -eq 0 ] && echo "✅ organize-list 全部通过" || echo "❌ 有失败项"
exit $fail
