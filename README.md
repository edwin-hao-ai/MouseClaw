# MouseClaw 🦞

> 桌面上的像素小老鼠，按快捷键召唤，截图 + 语音问 AI。回答完自己跑回角落。

平时它**不存在**——没有 dock 图标、没有 menubar 项、没有窗口。按下 `Cmd+Shift+Space`，一只 16×16 的像素老鼠出现在你鼠标位置附近，听你说话，把截屏 + 转写一起发给 Claude Code CLI，把回答显示在气泡里。3 秒后老鼠跑回角落，世界恢复安静。

```
按快捷键 → 老鼠出现 + 听 → 说话 → 再按一次 → Whisper 转写 → Claude → 气泡 → 3 秒后消失
```

## 特点

- **纯本地语音**：cpal 录音 + whisper.cpp（中英混合），音频不上云
- **截屏一起发**：用户视野的整张主屏，AI 看得见你在看啥
- **两种输出模式**：
  - A（默认）：Claude Code 内部 agentic 干活（改文件、跑命令…），气泡显示结果
  - B（"续写"等）：3 秒倒数 + CGEvent unicode 键盘事件直接写入光标位置（不走剪贴板、不被 IME 干扰）
- **真后台进程**：`LSUIElement=true` + `setActivationPolicy(.accessory)` → 完全不打扰
- **5 分钟会话窗口**：连续按快捷键续 session，过 5 分钟自动新 session
- **menubar 托盘图标**：召唤老鼠 / 查看历史记录 / 关于 / 退出
- **历史记录**：自维护 `~/.mouseclaw/sessions.jsonl`，每个 user turn 关联截图缩略图

## 技术栈

| 层 | 技术 |
|---|---|
| GUI shell | Tauri 2 + Vite + React + TypeScript |
| 像素艺术 | 手撸 16×16 SVG `<rect>`s，`shape-rendering: crispEdges` |
| 录音 | cpal 0.15（专用音频线程 + mpsc channel，因为 `cpal::Stream` 不是 `Send`） |
| 转写 | whisper-rs 0.14（whisper.cpp Rust binding） + `ggml-base-q5_1.bin`（57MB） |
| AI 后端 | Claude Code CLI v2.1.138+ via `claude -p --allowedTools "Read" ...` |
| 截屏 | macOS `screencapture -x -o /tmp/...png` |
| 写回光标 | macOS `CGEventKeyboardSetUnicodeString` |
| Session 存储 | `~/.mouseclaw/sessions.jsonl`（每行一个 turn 记录） |

## 开发

### 依赖

- Rust 1.95+
- Bun 1.3+（或 npm/pnpm/yarn）
- macOS 11+
- Xcode Command Line Tools
- CMake（whisper.cpp 编译需要）：`brew install cmake`
- Claude Code CLI：`npm install -g @anthropic-ai/claude-code` 或参考官方安装文档

### 跑起来

```bash
# 装前端依赖
bun install

# 下载 Whisper 模型（~57MB，首次运行需要）
mkdir -p ~/.mouseclaw/models
curl -L -o ~/.mouseclaw/models/ggml-base-q5_1.bin \
  'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin'

# 开发模式（带 hot reload）
bun tauri dev

# 打包 dmg（未签名）
bun tauri build --bundles dmg
# 产物在 src-tauri/target/release/bundle/dmg/
```

### 首次运行会要求 3 个权限

| 权限 | 用途 |
|---|---|
| 屏幕录制 | `screencapture` 抓主屏发给 AI |
| 麦克风 | cpal 录音给 Whisper |
| 辅助功能 / 输入监控 | CGEvent 写入光标（Mode B） |

撤销/重授后必须重启 app。

## 项目结构

```
MouseClaw/
├── CLAUDE.md                  # AI 协作约束（架构 + Claude CLI 调用模板 + Mode B 红线）
├── DESIGN.md                  # 设计系统单一信源（color/typography/spacing/motion tokens）
├── DEBUG.md                   # 本机调试小抄（日志位置、状态重置、常见问题）
├── src/                       # React 前端
│   ├── App.tsx                # 状态机：driven by ViewKind events from Rust
│   ├── HistoryView.tsx        # menubar → 查看历史记录
│   ├── AboutView.tsx          # menubar → 关于
│   ├── components/            # PixelMouse / Bubble / Panel / Onboarding / RecordingBubble
│   └── styles/tokens.css      # DESIGN.md §9 落地到 CSS 变量
└── src-tauri/                 # Rust 后端
    ├── src/
    │   ├── lib.rs             # Tauri 入口 + 全局快捷键 + pipeline 编排
    │   ├── audio.rs           # cpal 录音 worker（专用线程）
    │   ├── transcribe.rs      # whisper-rs 包装
    │   ├── claude_cli.rs      # Claude Code CLI 调用 + INSERT_AT_CURSOR 标记解析
    │   ├── screenshot.rs      # screencapture wrapper
    │   ├── sessions.rs        # ~/.mouseclaw/sessions.jsonl 管理
    │   ├── mode_b.rs          # CGEvent 直接键盘事件写入光标
    │   ├── tray.rs            # menubar 托盘图标 + 召唤/历史/退出
    │   ├── config.rs          # ~/.mouseclaw/config.json
    │   └── events.rs          # IPC ViewKind 枚举（与 src/types.ts 对应）
    ├── examples/              # 独立 smoke test：whisper / claude pipeline
    └── Info.plist             # LSUIElement + 3 个 UsageDescription
```

## 设计哲学

读 [`DESIGN.md`](./DESIGN.md) — 视觉语言的单一权威：色板、字体、间距、动画 token 一应俱全。
读 [`CLAUDE.md`](./CLAUDE.md) — AI 协作的硬规则：UI 改动必须先 HTML prototype、self-contained 完成立即 commit。

核心理念：**Quiet by default**。99% 时间老鼠不存在，触发后老鼠跑出来又跑回去。每个可见像素都得挣它出场的资格。

## License

MIT — see [LICENSE](./LICENSE).
