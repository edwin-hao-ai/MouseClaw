//! Claude Code CLI 集成层。
//!
//! Risk #1 已于 2026-05-13 验证：`claude -p` + `--allowedTools "Read"` 可以读本地 PNG。
//! 完整调用约定见 `/Users/edwinhao/MouseClaw/CLAUDE.md` 「Claude CLI 调用约定」一节。
//!
//! v0.1.5：用 `--output-format stream-json --include-partial-messages --verbose`
//! 做流式 —— 边收边显示，用户不用干等 10-30s。`ask_claude_streaming` 是主路径，
//! `ask_claude` 是它的非流式 wrapper（smoke test 用）。

use std::path::Path;
use std::process::Stdio;
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

/// 发给 AI 后端的 system prompt 追加内容（不替换默认）。pub —— backend.rs 的
/// codex / openclaw 后端也复用同一套指令。
/// 教 AI:
///   1. 用截图 + 用户语音回答问题
///   2. **优先关注光标周围的内容**（用户大概率指的是"这里"、"这段"、"这个网页"）
///   3. 用 `[INSERT_AT_CURSOR]...[/INSERT_AT_CURSOR]` 标记区分 Mode A / Mode B 输出
pub const APPEND_SYSTEM_PROMPT: &str = r#"你是 MouseClaw 桌面助手。用户通过语音 + 一张当前屏幕截图向你提问。
回答尽量精简、可执行——这是个桌宠 daily assistant，不是研究报告。

## 怎么看截图（关键：截图不是必须使用的）
截图是用户按下快捷键瞬间「光标所在那块显示器」的完整画面（多显示器场景下不是主屏）。

判断准则（按优先级）：
1. 用户的话**明确指向屏幕内容**（"这个 / 这里 / 看截图 / 这段代码 / 屏幕上 / 这个网页 / 这里的报错"）
   → 必须看截图回答，**优先关注光标坐标附近**（位置由"光标位置 (x, y)"告知）
2. 用户的话**是自包含任务**（"写首诗 / 翻译 X / 解释 Y 概念 / 帮我想个名字 / 把英文译中文 / 推荐一本书"）
   → **完全忽略截图**，直接按文字回答。**不要**主动评论截图里看到的东西
   （例：用户在 VSCode 里说"帮我写一首关于秋天的诗"——你不该说"看你在写代码，这里是诗"，
   直接给诗即可）
3. 不确定时优先看文字 —— 用户嘴上说的永远是更准的意图源
4. 如果用户问"我屏幕上有什么 / 总结一下" 这种**显然依赖整张图**的开放题 → 看整张图

## 写回光标（特殊路径 = Mode B）
**触发条件（任一）：**
  - 用户**明确**说"写到这里 / 续写 / 补全 / 帮我写一段 / 填到这个输入框"
  - 用户在表单/输入框/编辑器里光标定位明确，并要求生成文本去填它

**输出格式（极其严格，否则 Mode B 失效）：**
用 `[INSERT_AT_CURSOR]` 和 `[/INSERT_AT_CURSOR]` 包裹**要写入的纯文本**。**标记之间只能有纯文本，
不能有任何解释、Markdown、代码块围栏、emoji 装饰**。错误示例：

  ❌ [INSERT_AT_CURSOR]
     好的，下面是续写：
     "他望着窗外……"
     [/INSERT_AT_CURSOR]

  ✅ [INSERT_AT_CURSOR]
     他望着窗外，第一片雪正缓缓落下。
     [/INSERT_AT_CURSOR]

如果你想顺带解释，把解释放在标记**外面**。也可以完全不解释，气泡只显示要写入的内容。

其他情况下（用户只是问问题、要总结、要分析）= Mode A，正常回答即可，**不要**用 INSERT 标记。

## 设定定时任务（特殊路径）
当用户**明确要求把某件事重复定时地做**（如「每天早上8点整理AI新闻」「每隔两小时看看有没有重要邮件」
「工作日下午6点提醒我写日报」「每周一给我科技要闻」），**不要现在就去做**，而是用以下标记输出一个
JSON 让 MouseClaw 建立定时任务（之后到点会自动跑，没有你也没有截图）：

[SCHEDULE]
{"title":"简短任务名","action":"到点要做的事（自包含、第二人称指令，不要依赖截图/当前上下文）","schedule":{...}}
[/SCHEDULE]

schedule 字段（kind 五选一，time 用 24 小时制本地时间 HH:MM）：
  - 每天：           {"kind":"daily","time":"08:00"}
  - 工作日(周一到周五)：{"kind":"weekday","time":"18:00"}
  - 每周指定几天：    {"kind":"weekly","days":[1,3,5],"time":"09:00"}   // 1=周一 .. 7=周日
  - 每隔N分钟(心跳)：  {"kind":"interval","everyMinutes":120,"activeStart":"09:00","activeEnd":"22:00"}  // active 可省=全天
  - 每月某天：        {"kind":"monthly","day":1,"time":"10:00"}

