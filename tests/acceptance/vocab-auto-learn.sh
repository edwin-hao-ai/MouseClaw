#!/usr/bin/env bash
# Acceptance · 更深词表 / 自动学词 v0.6
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

# 1. 核心
rg -q 'pub fn learn_word' src-tauri/src/vocab.rs && pass "learn_word 存在" || die "缺 learn_word"
rg -q 'pub fn is_learnable_term' src-tauri/src/vocab.rs && pass "is_learnable_term 存在" || die "缺过滤器"
rg -q 'pub fn auto_file_path' src-tauri/src/vocab.rs && pass "auto.txt 路径存在" || die "缺 auto.txt"

# 2. regenerate 合并 auto.txt
rg -q 'auto_file_path' src-tauri/src/vocab.rs && rg -q '自动学的词（auto.txt）也并进来' src-tauri/src/vocab.rs \
  && pass "regenerate_active 合并 auto.txt" || die "auto.txt 未并入 active"

# 3. 纠错路径挂钩（学 new）
rg -q 'vocab::learn_word\(&new\)' src-tauri/src/voice_ime.rs && pass "纠错成功后自动学 new" || die "未挂自动学词"

# 4. 单测
if cargo test --manifest-path src-tauri/Cargo.toml --lib 'vocab::tests' -- --test-threads=1 >/tmp/autolearn_test.log 2>&1; then
  pass "vocab 单测通过（含 learnable 过滤）"
else
  die "vocab 单测失败（见 /tmp/autolearn_test.log）"
fi

echo "----"
[ "$fail" -eq 0 ] && echo "✅ vocab-auto-learn 全部通过" || echo "❌ 有失败项"
exit $fail
