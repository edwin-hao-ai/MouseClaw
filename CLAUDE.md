---
mddock_imported_at: 2026-05-14T13:21:17Z
mddock_import_reason: CLAUDE.md convention
type: agent-config
created_by: agent
---

# MouseClaw 项目规则

## 头号硬规则：动手前先深度思考 UX（不要无脑做）

**最高优先级规则**（用户语 2026-05-18）：每动一行代码前先把这条问透 ——
**「用户拿到这个功能之后，从第一秒到完成目标，全流程他会经历什么？哪一步会困惑、烦躁、放弃？」**

任何功能 / UI 改动开工前的 mandatory checklist（必须**在脑子里跑一遍**，不是事后补；
PR / 提交前 review 也要拿这个对照）：

1. **入口可发现性**：用户怎么知道这个功能存在？看到桌宠状态变化、托盘菜单、键盘 hint、还是只能靠看 README？
2. **首次使用**：第一次用的人 5 秒内能不能搞懂？哪些前置条件（权限、模型、网络）会让首次失败？失败之后怎么自救？
3. **常见操作路径**：每天 / 每小时会做一次的动作，从触发到完成多少步？能不能再砍掉一步？
4. **快路径 vs 慢路径**：好预期下用户用多少秒？坏预期下（卡 / 慢 / 失败）他看到什么 / 能不能取消？
5. **状态可见性**：用户当前在哪个状态？下一步会自动发生什么？还是要他操作？桌宠 / 气泡 / 托盘有没有反映？
6. **取消和后悔**：每个进行中的状态都要能 Esc / 点外面取消。"无法取消"等于"不敢用"。
7. **失败模式**：权限缺、模型没下完、文件太大、AI 调用挂、网络断 —— 每条路径都要给具体的下一步建议，不能甩 stack trace。
8. **可访问性 + 边界**：键盘可达？字号最小是不是看得清？深色模式？中英文混排？多屏幕？前台是终端怎么办？
9. **会不会打扰**：会不会抢焦点？会不会盖住用户正在看的东西？多长时间不操作会自动消失？
10. **第一印象 vs 长期使用**：让人惊艳的一次性动效，跑 50 次之后是不是会烦？反过来，长期好用的功能初见会不会过于朴素 / 看不出价值？

写代码之前应该已经能在脑子里**演**一遍完整的视频：用户在干什么 → 触发 → 桌宠做什么 → 气泡显示什么 →
用户看到什么 / 听到什么 / 接下来怎么操作。**演不出来 = 还没想清楚 = 不要开始写。** 这种时候的正确动作是回到
`docs/prototypes/<feature>-YYYYMMDD.html` 把流程做成可点的 demo（见下一节）。

### 反例（PR 拒绝）
- ❌ "我先把后端写完，UI 改天再说"——后端写法直接影响 UX，没想清 UI 不能动后端
- ❌ "默认参数就行，用户可以自己改"——默认值就是 99% 的用户体验，别推卸
- ❌ "这个边界 case 概率很小不用管"——MouseClaw 是 daily-use 工具，1% × 每天 × N 用户 = 每天都有人踩到
- ❌ 在 commit message / 总结里只写"实现了 X"，没写"用户怎么用 X"

### 工作流锚点
- 任何"是否要做"的决定先用上面 10 问自己评，把答案写进设计文档
- 任何"怎么做"的争议拿 prototype HTML 做 A/B，让用户拍板
- 任何"已完成"的声明前回看："如果是我第一次装上 MouseClaw，跑这条路径我会不会卡住？"

## 设计系统：DESIGN.md 是视觉单一信源

**最硬规则**：[DESIGN.md](./DESIGN.md) 是 MouseClaw 视觉语言的**唯一权威**。任何 UI 工作（webview 组件、prototype HTML、README 截图、营销页面）必须用 DESIGN.md 里的 design tokens，不允许在组件里硬编码颜色/间距/圆角字面量。

### 强制执行
- **写 prototype HTML 前**：先读 DESIGN.md 第 2 节（tokens）和第 4 节（components），选好要复用的 token
- **写 React/Rust UI 代码前**：把 DESIGN.md 第 9 节的 `:root` cheatsheet 粘到全局 CSS，所有值都从变量来
- **新增颜色/间距/动画**：必须**同 PR 内**更新 DESIGN.md 的对应 token 表，否则审查不通过
- **prototype 和 DESIGN.md 冲突**：以 DESIGN.md 为准，改 prototype 不改 DESIGN.md
- **像素老鼠**：8 色调色板锁定，不能加第 9 色；新增动画状态必须在 DESIGN.md 第 3.1 节补完整 anatomy

