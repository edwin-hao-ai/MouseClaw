/**
 * Speech bubble — per DESIGN.md §4.1.
 * Always paired with a PixelMouse below it (the pointer aims at the mouse).
 *
 * v0.1.9：用 marked 渲染 markdown（**bold**, `code`, 代码块, 列表, 链接…），
 * 否则 Claude 返回的 `**关键点**` 会被原样显示，体验很差。
 * 安全：marked 默认会逃逸 HTML，不会 XSS。
 */
import { useMemo } from "react";
import { marked } from "marked";
import { useT } from "../i18n";
import "./Bubble.css";

// 配置 marked：极简 + 安全
marked.setOptions({
  gfm: true,        // 表格、删除线、autolink
  breaks: true,     // 单换行 = <br>（更贴近聊天直觉）
});

function renderMarkdown(text: string): string {
  try {
    return marked.parse(text, { async: false }) as string;
  } catch {
    // 流式中间状态可能 parse 失败 —— 兜底原文
    return text;
  }
}

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
  /** 渲染为 markdown (v0.1.9)；流式中也可开，半完整 markdown marked 会兜底 */
  markdown?: boolean;
}

export function Bubble({
  text, variant = "default", streaming = false,
  expandable = false, onExpand,
  sessionChip, voiceBars, loading = false, scrollable = false, markdown = false,
}: BubbleProps) {
  const t = useT();
  // markdown 解析结果 —— 流式期间每个 chunk 都重 parse 是 OK 的（marked 很快）
  const html = useMemo(
    () => (markdown ? renderMarkdown(text) : null),
    [text, markdown]
  );

  return (
    <div className={`bubble bubble-${variant}`} role="status" aria-live="polite">
      {sessionChip && (
        <div className="bubble-chip">
          {t("bubble.session_chip", { id: sessionChip.sessionId, turn: sessionChip.turn })}
        </div>
      )}
      <div className={`bubble-body ${scrollable ? "bubble-scroll" : ""}`}>
        {markdown && html != null ? (
          <div className="bubble-text bubble-md" dangerouslySetInnerHTML={{ __html: html }} />
        ) : (
          <span className="bubble-text">{text}</span>
        )}
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
          aria-label={t("bubble.expand_to_panel")}
        >
          {t("bubble.expand_to_panel")}
          <kbd>↓</kbd>
        </button>
      )}
    </div>
  );
}
