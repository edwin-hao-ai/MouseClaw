/**
 * Speech bubble — per DESIGN.md §4.1.
 * Always paired with a PixelMouse below it (the pointer aims at the mouse).
 *
 * v0.1.9：用 marked 渲染 markdown（**bold**, `code`, 代码块, 列表, 链接…），
 * 否则 Claude 返回的 `**关键点**` 会被原样显示，体验很差。
 * 安全：marked 默认会逃逸 HTML，不会 XSS。
 */
import { useEffect, useMemo, useRef } from "react";
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
  /** session chip 旁边显示一个「✨新对话」按钮 (v0.1.9 UX 改进) */
  onNewSession?: () => void;
}

export function Bubble({
  text, variant = "default", streaming = false,
  expandable = false, onExpand,
  sessionChip, voiceBars, loading = false, scrollable = false, markdown = false,
  onNewSession,
}: BubbleProps) {
  const t = useT();
  // markdown 解析结果 —— 流式期间每个 chunk 都重 parse 是 OK 的（marked 很快）
  const html = useMemo(
    () => (markdown ? renderMarkdown(text) : null),
    [text, markdown]
  );

  // v0.1.22 · 流式期间智能滚动：用户在底部 → 跟着新内容；用户往上翻 → 不打扰
  // 流完后强制滚到顶，让用户看到完整回答的开头（之前会停在最后的滚动位置，看起来像"上面缺一块"）
  const scrollRef = useRef<HTMLDivElement>(null);
  const userScrolledUpRef = useRef(false);
  const prevStreamingRef = useRef(streaming);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !scrollable) return;
    if (streaming) {
      // 流式中：用户没自己滚动 → 跟着到底；否则不动
      if (!userScrolledUpRef.current) {
        el.scrollTop = el.scrollHeight;
      }
    } else if (prevStreamingRef.current) {
      // 刚从 streaming → 终态：滚到顶让用户看见完整开头
      el.scrollTop = 0;
      userScrolledUpRef.current = false;
    }
    prevStreamingRef.current = streaming;
  }, [text, streaming, scrollable]);

  // 监听用户手动滚动
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !scrollable) return;
    const onScroll = () => {
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 8;
      userScrolledUpRef.current = !atBottom;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, [scrollable]);

  return (
    <div className={`bubble bubble-${variant}`} role="status" aria-live="polite">
      {sessionChip && (
        <div className="bubble-chip">
          <span>{t("bubble.session_chip", { id: sessionChip.sessionId, turn: sessionChip.turn })}</span>
          {onNewSession && (
            <button
              type="button"
              className="bubble-chip-newsession"
              onClick={onNewSession}
              title={t("bubble.new_session")}
              aria-label={t("bubble.new_session")}
            >
              ✨ {t("bubble.new_session")}
            </button>
          )}
        </div>
      )}
      <div ref={scrollRef} className={`bubble-body ${scrollable ? "bubble-scroll" : ""}`}>
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
          <span className="loading-dots" aria-label={t("bubble.thinking")} role="status">
            <span /><span /><span />
          </span>
        )}
        {streaming && <span className="stream-cursor" aria-hidden>▮</span>}
      </div>
      {loading && <span className="bubble-shimmer" aria-hidden />}
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
