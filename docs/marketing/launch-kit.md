# 🦞 MouseClaw — Launch / Promotion Kit

> Viral-leaning copy for HN · Reddit · X · YouTube · Instagram · 小红书.
> EN + 中文 side-by-side. Calibrated to each platform's 2026 conventions
> (HN sobriety, 小红书 emoji-rich note format, IG 5-hashtag cap, X thread
> structure, YouTube SEO front-loading).
>
> **Assets to attach:**
> - `docs/assets/demo-en.mp4` (35s · 1920×1080 · ~4.5 MB · EN)
> - `docs/assets/demo-zh.mp4` (35s · 1920×1080 · ~4.3 MB · 中文)
> - `docs/assets/demo-en.gif` / `demo-zh.gif` (~3.3 MB each, for chat
>   embeds where MP4 doesn't autoplay)
> - `docs/assets/social-preview.png` (1280×640 · GitHub social preview /
>   X card / OG image)
> - `docs/assets/hero.png` (1920×1080 · README hero)

---

## Posting cadence (recommended)

| Day | Platform | What to post |
|---|---|---|
| Mon | 小红书 + Instagram | 笔记 + Reel — soft awareness pass first |
| Tue 9–11am ET | **Hacker News + X thread** | Show HN + thread; HN traffic catches the X thread |
| Wed | Reddit (`r/macapps` + `r/SideProject`, stagger 4h) | Per-sub rewrites — never literal cross-post |
| Thu | YouTube Short (EN) + YouTube Short (中文) | Re-uploaded 9:16 crops of the 35-s demo |
| Fri | LinkedIn (optional) | Founder-mode "what shipped this week" |

### Cross-platform rules of thumb

- **Reply to every comment in the first 4 hours.** HN, Reddit, X, and
  小红书 all weight early engagement very heavily.
- **小红书 doesn't allow inline links in the body.** Direct users to
  "Google MouseClaw github" or your bio link.
- **Do not cross-post on Reddit.** Different subs ≠ cross-post — write
  a fresh body each time (mods see verbatim duplicates as spam).
- **HN: be candid about what doesn't work yet** (no Intel, no Windows,
  OpenClaw `--local` partial). HN punishes salesy framing and rewards
  honest gaps + a request for feedback on a specific decision.
- **Instagram 2026:** hashtag cap is 5. Keywords in the caption do
  more for discovery than hashtags now.

---

## 1. Hacker News · Show HN (English only)

> HN audience is English; don't post a Chinese variant on news.ycombinator.

### Title (≤ 80 chars · no emoji · no caps · no clickbait)

```
Show HN: MouseClaw – a pixel-art desktop AI pet for macOS (hold to speak)
```

### First comment / body — post yourself right after submitting

```
Hi HN — I built MouseClaw because I wanted to ask my AI about whatever
my cursor was on, without alt-tabbing into ChatGPT and re-explaining
context.

How it works:
- Hold ⌘⇧Space. It screenshots the display your cursor is on, records
  you talking, and pipes both into one of four backends: Claude Code
  CLI (default), OpenAI Codex CLI, OpenClaw, or Hermes Agent.
- Drag while holding the shortcut and your cursor leaves a pink trail
  that bakes into the screenshot, so you can literally circle the
  thing you mean — works really well for "what is this in the photo"
  or "what's wrong with this paragraph".
- Long-press fn turns the pet into a voice IME — you speak, sherpa-onnx
  transcribes locally (streaming Zipformer, zh-en), and the text is written via
  CGEventKeyboardSetUnicodeString directly to the focused field
  (bypasses the active IME, so 中文 input methods don't intercept).
- ⌘⇧V opens a clipboard hub with everything you've copied, AES-256-GCM
  encrypted on disk, key in macOS Keychain.

Stack: Tauri 2, Rust backend, React/TS for the webview, sherpa-onnx
(streaming Zipformer zh-en + CT-Transformer punctuation) bundled in the
.app so first launch is offline. DMG is signed + notarized. Apple Silicon only for now.

Two things I'd love feedback on:
1. The "cursor trail as context" interaction — I haven't seen it used
   elsewhere and I'm curious whether it reads as obvious or weird.
2. The 4-backend abstraction in src-tauri/src/backend.rs — provider
   selection lives in ~/.mouseclaw/provider.env so you can point any
   non-Claude backend at a Vercel AI Gateway / OpenRouter / etc.
   without touching shell rc.

Repo (DMG in releases): https://github.com/edwin-hao-ai/MouseClaw
Demo (35 s, no sound): https://github.com/edwin-hao-ai/MouseClaw/raw/main/docs/assets/demo-en.mp4
```

