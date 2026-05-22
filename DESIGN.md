# MouseClaw Design System

**Version:** v0.1.4 · **Last updated:** 2026-05-13 · **Owner:** edwin-hao-ai

This is the **single source of truth** for MouseClaw's visual language. Any UI work (React webview, prototype HTML, marketing site, README screenshots) **must** conform to the tokens and components defined here. When the prototype and this doc disagree, **this doc wins**; update the prototype.

If you find yourself reaching for a value not listed here — stop and discuss whether to (a) use the closest existing token, or (b) extend this doc. Never inline ad-hoc values.

---

## 1 · Foundations

### 1.1 Design Principles

1. **Quiet by default** — The app is invisible 99% of the time. Every visible pixel must earn its presence. No idle indicators, no menubar items, no notification badges.
2. **Pixel character, glass UI** — The mouse is 8-bit retro. Everything around the mouse (bubbles, panels, buttons) is modern translucent glass. This contrast is the brand.
3. **Motion = personality** — Each animation conveys an emotion (sleep = calm, listen = alert, think = curious, jump = celebration). Generic fade/slide is forbidden for the mouse character.
4. **Subtraction beats addition** — Short answers stay short (3-sec bubble). Only escalate to panel when content demands it. Default = least intrusive.
5. **Safety made visible** — Anything that modifies the user's environment (Mode B write-back) gets a visible countdown. Hidden side-effects are bugs.
6. **Bilingual first** — All copy in English + 中文. Type stacks, sizes, and line-heights must render both well. Test with mixed-script strings.
7. **Accessibility is not a phase** — Contrast ≥ 4.5:1 for body, ≥ 3:1 for large text. Respect `prefers-reduced-motion`. Every interactive element has a focus state.

### 1.2 Brand Voice in Visuals