### 任何视觉变更的 review checklist
- [ ] 所有 color/spacing/radius/shadow 都用 DESIGN.md 里的 token，没有内联字面量
- [ ] 文字大小用 type scale（`--text-body`、`--text-meta` 等），不直接写 px
- [ ] 动画 duration / curve 用 motion 表里的值
- [ ] 新颜色已检验对比度 ≥ 4.5:1（body）/ 3:1（large）
- [ ] 检查 `prefers-reduced-motion` fallback
- [ ] 中英文混排测过（用 "🦞 Hello 你好 ABC 中文 World" 这种 string 试一下）
- [ ] 如果改了 token 或加了新组件，DESIGN.md 同 PR 更新

## UI/UX 工作流：先 HTML prototype，再写代码

**硬规则**：任何涉及视觉/交互的功能，**先用单文件 HTML prototype 让用户看到效果**（prototype 也必须用 DESIGN.md 的 token），用户拍板之后才进入 Rust/Tauri 实现。

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

## i18n：所有用户可见文案必须走翻译表（硬规则 · v0.1.9+）

任何新功能涉及**用户可见的字符串**（标题/按钮/提示/错误/工具提示/菜单文字/对话气泡）必须按以下流程：

### 必做
1. **前端字符串**：先在 `src/i18n/zh.ts` 加 key + 中文 → 同步在 `src/i18n/en.ts` 加同 key 的英文 → 组件用 `useT()` hook 调用 `t("your.key")`
2. **类型校验**：所有 key 必须先在 `src/i18n/types.ts` 的 `Strings` interface 里声明 —— 写错 key TS 编译期就报错（这是设计意图）
3. **Rust 端用户可见字符串**：托盘菜单标签、emit 给前端的 reply/blocked 文案，按 `current_lang` 双语切换（参考 `tray.rs::setup` 里 `s_summon` 等变量的写法）
4. **支持插值**：变量插到字符串里用 `t("key", { name: "Edwin" })`，对应 zh/en 文件写 `"你好 {name}"` / `"Hi {name}"`

### 反例（PR 拒绝）
- ❌ 直接在 JSX 写 `<h1>系统状态</h1>` —— 必须 `<h1>{t("status.title")}</h1>`
- ❌ Rust 里 hardcode `MenuItem::with_id(app, "x", "关于 MouseClaw", ...)` —— 必须按 `current_lang` 分支
- ❌ 加 key 只更新 zh.ts 不更新 en.ts —— TS 编译会报 `Strings` 接口缺字段

### 加新语言（比如未来加日语）
1. 新建 `src/i18n/ja.ts`，导出 `const ja: Strings = { ... }`（所有 key 必须齐全，否则 TS 编译失败）
2. `src/i18n/index.ts` 的 `LANGUAGES` 数组加 `{ id: "ja", label: "日本語", strings: ja }`
3. `LangId` 类型加 `"ja"`
4. Rust `save_language` 的 `allowed` 数组加 `"ja"`
5. 托盘菜单 `tray.rs` 的双语 if-else 改成 match 三分支
6. 完事

### 例外（可不翻译）
- 调试日志 / `println!` / `eprintln!` —— 英文为主，开发者看
- 错误的 stack trace 原文（外部 CLI 抛回来的）—— 用 `friendly_backend_error()` 翻译已知模式即可
- 代码里的 const 名 / 文件名 / git commit message

> 这条规则因为 v0.1.9 用户反馈「想支持中英文 + 以后可能其他语言」而立。每次写新功能就当成本来就是双语项目 —— 后补成本远高于一开始就做。

## 代码组织：单文件 ≤ 800 行（硬规则）

**任何源文件不得超过 800 行**，目标 200–400 行。超了就按职责拆模块。

- Rust：按功能拆 module（`overlay.rs` / `pipeline.rs` / `commands.rs` / `backend.rs` …），
  `lib.rs` 只留 `AppState` + `run()` + 启动钩子
- React：组件单文件，逻辑抽 hook / 子组件
- 检查：`wc -l src-tauri/src/*.rs src/**/*.tsx | sort -rn | head` —— 任何一行超 800 就拆
- **2026-05-15 教训**：lib.rs 一度涨到 792 行，加新功能前必须先拆，不然滚雪球

