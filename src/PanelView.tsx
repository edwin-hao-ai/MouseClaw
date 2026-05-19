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
import { EV_VIEW_CHANGED, type ViewKind } from "./types";
import { useT } from "./i18n";

interface PanelContext {
  session_id: number;
  transcript: string;
  reply: string;
  /** v0.3.11 · 从 HistoryView 恢复时后端塞进来的完整历史 turns；
   *  bubble 的「继续追问」入口不填，此时退化到 [transcript, reply] 一对。 */
  turns?: Turn[];
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
        // v0.3.11 · 优先用后端塞进来的完整 turns（resume_session 路径），
        // 否则退化到 transcript+reply 一对（bubble 「继续追问」路径）。
        if (c.turns && c.turns.length > 0) {
          setTurns(c.turns);
        } else {
          setTurns([
            { role: "user", text: c.transcript },
            { role: "assistant", text: c.reply },
          ]);
        }
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

  // v0.3.12 · 监听 pipeline 流式回复 —— 之前 Panel 完全没接 view-changed 事件，
  // follow_up 发出去后 streaming reply 不更新 UI，用户要关闭重开窗口才能看到结果。
  // 现在每收到一次 Reply，就把最后一个 assistant turn 的 text 替换成最新 accumulated reply。
  // streaming=true 期间保留 streaming 标记（气泡上的闪烁光标），streaming=false 时去掉。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      const p = listen<ViewKind>(EV_VIEW_CHANGED, (e) => {
        const v = e.payload;
        if (v.kind !== "reply") return;
        setTurns(prev => {
          // 找最后一个 assistant turn 替换内容；如果没有就追加一个
          const lastIdx = (() => {
            for (let i = prev.length - 1; i >= 0; i--) {
              if (prev[i].role === "assistant") return i;
            }
            return -1;
          })();
          if (lastIdx === -1) {
            return [...prev, { role: "assistant", text: v.reply, streaming: v.streaming }];
          }
          const next = prev.slice();
          next[lastIdx] = {
            role: "assistant",
            text: v.reply,
            streaming: v.streaming ?? false,
          };
          return next;
        });
      });
      p.then(fn => { unlisten = fn; }).catch(() => {});
    } catch {/* browser-only dev */}
    return () => { if (unlisten) unlisten(); };
  }, []);

  const handleSend = useCallback(async (text: string) => {
    // 立即把用户输入加进 turns 给视觉反馈（再加一个 streaming 占位 assistant turn）
    setTurns(prev => [...prev, { role: "user", text }, { role: "assistant", text: "", streaming: true }]);
    try {
      await invoke("follow_up", { text });
      // 之后由上面 view-changed listener 持续更新最后一个 assistant turn
    } catch (e) {
      console.warn("follow_up failed:", e);
      // 失败也要把 streaming flag 关掉，否则气泡一直闪烁
      setTurns(prev => {
        const next = prev.slice();
        if (next.length > 0) {
          const last = next[next.length - 1];
          if (last.role === "assistant") {
            next[next.length - 1] = { ...last, text: `⛔ 发送失败：${String(e)}`, streaming: false };
          }
        }
        return next;
      });
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
