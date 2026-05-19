<div align="center">

# 🦞 MouseClaw

**桌面养只老鼠 · AI 帮你干活**
**A pixel-art desktop AI that lives in your screen corner**

[中文](#中文) · [English](#english) · [官网](https://edwin-hao-ai.github.io/MouseClaw/) · [下载最新版](https://github.com/edwin-hao-ai/MouseClaw/releases/latest)

<img src="docs/assets/hero.png" alt="MouseClaw hero" width="720" />

**v0.4 · 12 MB 安装包 · 首启下载 ~260 MB 语音模型 · 100% 本地推理 · 国内无需 VPN**

</div>

---

## 中文

### 这是什么？

一只睡在你 macOS 屏幕角落的**像素老鼠**。

- **按住 ⌘⇧Space + 说话** → 它截屏 + 听你说 + 调本机的 AI（Claude/Codex/OpenClaw/Hermes）→ 流式回答到气泡
- **长按 fn + 说话** → 中英文语音直接打到任何输入框的光标位置（不切输入法）
- **拖文件给它** → 老鼠"吞下"后你接着说"用三句话总结" → AI 读懂文档帮你

100% 本地推理（sherpa-onnx 语音识别 + 标点模型，零字节上云），但 AI 调用部分用你自己装的 CLI（你的 token 你做主）。

### 演示

<a href="docs/assets/demo-zh.mp4"><img src="docs/assets/demo-zh.gif" alt="MouseClaw demo" width="720" /></a>

*30-秒功能演示 · [▶ 看完整版 MP4](docs/assets/demo-zh.mp4)*

### 为什么用得久

- **零切换**：没窗口要打开，没 app 要 focus，老鼠**就在那**
- **AI 后端你选**：装了 Claude / Codex / Hermes / OpenClaw 都行，托盘菜单切换
- **圈定问题**：按住快捷键 +  **按左键拖动画圈** → AI 拿到的截图带你高亮的部分。试试在 stack trace 上圈
- **认识你的项目**：托盘指定 workspace path 一次 → AI 的 Read/Write/Bash 都以这个为 cwd。"修这个 bug" 真能修
- **能动手**：说"打开附近的咖啡馆" → 开 Maps。"把这个网页转成 Word" → 干。macOS URL schemes + osascript + Claude Bash 工具 = AI 真的能做事
- **浏览器自动化**：托盘一键开 `chrome-devtools-mcp`，AI 能驱动你的真实 Chrome（独立调试 profile）
- **历史会话**：托盘"📜 查看历史" → 点任意旧对话「💬 继续这个话题」→ 接着聊不丢上下文

### v0.4 新功能（2026-05）

- **📦 DMG 12 MB**（之前 238 MB）—— 模型首启动按需下载，国内优先 hf-mirror.com / gh-proxy.com
- **🌐 中文 + 英文双模型**：Onboarding 选语言，海外用户用英文模型（73 MB），中文用户用 zh-en bilingual（199 MB）
- **📥 多镜像 fallback + 断点续传**：网络抖动自动切镜像 / 断网重启接着下 / 30s 速度<1KB/s 主动断开
- **🎓 首次使用引导**：模型下完桌宠主动跳出 5 步教学 · PetMenu「📖 教我用」可重看
- **✏️ 语音确认编辑**：转写完 3 秒倒数 · Esc 取消 / Enter 立即 / 点气泡进编辑 — 防止语音误识别浪费 token
- **🍽 拖文件喂桌宠**：Finder 文件拖到老鼠嘴里 = AI 读 PDF/网页/截图/代码
- **🦞 桌宠跟随光标**：托盘开关，老鼠在屏幕里跟着你的鼠标走
- **🎨 6 款皮肤**：经典灰 / 小白鼠 / 田鼠 / 忍者 / 机械 / 金鼠

### 隐私

- **语音识别 100% 本地**：sherpa-onnx Zipformer 模型在你电脑上跑，不联网
- **剪贴板 AES-256-GCM 加密**，密钥存 macOS Keychain（只有你能读）
- **标记为 transient 的内容直接忽略**：1Password / Bitwarden / sudo 提示 → 永远不记
- **截图保存在临时文件**，可以 `open` 查看
- **AI 调用走你自己的 CLI**：你装了 Claude Code 就用你的 Anthropic key；装 Codex 就用 OpenAI；不上 MouseClaw 的云

### 安装

**最简单**：[下最新 `.dmg`](https://github.com/edwin-hao-ai/MouseClaw/releases/latest) → 拖到 Applications。

> ⚠️ Developer-ID 签名但未公证。首次启动：右键 MouseClaw.app → "打开" → Gatekeeper 警告 → 仍打开。

**首启 onboarding**：
1. **选快捷键**（默认 `⌘⇧Space`）
2. **选 AI 后端**（缺哪个 onboarding 会告诉你怎么装）
3. **选桌宠皮肤**
4. **选语音模型语言** + **语音输入触发键**
5. **桌宠位置** + **授权**（辅助功能 / 屏幕录制 / 麦克风）

完事后桌宠开始下载语音模型（zh-en ~199MB / en ~73MB + 标点 ~62MB），下完会主动教你用一次。

### 系统要求

macOS 11+ · Apple Silicon 推荐（Intel 没仔细测）· Windows/Linux 暂无

### 状态

**Pre-1.0**，迭代快。看 [CHANGELOG](CHANGELOG.md) 知道最新。

---

## English

### What is it?

A pixel-art mouse that sleeps in the corner of your macOS screen.

- **Hold ⌘⇧Space + speak** → screenshots + transcribes + asks your AI CLI (Claude / Codex / OpenClaw / Hermes) → streams reply into a speech bubble
- **Hold fn + speak** → voice transcription typed directly into your cursor in any app (no IME switching, English & Chinese)
- **Drag a file onto the pet** → it "eats" the file, then you say "summarize in 3 sentences" → AI reads it

100% local speech recognition (sherpa-onnx + punctuation model, zero bytes uploaded). AI calls use **your own CLI** (your tokens, your rules).

### Demo

<a href="docs/assets/demo-en.mp4"><img src="docs/assets/demo-en.gif" alt="MouseClaw demo" width="720" /></a>

*30-second tour · [▶ Watch full MP4](docs/assets/demo-en.mp4)*

### Why daily-use

- **Zero context switch.** No window to open, no app to focus
- **Pick your AI brain.** Claude / Codex / OpenClaw / Hermes — switch in tray menu
- **Circle the bug.** Hold shortcut + **left-click + drag** to highlight on screen. AI gets screenshot with your marks
- **Workspace-aware.** Tray sets project folder → AI's Read/Write/Bash use it as cwd. "Fix this bug" actually fixes it
- **Computer use built-in.** macOS URL schemes + osascript + Claude's Bash tool → AI actually does things
- **Browser automation.** Tray enables `chrome-devtools-mcp` against dedicated profile of your real Chrome
- **Session history.** Tray → "📜 View history" → "💬 Continue this conversation" picks up where you left off

### v0.4 highlights (May 2026)

- **📦 12 MB DMG** (was 238 MB) — models download on first launch with multi-mirror fallback
- **🌐 Chinese + English** — pick at Onboarding; English-only users get 73 MB model (vs 199 MB bilingual)
- **📥 Resumable downloads** — auto-switch mirror on failure, resume from `.part` on crash/restart
- **🎓 First-run tour** — pet teaches you in 5 steps after model downloads; "📖 Teach me" in pet menu to redo
- **✏️ Voice confirmation** — 3-second countdown after transcription, Esc cancel / Enter send / click to edit (prevents token waste on misrecognized speech)
- **🍽 Feed files** — drag PDF/page/screenshot/code onto pet, AI reads
- **🦞 Cursor follow** — pet follows your mouse around screen (toggle in tray)
- **🎨 6 skins** — classic / white / brown / ninja / robot / gold

### Privacy

- **Voice recognition 100% local.** sherpa-onnx Zipformer runs on your CPU/Metal
- **Clipboard AES-256-GCM encrypted** with key in macOS Keychain
- **Transient-marked content ignored** (1Password, Bitwarden, sudo prompts)
- **Screenshots in tmpfs**, `open` to inspect
- **AI calls use your CLI** — your Anthropic / OpenAI key, never through us

### Install

[Download latest `.dmg`](https://github.com/edwin-hao-ai/MouseClaw/releases/latest) → drag to Applications.

> ⚠️ Developer-ID signed but not notarized yet. First launch: right-click → "Open" → Gatekeeper → Open.

First-launch onboarding picks shortcut, AI backend, skin, **voice language**, voice-IME trigger, pet anchor, permissions. Then downloads voice models (~199 MB zh-en / ~73 MB en + 62 MB punctuation).

### Requirements

macOS 11+ · Apple Silicon recommended (Intel less tested) · Windows/Linux not yet.

### Status

**Pre-1.0**, moving fast. See [CHANGELOG](CHANGELOG.md).

---

## Under the hood

- **Tauri 2** (Rust + React/TypeScript)
- **sherpa-onnx streaming Zipformer** for ASR (CPU + CoreML)
- **sherpa-onnx CT-Transformer** for Chinese punctuation
- **CGEventTap** for `fn` long-press detection
- **NSPasteboard polling** @ 500ms (Maccy's pattern)
- **AES-256-GCM + Keychain** for clipboard at rest
- **chrome-devtools-mcp** bridge for browser automation
- **Curl with `-C -`** for resumable model downloads + multi-mirror fallback
- Multi-backend: Claude Code CLI / OpenAI Codex CLI / OpenClaw / Nous Hermes Agent

App: 12 MB DMG · 35 MB installed · ~260 MB voice models (downloaded first launch) · ~400 MB RAM idle (incl. loaded models) · <1% CPU idle.

---

## Contributing

Bug reports + screenshots welcome at [Issues](https://github.com/edwin-hao-ai/MouseClaw/issues).

This is a personal "vibe project" — I make it pretty first, useful second. PRs that match the design direction (see [DESIGN.md](DESIGN.md)) are merged fast.

## License

MIT
