/**
 * StatusView · 系统状态 / 健康检查窗
 *
 * 一眼看到所有能力（claude / agent-browser / Chrome CDP / 三项权限）的就绪状态。
 * 每行红/绿灯 + 一句解释 + 「去解决」操作按钮。1.5s 自动 refresh，授权完立即变绿。
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface CapabilityStatus {
  claude_cli: boolean;
  agent_browser: boolean;
  chrome_cdp: boolean;
}
interface PermissionStatus {
  accessibility: boolean;
  screen_recording: boolean;
  microphone: boolean;
}

type Row = {
  key: string;
  title: string;
  ok: boolean;
  goodNote: string;
  badNote: string;
  action?: { label: string; onClick: () => void };
};

export default function StatusView() {
  const [caps, setCaps] = useState<CapabilityStatus>({
    claude_cli: false, agent_browser: false, chrome_cdp: false,
  });
  const [perms, setPerms] = useState<PermissionStatus>({
    accessibility: false, screen_recording: false, microphone: false,
  });
  const [enabling, setEnabling] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [c, p] = await Promise.all([
        invoke<CapabilityStatus>("capability_status"),
        invoke<PermissionStatus>("check_permissions"),
      ]);
      setCaps(c); setPerms(p);
    } catch { /* browser-only mode */ }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 1500);
    return () => clearInterval(t);
  }, [refresh]);

  const handleEnableBrowser = useCallback(async () => {
    setEnabling(true);
    try {
      await invoke("enable_browser_automation");
      // 等 Chrome 起来，再 refresh 一次
      setTimeout(() => { refresh(); setEnabling(false); }, 800);
    } catch (e) {
      alert(`启用失败：${e}\n\n建议手动确认已装 Google Chrome。`);
      setEnabling(false);
    }
  }, [refresh]);

  const handlePerm = useCallback(async (name: keyof PermissionStatus) => {
    try { await invoke("request_permission", { name }); }
    catch (e) { console.warn("request_permission failed:", e); }
  }, []);

  const rows: Row[] = [
    {
      key: "claude",
      title: "Claude Code CLI",
      ok: caps.claude_cli,
      goodNote: "已就绪 — AI 后端可用",
      badNote: "未安装。跑：npm i -g @anthropic-ai/claude-code",
    },
    {
      key: "cdp",
      title: "Chrome 浏览器自动化（推荐路径）",
      ok: caps.chrome_cdp,
      goodNote: "已连上你的 Chrome — 可直接操作真浏览器",
      badNote: "未启用。让 MouseClaw 自动开一个带 CDP 的专用 Chrome profile。",
      action: caps.chrome_cdp
        ? undefined
        : { label: enabling ? "启用中…" : "🌐 一键启用", onClick: handleEnableBrowser },
    },
    {
      key: "ab",
      title: "agent-browser CLI（备选 · headless）",
      ok: caps.agent_browser,
      goodNote: "可用作 headless 浏览器（不连你当前 Chrome）",
      badNote: "未装（可选）。跑：npm i -g @vercel/agent-browser",
    },
    {
      key: "acc",
      title: "权限 · 辅助功能",
      ok: perms.accessibility,
      goodNote: "全局快捷键能工作",
      badNote: "未授权。前往「系统设置 → 隐私 → 辅助功能」开启。",
      action: perms.accessibility ? undefined : {
        label: "去开启", onClick: () => handlePerm("accessibility"),
      },
    },
    {
      key: "scr",
      title: "权限 · 屏幕录制",
      ok: perms.screen_recording,
      goodNote: "截屏给 AI 看可用",
      badNote: "未授权。前往「系统设置 → 隐私 → 屏幕录制」开启。",
      action: perms.screen_recording ? undefined : {
        label: "去开启", onClick: () => handlePerm("screen_recording"),
      },
    },
    {
      key: "mic",
      title: "权限 · 麦克风",
      ok: perms.microphone,
      goodNote: "语音输入可用",
      badNote: "未授权。前往「系统设置 → 隐私 → 麦克风」开启。",
      action: perms.microphone ? undefined : {
        label: "去开启", onClick: () => handlePerm("microphone"),
      },
    },
  ];

  const allGood = rows.filter(r => r.key !== "ab").every(r => r.ok);

  return (
    <div style={{
      maxWidth: 520, margin: "0 auto", padding: "32px 24px",
      fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif", color: "#2b2622",
    }}>
      <h1 style={{ fontSize: 24, margin: "0 0 4px" }}>🦞 系统状态</h1>
      <p style={{ color: "#8a8178", marginTop: 0, fontSize: 13 }}>
        每 1.5s 自动刷新。授权完成后这里会立刻变绿。
      </p>

      <div style={{
        padding: "10px 14px", borderRadius: 10, marginBottom: 16,
        background: allGood ? "#e6f4ec" : "#fdf3e0",
        color: allGood ? "#3fa66a" : "#d9952b", fontWeight: 600,
      }}>
        {allGood ? "✓ 全部就绪 — 可以正常使用了" : "⚠ 还有项目未就绪 — 见下方"}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {rows.map(r => (
          <div key={r.key} style={{
            display: "flex", alignItems: "center", gap: 12,
            padding: "12px 14px", background: "#fff", border: "1px solid #ece6dd",
            borderRadius: 12,
          }}>
            <span style={{
              width: 12, height: 12, borderRadius: 999,
              background: r.ok ? "#3fa66a" : "#e8638c", flexShrink: 0,
            }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600, fontSize: 14 }}>{r.title}</div>
              <div style={{ fontSize: 12, color: "#8a8178", marginTop: 2 }}>
                {r.ok ? r.goodNote : r.badNote}
              </div>
            </div>
            {!r.ok && r.action && (
              <button
                onClick={r.action.onClick}
                disabled={enabling}
                style={{
                  padding: "6px 12px", borderRadius: 8, border: "none",
                  background: "#e8638c", color: "#fff", fontWeight: 600,
                  fontSize: 12, cursor: enabling ? "wait" : "pointer", whiteSpace: "nowrap",
                }}
              >
                {r.action.label}
              </button>
            )}
          </div>
        ))}
      </div>

      <p style={{ marginTop: 20, fontSize: 12, color: "#8a8178", lineHeight: 1.6 }}>
        💡 浏览器自动化第一次启用后，会开一个**专用 debug profile** 的 Chrome（跟你日常用的隔离开）。
        在那个 Chrome 里登录一次你要操作的网站，之后 MouseClaw 就能用 chrome-devtools MCP 帮你操作了。
      </p>
    </div>
  );
}
