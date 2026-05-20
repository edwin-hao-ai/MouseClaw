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
check "companion 类名覆盖全部 11 个状态（含 v0.4+ 全集补完）" \
  bash -c 'for s in typing alert excited sleep clicked worried drowsy waking dizzy hop glance; do grep -q "companion-$s" src/components/PixelMouse.css || exit 1; done'
check "Rust 端 sinceClick 字段就位（点击反应）" \
  grep -q 'rename = "sinceClick"' src-tauri/src/companion.rs
check "Picker 预览大老鼠也接 companion" \
  grep -q "useCompanion" src/PickerView.tsx
check "点桌宠出爱心：pet-heart CSS + popHeart wire" \
  bash -c 'grep -q "pet-heart" src/App.css && grep -q "popHeart" src/App.tsx'
check "亲密度系统：useIntimacy hook + 持久化 key" \
  bash -c 'test -f src/hooks/useIntimacy.ts && grep -q "mouseclaw.intimacy" src/hooks/useIntimacy.ts'
check "PixelMouse 接 intimacyLevel + neglected" \
  bash -c 'grep -q "intimacyLevel" src/components/PixelMouse.tsx && grep -q "neglected" src/components/PixelMouse.tsx'
check "LongFocus nudge：Rust enum + 规则 + 前端 type" \
  bash -c 'grep -q "LongFocus" src-tauri/src/nudge.rs && grep -q "long-focus" src/types.ts'
check "眼球偏移用 SVG transform attribute（WebKit 可靠，非 CSS px）" \
  grep -q 'transform={eyesTransformAttr}' src/components/PixelMouse.tsx
check "无 CSS .intimacy-N .mc-eyes transform（会覆盖追鼠标 attribute · 2026-05-20 回归）" \
  bash -c '! grep -E "intimacy-[123] .mc-eyes" src/components/PixelMouse.css | grep -q transform'
check "PixelMouse.companion.test 覆盖 9 款皮肤" \
  grep -q "for (const skin of SKINS)" src/components/__tests__/PixelMouse.companion.test.tsx

# ── 4. 编译检查 ──
check "TS 编译干净" \
  bunx tsc --noEmit
check "Rust cargo check 干净" \
  cargo check --manifest-path src-tauri/Cargo.toml --quiet

# ── 5. 测试金字塔 ──
check "vitest 单元/集成测试 全绿（69+）" \
  bash -c 'bunx vitest run 2>&1 | grep -qE "Tests  ([6-9][0-9]|[1-9][0-9]{2}) passed"'
check "Rust 单元测试 cargo test 全绿" \
  cargo test --manifest-path src-tauri/Cargo.toml --quiet --lib

# ── 6. CLAUDE.md 规则文档化 ──
check "CLAUDE.md 已收录 '动画/陪伴效果必须适配所有皮肤' 硬规则" \
  grep -q "动画/陪伴效果必须适配所有皮肤" CLAUDE.md
check "DESIGN.md §3.1 已记录 companion 状态（硬规则：新动画状态必须进 DESIGN.md）" \
  bash -c 'grep -q "Companion Layer States" DESIGN.md && grep -q "companion-dizzy" DESIGN.md && grep -q "companion-waking" DESIGN.md'

echo "──────────────────────────"
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
