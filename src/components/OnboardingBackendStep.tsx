/**
 * Onboarding Step 2 · AI 后端选择
 *
 * v0.4.4 「已安装优先」设计：
 *   - 检测完成 + ≥1 个已安装 → 只展示已安装的，折叠展开未安装（带 copyable 安装命令）
 *   - 检测完成 + 0 个已安装 → 空状态：推荐 Claude Code + 「重新检测」按钮 + 折叠全量列表
 *   - 检测中 → 「检测中…」占位
 *
 * 从 Onboarding.tsx 抽出（Onboarding.tsx 超 800 行硬上限 · 见 CLAUDE.md）。
 */
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PixelMouse } from "./PixelMouse";
import type { BackendChoice, SkinId } from "../types";
import { useT } from "../i18n";

export const BACKENDS_META: Array<{
  id: BackendChoice; label: string; descKey: string; tag?: string;
}> = [
  { id: "claude-cli",      label: "Claude Code CLI",              descKey: "claude",       tag: "common.recommended" },
  { id: "codex-cli",       label: "OpenAI Codex CLI",             descKey: "codex" },
  { id: "gemini-cli",      label: "Gemini CLI",                   descKey: "gemini" },
  { id: "copilot-cli",     label: "GitHub Copilot CLI",           descKey: "copilot" },
  { id: "opencode-cli",    label: "OpenCode",                     descKey: "opencode-sst" },
  { id: "cline-cli",       label: "Cline CLI",                    descKey: "cline" },
  { id: "kimi-cli",        label: "Kimi Code CLI",                descKey: "kimi" },
  { id: "kiro-cli",        label: "Kiro CLI",                     descKey: "kiro" },
  { id: "antigravity-cli", label: "Antigravity CLI",              descKey: "antigravity" },
  { id: "vibe-cli",        label: "Mistral Vibe CLI",             descKey: "vibe" },
  { id: "pi-agent",        label: "Pi Coding Agent",              descKey: "pi" },
  { id: "openclaw-cli",    label: "OpenClaw CLI",                 descKey: "openclaw" },
  { id: "hermes-agent",    label: "Hermes Agent (Nous Research)", descKey: "hermes" },
];

export function backendDesc(id: BackendChoice, en: boolean): string {
  switch (id) {
    case "claude-cli":
      return en
        ? "Most mature · native agentic + image reading. Needs claude installed & logged in."
        : "最成熟 · 原生 agentic + 读图。需已装并登录 claude。";
    case "codex-cli":
      return en
        ? "codex exec non-interactive mode. Needs npm i -g @openai/codex + OpenAI key."
        : "codex exec 非交互模式。需 npm i -g @openai/codex 并配好 key。";
    case "openclaw-cli":
      return en
        ? "openclaw agent --local. Needs npm i -g openclaw + provider key in shell."
        : "openclaw agent --local。需 npm i -g openclaw 并配好 provider key。";
    case "hermes-agent":
      return en
        ? "Hermes -z one-shot mode. Self-improving agent from Nous Research. Needs hermes installed + setup."
        : "hermes -z 单次模式。Nous Research 的自学习 agent。需先装 hermes 并配 provider key。";
    case "gemini-cli":
      return en
        ? "gemini -p headless mode. Google's open-source CLI. Needs npm i -g @google/gemini-cli + a Gemini key."
        : "gemini -p 无头模式。Google 官方开源 CLI。需 npm i -g @google/gemini-cli 并配 Gemini key。";
    case "copilot-cli":
      return en
        ? "copilot -p (standalone GitHub Copilot CLI). Needs npm i -g @github/copilot + Copilot subscription."
        : "copilot -p（GitHub 官方独立 Copilot CLI）。需 npm i -g @github/copilot 并有 Copilot 订阅。";
    case "opencode-cli":
      return en
        ? "opencode run, SST's terminal agent. Install via opencode.ai/install. Pick a provider/model in its config."
        : "opencode run，SST 出品的终端 agent。装：opencode.ai/install。在它配置里选 provider/model。";
    case "cline-cli":
      return en
        ? "cline -y headless mode. The autonomous agent's standalone CLI. Needs npm i -g cline + a provider key."
        : "cline -y 无头模式。Cline 自主 agent 的独立 CLI。需 npm i -g cline 并配 provider key。";
    case "kimi-cli":
      return en
        ? "kimi --quiet -p print mode. Moonshot AI's Kimi Code CLI. Install via code.kimi.com + a Kimi key."
        : "kimi --quiet -p 打印模式。Moonshot 月之暗面的 Kimi Code CLI。装：code.kimi.com 并配 Kimi key。";
    case "kiro-cli":
      return en
        ? "kiro-cli chat --no-interactive. AWS's agentic CLI. Install via cli.kiro.dev + sign in / API key."
        : "kiro-cli chat --no-interactive。AWS 的 agent CLI。装：cli.kiro.dev 并登录 / 配 API key。";
    case "antigravity-cli":
      return en
        ? "agy -p mode. Google's Antigravity CLI (successor to Gemini CLI, May 2026). Install via antigravity.google/cli."
        : "agy -p 模式。Google Antigravity CLI（2026-05 接替 Gemini CLI）。装：antigravity.google/cli。";
    case "vibe-cli":
      return en
        ? "vibe --prompt mode. Mistral's open-source CLI (Devstral). Install via uv tool install mistral-vibe + Mistral key."
        : "vibe --prompt 模式。Mistral 开源 CLI（Devstral）。装：uv tool install mistral-vibe 并配 Mistral key。";
    case "pi-agent":
      return en
        ? "pi -p print mode. Lightweight 4-tool coding agent. Needs npm i -g @mariozechner/pi-coding-agent + a provider key."
        : "pi -p 打印模式。轻量四工具编码 agent。需 npm i -g @mariozechner/pi-coding-agent 并配 provider key。";
  }
}

