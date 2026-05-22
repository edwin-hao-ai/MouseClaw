# `~/.mouseclaw/provider.env` — share one provider across all backends

MouseClaw 不读 OS 安装的 shell rc (zshrc / fish.config / etc.)，因为 LaunchAgents /
托盘进程不走交互 shell。要给各后端 CLI 设 provider key，最稳的办法就是写到
`~/.mouseclaw/provider.env` —— 启动时 MouseClaw 会把这里所有 KEY=VALUE 灌到 spawn 的
CLI 子进程里（已在 OS env 里的同名变量不覆盖，shell rc 优先）。

> v0.4.4 起后端从 4 个扩到 13 个。**注意**：少数后端不靠 API key 而靠各自的 **login 流程**
> （GitHub Copilot CLI 走 GitHub 账号；Antigravity CLI 走 Google 账号）—— 这类 provider.env
> 帮不上忙，得先在 Terminal 跑它自己的登录命令。下面表格里标了每个后端的认证方式。

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

## 各后端认证方式

✅ = v0.1.25 实测过（2026-05-17）。🔹 = v0.4.4 新增，调用命令已按官方文档核对到
2026-05-21，但凭证未逐一实测 —— 按下表的认证变量/登录命令配好即可。

| 后端 | 认证方式（provider.env 里设的 key / 或 login 命令） | 备注 |
|---|---|---|
| Claude Code CLI ✅ | `claude login` 或 `ANTHROPIC_API_KEY` | 默认后端 |
| OpenAI Codex CLI ✅ | `OPENAI_*` + `~/.codex/config.toml` 配 provider（`wire_api="responses"`） | 见上面 gateway 段 |
| Hermes (Nous Research) ✅ | `hermes --provider ai-gateway` 原生支持 `AI_GATEWAY_API_KEY` | 最干净 |
| OpenClaw CLI ✅ | `OPENAI_*`（走 OpenAI 兼容） | model id 必须是 gateway 上存在的 |
| Gemini CLI 🔹 | `GEMINI_API_KEY`（或 `gemini` 内的 Google 登录） | 6/18 起 Gemini CLI 免费额度迁到 Antigravity |
| GitHub Copilot CLI 🔹 | **`copilot` 登录 GitHub 账号**（或 `GH_TOKEN`）—— 非 API key | 需 Copilot 订阅；provider.env 帮不上 |
| OpenCode 🔹 | 看它配置里选的 provider：`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `OPENROUTER_API_KEY` 等 | `opencode auth login` 也行 |
| Cline CLI 🔹 | 选定 provider 的 key（`OPENROUTER_API_KEY` / `ANTHROPIC_API_KEY` …） | CLI 2.0 自带免费 Kimi K2.5，可零配置先试 |
| Kimi Code CLI 🔹 | `MOONSHOT_API_KEY`（或 `kimi` 内登录 code.kimi.com） | Moonshot 平台 key |
| Kiro CLI 🔹 | `KIRO_API_KEY`（headless）或 `kiro-cli login` | AWS 账号 / API key |
| Antigravity CLI 🔹 | **`agy` 登录 Google 账号**（headless 用缓存 token）—— 非 API key | provider.env 帮不上 |
| Mistral Vibe 🔹 | `MISTRAL_API_KEY` | Devstral |
| Pi Coding Agent 🔹 | 看选定 model 的 provider key（`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` …） | 统一 LLM API 读标准 provider 变量 |

## 直接走原生 provider key（不用 gateway）

如果你想给某个后端单独配 key，按它原生的环境变量来就行 —— 都会被原样灌进去：

```bash
# Anthropic 直连（Claude / OpenCode / Cline / Pi 都认）
ANTHROPIC_API_KEY=sk-ant-xxx

# OpenRouter（Hermes 默认就走这个；Cline / OpenCode 也认）
OPENROUTER_API_KEY=sk-or-xxx

# 新后端各自的原生 key（v0.4.4）
GEMINI_API_KEY=...            # Gemini CLI
MISTRAL_API_KEY=...           # Mistral Vibe
MOONSHOT_API_KEY=...          # Kimi Code CLI
KIRO_API_KEY=...              # Kiro CLI（headless）

# OpenAI / Groq / DeepSeek / etc. 任何 KEY=VALUE 都会被原样灌进去
OPENAI_API_KEY=...
GROQ_API_KEY=...
```

> GitHub Copilot CLI 和 Antigravity CLI 不在这里配 —— 它们走账号登录：
> `copilot`（GitHub 账号）/ `agy`（Google 账号）。在 Terminal 里跑一次登录即可，
> token 缓存后托盘进程也能用。

## 安全提示

- 文件 mode 设 `600` —— 别人读不到
- 不要 `git add` 这个文件（它在用户 home 不在仓库，但还是提一下）
- MouseClaw 启动时**不**覆盖已存在的 OS env，所以你 shell rc 里设的同名 key 优先
