/**
 * NudgeBubble (v0.1.27 P3) — proactive reminder bubble.
 *
 * Triggered by Rust `nudge` event (presence + nudge engine).
 * Renders above the pet with: message text, optional CTA button,
 * "今天闭嘴" mute action, auto-dismiss after 12s.
 *
 * Design source: docs/prototypes/pet-anchor-menu-nudges-20260518.html §3
 * All visual tokens from DESIGN.md (no hardcoded literals).
 */
import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { NudgePayload } from "../types";
import { useT } from "../i18n";
import "./NudgeBubble.css";

interface NudgeBubbleProps {
  payload: NudgePayload;
  onDismiss: () => void;
}

const AUTO_DISMISS_MS = 12_000;

export function NudgeBubble({ payload, onDismiss }: NudgeBubbleProps) {
  const t = useT();

  // Auto-dismiss after 12s — keeps the bubble unintrusive
  useEffect(() => {
    const id = window.setTimeout(onDismiss, AUTO_DISMISS_MS);
    return () => window.clearTimeout(id);
  }, [payload, onDismiss]);

  // v0.1.32 · 按 ESC 立即关闭
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDismiss();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onDismiss]);

  const handleCta = () => {
    if (payload.ctaAction === "summon") {
      // Same as PetMenu's summon: toggle recording (acts as if shortcut pressed)
      invoke("toggle_recording").catch(() => {});
    } else if (payload.ctaAction === "open-status") {
      // v0.4.x · 升级提示 → 打开系统状态窗，用户在那里一键装 browser/office CLI
      invoke("show_status_window").catch(() => {});
    } else if (payload.ctaAction === "open-memory") {
      // v0.4.4 · 记忆首次告知 → 打开「它记得的事」窗口
      invoke("open_memory_window").catch(() => {});
    }
    onDismiss();
  };

  const handleMute = () => {
    // mark this nudge kind as just-fired → backend cooldown will suppress
    // it for the next 30 min. (Full "today closed" lands in P3.1.)
    invoke("dismiss_nudge", { kind: payload.kind }).catch(() => {});
    onDismiss();
  };

  return (
    <div
      className={`nudge-bubble nudge-${payload.kind}`}
      role="alert"
      aria-live="polite"
      // v0.1.30 · 同 PetMenu 修复：截住 click 冒泡，否则会触发父 .stage-mouse
      // 的 handleMouseClick → 把 PetMenu 打开（不想要的副作用）
      onClick={(e) => e.stopPropagation()}
    >
      {/* v0.1.32 · ✕ 关闭按钮，右上角 —— 用户立刻想关就一键关 */}
      <button
        type="button"
        className="nudge-close"
        onClick={onDismiss}
        aria-label="Dismiss"
        title="ESC"
      >×</button>
      <div className="nudge-msg">{payload.message}</div>
      <div className="nudge-actions">
        {payload.ctaLabel && (
          <button
            type="button"
            className="nudge-btn nudge-btn-primary"
            onClick={handleCta}
          >
            {payload.ctaLabel}
          </button>
        )}
        <button
          type="button"
          className="nudge-btn nudge-btn-ghost"
          onClick={onDismiss}
        >
          {t("nudge.later")}
        </button>
      </div>
      <button
        type="button"
        className="nudge-mute"
        onClick={handleMute}
        title={t("nudge.mute_today")}
      >
        {t("nudge.mute_today")}
      </button>
    </div>
  );
}