## 技术选型（已锁定）

- **GUI**：Tauri 2（理由：Webview 写"漂亮+流式文本"几乎零成本，纯 Rust GUI 在文本布局上是地狱）
- **录音 → 转写**：cpal 录音 + whisper-rs（whisper.cpp Rust binding）+ Whisper base 量化模型
- **截屏**：macOS 用 `screencapture` CLI 取光标所在屏完整截图（不是 300×300 局部）
- **AI 后端**：多后端抽象（`backend.rs`）—— Claude Code CLI（默认，最成熟）/
  OpenAI Codex CLI / OpenClaw CLI。用户在 Onboarding 选，存 config.json。
  所有后端走统一契约 `(截图 + prompt) → 流式文本`
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
- 无 app 主窗口
- **有 menubar 托盘图标**（v0.1.4 加入）→ 提供「召唤老鼠 / 查看历史记录 / 关于 / 退出」菜单
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
- 写入用 `CGEventKeyboardSetUnicodeString` 直接发 unicode 键盘事件——不污染剪贴板，**绕过 IME**（中文输入法激活时不会变成 composition），不依赖外部进程
- 长文本分 15 char/chunk 发送，每 chunk 间 5ms 让前台 app input loop 跟上
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

- ⚠️ ~~不要在 Onboarding 让用户选 AI 后端~~ —— v0.1.6 起改为**支持**多后端
  （Claude Code / Codex / OpenClaw CLI），Onboarding 第 2 步选。见 `backend.rs`。
- ❌ 不要让老鼠跟随鼠标（V1 停在触发位置即可，跟随是 V2）
- ❌ 不要做图形设置界面（除了一步 Onboarding）
- ❌ 不要自己实现一套浏览器自动化引擎（**Mode C 独立引擎是 V2**，安全模型完全不同）
  - ✅ 但**允许**：检测到本机装了 `agent-browser` CLI 时，在 system prompt 里告诉
    后端「你的 Bash 工具里有这个 CLI」——compute use 能力随后端 agentic 能力自然获得，
    零新增安全面。见 `claude_cli.rs::BROWSER_CAPABILITY_PROMPT`。不可逆动作要求后端先确认。
- ❌ 不要在没有 prototype 之前写任何 UI 相关的 Rust/TS 代码
- ❌ 不要超过 5 天 ship V1（4-5 天预算已经把 long-form panel + session 算进去了）

## Git 工作流（每个 self-contained 改动必须立即 commit）

继承 global rule（`~/.claude/rules/common/git-workflow.md`）的"完成任何 self-contained 功能立即 commit"硬规则。

**MouseClaw 项目特定补充：**
- **每完成 docs/prototypes/ 下任何 HTML 修改**，立即 `git add docs/ && git commit -m "design: ..."` —— 防止脚手架工具用 `-rm`/`-f` 把未提交的设计文件清掉（**2026-05-13 已经吃过一次亏**）
- **每完成 CLAUDE.md / 设计文档更新**，立即 commit
- 写代码前先 commit 当前文档状态作为基线

## 发布流程 cheat sheet（v0.1.24+ 这一系列踩过的全部坑）

### 🚨 硬规则（v0.3.0 学到的教训）：用户验证之前**不许 gh release / 不许 push tag**

**问题场景**：每次改完代码 → build → 自动 notarize → 自动 push tag → 自动 gh release → 用户装上发现有 bug → 已经发布的版本回收成本极高（GH releases 可以删但 cdn / version.json 拉过的客户端已经看到了），而且 release notes 堆一堆"已修复"显得很乱。

**正确流程**：

```
代码改完 → build DMG (不 push tag, 不 notarize, 不 gh release)
         → open DMG 给用户本地测
         → 用户确认 OK 后
            才允许：notarize + push tag + gh release + update version.json + push main
```

**Claude 默认行为**：写代码 / build / open DMG 一气呵成，**但停在 open 这一步**，
等用户明确说"OK 可以发了" / "release 吧" 之类的指令再走 notarize + push + release。

**用户能容忍的快速循环**：build → open → 测 → 不行就改代码 → rebuild → reopen。
**用户不能容忍的浪费循环**：build → 自动 release → 装上发现 bug → 又一个新 release 修。

### 1. 完整发新版本 step-by-step

