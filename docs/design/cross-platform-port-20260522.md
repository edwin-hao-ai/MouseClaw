# 跨平台移植设计：Windows + Linux（X11 & Wayland）

> 立项 2026-05-22。目标：让 MouseClaw 在 **Windows / Linux-X11 / Linux-Wayland** 上达到与
> macOS **功能基本对齐**，同时**不破坏 macOS**。本文是这件事的「全集」——每个 macOS 专属能力
> 对应到三个新目标的实现方案、所选 crate、风险等级、以及由此产生的 UX 差异。
>
> 配套硬规则见 CLAUDE.md。本文是落地这次移植的单一信源；每完成一项回来打勾。

## 0. 验证策略与诚实的边界（先说清楚）

这次移植**无法在开发容器里运行时验证**：容器是无头 Linux（无 `DISPLAY`）、没有 Windows、
没有 macOS。因此：

| 层级 | 怎么验 | 谁来验 |
|---|---|---|
| **编译**（Linux） | 本地 `cargo check`（已装齐 webkit2gtk / gtk / x11 / xtst / atspi / alsa；sherpa 预置 `SHERPA_ONNX_LIB_DIR`） | Claude 本地 |
| **编译**（Win / macOS） | GitHub Actions matrix 构建 → 读 CI 日志 | Claude 通过 CI |
| **运行时行为**（截图截对屏？Mode B 键入对不对？选词抓得到吗？托盘/穿透/热键？） | **只能在真机上人工测** | **用户**（Win / 各 Linux 桌面） |

**铁律**：CI 绿 ≠ 功能可用。任何「已实现」的平台原生功能，在用户真机验证前都标 ⚠️ 未运行时验证。

## 1. 现状盘点

代码库早已为非 macOS 留了 `#[cfg(not(target_os = "macos"))]` 兜底，但**全是 no-op**。
天然跨平台、已可用的部分：AI pipeline（13 个 CLI backend）、托盘、全局快捷键、窗口、
sherpa-onnx ASR 引擎本身、autostart 插件、SQLite 记忆、配置、i18n。

**会直接挡住编译**的问题（不是 no-op，是真编译错误）：

| 问题 | 文件 | 说明 |
|---|---|---|
| `security-framework` 在跨平台 deps 里 | `Cargo.toml:39` + `clipboard_crypto.rs` | 这是 Apple 专属 crate（Keychain），Win/Linux 编译会炸 |
| 发布版日志重定向只在 macOS 有 | `lib.rs:204` | Win/Linux release 版日志无处可去（功能缺失，非编译错误） |

## 2. 逐功能移植矩阵

风险图例：🟢 低（成熟跨平台 crate）/ 🟡 中（需平台 API，但路子清楚）/ 🔴 高（平台分裂 / Wayland 限制 / 需 UX 重新设计）

### 2.1 截图上下文 — `screenshot.rs`
macOS：`screencapture` CLI 抓光标所在屏整图 + Cocoa 算光标在哪块屏。

| 目标 | 方案 | crate | 风险 |
|---|---|---|---|
| Windows | 枚举显示器，取光标所在屏整图 | `xcap`（DXGI/GDI） + `GetCursorPos`(windows-rs) | 🟢 |
| Linux X11 | `xcap` 抓屏 + `XQueryPointer` 取光标屏 | `xcap` + x11 | 🟡 |
| Linux Wayland | `org.freedesktop.portal.Screenshot`（xcap 内置走 portal） | `xcap` | 🔴 portal 可能弹一次授权框 / 某些合成器返回交互式选择；首次有摩擦 |

- 临时文件已用 `std::env::temp_dir()`（`screenshot.rs:29`，跨平台 OK）。
- **UX 差异**：Wayland 首次截图会有 portal 授权框 → onboarding 要解释。

### 2.2 Mode B 写回光标 + 听写打字 — `mode_b.rs` / `voice_ime.rs` 打字部分
macOS：`CGEventKeyboardSetUnicodeString` 直发 unicode；Electron/Chrome 退回剪贴板 ⌘V；退格用 keycode 51。

