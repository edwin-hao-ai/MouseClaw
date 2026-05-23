# 🦞 MouseClaw v0.4.6 — Launch Copy

> 复制粘贴用。v0.4.6 是大功能版本，主线故事：**桌宠现在会记事 + 会定时干活 + 浮在全屏之上**。
> 遵循 [twitter-and-hn-playbook.md](./twitter-and-hn-playbook.md)：HN 标题无 emoji / 坦诚局限、
> 前 4 小时回复每条评论、Reddit 不要直接 cross-post、小红书正文不能放链接。

---

## 1. Hacker News · Show HN（仅英文）

### Title（≤ 80 chars · 无 emoji · 不全大写 · 不标题党）

```
Show HN: MouseClaw – a desktop AI pet that remembers and runs scheduled tasks
```

### First comment / body — 提交后立刻自己贴

```
Hi HN — MouseClaw is a pixel-art pet that sleeps in your macOS screen
corner. Hold ⌘⇧Space and talk; it screenshots the display your cursor
is on, transcribes you locally, and pipes both into whatever AI CLI you
already have authed. I posted an early version a while back; v0.4.6 is a
big jump and three things are new enough to be worth a fresh look:

1. It remembers. A fully-local SQLite knowledge graph (no vector DB —
   retrieval is plain SQL + recency/importance scoring). When idle it
   distills a profile of you (stack, preferences, projects) and an
   entity graph; on summon it injects the relevant bits. There's a
   window to see exactly what it stored and delete any line. Off-switch
   + pause, on-device only.

2. It runs scheduled tasks. Say "summarize AI news every morning" and it
   creates a cron-like job; results land in a unified, newest-first feed
   (markdown, links open in your browser). The interesting part is the
   anti-hallucination harness: scheduled runs get real tools (web search,
   file read, bash) plus a hard "fetch real data, never fabricate, say
   so if you can't" instruction — otherwise an unattended "summarize the
   news" just invents plausible headlines.

3. 15 backend CLIs now, including China's Qwen Code and ByteDance's Trae
   Agent alongside Claude Code / Codex / Gemini / Copilot / etc. Pick at
   onboarding (only installed ones show), switch from the tray.

The annoying one I finally fixed: the pet vanished whenever another app
went fullscreen. Plain NSWindow + collectionBehavior/level works in dev
but not in release builds on macOS (Tauri #5566/#9556). The fix was
converting the overlay to a real NSPanel (via tauri-nspanel) — same
approach BongoCat/Cap use.

Stack: Tauri 2, Rust, React/TS webview, sherpa-onnx (streaming Zipformer
zh-en + CT-Transformer punctuation) bundled so first launch is offline.
DMG signed + notarized.

Candid about limits: Apple Silicon only, no Windows yet, and the
scheduled-task quality is only as good as the backend's tools (Claude
Code is best here because web/bash are first-class).

Repo + DMG: https://github.com/edwin-hao-ai/MouseClaw
Demo (35s, no sound): https://github.com/edwin-hao-ai/MouseClaw/raw/main/docs/assets/demo-en.mp4

Two things I'd love feedback on:
- The memory design: SQL + scoring instead of embeddings, for a
  single-user local agent. Overkill? Under-built?
- The anti-hallucination harness for unattended runs — is "give it real
  tools + a strict prompt" enough, or do you gate on citations?
```

**Tue–Thu 09:00–11:00 ET 发，之后盯帖 3 小时。**

---

## 2. Twitter / X · EN thread（6 条）

```
1/ My desktop pet now remembers what I work on and runs tasks while I'm away.

MouseClaw v0.4.6 — a pixel mouse in your macOS corner. Hold a key, talk,
it sees your screen + answers. Now it also has a memory and a to-do
schedule. Free + open source. 🧵

[demo gif]

2/ It remembers — fully on-device.

A local SQLite knowledge graph (no cloud, no vector DB) quietly builds a
profile: your stack, how you like answers, what you're building. Summon
it and the relevant memory rides along. One window shows everything it
knows; delete any line, or pause it.

3/ It runs scheduled tasks.

"Summarize AI news every morning." Done. Results collect in one feed you
read like a newspaper. The trick is an anti-hallucination harness:
unattended runs get real web/file tools + a strict "never make it up"
rule, so it fetches actual data instead of inventing headlines.

4/ 15 AI backends now.

Claude Code, Codex, Gemini, Copilot, OpenCode, Cline, Kimi, + China's
Qwen Code (通义) and ByteDance's Trae Agent, and more. Use whichever CLI
you've already got. Your tokens, your choice.

5/ It also has a name and a personality now 🐭

Name it, pick a tone (warm / snarky / minimal / tsundere / 6 more). And
it finally floats over fullscreen apps — turns out you need a real
NSPanel for that on macOS.

6/ 100% local speech (sherpa-onnx). Signed + notarized. Apple Silicon.
Free, MIT, no account.

⭐ https://github.com/edwin-hao-ai/MouseClaw
```

---

## 3. Twitter / X · 中文单条（爆款式）

```
我的 macOS 桌宠更新了，现在它会记事、还会定时帮我干活 🦞

一只睡在屏幕角落的像素老鼠：按住快捷键说话 → 它截屏 + 听你说 + 调你本机的 AI
→ 流式回答。v0.4.6 新增：

🧠 长期记忆：全本地，悄悄记住你的技术栈/习惯/在做的项目，召唤时自动带上（可看可删）
⏰ 定时任务：说一句"每天早上整理 AI 新闻"就建好，结果一页看全 —— 还专门做了反幻觉
   （强制联网取真实数据，绝不编造）
🤖 15 个 AI 后端任选，含通义 Qwen Code + 字节 Trae
🐭 能起名 + 9 种性格 / 🖥️ 终于能浮在全屏 app 之上了

免费开源，国内免 VPN：github.com/edwin-hao-ai/MouseClaw
```

---

## 4. Reddit · r/macapps（英文 · 克制 · 不要 cross-post）

```
Title: MouseClaw v0.4.6 — pixel desktop AI pet that now remembers + runs scheduled tasks (free, open source)

Body:
Update to my little macOS desktop pet. Hold a hotkey, talk, it screenshots
+ answers using whatever AI CLI you've authed (15 supported now).

New in 0.4.6:
- Long-term memory, fully local (SQLite, no cloud) — view/delete what it stored
- Scheduled tasks with a unified results feed + an anti-hallucination harness
- Name + 9 personalities, per-skin sound effects
- Finally floats over fullscreen apps (real NSPanel)

100% local speech, signed + notarized, Apple Silicon only for now.
Repo + DMG in releases: https://github.com/edwin-hao-ai/MouseClaw
Happy to answer anything.
```

---

## 5. 小红书（正文不能放链接 → 引导"主页/评论区"）

```
标题：我给 Mac 养了只会记事的像素小老鼠 🦞

正文：
桌面角落一只像素老鼠，按住快捷键跟它说话，它就截屏 + 听懂你 + 调 AI 回答你～

这次大更新它真的"长脑子"了：
🧠 会记事：全本地记住我的技术栈、习惯、在做的项目，下次召唤自动带上（能看能删，不上云）
⏰ 会定时干活：跟它说"每天早上整理 AI 新闻"，到点自己跑完，所有结果一页看，像看报纸
🐭 能起名 + 选性格（暖心 / 毒舌 / 傲娇…9 种）
🖥️ 终于能浮在全屏视频/编辑器上面了

完全免费 + 开源，国内不用 VPN。下载在主页链接 / 评论区～

#Mac #效率工具 #AI #桌宠 #开源 #程序员 #数字生活
```
