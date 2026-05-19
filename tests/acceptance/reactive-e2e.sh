#!/usr/bin/env bash
# Acceptance: v0.4 reactive ribbon (clipboard + selection) 结构 grep guard。
# 不跑真实程序 —— 是 PR 结构 lint，跟 onboarding-backend-e2e.sh 一个套路。
# 真实交互测试见：
#   - tests/e2e/reactive-chrome-mcp.md（Chrome MCP 手动驱动剧本）
#   - vitest 套件 (src/components/ReactiveOverlay.test.tsx)
#   - cargo test (reactive / clipboard_action / selection 模块的 unit tests)
#
# Run: bash tests/acceptance/reactive-e2e.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

PASS=0; FAIL=0
ok()   { echo "  ✅ $1"; PASS=$((PASS+1)); }
no()   { echo "  ❌ $1"; FAIL=$((FAIL+1)); }
check(){ if eval "$2" >/dev/null 2>&1; then ok "$1"; else no "$1"; fi; }

echo "── Backend reactive 模块 ──"
check "reactive.rs 存在"                               "test -f src-tauri/src/reactive.rs"
check "Source enum 含 Clipboard / Selection"           "grep -q 'Clipboard,' src-tauri/src/reactive.rs && grep -q 'Selection,' src-tauri/src/reactive.rs"
check "on_new_text 统一入口"                            "grep -q 'pub fn on_new_text' src-tauri/src/reactive.rs"
check "LAST_TEXT 缓存"                                  "grep -q 'pub static LAST_TEXT' src-tauri/src/reactive.rs"
check "take_last_text 拿走+清空"                        "grep -q 'pub fn take_last_text' src-tauri/src/reactive.rs"
check "classify 暴露给其他模块"                          "grep -q 'pub fn classify' src-tauri/src/reactive.rs"
check "events constant EV_CLIPBOARD_REACTIVE"           "grep -q 'EV_CLIPBOARD_REACTIVE.*=.*\"clipboard-reactive\"' src-tauri/src/reactive.rs"

echo "── Backend clipboard hook ──"
check "clipboard.rs 调 reactive::on_new_text"           "grep -q 'reactive::on_new_text(' src-tauri/src/clipboard.rs"
check "Source::Clipboard 标签正确"                       "grep -q 'reactive::Source::Clipboard' src-tauri/src/clipboard.rs"
check "frontmost_app_pub 导出"                          "grep -q 'pub fn frontmost_app_pub' src-tauri/src/clipboard.rs"

echo "── Backend selection 模块 ──"
check "selection.rs 存在"                               "test -f src-tauri/src/selection.rs"
check "selection 用 AXSelectedText"                     "grep -q 'AXSelectedText' src-tauri/src/selection.rs"
check "selection 用 AXFocusedUIElement"                 "grep -q 'AXFocusedUIElement' src-tauri/src/selection.rs"
check "spawn_capture_loop 公共导出"                      "grep -q 'pub fn spawn_capture_loop' src-tauri/src/selection.rs"
check "MIN_SELECTION_CHARS ≥ 20"                        "grep -E 'MIN_SELECTION_CHARS.*=.*[2-9][0-9]' src-tauri/src/selection.rs"
check "COOLDOWN_SECS 合理范围"                          "grep -E 'COOLDOWN_SECS.*=.*[0-9]+' src-tauri/src/selection.rs"
check "selection 调 reactive::on_new_text"              "grep -q 'reactive::on_new_text(' src-tauri/src/selection.rs"
check "selection 检查 AX 权限"                          "grep -q 'check_accessibility' src-tauri/src/selection.rs"

echo "── Backend action 模块 ──"
check "clipboard_action::process_reactive_action"        "grep -q 'pub async fn process_reactive_action' src-tauri/src/clipboard_action.rs"
check "build_prompt 四个 action 都在"                    "grep -q '\"clean\"' src-tauri/src/clipboard_action.rs && grep -q '\"translate\"' src-tauri/src/clipboard_action.rs && grep -q '\"explain\"' src-tauri/src/clipboard_action.rs && grep -q '\"reply\"' src-tauri/src/clipboard_action.rs"
check "strip_wrappers 兜底剥引号 / 反引号"               "grep -q 'fn strip_wrappers' src-tauri/src/clipboard_action.rs"

echo "── lib.rs 注册 ──"
check "lib.rs pub mod reactive"                         "grep -q '^pub mod reactive' src-tauri/src/lib.rs"
check "lib.rs pub mod selection"                        "grep -q '^pub mod selection' src-tauri/src/lib.rs"
check "lib.rs pub mod clipboard_action"                 "grep -q '^pub mod clipboard_action' src-tauri/src/lib.rs"
check "process_reactive_action 接入 invoke_handler"      "grep -q 'clipboard_action::process_reactive_action' src-tauri/src/lib.rs"
check "selection::spawn_capture_loop 启动"               "grep -q 'selection::spawn_capture_loop' src-tauri/src/lib.rs"
check "reactive::init 启动"                              "grep -q 'reactive::init' src-tauri/src/lib.rs"

