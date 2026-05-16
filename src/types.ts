/**
 * Shared types between Rust backend and React frontend.
 * Match exactly with the `serde` types in src-tauri/src/events.rs.
 */

export type ViewKind =
  | { kind: "idle" }
  | { kind: "onboarding" }
  | { kind: "listening" }     // after shortcut; awaiting user text/voice
  | { kind: "voice-ime-listening" }  // v0.1.14: hold-trigger voice IME 中
  | { kind: "thinking"; transcript: string }
  | { kind: "reply"; transcript: string; reply: string; mode: "A" | "B"; insertText?: string; streaming?: boolean }
  | { kind: "panel"; sessionId: number; turns: Turn[] }
  | { kind: "mode-b-countdown"; insertText: string; remaining: number }
  | { kind: "mode-b-inserting"; insertText: string }
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
export type BackendChoice = "claude-cli" | "codex-cli" | "openclaw-cli";

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
