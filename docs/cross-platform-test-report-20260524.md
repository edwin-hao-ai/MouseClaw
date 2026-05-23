# 跨平台移植（PR #6）审计 + 测试报告 · 2026-05-24

> 自主夜间任务：拉取最新 main（含 PR #6 Windows+Linux 移植）→ 审计新功能 → 尽可能完整测试，
> 重点尝试「在 Mac 上测 Windows / Linux app」。本报告是结论 + 给真机测试的交接清单。

## TL;DR

- ✅ **macOS 没被弄坏**：cargo check + 211 单测 + tsc 全绿；Keychain 密钥语义保留；voice_ime
  CGEventTap 路径原样保留。**已发布的 v0.4.7（macOS）不受影响**（且 0.4.7 二进制本就在 PR #6 之前）。
- ✅ **三平台编译+打包全过（GitHub Actions CI）**：当前 main 的 `build` workflow 绿——
  Windows 出 `.exe`、Linux 出 `.deb`+`.AppImage`、macOS cargo build 通过。**真实安装器已产出**。
- ✅ **代码审计干净**：`platform.rs` / `clipboard_crypto.rs` / `voice_ime.rs` / 设计文档都扎实，
  cfg 分发清晰、Wayland 诚实降级、macOS 路径不动。
- ⚠️ **Win/Linux 运行时行为未验证**：设计文档自己就写了「CI 绿 ≠ 功能可用，运行时只能真机」。
  我尝试在 Mac 上用 Docker+Xvfb 跑 Linux app，**被本机网络代理挡住**（apt 下载 502，见下）。
  Windows 在 Mac 上**无法运行**（无 VM）。→ 需真机测，清单见末尾。

## 1. 做了什么

| 步骤 | 结果 |
|---|---|
| `git pull --rebase`（含 PR #6 + PR #5 + 我的 v0.4.7）| 已是最新，无冲突 |
| 审计 `platform.rs`（381 行新抽象）| 干净：is_wayland/can_inject/global_cursor/frontmost/idle/type_text(enigo)/clipboard/speak(tts)，Win 用 windows-rs、Linux 用 x11rb（连接缓存+重连），Wayland 降级 |
| 审计 `clipboard_crypto.rs`（密钥存储 cfg 拆分）| macOS 仍走 security-framework + 同 service/account 常量 → **现有用户 Keychain 不迁移不破坏**；Win/Linux 用 keyring + 0600 文件兜底 |
| 审计 `voice_ime.rs`（+159，最大改动）| macOS CGEventTap 原样保留（file-cfg → per-item cfg）；新增 Windows 键盘钩子长按；Linux 长按听写**未实现**（设计文档 §4 标注待 UX 决策）|
| macOS 回归：`cargo check` + `cargo test --lib` + `tsc` | **全绿**（211 测试，0 失败）|
| GitHub Actions CI（最新 main run 26339755290）| **3 平台全 success**：macos-latest✓ / windows-latest✓ / ubuntu-24.04✓ |
| 下载 CI 产物到 `/tmp/mc-artifacts/` | Win `.exe`(8.4M) · Linux `.deb`(20M) + `.AppImage`(89M) —— 给真机测用 |

## 2. 「在 Mac 上测 Windows / Linux」可行性结论

### Linux —— 方法正确但被网络环境挡住
- **方法**：Docker Desktop（已装，arm64）跑 `--platform linux/amd64` 模拟 x86_64 容器 +
  装 webkit2gtk/gtk/x11 等运行时库 + `xvfb-run` 虚拟显示，跑 CI 的 `.AppImage`，看 Rust 是否 panic。
- **结果**：**被本机网络代理挡住** —— `apt-get` 下载 .deb 全部 `502 Bad Gateway [IP: 198.18.3.31]`。
  198.18.x 是代理软件（Clash/Surge 之类）的 fake-IP 段；Docker 流量被路由过去且 ubuntu 源被拦。
  换 aliyun 镜像后 `apt update` 通了，但 `.deb` 下载仍失败。**这是网络环境问题，非 MouseClaw 代码问题。**
- **可行的修法**（你方便时）：① 关掉/调整代理对 Docker 子网的拦截；② 或用一个已预装 webkit2gtk
  运行时的 Docker 镜像（避开 apt）；③ 或直接在真 Linux 桌面跑 `.AppImage`（最可靠）。
- 即便跑起来，**无头 webkit GUI（无 GPU/真显示）信号有限** —— 真行为（托盘/穿透/X11 光标/注入）
  仍需真 Linux 桌面。**CI 的真 ubuntu-24.04 编译+打包已是 Mac 上能拿到的最强 Linux 验证。**

### Windows —— Mac 上无法运行
- 本机无桌面 VM（无 Parallels/UTM/VMware）；Tauri 用 WebView2，Wine 跑不可靠。
- → **Windows 运行时只能在真 Windows 机测**（或装 Parallels/UTM Windows ARM VM）。CI 已验证编译+打包出 `.exe`。

## 3. 已知缺口（设计文档已列，非 bug）

- **Linux 长按听写（fn）未实现** —— Win 用键盘钩子做了，Linux/Wayland 待 §4 UX 决策（普通全局快捷键替代）。
- **Wayland 降级**：注入键盘 / 全局光标 / 前台窗口三样在 Wayland 拿不到 → Mode B 退化为「复制到剪贴板+提示」、
  陪伴动效（眼球追光标）关闭、session 边界只靠时间。这些是 Wayland 安全模型限制，设计文档已标 🔴 + 需用户拍板。
- **截图**：Wayland 首次走 portal 可能弹授权框（onboarding 需解释）。

## 4. 真机测试清单（交接）

安装器在 `/tmp/mc-artifacts/`（也可从 GitHub Actions 最新 run 的 artifacts 下）。

### Windows（真机 / VM）
1. 装 `MouseClaw_0.4.7_x64-setup.exe` → 启动，托盘出现、桌宠 overlay 显示
2. 全局快捷键召唤 → 截屏（当前屏）+ 录音 + 调 AI → 回答
3. Mode B 写回光标（记事本/Word）；前台是终端时强制 A 模式
4. 剪贴板监控 + 密码管理器内容是否被正确排除（Exclude 格式）
5. TTS 朗读（SAPI5）；托盘/穿透/前台 app 名（session 边界）

### Linux X11（真机）
1. `sudo dpkg -i MouseClaw_0.4.7_amd64.deb` 或直接跑 `.AppImage`
2. 同上召唤/截屏/Mode B/剪贴板/选词（X11 PRIMARY selection）/TTS（需 speech-dispatcher）
3. 桌宠眼球追光标（X11 XQueryPointer）、托盘（appindicator）

### Linux Wayland（真机）
1. 同上，但预期：Mode B 降级为「复制+提示 Ctrl+V」、眼球追踪关闭、首次截图弹 portal 授权
2. 确认降级有清晰提示、不崩

## 5. 给下个版本的建议

- PR #6 现在在 main 上、**不在已发布的 0.4.7 里**。要发含 Win/Linux 的版本时，需先在真 Win/Linux
  机过一遍上面清单，再 bump 版本 + 发 release（CI 产物可直接用）。
- Linux 长按听写 / Wayland Mode B 默认行为这两个 UX 决策（设计文档 §4）需要你拍板后再接。