echo "── Frontend ReactiveOverlay ──"
check "ReactiveOverlay.tsx 存在"                        "test -f src/components/ReactiveOverlay.tsx"
check "data-testid 全套"                                "grep -q 'data-testid=\"rx-ribbon\"' src/components/ReactiveOverlay.tsx && grep -q 'rx-action-clean' src/components/ReactiveOverlay.tsx && grep -q 'rx-action-translate' src/components/ReactiveOverlay.tsx && grep -q 'rx-action-explain' src/components/ReactiveOverlay.tsx && grep -q 'rx-action-reply' src/components/ReactiveOverlay.tsx"
check "调用 process_reactive_action 不带 clipId"         "grep -q 'process_reactive_action.*action' src/components/ReactiveOverlay.tsx && ! grep -q 'clipId' src/components/ReactiveOverlay.tsx"
check "支持 source 字段"                                "grep -q 'source: ReactiveSource' src/components/ReactiveOverlay.tsx || grep -q 'source:' src/components/ReactiveOverlay.tsx"
check "AUTO_DISMISS_MS = 4000"                          "grep -q 'AUTO_DISMISS_MS = 4000' src/components/ReactiveOverlay.tsx"
check "Esc 关闭逻辑"                                     "grep -q 'Escape' src/components/ReactiveOverlay.tsx"

echo "── Frontend App.tsx wire ──"
check "listen clipboard-reactive"                       "grep -q '\"clipboard-reactive\"' src/App.tsx"
check "ReactiveOverlay 渲染在 idle 视图"                 "grep -q '<ReactiveOverlay' src/App.tsx"
check "hasReactUi 包含 reactive"                        "grep -q '!!reactive' src/App.tsx"
check "PixelMouse twitching prop 接通"                  "grep -q 'twitching={twitching}' src/App.tsx"

echo "── PixelMouse 皮肤动画 ──"
check "mc-ear-l / mc-ear-r 拆分"                        "grep -q 'mc-ear mc-ear-l' src/components/PixelMouse.tsx && grep -q 'mc-ear mc-ear-r' src/components/PixelMouse.tsx"
check "mc-tail / mc-nose 包裹"                          "grep -q 'mc-tail' src/components/PixelMouse.tsx && grep -q 'mc-nose' src/components/PixelMouse.tsx"
check "蛙皮肤 mc-ear-frog 特殊处理"                       "grep -q 'mc-ear-frog' src/components/PixelMouse.tsx"
check "CSS keyframe 全套"                                "grep -q 'mc-ear-flick-l' src/components/PixelMouse.css && grep -q 'mc-ear-flick-r' src/components/PixelMouse.css && grep -q 'mc-tail-flick' src/components/PixelMouse.css && grep -q 'mc-nose-sniff' src/components/PixelMouse.css && grep -q 'mc-ear-frog-bob' src/components/PixelMouse.css"
check "prefers-reduced-motion fallback"                  "grep -q 'prefers-reduced-motion' src/components/PixelMouse.css"

echo "── E2E mock 桩 ──"
check "dev-tauri-mock.ts 存在"                          "test -f src/lib/dev-tauri-mock.ts"
check "shouldInstallDevTauriMock 导出"                  "grep -q 'shouldInstallDevTauriMock' src/lib/dev-tauri-mock.ts"
check "main.tsx 调用 install hook"                      "grep -q 'shouldInstallDevTauriMock' src/main.tsx"
check "__mcEmit helper 暴露"                            "grep -q '__mcEmit' src/lib/dev-tauri-mock.ts"
check "fixtures 覆盖 process_reactive_action"            "grep -q 'process_reactive_action' src/lib/dev-tauri-mock.ts"

echo ""
echo "── 测试套件 ──"
check "Rust unit tests 模块齐全"                        "grep -q '#\\[cfg(test)\\]' src-tauri/src/reactive.rs && grep -q '#\\[cfg(test)\\]' src-tauri/src/clipboard_action.rs && grep -q '#\\[cfg(test)\\]' src-tauri/src/selection.rs"
check "vitest 配置存在"                                  "test -f vitest.config.ts"
check "vitest test 文件存在"                             "test -f src/components/ReactiveOverlay.test.tsx"
check "package.json 有 test script"                     "grep -q '\"test\":' package.json"

echo ""
echo "============================"
echo "✅ $PASS pass · ❌ $FAIL fail"
test $FAIL -eq 0
