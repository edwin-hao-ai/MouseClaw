/**
 * StatusView · 系统状态 / 健康检查窗
 *
 * 一眼看到所有能力（claude / agent-browser / OfficeCLI / Chrome CDP / 三项权限）的就绪状态。
 * 每行红/绿灯 + 一句解释 + 「去解决」操作按钮。1.5s 自动 refresh，授权完立即变绿。
 *
 * v0.4.x · agent-browser / officecli 那两行变成可点「装上」—— 走 install_cli 命令，
 * EV_INSTALL_PROGRESS 流式更新行状态：红 → 黄（安装中 + 流式日志）→ 绿。
 */
import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useT } from "./i18n";

interface CapabilityStatus {
  claude_cli: boolean;
  agent_browser: boolean;
  chrome_cdp: boolean;
  officecli: boolean;
}
interface PermissionStatus {
  accessibility: boolean;
  screen_recording: boolean;
  microphone: boolean;
}

type InstallPhase =
  | { phase: "started"; target: string }
  | { phase: "log"; target: string; line: string }
  | { phase: "done"; target: string }
  | { phase: "failed"; target: string; code: string; message: string };

type InstallState = {
  /** "idle" 没动 · "installing" 跑中 · "failed" 出错 · "done" 装完（短暂展示） */
  state: "idle" | "installing" | "failed" | "done";
  /** 最近一条日志（行内展示） */
  lastLog?: string;
  /** 失败码 + 详细消息 */
  errorCode?: string;
  errorMessage?: string;
};

type Row = {
  key: string;
  title: string;
  ok: boolean;
  goodNote: string;
  badNote: string;
  action?: { label: string; onClick: () => void; busy?: boolean };
  /** 安装中要显示的流式行（最新一条 log） */
  installing?: { hint: string };
  /** 失败时的具体错误（替代 badNote） */
  failed?: { message: string };
};

