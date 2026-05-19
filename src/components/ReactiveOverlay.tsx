/**
 * Reactive 桌宠头顶轻量动作 ribbon（v0.4+）
 *
 * 设计原则（用户反馈 2026-05-19）：
 *   - 就是几个轻飘飘的按钮浮在桌宠头顶，不要"先小提示再 hover 大面板"两层
 *   - 不能在透明 overlay 上画重 box-shadow（macOS 会渲染成黑边伪影）
 *   - 必须让 overlay 窗口先撑大（compact 80×80 → expanded 320×320），否则按钮会被裁掉
 *
 * 行为：
 *   - reactive payload 到 → 父组件 invoke set_overlay_has_ui(true) 撑窗口
 *   - 这里渲染 3 个 pill 按钮：📝 清理 / 🌐 翻 / 💡 解
 *   - 5s 自动消失（mouseenter 期间不消失，让人有时间瞄准）
 *   - 点按钮 → ribbon 原地切到 "处理中" → "✓ 已写回" → 2s 后消失
 */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./ReactiveOverlay.css";

export type HintIcon = "format" | "translate" | "code" | "url" | "generic";

export interface ReactivePayload {
  tier: "acknowledge" | "hint";
  icon?: HintIcon;
  clipId: number;
  preview: string;
  charLen: number;
  /** 客户端记录的接收时间戳（ms since epoch）—— 用来判 hint 是否还新鲜 */
  receivedAt: number;
}

const AUTO_DISMISS_MS = 5000;
const RESULT_DISPLAY_MS = 2000;

type Phase =
  | { kind: "idle" }
  | { kind: "busy" }
  | { kind: "done"; ok: boolean; msg: string };

export function ReactiveOverlay({
  payload, lang, onDismiss,
}: {
  payload: ReactivePayload | null;
  lang: "zh" | "en";
  onDismiss: () => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [hovered, setHovered] = useState(false);

  // 每次 payload 换新 → 重置 phase
  useEffect(() => { setPhase({ kind: "idle" }); }, [payload?.clipId]);

  // 自动消失定时器：idle 阶段 5s（hover 时暂停）；done 阶段 2s
  useEffect(() => {
    if (!payload) return;
    if (phase.kind === "busy") return;       // 处理中不消失
    if (phase.kind === "idle" && hovered) return;  // hover 暂停 idle 计时
    const delay = phase.kind === "done" ? RESULT_DISPLAY_MS : AUTO_DISMISS_MS;
    const id = window.setTimeout(onDismiss, delay);
    return () => window.clearTimeout(id);
  }, [payload?.clipId, phase, hovered, onDismiss]);

  // Esc 即时关闭
  useEffect(() => {
    if (!payload) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [payload, onDismiss]);

  if (!payload || payload.tier !== "hint") return null;

  const L = lang === "zh"
    ? { clean: "清理", translate: "翻译", explain: "解释", soon: "下个版本",
        busy: "处理中…", done: "✓ 已写回 · ⌘V 粘贴", fail: "出错了" }
    : { clean: "Clean", translate: "Translate", explain: "Explain", soon: "Soon",
        busy: "Working…", done: "✓ Copied · ⌘V", fail: "Failed" };

  async function run(action: "clean" | "translate" | "explain") {
    if (!payload) return;
    setPhase({ kind: "busy" });
    try {
      await invoke<string>("process_clipboard_action", {
        clipId: payload.clipId,
        action,
      });
      setPhase({ kind: "done", ok: true, msg: L.done });
    } catch (e: unknown) {
      setPhase({ kind: "done", ok: false, msg: `${L.fail}: ${String(e)}` });
    }
  }

  // ribbon 在 stage-mouse 内 → click 会冒泡到 stage-mouse onClick 触发 PetMenu。
  // 所有 ribbon 上的事件都吃掉，不让冒泡。pointerdown 也要吃，因为 stage-mouse 有拖拽 handler。
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();

  return (
    <div
      className="rx-ribbon"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={stop}
      onPointerDown={stop}
      onPointerUp={stop}
      onPointerMove={stop}
      role="toolbar"
    >
      {phase.kind === "idle" && (
        <>
          <button className="rx-pill rx-pill-primary" onClick={() => run("clean")} title={L.clean}>
            <span className="rx-pill-glyph">📝</span>
            <span className="rx-pill-label">{L.clean}</span>
          </button>
          <button className="rx-pill" onClick={() => run("translate")} title={L.translate}>
            <span className="rx-pill-glyph">🌐</span>
            <span className="rx-pill-label">{L.translate}</span>
          </button>
          <button className="rx-pill" onClick={() => run("explain")} title={L.explain}>
            <span className="rx-pill-glyph">💡</span>
            <span className="rx-pill-label">{L.explain}</span>
          </button>
        </>
      )}
      {phase.kind === "busy" && (
        <div className="rx-status rx-status-busy">
          <span className="rx-dot" /><span className="rx-dot" /><span className="rx-dot" />
          <span className="rx-status-text">{L.busy}</span>
        </div>
      )}
      {phase.kind === "done" && (
        <div className={`rx-status ${phase.ok ? "rx-status-ok" : "rx-status-err"}`}>
          <span className="rx-status-text">{phase.msg}</span>
        </div>
      )}
    </div>
  );
}
