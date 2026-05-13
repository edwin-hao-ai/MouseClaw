# MouseClaw 项目规则

## UI/UX 工作流：先 HTML prototype，再写代码

**硬规则**：任何涉及视觉/交互的功能，**先用单文件 HTML prototype 让用户看到效果**，用户拍板之后才进入 Rust/Tauri 实现。

### 适用范围
- 像素老鼠的任何新动画状态
- 气泡 UI 的样式、字体、动效
- Panel 展开形态、对话历史、follow-up 输入框
- Onboarding / 任何弹窗
- 错误状态、空状态、流式 loading 效果
- Session chip 视觉提示
- Mode B 倒数 UI

### Prototype 规范
- **单文件 HTML**（self-contained，无外部 CDN 依赖），方便 `open` 直接看
- 放在 `docs/prototypes/<feature>-YYYYMMDD.html`
- 用 inline SVG 画像素艺术（`shape-rendering: crispEdges`）或 CSS 网格——**不要用 emoji 占位**
- 用 CSS `@keyframes` 把动画真做出来（不是静态截图），用户要看到"它动起来什么感觉"
- 同时展示所有相关状态（睡/听/想/跳 同屏摆开），方便比较
- 背景模拟真实桌面场景（mock 一个浏览器/IDE/终端窗口在底下），让用户看到老鼠**叠加**之后的效果，不是孤零零的角色卡

### 反例 ❌
- "我先把 Rust 端写好再调 UI"——禁止
- "我口述一下样式，你想象一下"——禁止
- "我用 markdown 画 ASCII art"——禁止

### 评审流程
1. 写完 HTML，`open docs/prototypes/<feature>.html` 让用户本地看
2. 用户给反馈（哪里不对、动画太快/慢、字体不对、配色想换）
3. 改 HTML，再 open，循环到用户说"对了，照这个做"
4. 然后才开始写 Tauri/Rust 实现，**实现必须和 prototype 视觉一致**——不一致是 bug

---

## 技术选型（已锁定）

- **GUI**：Tauri 2（理由：Webview 写"漂亮+流式文本"几乎零成本，纯 Rust GUI 在文本布局上是地狱）
- **录音 → 转写**：cpal 录音 + whisper-rs（whisper.cpp Rust binding）+ Whisper base 量化模型
- **截屏**：macOS 用 `CGWindowListCreateImage` 取当前活动窗口完整截图（不是 300×300 局部）
- **AI 后端**：V1 写死 Claude Code CLI（v2.1.138+）。后端抽象层留接口但**不暴露给用户**
- **平台**：macOS 优先，Windows V2

### Claude CLI 调用约定（2026-05-13 验证 ✅）

Risk #1 已解除：Claude Code CLI 能通过 `Read` tool 读取本地图片。调用模式：

```bash
claude -p "{user_voice_transcript}\n\n截图位置：/tmp/mouseclaw-frame.png\n上下文窗口：{frontmost_app_title}" \
  --allowedTools "Read,Write,Edit,Bash,Grep,Glob" \
  --append-system-prompt "你是 MouseClaw 桌面助手。用户通过语音 + 截图向你提问。
如果用户**明确要求**把内容写入当前光标位置（'续写'、'补全这里'、'写一段在这里'），
用以下标记包裹要写入的纯文本（除标记内文本外不要其他内容）：
[INSERT_AT_CURSOR]
要写入的内容
[/INSERT_AT_CURSOR]
否则正常回答即可。" \
  --output-format stream-json \
  --include-partial-messages \
  --permission-mode auto
```

**关键参数说明：**
- `--allowedTools "Read,..."`：必须显式允许 Read，否则会卡在权限提示
- `--append-system-prompt`：用 append 而不是 `--system-prompt`，保留 Claude Code 默认的 agentic 能力
- `--output-format stream-json --include-partial-messages`：流式给气泡用，每 token 一个事件
- `--permission-mode auto`：daemon 模式下不能交互，让 CLI 自动决定（非 dangerously-skip-permissions，更安全）
- **不**用 `--continue`/`--resume`：MouseClaw 自管 history（拼到 prompt 里），更可控

**Session 上下文拼接策略：**
- 同 session 续聊时把最近 ≤10 轮拼到 prompt 前面，格式 `[历史]\n用户: ...\n助手: ...\n[/历史]\n\n新问题: ...`
- 超过 8K token 时丢最早的轮次

## App 形态（重要约束）

MouseClaw 是**纯后台进程**，平时完全"不存在"：
- macOS `LSUIElement=true`（Info.plist）→ 无 dock 图标
- 无 app 主窗口、无 menubar 项（V1 极简策略；V2 可选加 menubar 图标做退出/设置）
- 不出现在 Cmd+Tab 列表（`NSWindowCollectionBehaviorIgnoresCycle` + transient panel）
- 唯一可见物 = 像素老鼠的透明 always-on-top 小窗口（Tauri 窗口配 `decorations:false, transparent:true, alwaysOnTop:true, skipTaskbar:true, focus:false`，macOS 端需要 NSPanel 行为）
- 启动 = 注册全局快捷键 + 创建隐藏 mouse 窗口；退出 = 杀进程（V1 用 `killall mouseclaw`，V2 加 menubar 退出）

## 输出模式（两种，默认 A）