```bash
# (0) 决定版本号 —— 大功能 → 小步进 (0.1.25 → 0.1.26)。三个地方都要改：
sed -i '' 's/"version": "0.1.25"/"version": "0.1.26"/' src-tauri/tauri.conf.json package.json
sed -i '' 's/^version = "0.1.25"/version = "0.1.26"/' src-tauri/Cargo.toml

# (1) 更新 CHANGELOG.md 顶部加新 section（双语 Added/Changed/Fixed）

# (2) cargo check + tsc 双跑（cargo 报 E0063 missing field 见 §8 第 1 条）
cargo check --manifest-path src-tauri/Cargo.toml --quiet
bunx tsc --noEmit

# (3) 打包（会触发 beforeBuildCommand：fetch whisper model + vite build）
bun tauri build
# 输出在 ~/.cargo/shared-target/release/bundle/dmg/MouseClaw_X.Y.Z_aarch64.dmg
cp ~/.cargo/shared-target/release/bundle/dmg/MouseClaw_X.Y.Z_aarch64.dmg ./

# (4) 公证 + staple（脚本自动找 keychain profile）
bash scripts/notarize-dmg.sh
# 看到 "✅ All set." + spctl "source=Notarized Developer ID" 才算过

# (5) 更新 docs/version.json（latest / released_at / dmg_url / notes_zh / notes_en）
# !! 这是最常忘的步骤 !! 不改的话现有用户永远收不到新版本提示

# (6) commit + tag + push
git add -A && git commit -m "feat(vX.Y.Z): ..."
git tag vX.Y.Z
git push && git push --tags

# (7) GitHub Release（自动 publish DMG）
gh release create vX.Y.Z \
  --title "vX.Y.Z · 一行总结" \
  --notes-file /tmp/release-vX.Y.Z.md \
  MouseClaw_X.Y.Z_aarch64.dmg

# (8) 如果是改了已有 release 的 DMG / demo mp4
gh release upload vX.Y.Z file1 file2 --clobber
```

### 2. 公证（notarytool）流程

- **Profile 已存在 keychain 里**：`AwarenessClawNotary`（与 Awareness 项目共用同一个 team `5XNDF727Y6`）
- `scripts/notarize-dmg.sh` 自动按 **OCTAgentNotary → AwarenessClawNotary → mouseclaw** 顺序探测可用 profile —— 装 keychain 时取哪个名字都行
- 公证返回 `Accepted` 后必须 `xcrun stapler staple`，否则离线时 Gatekeeper 仍报警
- 验证：`spctl --assess --type install -vv MouseClaw_X.Y.Z_aarch64.dmg` 必须返回 `source=Notarized Developer ID`
- Team ID `5XNDF727Y6`（Beijing VGO Co;Ltd） · apple-id `120298858@qq.com`（公开可见，app-specific password 一次性存 keychain 后不明文出现）
- 公证耗时通常 1-3 分钟，最多 10 分钟。5 分钟没动 = 苹果服务忙，**别 kill 重发**（会被记重复提交）

### 3. GitHub Pages 流程

- **Pages source 必须人工**在 repo Settings → Pages 配 `main` 分支的 `/docs` 目录 —— `gh api -X PUT /repos/.../pages` 在我们 PAT 范围外，**返回 403**
- 配好之后任意 push 到 `docs/` 改动都自动部署（~30 秒生效）
- 主页 URL：`https://edwin-hao-ai.github.io/MouseClaw/`
- 入口 HTML：`docs/index.html`（中英双语，`?lang=` 切换，video 也会跟着切语言）
- 改 `docs/assets/demo-{en,zh}.mp4` push 后等几分钟刷一下浏览器（CDN 缓存可能逗你 5-10 分钟）

### 4. version.json 流程（auto-update channel）

- 路径：`docs/version.json`（GitHub Pages 直接 serve · 公开可读）
- App 启动 60s 后拉一次（24h 缓存写在 `~/.mouseclaw/last_update_check`），见 `src-tauri/src/update_check.rs`
- 字段：`latest` / `min_supported` / `released_at` / `size_mb` / `dmg_url` / `release_url` / `notes_zh` / `notes_en`
- **每次 release 必须更新这个文件**，否则现有用户永远收不到新版本通知
- `dmg_url` 一律指向 `releases/latest/download/MouseClaw_X.Y.Z_aarch64.dmg`（**注意带版本号**，否则下错）
- 客户端 `is_newer(latest, CURRENT)` 比较 semver，新版本则 emit 一个 reply 气泡（不打断主流程）
- **改完 version.json 必须 push** —— 不 push 就只在本地，更新通道不通