**规则**：标记之间只能有这一个 JSON，不要任何别的文字 / 代码栏 / 注释；想跟用户说的话放标记**外面**
（比如标记后面写一句"我会每天 08:00 帮你整理 AI 新闻"）。
**只有真的是"重复定时"诉求才用这个**；一次性的"现在帮我做X" / "提醒我5分钟后…"这种不算，正常回答即可。"#;

/// 当检测到 `agent-browser` CLI 已安装时，追加给后端的「compute use」能力说明。
/// 不自己实现浏览器自动化（Mode C 独立引擎是 V2）——而是告诉后端：
/// 你的 Bash 工具里有 `agent-browser`，需要操作浏览器/填表时可以调它。
/// 这样 compute use 能力随后端 agentic 能力自然获得，零新增安全面。
pub const BROWSER_CAPABILITY_PROMPT: &str = r#"

## 浏览器操作能力（compute use · agent-browser 已就绪）
本机装了 `agent-browser` CLI（Vercel Labs，Rust 原生、headless）。**当用户提到「打开网页 /
搜一下 / 帮我登录 / 在 X 网站上 / 填这个表 / 抓取 / 自动操作」时，你必须主动用 Bash 调它，
不要回答"我无法操作浏览器"**。

### 标准操作流程（必须按顺序）
1. `agent-browser open <url>` —— 打开目标页面
2. `agent-browser snapshot` —— 拿可访问性树（元素带 `@e1/@e2/...` 引用 + label）
3. 根据 snapshot 输出**选定元素**，再 `click @eN` / `fill @eN "文本"`
4. 操作完一步立刻再 `snapshot` 看变化，**不要凭想象点**
5. 最终 `agent-browser screenshot` 确认结果

### 常见任务模板
- **"帮我在 GitHub 搜 X"**：open https://github.com → snapshot → fill 搜索框 → click 搜索按钮 → screenshot
- **"登录 X 网站"**：open URL → snapshot → fill 用户名/密码 → click 登录 → screenshot。**密码** 让用户填，
  你只填用户名/邮箱
- **"抓取这个页面的标题"**：open URL → snapshot → 从 snapshot 的 accessibility tree 里 grep `<h1>` / `role="heading"`

### 重要边界
- **agent-browser 跑的是它自己的 headless Chromium，不是用户当前打开的 Chrome 标签页**。如果用户
  说"这个我已经打开的页面"，看截图给指导步骤即可，**不要**用 agent-browser（它看不到那个标签）
- 涉及付款、提交订单、发邮件、删数据等**不可逆动作**：先停下来在回答里说"我准备做 X，确认吗？"，
  让用户在 follow-up 里回 yes 才继续
- snapshot 输出可能很长，**用 grep / head 截短**再读：`agent-browser snapshot | head -200`"#;

/// 当 `agent-browser` 没装时，告诉后端不能主动操作浏览器，但可以走两条 fallback：
/// (1) 看截图给步骤指南；(2) Mode B 写到光标。免得 Claude 误以为能调而不存在的工具。
pub const NO_BROWSER_CAPABILITY_PROMPT: &str = r#"

## 浏览器操作能力（未启用）
本机**目前没启用浏览器自动化** —— Chrome 没在 CDP debug 模式下跑，也没装 agent-browser。
所以你**不能主动**打开新页面、点按钮、抓取网站。当用户要求浏览器自动化时，按下面 fallback：

1. **截图里看得到目标网页** → 给清晰的"点哪里、填什么"步骤指南，告诉用户自己操作。结尾加一句：
   "想让我直接帮你点，去托盘菜单点「🌐 启用浏览器自动化」就行 —— 一次配置永久生效"
2. **用户已经把光标放在某个字段里、想让你生成文本去填** → 用 `[INSERT_AT_CURSOR]...[/INSERT_AT_CURSOR]`
   走 Mode B 写进去（这条路不依赖浏览器自动化）

不要假装会用浏览器工具 —— 调用会直接失败，对用户毫无帮助。"#;

/// 当用户的 Chrome 以 CDP 模式跑着（`browser_bridge::cdp_is_alive() == true`）+
/// chrome-devtools MCP 已注册时，把 MCP 工具说明塞进 prompt。
///
/// 这是 daily-use 的真正解 —— 不是 headless 的新 Chromium，而是用户**本人正在用**的 Chrome：
/// 已登录的网站、已开的标签、cookie/session 全在，Claude 可以直接接管。
pub const CHROME_CDP_CAPABILITY_PROMPT: &str = r#"

## 浏览器操作能力（MCP · 已连到用户实时 Chrome）
本机的 Chrome 正在 CDP debug port 9222 上跑，并且 `chrome-devtools-mcp` 已经注册到 Claude。
**这意味着你能直接通过 MCP tools 操作用户当前打开的真 Chrome 窗口** —— 包含他所有的登录态、
cookies、已开的标签。这跟启动新 Chromium 完全不同。