| 目标 | 方案 | crate | 风险 |
|---|---|---|---|
| Windows | `SendInput` 发 unicode（`KEYEVENTF_UNICODE`）；退格 `VK_BACK`；退回 Ctrl+V | `enigo`（封装 SendInput） | 🟡 |
| Linux X11 | `XTest` 合成按键 / unicode | `enigo`（XTest 后端） | 🟡 |
| Linux Wayland | 合成输入受限：需 `zwp_virtual_keyboard` / libei；多数合成器不允许任意 app 注入 | `enigo`（实验性 wayland）或退回剪贴板+提示用户手动粘贴 | 🔴 大概率只能做到「填到剪贴板 + 提示」 |

- **终端安全红线**（CLAUDE.md）：前台是终端时强制走 A 模式。需要跨平台「前台 app 名」→ 见 2.7。
- **UX 差异**：Wayland 下 Mode B 自动键入很可能降级为「已复制到剪贴板，请按 Ctrl+V」。这是
  产品体验变化，**需用户拍板**（见 §4）。

### 2.3 选词检测 — `selection.rs`
macOS：轮询 `AXSelectedText`。

| 目标 | 方案 | crate | 风险 |
|---|---|---|---|
| Linux X11 | **读 PRIMARY selection**（X11 选中即进 primary，远比 AT-SPI 简单可靠） | `arboard`（支持 primary） | 🟢 比 mac 还稳 |
| Linux Wayland | primary-selection 协议各合成器支持不一；或 AT-SPI | `wl-clipboard` 思路 / `atspi` | 🔴 |
| Windows | 无「primary selection」概念 → UIAutomation `TextPattern.GetSelection` | `windows`(UIAutomation) | 🔴 COM 繁琐、多 app 边界多 |

- **已知限制延续**：CLAUDE.md 记载 mac 上选词只在原生 app 生效。Win UIAutomation 覆盖面也不全，
  Wayland 更差。统一兜底：**复制路径**（reactive ribbon）始终可用。

### 2.4 听写触发热键 — `voice_ime.rs`（全文件 `#![cfg(macos)]`）
macOS：`CGEventTap` 监听 fn / 修饰键**长按**。

| 目标 | 方案 | 风险 |
|---|---|---|
| Windows | `SetWindowsHookEx(WH_KEYBOARD_LL)` 全局键盘钩子检测长按 | 🟡 |
| Linux X11 | `XGrabKey` / evdev | 🔴 fn 键常不上报 |
| Linux Wayland | 无全局键捕获；需 `org.freedesktop.portal.GlobalShortcuts`（新、合成器支持参差） | 🔴 |

- **UX 差异（需用户拍板）**：「按住 fn 长按听写」这个手势在 Win/Linux 不一定可复刻。
  建议：Win/Linux 默认改成**普通全局快捷键**（已有 `tauri-plugin-global-shortcut` 跨平台），
  长按手势作为 Win 上的可选项。这是默认交互的改变 → 见 §4。

### 2.5 TTS 朗读 — `tts.rs`（全文件 `#![cfg(macos)]`）
macOS：`say` 命令。

| 目标 | 方案 | crate | 风险 |
|---|---|---|---|
| Windows | SAPI5 | `tts` crate（跨平台） | 🟢 |
| Linux | speech-dispatcher（需用户装 `speech-dispatcher`） | `tts` crate | 🟡 缺 speechd 时静默降级 |

### 2.6 剪贴板监控 + 加密 — `clipboard.rs` / `clipboard_crypto.rs`
macOS：轮询 `NSPasteboard.changeCount`，读 `public.utf8-plain-text`，检测 1Password transient 标志；
密钥存 Keychain。

| 子项 | 目标 | 方案 |
|---|---|---|
| 读剪贴板 + 变更检测 | 全部 | `arboard` 读文本 + 哈希轮询（保持现有轮询模型） |
| 敏感剪贴板抑制 | Win | `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory` 格式 |
| 敏感剪贴板抑制 | Linux | KDE `x-kde-passwordManagerHint`；其他桌面尽力而为 |
| 密钥存储 | Win | Credential Manager |
| 密钥存储 | Linux | Secret Service / libsecret |

- **密钥存储统一**：用 `keyring` crate（mac Keychain / Win Cred Mgr / Linux Secret Service）。
  **macOS 保留现有 `security-framework`**（避免改动现有用户 Keychain 条目语义），Win/Linux 用 keyring。
  → `clipboard_crypto.rs` 拆 `#[cfg]` 后端。
