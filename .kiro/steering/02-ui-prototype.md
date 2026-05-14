---
inclusion: auto
description: UI/UX 开发流程规范
fileMatchPattern: "*.tsx,*.rs"
---

# UI/UX 开发流程

## 硬规则：先 prototype 再写代码

任何涉及视觉/交互的功能，必须按以下流程：

### 适用范围
- 像素老鼠的新动画状态
- 气泡 UI 样式、字体、动效
- Panel 展开形态、对话历史
- Onboarding / 弹窗
- 错误状态、空状态、loading 效果
- Session chip 视觉提示
- Mode B 倒数 UI

### Prototype 规范
1. **单文件 HTML**（self-contained，无外部 CDN）
2. 放在 `docs/prototypes/<feature>-YYYYMMDD.html`
3. 用 inline SVG 或 CSS 画像素艺术（`shape-rendering: crispEdges`），禁止 emoji 占位
4. 用 CSS `@keyframes` 做真动画，不是静态截图
5. 同时展示所有相关状态（睡/听/想/跳同屏摆开）
6. 背景模拟真实桌面场景

### 评审流程
1. 写完 HTML → `open docs/prototypes/<feature>.html` 让用户看
2. 用户反馈 → 改 → 循环直到用户说"对了，照这个做"
3. **只有用户拍板后才能写 Rust/Tauri 实现**
4. 实现必须和 prototype 视觉一致——不一致是 bug

### 禁止做法
- ❌ 先写 Rust 端再调 UI
- ❌ 口述样式让用户想象
- ❌ 用 ASCII art 代替真实 prototype