### 触发条件 & 任务模板
用户说"在浏览器里…"、"帮我点…"、"在这个网站上…"、"自动填表…" → **必须**用 chrome-devtools MCP 工具。
典型流程：
1. `mcp__chrome-devtools__list_pages` —— 看用户当前开的标签页
2. `mcp__chrome-devtools__select_page` —— 选目标 tab（通常是 frontmost）
3. `mcp__chrome-devtools__take_snapshot` —— 拿可访问性树（带 uid）
4. 根据 snapshot 的 uid，用 `mcp__chrome-devtools__click` / `fill` / `fill_form`
5. `mcp__chrome-devtools__take_screenshot` 截图确认 / 给用户看

### 重要约束
- **绝对不要随便 `navigate` 走用户当前页**。除非用户明确说"打开 X 网站"，否则只在当前 tab 操作。
- 不可逆动作（提交订单、发邮件、删数据、汇款）→ 先停下来在回答里说"我准备点 X 按钮，确认吗？"
  让用户在 follow-up 里回 yes 再继续。
- 表单里碰到**密码字段**永远跳过 —— 用户必须自己填。
- 看不懂 snapshot 的语义就再 take_snapshot 一次，**不要凭想象点 uid**。"#;

/// macOS 系统级 computer use（v0.1.19）—— 教 AI 它能调系统功能，不是只回答
///
/// 用户提出诉求：「指着地名说去这里要开 Maps」「指着网页说转 Word 打开」等等。
/// 这些**早就**做得到（Claude 的 Bash 工具就能跑 open/osascript），但之前 prompt 里
/// 没明说，AI 见到模糊指令偏向「解释」而不是「执行」。这一段把能力列清楚 +
/// 标准触发词 + 安全护栏。
///
/// 关键设计：
///   - 列**触发词**让 AI 一眼识别意图（"去这里"/"打开"/"加入日历"/"发邮件给"…）
///   - 列**手段**：open URL scheme / osascript / shortcuts run / pandoc / textutil
///   - **不可逆**动作（发送/删除/付款/打电话）一律先在回答里 confirm
pub const MACOS_COMPUTER_USE_PROMPT: &str = r#"

## macOS 系统操作能力（你直接动手，别只解释）
你在 macOS 上跑，Bash 工具可用。**当用户的诉求是「动作」时，立即执行 + 简短报告，不要只描述、不要问废话。**

### 标准触发词 → 标准动作
- **「去这里」/「导航到」/「在 Maps 里看」+ 截图里有地名** → 立即跑：
    open "maps://?q=URL_ENCODED_PLACE_NAME"
  Apple Maps 会启动并搜索。气泡里只回「✅ 已在地图打开 <地名>」。
- **「打开 X 网站」/「在浏览器打开」** → `open "https://..."`
- **「发邮件给 X」/「写邮件」** → `open "mailto:X?subject=...&body=..."`（已知用户希望草稿；不要直接发）
- **「打个电话给 X」** → `open "tel:NUMBER"` 或 `open "facetime://NUMBER"`
- **「加日历事件 X 在 Y」** → `osascript -e 'tell application "Calendar" ...'` 创建事件
- **「提醒我 X 在 Y」** → `osascript -e 'tell application "Reminders" ...'` 加 reminder
- **「跑 Shortcut X」/「触发快捷指令 X」** → `shortcuts run "X"`
- **「打开终端跑 X」** → 先 confirm，再 `open -a Terminal --args ...` 或写 .command 脚本 → open
- **「把这个网页转 Word 并打开」**（用户指向 Chrome）：
    1. 先用 chrome-devtools MCP 拿当前 tab URL（如果 CDP 活着）
       OR: osascript -e 'tell application "Google Chrome" to get URL of active tab of front window'
    2. 用 pandoc 转：`pandoc "<URL>" -o /tmp/page.docx`（pandoc 可能没装 —— 装不上就降级 textutil）
    3. open /tmp/page.docx → 默认 Word/Pages 接管
- **「把屏幕内容存成 PDF」** → `screencapture -t pdf ~/Desktop/screen.pdf && open ~/Desktop/screen.pdf`
- **「打开 Finder 到 X 路径」** → `open ~/Documents` / `open /Applications`
- **「截图 + 复制到剪贴板」** → `screencapture -c -i` （交互区域截图直接进剪贴板）

### 优先级铁律
1. **能 action 就 action，别先问「您是不是想…」** —— 用户既然按了快捷键说话，就是要你动手
2. **不可逆动作必须先 confirm**：发送 / 删除文件 / 付款 / 打电话 / 群发邮件 / 关机 / 推码
   → 回答里写「我准备 X，回 'yes' 确认」，等 follow-up 再做
3. **可逆动作直接做**：打开 app / 加草稿 / 复制到剪贴板 / 临时文件 —— 错了 ⌘Z 或删文件就行
4. **报告要短**：动作完成 → 一行（✅ 已...）；动作失败 → 一行（❌ 原因）+ 给手动命令

