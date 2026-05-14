#!/usr/bin/env bash
# Acceptance: multi-backend Onboarding wiring + compute-use integration.
# Structural grep guard — catches a future PR breaking the frontend↔Rust contract.
# Run: bash tests/acceptance/onboarding-backend-e2e.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

PASS=0; FAIL=0
ok()   { echo "  ✅ $1"; PASS=$((PASS+1)); }
no()   { echo "  ❌ $1"; FAIL=$((FAIL+1)); }
check(){ if eval "$2" >/dev/null 2>&1; then ok "$1"; else no "$1"; fi; }

echo "── Onboarding 多后端接线 ──"
check "types.ts 定义 BackendChoice"                "grep -q 'BackendChoice' src/types.ts"
check "Onboarding onComplete 带 backend 参数"        "grep -q 'onComplete: (choice: ShortcutChoice, backend: BackendChoice)' src/components/Onboarding.tsx"
check "Onboarding 有 BACKENDS 选项数组"              "grep -q 'const BACKENDS' src/components/Onboarding.tsx"
check "Onboarding 是 3 步 (step: 1|2|3)"            "grep -q 'useState<1 | 2 | 3>' src/components/Onboarding.tsx"
check "onComplete 调用传 selected+backend"          "grep -q 'onComplete(selected, backend)' src/components/Onboarding.tsx"
check "OnboardingView 把 backend 传给 save_shortcut" "grep -q 'save_shortcut.*{ choice, backend }' src/OnboardingView.tsx"

echo "── Rust 后端契约 ──"
check "save_shortcut 接收 backend 参数"             "grep -q 'backend: String' src-tauri/src/commands.rs"
check "Backend::from_choice 存在"                  "grep -q 'pub fn from_choice' src-tauri/src/backend.rs"
check "config.rs 有 backend 字段"                  "grep -q 'pub backend: Backend' src-tauri/src/config.rs"
check "三后端枚举齐全"                              "grep -q 'ClaudeCli' src-tauri/src/backend.rs && grep -q 'CodexCli' src-tauri/src/backend.rs && grep -q 'OpenclawCli' src-tauri/src/backend.rs"

echo "── compute use (agent-browser) ──"
check "BROWSER_CAPABILITY_PROMPT 存在"             "grep -q 'BROWSER_CAPABILITY_PROMPT' src-tauri/src/claude_cli.rs"
check "system_prompt() 检测 agent-browser"         "grep -q 'find_binary(\"agent-browser\")' src-tauri/src/claude_cli.rs"
check "claude_cli 用 system_prompt() 而非常量"      "grep -q 'sys_prompt' src-tauri/src/claude_cli.rs"
check "backend.rs 各后端用 system_prompt()"        "grep -q 'crate::claude_cli::system_prompt()' src-tauri/src/backend.rs"

echo "── 构建 / 测试闸门 ──"
check "前端 bun build 通过"                         "bun run build"
check "Rust cargo check 通过"                       "cargo check --manifest-path src-tauri/Cargo.toml"
check "Rust 单元测试通过"                           "cargo test --manifest-path src-tauri/Cargo.toml --lib"

echo
echo "════════ $PASS passed, $FAIL failed ════════"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
