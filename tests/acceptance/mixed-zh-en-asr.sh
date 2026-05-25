#!/usr/bin/env bash
# Acceptance · 中英混合识别修复（"push"→"铺石"）v0.6
# 根因：英文 hotword 没按模型 BPE units 切分 → sherpa skip → 英文 biasing 是 no-op。
# 修复：内嵌 bpe.vocab + modeling_unit=cjkchar+bpe + active.txt 英文大写 + 词表加常用动词。
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

TS=src-tauri/src/transcribe_stream.rs
VOCAB=src-tauri/src/vocab.rs
PROG=src-tauri/resources/vocab/programmer.txt

# 1. bpe.vocab 资源存在 + 内嵌
[ -f src-tauri/resources/sherpa/bpe.vocab ] && pass "bpe.vocab 资源存在" || die "缺 bpe.vocab 资源"
rg -q 'include_str!\("\.\./resources/sherpa/bpe\.vocab"\)' "$TS" && pass "bpe.vocab 已内嵌二进制" || die "bpe.vocab 未内嵌"

# 2. recognizer 设了 modeling_unit + bpe_vocab
rg -q 'modeling_unit = Some\("cjkchar\+bpe"' "$TS" && pass "modeling_unit=cjkchar+bpe" || die "缺 modeling_unit"
rg -q 'config\.model_config\.bpe_vocab = Some' "$TS" && pass "bpe_vocab 注入" || die "缺 bpe_vocab 注入"
# 缺 bpe.vocab 时降级不阻断（写失败只 eprintln）
rg -q '英文 biasing 降级' "$TS" && pass "缺 bpe 时降级不阻断识别" || die "未做降级保护"

# 3. active.txt 英文大写化（对上模型大写 BPE units）
rg -q 'fn to_active_word' "$VOCAB" && pass "to_active_word 存在" || die "缺 to_active_word"
rg -q 'let word = to_active_word\(&word\)' "$VOCAB" && pass "regenerate_active 走大写化" || die "active 未走大写化"

# 4. 词表加了常用 git/工作流英文动词（最易被音译）
for w in push pull commit merge rebase deploy refactor branch; do
  rg -q "^${w} :" "$PROG" && pass "词表含 $w" || die "词表缺 $w"
done

# 5. 单测（含大写化回归 + recase 不受影响）
if cargo test --manifest-path src-tauri/Cargo.toml --lib 'vocab::' >/tmp/asr_vocab_test.log 2>&1; then
  pass "vocab 单测通过"
else
  die "vocab 单测失败（见 /tmp/asr_vocab_test.log）"
fi

# 6. A/B/C 真音频实测留作 #[ignore] 手动跑（TTS 无法忠实合成中英 code-switch）
rg -q '#\[ignore\]' "$TS" && rg -q 'fn ab_mixed_zh_en' "$TS" && pass "A/B/C 真音频回归测试在位(ignore)" || die "缺真音频回归测试"

echo "----"
[ "$fail" -eq 0 ] && echo "✅ mixed-zh-en-asr 全部通过" || echo "❌ 有失败项"
exit $fail