- Linux 无 Secret Service（headless / 无 keyring daemon）时：降级为 `~/.mouseclaw/` 下 0600 权限明文 key 文件 + 日志告警。

### 2.7 前台 app 名 / bundle — `frontmost.rs`（`#![cfg(macos)]`） + `mode_b::frontmost_app_*`
用途：session 边界、Mode B 终端检测、剪贴板来源标注。

| 目标 | 方案 | 风险 |
|---|---|---|
| Windows | `GetForegroundWindow` + `GetWindowThreadProcessId` → 进程名 + 标题 | 🟢 |
| Linux X11 | `_NET_ACTIVE_WINDOW` + `WM_CLASS` / `_NET_WM_NAME` | 🟡 |
| Linux Wayland | **无标准方式读前台窗口**（安全模型禁止） | 🔴 退化：session 边界只靠时间；终端检测失效 → Wayland 下 Mode B 更保守 |

### 2.8 点击穿透 — `pet_passthrough.rs`
macOS：`NSWindow setIgnoresMouseEvents` + 全局光标 hit-test（光标在桌宠 box 内才接收点击）。

| 目标 | 方案 | 风险 |
|---|---|---|
| 全部 | Tauri `WebviewWindow::set_ignore_cursor_events(bool)`（跨平台） | 🟢 API 层 |
| 动态 hit-test 所需「全局光标位置」 Win | `GetCursorPos` | 🟢 |
| 同上 X11 | `XQueryPointer` | 🟡 |
| 同上 Wayland | **无全局光标位置 API** | 🔴 退化：不做动态 hit-test，改用 webview 内 CSS 命中区 / 始终穿透透明区 |

### 2.9 全局光标轮询类 — `cursor_follow` / `cursor_trail` / `companion` / `presence` / `pet_passthrough`
都依赖「全局光标位置」+「idle 秒数」。Win/X11 可得；**Wayland 不可得** → 这些陪伴动效在
Wayland 上退化（桌宠不跟随、无轨迹、idle 检测靠键盘钩子或干脆关）。统一抽象一个
`platform::global_cursor()` -> `Option<(f64,f64)>`，Wayland 返 `None`，上层据此降级。

### 2.10 拖文件投喂 — `drag_detector.rs`
macOS：轮询 NSPasteboard 拖拽 + 鼠标键状态（绕开 WKWebView 吞 HTML 文件）。

| 目标 | 方案 | 风险 |
|---|---|---|
| Win/Linux | 改用 Tauri `WindowEvent::DragDrop`（mac 因 WKWebView 才绕开；WebView2 / webkit2gtk 行为不同，多数可用） | 🟡 需真机测各文件类型 |

### 2.11 权限 — `permissions.rs`
macOS：TCC（辅助功能 / 屏幕录制 / 麦克风）。Win/Linux 无对应 TCC，`check_all()→all true` 兜底基本 OK。
例外：**Wayland 截图走 portal 有运行时授权**；**麦克风**在某些 Linux 配置需 PipeWire portal。
onboarding 的「授权三连」页 macOS 专属 → Win/Linux 改成精简页（见 §4）。

### 2.12 杂项
- **无 dock / LSUIElement**：mac 用 accessory activation policy（已 `#[cfg]` 兜底 no-op）。Win/Linux
  靠窗口 `skipTaskbar` + 仅托盘。OK。
- **发布版日志**：`lib.rs:204` 的 fd 重定向是 unix+mac 专属。Win/Linux release 版需跨平台文件日志
  （写 `~/.mouseclaw/mouseclaw.log` 或 `%APPDATA%`）。补一个跨平台 logger。
- **asset protocol scope**（`tauri.conf.json:37`）：含 mac 专属 `/private/var/folders/**`。
  需加 Win temp（`$APPDATA`/`$TEMP`）和 Linux `/tmp`、`$HOME/.mouseclaw`。
- **打开 URL/文件夹**：`commands.rs` 用 `open`、`tray_actions.rs` 用 `osascript` 选文件夹。
  改走 `tauri-plugin-opener`（已是依赖）+ `tauri-plugin-dialog` 选文件夹。
- **`open_accessibility_settings`**：mac 专属深链；Win/Linux 该命令 no-op 或指向对应设置。

