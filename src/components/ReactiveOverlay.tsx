/**
 * Reactive 桌宠头顶轻量动作 ribbon（v0.4+）
 *
 * 设计原型：docs/prototypes/reactive-pet-20260519.html
 * 用户反馈（2026-05-19）：就是几个轻飘飘的按钮浮在桌宠头顶，无两层 UI。
 *
 * 触发来源：
 *   - source="clipboard" —— 用户复制了内容（NSPasteboard 变化）
 *   - source="selection" —— 用户在某个 app 里选中了文字（AXSelectedText 变化）
 *
 * 两条 source 共用同一管道（Rust 端 reactive.rs），同一事件 channel
 *   `clipboard-reactive`，前端用 payload.source 区分。
 *
 * 行为：
 *   - reactive payload 到 → 5 个 pill 按钮（清理 / 翻译 / 解释 / 回信）
 *   - 4s 自动消失（hover 期间暂停）
 *   - 点 → 处理中 → ✓ 已写回 → 2s 后消失
 *   - Esc 即时关
 */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./ReactiveOverlay.css";

export type HintIcon = "format" | "translate" | "code" | "url" | "generic";
export type ReactiveSource = "clipboard" | "selection";

export interface ReactivePayload {
  source: ReactiveSource;
  tier: "acknowledge" | "hint";
  icon?: HintIcon;
  preview: string;
  charLen: number;
  /** 客户端记录的接收时间戳（ms since epoch）—— 用来判 hint 是否还新鲜 */
  receivedAt: number;
}

export type ReactiveAction = "clean" | "translate" | "explain" | "reply";

const AUTO_DISMISS_MS = 4000;
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
  // 注意：payload.receivedAt 比 source/preview 更适合做 key（同一条剪贴板内容
  // 可能因为 cooldown 过期而再次触发，receivedAt 会变）
  useEffect(() => { setPhase({ kind: "idle" }); }, [payload?.receivedAt]);

  // 自动消失：idle 4s（hover 暂停），done 2s
  useEffect(() => {
    if (!payload) return;
    if (phase.kind === "busy") return;
    if (phase.kind === "idle" && hovered) return;
    const delay = phase.kind === "done" ? RESULT_DISPLAY_MS : AUTO_DISMISS_MS;
    const id = window.setTimeout(onDismiss, delay);
    return () => window.clearTimeout(id);
  }, [payload?.receivedAt, phase, hovered, onDismiss]);

  // Esc 即时关
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
    ? { clean: "清理", translate: "翻译", explain: "解释", reply: "回信",
        busy: "处理中…", done: "✓ 已写回 · ⌘V 粘贴", fail: "出错了",
        srcClip: "📋", srcSel: "🔤" }
    : { clean: "Clean", translate: "Translate", explain: "Explain", reply: "Reply",
        busy: "Working…", done: "✓ Copied · ⌘V", fail: "Failed",
        srcClip: "📋", srcSel: "🔤" };

  async function run(action: ReactiveAction) {
    if (!payload) return;
    setPhase({ kind: "busy" });
    try {
      await invoke<string>("process_reactive_action", { action });
      setPhase({ kind: "done", ok: true, msg: L.done });
    } catch (e: unknown) {
      setPhase({ kind: "done", ok: false, msg: `${L.fail}: ${String(e)}` });
    }
  }

  // ribbon 在 stage-mouse 内 → click 会冒泡触发 PetMenu。所有事件吃掉不冒泡。
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();

  const sourceGlyph = payload.source === "selection" ? L.srcSel : L.srcClip;

  return (
    <div
      className="rx-ribbon"
      data-testid="rx-ribbon"
      data-source={payload.source}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={stop}
      onPointerDown={stop}
      onPointerUp={stop}
      onPointerMove={stop}
      role="toolbar"
      aria-label={`Reactive ribbon (${payload.source})`}
    >
      {phase.kind === "idle" && (
        <>
          <span className="rx-source-tag" title={payload.source} aria-hidden>{sourceGlyph}</span>
          <button
            className="rx-pill rx-pill-primary"
            data-testid="rx-action-clean"
            onClick={() => run("clean")}
            title={L.clean}
          >
            <span className="rx-pill-glyph">📝</span>
            <span className="rx-pill-label">{L.clean}</span>
          </button>
          <button
            className="rx-pill"
            data-testid="rx-action-translate"
            onClick={() => run("translate")}
            title={L.translate}
          >
            <span className="rx-pill-glyph">🌐</span>
            <span className="rx-pill-label">{L.translate}</span>
          </button>
          <button
            className="rx-pill"
            data-testid="rx-action-explain"
            onClick={() => run("explain")}
            title={L.explain}
          >
            <span className="rx-pill-glyph">💡</span>
            <span className="rx-pill-label">{L.explain}</span>
          </button>
          <button
            className="rx-pill"
            data-testid="rx-action-reply"
            onClick={() => run("reply")}
            title={L.reply}
          >
            <span className="rx-pill-glyph">✉️</span>
            <span className="rx-pill-label">{L.reply}</span>
          </button>
        </>
      )}
      {phase.kind === "busy" && (
        <div className="rx-status rx-status-busy" data-testid="rx-status-busy">
          <span className="rx-dot" /><span className="rx-dot" /><span className="rx-dot" />
          <span className="rx-status-text">{L.busy}</span>
        </div>
      )}
      {phase.kind === "done" && (
        <div
          className={`rx-status ${phase.ok ? "rx-status-ok" : "rx-status-err"}`}
          data-testid={phase.ok ? "rx-status-done" : "rx-status-err"}
        >
          <span className="rx-status-text">{phase.msg}</span>
        </div>
      )}
    </div>
  );
}
