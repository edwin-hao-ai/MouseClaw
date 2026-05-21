---
inclusion: auto
description: MouseClaw 项目概述和核心约束
---

# MouseClaw 项目概述

## 项目定位
- **产品**：像素小老鼠 AI 桌面助手（🦞）
- **平台**：macOS 优先，Windows V2
- **形态**：纯后台进程，无 dock 图标，menubar 托盘 + 透明 overlay 窗口

## 核心约束
- 安装包 < 30MB（不含 sherpa ASR 模型）
- 常驻内存 < 600MB（含模型）/ < 400MB（待机）
- 待机 CPU < 1%
- 触发延迟 < 100ms