## 3. 打包 / 构建 / 签名

| 平台 | bundle target | 签名 | 备注 |
|---|---|---|---|
| macOS | `dmg`（现状） | Developer ID + notarize（现状脚本） | 不动 |
| Windows | `nsis`（安装器）+ 可选 `msi` | 暂不签名（或后续 EV 证书 / Azure Trusted Signing） | 未签名会有 SmartScreen 警告，文档说明 |
| Linux | `deb` + `appimage`（CI 跑在 ubuntu-24.04） | 暂不签名 | deb 声明依赖（webkit2gtk / pipewire / speechd 等） |

> **CI 用 ubuntu-24.04**（2026-05-23）：22.04 runner 上 bundle 步一直失败，但读不到 Actions
> 日志、也无 docker 复现 22.04，定位不到 22.04 专属差异。本地开发机是 24.04，完整 `tauri build`
> 能稳定出 **deb + AppImage**（xdg-utils + `APPIMAGE_EXTRACT_AND_RUN=1`，无需 libfuse2）。
> 故 CI 直接用已验证的 24.04 环境。代价：deb/AppImage 需较新 glibc（24.04+），老发行版兼容性
> 让步换取"能出包"。若日后要兼容老系统，再回头查 22.04 的具体报错。

- `tauri.conf.json` 的 `bundle.targets` 由 `"dmg"` 改为按平台。Tauri 会按当前 OS 只产对应包。
- Cargo.toml 加 `[target.'cfg(windows)'.dependencies]`（windows-rs）和
  `[target.'cfg(target_os="linux")'.dependencies]`（x11 / atspi 等）。
- **CI matrix**（`.github/workflows/build.yml`）：`macos-latest` / `windows-latest` / `ubuntu-22.04`
  三路构建，Linux 装 webkit2gtk 等 apt 依赖。CI 是 Win/mac 的编译验证手段。

## 4. UX 决策（已拍板 2026-05-22）

用户已就以下 4 项做出决策（驱动后续实现）：

1. **Wayland 下 Mode B** → ✅ **降级为「已复制到剪贴板，请 Ctrl+V」+ 气泡提示**。
   实现：注入层提供 `can_inject()`；Wayland 返回 false → 走剪贴板 + 提示路径。
2. **听写触发手势** → ✅ **两者都给，用户选**。Win/Linux 默认普通全局快捷键
   （走 `tauri-plugin-global-shortcut`，可发现、稳）；Win 额外提供「长按修饰键」选项
   （`SetWindowsHookEx` 钩子）。Wayland 仅普通快捷键（或 portal GlobalShortcuts）。
3. **onboarding** → ✅ **精简版**：Win/Linux 跳过权限三连页，保留后端选择 + 快捷键设置。
   Wayland 首次截图时就地处理 portal 授权。
4. **Wayland 陪伴动效** → ✅ **自动降级关闭**：检测到 Wayland → 桌宠固定锚点、不跟随、
   无轨迹/渐睡（这些依赖全局光标，Wayland 拿不到）。X11/Win/mac 完整。

→ 凡涉及新 UI/文案的（如精简 onboarding 的平台说明），先按 CLAUDE.md 出 prototype HTML。

## 5. 任务清单（全集 · 实时打勾）

> 验证图例：✅ Linux 本地编译验证 + CI · 🟡 仅 CI 编译（Win/mac）· ⚠️ 运行时未验证（待真机）

**A. 基础设施** — ✅ 全部完成（commit `build:` + 后续）
- [x] `tauri.conf.json` bundle.targets 按平台 + asset scope（$TEMP/$HOME）+ deb depends
- [x] Cargo.toml Win/Linux target-gated deps；`security-framework`→macOS；`libc`→unix；
      新增 keyring/arboard/xcap/enigo/tts/x11rb/windows
- [x] `.github/workflows/build.yml` 三平台 matrix（含 pipewire/drm/gbm/speechd 系统依赖）
- [x] 跨平台发布版文件日志（unix dup2 覆盖 mac+Linux）

