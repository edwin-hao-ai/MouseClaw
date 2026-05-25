# Changelog · 更新日志

All notable changes to MouseClaw. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [SemVer](https://semver.org/).
所有重要变更。格式遵循 Keep a Changelog，版本号 SemVer。

---

## [0.6.0] · 2026-05-25 · 语音输入法大升级

### Fixed
- **中英混说不再听错**（"push" 不再变"铺石"）。根因：双语 zh-en 模型的英文建模单元
  是大写 BPE piece（`▁PUSH`），而旧术语库把英文 hotword 当整词喂给 sherpa →
  `Cannot find ID for token push` → 直接 skip，**英文 contextual biasing 一直是
  no-op**。修复：(1) 内嵌 `bpe.vocab`（~12 KB，随模型版本锁定）并设
  `modeling_unit=cjkchar+bpe` + `bpe_vocab`，让 sherpa 把英文词正确切成模型 BPE
  units 来 boost；(2) `active.txt`（sherpa hotwords_file）英文大写化对上模型 units
  （下游 `recase_english` 再还原自然大小写，零副作用）；(3) 内置词表补 100+ 常用
  git/工作流英文动词（push / pull / commit / merge / deploy / refactor…）。
  缺 `bpe.vocab` 时降级到"中文 biasing 仍在、英文照旧"，不阻断识别。

### Added
- **同句内自我纠正**（本地规则，零延迟）：一段话里先说正文、再用"哦/不对/等等 +
  去掉/删掉/改成"纠正自己，松手前就地改对再写入。例："…喝了一碗汤。哦，把那个喝汤去掉。"
  → 只写"…吃了一顿饭。"。marker 门控避免误删正文；中英文都支持。
- **词表加词（去权重）**：设置 → 语音 → "加一个词"。输一个词就进，中文自动逐字
  分词喂 sherpa（心房颤动→`心 房 颤 动`）、英文整词保留、**永不出现权重**（用全局默认）。
  加完自动刷新生效。
- **听写统计**：历史窗顶部 banner —— 累计字数 / 时长 / 平均 WPM / **省下多少分钟打字**
  （打字基线 40 WPM）。纯本地累计在 `~/.mouseclaw/stats.json`。（macOS 路径已接；
  Windows 听写路径待接。）
- **词表自动学词（更深词表）**：用户纠错"把 X 改成 Y"时，把 Y 静默学进词表
  （`~/.mouseclaw/vocab/auto.txt`，与手动 user.txt 分开），下次识别更准。term-like 过滤
  （短、无句子标点、非纯数字）+ 去重 + 上限 500 条。零用户操作。
- **轻量化"本地处理"：规则 + CLI，零额外模型下载**。小任务不该让用户下重模型 ——
  最终方案（评估并弃用了 580MB 本地 LLM，见下）：
  - **清理** = 纯规则（去口头禅 + 收敛空白/标点），0 模型、即时。
  - **整理成清单** = 纯规则：把"先…然后…还有…对了…"这种一长串口述切成 `1. 2. 3.` 有序清单
    + 换行（中英双语，英文按词边界匹配，剥句首引子）。设置→语音→"听写后整理成清单"
    开关（默认关）。实测输出 = 之前 LLM 同质量，但 0 模型、亚秒。
  - **翻译 / 解释 / 写回信** = 走已配置的 CLI 后端（真模型该干的活）。
  - 结果：**不再需要任何额外模型下载**（沿用 sherpa 语音模型即可），安装包不臃肿。
  - 评估记录：曾接入 Qwen3-0.6B ONNX 本地 LLM（纯 Rust + ort，无 Python）做兜底/整理，
    端到端跑通；但 q4f16 已是最小变体仍 580MB，CPU 推理冷启动 ~20s / 暖 ~3-5s，CoreML
    更慢；而"清理/整理"经 Grammarly 式思路用规则即可达同等效果、翻译交给 CLI ——
    故**移除该 LLM 及 ort/tokenizers 依赖**，换来零下载 + 即时。

---

## [0.5.0] · 2026-05-24

🪟🐧 **MouseClaw 登陆 Windows + Linux（测试版）**。macOS 之外，现在 Windows 10/11 与
Linux（X11 完整 / Wayland 降级）都能跑同一只桌宠。
MouseClaw now runs on **Windows + Linux** (beta), alongside macOS.

> ⚠️ **Windows / Linux 为测试版**：编译 + 打包已由 CI 三平台验证通过并产出安装器，但**运行时
> 行为（截图/Mode B 键入/选词/托盘/穿透/热键）尚未在真机充分验证** —— 欢迎试用并反馈。macOS
> 不受影响、行为与 0.4.7 一致。Windows installer 暂未代码签名（SmartScreen 会提示"未知发布者"，
> 点「更多信息 → 仍要运行」即可）。
> Windows/Linux are **beta**: CI verifies compile+bundle on all 3 OSes, but runtime behavior is
> not yet fully verified on real hardware. macOS unchanged. The Windows installer is unsigned for now.

### Added
- **🪟 Windows 10/11 支持** —— 截图(xcap)/Mode B 键入(enigo SendInput)/剪贴板/前台 app(Win32)/
  TTS(SAPI5)/闲置检测/托盘/全局快捷键。听写默认普通全局快捷键 `Ctrl+Alt+I`（另可选长按右 Ctrl）。
- **🐧 Linux 支持（X11 完整 / Wayland 降级）** —— X11：xcap 截图 + XQueryPointer 光标 + PRIMARY
  选区 + WM_CLASS 前台 + enigo(XTest) 注入；提供 x86_64 与 **arm64** 两种 `.deb`/`.AppImage`。
  Wayland 安全模型受限 → 自动降级：Mode B 改「复制到剪贴板 + 提示 Ctrl+V」、眼球追踪/跟随关闭、
  首次截图走 portal 授权。
- **🧩 跨平台抽象 `platform.rs`** + 按平台密钥存储（macOS Keychain 不变 / Win Credential Manager /
  Linux Secret Service，缺时降级 0600 文件）+ 精简版 onboarding（Win/Linux 跳过权限三连页）。
- **⚙️ CI 三平台构建矩阵** —— Windows / Linux(x64+arm64) 完整打包出安装器，macOS 编译验证。

### Note
- macOS 自动更新通道（version.json）**故意停在 0.4.7** —— 本版对 macOS 用户无新功能，不打扰。
  Win/Linux 测试用户从 GitHub release 页手动下载。

## [0.4.7] · 2026-05-23

紧跟 0.4.6：补上三个"陪伴感"功能（首次 aha / 我们的故事 / 主动记忆提醒）+ 后端可用性核对。
Follow-up to 0.4.6: three companionship touches (first-run aha / our story / proactive memory
glances) + a backend-verification pass.

### Added
- **✨ 首次 aha —— 向导里桌宠就"活"了 / First-run aha** —— Onboarding 向导窗口里的桌宠
  眼球就开始追你的光标、贴近抬头（复用桌面同一套 companion 数学，9 皮肤通吃）；并在进向导
  那一刻就**后台预取语音模型**，等你点完几步基本下好，消灭"装完才开始下"的死等。
  The pet comes alive during onboarding (eyes track your cursor) + voice models prefetch while you click through.
- **💗 "我们的故事" / "Our story"** —— 记忆查看器新增一页：认识天数 / 夜晚陪伴次数 / 一起做过
  多少次 / "它眼里的你"，全部来自本地 memory.db。**数据不够时如实说"才刚开始"，绝不编数字**。
  A relationship page (days known / nights together / times / how it sees you), all from local data — honest when sparse.
- **💭 主动记忆提醒 / Proactive memory glances** —— 没别的要说时，偶尔基于**真实**高频实体
  （你反复在做的项目/话题）轻轻来一句「还在忙 X 吗」。多重守门：6h 长冷却 + 你在场 + 自然停顿 +
  非心流敲键 + 真有 freq≥3 的常做实体才发，无数据绝不打扰、绝不编造。性格还会给它上一层语气。
  Occasional, heavily-gated "still on X?" nudges from real recurring entities — never fabricated, never spammy.
- **📄 开源许可改为 FSL / Switched to Functional Source License (FSL)** —— 见 LICENSE。

### Fixed
- **移除 Trae Agent（不可用）/ Removed Trae Agent (not usable)** —— 实测它不在 PyPI，
  从 git 装会缺一串依赖（docker → pexpect → …）启动即崩，且是 Docker 重型研究 agent、输出
  冗长，不适合本场景。后端从 15 → **14**。It crashes on launch (broken packaging) and is a
  heavyweight Docker-based research agent — a poor fit, so it's out (15 → 14 backends).
- **Antigravity 去掉错误的 flag / Antigravity flag fix** —— 早期误加了 `--dangerously-skip-permissions`
  （那是 Claude 的 flag，`agy` 没有，会当 unknown flag 报错）。`agy -p` 自身就是 headless。
- **Qwen Code 改用位置参数 / Qwen Code positional prompt** —— 官方已把 `-p` 标 deprecated，
  改 `qwen "PROMPT"` one-shot，避免未来失效 + deprecation 警告污染输出。
- 其余后端（Gemini / Copilot / Kiro / Kimi / Vibe / Pi + 已装的 Claude/Codex/OpenClaw/Hermes/
  OpenCode/Cline）旗标 + 安装命令均经官方文档 / `--help` 核对确认正确。

## [0.4.6] · 2026-05-23

一个大版本：桌宠从"语音问答工具"长成"会记事、会定时干活、有名字有性格"的桌面伙伴。
14 个 AI 后端任选（含国产 Qwen Code 通义千问）、长期记忆（全本地 SQLite 知识图谱）、
定时任务/心跳（一句话创建 + 统一结果页 + 反幻觉 harness）、桌宠起名 + 9 种性格、程序化
音效。并根治了"某 app 全屏后桌宠消失"——桌宠现在用真正的 NSPanel 浮在全屏之上。
A big one: the pet grows from a voice Q&A tool into a desktop companion that remembers,
runs tasks on a schedule, and has a name + personality. 14 AI backends (incl. China's
Qwen Code), long-term memory (fully-local SQLite knowledge graph), scheduled
tasks (one-line create + unified results page + anti-hallucination harness), pet naming +
9 personalities, procedural sound effects. Plus the fix everyone wanted: the pet now
floats over fullscreen apps via a real NSPanel.

### Added
- **🤖 14 个 AI 后端任选 / 14 AI backend CLIs** —— Claude Code / Codex / Gemini /
  Copilot / OpenCode / Cline / Kimi / Kiro / Antigravity / Mistral Vibe / Pi / OpenClaw /
  Hermes + 国产 **Qwen Code（通义千问）**。Onboarding
  只列**已安装**的后端，托盘可随时切换。Pick any of 14 backends incl. China's Qwen Code;
  onboarding shows only installed ones; switch anytime from the tray.
- **🧠 长期记忆 / Long-term memory** —— 全本地 SQLite（无向量库）三层记忆：画像 / 情景 /
  知识图谱。空闲时蒸馏画像、构建实体关系图，召唤时检索相关记忆注入上下文（"更懂你"）。
  「桌宠记得的事」窗口可看画像 / 历史 / 关系图谱，逐条删除、一键清空、随时暂停。默认开，
  首次启动透明告知"全部存你本地"。Fully-local SQLite memory (profile / episodic / graph),
  idle-time reflection, a viewer window to inspect & delete, pause anytime, all on-device.
- **⏰ 定时任务 / 心跳 / Scheduled tasks** —— 召唤桌宠说一句"每天早上整理 AI 新闻"就建好；
  到点自动后台跑，结果用轻气泡投递、不打扰；**统一结果页**把所有任务的所有执行倒序排一起
  （多任务完成一页看全，markdown 渲染、外链走系统浏览器），完成自动跳过去"像看报纸"。
  **反幻觉 harness**：放开联网/读文件工具去取真实数据 + 真实性铁律（取不到就如实说，绝不编造）。
  One-line natural-language create; runs in the background; a unified results feed
  (newest-first, markdown, links open in browser); anti-hallucination harness with real tools.
- **🐭 桌宠起名 + 9 种性格 / Pet name + 9 personalities** —— 给桌宠起个名字（🎲 随机 +
  建议名 + 起好打招呼）、选语气（暖心 / 毒舌 / 极简 / 话痨 / 干练 / 元气 / 沉稳 / 好奇 /
  傲娇 / 自定义）。性格只改语气，绝不影响任务正确性。Name your pet & pick its tone.
- **🔊 皮肤音效系统 / Procedural sound effects** —— 程序化 chiptune 合成，零素材，9 款皮肤
  各有音色。可在托盘开关 / 调音量。Zero-asset chiptune synthesis, per-skin timbre.

### Changed
- **🖥️ 桌宠浮在全屏 app 之上 / Pet floats over fullscreen apps** —— 改用真正的 **NSPanel**
  （level=ScreenSaver + canJoinAllSpaces|fullScreenAuxiliary），根治"某 app 全屏后桌宠消失"。
  普通窗口在 release 构建下浮不上全屏（Tauri 已知限制），NSPanel 是 BongoCat / Cap 等同类
  应用的标准方案。Now uses a real NSPanel so it stays visible over any fullscreen app.
- **👀 眼球追踪整屏跟手 / Eye-tracking across the whole screen** —— falloff 180→600，桌宠
  蹲角落时眼睛也能跟着整个屏幕的光标按比例转，不再一远就"钉死"看着不追。
- **🧹 代码整理 / Internal cleanup** —— `commands.rs` 按职责拆 4 个子模块（1458→751 行），
  满足单文件 ≤800 行硬规则。

### Fixed
- **🔇 关掉"起床"音 / Removed the wake-up sound** —— 每次回到桌面桌宠一醒就叫，太骚扰，去掉。
- **🎬 入场动画 7 个真问题 / 7 entrance-animation bugs** —— 多智能体审计发现并修复：开机即
  召唤的竞态、被打断时位置错乱、桌宠卡隐藏、首次 loud 入场被误标已见等。
- **⚡ text-only 调用快 3 秒 / 3s faster short tasks** —— spawn 子进程关掉 stdin，不再傻等
  "no stdin data received in 3s"，reactive / 定时任务 / 记忆反思都快一截。
- **🔁 记忆重复条目 / Duplicate memory insights** —— reflection 加串行闸，防 pipeline 与
  定时器并发蒸馏同一批 → 重复画像。
- **📋 定时任务体验 / Scheduled-task UX** —— 新建解析时 loading 转起来（不再像卡死）、立刻跑
  有"执行中"态、结果面板可随窗口拉大、按钮带文字（▶ 立即运行 / 📄 结果 / ✏️ 编辑 / 🗑）。

## [0.4.3] · 2026-05-21

修桌宠贴屏幕角落点开菜单时的两个老问题：菜单被切 + 桌宠跳。安装包仍 12 MB。
Fixes two long-standing issues when opening the pet menu in a screen corner:
the menu getting clipped + the pet jumping. Still a 12 MB DMG.

### Fixed
- **🐭 桌宠菜单贴边不再被切 / 桌宠不跳 / Pet menu no longer clipped or jumpy at edges**
  —— 桌宠在屏幕角落（尤其右下 / 左下）点出菜单时，菜单会往屏幕内侧自动翻转展开
  （贴右→往左、贴左→往右），不再被屏幕边切掉一截；同时桌宠不再被窗口撑大时的整窗
  clamp 拽回角落乱跳。The pet menu now flips toward the screen interior when the pet is
  near an edge (anchored right→opens left, left→opens right) instead of being clipped,
  and the pet no longer jumps when the menu expands the overlay window.

## [0.4.2] · 2026-05-21

语音输入更快、快捷键设置更顺手：本地转写 + 标点改多线程推理（出字更跟手），托盘新增
统一的快捷键设置入口（召唤 AI 快捷键 + 语音触发键一处切换），修掉连续语音输入时桌宠
被拽回角落的跳动。安装包仍 12 MB。
Faster dictation and tidier shortcut settings: multi-threaded local ASR + punctuation,
a unified shortcut-settings area in the tray (summon hotkey + voice trigger key), and a
fix for the pet jumping back to its corner during rapid dictation. Still a 12 MB DMG.

### Added
- **⌨️ 托盘统一快捷键设置 / Unified shortcut settings in the tray** —— 召唤 AI 的快捷键
  之前只能在 onboarding 设，现在托盘新增「召唤快捷键」子菜单（⌘⇧Space / ⌘⇧M / ⌘⇧D /
  ⌃⇧Space 预设），紧挨「语音触发键」子菜单，一处切换两个快捷键，热切换不用重启。
  The summon hotkey used to be onboarding-only; now there's a "Summon shortcut" submenu in
  the tray next to the voice trigger key — switch both in one place, hot-swapped, no restart.

### Changed
- **⚡ 语音输入法提速 / Faster dictation** —— sherpa 本地转写 + 标点推理从单线程改成按
  机器核数并行（2~4 线程），松手后出字、加标点更跟手。Local ASR + punctuation inference
  went single-threaded → multi-threaded (2–4 by core count) — snappier output on release.
- **🧹 内部清理 / Internal cleanup** —— tray.rs 拆成 5 个 ≤400 行模块；移除 v0.3 已删的
  Whisper 过时引用，统一指向 sherpa-onnx。Split tray.rs into 5 modules; purged stale
  Whisper references (sherpa-onnx since v0.3).

### Fixed
- **🐭 连续语音输入桌宠不再跳 / No more pet jump during rapid dictation** —— 写入光标后
  立刻再按触发键说下一段，桌宠会先被上一次的 auto-hide 计时器拽回角落再跳到光标。改为
  受 generation 保护的延迟隐藏 + 新会话开始时让旧计时器失效。Speaking a second phrase
  right after the first no longer yanks the pet back to its corner mid-session.

## [0.4.1] · 2026-05-21

一波打磨：根治提醒后桌宠漂移、修好 agent-browser 安装、浏览器能力改成"优先用你自己的
Chrome"、中英混合识别更准、onboarding 精简一步。安装包仍 12 MB。
Polish pass: kill the post-nudge pet drift, fix agent-browser install, make
browser automation prefer *your own* Chrome, sharper mixed zh/en recognition,
one fewer onboarding step. Still a 12 MB DMG.

### Fixed
- **🎓 首次引导 tour 的按钮能点了 / First-run tour buttons are clickable** —— tour 气泡的
  「好啊 / 下次再说」点不动：helper 组件定义在渲染函数体内，桌宠眼球追鼠标导致频繁重渲染时
  按钮被反复 remount，点击落空。提到模块级后引用稳定。The tour bubble's buttons were
  remounting on every re-render (inline component defs) — hoisted them to module scope.
- **🎯 提醒「稍后」后桌宠不再漂移 / No more pet drift after a reminder** —— 喝水/睡觉等
  提醒气泡点「稍后」收起时，桌宠会一点点往斜上方挪。根因是 idle 视图下"前端 has_ui 改尺寸"
  和"自适应测量改尺寸"两条路同时 reposition + 锚点取整偏置。改为自适应测量单一管尺寸 +
  锚点 round 取整。Dismissing a reminder no longer nudges the pet diagonally upward.
- **🌐 agent-browser 装得上了 / agent-browser install fixed** —— 之前装的是不存在的
  `@vercel/agent-browser`（npm 404 → exit 1）。修正为正确包名 `agent-browser` + 自动备好浏览器。
  Was installing a non-existent package; now uses the correct `agent-browser`.
- **🗣 中英混合识别更准 / Sharper mixed zh/en speech** —— 抗混叠重采样修识别率根因 +
  英文大小写还原 + 品牌词表，混说句子里的英文不再全大写。Anti-aliasing resample +
  English casing restore + brand vocab.

### Changed
- **✨ 浏览器自动化：优先用你自己的 Chrome / Browser automation prefers your own Chrome** ——
  onboarding 浏览器步改成三选一，把 CDP「用我的 Chrome」（复用本机 Chrome、带你的登录/cookie、
  不下载）设为默认推荐项，agent-browser（独立 headless）降为进阶选项，另加"暂不启用"。
  The onboarding browser step is now a 3-way choice with "Use my Chrome" (CDP, your
  logins/cookies, no download) as the recommended default.
- **🪜 onboarding 少一步 / One fewer onboarding step** —— 移除原"试这条"三卡 demo（依赖外部
  前置条件、常常落空误导用户），重点改为确保浏览器/Office CLI 装好可用。8 步 → 7 步。
  Removed the "try this" demo step; focus on getting the CLIs installed.

## [0.4.0] · 2026-05-20

桌宠从"会说话的工具"长成"有性格的伙伴"：陪伴向动画全集、撞墙回弹、Reactive 反应、
喂文件、AI 实时活动、串行队列。安装包仍 12 MB。
The pet grows from a talking tool into a companion with personality: full
companion-animation set, edge bounce, reactive ribbon, file-feeding, live AI
activity, serial task queue. Still a 12 MB DMG.

### Added
- **🐭 陪伴向动画全集 / Full companion-animation set** —— 眼球追鼠标、打字时点头陪伴、
  贴近抬头/兴奋、闲置渐睡、点头浮爱心、亲密度成长（互动越多眼睛越大）、伸懒腰 / 晕 /
  抬头 / 小跳、深夜 drowsy、长期冷落委屈。**9 款皮肤全部适配**（纯 palette/transform 驱动）。
  Eyes track the cursor, nods while you type, perks up on hover, dozes off when idle,
  floats a heart on pat, grows intimacy (bigger eyes), stretch/dizzy/glance/hop,
  late-night drowsy, neglected sulk — all working across **all 9 skins**.
- **🧱 撞墙回弹 / Edge bounce** —— 桌宠跟随光标 / 被拖动 / 气泡撞到屏幕边时，被挡在屏内
  并朝那面墙做"软果冻挤压 + 回弹"，不再飘出屏幕或在边缘累积偏移。
  When the pet follows the cursor, is dragged, or a bubble hits a screen edge, it stays
  on-screen and does a soft squash-and-recoil against the wall.
- **📋 Reactive 桌宠 / Reactive pet** —— 复制内容 → 桌宠抖耳 + 头顶 ribbon 弹快捷动作
  （翻译 / 解释 / 📄 纯文本…），5s 自动消失或点了就做。处理完即使 ribbon 已消失也会通知。
  Copy something → the pet twitches + a ribbon of quick actions pops above its head.
- **🍽 喂文件 / Feed files** —— Finder 文件拖到桌宠嘴里 → 它"吞下" → 你接着说
  "总结一下" → AI 读 PDF / 网页 / 代码 / 截图。Drag a file onto the pet, then ask about it.
- **🧠 AI 实时活动 / Live AI activity** —— AI 处理期间气泡实时显示"正在思考 / 读文件 /
  跑命令"，不再像卡死。Bubble shows what the AI is doing during long tasks.
- **🔁 AI 任务串行队列 + 忙碌指示 / Serial AI queue + busy badge** —— 桌宠一次做一件 AI
  工作，按下就排队、逐个做完都通知；语音听写永远即时、不排队。三点忙碌 badge 任何视图可见。
  One AI task at a time, queued and each notified on done; voice typing never queues.
- **✏️ 语音确认可编辑 / Editable voice confirm** —— 转写完进确认框可直接改字；续写若丢
  光标自动落剪贴板兜底。Edit the transcript in place; insert-at-cursor falls back to clipboard.
- **📖 语音术语库 + 本地纠错 / Voice vocabulary + local correction** —— 内置程序员词表
  （API / Tauri / Rust / commit 等不被听错）+ 可加自定义词（sherpa hotwords）；3 秒规则纠
  常见误识别，纯本地不调 LLM。中英双语 ASR 锁定，不再二选一。
  Built-in programmer vocabulary + custom words via sherpa hotwords; rule-based local
  correction of common misrecognitions (no LLM); bilingual ASR locked (no language toggle).
- **🔤 选词触发 / Selection trigger** —— 选中文字也能弹 Reactive 动作（macOS 限制：仅原生
  app；Chrome/Electron 走复制路径）。Select text to trigger Reactive too (native apps only).
- **🛠 一键装 CLI / One-click CLI install** —— 托盘 / onboarding 一键装 agent-browser /
  OfficeCLI，装完桌宠弹庆祝气泡。One-click install of agent-browser / OfficeCLI from the tray.
- **🎓 learn-by-doing 教程 / Hands-on onboarding** —— onboarding 第 7/8 步带你真用一遍。
- **🎨 9 款皮肤 / 9 skins** —— 新增 🐱 小灰猫 / 🦊 赤狐 / 🐸 树蛙（原 6 款 + 3 个非鼠物种）。
  Added cat / fox / frog alongside the original six.

### Changed
- **📐 自适应 overlay 尺寸 / Content-driven overlay size** —— 气泡 / 菜单 / ribbon 按内容
  自动量尺寸再设窗口，不再手调尺寸常量导致内容被裁。Bubbles/menus measure their content
  and size the window to fit — no more hand-tuned size constants getting clipped.

### Fixed
- **桌宠"乱飘"根治 / No more random drifting** —— listening 跟随时禁用自适应（避免跟
  cursor-follow 抢窗口），resize 用"权威锚点"防边缘 clamp 累积偏移。
- 冷启动首次 fn 唤不出 / 桌宠不移到光标。First-fn-after-cold-start now works.
- 语音听写在 AI 子进程并发时卡死 —— 解码挪出 tokio 专用线程 + AI 子进程降优先级。
- Reactive 反馈环（处理完写回剪贴板不再触发 ribbon 重弹）。
- 喂食取消 / drag-leave 顺序 + crash；多屏 y 翻转；pet_passthrough off-main crash。
- loading / 气泡的"一圈黑色阴影"（阴影被自适应窗口裁切）。

---

## [0.3.11] · 2026-05-19

### Changed
- **语音确认气泡彻底重做** —— 之前是"先看气泡 → 点一下进编辑模式 → 再出 textarea"两段式，
  用户反馈"修改框很乱"。改成**一进确认就是 textarea**，光标自动定到末尾，可以接着补充；
  ↵ 直接发，Esc 取消。框宽 282px 适配 320 overlay，不再被右边切。
- **倒数 3 秒 → 6 秒** —— 3 秒来不及看清更别说改字。
- **用户开始打字 → 自动暂停倒数** —— 标题切到绿色"✏️ 编辑中…"，等用户主动 ↵/Esc，
  无限时间。新加 `voice_confirm_hold` command + `VC_HOLD=3` 状态。
- **流式滚动改成跟随最新 token**（v0.1.23 永远顶部被推翻）—— 流式期间默认贴底显示
  正在生成的内容；手动上滚则暂停跟随；滚回底部自动恢复。手动上滚时右下角浮 ▼ 一键回底。
- **桌宠新增 `talk` 状态** —— AI 流式输出时嘴一直在动 + 身体打节奏。之前用 `think`
  (头部小晃) 让人感觉"卡住了"，现在能看出"它真的在说话"。`reply.streaming === true`
  自动切到 talk sprite。
- **Markdown 风格统一** —— 段落 / 列表 / 标题间距收紧 ~10%，首末元素 margin
  强制为 0（scroll / 非 scroll 都生效），消除底部"💬 继续追问"前那条空行。

---

## [0.3.10] · 2026-05-19

### Added
- **🪞 多屏 y 翻转修复** —— 原来用 `NSScreen.mainScreen`（key window 所在屏），副屏激活时
  拿错 height 导致 y 翻转偏 100-200px，跑步动画落点不对。现在固定 `NSScreen.screens[0]`
  （永远是主屏 / 菜单栏 / 全局坐标原点所在屏）跟 macOS 全局坐标系一致。

### Note
- 从浏览器拖图片/PDF：Chrome/Safari 会把拖动的图片写临时文件，Tauri DragDropEvent 收到
  这个文件路径 → 我们当成本地图片处理。**不需要专门写 URL drag 逻辑**，开箱就支持。
  纯 URL 文本拖动（不带图片）目前不触发 —— 那是 Tauri 2 OS-level intercept 的限制，
  要解需要重写 webview drag-drop 层，v0.5 看用户反馈再说。

### Fixed
- **顶部"缺一条边"气泡 bug** —— `--bubble-border` 从 `rgba(0,0,0,0.06)` 升 `0.10`，
  success/warn 变体的 135deg 渐变浓色处现在也看得见上边。
- **长回答看不见开头** —— 现在 scrollable bubble：
  - 顶部 / 底部各加 fade gradient（跟 variant 渐变对齐），提示"上面/下面还有"
  - 右上角圆形 ▲ 回顶按钮，仅当用户滚下去时浮现
  - 字号顶部 padding 10→14px，正文不再紧贴圆角
- **拖文档桌宠不"跑"过来** —— v0.3.7 的 `on_drag_enter` 调 `show_mouse(app)`，
  内部又 `cursor_follow::enable()` 抢前 100ms 把窗口直接传送到光标，384ms ease-out
  插值动画看不见。改为直接 `window.show()` + 显式 `cursor_follow::disable()`
  让动画独占 set_position，结束才把控制权交回 follow loop。

### Technical
- 编译警告清理：删 `bail` / `Emitter` / `NSString` / `s_skin` 等 unused imports + variables

---

## [0.3.7] · 2026-05-19

### Added
- **🦞 拖文档喂桌宠（核心新功能）** —— 拖任意文档 / 图片到桌宠脸上，桌宠
  从角落**跑过去**张嘴接（12 帧 ease-out cubic ≈ 384ms 窗口位置插值），drop 后
  600ms 吞咽动画 → 进入消化态 + 自动开 mic → 你**说问题** → 2.5s 静默自动 finalize →
  AI 看着文件回答你。
  - **支持格式**：png/jpg/heic/webp（图片）· txt/md/json/csv/log/yaml + 所有源码（文本）·
    pdf（`mdls -raw -name kMDItemTextContent`，Spotlight 已索引秒提）· docx/rtf/odt
    （`textutil -convert txt -stdout`）· xlsx（`unzip` + 手解 sharedStrings/sheet XML）
  - **拒收**：.app/.exe/视频/音频/zip · 单文件 >20 MB · 一次 >5 个文件
  - **多文件**：preamble 列出每个文件 + 文本类内联到 prompt（30k 字以内）；超长给路径
    让 Claude `Read` 工具自己看
  - **录音判定**：8s grace 等用户开口（drop 完走到键盘的时间），出第一字后 2.5s 静默
    finalize，兜底 30s 不说就放弃
  - **Esc 取消**：drag 中或录音中按 Esc → 清掉 fed_docs + 停录音 + 桌宠滑回 anchor
- **anchor=Hidden 提示** —— Hidden 选项 hint 加 "⚠️ 拖文档喂桌宠会失效（需要桌宠可见）"

### Engineering
- 新模块 `src-tauri/src/feed.rs`（625 行）+ `feed_flow.rs`（380 行）—— 都在 800 行硬规则内
- 全程零新依赖（unzip / mdls / textutil 都是 macOS 自带）
- `cargo test feed::tests` 7 个测试

### CLAUDE.md
- **新加头号硬规则：动手前先深度思考 UX**，10 问 checklist 写代码前必须脑里跑一遍

---

## [0.3.6] · 2026-05-18

UX 大整顿。按 CLAUDE.md 新的「头号硬规则：动手前先深度思考 UX」一条一条审过。

### Fixed
- **🔒 语音字进不去输入框 (用户反馈 #1)** —— Voice IME 触发时显式检查
  `AXIsProcessTrusted`，缺权限就**立刻** emit Blocked 气泡 + 加按钮"🔓 去授权"，
  点了直达 `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`。
  之前是 CGEventPost 静默 drop，用户完全不知道为啥没字，体验等于一直"坏掉"。

### Added
- **🎯 中英文标点补全** —— sherpa-onnx CT-Transformer int8 模型 (72 MB)
  bundled in DMG，首启动 seed 到 `~/.mouseclaw/models/sherpa-punct/`，
  本地推理 ~10ms 给 light_clean 后的文本补 `。，？！`。voice IME + 主 pipeline
  都过这一层。失败兜底返回原文，永远不阻塞主流程。
- **🖱️ 拖动桌宠到任意位置** —— idle 时 `.stage-mouse` 加 Tauri `data-tauri-drag-region`，
  整片区域可拖动整个 overlay 窗口。拖完 mouseup 把窗口左上角坐标存到
  `config.pet_custom_position`，下次启动 `apply_idle_anchor` 优先用这个坐标。
  托盘 anchor 子菜单点任意角落 = 清空 custom 回到 corner。click vs drag 由
  Tauri 原生区分（鼠标几乎没动 = 触发 onClick 弹菜单）。

### Technical
- 新模块 `punctuation.rs` + 新命令 `open_accessibility_settings` + `save_pet_custom_position`
- `PetAnchor::Custom` 没引入额外 enum 变体（用 Option 字段 `pet_custom_position` 更简洁）
- `Bubble` 组件新增 `action?: {label, onClick}` prop，blocked 气泡用来挂"去授权"按钮
- DMG 增 ~72MB → 总体积 ~250MB（model 占大头）

### CLAUDE.md
- 在文件最顶端加了 **「头号硬规则：动手前先深度思考 UX（10 问 checklist）」**
- 写入 v0.3 全程踩坑教训：「无脑动手等于一晚上 5-6 个 release 但核心问题没解」

---

## [0.1.31] · 2026-05-18

### Fixed
- **🎨 换桌宠点取消（再次）/ 点窗口 ✕ 仍然把 app 关掉** — v0.1.30 只改了 `apply()`
  路径，`cancel()` 路径仍是 `getCurrentWindow().close()`，所以点取消按钮还是
  触发同样的 quit-app bug。同时原生标题栏的红 ✕ 走的是 Tauri 默认 close-requested
  行为，也会 destroy 窗口。完整修复：
  - `cancel()` 也改成 `.hide()`
  - 加 `onCloseRequested(e => { e.preventDefault(); ...; w.hide() })` 拦截原生 ✕
- 反思：v0.1.30 的 Edit `replace_all: true` 应该一次改两处都覆盖到，但实际只
  覆盖了第一处。**以后改完后必须 grep 验证 hide / close 的最终分布。**

---

## [0.1.30] · 2026-05-18

### Fixed
- **🖱️ PetMenu 点项目后不收起** — click 事件在菜单项里冒泡到父 `.stage-mouse`
  div，触发 `handleMouseClick` 的 toggle，刚 `onClose()` 的菜单又被立刻 reopen。
  在 PetMenu 容器（和 NudgeBubble 容器）上加 `onClick={e => e.stopPropagation()}`
  截住冒泡。点喂奶酪 / 召唤 / 历史 / 任何项目都正确关闭菜单了
- **🎨 换形象 → 取消把整个 app 关掉** — `getCurrentWindow().close()` 在
  `decorations:true` 的 picker 窗口上会让 Tauri 复位 activation policy → macOS
  把 accessory app 一并 quit。改成 `.hide()`，下次打开自动 reuse 这个隐藏窗口
- **🎤 PetMenu「开始说话」点击没反应** — 原来调的是 `toggle_recording`（=松开
  快捷键，对刚启动录音是反向操作）。新增 `start_recording` 命令（等价"按下"），
  listening 状态下点桌宠 = 自动 stop+send。Mouse-only 用户现在能完整跑完一次
  push-to-talk：点 PetMenu → 说话 → 再点桌宠 / Esc → 发送
- **菜单文案变清晰** — "召唤·说话" → "开始说话"，并加 hint：
  "或者按住 ⌘⇧空格说话，松开发送"

---

## [0.1.29] · 2026-05-18

### Fixed
- **🖱️ 角落桌宠真的能点了** — v0.1.28 加了 `acceptFirstMouse: true` 后用户反馈
  依旧无法点击。真正的根因是 [App.css](src/App.css) 里 `.stage-mouse` 设了
  `pointer-events: none`（早期为了让透明 overlay 穿透到底层 app），同时也把
  桌宠自己一起灭了。`PixelMouse.css` 的 `.mouse-wrap` 也是同样问题。
  v0.1.29 把容器 `.stage` 设 pointer-events:none（继续穿透透明区），但桌宠
  本体和 menu/bubble 区开 auto，PetMenu / NudgeBubble / 召唤菜单这些都终于
  能正常 click 了
- **🎙️ Whisper 中文识别率突然变差** — v0.1.25 起把 Base 模型（59 MB）打进
  bundle 做"零网络 OOTB"体验，同时把默认从 Small 改成 Base。用户实际感受：
  「内置版准度明显不如以前下载的版本」。修复：
  - 默认改回 **Small** (190 MB · 中文质量大跳)
  - 新增 **fallback chain**：Small 没下载好时临时用 bundled Base，下载完
    自动切回 Small —— 既保留 OOTB 体验又给出更好的最终准度
  - schema v15 → v16 迁移：现有 config 上仍是 Base 的用户自动升级到 Small

### Technical
- `pointer-events` 分层：透明 stage 穿透 / 桌宠自己接 click / 菜单+气泡 absolute
  子元素从 .stage-mouse position:relative 起算
- `transcribe::resolve_loadable()` 实现 fallback chain · 优雅降级 Small → Base
- config v16 migration：Base → Small auto-upgrade，next save 后再不触发

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
