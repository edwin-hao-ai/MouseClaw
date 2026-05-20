# Companion 动画手工验证清单 (v0.4+ 全集)

设计原型：[companion-animations-20260519.html](../../prototypes/companion-animations-20260519.html)
（已含 v0.4+ 全集补完状态的触发按钮）

实现提交：
- `149e957` 第一刀（眼球追 / 打字点头 / 贴近 / 闲置睡）
- `ac85660` 点击 / drowsy / worried / picker
- `c553d90` sleep-state companion gate 修复
- `ef51002` 坐标系修复（Rust 算窗口本地坐标）
- `a9deb63` 全集补完（爱心 / 伸懒腰 / 晕 / 抬头 / 小跳 / 亲密度 / 2h 提醒）

---

## 状态全集（11 个 companion state + 3 个附加系统）

| # | 状态 / 功能 | 触发 | 视觉 | 优先级 |
|---|------------|------|------|--------|
| 1 | sleep | 键鼠静止 10s（深夜 5s） | 闭眼 + 瘫 + ZZZ | 最高 |
| 2 | waking | sleep/drowsy → 醒 | 一次性伸懒腰拉伸回弹 | 2 |
| 3 | dizzy | 鼠标速度 >2400px/s | 整体摇晃 + 眼睛转圈，不追鼠标 | 3 |
| 4 | hop | 一段打字 burst 结束 | 小跳一下（"提交完成"近似） | 4 |
| 5 | clicked | 点鼠标 <0.25s 内 | 双耳抽 + 微缩 | 5 |
| 6 | worried | 连续狂敲/退格 ≥1.5s | 歪头担心 | 6 |
| 7 | typing | sinceKey <0.4s | 220ms 节奏点头 | 7 |
| 8 | excited | 光标 <30px | 上抬 + scale 1.08 | 8 |
| 9 | alert | 光标 <80px | 抬头一点 | 9 |
| 10 | glance | 过整点 40% 概率 | 抬头看一眼 2s（仅 idle 时） | 10 |
| 11 | drowsy / idle | 默认态（深夜/白天） | 半垂慢摇 / 呼吸 | 最低 |
| + | ❤️ 爱心 | 点桌宠 | 心上飘 700ms | （独立，非 state） |
| + | 亲密度 | 互动累计 | level 1-3 眼睛微大 | （CSS class，静止时生效） |
| + | 冷落委屈 | >3 天没互动 | 低头略暗 | （覆盖眼睛） |
| + | LongFocus nudge | 连续专注 2h + 停顿 | 气泡"歇会儿" | （nudge 子系统，非 companion） |

眼球追鼠标贯穿所有非 sleep/drowsy/dizzy/neglected 态（tanh 分量映射，X/Y 解耦）。

---

## A · Prototype 自查（preview 面板 / `open` HTML）

prototype 现在有两排控制按钮。9 款皮肤同屏，逐个验证：

- [ ] 移鼠标 → 9 只眼球都跟手追（左/右/上/下方向都对）
- [ ] 代码框打字 → 9 只点头
- [ ] 鼠标贴近某只 → 该只兴奋，其他不动
- [ ] 停手 → 全员入睡 + ZZZ
- [ ] **醒来伸懒腰** 按钮 → 9 只拉伸回弹
- [ ] **甩动晕 ×_×** 按钮 → 9 只摇晃 + ×_× 眼
- [ ] **回车小跳** 按钮 → 9 只跳一下
- [ ] **整点抬头** 按钮 → 9 只抬头 2s
- [ ] **深夜 drowsy** 按钮 → 9 只半垂慢摇
- [ ] **冷落委屈** 按钮 → 9 只低头略暗
- [ ] **亲密度 +1** 按钮 → 9 只眼睛逐级变大
- [ ] 点任意桌宠头 → ❤️ 上飘

> **Chrome 扩展 E2E 复验（2026-05-20）**：经本地 http server（file:// 被扩展拒）
> 用 Claude-in-Chrome 加载 prototype，确认：
> - 9 款皮肤齐全 ✅
> - 7 个新状态按钮全部就位（醒来/晕/小跳/抬头/drowsy/委屈/亲密度）✅
> - 点「深夜 drowsy」→ 9 只全员 drowsy（截图 07）✅
> - 点「亲密度 +1」×3 → 9 只眼睛同步放大（DOM `data-intimacy=3` × 9 + 肉眼可见）✅
>
> **限制**：后台标签页 `requestAnimationFrame` 被 Chrome 节流，**瞬时态**
> （waking 700ms / dizzy 1600ms / hop 480ms）窗口短于截图延迟，截不到静帧 ——
> 这些由 69 个前端单测覆盖逻辑 + preview 面板/真 app 肉眼验证。长窗口态
> （drowsy/neglected 6s）可截。早期 6 张截图（01–06）覆盖原始状态仍有效。

---

## B · 真 Tauri app 实测（DMG / `bun tauri dev`）

桌宠在屏幕右下角，**不用按快捷键**，idle 静默状态下：

### 被动感知
- [ ] 鼠标在屏幕各方向移动 → 眼球追（清晰可见 ~5px 偏移）
- [ ] 9 款皮肤逐个切（tray → 选皮肤）→ 每款眼球都追

### 主动反应
- [ ] 点鼠标任意位置 → 桌宠耳朵抽一下
- [ ] 任何 app 打字 → 节奏点头；打完一段停手 → 小跳
- [ ] 鼠标快速大幅甩动 → 桌宠晕（摇晃 + ×_×）
- [ ] 鼠标慢慢靠近桌宠 → 80px 抬头 / 30px 兴奋
- [ ] 点桌宠本体 → ❤️ 上飘

### 情境
- [ ] 现在深夜（>23:00 或 <6:00）→ 桌宠 drowsy 半垂慢摇 + 5s 静止就睡
- [ ] sleep 后动鼠标 → 先伸懒腰再恢复（不是瞬切）
- [ ] 整点附近偶尔抬头看一眼（概率性，不保证每次）

### 长期 / 累积
- [ ] 多用几天，互动多了眼睛会略大（亲密度）
- [ ] 连续专注工作 2h，在停下打字的瞬间 → "歇会儿" 提醒气泡
- [ ] 超 3 天没碰 → 桌宠委屈低头（下次互动恢复）

### 不打扰回归（关键）
- [ ] 按快捷键进 listening/thinking/talk → companion 动画**不抢戏**
- [ ] 系统"减少动态效果"开 → 所有 companion 动画静止（仅状态指示）

### 隐私 / 性能
- [ ] 待机 CPU < 1%（30 FPS tick）
- [ ] 首次启动**不弹** Accessibility 权限（只用 CGEventSource，不读键内容）

---

## C · 已知边界 / 刻意不做（明确，不藏）

- **精确 Enter / Backspace 检测**：刻意不做 —— 要读键盘**内容**，跨隐私红线。
  回车用 burst-end 近似（hop）、退格用打字风暴近似（worried）替代。
- **Windows / Linux**：companion.rs 非 macOS 分支返回零值，V2 各平台实现。
- **多屏副屏**：`cursor_xy()` Y 翻转用主屏高度，桌宠在副屏时眼球追有 Y 偏移，follow-up。
- **亲密度持久化**：localStorage（非 config.json）—— 丢了无所谓，纯视觉调味。

---

## D · 自动化兜底（全绿才算完）

```bash
bash tests/acceptance/companion-animations-e2e.sh   # 27/27 PASS
bunx vitest run                                      # 69 passed
cargo test --manifest-path src-tauri/Cargo.toml --lib  # 160 passed
```
