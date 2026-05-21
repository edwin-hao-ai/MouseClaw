# MouseClaw 本机调试小抄

## 启动 / 停止

```bash
# 开发模式（带 hot reload）— 修改 React 自动刷新，改 Rust 自动重编
cd /Users/edwinhao/MouseClaw && bun tauri dev

# 强制退出（无 dock 图标所以没 ⌘Q）
killall mouseclaw
# 或在跑 bun tauri dev 的终端按 Ctrl+C
```

## 关键日志位置

| 日志 | 路径 / 命令 |
|---|---|
| dev 终端实时输出 | 跑 `bun tauri dev` 的终端 |
| Session JSONL | `tail -f ~/.mouseclaw/sessions.jsonl` |
| 配置（快捷键等） | `cat ~/.mouseclaw/config.json` |
| 临时截图 | `ls -lt /tmp/mouseclaw-frame-*.png \| head` |
| sherpa ASR 模型 | `~/.mouseclaw/models/sherpa-zh-en/` (~189MB · encoder/decoder/joiner.onnx + tokens.txt) |

## Webview DevTools（看 React 状态/网络/console）

dev 模式下右键老鼠窗口（如果窗口对得到鼠标） → "Inspect Element"
**或者**：

```bash
# 强制 webview 开 DevTools — 已经默认开启 in tauri dev
# 如果 DevTools 没出来，确认 tauri.conf.json 没 disable
```

## DEV 状态预览（不真触发 pipeline）

老鼠窗口聚焦后按 **数字键 1-9** 切换所有 UI 状态：

| 键 | 状态 |
|---|---|
| `1` | idle（睡觉） |
| `2` | onboarding |
| `3` | listening（录音中） |
| `4` | thinking（带 transcript） |
| `5` | reply 长内容（带 ▼ 展开） |
| `6` | reply 短内容（"✅ 已发送邮件"） |
| `7` | panel 展开（带 4 turns 对话历史 + 链条图标） |
| `8` | Mode B 倒数 |
| `9` | blocked |

## 重置状态

```bash
# 重置 onboarding（让首启 onboarding 再走一次）
# 在 webview DevTools console:
localStorage.removeItem('mouseclaw.onboarded'); location.reload();

# 重置所有 session 历史
rm ~/.mouseclaw/sessions.jsonl

# 重置快捷键配置
rm ~/.mouseclaw/config.json   # 下次启动默认 Cmd+Shift+Space
```

## macOS 权限对话框（首次启动会逐个弹）

按 Cmd+Shift+Space 第一次时，三个权限会逐个请求。如果你**误点拒绝**：

| 权限 | 用途 | 重新打开 |
|---|---|---|
| 麦克风 | cpal 录音给 sherpa ASR | 系统设置 → 隐私 → 麦克风 → 找 mouseclaw 打勾 |
| 屏幕录制 | `screencapture` 抓主屏 | 系统设置 → 隐私 → 屏幕录制 → 找 mouseclaw 打勾 |
| 辅助功能 / Apple Events | osascript Cmd+V 写回光标 | 系统设置 → 隐私 → 辅助功能 → 找 mouseclaw 打勾 |

撤销/重授后必须**重启 app**（killall mouseclaw + 重新 bun tauri dev）。

## 常见问题速查

### 老鼠按了快捷键不出现
1. 终端有 `🦞 shortcut fired` 日志吗？
   - **没有** → 快捷键被别的 app 占了。换 `~/.mouseclaw/config.json` 里的 shortcut 字段或删了重启。
   - **有但窗口不出** → `window.show()` 失败。检查 NSScreen / NSEvent.mouseLocation 权限。

### 录音没反应 / 转写空
1. 终端有 `captured N samples @ 16kHz (Ys)` 吗？
   - **N=0** 或 `recording was empty` → 麦克风权限被拒，或者 cpal 选错设备。
   - **N>0 但 transcript 空 / 乱** → sherpa 在静音/噪声上识别不出，说明你没说话或者声音太小。

### Mode B 没写到光标
1. 检查 `frontmost_app_name` 日志，是不是终端类（被黑名单了）
2. AppleEvents 权限有没有给（系统设置 → 隐私 → 自动化）
3. osascript 报错可以手动测：
   ```bash
   osascript -e 'tell application "System Events" to keystroke "v" using {command down}'
   ```

### Claude 调用慢/失败
1. `claude --version` 确认 v2.1.138+
2. 直接命令行测：
   ```bash
   claude -p "Hello" --allowedTools "Read" --output-format text
   ```
3. 网络 / API quota 问题

### 改了 Rust 没生效
- `bun tauri dev` 会自动重编。如果没动，Ctrl+C 重启。
- 大改 Cargo.toml 后第一次会很慢（sherpa-onnx 全量重编 ~2 分钟）。

### 改了 React 没生效
- Vite hot reload 应该秒级。如果没动，刷新 webview（DevTools → 右键 → Reload）

## 释义日志 prefix

| 前缀 | 来源 |
|---|---|
| `[mouseclaw]` | 我们自己的 Rust 打印 |
| `🎤 sherpa ...` | sherpa-onnx ASR 加载 / resampler 日志 |
| `cargo:warning=` | 构建期 |
| `error[E...]` | Rust 编译错（红） |

## 调试 pipeline（不依赖 UI）

```bash
cd /Users/edwinhao/MouseClaw/src-tauri
cargo run --example smoke_pipeline  # 截屏 + Claude
```

## 调试坐标 / 屏幕

```bash
# 看当前鼠标坐标
osascript -e 'tell application "System Events" to return position of (the front-most window of the front-most process)'

# 看 NSEvent.mouseLocation（bottom-left origin）
# 不容易直接测，看 lib.rs::current_mouse_pos_top_left 的日志
```

## 性能 / 内存

```bash
# 看 MouseClaw 进程
ps aux | grep -i mouseclaw

# 看 Tauri webview 进程组
ps aux | grep -i "MouseClaw\|webview"

# 看 sherpa ASR 模型加载后的内存峰值
top -pid $(pgrep mouseclaw | head -1)
```