### 工具备忘
- `open -a "Application Name"` 启动 app（无参）
- `open "URL_or_scheme://..."` 跟一个 URL/scheme
- `open <文件>` 用默认 app 打开
- `osascript -e '...'` 一行 AppleScript / `osascript script.scpt` 跑文件
- `shortcuts run "Name"` / `shortcuts list` 看用户配过的快捷指令
- `pbcopy` / `pbpaste` 读写剪贴板
- `screencapture` 截屏（`-i` 交互，`-c` 入剪贴板，`-t pdf` 指定格式）
- `say "text"` 朗读
"#;

/// 当检测到 `officecli` 已安装时，告诉后端：你的 Bash 里有 OfficeCLI，
/// 可以读写 Word / Excel / PowerPoint，不需要装 Office、不需要登录。
///
/// 跟 agent-browser 一个套路 —— 不自己实现 Office 操作，让后端 agentic 能力调它。
/// 来源 https://github.com/iOfficeAI/OfficeCLI（自包含二进制，无账号）。
pub const OFFICE_CAPABILITY_PROMPT: &str = r#"

## Office 文档操作能力（OfficeCLI 已就绪）
本机装了 `officecli` 单二进制（自包含 .NET runtime，**不需要装 Office**、不需要登录账号）。
**当用户提到「读这个 Excel / 改这个 Word / 总结 PPT / 把这段写进 docx / 把这个表换个格式」时，
你必须主动用 Bash 调它，不要回答"我无法操作 Office 文件"**。

### 常用子命令（其它子命令用 `officecli --help` / `officecli <cmd> --help` 自查）
- `officecli read <file>` —— 读 Word/Excel/PPT 文本内容
- `officecli edit <file> ...` —— 改 Word 文档内容
- `officecli convert <input> <output>` —— 格式互转（docx↔pdf↔md 等）
- 用户拖来的文件路径若是相对路径，先 `cd` 到合适目录或用绝对路径

### 标准任务模板
- **"总结这个 Excel"** → `officecli read /path/to/file.xlsx | head -200` → 看完给 3 句摘要
- **"把这段写进 docx"** → 拿用户给的文本 + 路径 → `officecli edit ...`
- **"把这份 Word 转成 PDF"** → `officecli convert in.docx out.pdf` → `open out.pdf`

### 重要边界
- **覆盖原文件前先 confirm**（"我要把改动写回 `<原路径>`，确认吗？"）—— Office 文档不可 ⌘Z
- 文件特别大时先 `read | head -300` 取一段看结构，再决定下一步
- 路径里有空格 / 中文记得引号"#;

/// 当 `officecli` 没装时，告诉后端不能直接操作 Office 文件，但可以走 fallback：
/// (1) 看截图给操作步骤；(2) 引导用户去状态页一键装。
pub const NO_OFFICE_CAPABILITY_PROMPT: &str = r#"

## Office 文档操作能力（未启用）
本机**没装 OfficeCLI** —— 不能直接读写 Word/Excel/PPT。当用户要求操作 Office 文档时按 fallback：

1. **截图里看得到文档内容** → 给清晰的"在 X 处改成 Y"步骤指南，告诉用户自己改。结尾加一句：
   "想让我直接帮你改，去托盘 → 系统状态 → 装上 OfficeCLI 就行（无需账号，一次装永久生效）"
2. **纯文本类的转换** → 走 Mode B 把目标文本写到光标（用户自己粘进 Word）

不要假装会调 officecli —— 调用会直接失败。"#;

/// 按本机已安装的能力拼出最终 system prompt。
/// macOS 上永远注入 computer-use prompt；浏览器 / Office 层按可用性各自分支。
pub fn system_prompt() -> String {
    let mut p = APPEND_SYSTEM_PROMPT.to_string();
    // v0.1.19 · macOS 系统级 computer use 永远开（Claude 默认就有 Bash 工具）
    #[cfg(target_os = "macos")]
    {
        p.push_str(MACOS_COMPUTER_USE_PROMPT);
    }
    let cdp_alive = crate::browser_bridge::cdp_is_alive();
    let has_agent_browser = find_binary("agent-browser").is_ok();
    if cdp_alive {
        p.push_str(CHROME_CDP_CAPABILITY_PROMPT);
    }
    if has_agent_browser {
        p.push_str(BROWSER_CAPABILITY_PROMPT);
    }
    if !cdp_alive && !has_agent_browser {
        p.push_str(NO_BROWSER_CAPABILITY_PROMPT);
    }
    // v0.4.x · Office Use
    if find_binary("officecli").is_ok() {
        p.push_str(OFFICE_CAPABILITY_PROMPT);
    } else {
        p.push_str(NO_OFFICE_CAPABILITY_PROMPT);
    }
    p
}

