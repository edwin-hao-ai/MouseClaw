#!/usr/bin/env bash
# Acceptance · 本地兜底模型 (Qwen3-0.6B ONNX) v0.6
# 静态校验接线齐全；端到端推理由 local_model::tests（需模型在场）覆盖。
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
pass(){ echo "PASS $1"; }
die(){ echo "FAIL $1"; fail=1; }

# 1. 模块 + 推理逻辑
rg -q 'pub async fn generate' src-tauri/src/local_model.rs && pass "local_model::generate 存在" || die "缺 generate"
rg -q 'pub fn is_ready' src-tauri/src/local_model.rs && pass "is_ready 存在" || die "缺 is_ready"
rg -q 'pub mod local_model;' src-tauri/src/lib.rs && pass "模块已注册" || die "模块未注册"

# 2. 依赖（纯 Rust ONNX，无 Python/Ollama）
rg -q '^ort = ' src-tauri/Cargo.toml && pass "ort 依赖在" || die "缺 ort"
rg -q 'tokenizers' src-tauri/Cargo.toml && pass "tokenizers 依赖在" || die "缺 tokenizers"

# 3. 模型下载 spec（复用 model_downloader）
rg -q 'pub fn qwen_06b_spec' src-tauri/src/model_downloader.rs && pass "qwen_06b_spec 存在" || die "缺 spec"
rg -q 'qwen3-0.6b-onnx' src-tauri/src/model_downloader.rs && pass "spec id 正确" || die "spec id 错"

# 4. backend 兜底接线（CLI 没装→本地；agentic 不兜底）
rg -q 'local_model::is_ready\(\)' src-tauri/src/backend.rs && rg -q 'local_model::generate' src-tauri/src/backend.rs \
  && pass "backend.rs CLI 缺失→本地兜底" || die "backend 未接兜底"

# 5. 命令注册
rg -q 'pub async fn download_local_model' src-tauri/src/commands/settings.rs && pass "download 命令存在" || die "缺下载命令"
rg -q 'commands::download_local_model' src-tauri/src/lib.rs && pass "命令已注册" || die "命令未注册"

# 6. 强 system 消息防 echo
rg -q '只输出处理后的结果文本本身' src-tauri/src/local_model.rs && pass "防 echo system 消息在" || die "缺防 echo 提示"

# 7. i18n 三处 + UI + dev-mock
for f in src/i18n/types.ts src/i18n/zh.ts src/i18n/en.ts; do
  rg -q '"set.localmodel"' "$f" && pass "i18n key 在 $f" || die "i18n 缺 @ $f"
done
rg -q 'data-testid="localmodel-download"' src/SettingsView.tsx && pass "下载按钮 UI" || die "缺下载按钮"
rg -q 'model-progress' src/SettingsView.tsx && pass "监听下载进度" || die "未监听进度"
rg -q 'download_local_model' src/lib/dev-tauri-mock.ts && pass "dev-mock 桩" || die "缺 dev-mock 桩"

# 8. 下载进度显示在统一下载窗（snapshot_all 含 qwen + 点下载开下载窗）
rg -q 'qwen_06b_spec\(\)\]' src-tauri/src/model_downloader.rs && pass "qwen 纳入 snapshot_all（下载窗可见）" || die "snapshot_all 未含 qwen"
rg -q 'open_downloader_window' src-tauri/src/commands/settings.rs && pass "下载时开下载进度窗" || die "未开下载窗"

# 9. onboarding 没装 CLI 时提供本地模型入口
rg -q 'download_local_model' src/components/OnboardingBackendStep.tsx && pass "onboarding 本地模型入口" || die "onboarding 缺本地入口"

echo "----"
[ "$fail" -eq 0 ] && echo "✅ local-model 全部通过" || echo "❌ 有失败项"
exit $fail
