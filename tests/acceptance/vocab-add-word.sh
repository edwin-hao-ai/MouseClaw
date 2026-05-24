#!/usr/bin/env bash
# Acceptance · 词表加词（去权重）v0.6
# 验证：输一个词就进、自动 CJK 分字、无 score 暴露、i18n 齐全、UI 接好。
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

# 1. Rust 核心逻辑存在
rg -q 'pub fn to_hotword_form' src-tauri/src/vocab.rs && pass "to_hotword_form 存在" || die "缺 to_hotword_form"
rg -q 'pub fn add_user_word' src-tauri/src/vocab.rs && pass "add_user_word 存在" || die "缺 add_user_word"

# 2. 命令注册
rg -q 'pub fn vocab_add_word' src-tauri/src/commands/settings.rs && pass "vocab_add_word 命令存在" || die "缺命令"
rg -q 'commands::vocab_add_word' src-tauri/src/lib.rs && pass "命令已注册 invoke_handler" || die "命令未注册"

# 3. 永不暴露权重：加词写入不带 :score（add_user_word 用 to_hotword_form，不拼 score）
rg -q 'writeln!\(f, "\{stored\}"\)' src-tauri/src/vocab.rs && pass "写入无 :score（仅词面）" || die "写入可能带 score"

# 4. i18n 三处齐全（types/zh/en）—— 少一处 tsc 会挂
for f in src/i18n/types.ts src/i18n/zh.ts src/i18n/en.ts; do
  rg -q '"set.vocab.add"' "$f" && pass "i18n key 在 $f" || die "i18n 缺 set.vocab.add @ $f"
done

# 5. UI 接好（testid + 命令调用）
rg -q 'data-testid="vocab-add-input"' src/SettingsView.tsx && pass "加词输入框 testid" || die "缺输入框"
rg -q 'invoke<.*>\("vocab_add_word"' src/SettingsView.tsx && pass "UI 调 vocab_add_word" || die "UI 未调命令"
rg -q 'vocab_add_word' src/lib/dev-tauri-mock.ts && pass "dev-mock 桩存在" || die "缺 dev-mock 桩"

# 6. 单测绿（CJK 分字 + 去重）
if cargo test --manifest-path src-tauri/Cargo.toml --lib 'vocab::tests' -- --test-threads=1 >/tmp/vocab_test.log 2>&1; then
  pass "vocab 单测通过"
else
  die "vocab 单测失败（见 /tmp/vocab_test.log）"
fi

echo "----"
[ "$fail" -eq 0 ] && echo "✅ vocab-add-word 全部通过" || echo "❌ 有失败项"
exit $fail
