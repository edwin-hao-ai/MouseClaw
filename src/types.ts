/**
 * Shared types between Rust backend and React frontend.
 * Match exactly with the `serde` types in src-tauri/src/events.rs.
 */

export type ViewKind =
  | { kind: "idle" }
  | { kind: "onboarding" }
  | { kind: "listening"; partial?: string }     // after shortcut; awaiting user text/voice (v0.2: streaming partial)
  | { kind: "voice-ime-listening"; partial?: string }  // v0.1.14: hold-trigger voice IME 中 (v0.3.8: streaming partial in bubble)
  | { kind: "feed-waiting" }         // v0.4: 文件 hover 在桌宠上、还没 drop
  | { kind: "feed-listening"; files: string[]; partial?: string }  // v0.4: 已吞下文件、听用户问问题
  | { kind: "thinking"; transcript: string; status?: string }
  | { kind: "reply"; transcript: string; reply: string; mode: "A" | "B"; insertText?: string; streaming?: boolean }
  | { kind: "panel"; sessionId: number; turns: Turn[] }
  | { kind: "mode-b-countdown"; insertText: string; remaining: number }
  | { kind: "mode-b-inserting"; insertText: string }
  // v0.4.0 · 语音转写完成后的 3 秒确认窗口（A 方案）
  // Esc 取消 / Enter 立即发 / 点气泡编辑 / 倒数完自动发
  | { kind: "voice-confirm"; transcript: string; remaining: number }
  // v0.4.0 · 首次使用引导 · 5 步流程
  | { kind: "tour-step"; step: number }
  | { kind: "blocked"; reason: string };

export interface Turn {
  role: "user" | "assistant";
  text: string;
  streaming?: boolean;
}

export type ShortcutChoice =
  | "double-option" | "hold-option"
  | "double-cmd"    | "hold-cmd";

/** AI backend choice — matches Rust `Backend::from_choice`. */
export type BackendChoice = "claude-cli" | "codex-cli" | "openclaw-cli" | "hermes-agent";

/** 桌宠悬停位置 (v0.1.27, +hidden in v0.1.28) — must match Rust `PetAnchor::from_str`. */
export type PetAnchor =
  | "top-left" | "top-right" | "bottom-left" | "bottom-right"
  | "follow" | "hidden";

/** Re-export SkinId here for convenience (single source of truth lives in src/skins.ts). */
export type { SkinId } from "./skins";

export interface SessionMeta {
  id: number;
  startedAt: string; // ISO 8601
  turns: Turn[];
}

/** Event names emitted from Rust via app.emit(). */
export const EV_VIEW_CHANGED = "view-changed";
export const EV_SKIN_CHANGED = "skin-changed";
export const EV_STREAM_CHUNK = "stream-chunk";
export const EV_COUNTDOWN    = "countdown-tick";
export const EV_NUDGE        = "nudge";
export const EV_SESSION_STATE = "session-state";

/** Session 状态（v0.4.x）—— 驱动链条图标 / 第 N 轮 / 钉住 / 软提示。 */
export interface SessionState {
  continuing: boolean;
  round: number;
  pinned: boolean;
  pinnedLabel?: string;
  softHint: boolean;
}

/** Proactive reminder kind (v0.1.27 P3) — matches Rust `NudgeKind`. */
export type NudgeKind =
  | "stretch" | "stuck" | "late-night"
  | "water" | "good-morning" | "lunch"  // v0.1.32
  | "learned-cli"                        // v0.4.x · 老用户升级提示
  | "long-focus"                         // v0.4+ · 连续专注 2h 提醒休息
  | "memory-intro";                      // v0.4.4 · 记忆首次透明告知

export interface NudgePayload {
  kind: NudgeKind;
  message: string;
  ctaLabel?: string;
  ctaAction?: string;
}