/// 光标在截图坐标系里的位置 + 屏幕尺寸（logical points, top-left origin）。
pub struct CursorContext {
    pub x: i32,
    pub y: i32,
    pub screen_w: i32,
    pub screen_h: i32,
}

/// `.app` bundle 启动时 PATH 默认是 launchd 给的最小集（`/usr/bin:/bin:/usr/sbin:/sbin`）
/// 不会继承用户 shell 的 `.zshrc` 等。这导致 `claude` CLI 找不到。
///
/// `.env("PATH", ...)` 只影响**子进程**看到的 PATH，不影响 OS 找二进制 ——
/// OS 在 spawn 之前已经用**当前进程**的 PATH 找 `claude` 了。所以必须先
/// 自己用拓宽过的 PATH 找到二进制的绝对路径，再用绝对路径 spawn。
/// 给 AI 子进程降优先级（nice +10），让本地 ASR（sherpa）/ UI 抢得到 CPU。
///
/// v0.4 fix (2026-05-20)：用户实测 AI 子进程（claude / codex …）默认优先级会把
/// 语音输入法的实时解码饿到卡顿。nice +10 只在 CPU 紧张时让出 —— 空闲时 niced 进程
/// 仍拿满 CPU，所以对单任务速度无影响，只在并发时保护听写流畅。
#[cfg(unix)]
pub fn lower_priority(cmd: &mut tokio::process::Command) {
    unsafe {
        cmd.pre_exec(|| {
            // setpriority(PRIO_PROCESS, who=0=self, prio=10)；数值越大优先级越低。
            // 失败也无所谓（返回 -1）—— 不影响子进程正常跑。
            libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            Ok(())
        });
    }
}
#[cfg(not(unix))]
pub fn lower_priority(_cmd: &mut tokio::process::Command) {}

pub fn expanded_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let extras = [
        format!("{home}/.npm-global/bin"),
        format!("{home}/.bun/bin"),
        format!("{home}/.cargo/bin"),
        format!("{home}/.local/bin"),
        // v0.1.23 · Hermes 安装 ~/.hermes 但 binary 也在 ~/.local/bin（上面已包含）
        // 这里只确保官方路径都覆盖；hermes 真实路径就是 ~/.local/bin/hermes
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
    ];
    let mut paths: Vec<String> = current.split(':').map(String::from).collect();
    for p in extras {
        if !p.is_empty() && !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths.join(":")
}

/// 在拓宽过的 PATH 里查找任意 CLI 二进制，返回绝对路径。
/// pub —— backend.rs 的 codex / openclaw 后端也用它定位自己的二进制。
/// 找不到时报错文本里列出所有搜过的目录方便排查。
pub fn find_binary(name: &str) -> Result<std::path::PathBuf> {
    let path = expanded_path();
    let mut searched = Vec::new();
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::PathBuf::from(dir).join(name);
        searched.push(candidate.display().to_string());
        if candidate.exists() && std::fs::metadata(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "找不到 `{name}` CLI。\n已搜索的路径：\n  {}",
        searched.join("\n  ")
    )
}

/// 在拓宽过的 PATH 里查找 `claude` 二进制。
fn find_claude_binary() -> Result<std::path::PathBuf> {
    find_binary("claude").map_err(|e| {
        anyhow::anyhow!("{e}\n装一下：npm install -g @anthropic-ai/claude-code")
    })
}

/// 拼接发给 AI 的最终 prompt —— pub，backend.rs 各后端共用同一套格式。
pub fn build_prompt_pub(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
    trail_summary: Option<&str>,
) -> String {
    build_prompt(transcript, image_path, frontmost_app, cursor, trail_summary)
}

/// 拼接发给 Claude 的最终 prompt（transcript + 截图路径 + 光标位置 + 前台 app + 鼠标轨迹）。
fn build_prompt(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
    trail_summary: Option<&str>,
) -> String {
    let context_line = frontmost_app
        .map(|t| format!("\n上下文窗口：{t}"))
        .unwrap_or_default();
    let cursor_line = cursor
        .map(|c| {
            let pct_x = (c.x as f32 / c.screen_w.max(1) as f32 * 100.0).round() as i32;
            let pct_y = (c.y as f32 / c.screen_h.max(1) as f32 * 100.0).round() as i32;
            format!(
                "\n光标位置（截图坐标系 top-left, 单位 logical points）：x={}, y={} \
                 — 即屏幕的横向 {}%、纵向 {}%。屏幕尺寸：{}×{}。",
                c.x, c.y, pct_x, pct_y, c.screen_w, c.screen_h
            )
        })
        .unwrap_or_default();
    // v0.1.21 · 工作区路径 → AI 知道在哪个项目里读写
    let workspace_line = crate::config::Config::load().workspace_path
        .map(|p| format!("\n\n📁 当前工作区：{p}\n\
             你的 Read/Write/Edit/Bash/Grep 工具默认以此为根，可以直接读改这里面的文件。"))
        .unwrap_or_default();
    let trail_line = trail_summary.map(|s| {
        let mut out = String::new();
        out.push_str("\n\n🐭 鼠标标注（v0.1.20）：");
        out.push_str(s);
        out.push_str("\n⚠️ 截图上的粉红色线 / 端点圆都是用户在按住快捷键期间画的标注，");
        out.push_str("用鼠标圈选他要让你重点看的东西。粉红色越粗 = 用户越重点强调。");
        out.push_str("灰色细线是辅助轨迹（光标移动路径，背景信息）。");
        out.push_str("你的回答应该围绕粉红色标注覆盖的内容展开。");
        out
    }).unwrap_or_default();
    format!(
        "{transcript}\n\n截图位置：{}{cursor_line}{context_line}{workspace_line}{trail_line}",
        image_path.display()
    )
}