### 5. 演示视频流程（hyperframes）

- 目录：`marketing/demo-en/` + `marketing/demo-zh/`（**已 gitignore：`/marketing/` 加前缀防止波及 `docs/marketing/`**）
- 两个 dir 完全独立，各自有 `index.html` + `meta.json` + `package.json`
- 渲染：`(cd marketing/demo-en && npx hyperframes render)`
- **并行渲染必须用 subshell `(cd dir && cmd) &`**：Claude Code 的 `cd` 不跨 Bash 调用持久；两路并行如果不用 subshell，第二个 render 会跑错文件。验证：`lsof -p $(pgrep -f hyperframes) | awk '$4=="cwd"'`
- 渲染时长：~1-2 分钟（35s 1920×1080 @ 30fps · 1050 帧）
- 中文字体警告 `[Compiler] No deterministic font mapping for: PingFang SC` 是 **benign**，fallback 到 PingFang 正常出图
- GSAP `onUpdate` 是 deterministic 的，可以用 string slicing 做打字动画（hyperframes 按 progress seek 时 onUpdate 也会跑）
- 每个 scene 必须有 `class="clip" data-start="X" data-duration="Y" data-track-index="1"`
- 退场用 `tl.set("#sceneN", { visibility: "hidden" }, exit-end-time)` 否则 lint 报 `scene_layer_missing_visibility_kill`
- 输出在 `marketing/demo-en/renders/demo-en_YYYY-MM-DD_HH-MM-SS.mp4`，**手动 cp 到 `docs/assets/demo-en.mp4`**

### 6. 营销资产位置（all in `docs/assets/`）

| 文件 | 用途 |
|---|---|
| `demo-en.mp4` / `demo-zh.mp4` | 35s 演示视频，README + landing + 推文必用 |
| `demo-en.gif` / `demo-zh.gif` | GIF 版（~3.3 MB），chat 平台嵌入用 |
| `demo.mp4` / `demo.gif` | 别名 = EN 版（向后兼容） |
| `hero.png` | README 大图（从视频抽帧 1920×1080） |
| `social-preview.png` | 1280×640，GitHub 仓库 social preview / OG card（**必须人工上传** at Settings → Social preview，API 无 endpoint） |

GIF 生成命令（lanczos + 128 色调色板，~3.3MB / 35s）：

```bash
ffmpeg -y -i demo-en.mp4 \
  -vf "fps=12,scale=720:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128[p];[s1][p]paletteuse=dither=bayer:bayer_scale=4" \
  demo-en.gif
```

### 7. 多后端 / provider.env 约定

- 文件：`~/.mouseclaw/provider.env`（不进 repo · 单机生效）
- 格式：`KEY=VALUE` 一行一对，被 `provider_env.rs::apply_to` 灌给 spawn 的子进程
- **不覆盖** OS env 同名变量（shell rc 优先）
- 详见 `docs/provider-env-template.md`
- **Codex 2026 改了 `wire_api`：必须 `"responses"`**，`"chat"` 已废弃（启动会报错）
- **Hermes** 原生支持 `--provider ai-gateway`，最干净
- **OpenClaw** 走 OPENAI_BASE_URL/KEY，但 model id 必须是 gateway 上真实存在的（`curl gateway/v1/models` 验证）
- Vercel AI Gateway 可一个 key 跑通所有非 Claude 后端

### 8. 已知坑 + 绕法（**这是这份 cheat sheet 最有价值的部分**）