**B. 编译级移植** — ✅ 完成（Linux `cargo check --all-targets` green）
- [x] `clipboard_crypto.rs` 密钥存储拆 `#[cfg]`：mac=Keychain，Win/Linux=keyring(+0600 文件兜底)
- [x] `platform.rs` 抽象薄层：`is_wayland/can_inject/global_cursor/frontmost_app/type_text/
      delete_chars/paste_via_ctrl_v/set_clipboard_text/speak`，Win+Linux 实现，Wayland 降级
- [x] 把 no-op 兜底替换为真实现（screenshot/mode_b/clipboard/selection/overlay/companion/pet_passthrough）

**C. 功能原生实现** — ⚠️ 已实现，运行时待真机验证
- [x] 截图：`xcap`（光标所在屏 / Wayland 主屏 portal）⚠️
- [x] 剪贴板读：`arboard`（thread-local 实例 + 文本哈希探针）⚠️
- [x] 选词：X11 PRIMARY via `arboard`（Wayland 自禁用，Win 无 primary→no-op）⚠️
- [x] 文本注入 / Mode B：`enigo`（Win/X11）；Wayland → 剪贴板+「Ctrl+V」提示 ⚠️
- [x] TTS：`tts` crate（SAPI / speech-dispatcher）⚠️
- [x] 前台 app：Win 进程 exe / X11 WM_CLASS+_NET_WM_NAME（Wayland → 空）⚠️
- [x] 点击穿透 + 跟随：`platform::global_cursor`（Win/X11；Wayland→None 自动降级）⚠️
- [x] 终端写入红线：非 mac 用 exe/WM_CLASS 匹配终端列表 ⚠️

**D. 收尾批次** — ⚠️ 已实现，运行时待真机验证
- [x] **听写监听流（Win/Linux）** —— 默认全局快捷键 `Control+Alt+KeyI`：press 复用
      `pipeline::on_shortcut_press`，release 走 `on_dictation_release`（finalize+键入，不进 AI）⚠️
- [x] **Windows 长按右 Ctrl 听写**（§4 可选项）—— `WH_KEYBOARD_LL` 钩子线程 + watcher。
      🟡 仅 CI 编译验证（Windows-only，本地编不了）+ ⚠️ 运行时未验证，**这块最可能要真机迭代**
- [x] 闲置检测 → 渐睡：`platform::idle_seconds`（Win `GetLastInputInfo` / X11 screensaver）⚠️
- [x] 剪贴板敏感 app 排除：非 mac 密码管理器 + 终端列表（exe/WM_CLASS 子串）⚠️
- [x] 拖文件投喂：非 mac Tauri `WindowEvent::DragDrop`（拖到桌宠窗口）⚠️

**E. 收尾（全部完成）**
- [x] **精简 onboarding 前端** —— 非 mac 用 `navigator.userAgent` 判平台，跳过「语音触发键」(step4)
      和「权限三连」(step6)，7 步→5 步。按钮文案随平台切换；新增 i18n key `next_capabilities`。
      prototype：`docs/prototypes/slim-onboarding-cross-platform-20260522.html`。
      🟡 前端本地无法 tsc（容器 npm 镜像 403，无 node_modules）→ 由 CI `bun run build` 验证 ⚠️
- [x] **Win 剪贴板敏感格式抑制** —— `platform::clipboard_excluded()` 检查 Windows 注册格式
      `ExcludeClipboardContentFromMonitorProcessing`（密码管理器约定）→ `is_transient` 跳过记录。
      🟡 Windows-only，CI 编译验证 ⚠️。Linux app 名单兜底。

> 至此 §5.A–E 全部落地。唯一未做的是"非 Exclude 标志的更细粒度 Win 剪贴板历史控制"
> 与"Linux 各桌面的敏感标志统一"——属边角，且无统一标准，留观察。

## 6. 取舍记录
- **优先用成熟跨平台 crate（xcap/arboard/enigo/tts/keyring）** 而非手写每个平台的 FFI——
  减少不可验证的盲写面，把风险集中在「crate 在该平台行为是否符合预期」这一层，靠真机验证。
- **不重写 macOS 路径**：mac 已验证可用，移植只新增 `#[cfg(not(macos))]` 分支，降低回归风险。
- **Wayland 是头号风险**：注入 / 全局光标 / 全局热键 / 前台窗口四样都受合成器安全模型限制，
  注定部分降级。X11 体验完整，Wayland「能聊天 + 截图 + 复制路径 reactive + TTS」保底。
