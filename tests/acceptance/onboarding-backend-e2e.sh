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
check "Onboarding onComplete 带 backend+skin 参数"   "grep -q 'onComplete: (choice: ShortcutChoice, backend: BackendChoice, skin: SkinId)' src/components/Onboarding.tsx"
check "Onboarding 有 BACKENDS 选项数组"              "grep -q 'const BACKENDS' src/components/Onboarding.tsx"
check "onComplete 调用传 selected+backend+skin"     "grep -q 'onComplete(selected, backend, skin)' src/components/Onboarding.tsx"
check "OnboardingView 把 backend+skin 传给 save_shortcut" "grep -q 'save_shortcut.*{ choice, backend, skin }' src/OnboardingView.tsx"

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

echo "── 桌宠皮肤系统 (v0.1.7) ──"
check "src/skins.ts 定义 6 款 skin"             "grep -c '^  {' src/skins.ts | grep -q '^6$'"
check "Rust skins.rs 有 6 个 SkinId 枚举值"      "grep -E '^    (Classic|Lab|Field|Ninja|Cyber|Golden),' src-tauri/src/skins.rs | wc -l | tr -d ' ' | grep -q '^6$'"
check "config.rs 有 skin 字段"                   "grep -q 'pub skin: SkinId' src-tauri/src/config.rs"
check "CURRENT_CONFIG_VERSION = 7"               "grep -q 'CURRENT_CONFIG_VERSION: u32 = 7' src-tauri/src/config.rs"
check "save_shortcut 接收 skin 参数"             "grep -q 'skin: Option<String>' src-tauri/src/commands.rs"
check "save_skin / get_skin 命令存在"             "grep -q 'pub fn save_skin' src-tauri/src/commands.rs && grep -q 'pub fn get_skin' src-tauri/src/commands.rs"
check "tray 有皮肤子菜单"                          "grep -q 'skin-submenu' src-tauri/src/tray.rs"
check "events.rs 有 EV_SKIN_CHANGED"             "grep -q 'EV_SKIN_CHANGED' src-tauri/src/events.rs"
check "Onboarding 是 4 步 (1|2|3|4)"            "grep -q 'useState<1 | 2 | 3 | 4>' src/components/Onboarding.tsx"
check "Onboarding 引入 SKINS"                    "grep -q 'import { SKINS' src/components/Onboarding.tsx"
check "onComplete 带 skin 参数"                   "grep -q 'onComplete: (choice: ShortcutChoice, backend: BackendChoice, skin: SkinId)' src/components/Onboarding.tsx"
check "OnboardingView 把 skin 传给 save_shortcut" "grep -q 'save_shortcut.*{ choice, backend, skin }' src/OnboardingView.tsx"
check "App.tsx 订阅 EV_SKIN_CHANGED"             "grep -q 'EV_SKIN_CHANGED' src/App.tsx"
check "App.tsx 传 skin 给 PixelMouse"            "grep -q 'skin={skin}' src/App.tsx"
check "PixelMouse 接 skin prop"                  "grep -q 'skin?: SkinId' src/components/PixelMouse.tsx"
check "DESIGN.md 加 Skin Palettes 表"             "grep -q 'Skin Palettes' DESIGN.md"

echo "── P0a · 浏览器桥接（CDP + chrome-devtools-mcp） ──"
check "browser_bridge.rs 存在"                    "test -f src-tauri/src/browser_bridge.rs"
check "lib.rs 注册 browser_bridge 模块"            "grep -q 'pub mod browser_bridge' src-tauri/src/lib.rs"
check "CDP_PORT = 9222 (生态对齐)"                "grep -q 'pub const CDP_PORT: u16 = 9222' src-tauri/src/browser_bridge.rs"
check "debug profile 隔离在 .mouseclaw/"          "grep -q '.mouseclaw/chrome-debug-profile' src-tauri/src/browser_bridge.rs"
check "chrome-devtools-mcp 注册命令"               "grep -q 'chrome-devtools-mcp@latest' src-tauri/src/browser_bridge.rs"
check "enable_browser_automation 命令注册"         "grep -q 'enable_browser_automation' src-tauri/src/lib.rs"
check "capability_status 命令存在"                 "grep -q 'pub fn capability_status' src-tauri/src/commands.rs"
check "tray 有「启用浏览器自动化」菜单"             "grep -q 'enable-browser' src-tauri/src/tray.rs"
check "CHROME_CDP_CAPABILITY_PROMPT 存在"          "grep -q 'CHROME_CDP_CAPABILITY_PROMPT' src-tauri/src/claude_cli.rs"
check "system_prompt 走 3 档分支"                  "grep -q 'cdp_alive' src-tauri/src/claude_cli.rs"

echo "── P0b · Mode B 剪贴板 fallback（富文本编辑器） ──"
check "RICH_EDITOR_BUNDLES 列表存在"               "grep -q 'RICH_EDITOR_BUNDLES' src-tauri/src/mode_b.rs"
check "Slack / Notion / VSCode 在 fallback 名单"  "grep -q 'tinyspeck.slackmacgap' src-tauri/src/mode_b.rs && grep -q 'notion.id' src-tauri/src/mode_b.rs && grep -q 'microsoft.VSCode' src-tauri/src/mode_b.rs"
check "paste_via_clipboard 实现存在"               "grep -q 'fn paste_via_clipboard' src-tauri/src/mode_b.rs"
check "Cmd+V 合成路径走 CGEventFlagCommand"        "grep -q 'CGEventFlagCommand' src-tauri/src/mode_b.rs"
check "should_use_clipboard_paste 路由函数"        "grep -q 'fn should_use_clipboard_paste' src-tauri/src/mode_b.rs"

echo "── 构建 / 测试闸门 ──"
check "前端 bun build 通过"                         "bun run build"
check "Rust cargo check 通过"                       "cargo check --manifest-path src-tauri/Cargo.toml"
check "Rust 单元测试通过"                           "cargo test --manifest-path src-tauri/Cargo.toml --lib"

echo
echo "════════ $PASS passed, $FAIL failed ════════"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