1. **添加 Config 字段忘记同步**：新 `pub field` 到 `config::Config` 之后，要同时改 `Config::default()` + `commands.rs::save_shortcut` 里的字面量构造 `Config { ... }`。漏一处 cargo 报 `E0063 missing field`
2. **i18n 加新 key**：必须三处都加 —— `src/i18n/types.ts`（Strings interface）+ `src/i18n/zh.ts` + `src/i18n/en.ts`。少一处 tsc 编译期报错
3. **新窗口 capability**：每次 new Tauri 窗口 label，要去 `src-tauri/capabilities/default.json` 的 `windows` 列表加进去，否则前端 invoke 报权限错误
4. **`WebviewWindowBuilder::new` 参数**：Tauri 2 需要 `&app`（引用），不是 `app`（值）。错了报 `expected &_, found AppHandle`
5. **多窗口路由策略**：单 bundle + `?view=picker` query 在 `main.tsx` 分发到不同组件，比多 HTML 入口简单。已建立约定：`hub` / `panel` / `picker` / `history` / `about` / `status` / `draw`
6. **网络问题（LibreSSL handshake failure）**：本地代理 / VPN 会偶发 `git push` 失败，**不要 retry-spam**，等 30-60 秒重试一次。`HTTPS_PROXY="" git push` 有时能绕过
7. **Whisper 模型大文件**：57MB 不进 git（`/src-tauri/resources/ggml-*.bin` 在 gitignore）。用 `scripts/fetch-whisper-model.sh` 作为 `beforeBuildCommand` 钩子，每次构建前 curl 一份到 `src-tauri/resources/`。Tauri 把它打进 `MouseClaw.app/Contents/Resources/models/`，`transcribe.rs::try_seed_from_bundle` 在首次启动时拷到 `~/.mouseclaw/models/`
8. **多 session 并行写代码**：经常出现 main 已被另一 session push 新 commit。本 session commit 前 `git fetch && git log origin/main -3` 查一下，必要时 `git pull --rebase`。**不要 force-push**
9. **gitignore 过宽**：曾经 `marketing/` 一行把 `docs/marketing/` 也屏了。**规则要尽量加 `/` 前缀锚定根**（`/marketing/` 只挡根目录的 `marketing/`），下层同名目录就不会被波及
10. **macOS 自启动真信源是 plist**：`tauri-plugin-autostart` 写到 `~/Library/LaunchAgents/com.edwinhao.mouseclaw.plist`，**真信源是这个 plist + 系统设置 → 登录项**。`config.json` 里的 `autostart: bool` 只是镜像，启动时双向同步：用户在系统设置里关掉 → app 启动时读真实状态回写 config
11. **`--minimized` 自启动静默**：`std::env::args().any(|a| a == "--minimized")` 检测，自启动场景不弹任何窗口，只挂菜单栏
12. **托盘 emoji 选 vs 真实形象**：macOS native menu 不能内联渲染 SVG/PNG。**多于 6 个选项时改成一个「打开 picker 窗口」入口**，把选择交给真窗口（v0.1.26 桌宠选择器就是这么做的）
13. **PixelMouse size prop**：曾经写死 `32 | 48 | 64 | 96 | 128`，遇到 size={22} 报错。**改成 `number`** 留弹性
14. **bundle.resources 资源访问**：app bundle 里的资源在 `current_exe()/../../Resources/`，例如 whisper 模型在 `MouseClaw.app/Contents/Resources/models/ggml-base-q5_1.bin`。dev 模式（`bun tauri dev`）走 `CARGO_MANIFEST_DIR` 兜底
15. **公证服务偶发慢**：3 分钟内通常完成，最长 10 分钟。**没动也别 kill** —— 重启会被记重复提交，反而更慢
16. **release 文件改名**：版本号变了 dmg 文件名也变，别忘了同步 `version.json` 的 `dmg_url`、`landing-kit.md` 里的引用、README 截图引用

### 9. Notarization keychain profile 备份命令

换电脑 / keychain 重建时重新存一次：

```bash
xcrun notarytool store-credentials "OCTAgentNotary" \
  --apple-id "120298858@qq.com" \
  --team-id  "5XNDF727Y6" \
  --password "<app-specific-password from appleid.apple.com>"

# 验证：
xcrun notarytool history --keychain-profile OCTAgentNotary
```

每台开发机都要独立存一次（凭证不能 export 不能共享）。

### 10. Marketing 文档索引

整套发布运营资产在 `docs/marketing/`：

- [`launch-kit.md`](docs/marketing/launch-kit.md) —— 各平台**复制粘贴用**的文案（HN / Reddit / X / YouTube / IG / 小红书 · EN+中文）
- [`twitter-and-hn-playbook.md`](docs/marketing/twitter-and-hn-playbook.md) —— 何时发、4 小时内做啥、X 算法 + HN 上首页攻略、回复策略、anti-patterns
- 配套设计文档在 `docs/design/v0.1.XX.md`，每个大版本一份

---

## 引用全局规则

继承 `~/.claude/rules/common/` 下所有通用规则（coding-style, git-workflow, testing, security 等）。
本文件只覆盖 MouseClaw 项目特定约束。
