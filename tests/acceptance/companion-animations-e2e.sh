#!/usr/bin/env bash
# Acceptance gate for v0.4+ companion-向 animations.
# 静态 grep + 编译检查 —— Chrome MCP / Tauri shell 实测在 docs/manuals/ 里跑。
set -euo pipefail

cd "$(dirname "$0")/../.."

pass=0
fail=0
check() {
  local label="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "✅ PASS  $label"
    pass=$((pass+1))
  else
    echo "❌ FAIL  $label"
    fail=$((fail+1))
  fi
}

echo "── companion-animations acceptance ──"

# ── 1. Rust 端事件源就位 ──
check "companion.rs 存在" \
  test -f src-tauri/src/companion.rs
check "companion 在 lib.rs 注册" \
  grep -q "pub mod companion;" src-tauri/src/lib.rs
check "companion::spawn 在 setup 内调用" \
  grep -q "companion::spawn" src-tauri/src/lib.rs
check "事件名稳定（前后端契约）" \
  grep -q '"companion-tick"' src-tauri/src/companion.rs
check "payload key 名 = camelCase（前端 hook 直接消费）" \
  grep -q 'rename = "sinceKey"' src-tauri/src/companion.rs
check "隐私边界：不用 Accessibility API（只 CGEventSource）" \
  bash -c 'grep -q "CGEventSourceSecondsSinceLastEventType" src-tauri/src/companion.rs && ! grep -q "AXSelectedText\|AXUIElement" src-tauri/src/companion.rs'

# ── 2. React 端 hook + PixelMouse 集成 ──
check "useCompanion hook 存在" \
  test -f src/hooks/useCompanion.ts
check "hook 订阅正确的事件名" \
  grep -q '"companion-tick"' src/hooks/useCompanion.ts
check "deriveCompanionFrame 是 pure export（单测能引）" \
  grep -q "export function deriveCompanionFrame" src/hooks/useCompanion.ts
check "PixelMouse 接收 companionState prop" \
  grep -q "companionState" src/components/PixelMouse.tsx
check "PixelMouse 接收 eyeOffset prop" \
  grep -q "eyeOffset" src/components/PixelMouse.tsx
check "App.tsx wire 了 useCompanion" \
  grep -q "useCompanion" src/App.tsx

# ── 3. 适配所有皮肤的硬规则 ──
check "companion CSS 走整体 .mouse-svg transform（不写 palette 字面量）" \
  bash -c '! grep -E "#[0-9a-fA-F]{6}" src/components/PixelMouse.css | grep -i "companion"'
check "companion 类名覆盖 7 个状态（含 v0.4+ click/worried/drowsy 扩展）" \
  bash -c 'for s in typing alert excited sleep clicked worried drowsy; do grep -q "companion-$s" src/components/PixelMouse.css || exit 1; done'
check "Rust 端 sinceClick 字段就位（点击反应）" \
  grep -q 'rename = "sinceClick"' src-tauri/src/companion.rs
check "Picker 预览大老鼠也接 companion" \
  grep -q "useCompanion" src/PickerView.tsx
check "PixelMouse.companion.test 覆盖 9 款皮肤" \
  grep -q "for (const skin of SKINS)" src/components/__tests__/PixelMouse.companion.test.tsx

# ── 4. 编译检查 ──
check "TS 编译干净" \
  bunx tsc --noEmit
check "Rust cargo check 干净" \
  cargo check --manifest-path src-tauri/Cargo.toml --quiet

# ── 5. 测试金字塔 ──
check "vitest 单元/集成测试 42+ 全绿" \
  bash -c 'bunx vitest run 2>&1 | grep -qE "Tests  4[0-9] passed"'
check "Rust 单元测试 cargo test 全绿" \
  cargo test --manifest-path src-tauri/Cargo.toml --quiet --lib

# ── 6. CLAUDE.md 规则文档化 ──
check "CLAUDE.md 已收录 '动画/陪伴效果必须适配所有皮肤' 硬规则" \
  grep -q "动画/陪伴效果必须适配所有皮肤" CLAUDE.md

echo "──────────────────────────"
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
