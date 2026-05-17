/**
 * PanelView — 独立的「继续追问」窗口（v0.1.10）
 *
 * 之前 Panel 直接渲染在 320×320 透明 overlay 里被挤爆 —— 现在改成独立 720×560 窗口。
 * 流程：
 *   1. 用户点 Bubble 上的「💬 继续追问」
 *   2. App.tsx 调 invoke("open_panel_window", { sessionId, transcript, reply })
 *   3. Rust 把 ctx 塞进 AppState.pending_panel_context + 打开此窗口
 *   4. 这里 mount 时 invoke("take_panel_context") 拿到 ctx 渲染 Panel
 *   5. follow-up 提交 → invoke("follow_up") 走原 pipeline
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Panel, type Turn } from "./components/Panel";
import { useT } from "./i18n";

interface PanelContext {
  session_id: number;
  transcript: string;
  reply: string;
}

export default function PanelView() {
  const t = useT();
  const [ctx, setCtx] = useState<PanelContext | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);

  const loadContext = useCallback(async () => {
    try {
      const c = await invoke<PanelContext | null>("take_panel_context");
      if (c) {
        setCtx(c);
        setTurns([
          { role: "user", text: c.transcript },
          { role: "assistant", text: c.reply },
        ]);
      }
    } catch (e) {
      console.warn("take_panel_context failed:", e);
    }
  }, []);

  useEffect(() => {
    loadContext();
    // 已经打开的窗口被重新触发时也刷新一次
    let unlisten: (() => void) | null = null;
    try {
      const p = listen("panel-context-changed", () => loadContext());
      p.then(fn => { unlisten = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => { if (unlisten) unlisten(); };
  }, [loadContext]);

  const handleSend = useCallback(async (text: string) => {
    // 立即把用户输入加进 turns 给视觉反馈
    setTurns(prev => [...prev, { role: "user", text }, { role: "assistant", text: "", streaming: true }]);
    try {
      await invoke("follow_up", { text });
      // pipeline 的进度通过 view-changed 事件回来。这里也监听一下取最新 reply。
      // TODO: 后续接 streaming 事件流真的更新
    } catch (e) {
      console.warn("follow_up failed:", e);
    }
  }, []);

  const handleCollapse = useCallback(() => {
    // 关掉这个 panel 窗口（不重启 app）
    invoke("dismiss").catch(() => {});
    // 同时让窗口隐藏 —— 通过 Tauri API
    import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
      const w = getCurrentWebviewWindow();
      w.hide().catch(() => {});
    });
  }, []);

  const handleNewSession = useCallback(async () => {
    try { await invoke("new_session"); }
    catch (e) { console.warn("new_session failed:", e); }
    // 清空当前展示
    setTurns([]);
    setCtx(null);
  }, []);

  if (!ctx) {
    return (
      <div style={{
        display: "grid", placeItems: "center", height: "100vh",
        fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
        color: "#8a8178", fontSize: 14,
      }}>
        🦞 {t("common.loading")}
      </div>
    );
  }

  return (
    <div style={{
      width: "100vw", height: "100vh",
      background: "#faf7f2",
      fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
      overflow: "hidden",
    }}>
      <Panel
        sessionId={ctx.session_id}
        turns={turns}
        onSend={handleSend}
        onCollapse={handleCollapse}
        onNewSession={handleNewSession}
      />
    </div>
  );
}
