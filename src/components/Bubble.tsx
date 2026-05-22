/**
 * Speech bubble — per DESIGN.md §4.1.
 * Always paired with a PixelMouse below it (the pointer aims at the mouse).
 *
 * v0.1.9：用 marked 渲染 markdown（**bold**, `code`, 代码块, 列表, 链接…），
 * 否则 Claude 返回的 `**关键点**` 会被原样显示，体验很差。
 * 安全：marked 默认会逃逸 HTML，不会 XSS。
 */
import { useEffect, useMemo, useRef, useState } from "react";
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
  /** v0.3.6 · 气泡底部行动按钮（如"🔓 去授权"打开系统设置）—— 用于 blocked 状态给用户具体下一步 */
  action?: { label: string; onClick: () => void };
  /** v0.4.4 · 这次回复用到的记忆(🧠 可展开看 + 当场删错的)。 */
  memItems?: { kind: string; id: number; text: string }[];
  onDeleteMem?: (kind: string, id: number) => void;
}

export function Bubble({
  text, variant = "default", streaming = false,
  expandable = false, onExpand,
  sessionChip, voiceBars, loading = false, scrollable = false, markdown = false,
  onNewSession, action, memItems, onDeleteMem,
}: BubbleProps) {
  const t = useT();
  const [memOpen, setMemOpen] = useState(false);
  // markdown 解析结果 —— 流式期间每个 chunk 都重 parse 是 OK 的（marked 很快）
  const html = useMemo(
    () => (markdown ? renderMarkdown(text) : null),
    [text, markdown]
  );

  // v0.3.11 · 流式滚动策略（聊天标准行为）：
  //   - streaming=true 时默认跟随最新 token 滚到底；用户手动上滚则**暂停跟随**，
  //     把视图保持在用户当前位置（防止"我想看上面那段，结果它把我拽下来"）
  //   - 用户再滚回 ~bottom 时自动恢复跟随
  //   - streaming=false（流结束 / 静态长回复）保持当前位置，不强制顶 / 底，
  //     避免抢用户阅读焦点。新 mount 的静态长回复初始就在顶部（浏览器默认）。
  //   - v0.1.23 的"永远顶部"被用户反向反馈推翻：流式时看不到正在生成的内容更难受。
  const scrollRef = useRef<HTMLDivElement>(null);
  const followBottomRef = useRef(true); // 流式默认跟随
  const [showFadeTop, setShowFadeTop] = useState(false);
  const [showFadeBottom, setShowFadeBottom] = useState(false);

  // 流式内容更新时：若处于跟随模式，滚到底
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !scrollable) return;
    if (streaming && followBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
    setShowFadeTop(el.scrollTop > 4);
    setShowFadeBottom(el.scrollTop + el.clientHeight < el.scrollHeight - 4);
  }, [text, streaming, scrollable]);

  // 监听用户滚动 → 决定是否继续跟随
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !scrollable) return;
    const onScroll = () => {
      const nearBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 12;
      followBottomRef.current = nearBottom;
      setShowFadeTop(el.scrollTop > 4);
      setShowFadeBottom(!nearBottom);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => el.removeEventListener("scroll", onScroll);
  }, [scrollable]);

  const scrollToBottom = () => {
    const el = scrollRef.current;
    if (!el) return;
    followBottomRef.current = true;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  };
  const scrollToTop = () => {
    const el = scrollRef.current;
    if (!el) return;
    followBottomRef.current = false;
    el.scrollTo({ top: 0, behavior: "smooth" });
  };

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
      {/* v0.4.x · 滚动区包一层定位 wrapper —— fade / ▲▼ 按钮以「滚动视口真实顶部」为锚，
          而不是整个气泡顶部。否则有 session chip 时它们会浮到 chip 行上（▲ 压住「新对话」、
          fade 盖不住被裁的正文 → 漏出游离句号）。chip 在 wrapper 之外，互不干扰。 */}
      <div className="bubble-scrollwrap">
        {scrollable && showFadeTop && (
          <div className="bubble-fade-top" aria-hidden />
        )}
        {scrollable && showFadeTop && (
          <button
            type="button"
            className="bubble-scroll-top"
            onClick={scrollToTop}
            aria-label={t("bubble.scroll_to_top")}
            title={t("bubble.scroll_to_top")}
          >▲</button>
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
        {scrollable && showFadeBottom && (
          <div className="bubble-fade-bottom" aria-hidden />
        )}
        {scrollable && streaming && showFadeBottom && (
          <button
            type="button"
            className="bubble-scroll-bottom"
            onClick={scrollToBottom}
            aria-label="跟随最新"
            title="跟随最新"
          >▼</button>
        )}
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
      {action && (
        <button
          type="button"
          className="bubble-action"
          onClick={action.onClick}
        >
          {action.label}
        </button>
      )}
      {memItems && memItems.length > 0 && (
        <div className="bubble-mem">
          <button
            type="button"
            className="bubble-mem-toggle"
            onClick={() => setMemOpen((o) => !o)}
          >
            {t("memory.used_badge", { n: memItems.length })} {memOpen ? "▴" : "▾"}
          </button>
          {memOpen && (
            <div className="bubble-mem-list">
              {memItems.map((m) => (
                <div className="bubble-mem-item" key={`${m.kind}-${m.id}`}>
                  <span className="bubble-mem-text">{m.text}</span>
                  {onDeleteMem && (
                    <button
                      type="button"
                      className="bubble-mem-x"
                      title={t("memory.delete")}
                      onClick={() => onDeleteMem(m.kind, m.id)}
                    >✕</button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