type BackendStatusMap = Record<
  string,
  { installed: boolean; installCmd: string; installUrl: string } | "loading"
>;

// ── 折叠区里的后端卡片列表（未安装 / 全量）────────────────────────────────
function BackendCardList({
  backends, backendStatus, handleCopy, copiedCmd, isEnUi, showMissingPill,
}: {
  backends: typeof BACKENDS_META;
  backendStatus: BackendStatusMap;
  handleCopy: (cmd: string) => void;
  copiedCmd: string | null;
  isEnUi: boolean;
  showMissingPill?: boolean;
}) {
  const t = useT();
  return (
    <div style={{ maxHeight: "28vh", overflowY: "auto", paddingRight: 4, marginTop: 6 }}>
      {backends.map(b => {
        const st = backendStatus[b.id];
        const stObj = st && st !== "loading" ? st : null;
        return (
          <div key={b.id} style={{
            padding: "8px 12px", borderRadius: 10, marginBottom: 6,
            background: "rgba(255,255,255,0.04)",
            border: "1px solid rgba(255,255,255,0.08)",
          }}>
            <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text-on-dark)", marginBottom: 2 }}>
              {b.label}
              {showMissingPill && (
                <span className="ob-install-pill ob-install-missing">
                  {t("backend.missing")}
                </span>
              )}
            </div>
            <div style={{ fontSize: 11.5, color: "var(--text-dim)", marginBottom: stObj?.installCmd ? 6 : 0 }}>
              {backendDesc(b.id, isEnUi)}
            </div>
            {stObj?.installCmd && (
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <code style={{
                  flex: 1, background: "rgba(0,0,0,0.35)", color: "#f5d68a",
                  borderRadius: 5, padding: "4px 8px", fontSize: 11.5,
                  fontFamily: "var(--font-mono, ui-monospace, Menlo, monospace)",
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                  border: "1px solid rgba(255,255,255,0.06)",
                }}>
                  {stObj.installCmd}
                </code>
                <button
                  type="button"
                  onClick={() => handleCopy(stObj.installCmd)}
                  style={{
                    background: copiedCmd === stObj.installCmd
                      ? "rgba(92,214,160,0.18)" : "rgba(255,255,255,0.08)",
                    border: "1px solid rgba(255,255,255,0.15)",
                    borderRadius: 5, padding: "4px 8px",
                    fontSize: 11, cursor: "pointer", whiteSpace: "nowrap",
                    color: "var(--text-dim)", fontFamily: "inherit",
                  }}
                >
                  {copiedCmd === stObj.installCmd ? "✓" : (isEnUi ? "Copy" : "复制")}
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

interface Props {
  skin: SkinId;
  onNext: (backend: BackendChoice) => void;
}

export function OnboardingBackendStep({ skin, onNext }: Props) {
  const t = useT();
  const isEnUi = t("onboarding.shortcut.hint").includes("Hold");
  const [backend, setBackend] = useState<BackendChoice>("claude-cli");
  const [backendStatus, setBackendStatus] = useState<BackendStatusMap>({});
  const [showAllBackends, setShowAllBackends] = useState(false);
  const [copiedCmd, setCopiedCmd] = useState<string | null>(null);
  const [backendRefreshKey, setBackendRefreshKey] = useState(0);

  useEffect(() => {
    const ids: BackendChoice[] = BACKENDS_META.map(b => b.id);
    setBackendStatus(Object.fromEntries(ids.map(id => [id, "loading" as const])));
    ids.forEach((id) => {
      invoke<{ installed: boolean; installCmd: string; installUrl: string }>(
        "check_backend_installed", { backend: id }
      ).then((res) => {
        setBackendStatus(prev => ({ ...prev, [id]: res }));
      }).catch(() => {
        setBackendStatus(prev => ({
          ...prev,
          [id]: { installed: true, installCmd: "", installUrl: "" },
        }));
      });
    });
  }, [backendRefreshKey]);

  // 检测完后自动选中第一个已安装的（按优先级顺序）
  useEffect(() => {
    const allLoaded = BACKENDS_META.every(
      b => backendStatus[b.id] !== undefined && backendStatus[b.id] !== "loading"
    );
    if (!allLoaded) return;
    const curStatus = backendStatus[backend];
    const curInstalled = curStatus && curStatus !== "loading" && curStatus.installed;
    if (!curInstalled) {
      const first = BACKENDS_META.find(b => {
        const st = backendStatus[b.id];
        return st && st !== "loading" && st.installed;
      });
      if (first) setBackend(first.id);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backendStatus]);

  const allLoaded = BACKENDS_META.every(
    b => backendStatus[b.id] !== undefined && backendStatus[b.id] !== "loading"
  );
  const installedBackends = BACKENDS_META.filter(b => {
    const st = backendStatus[b.id];
    return st && st !== "loading" && st.installed;
  });
  const missingBackends = BACKENDS_META.filter(b => {
    const st = backendStatus[b.id];
    return st && st !== "loading" && !st.installed;
  });
  const noneInstalled = allLoaded && installedBackends.length === 0;

  const handleCopy = (cmd: string) => {
    navigator.clipboard.writeText(cmd).catch(() => {});
    setCopiedCmd(cmd);
    setTimeout(() => setCopiedCmd(c => c === cmd ? null : c), 2000);
  };

  // ── 空状态：一个都没装 ────────────────────────────────────────────────
  if (noneInstalled) {
    const claudeStatus = backendStatus["claude-cli"];
    const claudeInstallCmd = (claudeStatus && claudeStatus !== "loading")
      ? claudeStatus.installCmd
      : "npm install -g @anthropic-ai/claude-code";
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="think" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.backend.title")}</h1>
        <p className="ob-subtitle">
          {isEnUi
            ? "No AI CLI detected yet. Install one to get started — Claude Code is the most capable."
            : "还没检测到 AI CLI。装一个就能用了 —— 推荐 Claude Code，功能最全。"}
        </p>

        <div style={{
          background: "rgba(255, 107, 157, 0.08)",
          border: "1.5px solid rgba(255, 107, 157, 0.28)",
          borderRadius: 12, padding: "14px 16px", marginBottom: 12,
        }}>
          <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4, color: "var(--text-on-dark)" }}>
            Claude Code CLI
            <span style={{
              fontSize: 10.5, background: "rgba(92,214,160,0.18)", color: "#5cd6a0",
              borderRadius: 999, padding: "2px 8px", marginLeft: 8, verticalAlign: "middle",
              fontWeight: 700, letterSpacing: "0.04em", textTransform: "uppercase",
            }}>
              {isEnUi ? "Recommended" : "推荐"}
            </span>
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-dim)", marginBottom: 10, lineHeight: 1.55 }}>
            {isEnUi
              ? "Anthropic's official CLI · native agentic + image reading. Run claude login after install."
              : "Anthropic 官方 CLI · 原生 agentic + 读图能力。装完跑 claude login 登录一次。"}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <code style={{
              flex: 1, background: "rgba(0,0,0,0.35)", color: "#f5d68a",
              borderRadius: 6, padding: "6px 10px", fontSize: 12.5,
              fontFamily: "var(--font-mono, ui-monospace, Menlo, monospace)",
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              border: "1px solid rgba(255,255,255,0.08)",
            }}>
              {claudeInstallCmd}
            </code>
            <button
              type="button"
              onClick={() => handleCopy(claudeInstallCmd)}
              style={{
                background: copiedCmd === claudeInstallCmd
                  ? "rgba(92,214,160,0.18)" : "rgba(255,107,157,0.15)",
                border: "1px solid rgba(255,107,157,0.35)",
                borderRadius: 6, padding: "6px 12px",
                fontSize: 12, cursor: "pointer", whiteSpace: "nowrap",
                color: "var(--text-on-dark)", fontFamily: "inherit", fontWeight: 600,
              }}
            >
              {copiedCmd === claudeInstallCmd
                ? (isEnUi ? "Copied ✓" : "已复制 ✓")
                : (isEnUi ? "Copy" : "复制")}
            </button>
          </div>
        </div>

        <button
          type="button"
          className="ob-cta ob-cta-secondary"
          style={{ marginTop: 0 }}
          onClick={() => { setBackendRefreshKey(k => k + 1); setShowAllBackends(false); }}
        >
          {isEnUi ? "🔄 Re-detect installed CLIs" : "🔄 重新检测已安装的 CLI"}
        </button>

        <button
          type="button"
          className="ob-expander-toggle"
          onClick={() => setShowAllBackends(v => !v)}
        >
          {showAllBackends
            ? (isEnUi ? "▲ Collapse" : "▲ 收起")
            : (isEnUi
                ? "▾ Want something else? All 13 backends supported"
                : "▾ 想用别的？全部 13 个后端都支持")}
        </button>
        {showAllBackends && (
          <BackendCardList
            backends={BACKENDS_META}
            backendStatus={backendStatus}
            handleCopy={handleCopy}
            copiedCmd={copiedCmd}
            isEnUi={isEnUi}
          />
        )}

        <button
          type="button"
          className="ob-cta"
          style={{ marginTop: 16 }}
          onClick={() => onNext(backend)}
        >
          {t("onboarding.cta.next_skin")}
        </button>
      </div>
    );
  }

  // ── 正常状态：≥1 个已安装（或检测中）───────────────────────────────────
  return (
    <div className="ob-root">
      <div className="ob-mouse-stage">
        <PixelMouse state="think" size={96} skin={skin} />
      </div>
      <h1 className="ob-title">{t("onboarding.backend.title")}</h1>
      <p className="ob-subtitle">
        {t("onboarding.backend.subtitle")}<br />
        {t("onboarding.backend.uncertain_hint")}
      </p>

      {!allLoaded && installedBackends.length === 0 ? (
        <div style={{
          textAlign: "center", padding: "28px 0",
          color: "var(--text-dim)", fontSize: 13,
        }}>
          {isEnUi ? "Detecting installed CLIs…" : "检测已安装的 CLI…"}
        </div>
      ) : (
        <div
          className="ob-options"
          role="radiogroup"
          aria-label={t("onboarding.backend.title")}
          style={{ maxHeight: "36vh", overflowY: "auto", paddingRight: 4 }}
        >
          {installedBackends.map(b => (
            <button
              key={b.id}
              type="button"
              role="radio"
              aria-checked={backend === b.id}
              className={`ob-option ${backend === b.id ? "selected" : ""}`}
              onClick={() => setBackend(b.id)}
            >
              <span className="ob-label">
                {b.label}
                <span className="ob-install-pill ob-install-ok">{t("backend.installed")}</span>
              </span>
              <span className="ob-perm-desc">{backendDesc(b.id, isEnUi)}</span>
              {b.tag && <span className="ob-tag">{t(b.tag as "common.recommended")}</span>}
            </button>
          ))}
        </div>
      )}

      {allLoaded && missingBackends.length > 0 && (
        <>
          <button
            type="button"
            className="ob-expander-toggle"
            onClick={() => setShowAllBackends(v => !v)}
          >
            {showAllBackends
              ? (isEnUi ? "▲ Collapse" : "▲ 收起")
              : (isEnUi
                  ? `▾ Don't see yours? ${missingBackends.length} more available`
                  : `▾ 没看到想用的？还有 ${missingBackends.length} 个可安装`)}
          </button>
          {showAllBackends && (
            <BackendCardList
              backends={missingBackends}
              backendStatus={backendStatus}
              handleCopy={handleCopy}
              copiedCmd={copiedCmd}
              isEnUi={isEnUi}
              showMissingPill
            />
          )}
        </>
      )}

      <button
        type="button"
        className="ob-cta"
        style={{ marginTop: 16 }}
        onClick={() => onNext(backend)}
      >
        {t("onboarding.cta.next_skin")}
      </button>
    </div>
  );
}