| Quality | Expression |
|---|---|
| Friendly, not childish | Soft palette + rounded geometry, but no cartoony exaggeration |
| Capable, not corporate | Pink accent (#ff6b9d) signals "approachable AI", not "enterprise SaaS blue" |
| Precise, not sterile | Pixel-perfect alignment + crisp typography, paired with warm translucent depth |
| Calm, not boring | Slow ambient animations (3+ second loops) so motion never feels frantic |

---

## 2 · Design Tokens

Tokens are **mandatory variables**. Hardcoded literals in component code are review failures.

### 2.1 Color

#### Surface (dark — for marketing, settings, prototype HTML backgrounds)

| Token | Value | Use |
|---|---|---|
| `--surface-deep` | `#0a0e1a` | Page background, root canvas |
| `--surface-glass` | `rgba(255, 255, 255, 0.06)` | Floating card background |
| `--surface-glass-strong` | `rgba(255, 255, 255, 0.10)` | Dock-style elements, kbd keys |
| `--surface-border` | `rgba(255, 255, 255, 0.10)` | Glass card border |

#### Surface (light — for floating bubbles and panels over user's app)

| Token | Value | Use |
|---|---|---|
| `--bubble-bg` | `rgba(255, 255, 255, 0.97)` | Default bubble |
| `--panel-bg` | `rgba(252, 252, 254, 0.98)` | Expanded panel |
| `--bubble-border` | `rgba(0, 0, 0, 0.06)` | Bubble/panel hairline |
| `--bubble-success-bg` | `linear-gradient(135deg, #d8f8e6, #ffffff)` | ✅ Done bubble |
| `--bubble-warn-bg` | `linear-gradient(135deg, #fff7e0, #ffffff)` | ⌨ Insert countdown |
| `--bubble-danger-bg` | `linear-gradient(135deg, #ffe1e0, #ffffff)` | ⛔ Mode B blocked |

#### Text

| Token | Value | Use |
|---|---|---|
| `--text-primary` | `#1a1a1a` | Bubble/panel body text |
| `--text-on-dark` | `#f5f5f7` | Text over dark surfaces |
| `--text-dim` | `#a1a1a8` | Captions, secondary text on dark |
| `--text-bubble-dim` | `#6c6c70` | Secondary text on bubble |
| `--text-warn` | `#b58900` | "INSERT AT CURSOR" label |
| `--text-danger` | `#7a1f1c` | "BLOCKED" body text |
| `--text-link` | `#b3326d` | Inline accent text |

#### Accent (the brand)

| Token | Value | Use |
|---|---|---|
| `--accent-primary` | `#ff6b9d` | Primary actions, CTAs, focus rings |
| `--accent-soft` | `#ff9bb8` | Hover/lighter accent, gradient end |
| `--accent-gradient` | `linear-gradient(135deg, #ff6b9d, #ff9bb8)` | CTA button background |
| `--accent-glow` | `rgba(255, 107, 157, 0.15)` | Focus ring fill, soft chip background |

#### Semantic

| Token | Value | Use |
|---|---|---|
| `--success` | `#5cd6a0` | Done states, session-chip background |
| `--success-soft` | `#88e2b7` | Success tag text |
| `--warn` | `#ffd166` | Caution states, countdown end |
| `--danger` | `#ff5f57` | Errors, blocked states (also macOS traffic-light red) |

#### Pixel-Mouse Palette (NES-style limited 8 colors)

| Token | Value | Anatomy |
|---|---|---|
| `--m-body` | `#cfcfcf` | Light gray — main body, head |
| `--m-belly` | `#ffffff` | White — belly highlight |
| `--m-ear-out` | `#cfcfcf` | Same as body (ear outer rim) |
| `--m-ear-in` | `#ff9bb8` | Pink — ear inner |
| `--m-eye` | `#1a1a1a` | Near-black — eye pupil |
| `--m-nose` | `#d63d6a` | Deep pink — nose |
| `--m-tail` | `#8a8a8a` | Dark gray — tail |
| `--m-paw` | `#ffffff` | White — paws |

**Locked constraint:** The mouse uses **exactly these 8 colors**, no more. Adding a 9th requires updating this doc and all states. NES discipline = strong silhouette + cheap recognition.

#### Skin Palettes (v0.1.7 — 6 mouse styles)

Same 8-color slot system per skin; only the values vary. Body变体（standard / slim
/ chubby / ninja / robot / round）控制耳/身/尾形状的像素差异 —— anatomy 仍锁 16×16
网格。前后端单一信源：TS 在 `src/skins.ts`、Rust 在 `src-tauri/src/skins.rs`。

| Skin | id | Body 变体 | body | belly | ear-in / ear-out | eye | nose | tail | paw | 角标 |
|---|---|---|---|---|---|---|---|---|---|---|
| 🐭 经典灰 | `classic` | standard | `#cfcfcf` | `#ffffff` | `#ff9bb8` / `#cfcfcf` | `#1a1a1a` | `#d63d6a` | `#8a8a8a` | `#ffffff` | 默认 |
| 🤍 小白鼠 | `lab` | slim | `#fafafa` | `#ffffff` | `#ffb3c8` / `#e8e3dc` | `#ff5f57` | `#ff6b9d` | `#ffb3c8` | `#ffffff` | — |
| 🌾 田鼠 | `field` | chubby | `#a47148` | `#e8d5b0` | `#d6a07a` / `#7a4e2e` | `#1a1a1a` | `#5a2e10` | `#7a4e2e` | `#d6a07a` | — |
| 🥷 忍者鼠 | `ninja` | ninja | `#3a3a42` | `#5a5a64` | `#1a1a1a` / `#2a2a30` | `#ffd166` | `#1a1a1a` | `#2a2a30` | `#1a1a1a` | — |
| 🤖 机械鼠 | `cyber` | robot | `#c0c8d0` | `#e8eef4` | `#00e5ff` / `#7a8290` | `#00e5ff` | `#00e5ff` | `#7a8290` | `#5a6270` | 酷 |
| ✨ 金鼠 | `golden` | round | `#e8c87a` | `#fff4d6` | `#d68a40` / `#c8a050` | `#3a2818` | `#a06028` | `#c8a050` | `#fff4d6` | 限定 |

**新增皮肤的硬规则：**
1. 仍只能用 **8 个 slot**（body/belly/ear-in/ear-out/eye/nose/tail/paw）—— 不加第 9 个槽
2. 体型变体只能从现有 6 个里挑，新增变体需同 PR 改 `PixelMouse.tsx::Ears/Body/Tail`
3. 同 PR 更新这张表 + `src/skins.ts` + `src-tauri/src/skins.rs::SkinId`
4. 6 款全 state 在 prototype（`docs/prototypes/mouse-skins-20260515.html`）截图确认对比度可读

---

### 2.2 Typography

#### Font Stack

```css
--font-system: -apple-system, BlinkMacSystemFont, "SF Pro Display",
               "PingFang SC", "Microsoft YaHei", sans-serif;
--font-mono:   ui-monospace, "SF Mono", "Cascadia Mono", Consolas, monospace;
```

Do **not** load Google Fonts, Inter, or any webfont. System fonts render Chinese + English uniformly across macOS/Windows and require zero network.

#### Type Scale

| Token | Size | Line-height | Weight | Use |
|---|---|---|---|---|
| `--text-display` | 38px | 1.1 | 700 | Marketing hero, onboarding title |
| `--text-headline` | 22px | 1.3 | 700 | Section headers, panel titles |
| `--text-title` | 16px | 1.4 | 600 | Subsection headers |
| `--text-body-lg` | 14px | 1.6 | 500 | Button text, panel input |
| `--text-body` | 13px | 1.5 | 500 | Bubble body, panel content (**default**) |
| `--text-body-sm` | 12px | 1.6 | 500 | Caption inside bubble |
| `--text-meta` | 11px | 1.6 | 600 | Tags, kbd hints, session chip |
| `--text-micro` | 10px | 1.5 | 700 | Uppercase labels (BLOCKED, INSERT AT CURSOR) |

#### Letter Spacing

- Headlines (≥22px): `-0.02em` (tighter, more confident)
- Uppercase labels: `+0.08em` to `+0.12em` (more breathing room)
- Default body: `0`

#### Numerals

Use `font-variant-numeric: tabular-nums` for timestamps, countdown numbers, session counts (#42, turn 2/15). Prevents jitter on count change.

---

### 2.3 Spacing

**8-pixel base grid.** Allowed values only:

| Token | Value | Common use |
|---|---|---|
| `--space-1` | 4px | Tag padding-y, tiny gaps |
| `--space-2` | 8px | Inline gap, button icon spacing |
| `--space-3` | 12px | Card body padding |
| `--space-4` | 16px | Card padding default |
| `--space-5` | 20px | Section internal |
| `--space-6` | 24px | Card→card gap |
| `--space-7` | 28px | Onboarding card padding-x |
| `--space-8` | 32px | Page padding-x mobile |
| `--space-10` | 48px | Section→section gap |
| `--space-12` | 56px | Onboarding panel padding-x |
| `--space-14` | 64px | Major section break |

**Forbidden:** 5px, 6px, 7px, 10px, 11px, 13px, 15px, 17–19px, 21–23px, etc. If you need a value not in the table, you're probably doing it wrong.

---

### 2.4 Radius

| Token | Value | Use |
|---|---|---|
| `--radius-pill` | 999px | Chips, tags, kbd, mode labels |
| `--radius-xs` | 4px | Small dock-app, tag-inside-bubble |
| `--radius-sm` | 6px | Buttons (≤32px), small inputs |
| `--radius-md` | 8px | Inputs, kbd, dock-app default |
| `--radius-lg` | 10px | Cards-in-card (browser tabs, mock windows) |
| `--radius-xl` | 12px | Bubble, dock-style container |
| `--radius-2xl` | 14px | Dock chrome |
| `--radius-3xl` | 16px | Section cards, panel root |
| `--radius-4xl` | 24px | Onboarding panel |

---

### 2.5 Shadow

| Token | Value | Use |
|---|---|---|
| `--shadow-bubble` | `0 12px 32px rgba(0,0,0,0.35), 0 2px 6px rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.7)` | Floating bubble over user app |
| `--shadow-panel` | `0 24px 64px rgba(0,0,0,0.4), 0 4px 12px rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.9)` | Expanded panel |
| `--shadow-card` | `0 30px 80px rgba(0,0,0,0.5), inset 0 0 0 1px rgba(255,255,255,0.06)` | Marketing/prototype section cards on dark |
| `--shadow-cta` | `0 8px 24px rgba(255,107,157,0.35)` | Primary CTA button |
| `--shadow-mouse` | `drop-shadow(0 4px 8px rgba(0,0,0,0.4))` | Pixel mouse on user screen |
| `--shadow-focus-ring` | `0 0 0 4px rgba(255,107,157,0.15)` | Onboarding option selected state |
| `--shadow-input-focus` | `0 0 0 3px rgba(255,107,157,0.15)` | Panel input focused |

The `inset 0 1px 0 rgba(255,255,255,0.x)` highlight is **mandatory** on every light-surface card to fake the glass top-edge sheen.

---

### 2.6 Backdrop Filter (glass blur)

| Surface | Blur | Notes |
|---|---|---|
| Bubble | none | Already opaque (97%), no blur needed |
| Panel | `blur(40px)` | Strong blur — content behind should be unreadable |
| Onboarding card | `blur(30px)` | Medium blur |
| Marketing card | `blur(20px)` | Light blur |
| Dock | `blur(30px)` | macOS dock parity |
| Menubar mock | `blur(20px)` | Matches macOS menubar |

If `backdrop-filter` isn't supported (rare on Tauri webview but possible), fall back to bumping background alpha to 0.95+.

---

### 2.7 Motion

#### Timing Functions

| Token | Curve | Feel |
|---|---|---|
| `--ease-default` | `ease-in-out` | Ambient (sleep, listen, hover) |
| `--ease-spring` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | Celebration (jump) — overshoot |
| `--ease-linear` | `linear` | Countdown progress, scrubbing |
| `--ease-tap` | `cubic-bezier(0.2, 0, 0, 1)` | Click feedback, button press |

#### Durations (mouse states)

| State | Duration | Frames | FPS | Iteration |
|---|---|---|---|---|
| Sleep | 3.2s | 2 | ~0.6 | infinite |
| Listen bounce + ear wiggle | 0.6s / 0.4s | 4 | ~10 | infinite |
| Think wobble | 1.4s | 2 (rotate ±3°) | ~1.4 | infinite |
| Jump | 0.9s | 3 keyframes | one-shot (3× max) | trigger on success |
| Countdown bar fill | 3000ms exact | continuous | linear | once per B-mode |

#### Launch entrance (v0.5 · `mc-entrance-*`)

开场调皮入场动画的桌宠精灵动作。窗口横穿位置由 Rust `entrance.rs` 逐帧推（不在此表）；
此表只列**精灵自身**的 CSS 动画。三档（loud / medium / subtle）共用这些 phase class。
所有动作只作用在整只 `.mouse-svg` + `.mc-ear` / `.mc-tail` 子组 → 9 款皮肤一视同仁，零 palette 硬编码。
原型：`docs/prototypes/launch-entrance-20260521.html`。

| Phase | Duration | Curve | 动作 |
|---|---|---|---|
| `peek` | 600ms loop | `ease-default` | 探头左右张望（±7° 摇头）+ 抖耳 |
| `run` | 240ms loop · `steps(2)` | linear steps | 跑腿弹跳（translateY + ±3° 旋转，像素跳帧感） |
| `skid` | 280ms one-shot | `ease-tap` | 急刹横向挤压回弹（squash/stretch） |
| `beat` | 760ms one-shot | `ease-spring` | 蹦跶张望（hop + 歪头）+ 甩尾 + 抖耳 |
| `stretch` | 620ms one-shot | `ease-spring` | 角落伸懒腰（subtle 档专用，纵向拉伸回弹） |

表情拍跨物种：鼠/猫/狐 `.mc-ear` 旋转抖动；蛙 `.mc-ear-frog` 改眼鼓气（scale 1.28）；
cyber `.mc-ear`（天线）同样摆；尾巴 `.mc-tail` 甩（狐尾本就大，摆幅天然更明显）。

#### UI Motion

| Element | Duration | Curve |
|---|---|---|
| Hover lift (card translateY -4px) | 300ms | `ease-default` |
| Button press | 150ms | `ease-tap` |
| Bubble appear / disappear | 200ms / 150ms | `ease-default` |
| Panel expand from bubble | 300ms | `ease-default` |
| Cursor blink (streaming) | 1s | `steps(2)` |

#### `prefers-reduced-motion`

When set:
- Mouse states: still animate (the character IS the product) — but slow to ~50% speed and reduce amplitude
- UI hover lifts: disable entirely
- Streaming cursor: replace blinking with static block
- Launch entrance (`mc-entrance-*`): fully skipped — Rust 端读 `reduced_motion` 不横穿，桌宠直接出现在 anchor；CSS 也把 phase class 动画降级为 `none`

---

## 3 · Iconography

### 3.1 The Pixel Mouse

**Canonical canvas: 16 × 16 pixels.** Always rendered at integer multiples (16, 32, 48, 64, 96, 128).

Mandatory CSS:
```css
image-rendering: pixelated;
image-rendering: crisp-edges;
shape-rendering: crispEdges;  /* for SVG */
```

#### Anatomy (canonical layout)

```
. . . X X . . . . X X . . . . .    row 1 — ear tops
. . X X X X . . X X X X . . . .    row 2 — ears wide
. . X P X X . X X P X X . . . .    row 3 — ear pink + head
. . X X X X X X X X X X . . . .    row 3 — head top
. X X X X X X X X X X X X X . .    row 4-6 — head full
. X X X X E X X X E X X X X . .    row 5-6 — eyes (E)
. X X X X X X X X X X X X X . .
. . X X X X X X X X X X X . . .    row 7 — chin
. . X X X X N N X X X X X . . .    row 7 — nose (N)
. . X X X B B B B X X X . . . T    row 8 — belly start, tail (T)
. . X X X B B B B X X X . . T T
. . . X X X X X X X X X . . . .    row 10 — bottom
. . . X P . . . . . . P X . . .    row 11 — paws (P)
```

The pixel positions in `src/components/Mouse.tsx` are the **legal authoritative version**. Prototypes mirror those positions.

#### Mouse States

| State | Eyes | Pose | Extras | Animation |
|---|---|---|---|---|
| `sleep` | `─ ─` (1px dot, lower row) | Curled, tail flat | none | Slow vertical breathing (3.2s) |
| `listen` | `● ●` (2px alert) | Standing, ears straight up | Mouth open dot under nose; sound bars in bubble | Ear wiggle (±12°) + 0.6s bounce |
| `think` | `● ●` | Standing | `?` pink mark over right ear | Body wobble (±3° rotation, 1.4s) |
| `write` | `─ ─` (focused/squint) | Standing | Yellow pencil in left paw | Slight forward lean |
| `jump` | `> <` (closed crescent) | Mid-air | Sparkle optional | One-shot 0.9s with overshoot |
| `block` | `● ●` | Standing back | Red `!` icon over head | None (static warning) |
| `feed-wait` *(v0.4)* | `● ●` (2px round) | Standing tall, mouth wide open (2×2 dark) | Pink `?` over right ear | Eager bob `mc-feed-wait` (600ms, scale 1.04) |
| `feed-digest` *(v0.4)* | `─ ─` (closed, content) | Standing | Pink belly glow (CSS drop-shadow) | Breathe `mc-feed-digest` (1100ms, asym scale) |
| `type` *(v0.1.14)* | `● ●` (2px) | Standing, leans toward cursor | Yellow pencil paw extended left | Static lean |
| `hub` *(v0.1.14)* | `︶ ︶` (curved closed) | Sitting | None | Static gentle |
| `paste` *(v0.1.14)* | `● ●` | Standing | White clipboard sprite in right paw | Brief flash |

#### Companion Layer States *(v0.4+)*

陪伴向"环境感知"层 —— 与上面的任务态解耦，只在 **idle 静默态**（`mouseStateFor(idle)→"sleep"`
或 onboarding `listen`）叠加。任务进行中（think/write/talk/feed-*/jump/block/paste/type）不挂。
驱动来源：`src/hooks/useCompanion.ts`（订阅 Rust `companion-tick` 30fps）。
**全部走整体 `.mouse-svg` transform / filter，不碰 palette —— 9 款皮肤自动适配**（见硬规则）。

优先级（高→低）：sleep > waking > dizzy > hop > clicked > worried > typing > excited > alert > glance > drowsy/idle。

| State | 触发 | 视觉 | 动画 |
|---|---|---|---|
| `companion-typing` | 正在打字（sinceKey<0.4s） | 睁眼追鼠标 | 点头 `mc-nod` (220ms loop) |
| `companion-alert` | 光标 <80px | 抬头 | translateY(-2px) scale(1.04) |
| `companion-excited` | 光标 <30px | 兴奋上抬 | translateY(-3px) scale(1.08) |
| `companion-clicked` | 点鼠标 <0.25s 内 | 耳朵抽 + 微缩 | `mc-ear-perk` + `mc-click-blink` (240ms) |
| `companion-worried` | 连续狂敲/退格 ≥1.5s | 歪头担心 | `mc-worried-tilt` (1.4s loop, ±3°) |
| `companion-sleep` | 键鼠静止 10s（深夜 5s） | 闭眼线 + 瘫 + ZZZ | scaleY 0.88 + brightness 0.85 |
| `companion-drowsy` | 深夜 23:00–05:59 默认态 | 眼皮半垂（eyes scaleY 0.55） | `mc-drowsy-sway` (4s) + brightness 0.92 |
| `companion-waking` | sleep/drowsy → 醒 | 伸懒腰 | `mc-waking` (700ms 拉伸回弹, 一次性) |
| `companion-dizzy` | 持续甩鼠标 ≥0.45s（>4200px/s） | ×_× 转圈，不追鼠标 | `mc-dizzy` 摇晃 + `mc-dizzy-eyes` |
| `companion-hop` | 一段打字 burst 结束 | 小跳 | `mc-hop` (480ms overshoot, 一次性) |
| `companion-glance` | 过整点 40% 概率（仅 idle） | 抬头看一眼（眼朝上） | `mc-glance` (2s) |
| `companion-neglected` | 冷落 >3 天 | 低头委屈 + 略暗 | translateY(1.5px) rotate(-1°) + 眼朝下 |
| `intimacy-1/2/3` | 互动累计 50/200/600 次 | 眼睛逐级微大（scale 1.05/1.10/1.16） | 静止（CSS scale on .mc-eyes） |

眼球追鼠标：贯穿所有非 sleep/drowsy/dizzy/neglected 态。`useCompanion` 算 `eyeOffset`（tanh
分量映射 X/Y 解耦，最大 ±1.4 SVG 单位），PixelMouse 用 **SVG `transform` attribute**（user 单位，
WebKit 可靠；CSS px 在 SVG group 上语义不一致）平移整个 `<g.mc-eyes>`。

爱心 ❤️：点桌宠本体浮一颗心（`.pet-heart`，`pet-heart-float` 700ms 上飘淡出）—— 见 App.css，
非 PixelMouse 内部 state。

所有 companion 动画在 `prefers-reduced-motion: reduce` 下降级为无动画（仅保留状态指示）。

#### Session Chain Indicator

When continuing a session, a **3-pixel-wide horizontal green chain** appears 1 pixel above the mouse's head (rows -1 to 0, cols 6-10):

```
. . . . . . G G G G . . . . . .   row -1
. . . . . . . G G G . . . . . .   row 0
```

Color: `var(--success)` (#5cd6a0). Static. Disappears on new session.

#### Forbidden mouse modifications

- ❌ Changing the 8-color palette
- ❌ Adding accessories (glasses, hats) — that's mascot variants for V3
- ❌ Resizing non-integer (mouse must be exactly 16, 32, 48, 64, 96, or 128 px)
- ❌ Anti-aliased rendering (always crispEdges)
- ❌ Using a different mouse character on different screens

---

## 4 · Components

### 4.1 Bubble (default speech bubble)

```
┌────────────────────────────┐
│ Hi, I'm 鼠标龙虾 ✨        │  ← --text-body
│                            │
│ ▼ 展开看完整回答           │  ← only if content ≥ 4 lines
└──┬─────────────────────────┘
   ▼  ← triangle pointer, 14×14 rotated 45°, bottom-left 24px
```

| Property | Value |
|---|---|
| Background | `var(--bubble-bg)` |
| Border | `1px solid var(--bubble-border)` (alpha 0.10 — v0.3.8 bumped from 0.06, was invisible on gradient variants) |
| Border-radius | `var(--radius-xl)` (12px) |
| Padding | `14px 14px 10px` (v0.3.8 — top 14 so text doesn't crowd corner radius) |
| Max-width | 280px (single bubble) / 320px (with rich content) |
| Text | `var(--text-body)` |
| Shadow | `var(--shadow-bubble)` |
| Pointer | 14×14 square rotated 45°, bottom: -7px, left: 24px, same bg + border-right/bottom only |

**Variants:** `.success`, `.warn`, `.danger` swap background to the corresponding `--bubble-*-bg` gradient. Pointer background must match the gradient endpoint (white).

**Auto-dismiss timing:**
- Short answer (< 4 lines): show 3s, then mouse runs to corner, bubble fades 150ms
- Long answer with `▼ 展开` ignored: same 3s timeout
- Streaming: never auto-dismiss while tokens arriving

#### Scroll affordances (v0.3.8, scrollable=true only)

When the reply is long enough to scroll inside the bubble:

| Element | Trigger | Position | Spec |
|---|---|---|---|
| `.bubble-fade-top` | `scrollTop > 4` | `top: 1px` inset | 18px tall gradient from variant bg → transparent, masks first row of clipped text |
| `.bubble-fade-bottom` | content below visible | `bottom: 1px` inset | mirror of fade-top, signals more below |
| `.bubble-scroll-top` (▲) | same as fade-top | `top: 6px right: 8px` | 22×22 round, `var(--accent-primary)` bg, white `▲`, `box-shadow: 0 2px 6px rgba(0,0,0,.15)` |

Fade gradients pull color from the variant's actual gradient endpoint (e.g. `#d8f8e6` for success-top, `#f3fbf6` for success-bottom). One pair per variant — see `Bubble.css`.

### 4.2 Panel (expanded long-form)

| Property | Value |
|---|---|
| Width × Height | 400 × 480 (max) |
| Background | `var(--panel-bg)` |
| Border | `1px solid var(--bubble-border)` |
| Border-radius | `var(--radius-3xl)` (16px) |
| Backdrop-filter | `blur(40px)` |
| Shadow | `var(--shadow-panel)` |
| Layout | header (auto) / body (1fr, scroll) / input (auto) |

**Header (40px):**
- Session chip on the left
- Turn count `{n}/{max} ⌬` next to chip
- Action buttons on the right: ＋ (new session) · ⌘ (copy) · ▾ (collapse)
- Background: subtle white-to-transparent gradient + 1px bottom border

**Body:**
- Padding `16px 18px`
- Each turn: 3px pink-bar left + italic user message, then full assistant content
- 18px gap between turns
- Custom 6px scrollbar with `rgba(0,0,0,0.15)` thumb

**Input row (auto height):**
- Top border 1px, padding `10px 12px`
- Input: 8px radius, full-width minus 40px send button
- Input focus: pink border + `--shadow-input-focus`
- Send button: 32×32, accent gradient, ↑ glyph

### 4.3 Session Chip

```
[ 🔗 续 Session #42 · 第 2 轮 ]
```

| Property | Value |
|---|---|
| Background | `rgba(92, 214, 160, 0.15)` (continuing) or `var(--accent-glow)` (fresh) |
| Text color | `#2d8c5f` (continuing) or `#b3326d` (fresh) |
| Border | `1px solid rgba(92, 214, 160, 0.3)` / accent variant |
| Padding | `3px 8px` |
| Border-radius | `var(--radius-pill)` |
| Font | `var(--text-meta)` (11px 600) |
| Icon | 🔗 for continuing, nothing for fresh |

### 4.4 Countdown Bar (Mode B confirm)

Layout:
```
[ keyhints row     ]
[ ──────────  3 ]
```

- Container width = bubble width
- Bar: 4px tall, `rgba(0,0,0,0.08)` track
- Fill: `linear-gradient(90deg, #5cd6a0, #ffd166, #ff6b9d)`, transform-origin left, scaleX animated 1→0 over exact 3000ms linear
- Number: tabular-nums, 12px 700, 16px min-width, transitions through 3→2→1→✓
- Key hints below: `<kbd>Esc</kbd>` 取消 · `<kbd>↵</kbd>` 立即写入

### 4.5 Onboarding Card

| Property | Value |
|---|---|
| Width | 560px max |
| Padding | `48px 56px` |
| Background | `var(--surface-glass)` |
| Backdrop-filter | `blur(30px)` |
| Border-radius | `var(--radius-4xl)` (24px) |

Internal layout:
1. Mouse hero (96×96, floating animation, 28px gap below)
2. Title `--text-headline`, 8px gap
3. Subtitle `--text-body`, 28px gap
4. Options stack (10px gap between)
5. CTA button (28px gap above, full-width, accent gradient, `--shadow-cta`)

**Option row:**
- 14×18 padding, 12px radius, 14px gap between key/label/tag
- kbd: 12px monospace on `rgba(0,0,0,0.35)` chip with 1px white-10 border
- Hover: bg → `rgba(255,107,157,0.08)`, border → `rgba(255,107,157,0.3)`
- Selected: bg → `var(--accent-glow)` (12% alpha), border → `rgba(255,107,157,0.5)`, `--shadow-focus-ring`

### 4.6 Voice Bars (in Listen bubble)

4 vertical bars, 3px wide each, 3px gap, heights 10/16/8/14 px, color `var(--accent-primary)`. Animation: `scaleY 0.6 → 1` over 0.7s with 0.1s stagger per bar.

### 4.7 Scheduled Tasks (v0.5)

定时任务（`?view=tasks` 管理窗 + 确认卡 + 结果轻气泡）**复用既有 token，不引入任何新颜色/间距/动画/mouse 状态**。

- **Schedule confirm card**（桌宠头顶，`schedule-confirm` 视图）：用 `--bubble-bg` / `--shadow-bubble` / `--radius-xl`，标签用 `--accent-glow` + `--text-link`，频率 chip 同 §4.3 session-chip 的 accent 变体。CTA 用 `--accent-gradient` + `--shadow-cta`。打 `data-adaptive-measure` 走自适应窗口测量。
- **Schedule result bubble**（`EV_SCHEDULE_RESULT`，不抢焦点、7s 自消）：成功用 `--bubble-success-bg`，`⏰ 定时` / `💡 提示` 标记用 `--accent-glow`/`--text-link`。同样打 `data-adaptive-measure`。
- **Tasks window**（与 Hub / Picker 同级的二级管理窗，非 overlay）：`--panel-bg` 玻璃头/底栏，任务卡 `--radius-lg` + 1px `--bubble-border`，开关 toggle 用 `--accent-primary`，频率 chip / 编辑表单全部走既有 token（见 `TasksView.css`）。
- **桌宠状态**：定时任务**不新增** mouse 状态 —— 确认时复用 `think`，跑任务时复用既有三点忙碌 badge（§"Companion / busy"），到点投递不改变 8 色调色板，9 款皮肤天然一致。

### 4.8 Forbidden Components

- ❌ Modal dialogs (we don't have a window — bubble or panel only)
- ❌ Dropdown menus (use AskUserQuestion-style options grid)
- ❌ Toast notifications (bubble already IS the toast)
- ❌ Loading spinners (use the think state on the mouse)
- ❌ Progress bars except for the countdown
- ⚠️ ~~Settings page (V1 has only Onboarding; no in-app settings)~~ —— v0.1.26+ 已有 Hub /
  Picker / Status / Tasks 等**二级管理窗**（非 overlay、各自独立窗口）。overlay 本体仍坚持
  "只有气泡 / panel"，但管理类功能走独立窗口是允许的。

---

## 5 · States and Empty Cases

| Scenario | Visual |
|---|---|
| Idle (no shortcut pressed) | Sleeping mouse, bottom-right corner, only tail visible (clip-path) |
| Listening (recording) | Mouse at trigger position, listen state, bubble: "听着呢" + voice bars |
| ASR transcribing | Mouse listen → think transition, bubble: italic transcript appearing |
| Claude thinking | Mouse think state, bubble: same transcript + dim pulsing |
| Claude streaming | Bubble grows with tokens + pink stream-cursor `▮` |
| Done (Mode A) | Jump animation 1×, bubble swaps to success variant, 3s auto-dismiss |
| Done (Mode B pending) | Write state, warn-variant bubble with countdown |
| Done (Mode B inserting) | Write state, bubble fades, foreground app receives text |
| Done (Mode B blocked) | Block state, danger-variant bubble, no countdown, dismiss after 4s |
| Error (Claude failed) | Block state, danger bubble: error message, no auto-dismiss |
| Error (no Claude CLI) | Onboarding pops with install instructions |

---

## 6 · Do / Don't

| ✅ Do | ❌ Don't |
|---|---|
| Use design tokens for every color/spacing/radius | Inline `#ff8aab` or `padding: 7px 11px` |
| Render mouse at integer multiples of 16px | Render mouse at 50px or 100px (blurry) |
| Match prototype animations within 50ms | "Fast enough" 200ms when spec says 300ms |
| Test bubble width with both 英文 and 中文 | Only test one language |
| Run new components past `prefers-reduced-motion` | Ship motion that ignores accessibility |
| Add new states with full anatomy in this doc | Add states by pasting SVG into a PR without doc update |
| Use system fonts | Bundle Inter, Roboto, or any webfont |
| Cap bubble at 320px wide | Let bubble grow to fill the screen |
| Auto-dismiss short bubbles in 3s | Linger past 3s for short content |
| Mark new session with absent chain icon | Use a separate "new" badge |

---

## 7 · Accessibility

### 7.1 Color Contrast

| Pair | Ratio | Required |
|---|---|---|
| `--text-primary` on `--bubble-bg` | 17.4:1 | ≥ 4.5:1 ✅ |
| `--text-on-dark` on `--surface-deep` | 18.0:1 | ≥ 4.5:1 ✅ |
| `--text-dim` on `--surface-deep` | 7.2:1 | ≥ 4.5:1 ✅ |
| `--accent-primary` on `--bubble-bg` | 3.1:1 | ≥ 3:1 (large text only) ✅ |
| `--text-warn` on `--bubble-warn-bg` | 5.2:1 | ≥ 4.5:1 ✅ |
| `--text-danger` on `--bubble-danger-bg` | 7.1:1 | ≥ 4.5:1 ✅ |

If you introduce a new color, run it through a contrast checker before merging.

### 7.2 Motion

Respect `@media (prefers-reduced-motion: reduce)`:
- Mouse character: slow to 50% speed, reduce rotation amplitude to 1°
- UI hover transforms: disabled
- Streaming cursor: replace blink with steady block
- Countdown bar: keep (safety-critical, never disable)

### 7.3 Keyboard

- All interactive bubble/panel actions have keyboard shortcuts (Esc, Enter, ↑, ↓)
- Onboarding options: Tab to navigate, Space/Enter to select
- Panel input: Esc to collapse, Enter to send, Cmd+↑/↓ to scroll history

### 7.4 Screen Reader

- Mouse SVG: `role="img" aria-label="MouseClaw, {state} state"`
- Bubble: `role="status" aria-live="polite"` for normal answers
- Countdown bar: `role="alert" aria-live="assertive"` with the text being inserted announced
- Session chip: `aria-label="Continuing session 42, turn 2 of 15"`

---

## 8 · Versioning and Updates

- This doc is versioned in lockstep with the app's major-minor (currently **v0.1.4**)
- Any visual change requires updating this doc **in the same PR** as the code change
- Code review must reject PRs that introduce visual changes without doc updates
- The 3 prototype HTML files in `docs/prototypes/` are **reference materials** — they capture intent but this doc is normative when they conflict
- Tokens are **append-only across minor versions**. Removing a token is a major version change

### 8.1 Changelog

| Version | Date | Change |
|---|---|---|
| v0.1.4 | 2026-05-13 | Initial design system (extracted from prototypes 1–3) |

---

## 9 · Quick Reference Cheatsheet

```css
/* Drop this into any new component file as a starting point */
:root {
  /* Surface */
  --surface-deep: #0a0e1a;
  --surface-glass: rgba(255, 255, 255, 0.06);

  /* Bubble */
  --bubble-bg: rgba(255, 255, 255, 0.97);
  --panel-bg: rgba(252, 252, 254, 0.98);
  --bubble-border: rgba(0, 0, 0, 0.06);

  /* Text */
  --text-primary: #1a1a1a;
  --text-on-dark: #f5f5f7;
  --text-dim: #a1a1a8;

  /* Accent */
  --accent-primary: #ff6b9d;
  --accent-soft: #ff9bb8;
  --accent-gradient: linear-gradient(135deg, #ff6b9d, #ff9bb8);

  /* Semantic */
  --success: #5cd6a0;
  --warn: #ffd166;
  --danger: #ff5f57;

  /* Mouse palette (locked) */
  --m-body: #cfcfcf;
  --m-belly: #ffffff;
  --m-ear-in: #ff9bb8;
  --m-eye: #1a1a1a;
  --m-nose: #d63d6a;
  --m-tail: #8a8a8a;
  --m-paw: #ffffff;

  /* Font */
  --font-system: -apple-system, BlinkMacSystemFont, "SF Pro Display",
                 "PingFang SC", sans-serif;
  --font-mono: ui-monospace, "SF Mono", monospace;

  /* Radius */
  --radius-pill: 999px;
  --radius-md: 8px;
  --radius-xl: 12px;
  --radius-3xl: 16px;

  /* Shadow */
  --shadow-bubble: 0 12px 32px rgba(0,0,0,0.35),
                   0 2px 6px rgba(0,0,0,0.2),
                   inset 0 1px 0 rgba(255,255,255,0.7);
}
```

End of design system spec.