/// Claude CLI 流式调用 —— 边收边把累计文本喂给 `on_chunk`，结束返回完整文本。
///
/// 用 `--output-format stream-json --include-partial-messages --verbose`，stdout 是
/// NDJSON：每行一个 JSON 事件。我们只关心
/// `stream_event` → `content_block_delta` → `text_delta` → `.text` 这种增量文本块，
/// 累加后每收到一块就回调 `on_chunk(累计文本)`，让前端气泡实时长出来。
///
/// `on_chunk` 收到的是**累计**文本（不是单个 delta），调用方直接拿去 emit 即可。
/// Claude 工具名 → 用户能懂的中文活动标签。
fn friendly_tool_label(tool: &str) -> String {
    match tool {
        "Read"      => "读取文件…".into(),
        "Write"     => "写文件…".into(),
        "Edit"      => "修改文件…".into(),
        "Bash"      => "运行命令…".into(),
        "Grep"      => "搜索代码…".into(),
        "Glob"      => "查找文件…".into(),
        "WebFetch"  => "读取网页…".into(),
        "WebSearch" => "搜索网络…".into(),
        "Task"      => "调度子任务…".into(),
        other       => format!("{other}…"),
    }
}

pub async fn ask_claude_streaming<F, G>(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
    trail_summary: Option<&str>,
    mut on_chunk: F,
    mut on_status: G,
) -> Result<String>
where
    F: FnMut(&str),
    G: FnMut(&str),
{
    let prompt = build_prompt(transcript, image_path, frontmost_app, cursor, trail_summary);
    let sys_prompt = system_prompt();
    let claude_bin = find_claude_binary()?;

    let mut cmd = tokio::process::Command::new(&claude_bin);
    cmd.env("PATH", expanded_path());
    // v0.4 · 降优先级 —— 保护并发时本地语音输入法 ASR 的 CPU
    lower_priority(&mut cmd);
    // v0.1.25 · provider.env 里的 ANTHROPIC_* / 自定义 base URL 也透给 Claude CLI
    crate::provider_env::apply_to(&mut cmd);
    // v0.1.21 · 工作区 cwd —— 让 Claude 知道在哪个项目里读/改文件
    if let Some(ws) = crate::config::Config::load().workspace_path {
        if std::path::Path::new(&ws).is_dir() {
            cmd.current_dir(&ws);
            println!("[mouseclaw] 📁 claude cwd = {ws}");
        }
    }
    let mut child = cmd
        .args([
            "-p",
            &prompt,
            "--allowedTools",
            "Read,Write,Edit,Bash,Grep,Glob",
            "--permission-mode",
            "auto",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--verbose",
            "--append-system-prompt",
            &sys_prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", claude_bin.display()))?;

    let stdout = child.stdout.take().context("claude stdout not piped")?;
    // stderr 先 take 出来，结束时如果失败再读 —— 用 tokio 的 AsyncRead，不碰 raw fd
    let mut stderr = child.stderr.take();
    let mut reader = BufReader::new(stdout).lines();
    let mut accumulated = String::new();
    let mut thinking_buf = String::new();

    while let Some(line) = reader.next_line().await.context("read claude stdout")? {
        if line.trim().is_empty() {
            continue;
        }
        // 每行是一个 JSON 事件；解析失败的行直接跳过（容错）
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let top_type = v.get("type").and_then(|t| t.as_str());

        // ① stream_event → content_block_delta：可能是最终文本 text_delta，
        //    也可能是 thinking_delta（扩展思考）—— 后者喂 on_status 显示"💭 思考中"。
        if top_type == Some("stream_event") {
            let ev = &v["event"];
            let ev_type = ev.get("type").and_then(|t| t.as_str());
            if ev_type == Some("content_block_delta") {
                let delta = &ev["delta"];
                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            accumulated.push_str(text);
                            on_chunk(&accumulated);
                        }
                    }
                    Some("thinking_delta") => {
                        // Claude 扩展思考 —— 累计思考文本，取尾部一小段当状态行
                        if let Some(th) = delta.get("thinking").and_then(|t| t.as_str()) {
                            thinking_buf.push_str(th);
                            let tail: String = thinking_buf.chars()
                                .rev().take(48).collect::<Vec<_>>()
                                .into_iter().rev().collect();
                            on_status(&format!("💭 {}", tail.trim()));
                        }
                    }
                    _ => {}
                }
            } else if ev_type == Some("content_block_start") {
                // ② tool_use 开始 → 报"🔧 正在用某工具"
                let cb = &ev["content_block"];
                if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let tool = cb.get("name").and_then(|t| t.as_str()).unwrap_or("工具");
                    on_status(&format!("🔧 {}", friendly_tool_label(tool)));
                }
            }
        }
    }

    let status = child.wait().await.context("wait claude")?;
    if !status.success() {
        let mut err_text = String::new();
        if let Some(ref mut s) = stderr {
            use tokio::io::AsyncReadExt;
            let _ = s.read_to_string(&mut err_text).await;
        }
        bail!("claude exited {}: {}", status, err_text.trim());
    }

    let trimmed = accumulated.trim().to_string();
    if trimmed.is_empty() {
        bail!("claude 没有返回任何文本（可能 stream-json 格式变了或调用被拒）");
    }
    Ok(trimmed)
}

