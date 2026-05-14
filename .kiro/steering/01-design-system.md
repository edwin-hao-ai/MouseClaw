---
inclusion: auto
description: MouseClaw 视觉设计系统规范
fileMatchPattern: "*.css,*.tsx,*.ts"
---

# MouseClaw 视觉设计系统

## 核心原则（优先级：必须遵守）
1. **DESIGN.md 是唯一信源**：所有 UI 工作必须使用 DESIGN.md 里的 design tokens，禁止硬编码颜色/间距/圆角
2. **8 色像素老鼠**：只能使用 `--m-body`, `--m-belly`, `--m-ear-out`, `--m-ear-in`, `--m-eye`, `--m-nose`, `--m-tail`, `--m-paw`，不能加第 9 色
3. **系统字体**：只用 `-apple-system, BlinkMacSystemFont, "SF Pro Display", "PingFang SC", "Microsoft YaHei", sans-serif`

## 必须遵守的 tokens
```
// 颜色
--bubble-bg: rgba(255, 255, 255, 0.97)
--panel-bg: rgba(252, 252, 254, 0.98)
--text-primary: #1a1a1a
--accent-primary: #ff6b9d
--success: #5cd6a0
--warn: #ffd166
--danger: #ff5f57

// 字号（用这些，不直接写 px）
--text-display: 38px
--text-headline: 22px
--text-title: 16px
--text-body-lg: 14px
--text-body: 13px
--text-body-sm: 12px
--text-meta: 11px

// 间距（8px 基数网格）
--space-1: 4px, --space-2: 8px, --space-3: 12px, --space-4: 16px, ...

// 圆角
--radius-pill: 999px, --radius-sm: 6px, --radius-md: 8px, --radius-xl: 12px, --radius-3xl: 16px
```

## 禁止的做法
- ❌ 硬编码 `#ff8aab` 或 `padding: 7px 11px`
- ❌ 渲染老鼠为非 16/32/48/64/96/128px
- ❌ 用 Google Fonts 或任何 webfont
- ❌ 超过 320px 宽的气泡