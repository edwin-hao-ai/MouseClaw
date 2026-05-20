#!/usr/bin/env bash
# Acceptance · v0.4.x · 一键装 agent-browser / OfficeCLI + Onboarding 第 7/8 步教程
# 设计文档：docs/prototypes/auto-install-cli-20260520.html · State 1-8
#
# 跑法：bash tests/acceptance/auto-install-cli-e2e.sh
# 期望：所有 PASS，0 FAIL，0 SKIP（除非 toolchain 没装）
#
# 这是结构性 grep 检查 —— 防止重构 / 重命名 / 误删带来的 latent bug。
# 真正的浏览器 E2E 走 Chrome MCP（见 docs/manuals/auto-install/verify.md 之后补）。

set -uo pipefail
cd "$(dirname "$0")/../.."

PASS=0; FAIL=0
ok()   { echo "  ✅ $1"; PASS=$((PASS+1)); }
fail() { echo "  ❌ $1"; FAIL=$((FAIL+1)); }
grep_f() {
  local pat="$1" file="$2" desc="$3"
  if grep -q -- "$pat" "$file" 2>/dev/null; then ok "$desc"; else fail "$desc (pat=$pat in $file)"; fi
}

echo "── Block 1 · CLI 探测 + system prompt ─────────────────────────────────"
grep_f "OFFICE_CAPABILITY_PROMPT" src-tauri/src/claude_cli.rs "OFFICE_CAPABILITY_PROMPT 定义"
grep_f "NO_OFFICE_CAPABILITY_PROMPT" src-tauri/src/claude_cli.rs "NO_OFFICE 分支也存在（避免假装能调）"
grep_f "officecli" src-tauri/src/claude_cli.rs "system_prompt 接进 officecli 检测"
grep_f "OFFICE_CAPABILITY_PROMPT" src-tauri/src/claude_cli.rs "system_prompt 引用 OFFICE_CAPABILITY_PROMPT"
grep_f "pub officecli" src-tauri/src/commands.rs "CapabilityStatus.officecli 字段公开"
grep_f 'find_binary("officecli")' src-tauri/src/commands.rs "capability_status 探测 officecli"

echo
echo "── Block 2 · install backend + 状态页 ────────────────────────────────"
grep_f "cli_install" src-tauri/src/lib.rs "cli_install 模块挂载"
grep_f "InstallTarget::AgentBrowser" src-tauri/src/cli_install.rs "AgentBrowser target"
grep_f "InstallTarget::OfficeCli" src-tauri/src/cli_install.rs "OfficeCli target"
grep_f "install_cli" src-tauri/src/lib.rs "install_cli 命令已注册"
grep_f "install-progress" src-tauri/src/cli_install.rs "EV_INSTALL_PROGRESS 事件名"
grep_f "no-npm" src-tauri/src/cli_install.rs "no-npm 错误码（引导 nodejs.org）"
grep_f "network" src-tauri/src/cli_install.rs "network 错误分类"
grep_f "permission" src-tauri/src/cli_install.rs "permission 错误分类"
grep_f "install-progress" src/StatusView.tsx "StatusView 订阅 install-progress"
grep_f "install_cli" src/StatusView.tsx "StatusView 调 install_cli"
grep_f "status.row.officecli.title" src/StatusView.tsx "OfficeCLI 行在 StatusView"
grep_f 'install-progress' src/App.tsx "App.tsx 监听装好事件 → 弹气泡"
grep_f "我学会" src/App.tsx "中文庆祝文案"
grep_f "I just learned" src/App.tsx "英文庆祝文案"

