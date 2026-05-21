---
inclusion: auto
description: 技术选型和实现约束
fileMatchPattern: "*.rs"
---

# 技术选型和实现约束

## 技术栈（已锁定，不可更改）
- **GUI**：Tauri 2
- **录音转写**：cpal + sherpa-onnx 流式 Zipformer（双语 zh-en）+ CT-Transformer 标点，全本地（v0.3 起删 Whisper）
- **截屏**：macOS screencapture CLI（完整屏幕，不是 300×300 局部）
- **AI 后端**：Claude Code CLI（v2.1.138+），**不暴露给用户选择**

## macOS 权限处理（重要）
- **Accessibility 权限**：全局快捷键必须，在 setup 阶段检查/请求
- **麦克风权限**：cpal 录音必须，在首次录音前检查/请求
- **屏幕录制权限**：screencapture 必须，在首次截屏前检查/请求
- 权限检查失败时，应先请求权限而不是直接报错

## 输出模式
- **模式 A（默认）**：Claude 回复无 `[INSERT_AT_CURSOR]` 标记 → 气泡显示结果摘要
- **模式 B**：Claude 回复含 `[INSERT_AT_CURSOR]` 块 → 3 秒倒数 → 写回光标

### 模式 B 关键约束
- 永远不要在没有 confirm UI 的情况下写入
- 永远不要写入终端（Terminal/iTerm/Warp 强制走 A 模式）
- 用 CGEvent 绕过 IME，不污染剪贴板

## Session 管理
- 不使用 `claude --resume/--continue`，自己拼接上下文
- 同 session 续聊：最近 ≤10 轮拼到 prompt 前
- 超过 8K token 时丢最早轮次