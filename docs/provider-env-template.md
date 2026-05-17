# `~/.mouseclaw/provider.env` — share one provider across all 4 backends

MouseClaw 不读 OS 安装的 shell rc (zshrc / fish.config / etc.)，因为 LaunchAgents /
托盘进程不走交互 shell。要给 Codex / OpenClaw / Hermes 设 provider key，最稳的办法
就是写到 `~/.mouseclaw/provider.env` —— 启动时 MouseClaw 会把这里所有 KEY=VALUE 灌到
spawn 的 CLI 子进程里。

## 推荐：所有后端走 Vercel AI Gateway

一个 key 转发到任意 model provider。`AI_GATEWAY_BASE_URL` 是 OpenAI 兼容的 `/v1`，
所以 Codex / OpenClaw 把它当 `OPENAI_BASE_URL` 也认。

```bash
mkdir -p ~/.mouseclaw
cat > ~/.mouseclaw/provider.env <<'EOF'
# Vercel AI Gateway (OpenAI-compatible · 一个 key 路由到任意 model)
AI_GATEWAY_BASE_URL=https://ai-gateway.vercel.sh/v1
AI_GATEWAY_API_KEY=vck_xxx

# Codex / OpenClaw 走 OpenAI 兼容接口，把 gateway 当 OpenAI 兜底
OPENAI_BASE_URL=https://ai-gateway.vercel.sh/v1
OPENAI_API_KEY=vck_xxx
EOF
chmod 600 ~/.mouseclaw/provider.env
```

## 测试矩阵（v0.1.25 实测过，2026-05-17）

| 后端 | 启用方式 | 实测命令 | 结果 |
|---|---|---|---|
| Claude Code CLI | 默认；走自己的订阅或 ANTHROPIC_API_KEY | `claude -p "ping"` | ✅ |
| OpenAI Codex CLI | provider.env 里加 OPENAI_* + `~/.codex/config.toml` 配 vercel-gateway provider（wire_api="responses"） | `codex exec ...` | ✅ |
| Hermes (Nous Research) | `hermes --provider ai-gateway` 原生支持 AI_GATEWAY_API_KEY | `hermes --provider ai-gateway -m anthropic/claude-haiku-4.5 -z "ping"` | ✅ |
| OpenClaw CLI | provider.env 里加 OPENAI_*；注意 model id 必须是 gateway 上存在的 | `openclaw agent --local --session-id <x> -m "..."` | ⚠️ model_not_found 时换 id |

## 直接走原生 provider key（不用 gateway）

如果你想给某个后端单独配 key，按它原生的环境变量来就行 —— 都会被原样灌进去：

```bash
# Anthropic 直连
ANTHROPIC_API_KEY=sk-ant-xxx

# OpenRouter（Hermes 默认就走这个）
OPENROUTER_API_KEY=sk-or-xxx

# Gemini / OpenAI / Groq / DeepSeek / etc. 任何 KEY=VALUE 都行
GEMINI_API_KEY=...
GROQ_API_KEY=...
```

## 安全提示

- 文件 mode 设 `600` —— 别人读不到
- 不要 `git add` 这个文件（它在用户 home 不在仓库，但还是提一下）
- MouseClaw 启动时**不**覆盖已存在的 OS env，所以你 shell rc 里设的同名 key 优先
