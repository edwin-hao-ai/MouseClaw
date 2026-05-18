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

  const handleCta = () => {
    if (payload.ctaAction === "summon") {
      // Same as PetMenu's summon: toggle recording (acts as if shortcut pressed)
      invoke("toggle_recording").catch(() => {});
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
    <div className={`nudge-bubble nudge-${payload.kind}`} role="alert" aria-live="polite">
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