echo
echo "── Block 3 · Onboarding install step ────────────────────────────────────"
grep_f "OnboardingInstall" src/components/Onboarding.tsx "Step 7 拼进 Onboarding"
grep_f "step === 7" src/components/Onboarding.tsx "Step 7 分支"
grep_f 'setStep(7)' src/components/Onboarding.tsx "Step 6 跳 Step 7"
grep_f "install_cli" src/components/OnboardingInstall.tsx "OnboardingInstall 调 install_cli"
grep_f "npm i -g agent-browser" src/components/OnboardingInstall.tsx "命令全文可见（正确包名 agent-browser）"
grep_f "enable_browser_automation" src/components/OnboardingInstall.tsx "CDP「用我的 Chrome」选项接 enable_browser_automation"
grep_f "raw.githubusercontent.com/iOfficeAI/OfficeCLI" src/components/OnboardingInstall.tsx "OfficeCLI 命令全文可见"

echo
echo "── Block 4 · 教程 demo 步已移除（2026-05-21）· Step 7 是最后一步 ─────────"
# v0.4.x：移除了 Step 8「试这条」三卡 demo（前置条件常不满足、易误导）。
grep_absent() {
  if grep -q -- "$1" "$2" 2>/dev/null; then echo "  ❌ 不该再出现: $3 (pat=$1 in $2)"; FAIL=$((FAIL+1));
  else echo "  ✅ 已移除: $3"; PASS=$((PASS+1)); fi
}
grep_absent "OnboardingTutorial" src/components/Onboarding.tsx "Onboarding 不再引用 OnboardingTutorial"
grep_absent "step === 8" src/components/Onboarding.tsx "无 Step 8 分支"
[ ! -f src/components/OnboardingTutorial.tsx ] \
  && { echo "  ✅ OnboardingTutorial.tsx 已删除"; PASS=$((PASS+1)); } \
  || { echo "  ❌ OnboardingTutorial.tsx 仍存在"; FAIL=$((FAIL+1)); }
grep_f "onComplete(selected" src/components/Onboarding.tsx "Step 7 完成直接 onComplete（最后一步）"

echo
echo "── 老用户升级发现性（upgrade hint nudge）────────────────────────────"
grep_f "maybe_hint_upgrade" src-tauri/src/cli_install.rs "maybe_hint_upgrade 定义"
grep_f "maybe_hint_upgrade" src-tauri/src/lib.rs "启动时调 maybe_hint_upgrade"
grep_f "upgrade_cli_hint_shown" src-tauri/src/cli_install.rs "一次性 marker file"
grep_f "LearnedCli" src-tauri/src/events.rs "NudgeKind::LearnedCli 变体"
grep_f "learned-cli" src-tauri/src/events.rs "as_str 映射"
grep_f "learned-cli" src/types.ts "前端 NudgeKind union 含 learned-cli"
grep_f "open-status" src-tauri/src/cli_install.rs "CTA action open-status"
grep_f "open-status" src/components/NudgeBubble.tsx "NudgeBubble 处理 open-status"
grep_f "show_status_window" src-tauri/src/lib.rs "show_status_window 命令已注册"

echo
echo "── i18n 完整性（zh + en + types）─────────────────────────────────────"
for k in \
  "status.row.officecli.title" "status.install.do_it" "status.install.retry" \
  "celebrate.title" "onbinst.title" "onbinst.no_node" "onbinst.cdp_title"
do
  grep_f "$k" src/i18n/types.ts "$k 在 types.ts"
  grep_f "$k" src/i18n/zh.ts    "$k 在 zh.ts"
  grep_f "$k" src/i18n/en.ts    "$k 在 en.ts"
done

echo
echo "── TypeScript + Rust 编译 ────────────────────────────────────────────"
if bunx tsc --noEmit >/tmp/_tsc.log 2>&1; then ok "tsc --noEmit 通过"
else fail "tsc 失败 (see /tmp/_tsc.log)"; fi

if cargo check --manifest-path src-tauri/Cargo.toml --quiet >/tmp/_cargo.log 2>&1; then
  ok "cargo check 通过"
else fail "cargo check 失败 (see /tmp/_cargo.log)"; fi

echo
echo "═══════════════════════════════════════════════════════════════"
echo "  PASS: $PASS · FAIL: $FAIL"
echo "═══════════════════════════════════════════════════════════════"
exit $FAIL