**Post Tue–Thu 09:00–11:00 ET. Stay in the thread for the next 3 hours.**

---

## 2. Reddit

### 2.1 `r/macapps` (English · promo-friendly · keep it modest)

**Title:**

```
I built a pixel-art AI pet that lives next to your cursor — free, notarized, open-source
```

**Body:**

```
After getting sick of cmd-tabbing into ChatGPT every time I wanted to
ask about something on screen, I built MouseClaw — a little pixel
mouse that lives in your menu bar and pops up next to your cursor
when you hold ⌘⇧Space.

The fun bit: hold the shortcut and drag your mouse, and a pink trail
"draws" on the screenshot — so you can circle the thing you want to
ask about. Then the AI (Claude Code by default, or OpenAI Codex /
Hermes / OpenClaw) answers with the screenshot + your voice as context.

What's in v0.1.25:
- DMG is notarized (no Gatekeeper warning — just drag to Applications)
- sherpa-onnx speech models bundled (zh-en ASR + punctuation, first launch is offline)
- 6 pet skins (classic gray / lab white / ninja / robot / golden / brown)
- Voice IME: long-press fn → speak → text appears at your cursor in
  any app (bypasses IME so 中文 input methods don't intercept)
- ⌘⇧V clipboard hub, AES-256 encrypted at rest

GitHub (DMG in releases): https://github.com/edwin-hao-ai/MouseClaw
35-sec demo: https://github.com/edwin-hao-ai/MouseClaw#readme

Apple Silicon only for now. Built with Tauri 2 + Rust. Roadmap
includes Intel and a Windows port — would love to know if anyone
actually wants Windows.
```

### 2.2 `r/SideProject` (English · personal-story angle)

**Title:**

```
I spent 5 days building a desktop AI pet that summons next to your cursor — would love your eyes
```

**Body:**

```
Background: every "AI desktop app" I tried wanted me to open a chat
window, paste a screenshot, and explain what I'm looking at. I just
wanted to hold a shortcut, speak the question, and have the AI
already see what's on screen.

So I built MouseClaw over 5 evenings. Pixel-art mouse, lives in the
menu bar, pops up where your cursor is.

The three moments I'm proudest of:
1. **Hold-and-drag draws a pink circle on the screenshot** — so the
   AI literally sees what you're pointing at. "Circle anything · ask
   anything." Surprisingly intuitive once you try it.
2. **Long-press fn = voice IME** — speak, text appears at your cursor
   in any app. Bypasses macOS IME so 中文 input doesn't intercept.
3. **Clipboard hub at ⌘⇧V** — every copy you've made, searchable,
   encrypted.

Tech: Tauri 2 + Rust + sherpa-onnx. DMG is signed + notarized so
it installs like a normal app. Apple Silicon only.

What I'd love feedback on:
- Is "hold-shortcut-and-drag-to-circle" obvious or do I need to
  explain it in onboarding?
- The pet skins (6 of them) — overkill or a smart way to make people
  feel ownership?

Repo: https://github.com/edwin-hao-ai/MouseClaw
Demo (35s): docs/assets/demo-en.mp4

★ if it's something you'd actually keep installed.
```

### 2.3 Other subs to consider (calibrate body per sub)

