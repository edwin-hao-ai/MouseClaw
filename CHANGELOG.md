# Changelog · 更新日志

All notable changes to MouseClaw. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [SemVer](https://semver.org/).
所有重要变更。格式遵循 Keep a Changelog，版本号 SemVer。

---

## [0.1.28] · 2026-05-18

### Fixed
- **🖱️ 桌宠在角落点击没反应** — 根因：overlay 窗口 `focus: false` 但没设
  `acceptFirstMouse: true`，macOS 把第一次点击吃掉去做焦点抢夺，永远 fire
  不到 click handler。一行 fix（tauri.conf.json）

### Added
- **👻 桌宠位置新增"隐藏"选项** — 第 6 个 anchor option。完全不显示桌宠，
  只在按召唤快捷键 / nudge 提醒触发时才出现。给"只要工具不要伴侣"的用户
- **🔍 AI 后端 CLI 安装检测** — Onboarding 第 2 步进来自动并发检测 4 个后端
  （`claude` / `codex` / `openclaw` / `hermes`）是否装在 PATH 里。已装显示 ✓ 绿色
  pill，未装显示黄色 pill + 一行可复制的 `npm i -g …` 命令 + 官网链接

### Technical
- 新 command：`check_backend_installed(backend) -> BackendInstallStatus`
- `Backend::install_url()` + `install_cmd()` 集中维护安装指引
- `PetAnchor::Hidden` 加入 `pin_visible_when_idle() = false` 分支

---

## [0.1.27] · 2026-05-18

### Added
- **🦞 桌宠悬停位置 / Pet anchor** — 老鼠有"家"了。4 个屏角或跟随光标 5 选 1。闲置时在角落
  打盹 zzz，召唤时跑到光标位置工作，完事跑回家。Onboarding 多一步、托盘新增 `📍 桌宠位置 ▸`
  子菜单
- **🖱️ 点击桌宠 → 菜单 / Click pet → menu** — 6 项玻璃气泡：召唤 / 历史 / 喂奶酪 / 休息 15min /
  换形象 / 设置。喂奶酪有 +N 累计 badge，休息会让 nudge 引擎全静音
- **🔔 环境感知主动提醒 / Proactive nudges** — 老鼠"看你做什么"全靠本地系统信号，**0 LLM
  调用 / 0 token 成本**：
  - 🧘 久坐（鼠标 ≥ 90min 不动）
  - 🤔 卡壳侦测（IDE 前台 5min 无敲键 + 鼠标乱动）⭐ — 主动喂 LLM 入口
  - 🌙 深夜劝睡（≥ 23:30 + 仍活跃）
- **心流保护** — 连续敲键 30min → 所有提醒自动排队不打扰
- **隐私红线** — 用 `CGEventSourceSecondsSinceLastEventType`（API 物理上拿不到键值），只看
  前台 bundle id，30min 环形 buffer 不落盘不联网

### Changed
- 点击桌宠从"直接打开剪贴板"改成"弹出菜单"（菜单里仍有 📜 历史入口）
- Onboarding 从 5 步扩到 6 步（多了 anchor picker，伴侣感的"选个家"环节）
- `config.json` schema v14 → v15（自动迁移）

### Technical
- 新模块：`anchor.rs` · `presence.rs` · `nudge.rs`（~650 LOC + 20 单测）
- 新前端组件：`PetMenu.tsx` · `NudgeBubble.tsx`
- 新 commands：`save_pet_anchor` / `get_pet_anchor` / `set_nap_until` / `dismiss_nudge`
- 设计原型：`docs/prototypes/pet-anchor-menu-nudges-20260518.html`

---

## [0.1.26] · 2026-05-18

### English

#### Added
- **Launch at login** via `tauri-plugin-autostart` (LaunchAgent on macOS). Onboarding step 5 default-checks it; tray toggle `🚀 Launch at login` flips both the LaunchAgent and the config mirror. Bidirectional sync at startup — if you disable it in System Settings → Login Items the app reconciles to match. Autostart-launched processes pass `--minimized`, so the app stays silent in the menubar (no Onboarding pop) on boot.
- **Pet picker window** replaces the 6-item tray submenu. Tray now shows a single `🎨 Change pet… (current)` that opens a dedicated 760×600 window with the real pixel-art previews grouped by species, a large hover preview, single-click peek / double-click apply, and a Cancel-to-revert behavior.
- **3 new species** drawn for v0.1.26 — total **9 pets**:
  - 🐱 **Gray Cat** — pointy ears · long curving tail · faint whiskers · green eyes
  - 🦊 **Red Fox** — large triangle ears · bushy tail with white tip · white V-chest
  - 🐸 **Tree Frog** — bulging eyes on top of head · wide green body · pale belly · no tail

### 中文

#### 新增
- **开机自启动**（用 `tauri-plugin-autostart`，macOS LaunchAgent）。Onboarding 第 5 步默认勾选；托盘菜单「🚀 开机自启动」一键开关，同时翻转 LaunchAgent 与 config。启动时双向同步 —— 用户在「系统设置 → 通用 → 登录项」里关掉，app 下次启动会读真实状态回写 config。自启动时传 `--minimized`，菜单栏静默驻留（不弹 Onboarding）。
- **桌宠选择器窗口** 替代托盘 6 项子菜单。托盘只剩单条「🎨 更换桌宠…（当前皮肤）」，点开 760×600 独立窗口：按物种分组的真实像素艺术卡片 + 大预览 + 单击试穿 / 双击应用 / 取消还原。
- **3 个新物种** ——总 **9 只**桌宠：
  - 🐱 **小灰猫** —— 尖三角耳、长卷尾、淡胡须、绿眼睛
  - 🦊 **赤狐** —— 大三角耳、蓬松大尾带白尾尖、白胸 V 形
  - 🐸 **树蛙** —— 头顶突眼、宽绿身、浅黄肚、无尾

#### 调整
- 托盘菜单皮肤入口从 6-item submenu（只能看到 emoji）改成单条入口 + 真实形象 picker。
- 加入新物种后 `SkinId` 从 6 项扩展到 9 项，体型变体新增 `cat` / `fox` / `frog`，调色板 / 耳/身/尾在 `PixelMouse.tsx` 各加独立分支。
- v0.1.27 起 `~/.mouseclaw/skins/` 支持加载用户安装的桌宠 manifest（社区路径已留好）。

---

## [0.1.25] · 2026-05-17

### English

#### Added
- **Bundled Whisper base model** (`ggml-base-q5_1.bin`, ~57MB). First launch no longer waits on a 60MB download — voice IME works offline immediately. Larger models (small/medium/turbo) still download on demand.
- **`~/.mouseclaw/provider.env`** convention — drop `OPENAI_API_KEY` / `AI_GATEWAY_API_KEY` / `ANTHROPIC_API_KEY` etc. into one file, MouseClaw passes them to spawned CLI backends. No more shell-rc gymnastics. Template at `docs/provider-env-template.md`.
- **All 4 backends verified via Vercel AI Gateway** (Claude Code / Codex / Hermes ✅; OpenClaw partial — needs valid gateway model id).

#### Changed
- `scripts/notarize-dmg.sh` defaults to keychain profile `OCTAgentNotary` (shared with our other projects); auto-picks the newest `MouseClaw_*_aarch64.dmg` if no path is passed.

### 中文

#### 新增
- **Whisper base 模型打进安装包**（57MB）。首次启动不用等下载，离线就能用语音 IME。更大的 small/medium/turbo 仍按需下载。
- **`~/.mouseclaw/provider.env`** 约定 —— 把 `OPENAI_API_KEY` / `AI_GATEWAY_API_KEY` / `ANTHROPIC_API_KEY` 写一个文件，MouseClaw 直接灌给 CLI 子进程，不用改 shell rc。模板：`docs/provider-env-template.md`。
- **4 个后端实测过 Vercel AI Gateway**（Claude Code / Codex / Hermes ✅；OpenClaw 部分跑通）。

#### 调整
- `scripts/notarize-dmg.sh` 默认 keychain profile `OCTAgentNotary`，无参数时自动选最新 `MouseClaw_*_aarch64.dmg`。

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
