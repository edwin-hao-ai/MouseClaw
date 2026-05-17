# Changelog · 更新日志

All notable changes to MouseClaw. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [SemVer](https://semver.org/).
所有重要变更。格式遵循 Keep a Changelog，版本号 SemVer。

---

## [0.1.24] · 2026-05-17

### English

#### Added
- **Hermes Agent CLI backend.** Now 4 AI backends preconfigured: Claude Code / OpenAI Codex / OpenClaw / Hermes (Nous Research).
- **Auto update check.** App pings `/version.json` on GitHub Pages 60s after launch (24h cache). Shows a non-intrusive bubble if a newer release is out.
- **Bilingual public landing page** at `edwin-hao-ai.github.io/MouseClaw/`.
- **Product demo video** rendered with HeyGen HyperFrames (24s, 1080p, real animation).

#### Fixed
- **Bubble top no longer feels truncated.** Was streaming auto-scrolled to bottom; now stays at top with `contain: layout`.
- **Crash on rapid copy.** Real root cause: missing `NSAutoreleasePool` around all Cocoa FFI calls in clipboard/voice-IME/mode-B threads → autoreleased objects piled up → crash. All 5 cocoa entry points now wrap their work in a pool.

#### Changed
- Bumped to `0.1.24`, opens auto-update channel.

### 中文

#### 新增
- **Hermes Agent CLI 后端**。现在内置 4 个 AI 后端：Claude Code / OpenAI Codex / OpenClaw / Hermes（Nous Research）。
- **自动版本检查**。启动 60s 后拉一次 GitHub Pages 上的 `version.json`（24h 缓存）。有新版本只用一个 bubble 提示，不打断你。
- **双语 landing page**：`edwin-hao-ai.github.io/MouseClaw/`。
- **产品演示视频**用 HeyGen HyperFrames 渲（24 秒 1080p，真动画）。

#### 修复
- **气泡顶部不再「缺一块」**。之前流式自动滚到底，现改 `contain: layout` 钉在顶部。
- **狂复制时崩溃**。真正元凶找到了：clipboard / voice-IME / mode-B 三处后台线程调 Cocoa FFI 时**没套 NSAutoreleasePool** → 自动释放对象越攒越多 → 必崩。5 个入口点全部加 pool 包住。

#### 调整
- 版本号升到 `0.1.24`，启动自动更新通道。

---

## [0.1.23] · 2026-05-17

### English
- **macOS computer-use prompt** taught to AI: opens Maps via `maps://?q=`, mailto/tel/facetime URL schemes, osascript for Calendar/Reminders, `pandoc` for "convert this page to Word".
- **Voice IME trigger key choice** — `fn` / `⌥` / `⌃` / right-`⇧` / right-`⌘` / right-`⌥`, or disable. Picked in Onboarding Step 4.
- **⌘⇧V opens clipboard Hub** as an independent window (not stuck behind the pet).
- **Tray fix:** single-select submenus no longer accumulate ticks after multiple changes (we now rebuild the menu).
- **Voice IME safety:** 60s force-cutoff, password-field detection, frontmost-app-switch cancels.
- **Hub right-click context menu**: paste / copy-without-paste / pin / delete.

### 中文
- **教 AI 用 macOS 系统功能** ：「去这里」→ Maps，邮件/电话/FaceTime URL schemes，osascript 加日历/提醒事项，pandoc 把网页转 Word。
- **语音输入触发键 6 选 1** —— `fn` / `⌥` / `⌃` / 右 `⇧` / 右 `⌘` / 右 `⌥`，或不启用。Onboarding 第 4 步选。
- **⌘⇧V** 直接打开剪贴板 Hub 独立窗口（不再卡在桌宠后面）。
- **托盘单选 bug 修了**：换皮肤 / 模型 / 语言时旧的 ✓ 不再叠加。
- **语音 IME 安全**：60s 强制截止、密码字段拒绝触发、录音中切走前台自动取消。
- **Hub 右键菜单**：粘贴 / 仅复制 / 标星 / 删除。

---

## [0.1.20] · 2026-05-17

### English
- **Cursor trail recording during AI summon.** Press shortcut, hold left mouse + drag — your highlights bake into the screenshot. AI knows exactly what you mean.
- **Live draw overlay** shows the pink lines as you draw, full-screen transparent, click-through (your underlying app still receives clicks).
- **Workspace folder.** Tray menu → "Workspace…" → choose a project root. AI's `Read`/`Edit`/`Bash` run with that as cwd.

### 中文
- **AI 召唤期间记录鼠标轨迹**。按住快捷键 + 按住鼠标左键拖 — 你画的粉红圈烘进截图。AI 一眼看懂你说哪。
- **实时画板 overlay**：全屏透明，画的同时就能看见线，点击穿透（你点底层 app 照样触发）。
- **工作区**：托盘 →「工作区…」选项目根目录，AI 的 Read/Edit/Bash 都以此为 cwd。

---

## [0.1.18] · 2026-05-17

### English
- **Hub focus rescue.** Records the previous frontmost app's PID before opening; explicitly `NSRunningApplication.activate()`s it before pasting. Fixes "click an item but it pastes nowhere."
- **Hub window resized** to 420×480 (was 540×640).
- **1-9 quick select** replaces ⌘1-9 (avoids Chrome tab / Slack channel conflicts).

### 中文
- **Hub 焦点救援**：开窗前记下当时前台 app 的 pid，粘贴时显式 `NSRunningApplication.activate()` 拉回。修「点条目但粘不到原输入框」。
- **Hub 窗口缩到 420×480**（原 540×640）。
- **1-9 数字快选**替代 ⌘1-9（避开 Chrome tab / Slack channel 冲突）。

---

## [0.1.14] · 2026-05-17

### English
- **Clipboard encryption.** AES-256-GCM with key in macOS Keychain. Old plain JSONL auto-migrated.
- **New pet sprites:** type (voice IME, paw raised like a pen) / hub (sitting + closed eyes) / paste (clipboard in paw).
- **Drag-out from Hub** to any app.
- **Hover preview** (>500ms) shows full clipboard item content.

### 中文
- **剪贴板 AES-256-GCM 加密**，密钥存 macOS Keychain。老 plain JSONL 自动迁移。
- **3 个新桌宠 sprite**：type（语音 IME，前爪举笔状）/ hub（坐下闭眼）/ paste（爪子拎剪贴板）。
- **Hub 拖出**：剪贴板项可以直接拖到任意 app drop。
- **Hover 预览**：停留 >500ms 显示完整内容浮层。

---

## [0.1.10] · 2026-05-15

### English
- **Bug fixes (4):** clipboard scroll restored / markdown rendering in bubble / "Continue chat" opens an independent Panel window / bubble pointer hidden when scrollable.
- **Whisper Simplified Chinese** — initial prompt biases output to Simplified.
- **Typeless-style LLM cleanup** opt-in toggle in tray.
- **Click pet → Hub** with tabs (clipboard / AI history), search, ↑↓/Enter/1-9 nav.

### 中文
- **4 个 bug 修复**：剪贴板恢复滚动 / 气泡 markdown 渲染 /「继续追问」开独立 Panel 窗口 / 气泡尾巴在滚动模式自动隐藏。
- **Whisper 中文强制简体**：加 initial_prompt 偏向简体输出。
- **Typeless 风格 LLM 整理**：托盘可一键开（默认关，因为加 3-8s 延迟）。
- **点桌宠 → Hub**：tabs（剪贴板 / AI 历史），搜索 / ↑↓ / ↵ / 1-9 操作。

---

## [0.1.7] · 2026-05-15

### English
- **6 mouse skins:** classic gray, slim white (lab), chubby brown (field), ninja, robot, golden. Pick in Onboarding or change live from tray.
- **Multi-backend abstraction:** Claude Code CLI / OpenAI Codex / OpenClaw CLI selectable in Onboarding.
- **Chrome DevTools MCP bridge** for browser automation (one-click enable with dedicated debug profile).
- **i18n framework** (zh + en) with type-safe keys, no external i18next.

### 中文
- **6 款桌宠皮肤**：经典灰 / 小白鼠 / 田鼠 / 忍者鼠 / 机械鼠 / 金鼠。Onboarding 选或托盘随时换。
- **多 AI 后端抽象**：Claude Code / Codex / OpenClaw CLI 三选一。
- **Chrome DevTools MCP 桥**：一键启用对接你真 Chrome 的专用 debug profile，AI 能开你登录态的 tab。
- **i18n 框架**（中 + 英），类型安全 key，不引 react-i18next。

---

## [0.1.0] · 2026-05-13 · Initial release

### English
- **AI summon mode**: hold ⌘⇧Space, speak, release — screenshot + voice → Claude Code CLI → streaming reply in pixel-art speech bubble.
- **Mode B**: AI writes text directly into your cursor (bypasses IME). Terminal-app blocking.
- **Pixel-art mouse pet** with 8-color palette, 6 anatomy states (sleep/listen/think/jump/write/block).

### 中文
- **AI 召唤模式**：按住 ⌘⇧Space → 说话 → 松开。截图 + 语音 → Claude Code CLI → 流式回到像素气泡。
- **Mode B**：AI 直接把字写到你光标（绕过输入法）。终端前台自动屏蔽。
- **像素老鼠桌宠**：8 色调色板，6 个动作状态（睡/听/想/跳/写/阻塞）。

---

[0.1.24]: https://github.com/edwin-hao-ai/MouseClaw/releases/tag/v0.1.24
[0.1.23]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.20...v0.1.23
[0.1.20]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.18...v0.1.20
[0.1.18]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.14...v0.1.18
[0.1.14]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.10...v0.1.14
[0.1.10]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.7...v0.1.10
[0.1.7]: https://github.com/edwin-hao-ai/MouseClaw/compare/v0.1.0...v0.1.7
[0.1.0]: https://github.com/edwin-hao-ai/MouseClaw/releases/tag/v0.1.0
