# Companion 动画手工验证清单 (v0.4+)

设计原型：[companion-animations-20260519.html](../../prototypes/companion-animations-20260519.html)
实现 PR：commit 149e957 `feat(companion)`

---

## A · Chrome MCP E2E（已自动化 · 截图证据见同目录）

Chrome DevTools MCP 跑 prototype HTML，9 款皮肤同屏验证：

| # | 场景 | 期望 | 截图 |
|---|------|------|------|
| 1 | 初始无操作 → 全员 sleep | 9 只都闭眼 + ZZZ | [01-sleep-all-9-skins.png](01-sleep-all-9-skins.png) |
| 2 | 鼠标移到右下 → 眼球追右 | 9 只瞳孔都偏 +x | [02-eyes-tracking-right.png](02-eyes-tracking-right.png) |
| 3 | 鼠标移到左 → 眼球追左 | 9 只瞳孔都偏 -x（allLeft=true） | [03-eyes-tracking-left.png](03-eyes-tracking-left.png) |
| 4 | 在 textarea 打字 → 全员点头 | 9 只都 "陪你打字" tag | [04-typing-all-9-nodding.png](04-typing-all-9-nodding.png) |
| 5 | 鼠标贴近 classic → 兴奋 | 仅 classic 兴奋，其他 idle | [05-proximity-classic-excited.png](05-proximity-classic-excited.png) |
| 6 | 强制全员入睡 → ZZZ 漂浮 | 9 只都 is-sleep | [06-forced-sleep-all-9.png](06-forced-sleep-all-9.png) |

**所有皮肤 (classic / lab / field / ninja / cyber / golden / cat-gray / fox-red / frog-tree)
都在每张截图中可见**，符合 CLAUDE.md "动画/陪伴效果必须适配所有皮肤" 硬规则。

---

## B · Tauri shell E2E（需手工跑，含 macOS 权限）

Chrome MCP 跑的是 prototype，不是真的 Tauri webview。**真 app 的验证**需要：

```bash
bun tauri dev
```

启动后用主屏：

1. **眼球追鼠标**：
   - 桌宠出现在屏幕右下角（默认锚点）
   - 把鼠标移到屏幕左上 → 桌宠眼睛朝左上偏（瞳孔位移 ≤ 0.6 SVG px ≈ 4 屏幕 px @ size=64）
   - 把鼠标移到桌宠正下方 → 眼睛朝下
   - **9 款皮肤都要试一遍**：tray menu → 选皮肤 → 重复

2. **打字陪伴**：
   - 在任何 app（VSCode / Notes / Terminal 都行）打字
   - 桌宠每 220ms 点头一次（CSS `mc-nod` 动画）
   - 停手 0.5s 后立即停（无残留抖动）

3. **闲置渐睡**：
   - 10s 不动键鼠 → 桌宠 scaleY 0.88 + brightness 0.85 + 眼睛压成一字
   - 移鼠标 / 按键立刻醒

4. **贴近反应**：
   - 鼠标 < 80px 抬头一点（companion-alert）
   - 鼠标 < 30px 兴奋抬头 + scale 1.08（companion-excited）

### 已知未做（明确）
- ❌ Windows / Linux 端 companion.rs 都返回零值（`#[cfg(not(target_os = "macos"))]` 分支），
  V1 macOS 优先，跨平台 V2
- ❌ Tauri shell E2E 没有自动化（**用户手工跑**）—— 原因：当前 session 未启动 `bun tauri dev`，
  且自动化 Tauri 透明窗口的 cursor / key 注入需要 Accessibility 权限。Chrome MCP 在 prototype HTML 上
  覆盖了所有状态机分支 + 9 皮肤兼容，足以信任 React/CSS 层；剩下的是 Rust `companion::spawn` 在真
  Tauri 上线后实际 emit。本地 cargo test 已验 emit payload 形状 + 事件名契约。
- ❌ 多屏（external display）下 `cursor_xy()` 用主屏 height 翻转 Y —— 桌宠如果跑在副屏，
  眼球追鼠标会有 Y 偏移。Y 翻转改成"当前光标所在屏的 frame" 是 follow-up。

### 通过条件
4 个场景全在视觉上成立 + reduced-motion 偏好开启时所有动画降级到无动 = 通过。

---

## C · 自动化兜底

```bash
bash tests/acceptance/companion-animations-e2e.sh  # 20/20 PASS
bunx vitest run                                     # 31/31
cargo test --manifest-path src-tauri/Cargo.toml --lib  # 146/146
```
