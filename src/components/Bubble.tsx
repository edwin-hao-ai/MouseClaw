/**
 * Speech bubble — per DESIGN.md §4.1.
 * Always paired with a PixelMouse below it (the pointer aims at the mouse).
 */
import "./Bubble.css";

export type BubbleVariant = "default" | "success" | "warn" | "danger";

interface BubbleProps {
  text: string;
  variant?: BubbleVariant;
  /** When true, the variants get a pink streaming cursor at end of text. */
  streaming?: boolean;
  /** Show "▼ 展开看完整回答" footer + onExpand handler. */
  expandable?: boolean;
  onExpand?: () => void;
  /** Session continuation chip at the top. */
  sessionChip?: { sessionId: number; turn: number };
  /** Voice bars (Listening state). */
  voiceBars?: boolean;
  /** Animated 3-dot loading indicator after the text (Thinking state). */
  loading?: boolean;
  /** 让正文区域可内部滚动 —— 长回复气泡用，不必跳 Panel 也能看完 (v0.1.8) */
  scrollable?: boolean;
}

export function Bubble({
  text, variant = "default", streaming = false,
  expandable = false, onExpand,
  sessionChip, voiceBars, loading = false, scrollable = false,
}: BubbleProps) {
  return (
    <div className={`bubble bubble-${variant}`} role="status" aria-live="polite">
      {sessionChip && (
        <div className="bubble-chip">
          🔗 续 Session #{sessionChip.sessionId} · 第 {sessionChip.turn} 轮
        </div>
      )}
      <div className={`bubble-body ${scrollable ? "bubble-scroll" : ""}`}>
        <span className="bubble-text">{text}</span>
        {voiceBars && (
          <span className="voicebars" aria-hidden>
            <span /><span /><span /><span />
          </span>
        )}
        {loading && (
          <span className="loading-dots" aria-label="思考中" role="status">
            <span /><span /><span />
          </span>
        )}
        {streaming && <span className="stream-cursor" aria-hidden>▮</span>}
      </div>
      {expandable && (
        <button
          type="button"
          className="bubble-expand"
          onClick={onExpand}
          aria-label="展开看完整回答"
        >
          ↗ 在 Panel 里打开（可继续追问）
          <kbd>↓</kbd>
        </button>
      )}
    </div>
  );
}