| Subreddit | Angle to lead with | Notes |
|---|---|---|
| `r/productivity` | Voice IME + clipboard hub | Skip the dev-tool framing |
| `r/LocalLLaMA` | "Plug any OpenAI-compatible gateway via provider.env" | Speak to local-first crowd |
| `r/ChatGPT` | "Cursor-aware desktop client" | Frame as a *complement*, not replacement |
| `r/opensource` | 4-backend abstraction · MIT-style license | Tech-stack heavy |
| `r/HackerNews` | (don't — they get HN already) | — |

---

## 3. Twitter / X

### 3.1 EN — 8-tweet thread

> Attach `demo-en.mp4` on tweet 1. Drop the rest as replies within ~60 s.
> Each tweet kept under 250 chars so retweets quote cleanly.

```
1/ I built a pixel-art AI pet that lives next to your cursor.

Hold a shortcut. Drag the mouse to circle the thing you want to ask
about. Pet pops up next to your cursor and tells you what it is.

Free · Open source · macOS.

🎥 ↓

[ATTACH: demo-en.mp4]

———

2/ The core idea: every other AI app makes you re-explain the screen.

This one screenshots your display the moment you press the shortcut,
records your voice, and ships both to the AI. So you can just say
"what does this say" and it works.

———

3/ The "circle anything" feature is my favorite.

Hold the shortcut and drag your mouse — a pink trail bakes into the
screenshot. The AI literally sees what you circled.

Works on photos, screenshots, code, anything. Way more natural than
typing "the third paragraph from the top".

———

4/ Long-press the fn key and the pet becomes a voice IME.

You speak. sherpa-onnx transcribes locally. Text appears at your cursor
in any app — Messages, Notes, Slack, 飞书 — bypassing the active
input method.

It's the fastest "reply to mom" I've ever had.

———

5/ ⌘⇧V opens a clipboard hub.

Everything you've copied, searchable, draggable, AES-256-GCM
encrypted on disk. The thing macOS shipped without.

———

6/ Six pet skins. Classic gray, lab white, field brown, ninja,
robot, golden 🦞

Because if it's going to live in your menu bar for 8 hours a day,
you should at least like looking at it.

———

7/ Tech: Tauri 2 + Rust + sherpa-onnx + four pluggable AI backends
(Claude Code / OpenAI Codex / OpenClaw / Hermes).

DMG signed + notarized. Speech models bundled — first launch
is offline. Apple Silicon, Intel coming.

———

8/ It's free and open source. v0.1.25 just shipped.

⭐ if this is something you'd actually use:
github.com/edwin-hao-ai/MouseClaw

Would love to hear what scenarios you'd try first 🦞
```

### 3.2 中文 — 单条爆款式

> 中文 X 用户更习惯单条高信息密度。Attach `demo-zh.mp4`.

```
我做了个住在光标旁边的桌面 AI 桌宠 🦞

按住快捷键 → 鼠标拖一圈 → 桌宠在光标旁弹出，告诉你圈住的是什么。
照片里的灯？$69 同款。邮件太长？三句话给你重点。地图里的地名？
一键打开地图。

免费、开源、已公证（无 Gatekeeper 警告）

github.com/edwin-hao-ai/MouseClaw

[ATTACH: demo-zh.mp4]
```

---

## 4. YouTube

> Upload the 35-s demo as a Short (9:16 crop) **and** a horizontal
> version. Title front-loads the primary keyword. Description is
> ≥ 250 words with timestamps + keyword variations.

### 4.1 EN · Short title (≤ 75 chars)

```
MouseClaw: I made a desktop AI pet that lives next to your cursor (macOS, free)
```

### 4.2 EN · description

```
MouseClaw is a pixel-art desktop AI pet for macOS. Hold a shortcut, drag your mouse to circle anything on screen, and the pet pops up next to your cursor to tell you what it is — or open the right app for you.

Free. Open source. Notarized so it installs like a normal app.

👉 Download (DMG in Releases): https://github.com/edwin-hao-ai/MouseClaw
👉 Landing page: https://edwin-hao-ai.github.io/MouseClaw/

▼ What's inside
0:00  Pixel-art pet that lives next to your cursor
0:04  Six pet skins — classic, lab white, ninja, robot, golden, brown
0:07  Read long emails — get the gist in 3 lines
0:11  Circle anything in a photo — pet identifies + finds it online
0:15  Computer-use — say "Take me to Tokyo", Maps opens
0:18  Browser-use — pet drives the browser, types search, clicks
0:22  Voice typing at your cursor — bypasses macOS IME, works in any app
0:26  Clipboard hub — every copy you've made, encrypted, searchable
0:29  One pet, six everyday uses
0:32  Free · Private · Open Source

▼ Stack
- Tauri 2 (Rust + WebView)
- sherpa-onnx (zh-en ASR + punctuation bundled · offline-capable)
- AES-256-GCM clipboard encryption with macOS Keychain
- Four pluggable backends: Claude Code CLI · OpenAI Codex · OpenClaw · Hermes Agent

▼ Apple Silicon only for now. Intel and Windows on the roadmap.

#macOS #AIDesktopAssistant #OpenSource #DeveloperTools #ClaudeCode #MacApps #Productivity #PixelArt
```

### 4.3 中文 · 视频标题

```
MouseClaw：一只住在光标旁边的桌面 AI 桌宠（macOS · 免费开源）
```

### 4.4 中文 · 视频描述

```
MouseClaw 是一只 Mac 桌面 AI 桌宠 —— 按住快捷键、鼠标拖一圈，桌宠就在你的光标旁弹出，告诉你圈住的东西是什么、帮你打开应用、回消息、找东西。

免费 · 开源 · 已签名 + 公证（双击就装）

👉 下载（DMG 在 Releases）：https://github.com/edwin-hao-ai/MouseClaw
👉 介绍页：https://edwin-hao-ai.github.io/MouseClaw/

▼ 视频里都有啥
0:00  住在光标旁边的像素桌宠
0:04  六款桌宠皮肤 —— 经典灰、小白鼠、田鼠、忍者鼠、机械鼠、金鼠
0:07  长邮件三句话告诉你重点
0:11  圈住照片里的东西 —— 桌宠告诉你是啥 + 帮你找
0:15  动嘴打开地图 —— 「带我去东京」，地图自动开
0:18  浏览器操作 —— 桌宠帮你打字、点搜索按钮
0:22  光标位置语音输入 —— 绕过输入法，任何 app 都能用
0:26  剪贴板大全 —— 所有复制过的内容，加密保存可搜索
0:29  一只桌宠，六个日常用法
0:32  免费 · 本地 · 开源

▼ 技术栈
- Tauri 2（Rust + WebView）
- sherpa-onnx（zh-en ASR + 标点模型已打包·首启动 0 下载）
- AES-256-GCM 剪贴板加密 + macOS Keychain
- 4 个可选 AI 后端：Claude Code · OpenAI Codex · OpenClaw · Hermes

▼ 仅支持 Apple Silicon，Intel 和 Windows 在路上。

#macOS #AI桌宠 #开源 #效率工具 #ClaudeCode #Mac应用 #桌面助手 #像素艺术
```

---

## 5. Instagram (Reel · 9:16 crop of the 35-s demo)

> Hashtag cap is 5 (Dec-2025 change). Keywords in the caption matter
> more than hashtags now.

### 5.1 EN caption

```
I made a pixel-art AI pet that lives next to your cursor on Mac.

You hold a shortcut, drag your mouse to circle anything on screen — a
product in a photo, a paragraph you don't understand, a place name in
an email — and the pet pops up next to your cursor and tells you what
it is, or just opens the right app for you.

It speaks. It types into any field (so you can reply to mom by voice
without ever touching the keyboard). It keeps every copy you've made.
Six pet skins so you can pick the one you like best.

Free. Open source. Already notarized so it installs like a normal Mac
app — drag, double-click, done.

Link in bio. ⭐ if you want to try it 🦞

#macOSApps #AIDesktopAssistant #ProductivityTools #OpenSource #PixelArt
```

### 5.2 中文 caption

```
我做了一只住在你光标旁边的桌面 AI 桌宠 🦞

按住一个快捷键，鼠标在屏幕上拖一圈，圈住任何东西 —— 照片里的某个物件、
看不懂的一段话、邮件里提到的地名 —— 桌宠就在你光标旁弹出，告诉你这是
什么，或者帮你直接打开对应的应用。

它能说话、能在任何输入框里打字（动嘴回妈妈消息根本不用碰键盘）、还能记
住你所有复制过的内容。六款皮肤随便挑。

免费、开源、已经做了苹果公证（不会再弹「未识别开发者」警告），双击 DMG
就装。

链接看主页 · 想试就点个 ⭐ 🦞

#Mac应用 #AI桌宠 #效率神器 #开源 #像素风
```

---

## 6. 小红书 · 笔记

> 800–1200 字 · emoji 重 · 标题党 + 干货 · 5–10 个标签 · 配 demo-zh.mp4 +
> 3–5 张截图（hero.png + 关键场景截图）。

### 6.1 标题（A/B 测试用，3 选 1）

```
A. 这只桌宠让我把 ChatGPT 卸了｜Mac 必装 🦞
B. 救命这玩意儿 Mac 上比 Siri 强 100 倍｜免费开源
C. 我光标旁边住了个 AI 桌宠，活了 ✨
```

### 6.2 正文

```
家人们谁懂啊‼️
我 Mac 上装了个开源桌面 AI，再也不用切 ChatGPT 复制粘贴了 🥹

📍 这玩意儿叫 MouseClaw 🦞，是个像素风小老鼠
按一下快捷键就在你光标旁边弹出来
你拖鼠标在屏幕上画个圈，它就知道你说的是哪
比对话框打字快太多了……

🌟 我用了一周，最爱的 4 个场景：

1️⃣ 看长邮件不想读
对着邮件按快捷键 →「这封说啥？」
它三句话告诉我重点 + 要做的事 ✅
我妈那种「顺便帮我做 5 件事」的邮件再也不用反复读了 😭

2️⃣ 拍照问它「这是啥」
家具杂志上看到个台灯，截图圈出来
它告诉我：「无印良品同款 · ¥529」
直接省了我半小时搜索 🛍️

3️⃣ 动嘴打字
按住 fn 键说话 → 字直接出现在光标位置
不管在微信、飞书、备忘录都能用
回我妈的「周末有空吗」三秒钟搞定 🎙️

4️⃣ 复制过的找回
按 ⌘⇧V 弹出剪贴板大全
今天复制过的链接、号码、地址全在里面
搜索 / 标星 / 拖出来到任何 app ✨

———

🪄 而且超良心的点：
✅ 完全免费 · 开源（GitHub 找得到）
✅ 已经做了苹果公证 → 双击直接装，没有什么「未识别开发者」弹窗
✅ 语音模型打包在 app 里，首次启动不用下载
✅ 6 款桌宠皮肤：经典灰、小白鼠、忍者鼠、机械鼠、金鼠…我选的金鼠 🌟
✅ 数据全在你 Mac 上，剪贴板都是加密的

🦞 下载方式
直接 Google 搜「MouseClaw github」第一个就是
DMG 在 Releases 里 · 仅支持 Apple Silicon
（M1/M2/M3/M4 都行，Intel Mac 暂时不行）

———

姐妹们试完跟我说说哦 ❤️
我感觉这玩意儿就是我心中的「Mac 版 ChatGPT 桌面端」终极形态了
```

### 6.3 标签

```
#Mac效率 #Mac必装 #开源软件 #AI神器 #提升效率
#苹果电脑 #桌宠 #数字生活 #软件推荐 #ChatGPT
```

---

## Appendix · key value props (steal as you remix)

Use these whenever a platform's tone changes — the underlying claims
stay the same.

**Identity (one-liner):**

- EN: A pixel-art desktop AI pet for macOS that lives next to your cursor.
- ZH: 一只住在你光标旁边的 Mac 桌面 AI 桌宠。

**Three killer scenarios:**

| # | EN | ZH |
|---|---|---|
| 1 | Circle anything · ask anything (cursor-as-context) | 圈住任何东西 · 问任何问题 |
| 2 | Voice IME at your cursor — bypasses macOS IME | 光标位置语音输入 · 绕过输入法 |
| 3 | Browser-use & computer-use — AI drives your apps | 浏览器 + 系统操作 · 让 AI 帮你点 |

**Differentiator vs. ChatGPT desktop:**

- It's at your **cursor**, not in a chat window.
- It sees the screen *and* the thing you circled.
- It can act on macOS (Maps, Reminders, browser, voice typing).
- All data is local. Clipboard is encrypted. No account needed.

**Trust signals:**

- ✅ Notarized DMG (no Gatekeeper warning)
- ✅ Open source on GitHub
- ✅ sherpa-onnx speech models bundled (no first-launch download)
- ✅ AES-256-GCM clipboard encryption · key in macOS Keychain
- ✅ Free · no account · no telemetry

---

## References (platform conventions verified 2026-05)

- [Hacker News Show HN guidance](https://news.ycombinator.com/showhn.html)
- [How to crush your Hacker News launch · dev.to/dfarrell](https://dev.to/dfarrell/how-to-crush-your-hacker-news-launch-10jk)
- [How to launch a dev tool on Hacker News · markepear](https://www.markepear.dev/blog/dev-tool-hacker-news-launch)
- [How to Market on r/SideProject · MediaFa.st](https://www.mediafa.st/marketing-on-rsideproject)
- [Reddit promotion templates 2026 · ReddiReach](https://www.reddireach.com/blog/reddit-promotion-without-being-salesy-in-2026-post-templates)
- [Reddit Self-Promotion Rules 2026 · KarmaGuy](https://karmaguy.io/en/blog/reddit-self-promotion-rules)
- [X (Twitter) Algorithm 2026 · Teract](https://www.teract.ai/resources/twitter-algorithm-2026)
- [Twitter Threads 2026 · 12 Proven Strategies](https://aifreeforever.com/blog/twitter-x-threads-for-business-12-proven-strategies-to-grow-your-brand)
- [Viral Xiaohongshu Notes Template · Hashmeta](https://hashmeta.com/blog/how-to-create-viral-xiaohongshu-notes-template-examples/)
- [Xiaohongshu Algorithm Secrets · Hashmeta](https://hashmeta.com/blog/how-to-go-viral-on-xiaohongshu-mastering-the-algorithm-secrets/)
- [Instagram 2026 · captions > hashtags](https://lamplightcreatives.com/captions-vs-hashtags-instagram-2026/)
- [Instagram Reel Hooks 2026 · OpusClip](https://www.opus.pro/research/best-video-hooks-instagram)
- [YouTube SEO 2026 · InfluenceFlow](https://influenceflow.io/resources/optimized-video-titles-and-descriptions-the-complete-2026-youtube-seo-guide/)
- [YouTube Shorts titles 2026 · MiraFlow](https://miraflow.ai/blog/youtube-shorts-titles-descriptions-2026-templates)
