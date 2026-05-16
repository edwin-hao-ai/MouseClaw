/**
 * StatusView · 系统状态 / 健康检查窗
 *
 * 一眼看到所有能力（claude / agent-browser / Chrome CDP / 三项权限）的就绪状态。
 * 每行红/绿灯 + 一句解释 + 「去解决」操作按钮。1.5s 自动 refresh，授权完立即变绿。
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "./i18n";

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
  const t = useT();
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
      title: t("status.row.claude.title"),
      ok: caps.claude_cli,
      goodNote: t("status.row.claude.good"),
      badNote: t("status.row.claude.bad"),
    },
    {
      key: "cdp",
      title: t("status.row.cdp.title"),
      ok: caps.chrome_cdp,
      goodNote: t("status.row.cdp.good"),
      badNote: t("status.row.cdp.bad"),
      action: caps.chrome_cdp
        ? undefined
        : { label: enabling ? t("status.row.cdp.enabling") : t("status.row.cdp.action"), onClick: handleEnableBrowser },
    },
    {
      key: "ab",
      title: t("status.row.agentbrowser.title"),
      ok: caps.agent_browser,
      goodNote: t("status.row.agentbrowser.good"),
      badNote: t("status.row.agentbrowser.bad"),
    },
    {
      key: "acc",
      title: t("status.row.acc.title"),
      ok: perms.accessibility,
      goodNote: t("status.row.acc.good"),
      badNote: t("status.row.acc.bad"),
      action: perms.accessibility ? undefined : {
        label: t("status.action.go"), onClick: () => handlePerm("accessibility"),
      },
    },
    {
      key: "scr",
      title: t("status.row.scr.title"),
      ok: perms.screen_recording,
      goodNote: t("status.row.scr.good"),
      badNote: t("status.row.scr.bad"),
      action: perms.screen_recording ? undefined : {
        label: t("status.action.go"), onClick: () => handlePerm("screen_recording"),
      },
    },
    {
      key: "mic",
      title: t("status.row.mic.title"),
      ok: perms.microphone,
      goodNote: t("status.row.mic.good"),
      badNote: t("status.row.mic.bad"),
      action: perms.microphone ? undefined : {
        label: t("status.action.go"), onClick: () => handlePerm("microphone"),
      },
    },
  ];

  const allGood = rows.filter(r => r.key !== "ab").every(r => r.ok);

  return (
    <div style={{
      maxWidth: 520, margin: "0 auto", padding: "32px 24px",
      fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif", color: "#2b2622",
    }}>
      <h1 style={{ fontSize: 24, margin: "0 0 4px" }}>{t("status.title")}</h1>
      <p style={{ color: "#8a8178", marginTop: 0, fontSize: 13 }}>
        {t("status.subtitle")}
      </p>

      <div style={{
        padding: "10px 14px", borderRadius: 10, marginBottom: 16,
        background: allGood ? "#e6f4ec" : "#fdf3e0",
        color: allGood ? "#3fa66a" : "#d9952b", fontWeight: 600,
      }}>
        {allGood ? t("status.all_good") : t("status.partial")}
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
        {t("status.tip.first_use")}
      </p>
    </div>
  );
}