| 模式 | 触发 | 行为 |
|---|---|---|
| **A · 思考+执行**（默认 99% 情况） | Claude 回复里**不含**特殊标记 | Claude Code CLI 内部走 agentic flow（读文件、改代码、跑命令），气泡只显示结果摘要 |
| **B · 写回光标**（特殊） | Claude 回复里包含 `[INSERT_AT_CURSOR]...[/INSERT_AT_CURSOR]` | 老鼠头顶气泡显示要写的内容 + 3 秒倒数进度条 → 不按 Esc 就自动用 CGEventCreateKeyboardEvent 模拟键盘输入到前台 app 的光标位置 |

### B 模式的工程要点
- 发给 Claude 的 system prompt 必须加 INSERT 标记规则（见上面 Claude CLI 调用约定）
- MouseClaw 解析时只取**第一个** `[INSERT_AT_CURSOR]` 块（防止 Claude 写多块文本到光标）
- 倒数期间按 **Esc** = 取消（不写入），按 **Enter** = 立即写入（跳过倒数）
- V1 用剪贴板 + Cmd+V + 保存/恢复原剪贴板（妥协，污染剪贴板的窗口期 < 100ms），V2 改 CGEventCreateKeyboardEvent 直接键入
- B 模式触发时，老鼠状态切换为新的 "ready-to-write" 动画（眼睛盯着鼠标位置，前爪上举做笔状）

### B 模式的安全红线
- **永远不要在没有 confirm UI 的情况下写入**（V1 3 秒倒数 = 最低 UX 标准）
- **永远不要写入终端**（前台 app 是 Terminal/iTerm/Warp 时，强制走 A 模式不管 Claude 回复有没有 INSERT 标记）—— 终端的"光标位置"语义和写代码命令冲突，会造成意外执行
- 倒数期间用户做任何键鼠操作都视为取消

## Panel 展开 + Session 管理（v0.1.4）

### 长内容展开 Panel
- Claude 回复 ≥ 4 行 → 气泡显示 3 行预览 + "▼ 展开看完整回答"（也支持按 ↓ 键展开）
- Panel 尺寸 400×480px，半透明白色玻璃风格，跟随老鼠位置浮动（不是新窗口，仍 always-on-top）
- Panel 内部：顶部 session chip + 滚动对话历史 + 底部 follow-up 单行输入框
- **Follow-up 不需要再按快捷键**：直接在输入框打字 + Enter
- 按 Esc 或点击 panel 外 = 折叠回气泡；3 秒后老鼠跑回角落
- 短回答（< 4 行）不显示展开按钮，保持轻量"3 秒消失"

### Session 管理
- MouseClaw 自己维护 `~/.mouseclaw/sessions.jsonl`（每行一个 turn：时间戳 + user input + Claude reply + session_id）
- **不**用 `claude --resume`/`--continue`，自己控制上下文拼接
- 每次调用 Claude CLI 时附最近 N 轮（默认 10 轮或 8K token）作为上下文

### Session 边界规则
| 触发 | 行为 |
|---|---|
| 上次回复 < 5 分钟 | 同 session，附上下文 |
| 上次回复 ≥ 5 分钟 | 自动新 session |
| 用户说"新对话/clear/重新开始" | 立即新 session |
| **双击触发快捷键** | 强制新 session（手动 reset） |
| 前台 app 切换且不同源 | 自动新 session |

### Session 视觉
- 续 session 时老鼠头顶有绿色 🔗 链条像素图标
- 新 session 时无标记，"fresh"出场
- Panel 顶部显示 "🔗 续 Session #42 · 第 N 轮"

## 体积/性能目标（v0.1.4 修订 — 漂亮 > 轻量）

- 安装包 < 30MB（不含 Whisper 模型）
- 常驻内存 < 600MB（含 Whisper base 量化模型 ~142MB） / < 400MB（待机不含模型）
- 待机 CPU < 1%
- 触发延迟（按键 → 老鼠出现）< 100ms

## 不要做的事

- ❌ 不要在 Onboarding 让用户选 AI 后端（V1 只有 Claude Code CLI，不是选项）
- ❌ 不要让老鼠跟随鼠标（V1 停在触发位置即可，跟随是 V2）
- ❌ 不要做图形设置界面（除了一步 Onboarding）
- ❌ 不要做自动填表（**Mode C，V2 独立项目**，安全模型完全不同）
- ❌ 不要在没有 prototype 之前写任何 UI 相关的 Rust/TS 代码
- ❌ 不要超过 5 天 ship V1（4-5 天预算已经把 long-form panel + session 算进去了）

## Git 工作流（每个 self-contained 改动必须立即 commit）

继承 global rule（`~/.claude/rules/common/git-workflow.md`）的"完成任何 self-contained 功能立即 commit"硬规则。

**MouseClaw 项目特定补充：**
- **每完成 docs/prototypes/ 下任何 HTML 修改**，立即 `git add docs/ && git commit -m "design: ..."` —— 防止脚手架工具用 `-rm`/`-f` 把未提交的设计文件清掉（**2026-05-13 已经吃过一次亏**）
- **每完成 CLAUDE.md / 设计文档更新**，立即 commit
- 写代码前先 commit 当前文档状态作为基线

## 引用全局规则

继承 `~/.claude/rules/common/` 下所有通用规则（coding-style, git-workflow, testing, security 等）。
本文件只覆盖 MouseClaw 项目特定约束。
