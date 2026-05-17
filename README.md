<div align="center">

# 🦞 MouseClaw

**A pixel-art desktop AI that lives where your cursor is.**

[English](#english) · [中文](#中文) · [Landing page](https://edwin-hao-ai.github.io/MouseClaw/) · [Latest release](https://github.com/edwin-hao-ai/MouseClaw/releases/latest)

<img src="docs/assets/hero.png" alt="MouseClaw hero" width="720" />

</div>

---

## English

### What is it?

MouseClaw is a tiny pixel-art mouse that sleeps in the corner of your macOS screen. **Hold a shortcut, speak, release** — it screenshots, hears you, and asks the best AI CLI on your machine (Claude / Codex / OpenClaw / Hermes) to do the thing. Reply streams back into a speech bubble; if the AI decided you wanted text written *into* your cursor, it types it for you bypassing the IME.

Three modes, one pet.

| Mode | Shortcut | What you get |
|---|---|---|
| **🦞 AI summon** | hold `⌘⇧Space` | screenshot + voice → Claude/Codex/OpenClaw/Hermes → bubble (with markdown) |
| **🎙️ Voice IME** | hold `fn` (configurable: ⌥/⌃/right⇧/right⌘) | Whisper transcription → typed into your cursor — no AI, no latency |
| **📋 Clipboard hub** | `⌘⇧V`, click the pet, or double-click tray | Searchable history of everything you copied. Paste back · Pin · Drag out |

### Demo

<div align="center">

<a href="docs/assets/demo-en.mp4"><img src="docs/assets/demo-en.gif" alt="MouseClaw demo (EN)" width="720" /></a>

*35-second tour · [▶ Watch the EN MP4](docs/assets/demo-en.mp4) · [▶ 看中文版](docs/assets/demo-zh.mp4)*

</div>

### Why daily-use?

- **Zero context switch.** No window to open, no app to focus. The pet is *already there*.
- **Pick your AI brain.** Comes with 4 backends preconfigured. Use whichever CLI you've already authed.
- **Circle the bug.** Hold the shortcut, **hold left mouse + drag** to draw on screen — the AI gets your screenshot with your highlights baked in. Try it on a stack trace.
- **Workspace-aware.** Point MouseClaw at your project folder once → AI's `Read`/`Write`/`Edit`/`Bash` all run with that as `cwd`. "Fix this bug" actually fixes it.
- **Computer use built-in.** Say "go here" pointing at a place → opens Maps. "Convert this page to Word" → does it. macOS URL schemes + `osascript` + Claude's Bash tool = the assistant actually does things.
- **Browser automation.** One-click enable `chrome-devtools-mcp` against a dedicated debug profile of your real Chrome. AI can drive your tabs, click buttons, fill forms.

### Privacy

- **Clipboard is encrypted** with AES-256-GCM. Key stored in macOS Keychain (only your user can read).
- **Transient-marked content is ignored** — 1Password, Bitwarden, KeePass, terminal sudo prompts: never recorded.
- **Voice + screenshot stay local** until you call AI. The screenshot is a temp file you can `open` and inspect.
- **Workspace path is opt-in.** No automatic file scanning.

### Install

**The easy way:** [Download the latest `.dmg`](https://github.com/edwin-hao-ai/MouseClaw/releases/latest) → drag MouseClaw to Applications.

> ⚠️ The build is **Developer-ID signed but not notarized yet**. First launch: right-click MouseClaw → "Open" → Gatekeeper warning → "Open". Subsequent launches are normal.

**Then on first launch** the onboarding asks you to:

1. **Pick a shortcut** (default `⌘⇧Space`)
2. **Pick an AI backend** — install if missing:
   - Claude Code CLI: `npm i -g @anthropic-ai/claude-code && claude login`
   - Codex CLI: `npm i -g @openai/codex && codex login`
   - OpenClaw CLI: `npm i -g openclaw`
   - Hermes Agent: `curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash`
3. **Pick a pet skin** — 6 styles (classic gray, slim white, chubby brown, ninja, robot, golden)
4. **Pick a voice-IME trigger** — `fn` recommended, can also pick `⌥`, `⌃`, right-shift, right-cmd, right-option, or disable
5. **Grant permissions** — Accessibility (global shortcut), Screen Recording (screenshot for AI), Microphone (voice input)

### What's under the hood

- **Tauri 2** + Rust backend + React/TypeScript frontend
- **Whisper.cpp** with Metal acceleration (base / small / medium / turbo — switchable in tray)
- **CGEventTap** for `fn` long-press detection (the tricky bit)
- **NSPasteboard polling** at 500ms for clipboard capture (Maccy's pattern)
- **AES-256-GCM** + Keychain for clipboard at rest
- **AppleScript / osascript / URL schemes** for computer-use actions
- **CDP bridge** (chrome-devtools-mcp on port 9222) for real-Chrome automation

DMG size: ~5 MB · Memory: ~280 MB idle · CPU: <1% idle.

### Status

**Pre-1.0.** I'm iterating fast. See [CHANGELOG](CHANGELOG.md) for what's shipped.

Known good: macOS 14+ on Apple Silicon. Intel: should work, not tested often. Windows/Linux: not yet.

---

## 中文

### 这是什么？

MouseClaw（鼠标龙虾）是一只睡在你 macOS 屏幕角落的像素老鼠。**按住快捷键 → 说话 → 松开** — 它截屏 + 听你说 + 调本机最顺手的那个 AI CLI（Claude / Codex / OpenClaw / Hermes）来干活。回答流式回到气泡里；如果 AI 判断要写字进光标，它直接帮你打字（绕过输入法）。

三种模式，一只宠物。

| 模式 | 快捷键 | 干啥 |
|---|---|---|
| **🦞 AI 召唤** | 按住 `⌘⇧Space` | 截图 + 语音 → Claude/Codex/OpenClaw/Hermes → 气泡（带 markdown） |
| **🎙️ 语音输入法** | 按住 `fn`（可改 ⌥/⌃/右⇧/右⌘） | Whisper 转写 → 写到你光标里 — 不调 AI，没延迟 |
| **📋 剪贴板 Hub** | `⌘⇧V` / 点桌宠 / 双击托盘 | 所有复制过的内容历史，可搜索 · 标星 · 拖出 |

### 演示视频

<div align="center">

<a href="docs/assets/demo-zh.mp4"><img src="docs/assets/demo-zh.gif" alt="MouseClaw 演示（中文）" width="720" /></a>

*35 秒预览 · [▶ 看中文 MP4](docs/assets/demo-zh.mp4) · [▶ Watch the EN version](docs/assets/demo-en.mp4)*

</div>

### 为什么 daily use？

- **零切换成本** — 不用开窗口、不用切应用，桌宠已经在那
- **AI 后端任选** — 内置 4 个，用哪个 CLI 你已经登过就选哪个
- **圈选 bug** — 按住快捷键，**按住鼠标左键拖** 直接在屏幕上画圈 — AI 拿到的截图带着你画的粉红色标注，回答精准
- **工作区感知** — 给 MouseClaw 指一次项目目录，AI 的 `Read`/`Write`/`Edit`/`Bash` 都以那为 cwd，「修这个 bug」是真的能修
- **Computer use** — 指着地名说「去这里」开 Maps；说「把这页转 Word」就转。macOS URL schemes + osascript + Claude Bash 工具 = AI 真动手
- **浏览器自动化** — 一键启动 `chrome-devtools-mcp` 桥接你的真 Chrome，AI 能操作你登录态的标签页

### 隐私

- **剪贴板用 AES-256-GCM 加密**，密钥存 macOS Keychain（只你能读）
- **遵守 transient 标志** — 1Password / Bitwarden / 终端 sudo 等密码管理器复制的内容永远不记
- **语音 + 截图本地保存**，只在你召唤 AI 时才发出去
- **工作区路径是 opt-in**，没设置就不扫描任何文件

### 安装

**最简单**：[下载最新 dmg](https://github.com/edwin-hao-ai/MouseClaw/releases/latest) → 拖到 Applications。

> ⚠️ dmg **签名了但还没公证**（Developer ID 是有的）。第一次启动右键点 MouseClaw → 「打开」→ Gatekeeper 弹窗 → 「打开」即可。之后正常双击。

**首次启动** Onboarding 会让你：

1. **选快捷键**（默认 `⌘⇧Space`）
2. **选 AI 后端** — 没装就先装：
   - Claude Code CLI：`npm i -g @anthropic-ai/claude-code && claude login`
   - Codex CLI：`npm i -g @openai/codex && codex login`
   - OpenClaw CLI：`npm i -g openclaw`
   - Hermes Agent：`curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash`
3. **挑桌宠皮肤** — 6 款（经典灰 / 小白鼠 / 田鼠 / 忍者鼠 / 机械鼠 / 金鼠）
4. **选语音输入触发键** — 推荐 `fn`，也可以是 `⌥` / `⌃` / 右 ⇧ / 右 ⌘ / 右 ⌥，或不启用
5. **开权限** — 辅助功能（全局快捷键）、屏幕录制（截屏给 AI）、麦克风（语音）

### 技术栈

- **Tauri 2** + Rust 后端 + React/TypeScript 前端
- **Whisper.cpp** Metal 加速（base / small / medium / turbo 托盘里随时切）
- **CGEventTap** 监听 `fn` 长按（最难的一块）
- **NSPasteboard** 500ms 轮询采剪贴板（Maccy 同款模式）
- **AES-256-GCM** + Keychain 加密静态剪贴板
- **AppleScript / osascript / URL schemes** 走 computer use
- **CDP 桥**（chrome-devtools-mcp on :9222）对接你真 Chrome

dmg 体积：~5 MB · 内存：闲时 ~280 MB · CPU：<1%

### 项目状态

**Pre-1.0**，迭代很快。已发布的看 [CHANGELOG](CHANGELOG.md)。

已验证：macOS 14+ Apple Silicon。Intel 应该能跑，没经常测。Windows/Linux 暂不支持。

---

<div align="center">

**Built with care · Apache-2.0 · Issues + PRs welcome**

[Bug report](https://github.com/edwin-hao-ai/MouseClaw/issues/new?labels=bug) · [Feature request](https://github.com/edwin-hao-ai/MouseClaw/issues/new?labels=enhancement) · [Discussions](https://github.com/edwin-hao-ai/MouseClaw/discussions)

</div>
