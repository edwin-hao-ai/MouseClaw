/**
 * Shared types between Rust backend and React frontend.
 * Match exactly with the `serde` types in src-tauri/src/events.rs.
 */

export type ViewKind =
  | { kind: "idle" }
  | { kind: "onboarding" }
  | { kind: "listening" }     // after shortcut; awaiting user text/voice
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

export interface SessionMeta {
  id: number;
  startedAt: string; // ISO 8601
  turns: Turn[];
}

/** Event names emitted from Rust via app.emit(). */
export const EV_VIEW_CHANGED = "view-changed";
export const EV_STREAM_CHUNK = "stream-chunk";
export const EV_COUNTDOWN    = "countdown-tick";