/// Claude CLI 非流式调用 —— `ask_claude_streaming` 的 wrapper，丢弃增量回调。
/// smoke test / 不需要流式的场景用。
pub async fn ask_claude(
    transcript: &str,
    image_path: &Path,
    frontmost_app: Option<&str>,
    cursor: Option<&CursorContext>,
) -> Result<String> {
    ask_claude_streaming(transcript, image_path, frontmost_app, cursor, None, |_| {}, |_| {}).await
}

/// 把 Mode B inner text 里 Claude 常加的装饰剥掉 —— prompt 已经禁止过，但 LLM 经常忘。
/// 处理：
///   - 外围 ``` / ```lang 代码栏（首尾）
///   - 整段被英文/中文引号包裹（首尾）
///   - 多余的前后空行 / `>` 引用符
fn strip_insert_decorations(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    // 代码栏
    if s.starts_with("```") {
        if let Some(first_nl) = s.find('\n') {
            s = s[first_nl + 1..].to_string();
        }
        if s.ends_with("```") {
            s.truncate(s.len() - 3);
        }
        s = s.trim().to_string();
    }
    // 整段引号包裹
    let quote_pairs: [(&str, &str); 4] = [
        ("\"", "\""), ("'", "'"), ("“", "”"), ("「", "」"),
    ];
    for (open, close) in quote_pairs {
        if s.starts_with(open) && s.ends_with(close) && s.len() > open.len() + close.len() {
            s = s[open.len()..s.len() - close.len()].trim().to_string();
        }
    }
    // 引用符行首
    s.lines()
        .map(|l| l.trim_start_matches('>').trim_start().to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// 解析回复是否是 Mode B（含 `[INSERT_AT_CURSOR]` 标记），返回要写入光标的纯文本（去掉标记 + 装饰）。
/// 若是 Mode A，返回 `None`。空内容也返回 `None` —— 避免 Mode B 写入空串误触发 UI。
pub fn parse_insert_directive(reply: &str) -> Option<String> {
    const OPEN: &str = "[INSERT_AT_CURSOR]";
    const CLOSE: &str = "[/INSERT_AT_CURSOR]";
    let start = reply.find(OPEN)? + OPEN.len();
    let rest = &reply[start..];
    let end = rest.find(CLOSE)?;
    let inner = strip_insert_decorations(&rest[..end]);
    if inner.is_empty() { None } else { Some(inner) }
}

/// v0.5 · 解析回复里的 `[SCHEDULE]...[/SCHEDULE]` 标记，返回里面的 JSON 字符串。
/// 只取第一个块（防多块）。去掉可能的 ``` 代码栏装饰。无标记 → None。
pub fn parse_schedule_directive(reply: &str) -> Option<String> {
    const OPEN: &str = "[SCHEDULE]";
    const CLOSE: &str = "[/SCHEDULE]";
    let start = reply.find(OPEN)? + OPEN.len();
    let rest = &reply[start..];
    let end = rest.find(CLOSE)?;
    let mut inner = rest[..end].trim().to_string();
    // 去掉 LLM 偶尔加的 ```json 代码栏
    if inner.starts_with("```") {
        if let Some(nl) = inner.find('\n') {
            inner = inner[nl + 1..].to_string();
        }
        if inner.ends_with("```") {
            inner.truncate(inner.len() - 3);
        }
        inner = inner.trim().to_string();
    }
    if inner.is_empty() { None } else { Some(inner) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_a_returns_none() {
        assert!(parse_insert_directive("这段代码的 bug 在第 13 行……").is_none());
    }

    #[test]
    fn mode_b_extracts_inner_text() {
        let reply = "我帮你续写。\n[INSERT_AT_CURSOR]\n第二天清晨，雪停了。\n[/INSERT_AT_CURSOR]";
        assert_eq!(
            parse_insert_directive(reply).as_deref(),
            Some("第二天清晨，雪停了。")
        );
    }

    #[test]
    fn mode_b_strips_code_fence() {
        let reply = "好的：[INSERT_AT_CURSOR]\n```\n第二天清晨，雪停了。\n```\n[/INSERT_AT_CURSOR]";
        assert_eq!(parse_insert_directive(reply).as_deref(), Some("第二天清晨，雪停了。"));
    }

    #[test]
    fn mode_b_strips_outer_chinese_quotes() {
        let reply = "[INSERT_AT_CURSOR]「他望着窗外。」[/INSERT_AT_CURSOR]";
        assert_eq!(parse_insert_directive(reply).as_deref(), Some("他望着窗外。"));
    }

    #[test]
    fn mode_b_strips_quote_marker_lines() {
        let reply = "[INSERT_AT_CURSOR]\n> 第一行\n> 第二行\n[/INSERT_AT_CURSOR]";
        assert_eq!(parse_insert_directive(reply).as_deref(), Some("第一行\n第二行"));
    }

    #[test]
    fn mode_b_empty_inner_returns_none() {
        // 空内容不应误触发 Mode B
        assert!(parse_insert_directive("[INSERT_AT_CURSOR]   \n  [/INSERT_AT_CURSOR]").is_none());
    }

    #[test]
    fn mode_b_takes_first_block() {
        let reply = "[INSERT_AT_CURSOR]A[/INSERT_AT_CURSOR][INSERT_AT_CURSOR]B[/INSERT_AT_CURSOR]";
        assert_eq!(parse_insert_directive(reply).as_deref(), Some("A"));
    }

    #[test]
    fn schedule_directive_extracts_json() {
        let reply = "好的，我帮你设好。[SCHEDULE]\n{\"title\":\"AI 新闻\",\"action\":\"收集\",\"schedule\":{\"kind\":\"daily\",\"time\":\"08:00\"}}\n[/SCHEDULE]\n我会每天 08:00 跑。";
        let json = parse_schedule_directive(reply).expect("应解析出 schedule JSON");
        assert!(json.contains("\"kind\":\"daily\""));
        assert!(!json.contains("[SCHEDULE]"));
    }

    #[test]
    fn schedule_directive_strips_code_fence() {
        let reply = "[SCHEDULE]\n```json\n{\"a\":1}\n```\n[/SCHEDULE]";
        assert_eq!(parse_schedule_directive(reply).as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn no_schedule_directive_returns_none() {
        assert!(parse_schedule_directive("这段代码的 bug 在第 13 行").is_none());
    }

    #[test]
    fn system_prompt_always_includes_base_and_browser_branch() {
        // 不管哪种状态，system_prompt 都必须告诉 Claude 它的"能 / 不能"边界。
        // 三档：CDP 活着 / agent-browser 装了 / 都没有
        let p = system_prompt();
        assert!(p.starts_with("你是 MouseClaw 桌面助手"));
        let cdp_alive = crate::browser_bridge::cdp_is_alive();
        let has_ab = find_binary("agent-browser").is_ok();
        if cdp_alive {
            assert!(p.contains("已连到用户实时 Chrome"));
        }
        if has_ab {
            assert!(p.contains("agent-browser 已就绪"));
        }
        if !cdp_alive && !has_ab {
            assert!(p.contains("未启用"));
        }
        // v0.1.19 · macOS 上必含 computer-use prompt
        #[cfg(target_os = "macos")]
        {
            assert!(p.contains("macOS 系统操作能力"));
            assert!(p.contains("maps://?q="));
            assert!(p.contains("不可逆动作必须先 confirm"));
        }
        // v0.4.x · Office Use 分支必须二选一出现
        let has_office = find_binary("officecli").is_ok();
        if has_office {
            assert!(p.contains("OfficeCLI 已就绪"));
        } else {
            assert!(p.contains("没装 OfficeCLI"));
        }
    }

    #[test]
    fn mode_b_prompt_warns_about_decoration() {
        // Mode B 失败的主因是 Claude 在 INSERT 标记里加了多余的解释/Markdown —— prompt 必须显式禁止
        let p = APPEND_SYSTEM_PROMPT;
        assert!(p.contains("不能有任何解释"));
        assert!(p.contains("[INSERT_AT_CURSOR]"));
    }

    #[test]
    fn build_prompt_includes_image_path_and_cursor() {
        let cursor = CursorContext { x: 100, y: 50, screen_w: 1000, screen_h: 500 };
        let p = build_prompt(
            "这段代码有 bug 吗",
            Path::new("/tmp/frame.png"),
            Some("Visual Studio Code"),
            Some(&cursor),
            None,
        );
        assert!(p.contains("/tmp/frame.png"));
        assert!(p.contains("x=100"));
        assert!(p.contains("Visual Studio Code"));
        assert!(p.contains("10%")); // 100/1000
    }
}