export default function StatusView() {
  const t = useT();
  const [caps, setCaps] = useState<CapabilityStatus>({
    claude_cli: false, agent_browser: false, chrome_cdp: false, officecli: false,
  });
  const [perms, setPerms] = useState<PermissionStatus>({
    accessibility: false, screen_recording: false, microphone: false,
  });
  const [enabling, setEnabling] = useState(false);
  const [installs, setInstalls] = useState<Record<string, InstallState>>({});
  const unlistenRef = useRef<UnlistenFn | null>(null);

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
    const id = setInterval(refresh, 1500);
    return () => clearInterval(id);
  }, [refresh]);

  // 订阅 install-progress 事件 —— 全程流式
  useEffect(() => {
    let active = true;
    (async () => {
      const un = await listen<InstallPhase>("install-progress", (e) => {
        if (!active) return;
        const p = e.payload;
        setInstalls(prev => {
          const next = { ...prev };
          if (p.phase === "started") {
            next[p.target] = { state: "installing" };
          } else if (p.phase === "log") {
            const cur = next[p.target] || { state: "installing" };
            next[p.target] = { ...cur, state: "installing", lastLog: p.line };
          } else if (p.phase === "done") {
            next[p.target] = { state: "done" };
            // 装完立刻刷一次能力，行变绿
            refresh();
          } else if (p.phase === "failed") {
            next[p.target] = {
              state: "failed",
              errorCode: p.code,
              errorMessage: p.message,
            };
          }
          return next;
        });
      });
      unlistenRef.current = un;
    })();
    return () => {
      active = false;
      if (unlistenRef.current) unlistenRef.current();
    };
  }, [refresh]);

  const handleEnableBrowser = useCallback(async () => {
    setEnabling(true);
    try {
      await invoke("enable_browser_automation");
      setTimeout(() => { refresh(); setEnabling(false); }, 800);
    } catch (e) {
      alert(`启用失败：${e}\n\n建议手动确认已装 Google Chrome。`);
      setEnabling(false);
    }
  }, [refresh]);

  const handleInstall = useCallback(async (target: "agent-browser" | "officecli") => {
    setInstalls(prev => ({ ...prev, [target]: { state: "installing" } }));
    try {
      await invoke("install_cli", { target });
    } catch (e) {
      setInstalls(prev => ({
        ...prev,
        [target]: { state: "failed", errorCode: "unknown", errorMessage: String(e) },
      }));
    }
  }, []);

  const handlePerm = useCallback(async (name: keyof PermissionStatus) => {
    try { await invoke("request_permission", { name }); }
    catch (e) { console.warn("request_permission failed:", e); }
  }, []);

  const buildCliRow = (
    key: "ab" | "off",
    target: "agent-browser" | "officecli",
    ok: boolean,
    title: string,
    goodNote: string,
    badNote: string,
  ): Row => {
    const inst = installs[target];
    if (ok) {
      return { key, title, ok: true, goodNote, badNote };
    }
    if (inst?.state === "installing") {
      return {
        key, title, ok: false, goodNote, badNote,
        installing: { hint: inst.lastLog ?? t("status.install.starting") },
        action: { label: t("status.install.busy"), onClick: () => {}, busy: true },
      };
    }
    if (inst?.state === "failed") {
      return {
        key, title, ok: false, goodNote, badNote,
        failed: { message: inst.errorMessage ?? badNote },
        action: { label: t("status.install.retry"), onClick: () => handleInstall(target) },
      };
    }
    return {
      key, title, ok: false, goodNote, badNote,
      action: { label: t("status.install.do_it"), onClick: () => handleInstall(target) },
    };
  };

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
        : { label: enabling ? t("status.row.cdp.enabling") : t("status.row.cdp.action"), onClick: handleEnableBrowser, busy: enabling },
    },
    buildCliRow(
      "ab", "agent-browser", caps.agent_browser,
      t("status.row.agentbrowser.title"),
      t("status.row.agentbrowser.good"),
      t("status.row.agentbrowser.bad"),
    ),
    buildCliRow(
      "off", "officecli", caps.officecli,
      t("status.row.officecli.title"),
      t("status.row.officecli.good"),
      t("status.row.officecli.bad"),
    ),
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

  const allGood = rows.filter(r => r.key !== "ab" && r.key !== "off").every(r => r.ok);

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
        {rows.map(r => {
          const dotColor = r.installing
            ? "#d9952b"
            : (r.ok ? "#3fa66a" : "#e8638c");
          return (
          <div key={r.key} style={{
            display: "flex", alignItems: "center", gap: 12,
            padding: "12px 14px", background: "#fff", border: "1px solid #ece6dd",
            borderRadius: 12,
          }}>
            <span style={{
              width: 12, height: 12, borderRadius: 999,
              background: dotColor, flexShrink: 0,
            }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600, fontSize: 14 }}>{r.title}</div>
              <div style={{
                fontSize: 12,
                color: r.failed ? "#b8302c" : "#8a8178",
                marginTop: 2,
                fontFamily: r.installing ? "ui-monospace, 'SF Mono', monospace" : undefined,
                whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
              }}>
                {r.installing
                  ? r.installing.hint
                  : r.failed
                    ? r.failed.message
                    : (r.ok ? r.goodNote : r.badNote)}
              </div>
            </div>
            {!r.ok && r.action && (
              <button
                onClick={r.action.onClick}
                disabled={r.action.busy}
                style={{
                  padding: "6px 12px", borderRadius: 8, border: "none",
                  background: r.action.busy ? "#d9c9b8" : "#e8638c",
                  color: "#fff", fontWeight: 600,
                  fontSize: 12, cursor: r.action.busy ? "wait" : "pointer", whiteSpace: "nowrap",
                }}
              >
                {r.action.label}
              </button>
            )}
          </div>
        );})}
      </div>

      <p style={{ marginTop: 20, fontSize: 12, color: "#8a8178", lineHeight: 1.6 }}>
        {t("status.tip.first_use")}
      </p>
    </div>
  );
}
